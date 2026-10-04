//! The wallet library on this device (phase 1, step 4). Unix only: Windows
//! is a desktop platform phase 1 does not need these numbers from.
//!
//! Runs on a worker thread, never the drawing thread: it derives Argon2id
//! keys at 64 MiB and makes a network call. Nothing here signs. Every store
//! uses public test vectors only: all-zero entropy is the BIP39 phrase
//! "abandon ... art", which must never be funded.

use std::fs;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mochimo_crypto::account::{Account, AccountKind, WotsIndex};
use mochimo_crypto::addr::Tag;
use mochimo_crypto::consts::SEED_LEN;
use mochimo_crypto::keystore::{Init, Kdf, Keystore, NONCE_SEED_LEN, SALT_LEN, Unlock};
use mochimo_crypto::mesh::hex;
use mochimo_crypto::{Error, Secret, cli, mnemonic};

use crate::report;

/// At least 12 characters: `cli::create::create` refuses fewer.
const PASSWORD: &str = "spike-password-not-for-real-use";
const ENTROPY_A: [u8; 32] = [0x00; 32];
const ENTROPY_B: [u8; 32] = [0x01; 32];
const SALT_A: [u8; SALT_LEN] = [0xA5; SALT_LEN];
const SALT_B: [u8; SALT_LEN] = [0xB5; SALT_LEN];
const NONCE_A: [u8; NONCE_SEED_LEN] = [0x0A; NONCE_SEED_LEN];
const NONCE_B: [u8; NONCE_SEED_LEN] = [0x0B; NONCE_SEED_LEN];

/// The node the owner named for the spike's TLS check (2026-10-03).
pub const NODE: &str = "https://api.mochimo.org";

fn check(name: &str, ok: bool, detail: impl std::fmt::Display) {
    report::check(&format!("lib.{name}"), ok, detail);
}

fn note(line: impl AsRef<str>) {
    report::note(format!("lib: {}", line.as_ref()));
}

trait Ctx<T> {
    fn ctx(self, what: &str) -> Result<T, String>;
}
impl<T, E: std::fmt::Debug> Ctx<T> for Result<T, E> {
    fn ctx(self, what: &str) -> Result<T, String> {
        self.map_err(|e| format!("{what}: {e:?}"))
    }
}

/// Fresh entropy for every `Unlock`, as the CLI draws it.
fn fresh<const N: usize>() -> Result<[u8; N], String> {
    let mut b = [0u8; N];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .ctx("/dev/urandom")?;
    Ok(b)
}

fn unlock(nonce_seed: [u8; NONCE_SEED_LEN]) -> Unlock<'static> {
    Unlock {
        password: PASSWORD.as_bytes(),
        nonce_seed,
    }
}

fn euid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

pub fn stat(p: &Path) -> String {
    match fs::symlink_metadata(p) {
        Ok(m) => format!(
            "mode={:o} uid={} gid={} {}",
            m.mode() & 0o7777,
            m.uid(),
            m.gid(),
            if m.file_type().is_symlink() {
                "link"
            } else if m.is_dir() {
                "dir"
            } else {
                "file"
            }
        ),
        Err(e) => format!("stat failed: {:?}", e.kind()),
    }
}

fn mode_uid(p: &Path) -> Option<(u32, u32)> {
    fs::symlink_metadata(p)
        .ok()
        .map(|m| (m.mode() & 0o7777, m.uid()))
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

// ---- memory ---------------------------------------------------------------

/// getrusage's high-water mark in KiB (Linux and Android report KiB, Apple
/// platforms bytes).
fn max_rss_kib() -> Option<u64> {
    // SAFETY: an all-zero rusage is a valid value; getrusage writes it.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: the pointer is to a live rusage.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) } != 0 {
        return None;
    }
    let v = u64::try_from(ru.ru_maxrss).ok()?;
    Some(if cfg!(target_vendor = "apple") {
        v / 1024
    } else {
        v
    })
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn proc_status_kib(field: &str) -> Option<u64> {
    let s = fs::read_to_string("/proc/self/status").ok()?;
    s.lines().find_map(|l| {
        l.strip_prefix(field)?
            .strip_prefix(':')?
            .trim()
            .strip_suffix("kB")?
            .trim()
            .parse()
            .ok()
    })
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn current_rss_kib() -> Option<u64> {
    proc_status_kib("VmRSS")
}

#[cfg(target_vendor = "apple")]
fn current_rss_kib() -> Option<u64> {
    // libc declares mach_task_self_ for macOS only; the symbol is libSystem's.
    unsafe extern "C" {
        static mach_task_self_: libc::mach_port_t;
    }
    // SAFETY: all-zero is a valid mach_task_basic_info; task_info fills it.
    let mut info: libc::mach_task_basic_info = unsafe { std::mem::zeroed() };
    let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
    // SAFETY: the pointers are to live values of the types task_info expects.
    let kr = unsafe {
        libc::task_info(
            mach_task_self_,
            libc::MACH_TASK_BASIC_INFO,
            (&raw mut info).cast(),
            &mut count,
        )
    };
    (kr == libc::KERN_SUCCESS).then(|| info.resident_size / 1024)
}

#[cfg(not(any(target_os = "android", target_os = "linux", target_vendor = "apple")))]
fn current_rss_kib() -> Option<u64> {
    None
}

/// Runs `f` while a second thread samples the resident size every
/// millisecond. Returns (result, wall time, RSS before, highest RSS sampled).
fn measure<T>(f: impl FnOnce() -> T) -> (T, Duration, Option<u64>, Option<u64>) {
    let stop = Arc::new(AtomicBool::new(false));
    let peak = Arc::new(AtomicU64::new(0));
    let sampler = {
        let (stop, peak) = (stop.clone(), peak.clone());
        std::thread::spawn(move || {
            loop {
                if let Some(k) = current_rss_kib() {
                    peak.fetch_max(k, Ordering::Relaxed);
                }
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        })
    };
    std::thread::sleep(Duration::from_millis(20));
    let before = current_rss_kib();
    let t = Instant::now();
    let out = f();
    let elapsed = t.elapsed();
    stop.store(true, Ordering::Relaxed);
    let _ = sampler.join();
    let p = peak.load(Ordering::Relaxed);
    (out, elapsed, before, (p > 0).then_some(p))
}

// ---- the checks -----------------------------------------------------------

/// `dir` is an existing app-private directory: on Android the `no_backup/`
/// sibling of `files/`, on iOS `Library/Application Support`, on the desktop
/// the directory CI names. `probes` are platform directories to test without
/// writing (the library refuses a group-writable one).
pub fn run(dir: &Path, probes: &[PathBuf], network: bool) {
    note(format!(
        "os={} arch={} euid={} dir={} [{}]",
        std::env::consts::OS,
        std::env::consts::ARCH,
        euid(),
        dir.display(),
        stat(dir)
    ));
    // `open` checks owner and mode, then reports Missing before it makes the
    // lock file, so this writes nothing.
    for probe in probes {
        match Keystore::open(probe, &unlock([0; NONCE_SEED_LEN])) {
            Err(Error::UnsafePermissions { mode }) => note(format!(
                "probe {}: refused as a store directory, mode {mode:o} [{}]",
                probe.display(),
                stat(probe)
            )),
            Err(Error::Missing) => note(format!(
                "probe {}: passes the owner and mode checks [{}]",
                probe.display(),
                stat(probe)
            )),
            other => note(format!(
                "probe {}: {other:?} [{}]",
                probe.display(),
                stat(probe)
            )),
        }
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let run = dir.join("tawara-spike").join(format!("run-{nanos}"));
    if let Err(e) = fs::create_dir_all(&run) {
        return check(
            "run_directory",
            false,
            format!("{}: {:?}", run.display(), e.kind()),
        );
    }
    let store_a = run.join("store-a");
    match create_store_a(&store_a) {
        Ok(tag0) => {
            if let Err(e) = reopen_and_commit(&store_a, tag0) {
                check("reopen_and_commit", false, e);
            }
        }
        Err(e) => check("store_a", false, e),
    }
    if let Err(e) = refusals(&run, &store_a) {
        check("refusals", false, e);
    }
    if let Err(e) = recommended_unlock(&run) {
        check("unlock_recommended", false, e);
    }
    if network {
        tls();
    } else {
        note("tls: skipped (offline)");
    }
    // The run directory stays: CI lists its modes (Android checklist A19).
}

/// Store A at CHEAP_FOR_TESTS: three durable commits, no signature.
fn create_store_a(store_a: &Path) -> Result<Tag, String> {
    let phrase = mnemonic::phrase_from_entropy(&ENTROPY_A).ctx("phrase_from_entropy")?;
    let master: Secret<SEED_LEN> =
        mnemonic::master_seed_from_phrase(phrase.expose(), "").ctx("master seed")?;
    let init = Init {
        password: PASSWORD.as_bytes(),
        salt: SALT_A,
        nonce_seed: NONCE_A,
        kdf: Kdf::CHEAP_FOR_TESTS,
    };
    let t = Instant::now();
    let mut ks = Keystore::create(store_a, &init).ctx("Keystore::create")?; // generation 0
    let _durable = ks.adopt_master(&master).ctx("adopt_master")?; // generation 1
    let account = Account::derive(&master, 0);
    let tag0 = account.tag();
    ks.add(account).ctx("add account 0")?; // generation 2
    let g = ks.generation().ctx("generation")?;
    check(
        "three_commits",
        g == 2,
        format!("generation {g} in {:.1} ms", ms(t.elapsed())),
    );

    let me = euid();
    check(
        "store_dir_0700",
        mode_uid(store_a) == Some((0o700, me)),
        stat(store_a),
    );
    for name in ["accounts.mks", "keystore.lock"] {
        let p = store_a.join(name);
        check(
            &format!("{name}_0600"),
            mode_uid(&p) == Some((0o600, me)),
            stat(&p),
        );
    }
    check(
        "no_temp_left",
        !store_a.join("accounts.mks.tmp").exists(),
        "accounts.mks.tmp absent",
    );

    // The lock: a second handle while `ks` lives.
    let t = Instant::now();
    let second = Keystore::open(store_a, &unlock(fresh()?));
    check(
        "second_open_locked",
        matches!(second, Err(Error::Locked)),
        format!("{second:?} in {:.1} ms", ms(t.elapsed())),
    );
    let again = Keystore::create(store_a, &init);
    check(
        "second_create_exists",
        matches!(again, Err(Error::Exists { what: "snapshot" })),
        format!("{again:?}"),
    );
    drop(ks);
    Ok(tag0)
}

/// Reopen, check what the commits left, commit once more through the
/// opened handle, and reopen again.
fn reopen_and_commit(store_a: &Path, tag0: Tag) -> Result<(), String> {
    let mut ks = Keystore::open(store_a, &unlock(fresh()?)).ctx("reopen")?;
    let g = ks.generation().ctx("generation")?;
    let tags = ks.tags().ctx("tags")?;
    check(
        "reopen",
        g == 2 && tags == vec![tag0],
        format!("generation {g}, {} tag(s)", tags.len()),
    );
    let view = ks
        .view(&tag0)
        .ctx("view")?
        .ok_or_else(|| "account 0 missing".to_string())?;
    check(
        "account_0",
        view.kind == AccountKind::Derived
            && view.wots_index == WotsIndex::ZERO
            && view.pending.is_none(),
        format!("{view:?}"),
    );
    let master = ks
        .master()
        .ctx("master")?
        .map(Secret::duplicate)
        .ok_or_else(|| "no master".to_string())?;
    ks.add(Account::derive(&master, 1)).ctx("add account 1")?; // generation 3
    drop(master);
    drop(ks);
    let ks = Keystore::open(store_a, &unlock(fresh()?)).ctx("second reopen")?;
    let (g, n) = (
        ks.generation().ctx("generation")?,
        ks.tags().ctx("tags")?.len(),
    );
    check(
        "commit_after_reopen",
        g == 3 && n == 2,
        format!("generation {g}, {n} tags"),
    );
    Ok(())
}

fn refusals(run: &Path, store_a: &Path) -> Result<(), String> {
    let init = Init {
        password: PASSWORD.as_bytes(),
        salt: SALT_A,
        nonce_seed: NONCE_A,
        kdf: Kdf::CHEAP_FOR_TESTS,
    };
    let gw = run.join("group-writable");
    fs::create_dir(&gw).ctx("mkdir")?;
    fs::set_permissions(&gw, fs::Permissions::from_mode(0o770)).ctx("chmod")?;
    let res = Keystore::create(&gw, &init);
    check(
        "group_writable_refused",
        matches!(res, Err(Error::UnsafePermissions { mode: 0o770 })),
        format!("{res:?}"),
    );
    let empty = fs::read_dir(&gw).map(|d| d.count() == 0).unwrap_or(false);
    check("group_writable_untouched", empty, stat(&gw));

    let link = run.join("link-to-store-a");
    match std::os::unix::fs::symlink(store_a, &link) {
        Ok(()) => {
            let res = Keystore::open(&link, &unlock(fresh()?));
            check(
                "symlink_refused",
                matches!(res, Err(Error::StoreDirectoryIsLink)),
                format!("{res:?}"),
            );
        }
        Err(e) => note(format!("symlink not creatable here: {:?}", e.kind())),
    }

    // Owned by root: Io { op: "directory owner", .. }, or a stat error where
    // a sandbox refuses it.
    let res = Keystore::open(Path::new("/"), &unlock(fresh()?));
    note(format!("foreign owner (/): {res:?}"));

    let res = Keystore::open(&run.join("absent"), &unlock(fresh()?));
    check(
        "absent_dir",
        matches!(
            res,
            Err(Error::Io {
                op: "stat directory",
                kind: std::io::ErrorKind::NotFound
            })
        ),
        format!("{res:?}"),
    );
    Ok(())
}

/// One unlock at Kdf::RECOMMENDED (64 MiB, t=3, p=1): wall time and memory.
fn recommended_unlock(run: &Path) -> Result<(), String> {
    let store_b = run.join("store-b");
    let phrase = mnemonic::phrase_from_entropy(&ENTROPY_B).ctx("phrase B")?;
    let t = Instant::now();
    // cli::create::create always uses Kdf::RECOMMENDED.
    let created = cli::create::create(&store_b, phrase.expose(), PASSWORD, SALT_B, NONCE_B)
        .ctx("create B")?;
    note(format!(
        "create at RECOMMENDED: {:.1} ms, tag 0x{}",
        ms(t.elapsed()),
        hex::encode(&created.tag)
    ));

    let nonce = fresh()?;
    let maxrss_before = max_rss_kib();
    let (res, elapsed, before, peak) = measure(|| Keystore::open(&store_b, &unlock(nonce)));
    let maxrss_after = max_rss_kib();
    let ks = res.ctx("open B")?;
    let delta = match (before, peak) {
        (Some(b), Some(p)) => Some(p.saturating_sub(b)),
        _ => None,
    };
    check(
        "unlock_recommended",
        ks.generation().ctx("generation")? == 2,
        format!(
            "{:.1} ms; RSS before {before:?} KiB, sampled peak {peak:?} KiB, delta {delta:?} KiB; \
             ru_maxrss {maxrss_before:?} -> {maxrss_after:?} KiB",
            ms(elapsed)
        ),
    );
    Ok(())
}

#[cfg(feature = "tls")]
fn tls() {
    use mochimo_crypto::mesh::MeshClient;
    use mochimo_crypto::mesh::http::UreqTransport;
    let transport = match UreqTransport::new(NODE) {
        Ok(t) => t,
        Err(e) => return check("tls.network_status", false, format!("transport: {e:?}")),
    };
    let client = MeshClient::new(transport);
    let t = Instant::now();
    match client.network_status() {
        Ok(tip) => check(
            "tls.network_status",
            true,
            format!(
                "{NODE}: block {} hash 0x{} in {:.0} ms",
                tip.index,
                hex::encode(&tip.hash),
                ms(t.elapsed())
            ),
        ),
        Err(e) => check(
            "tls.network_status",
            false,
            format!("{NODE}: {e:?} ({e}) after {:.0} ms", ms(t.elapsed())),
        ),
    }
}

#[cfg(not(feature = "tls"))]
fn tls() {
    note("tls: built without the tls feature");
}
