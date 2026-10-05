//! The worker: one thread that owns the store and the wallet, takes
//! [`Command`]s and sends [`Event`]s (docs/PLAN.md section 3).
//!
//! # Sessions
//!
//! The worker is in one of three states:
//!
//! - **Locked.** No store is open. Nothing secret is held, except a phrase
//!   waiting for its confirmation while a store is being created.
//! - **Store.** A `Keystore` is open: its master seed is in memory and its
//!   lock is held, but the wallet did not open, so nothing can be spent. The
//!   store's accounts and destinations can be shown, and the pre-gate
//!   operations run here (status, the acknowledged advance, restore,
//!   discovery). This is where a store waits when no node is chosen, when
//!   the library refused to open the wallet (the node did not answer, none
//!   of the accounts is on the ledger yet, or every account diverged), and
//!   when a cancel stopped the wallet opening.
//! - **Wallet.** `Wallet::open` reconciled every account: the ones the node
//!   confirmed can spend, the ones it could not explain are set aside and
//!   refused by name (the library's I4).
//!
//! # Opening the wallet
//!
//! The worker keeps the password only for the length of the command that
//! brought it, so a store must never be closed by a refusal it could
//! recover from. It opens the wallet with the library's
//! `Wallet::open_or_return_with_progress`: the same reconciliation as
//! `Wallet::open`, but a refusal or a cancel hands the store and the client
//! back, still open and still locked, and the store stays open in the Store
//! state. Its notice is the library's "WALLET WILL NOT START" page, except
//! for two refusals that are not a key index out of step and are not shown
//! as one: a node that did not answer, and a store none of whose accounts
//! the node holds (a store just created and not yet paid), which gets the
//! library's "THIS STORE IS NOT WHOLE" notice. Each is read from the
//! refusal itself; nothing asks the node twice.
//!
//! # Lifecycle (docs/PLAN.md section 4.1)
//!
//! Locking drops the session: the `Keystore` (its key, its master seed, its
//! lock), the worker's copy of the master seed, any pending phrase and any
//! pending plan. It happens on [`Command::Lock`], when nothing has been done
//! for the idle period, when the app moves to the background
//! ([`WorkerHandle::background`]), when the worker shuts down, and when the
//! last handle is dropped. The last two first stop every command still
//! queued, as a move to the background does, so nothing queued (a spend
//! among it) runs on the way out.
//!
//! The idle period is measured from the person's last input:
//! [`WorkerHandle::touch`], which counts at once even while a command runs,
//! and a command that carries something they typed (unlock, create, the
//! confirmation words). Other commands are not input, so an interface that
//! polls the tip or refreshes on a timer cannot keep the store open, and
//! the period is checked before each queued command is taken, so polls
//! queued behind a slow node cannot either. Commands run one at a time, so
//! a running command is stopped when the idle period passes, as a cancel
//! stops it (below), and the store locks as soon as it has stopped: a walk
//! to a far key index never holds the lock back. Unlocking and creating are
//! the exception. They carry the person's input and count as activity when
//! they finish, so they are stopped only by a cancel; the wallet opening
//! inside them walks no further than the library's own diagnostic scope.
//!
//! # The node
//!
//! Changing or clearing the node detaches the open session from the node
//! it was opened against: a wallet open against it becomes the store alone,
//! and a pending plan is dropped, so nothing goes on asking that node or
//! sending to it. The next command that asks a node asks the one chosen
//! then.
//!
//! # Cancellation
//!
//! [`WorkerHandle::cancel`] asks every command sent so far to stop. The
//! handle records the newest request id it has given out and each command
//! compares its own, so a cancel raised after a command finished cannot
//! stop the next one: the next one has a later id.
//!
//! - A command the cancel reached before it started does nothing and is
//!   answered `Cancelled`, except one that only drops something (Lock,
//!   discarding a plan or a phrase, forgetting the node). So a spend queued
//!   behind a slow command does not sign after a cancel, nor after a move to
//!   the background, which cancels first.
//! - A running command reads it through the library's `recon::Cancel`:
//!   opening the wallet, a refresh, a status read, a restore, an
//!   acknowledged advance and a discovery sweep, between accounts and inside
//!   each walk. A restore or an advance is asked once more just before its
//!   one write, so a cancel that stops it has written nothing; one that
//!   arrives after the write is too late, and the command reports what it
//!   wrote. A stop, once heard, holds for the rest of the command, even
//!   when the person returns before it ends: the library's walk ended on
//!   it, and what it reports is cut short.
//! - What a stopped command answers: `Cancelled`, with nothing it read
//!   applied, for a refresh with the wallet open, a status read and a sweep
//!   (a sweep that stopped short reports nothing, never the accounts asked
//!   so far). Where the cancel stopped the wallet opening (an unlock, a
//!   create, a refresh of the store alone), the store stays open on its own,
//!   not reconciled, and the answer is its view, whose notice says so. A
//!   restore or an advance took the store out of its session to write it;
//!   stopped, it answers with its own reply, saying nothing was written,
//!   and the store back open on its own.
//! - A spend is never stopped between its reservation and its submission:
//!   the last point it can be stopped is before it starts.
//!
//! # Account numbers
//!
//! The command line names a derived account by its number (`restore
//! --account N`); the store keeps it in a record the library does not show
//! (docs/LIBRARY-PROPOSALS.md, 9a). The worker finds it as the command line
//! makes it: account N's tag is `derive::derive_account_tag(master, N)`, so
//! it derives 0, 1, 2 and on from the session's seed until every derived
//! account is found, or until the discovery sweep's ceiling, and remembers
//! what it found for as long as it runs. Nothing is asked of the node and
//! nothing written.
//!
//! # Progress
//!
//! The library counts how far its long operations have got (opening the
//! wallet, a restore, an advance, a sweep), and the worker sends each count
//! on as [`Event::Progress`]. A status read reports none: the library's walk
//! for it takes a cancel and no counter.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use mochimo_crypto::addr::Tag;
use mochimo_crypto::cli::args::Spend;
use mochimo_crypto::cli::create::{self, CONFIRM_POSITIONS, CreateEntropy, nothing_was_created};
use mochimo_crypto::cli::outcome::{Outcome, Shipped};
use mochimo_crypto::cli::{self, Code, address, discover, reconcile, restore};
use mochimo_crypto::consts::SEED_LEN;
use mochimo_crypto::keystore::{self, Disk, Keystore, SALT_LEN, Unlock};
use mochimo_crypto::mesh::spend::SpendPlan;
use mochimo_crypto::mesh::{MeshClient, Transport};
use mochimo_crypto::recon::{self, Cancel, Divergence, ScanScope, Unfinished};
use mochimo_crypto::tx::wire::Destination;
use mochimo_crypto::wallet::{StartupRefusal, Unopened, Wallet};
use mochimo_crypto::{Error, Secret, derive, mnemonic};
use zeroize::Zeroizing;

use crate::command::{Command, PlanId, RequestId};
use crate::entropy::{self, EntropyUnavailable};
use crate::event::{
    AccountReport, Activity, Discovered, Event, LockReason, PlanView, PlannedDestination, Progress,
    ReceiveView, Refusal, RefusalKind, Reply, SentView,
};
use crate::explorer::{
    self, AccountHistory, BlockSummary, BlocksView, ExplorerRefusal, MempoolView,
};
use crate::node::{self, Connect, NetworkName, SyncState};
use crate::secret::{PhraseForDisplay, SecretText};
use crate::spend::{self, SpendRequest};
use crate::text;
use crate::view::{
    AccountId, AccountKind, AccountRow, AccountState, Notice, NoticeKind, WalletView,
};

/// How long the wallet stays unlocked with nothing done, unless the
/// interface sets another period. Five minutes, as the dashboard rendering
/// shows ("auto-locks in 5 min"); docs/DECISIONS.md D24.
pub const DEFAULT_IDLE_LOCK: Duration = Duration::from_secs(5 * 60);

/// How the worker runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Lock after this long with nothing done.
    pub idle_lock: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            idle_lock: DEFAULT_IDLE_LOCK,
        }
    }
}

/// The worker has stopped; nothing more will be done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerStopped;

impl core::fmt::Display for WorkerStopped {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("the wallet worker has stopped")
    }
}

impl std::error::Error for WorkerStopped {}

enum Envelope {
    Command(RequestId, Command),
    Background,
    /// The idle period changed ([`WorkerHandle::set_idle_lock`]): measure the
    /// wait for the next command again.
    Wake,
    Shutdown,
}

/// When the person last did something, and how long the store may stay
/// open after that, shared by the handle and the worker so that input or a
/// new period during a long command counts at once rather than queueing
/// behind it (module doc, "Lifecycle").
struct PersonActivity {
    epoch: Instant,
    /// Milliseconds after `epoch`.
    last: AtomicU64,
    /// The idle period, in milliseconds.
    period: AtomicU64,
}

impl PersonActivity {
    fn new(period: Duration) -> PersonActivity {
        let activity = PersonActivity {
            epoch: Instant::now(),
            last: AtomicU64::new(0),
            period: AtomicU64::new(0),
        };
        activity.set_period(period);
        activity
    }

    fn period(&self) -> Duration {
        Duration::from_millis(self.period.load(Ordering::Relaxed))
    }

    fn set_period(&self, period: Duration) {
        let ms = u64::try_from(period.as_millis()).unwrap_or(u64::MAX);
        self.period.store(ms, Ordering::Relaxed);
    }

    /// Whether the person has been away for the idle period.
    fn away(&self) -> bool {
        self.idle_for() >= self.period()
    }

    fn now(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// The person did something now.
    fn mark(&self) {
        self.last.fetch_max(self.now(), Ordering::Relaxed);
    }

    /// How long since the person last did something.
    fn idle_for(&self) -> Duration {
        Duration::from_millis(self.now().saturating_sub(self.last.load(Ordering::Relaxed)))
    }
}

/// The request ids, shared by every clone of a handle and dropped with the
/// last one.
struct Requests {
    /// The newest request id to stop (0 for none); see the module doc.
    cancel: Arc<AtomicU64>,
    next: AtomicU64,
}

impl Requests {
    /// Stop every command sent so far (module doc, "Cancellation").
    fn cancel_sent(&self) {
        let newest = self.next.load(Ordering::Relaxed).saturating_sub(1);
        self.cancel.fetch_max(newest, Ordering::Relaxed);
    }
}

impl Drop for Requests {
    /// The last handle is gone, so nobody will read an answer: every
    /// command still queued is stopped, as [`WorkerHandle::shutdown`] stops
    /// them. Without this the worker would run what is queued before it
    /// noticed, a spend among them.
    fn drop(&mut self) {
        self.cancel_sent();
    }
}

/// The interface's side of the worker. Cloneable; when the last clone is
/// dropped, every command still queued is stopped and the worker locks
/// and stops.
#[derive(Clone)]
pub struct WorkerHandle {
    // Declared before `tx`, so it is dropped first: the last handle stops
    // the queued commands before the channel closes.
    requests: Arc<Requests>,
    tx: Sender<Envelope>,
    activity: Arc<PersonActivity>,
}

impl WorkerHandle {
    /// Queue a command. Its answer is the [`Event::Done`] with the id
    /// returned here.
    pub fn send(&self, command: Command) -> Result<RequestId, WorkerStopped> {
        let id = RequestId(self.requests.next.fetch_add(1, Ordering::Relaxed));
        self.tx
            .send(Envelope::Command(id, command))
            .map_err(|_| WorkerStopped)?;
        Ok(id)
    }

    /// The person did something: restart the idle period. No answer. Call it
    /// on the person's input; commands do not restart the period unless they
    /// carry something the person typed (module doc, "Lifecycle"). It takes
    /// effect at once, even while a command is running.
    pub fn touch(&self) {
        self.activity.mark();
    }

    /// Lock after `period` with nothing done from now on, in place of the
    /// period the worker was started with ([`Config::idle_lock`]). It takes
    /// effect at once, for a running command too, and counts from the
    /// person's last input, so a shorter period can lock straight away. The
    /// interface offers [`crate::preferences::IDLE_LOCK_MINUTES`].
    pub fn set_idle_lock(&self, period: Duration) {
        self.activity.set_period(period);
        let _ = self.tx.send(Envelope::Wake);
    }

    /// Ask every command sent so far to stop (module doc, "Cancellation"):
    /// one not yet started does nothing and answers with a refusal of kind
    /// [`RefusalKind::Cancelled`], and a running one stops where the library
    /// lets it, writing nothing. A stopped command that had opened or taken
    /// out the store (an unlock, a create, a refresh of the store alone, a
    /// restore, an advance) answers with the store as it is left, open on
    /// its own and not reconciled; any other answers `Cancelled`. Commands
    /// sent afterwards are not affected.
    pub fn cancel(&self) {
        self.requests.cancel_sent();
    }

    /// The app moved to the background: stop what can be stopped, then lock
    /// (docs/PLAN.md section 4.1). Answered by [`Event::Locked`] with
    /// [`LockReason::Background`] when a store was open.
    pub fn background(&self) {
        self.cancel();
        let _ = self.tx.send(Envelope::Background);
    }

    /// Lock and stop the worker. [`Event::Stopped`] follows.
    pub fn shutdown(&self) {
        self.cancel();
        let _ = self.tx.send(Envelope::Shutdown);
    }
}

/// Start the worker. Events arrive on the returned receiver, in order.
pub fn spawn<C: Connect>(
    config: Config,
    connect: C,
) -> std::io::Result<(WorkerHandle, Receiver<Event>)> {
    let (tx, rx) = mpsc::channel();
    let (events, received) = mpsc::channel();
    let cancel = Arc::new(AtomicU64::new(0));
    let flag = Arc::clone(&cancel);
    let activity = Arc::new(PersonActivity::new(config.idle_lock));
    let person = Arc::clone(&activity);
    thread::Builder::new()
        .name("tawara-wallet-core".into())
        .spawn(move || {
            // Declared first, dropped last: `Stopped` goes out after the
            // worker, and every secret it held, has been dropped -- on a
            // panic too, since the release profile unwinds.
            let _stopped = StopGuard(events.clone());
            let mut worker = Worker {
                connect,
                node: None,
                session: Session::Locked,
                create: None,
                plan: None,
                events,
                cancel: flag,
                activity: person,
                idle_stops: false,
                stopped: Arc::new(AtomicBool::new(false)),
                next_plan: 1,
                numbers: RefCell::new(BTreeMap::new()),
            };
            worker.run(&rx);
        })?;
    Ok((
        WorkerHandle {
            requests: Arc::new(Requests {
                cancel,
                next: AtomicU64::new(1),
            }),
            tx,
            activity,
        },
        received,
    ))
}

struct StopGuard(Sender<Event>);

impl Drop for StopGuard {
    fn drop(&mut self) {
        let _ = self.0.send(Event::Stopped {
            panicked: thread::panicking(),
        });
    }
}

/// An open store taken out of its session to be written.
struct TakenStore<T: Transport> {
    dir: PathBuf,
    store: Keystore<Disk>,
    master: Option<Secret<SEED_LEN>>,
    client: MeshClient<T>,
}

/// A store being created: everything `CreateConfirm` needs to write it.
struct PendingCreate {
    dir: PathBuf,
    password: SecretText,
    phrase: mnemonic::Phrase,
    entropy: Zeroizing<CreateEntropy>,
}

/// A spend laid out and waiting for confirmation.
struct PendingPlan {
    id: PlanId,
    from: Tag,
    plan: SpendPlan,
}

enum Session<T: Transport> {
    Locked,
    Store(StoreSession<T>),
    Wallet(WalletSession<T>),
}

struct StoreSession<T: Transport> {
    dir: PathBuf,
    store: Keystore<Disk>,
    master: Option<Secret<SEED_LEN>>,
    /// The node it was last reconciled against, when one was asked.
    client: Option<(String, MeshClient<T>)>,
    rows: Vec<AccountRow>,
    notice: Option<Notice>,
}

struct WalletSession<T: Transport> {
    dir: PathBuf,
    node: String,
    wallet: Wallet<Disk, T>,
    /// A copy of the store's master seed. `KeyAccess` borrows the seed for
    /// each call while `reserve_and_sign` and `settle_if_landed` borrow the
    /// wallet mutably, so the copy is what lets both borrows exist; the
    /// command line makes the same copy (`cli::decide`). It is a `Secret`
    /// and is dropped with the session.
    master: Option<Secret<SEED_LEN>>,
    rows: Vec<AccountRow>,
}

struct Worker<C: Connect> {
    connect: C,
    node: Option<String>,
    session: Session<C::Transport>,
    create: Option<PendingCreate>,
    plan: Option<PendingPlan>,
    events: Sender<Event>,
    cancel: Arc<AtomicU64>,
    /// The person's last input and the idle period, shared with the handle.
    activity: Arc<PersonActivity>,
    /// Whether the idle period stops the running command, as a cancel does:
    /// for every command but those carrying the person's input (module
    /// doc, "Lifecycle").
    idle_stops: bool,
    /// Whether the running command has been told to stop, shared by every
    /// stop predicate made for it ([`stop_predicate`]). Fresh for each
    /// command.
    stopped: Arc<AtomicBool>,
    next_plan: u64,
    /// The derived accounts' numbers found so far, by tag: a tag names one
    /// account of one seed, so what was found holds for as long as the
    /// worker runs. Not secret: a tag is public, and its number is the
    /// command line's name for it.
    numbers: RefCell<BTreeMap<Tag, Option<u32>>>,
}

/// Whether a command does nothing when a cancel reached it before it
/// started. Not the ones that only drop something: a Lock sent before a
/// cancel still locks.
fn stops_before_starting(command: &Command) -> bool {
    !matches!(
        command,
        Command::Lock | Command::DiscardPlan | Command::CreateAbandon | Command::ClearNode
    )
}

/// Whether a command carries something the person typed (a password, a
/// phrase, confirmation words), which restarts the idle period. Other
/// commands do not: an interface that polls the tip or refreshes on a timer
/// must not keep the store open (docs/DECISIONS.md D24).
fn typed_by_the_person(command: &Command) -> bool {
    matches!(
        command,
        Command::Unlock { .. }
            | Command::CreateBegin { .. }
            | Command::CreateConfirm { .. }
            | Command::CreateFromPhrase { .. }
    )
}

/// The stop predicate for the running request `id`, for the library's
/// `Cancel`: a cancel reached it (`cancel` holds the newest request id to
/// stop), or, with `idle`, the person has been away for the idle period as
/// it is when asked (module doc, "Lifecycle").
///
/// **Once it says stop, it says stop for the rest of the request**, through
/// `stopped`, which every predicate made for the request shares. A cancel
/// stays raised by itself, but the idle period does not: the person can
/// touch the interface after it has passed ([`WorkerHandle::touch`] counts
/// at once, from another thread). By then the library has ended its walk on
/// the first stop, and the report it hands back is cut short. A predicate
/// asked again afterwards that answered "go on" would let that report be
/// applied as a finding about the account, and would let the wallet opening
/// that follows a stopped restore or advance run against the stop that
/// ended it.
fn stop_predicate(
    cancel: Arc<AtomicU64>,
    id: RequestId,
    idle: Option<Arc<PersonActivity>>,
    stopped: Arc<AtomicBool>,
) -> impl Fn() -> bool + 'static {
    move || {
        if stopped.load(Ordering::Relaxed) {
            return true;
        }
        let stop = cancel.load(Ordering::Relaxed) >= id.0
            || idle.as_ref().is_some_and(|person| person.away());
        if stop {
            stopped.store(true, Ordering::Relaxed);
        }
        stop
    }
}

fn refused(kind: RefusalKind, text: impl Into<String>) -> Reply {
    Reply::Refused(Refusal {
        kind,
        text: text.into(),
    })
}

fn library(e: Error) -> Refusal {
    Refusal {
        kind: if matches!(e, Error::Cancelled) {
            RefusalKind::Cancelled
        } else {
            RefusalKind::Library
        },
        text: text::refusal(e),
    }
}

fn entropy_refusal(e: EntropyUnavailable) -> Refusal {
    Refusal {
        kind: RefusalKind::Entropy,
        text: e.to_string(),
    }
}

/// Whether the library refused a directory for its owner, mode or access
/// list, or for being a symbolic link. The access-list variants exist only
/// in a Windows build.
fn unsafe_directory(e: &Error) -> bool {
    #[cfg(windows)]
    if matches!(e, Error::UnsafeAcl { .. } | Error::UnsafeFileAcl { .. }) {
        return true;
    }
    matches!(
        e,
        Error::UnsafePermissions { .. } | Error::StoreDirectoryIsLink
    )
}

/// Why `Keystore::open` refused, in its words.
fn open_refusal(dir: &Path, e: Error) -> Refusal {
    let kind = match e {
        Error::Locked => RefusalKind::StoreInUse,
        Error::WrongPassword => RefusalKind::WrongPassword,
        Error::Missing => RefusalKind::NoStore,
        // No folder at all: the library reports it as the directory's
        // `stat` finding nothing, which says less than this.
        Error::Io {
            kind: std::io::ErrorKind::NotFound,
            ..
        } => {
            return Refusal {
                kind: RefusalKind::NoStore,
                text: format!(
                    "there is no keystore at {}: the folder does not exist",
                    dir.display()
                ),
            };
        }
        Error::Cancelled => RefusalKind::Cancelled,
        ref e if unsafe_directory(e) => RefusalKind::UnsafeDirectory,
        _ => RefusalKind::Library,
    };
    Refusal {
        kind,
        text: format!("cannot open the keystore at {}: {e}", dir.display()),
    }
}

/// Open the store at `dir` with a fresh nonce seed.
fn open_store(dir: &Path, password: &SecretText) -> Result<Keystore<Disk>, Refusal> {
    let nonce = entropy::nonce_seed().map_err(entropy_refusal)?;
    Keystore::open(
        dir,
        &Unlock {
            password: password.expose().as_bytes(),
            nonce_seed: *nonce,
        },
    )
    .map_err(|e| open_refusal(dir, e))
}

/// A copy of the store's master seed (see [`WalletSession::master`]).
fn master_of(store: &Keystore<Disk>) -> Result<Option<Secret<SEED_LEN>>, Refusal> {
    store
        .master()
        .map(|m| m.map(Secret::duplicate))
        .map_err(library)
}

/// A spend's destinations, with "everything" resolved to `everything`: the
/// command line's `spend_destinations`, which is private there and which
/// re-signing needs (its `resign_destinations`, also private).
fn destinations(spend: &Spend, everything: u64) -> Vec<Destination> {
    spend
        .dsts
        .iter()
        .map(|d| Destination {
            tag: d.to,
            reference: d.reference,
            amount: d.amount.unwrap_or(everything),
        })
        .collect()
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn reference_text(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// Refuse a key index past [`crate::MAX_KEY_INDEX`] for a walk the person
/// named (`what` is "a scan to key index" or "an advance to key index"),
/// as the command line's parser refuses it.
fn within_key_range(what: &str, index: u32) -> Result<(), Refusal> {
    if index <= crate::MAX_KEY_INDEX {
        return Ok(());
    }
    Err(Refusal {
        kind: RefusalKind::OutOfRange,
        text: format!(
            "{what} {index} is out of range: key indices run from 0 to {}. The last position \
             cannot be advanced from, so an account placed there could never spend, and a walk \
             to it would end at a position the walk never derives. Nothing was done.",
            crate::MAX_KEY_INDEX
        ),
    })
}

/// The command line's bounds on a discovery sweep (`cli::args`'s `--to`),
/// for its two reasons.
fn discover_bound(to: u32) -> Result<(), Refusal> {
    const AT_ZERO: &str = "A sweep to account 0 asks only about the account a new store \
                           already holds, so it can find nothing the store does not record.";
    const PAST_MAX: &str = "Every account in the sweep is one request to the node, so the \
                            bound is this wallet's own: it keeps a mistyped number from \
                            becoming thousands of requests.";
    let max = crate::DISCOVER_MAX_TO;
    let why = match to {
        0 => AT_ZERO,
        t if t > max => PAST_MAX,
        _ => return Ok(()),
    };
    Err(Refusal {
        kind: RefusalKind::OutOfRange,
        text: format!(
            "a discovery sweep goes to an account from 1 to {max}, and {to} is outside that. \
             {why} Nothing was done."
        ),
    })
}

/// Make the folder the store's own folder goes in, when it is missing. The
/// library makes only the store's folder, inside a parent that exists
/// (`mkdirat` on Unix, `CreateDirectoryW` on Windows), and on a fresh
/// profile the default location's application folder
/// ([`crate::location::default_store_dir`]) is not there yet. Called only
/// once the store is about to be written; folders made here are private to
/// the user on Unix (mode `0700`), and on Windows take the access list of
/// the folder they are made in.
fn prepare_parent(dir: &Path) -> Result<(), Refusal> {
    let Some(parent) = dir.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return Ok(());
    };
    let mut folders = std::fs::DirBuilder::new();
    folders.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        folders.mode(0o700);
    }
    folders.create(parent).map_err(|e| Refusal {
        kind: RefusalKind::Library,
        text: format!(
            "cannot make the folder {} to hold the store: {e}. Nothing was created.",
            parent.display()
        ),
    })
}

/// The store's accounts with no wallet open, from its records alone: none
/// is reconciled and none can spend.
fn store_rows(store: &Keystore<Disk>) -> Result<Vec<AccountRow>, Refusal> {
    let held = address::accounts_in(store).map_err(library)?;
    Ok(held
        .into_iter()
        .map(|h| AccountRow {
            id: AccountId::from_tag(h.tag),
            kind: h.kind.into(),
            index: h.index.get(),
            state: AccountState::NotReconciled,
            spendable: false,
            number: None,
        })
        .collect())
}

/// The Store session's notice when the library would not open the wallet,
/// with each account's report put on its row (module doc, "Opening the
/// wallet").
///
/// Two refusals are not a key index out of step, and the library's startup
/// page, which explains one, is not shown for them. When every lookup failed
/// and at least one because the node did not answer, the node is reported
/// silent and the rows are left unreconciled. When the node answered
/// "account not found" for every account (a store nobody has paid yet, or
/// one whose every account was emptied), the notice is the library's "THIS
/// STORE IS NOT WHOLE", naming each. Anything else is the library's own
/// "WALLET WILL NOT START" page.
fn refused_notice(rows: &mut [AccountRow], refusal: StartupRefusal) -> Option<Notice> {
    let lookups_only = refusal.diverged.iter().all(|d| {
        matches!(
            d,
            Divergence::ChainUnreachable { .. } | Divergence::TagUnresolved { .. }
        )
    });
    let silent = refusal.diverged.iter().find_map(|d| match d {
        Divergence::ChainUnreachable { cause, .. } => Some(cause.clone()),
        _ => None,
    });
    if lookups_only && let Some(cause) = silent {
        return Some(Notice {
            kind: NoticeKind::NodeSilent,
            text: format!("{NODE_SILENT}\n\n{}", text::refusal(cause)),
        });
    }
    for d in &refusal.diverged {
        update_row(
            rows,
            &d.tag(),
            AccountState::from_divergence(d),
            false,
            None,
        );
    }
    if lookups_only {
        return not_whole(&refusal.diverged);
    }
    Some(Notice {
        kind: NoticeKind::WillNotStart,
        text: text::page(&[], Outcome::StartupRefused(Box::new(refusal))),
    })
}

/// The library's notice that the store is not whole, for these diverged
/// accounts, or none for a whole store.
fn not_whole(diverged: &[Divergence]) -> Option<Notice> {
    text::standing_notice(diverged).map(|text| Notice {
        kind: NoticeKind::NotWhole,
        text,
    })
}

/// The store's accounts with the wallet open: the partition it opened with.
fn wallet_rows<T: Transport>(wallet: &Wallet<Disk, T>) -> Result<Vec<AccountRow>, Refusal> {
    let held = address::accounts_in(wallet.store()).map_err(library)?;
    Ok(held
        .into_iter()
        .map(|h| {
            let (state, spendable) =
                if let Some((_, status)) = wallet.accounts().iter().find(|(t, _)| *t == h.tag) {
                    let state = AccountState::from_status(status);
                    let spendable = matches!(state, AccountState::InSync { .. });
                    (state, spendable)
                } else if let Some(d) = wallet.divergence_for(&h.tag) {
                    (AccountState::from_divergence(d), false)
                } else {
                    (AccountState::NotReconciled, false)
                };
            AccountRow {
                id: AccountId::from_tag(h.tag),
                kind: h.kind.into(),
                index: h.index.get(),
                state,
                spendable,
                number: None,
            }
        })
        .collect())
}

/// Which of `derived` the seed derives, and as which account: the tag of
/// account N is `derive::derive_account_tag(master, N)`, as `restore
/// --account N` makes it, so N is found by deriving 0, 1, 2 and on until
/// every one is found or the discovery sweep's ceiling is passed. One
/// derivation is one key's public half, about what a sweep pays per
/// account without the node. An account not found is `None`.
fn derivation_numbers(master: &Secret<SEED_LEN>, derived: &[Tag]) -> Vec<(Tag, Option<u32>)> {
    let mut left: Vec<Tag> = derived.to_vec();
    let mut found = Vec::with_capacity(derived.len());
    for n in 0..=crate::DISCOVER_MAX_TO {
        if left.is_empty() {
            break;
        }
        let tag = derive::derive_account_tag(master, n);
        if let Some(at) = left.iter().position(|t| *t == tag) {
            found.push((left.swap_remove(at), Some(n)));
        }
    }
    found.extend(left.into_iter().map(|t| (t, None)));
    found
}

/// Replace one account's row after an operation on it.
fn update_row(
    rows: &mut [AccountRow],
    tag: &Tag,
    state: AccountState,
    spendable: bool,
    index: Option<u32>,
) {
    if let Some(row) = rows.iter_mut().find(|r| r.id.tag() == *tag) {
        row.state = state;
        row.spendable = spendable;
        if let Some(i) = index {
            row.index = i;
        }
    }
}

const NODE_CHANGED: &str = "The node was changed, so nothing is reconciled against the new one \
                            yet and nothing can be sent. The store is open: its accounts and \
                            their destinations can be shown. Refresh to reconcile against the \
                            new node.";

const NO_NODE: &str = "No node is chosen, so nothing was reconciled and nothing can be sent. \
                       The store is open: its accounts and their destinations can be shown. \
                       Choose a node to reconcile them.";

pub(crate) const NODE_SILENT: &str = "The node did not answer, so nothing was reconciled and \
                                      nothing can be sent. The store is open: its accounts and \
                                      their destinations can be shown.";

const OPEN_CANCELLED: &str = "Reconciling the store was cancelled, so nothing can be sent. The \
                              store is open: its accounts and their destinations can be shown. \
                              Refresh to reconcile them.";

const ADVANCE_CANCELLED: &str = "The advance was cancelled before it wrote anything: no key index \
                                 moved.";

const RESTORE_CANCELLED: &str = "The restore was cancelled before it wrote anything: no account \
                                 was added to the store.";

impl<C: Connect> Worker<C> {
    fn emit(&self, event: Event) {
        // A dropped receiver means nobody is listening; the worker carries
        // on until its handles go.
        let _ = self.events.send(event);
    }

    fn busy(&self, id: RequestId, activity: Activity) {
        self.emit(Event::Busy { id, activity });
    }

    /// Whether a cancel has reached request `id` (module doc,
    /// "Cancellation").
    fn cancel_reached(&self, id: RequestId) -> bool {
        self.cancel.load(Ordering::Relaxed) >= id.0
    }

    /// Whether the running request `id` should stop, for the library's
    /// `Cancel`: [`stop_predicate`], sharing the request's memory of a stop
    /// with every other predicate made for it.
    fn stop_asked(&self, id: RequestId) -> impl Fn() -> bool + 'static {
        stop_predicate(
            Arc::clone(&self.cancel),
            id,
            self.idle_stops.then(|| Arc::clone(&self.activity)),
            Arc::clone(&self.stopped),
        )
    }

    /// Send on how far request `id` has got, as the library counted it.
    fn progress(&self, id: RequestId, counted: recon::Progress) {
        self.emit(Event::Progress {
            id,
            progress: Progress::of(counted),
        });
    }

    fn holds_secrets(&self) -> bool {
        !matches!(self.session, Session::Locked) || self.create.is_some()
    }

    fn run(&mut self, rx: &Receiver<Envelope>) {
        loop {
            // Checked before anything more is taken from the queue: commands
            // queued behind a slow one (polls outrunning a slow node) must
            // not keep the store open past the idle period.
            if self.holds_secrets() && self.activity.away() {
                self.lock(LockReason::Idle);
            }
            let envelope = if self.holds_secrets() {
                let left = self
                    .activity
                    .period()
                    .saturating_sub(self.activity.idle_for());
                match rx.recv_timeout(left) {
                    Ok(e) => e,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => Envelope::Shutdown,
                }
            } else {
                rx.recv().unwrap_or(Envelope::Shutdown)
            };
            match envelope {
                Envelope::Command(id, command) => {
                    let typed = typed_by_the_person(&command);
                    self.idle_stops = !typed;
                    self.stopped = Arc::new(AtomicBool::new(false));
                    let reply = if stops_before_starting(&command) && self.cancel_reached(id) {
                        // Sent before a cancel and not yet started: it does
                        // nothing at all (module doc). This is what keeps a
                        // spend queued behind a slow command from signing
                        // after a cancel or a move to the background.
                        Reply::Refused(library(Error::Cancelled))
                    } else {
                        self.handle(id, command)
                    };
                    // A command is the person's activity only when they
                    // typed it (module doc, "Lifecycle"), and counts from
                    // when it finished: a store has just opened.
                    if typed {
                        self.activity.mark();
                    }
                    self.emit(Event::Done { id, reply });
                }
                Envelope::Background => self.lock(LockReason::Background),
                Envelope::Wake => {}
                Envelope::Shutdown => {
                    self.lock(LockReason::Shutdown);
                    return;
                }
            }
        }
    }

    /// Drop the session and everything pending. `Locked` is reported when a
    /// store was open or a recovery phrase was waiting, so a screen showing
    /// either learns that it is gone.
    fn lock(&mut self, reason: LockReason) {
        let phrase = self.create.take().is_some();
        self.plan = None;
        let was_open = !matches!(
            core::mem::replace(&mut self.session, Session::Locked),
            Session::Locked
        );
        if was_open || phrase {
            self.emit(Event::Locked { reason });
        }
    }

    /// The store could not be read back on the way to reopening it
    /// (`reopen`), and is gone: report it as closed, with everything
    /// pending.
    fn closed(&mut self, reason: LockReason) {
        self.create = None;
        self.plan = None;
        self.session = Session::Locked;
        self.emit(Event::Locked { reason });
    }

    fn view(&self) -> Option<WalletView> {
        let mut view = match &self.session {
            Session::Locked => return None,
            Session::Store(s) => WalletView {
                dir: s.dir.clone(),
                accounts: s.rows.clone(),
                opened: false,
                notice: s.notice.clone(),
            },
            Session::Wallet(w) => WalletView {
                dir: w.dir.clone(),
                accounts: w.rows.clone(),
                opened: true,
                notice: not_whole(w.wallet.diverged()),
            },
        };
        self.number(&mut view.accounts);
        Some(view)
    }

    /// Put each derived account's number on its row (module doc, "Account
    /// numbers"), searching once for those not yet known.
    fn number(&self, rows: &mut [AccountRow]) {
        let master = match &self.session {
            Session::Locked => None,
            Session::Store(s) => s.master.as_ref(),
            Session::Wallet(w) => w.master.as_ref(),
        };
        let mut known = self.numbers.borrow_mut();
        let missing: Vec<Tag> = rows
            .iter()
            .filter(|r| r.kind == AccountKind::Derived && !known.contains_key(&r.id.tag()))
            .map(|r| r.id.tag())
            .collect();
        if let Some(master) = master
            && !missing.is_empty()
        {
            known.extend(derivation_numbers(master, &missing));
        }
        for row in rows {
            row.number = known.get(&row.id.tag()).copied().flatten();
        }
    }

    fn handle(&mut self, id: RequestId, command: Command) -> Reply {
        match command {
            Command::SetNode { url } => self.set_node(&url),
            Command::ClearNode => {
                if self.node.take().is_some() {
                    self.detach_node(NoticeKind::NoNode, NO_NODE);
                }
                Reply::NodeCleared { view: self.view() }
            }
            Command::CreateBegin {
                dir,
                password,
                password_again,
            } => self.create_begin(dir, password, &password_again),
            Command::CreateConfirm { answer } => self.create_confirm(id, &answer),
            Command::CreateAbandon => {
                self.create = None;
                Reply::CreateAbandoned
            }
            Command::CreateFromPhrase {
                dir,
                password,
                password_again,
                phrase,
            } => self.create_from_phrase(id, dir, &password, &password_again, &phrase),
            Command::Unlock { dir, password } => match self.unlock(id, dir, &password) {
                Ok(view) => Reply::Unlocked(view),
                Err(r) => Reply::Refused(r),
            },
            Command::Lock => {
                self.lock(LockReason::Asked);
                Reply::Locked
            }
            Command::Refresh => self.refresh(id),
            Command::Receive { account } => self.receive(account),
            Command::PlanSend { spend } => self.plan_send(id, &spend),
            Command::ConfirmSend { plan } => self.confirm_send(id, plan),
            Command::DiscardPlan => {
                self.plan = None;
                Reply::PlanDiscarded
            }
            Command::Settle { account } => self.settle(id, account),
            Command::Resign { spend } => self.resign(id, &spend),
            Command::SubmitArtifact { artifact_hex } => self.submit_artifact(id, &artifact_hex),
            Command::Status { account, scan_to } => self.status(id, account, scan_to),
            Command::Reconcile {
                account,
                advance_to,
            } => self.reconcile(id, account, advance_to),
            Command::Restore {
                account_index,
                scan_to,
            } => self.restore(id, account_index, scan_to),
            Command::Discover { to } => self.discover(id, to),
            Command::NetworkStatus => self.network_status(id),
            Command::Networks => self.networks(id),
            Command::Blocks => self.blocks(id),
            Command::Mempool => self.mempool(id),
            Command::Activity => self.activity(id, None),
            Command::OlderActivity(pages) => self.activity(id, Some(&pages)),
            Command::Review => self.review(id),
        }
    }

    fn set_node(&mut self, url: &str) -> Reply {
        match node::check_node_url(&self.connect, url) {
            Ok(_) => {
                let url = url.trim().to_owned();
                if self.node.as_deref() != Some(url.as_str()) {
                    self.node = Some(url.clone());
                    self.detach_node(NoticeKind::NodeChanged, NODE_CHANGED);
                }
                Reply::NodeSet {
                    url,
                    view: self.view(),
                }
            }
            Err(e) => refused(RefusalKind::NodeRefused, e.to_string()),
        }
    }

    /// The node choice changed: nothing open may go on asking the node it
    /// was opened against, or send to it. A wallet open against it becomes
    /// the store alone, with no node, and a pending plan (laid out against
    /// it) is dropped. The next command that asks a node asks the one now
    /// chosen; [`Command::Refresh`] reconciles against it and opens the
    /// wallet again.
    fn detach_node(&mut self, kind: NoticeKind, notice: &str) {
        self.plan = None;
        let (dir, store, master, mut rows) =
            match core::mem::replace(&mut self.session, Session::Locked) {
                Session::Locked => return,
                Session::Store(s) => (s.dir, s.store, s.master, s.rows),
                Session::Wallet(w) => {
                    let (store, _old) = w.wallet.into_parts();
                    (w.dir, store, w.master, w.rows)
                }
            };
        // The rows the session already holds, with what the old node said
        // taken off them: nothing is read from the store, so nothing here
        // can fail and close it.
        for row in &mut rows {
            row.state = AccountState::NotReconciled;
            row.spendable = false;
        }
        self.session = Session::Store(StoreSession {
            dir,
            store,
            master,
            client: None,
            rows,
            notice: Some(Notice {
                kind,
                text: notice.to_owned(),
            }),
        });
    }

    /// A client for the chosen node.
    fn client(&self) -> Result<(String, MeshClient<C::Transport>), Refusal> {
        let Some(url) = self.node.clone() else {
            return Err(Refusal {
                kind: RefusalKind::NoNode,
                text: "no node is chosen; choose one to ask the chain".into(),
            });
        };
        let transport = node::check_node_url(&self.connect, &url).map_err(|e| Refusal {
            kind: RefusalKind::NodeRefused,
            text: e.to_string(),
        })?;
        Ok((url, MeshClient::new(transport)))
    }

    // ----------------------------------------------------------------- create

    /// The checks `create` makes before it shows a phrase, in its order: an
    /// occupied directory, then the password floor, then the two passwords
    /// agreeing.
    fn create_checks(dir: &Path, password: &SecretText, again: &SecretText) -> Result<(), Refusal> {
        if let Some(what) = keystore::occupied(dir) {
            return Err(Refusal {
                kind: RefusalKind::Occupied,
                text: format!(
                    "a keystore already exists at {} (its {what} is present). Nothing was shown \
                     and nothing was changed. Choose another folder for a new store, or open this \
                     one.",
                    dir.display()
                ),
            });
        }
        if let Some(why) = create::password_refusal(password.expose()) {
            return Err(Refusal {
                kind: RefusalKind::PasswordTooShort,
                text: why,
            });
        }
        if password.expose().as_bytes() != again.expose().as_bytes() {
            return Err(Refusal {
                kind: RefusalKind::PasswordsDiffer,
                text: "those two passwords are not the same. Nothing was created.".into(),
            });
        }
        Ok(())
    }

    fn create_begin(&mut self, dir: PathBuf, password: SecretText, again: &SecretText) -> Reply {
        self.create = None;
        if let Err(r) = Self::create_checks(&dir, &password, again) {
            return Reply::Refused(r);
        }
        let entropy = match entropy::create_entropy() {
            Ok(e) => e,
            Err(e) => return Reply::Refused(entropy_refusal(e)),
        };
        let phrase = match mnemonic::phrase_from_entropy(&entropy.phrase) {
            Ok(p) => p,
            Err(e) => return refused(RefusalKind::Library, nothing_was_created(e)),
        };
        let display = PhraseForDisplay::new(phrase.expose());
        self.create = Some(PendingCreate {
            dir,
            password,
            phrase,
            entropy,
        });
        Reply::CreatePhrase {
            phrase: display,
            confirm_positions: CONFIRM_POSITIONS,
        }
    }

    fn create_refusal(e: Error) -> Refusal {
        let kind = match e {
            Error::PasswordTooShort { .. } => RefusalKind::PasswordTooShort,
            Error::Exists { .. } => RefusalKind::Occupied,
            Error::Locked => RefusalKind::StoreInUse,
            ref e if unsafe_directory(e) => RefusalKind::UnsafeDirectory,
            _ => RefusalKind::Library,
        };
        Refusal {
            kind,
            text: nothing_was_created(e),
        }
    }

    fn create_confirm(&mut self, id: RequestId, answer: &SecretText) -> Reply {
        let Some(pending) = self.create.as_ref() else {
            return refused(
                RefusalKind::NothingToConfirm,
                "there is no recovery phrase waiting to be confirmed. Nothing was created.",
            );
        };
        if !create::confirmation_matches(pending.phrase.expose(), answer.expose()) {
            let p = CONFIRM_POSITIONS;
            return refused(
                RefusalKind::ConfirmationWrong,
                format!(
                    "those are not words {}, {} and {}. Nothing was created.\n\n  The phrase \
                     belongs to no store yet. Check the words against it and answer again, or \
                     start over for a fresh phrase -- and either way, write the phrase down \
                     before anything is sent to the wallet.",
                    p[0], p[1], p[2]
                ),
            );
        }
        // A folder that cannot be made, or a store the library refuses to
        // write there (an unsafe folder, one that appeared since), leaves
        // the phrase waiting, as a wrong answer does: the person can put the
        // folder right and confirm again, or start over. It is taken only
        // once the store is written.
        if let Err(r) = prepare_parent(&pending.dir) {
            return Reply::Refused(r);
        }
        self.busy(id, Activity::DerivingKey);
        let created = match create::create(
            &pending.dir,
            pending.phrase.expose(),
            pending.password.expose(),
            pending.entropy.salt,
            pending.entropy.nonce_seed,
        ) {
            Ok(c) => c,
            Err(e) => return Reply::Refused(Self::create_refusal(e)),
        };
        let Some(pending) = self.create.take() else {
            return refused(RefusalKind::NothingToConfirm, "Nothing was created.");
        };
        let first = AccountId::from_tag(created.tag);
        let opened = self.unlock(id, pending.dir.clone(), &pending.password);
        Reply::Created {
            dir: pending.dir,
            first,
            opened,
        }
    }

    fn create_from_phrase(
        &mut self,
        id: RequestId,
        dir: PathBuf,
        password: &SecretText,
        again: &SecretText,
        phrase: &SecretText,
    ) -> Reply {
        self.create = None;
        if let Err(r) = Self::create_checks(&dir, password, again) {
            return Reply::Refused(r);
        }
        let salt = match entropy::os_bytes::<SALT_LEN>() {
            Ok(s) => s,
            Err(e) => return Reply::Refused(entropy_refusal(e)),
        };
        let nonce = match entropy::nonce_seed() {
            Ok(n) => n,
            Err(e) => return Reply::Refused(entropy_refusal(e)),
        };
        if let Err(r) = prepare_parent(&dir) {
            return Reply::Refused(r);
        }
        self.busy(id, Activity::DerivingKey);
        let created = match create::create(&dir, phrase.expose(), password.expose(), *salt, *nonce)
        {
            Ok(c) => c,
            Err(e) => return Reply::Refused(Self::create_refusal(e)),
        };
        let first = AccountId::from_tag(created.tag);
        let opened = self.unlock(id, dir.clone(), password);
        Reply::Created { dir, first, opened }
    }

    // ----------------------------------------------------------------- unlock

    fn unlock(
        &mut self,
        id: RequestId,
        dir: PathBuf,
        password: &SecretText,
    ) -> Result<WalletView, Refusal> {
        self.lock(LockReason::Replaced);
        self.busy(id, Activity::DerivingKey);
        let store = open_store(&dir, password)?;
        let master = master_of(&store)?;
        self.open_session(id, dir, store, master)
    }

    /// Make an open store a session: the wallet when the library opens it,
    /// the store on its own when it refuses or a cancel stops it (module
    /// doc, "Opening the wallet"). The store is never closed here except
    /// when its records cannot be read.
    fn open_session(
        &mut self,
        id: RequestId,
        dir: PathBuf,
        store: Keystore<Disk>,
        master: Option<Secret<SEED_LEN>>,
    ) -> Result<WalletView, Refusal> {
        let (url, client) = match self.client() {
            Ok(c) => c,
            Err(r) => {
                let rows = store_rows(&store)?;
                let notice = if r.kind == RefusalKind::NoNode {
                    Notice {
                        kind: NoticeKind::NoNode,
                        text: NO_NODE.to_owned(),
                    }
                } else {
                    Notice {
                        kind: NoticeKind::NodeRefused,
                        text: r.text,
                    }
                };
                self.session = Session::Store(StoreSession {
                    dir,
                    store,
                    master,
                    client: None,
                    rows,
                    notice: Some(notice),
                });
                return self.view().ok_or_else(Self::lost);
            }
        };
        self.busy(id, Activity::AskingNode);
        let asked = self.stop_asked(id);
        let opened = Wallet::open_or_return_with_progress(
            store,
            client,
            master.as_ref(),
            &Cancel::when(&asked),
            &mut |counted| self.progress(id, counted),
        );
        match opened {
            Ok(wallet) => {
                let rows = wallet_rows(&wallet)?;
                self.session = Session::Wallet(WalletSession {
                    dir,
                    node: url,
                    wallet,
                    master,
                    rows,
                });
            }
            Err(Unopened { why, store, client }) => {
                let mut rows = store_rows(&store)?;
                let notice = match why {
                    Unfinished::Cancelled => Some(Notice {
                        kind: NoticeKind::Cancelled,
                        text: OPEN_CANCELLED.to_owned(),
                    }),
                    Unfinished::Refused(refusal) => refused_notice(&mut rows, refusal),
                };
                self.session = Session::Store(StoreSession {
                    dir,
                    store,
                    master,
                    client: Some((url, client)),
                    rows,
                    notice,
                });
            }
        }
        self.view().ok_or_else(Self::lost)
    }

    /// Reopen a store taken out of an open session (a refresh against a new
    /// node, an advance, a restore). Any pending plan is dropped: it was laid
    /// out against the session that is gone. When the store's records cannot
    /// be read back, it is gone and reported closed.
    fn reopen(
        &mut self,
        id: RequestId,
        dir: PathBuf,
        store: Keystore<Disk>,
        master: Option<Secret<SEED_LEN>>,
    ) -> Result<WalletView, Refusal> {
        self.plan = None;
        let opened = self.open_session(id, dir, store, master);
        if opened.is_err() && matches!(self.session, Session::Locked) {
            self.closed(LockReason::ReopenFailed);
        }
        opened
    }

    fn lost() -> Refusal {
        Refusal {
            kind: RefusalKind::NotUnlocked,
            text: "the store is not open".into(),
        }
    }

    fn not_unlocked() -> Reply {
        refused(
            RefusalKind::NotUnlocked,
            "the wallet is locked; unlock it first",
        )
    }

    fn not_open() -> Reply {
        refused(
            RefusalKind::WalletNotOpen,
            "the wallet did not open against the node, so nothing can be sent from it; its \
             notice says why",
        )
    }

    // ---------------------------------------------------------------- refresh

    fn refresh(&mut self, id: RequestId) -> Reply {
        let node_moved = match &self.session {
            Session::Wallet(w) => self.node.as_deref() != Some(w.node.as_str()),
            _ => false,
        };
        match core::mem::replace(&mut self.session, Session::Locked) {
            Session::Locked => Self::not_unlocked(),
            Session::Wallet(w) if !node_moved => {
                self.session = Session::Wallet(w);
                self.refresh_wallet(id)
            }
            Session::Wallet(w) => {
                let (store, _old) = w.wallet.into_parts();
                match self.reopen(id, w.dir, store, w.master) {
                    Ok(view) => Reply::Wallet(view),
                    Err(r) => Reply::Refused(r),
                }
            }
            Session::Store(s) => match self.reopen(id, s.dir, s.store, s.master) {
                Ok(view) => Reply::Wallet(view),
                Err(r) => Reply::Refused(r),
            },
        }
    }

    /// Each account's status read fresh, through the open wallet's client.
    /// The library refuses to spend from an account `Wallet::open` set
    /// aside, by what it found then; so when such an account reconciles now,
    /// the wallet is opened again and it rejoins.
    fn refresh_wallet(&mut self, id: RequestId) -> Reply {
        self.busy(id, Activity::AskingNode);
        let asked = self.stop_asked(id);
        let Session::Wallet(w) = &mut self.session else {
            return Self::not_unlocked();
        };
        let tags: Vec<Tag> = w.rows.iter().map(|r| r.id.tag()).collect();
        let mut fresh = Vec::with_capacity(tags.len());
        let mut recovered = false;
        for tag in &tags {
            // Each account is a round trip or more; the rows change only
            // when every one was read, so a stop leaves them as they were.
            if asked() {
                return Reply::Refused(library(Error::Cancelled));
            }
            let reconciled_at_open = w.wallet.accounts().iter().any(|(t, _)| t == tag);
            let read = match recon::access_for(w.wallet.store(), tag, w.master.as_ref()) {
                Ok(access) => w.wallet.status_with(
                    tag,
                    &access,
                    &ScanScope::DIAGNOSTIC,
                    &Cancel::when(&asked),
                ),
                Err(d) => Err(d),
            };
            // The library reports a walk it was told to stop as a
            // divergence whose search was cut short; that is not a finding
            // about the account, so nothing read is applied.
            if asked() {
                return Reply::Refused(library(Error::Cancelled));
            }
            recovered |= w.wallet.divergence_for(tag).is_some() && read.is_ok();
            let state = match read {
                Ok(status) => AccountState::from_status(&status),
                Err(d) => AccountState::from_divergence(&d),
            };
            let spendable = reconciled_at_open && matches!(state, AccountState::InSync { .. });
            fresh.push((*tag, state, spendable));
        }
        if recovered {
            let Session::Wallet(w) = core::mem::replace(&mut self.session, Session::Locked) else {
                return Self::not_unlocked();
            };
            let (store, _old) = w.wallet.into_parts();
            return match self.reopen(id, w.dir, store, w.master) {
                Ok(view) => Reply::Wallet(view),
                Err(r) => Reply::Refused(r),
            };
        }
        for (tag, state, spendable) in fresh {
            let index = w
                .wallet
                .store()
                .view(&tag)
                .ok()
                .flatten()
                .map(|v| v.wots_index.get());
            update_row(&mut w.rows, &tag, state, spendable, index);
        }
        self.view().map_or_else(Self::not_unlocked, Reply::Wallet)
    }

    // ---------------------------------------------------------------- receive

    fn receive(&self, account: AccountId) -> Reply {
        let tag = account.tag();
        let (store, master) = match &self.session {
            Session::Locked => return Self::not_unlocked(),
            Session::Store(s) => (&s.store, s.master.as_ref()),
            Session::Wallet(w) => (w.wallet.store(), w.master.as_ref()),
        };
        // The command line's `address <tag>` (`cli::cmd_address`).
        let found = cli::key_access(store, &tag, master)
            .and_then(|access| address::address_of(store, &tag, &access));
        match found {
            Ok(place) => Reply::Receive(ReceiveView {
                account,
                destination: account.destination(),
                index: place.index.get(),
                ledger_address: hex(&place.address),
                text: text::page(
                    &[],
                    Outcome::Address {
                        tag,
                        address: place.address,
                        index: place.index,
                    },
                ),
            }),
            Err(Error::NoSuchAccount) => refused(
                RefusalKind::Library,
                text::page(&[], Outcome::NoAccountToAddress { tag }),
            ),
            Err(e) => Reply::Refused(library(e)),
        }
    }

    // ------------------------------------------------------------------ spend

    fn diverged_refusal(d: &Divergence) -> Reply {
        refused(
            RefusalKind::Diverged,
            text::page(&[], Outcome::Diverged(Box::new(d.clone()))),
        )
    }

    fn plan_send(&mut self, id: RequestId, request: &SpendRequest) -> Reply {
        self.plan = None;
        if let Some(reply) = self.needs_wallet() {
            return reply;
        }
        let Session::Wallet(w) = &self.session else {
            return Self::not_open();
        };
        let spend = match spend::check(request) {
            Ok(c) => c,
            Err(e) => return refused(RefusalKind::Spend(e.clone()), e.to_string()),
        };
        let tag = spend.tag;
        if let Some(d) = w.wallet.divergence_for(&tag) {
            return Self::diverged_refusal(d);
        }
        self.busy(id, Activity::AskingNode);
        // The command line's layout (`cli::plan_spend`): for "everything",
        // one ledger read gives both the amount and the plan, so it cannot
        // fail to empty the account because the balance moved between two.
        let planned = cli::key_access(w.wallet.store(), &tag, w.master.as_ref())
            .and_then(|access| cli::plan_spend(&w.wallet, &spend, &access));
        let plan: SpendPlan = match planned {
            Ok(p) => p,
            Err(e) => return Reply::Refused(library(e)),
        };
        // Every destination must render before a key can be spent on this
        // plan, as the command line requires (`cli::cmd_send`).
        let mut shown = Vec::with_capacity(spend.dsts.len());
        for d in &spend.dsts {
            let Some(destination) = AccountId::from_tag(d.to).destination() else {
                return refused(
                    RefusalKind::Library,
                    format!(
                        "cannot render the destination for the tag whose hex is {}; nothing was \
                         reserved",
                        hex(&d.to)
                    ),
                );
            };
            let amount = plan
                .dsts()
                .iter()
                .find(|p| p.tag == d.to && p.reference == d.reference)
                .map_or(0, |p| p.amount);
            shown.push(PlannedDestination {
                to: AccountId::from_tag(d.to),
                destination,
                amount,
                reference: reference_text(&d.reference),
            });
        }
        if AccountId::from_tag(tag).destination().is_none() {
            return refused(
                RefusalKind::Library,
                "cannot render this account's destination; nothing was reserved",
            );
        }
        // The library's page for a spend laid out and not signed, with the
        // store's standing divergences in front of it, as every page it
        // writes with the wallet open has them.
        let (page, renders) = text::report(w.wallet.diverged(), Outcome::planned(&plan));
        if !renders {
            return refused(RefusalKind::Library, page);
        }
        let plan_id = PlanId(self.next_plan);
        self.next_plan += 1;
        let view = PlanView {
            plan: plan_id,
            from: AccountId::from_tag(tag),
            destinations: shown,
            send_total: plan.send_total(),
            fee_total: plan.fee_total(),
            change_total: plan.change_total(),
            balance: plan.balance(),
            blk_to_live: plan.blk_to_live(),
            empties_account: plan.change_total() == 0,
            text: page,
        };
        self.plan = Some(PendingPlan {
            id: plan_id,
            from: tag,
            plan,
        });
        Reply::Planned(view)
    }

    fn confirm_send(&mut self, id: RequestId, plan_id: PlanId) -> Reply {
        if let Some(reply) = self.needs_wallet() {
            self.plan = None;
            return reply;
        }
        let Some(pending) = self.plan.take() else {
            return refused(
                RefusalKind::NoSuchPlan,
                "no spend is waiting to be confirmed; plan it again",
            );
        };
        if pending.id != plan_id {
            return refused(
                RefusalKind::NoSuchPlan,
                "that is not the spend waiting to be confirmed; plan it again",
            );
        }
        self.busy(id, Activity::AskingNode);
        let Session::Wallet(w) = &mut self.session else {
            return Self::not_open();
        };
        let tag = pending.from;
        let plan = pending.plan;
        let master = w.master.as_ref();
        let signed = match cli::key_access(w.wallet.store(), &tag, master) {
            Ok(access) => w.wallet.reserve_and_sign(&plan, access),
            Err(e) => Err(e),
        };
        let signed = match signed {
            Ok(s) => s,
            Err(e) => return Reply::Refused(library(e)),
        };
        let wire = signed.wire();
        let submitted = w.wallet.submit(&signed);
        let tx_id = submitted.as_ref().ok().map(|id| hex(&id.0));
        let shipped = Shipped {
            source: tag,
            destinations: plan.dsts().to_vec(),
            blk_to_live: plan.blk_to_live(),
            wire: wire.clone(),
            submitted: submitted.clone(),
        };
        let page = text::page(
            w.wallet.diverged(),
            Outcome::Sent {
                shipped,
                send_total: plan.send_total(),
                fee_total: plan.fee_total(),
                change_total: plan.change_total(),
                upgraded: w.wallet.store().upgraded_from(),
            },
        );
        self.refresh_one(&tag);
        let Some(view) = self.view() else {
            return Self::not_unlocked();
        };
        Reply::Sent(SentView {
            from: AccountId::from_tag(tag),
            submitted: submitted.is_ok(),
            tx_id,
            artifact_hex: hex(&wire),
            text: page,
            view,
        })
    }

    /// Read one account's status again after an operation on it.
    fn refresh_one(&mut self, tag: &Tag) {
        let Session::Wallet(w) = &mut self.session else {
            return;
        };
        let reconciled_at_open = w.wallet.accounts().iter().any(|(t, _)| t == tag);
        let read = match recon::access_for(w.wallet.store(), tag, w.master.as_ref()) {
            Ok(access) => w.wallet.status(tag, &access),
            Err(d) => Err(d),
        };
        let state = match read {
            Ok(status) => AccountState::from_status(&status),
            Err(d) => AccountState::from_divergence(&d),
        };
        let spendable = reconciled_at_open && matches!(state, AccountState::InSync { .. });
        let index = w
            .wallet
            .store()
            .view(tag)
            .ok()
            .flatten()
            .map(|v| v.wots_index.get());
        update_row(&mut w.rows, tag, state, spendable, index);
    }

    /// The refusal for a command that needs the wallet open, when it is not.
    fn needs_wallet(&self) -> Option<Reply> {
        match self.session {
            Session::Wallet(_) => None,
            Session::Locked => Some(Self::not_unlocked()),
            Session::Store(_) => Some(Self::not_open()),
        }
    }

    fn settle(&mut self, id: RequestId, account: AccountId) -> Reply {
        let tag = account.tag();
        if let Some(reply) = self.needs_wallet() {
            return reply;
        }
        self.busy(id, Activity::AskingNode);
        let Session::Wallet(w) = &mut self.session else {
            return Self::not_open();
        };
        // Not refused on what `Wallet::open` found: `settle_if_landed`
        // reconciles the account afresh and refuses on that, so an account
        // set aside when the wallet opened (an outstanding spend the node
        // could not see then) settles once the node shows it landed.
        let settled = match cli::key_access(w.wallet.store(), &tag, w.master.as_ref()) {
            Ok(access) => w.wallet.settle_if_landed(&tag, &access),
            Err(e) => Err(e),
        };
        let page = match settled {
            Ok(settlement) => text::page(
                w.wallet.diverged(),
                Outcome::Settled {
                    settlement,
                    upgraded: w.wallet.store().upgraded_from(),
                },
            ),
            Err(e) => return Reply::Refused(library(e)),
        };
        self.refresh_one(&tag);
        match self.view() {
            Some(view) => Reply::Settled { text: page, view },
            None => Self::not_unlocked(),
        }
    }

    fn resign(&mut self, id: RequestId, request: &SpendRequest) -> Reply {
        if let Some(reply) = self.needs_wallet() {
            return reply;
        }
        let spend = match spend::check(request) {
            Ok(c) => c,
            Err(e) => return refused(RefusalKind::Spend(e.clone()), e.to_string()),
        };
        self.busy(id, Activity::AskingNode);
        let Session::Wallet(w) = &mut self.session else {
            return Self::not_open();
        };
        let tag = spend.tag;
        if let Some(d) = w.wallet.divergence_for(&tag) {
            return Self::diverged_refusal(d);
        }
        let master = w.master.as_ref();
        let access = match cli::key_access(w.wallet.store(), &tag, master) {
            Ok(a) => a,
            Err(e) => return Reply::Refused(library(e)),
        };
        // "Everything" resolves against the balance now; if it moved, the
        // digest differs and the library refuses it as another spend
        // (`cli::resign_destinations`).
        let dsts = if spend.spends_everything() {
            match w
                .wallet
                .client()
                .resolve_tag(&tag)
                .and_then(|entry| cli::spend_all_amount(entry.balance, spend.fee_total))
            {
                Ok(amount) => destinations(&spend, amount),
                Err(e) => return Reply::Refused(library(e)),
            }
        } else {
            destinations(&spend, 0)
        };
        let mut listed = dsts.clone();
        listed.sort_by_key(Destination::mdst_image);
        let resigned =
            w.wallet
                .resign_pending(&tag, &access, dsts, spend.fee_total, spend.blk_to_live);
        // The command line's decision (`cli::resign_outcome`): a
        // reproduction is written to the socket unless the source will not
        // render, when it goes out to the person unwritten. The bytes go out
        // either way: they may be the only rendering of the only bytes that
        // can move those funds.
        let outcome = cli::resign_outcome(&w.wallet, &tag, listed, spend.blk_to_live, resigned);
        let reproduced = match &outcome {
            Outcome::Resigned { shipped } => {
                Some((shipped.wire.clone(), Some(shipped.submitted.clone())))
            }
            Outcome::ReproducedButUnrenderable { wire, .. } => Some((wire.clone(), None)),
            _ => None,
        };
        let Some((wire, submitted)) = reproduced else {
            return match outcome {
                Outcome::Failed(e) => Reply::Refused(library(e)),
                other => refused(RefusalKind::Library, text::page(w.wallet.diverged(), other)),
            };
        };
        let page = text::page(w.wallet.diverged(), outcome);
        self.refresh_one(&tag);
        let Some(view) = self.view() else {
            return Self::not_unlocked();
        };
        let tx_id = submitted
            .as_ref()
            .and_then(|s| s.as_ref().ok())
            .map(|id| hex(&id.0));
        Reply::Sent(SentView {
            from: AccountId::from_tag(tag),
            submitted: matches!(submitted, Some(Ok(_))),
            tx_id,
            artifact_hex: hex(&wire),
            text: page,
            view,
        })
    }

    fn submit_artifact(&self, id: RequestId, artifact_hex: &str) -> Reply {
        let (_, client) = match self.client() {
            Ok(c) => c,
            Err(r) => return Reply::Refused(r),
        };
        self.busy(id, Activity::AskingNode);
        let report = mochimo_crypto::cli::run_submit(&client, artifact_hex.trim());
        Reply::Submitted {
            accepted: report.code == Code::Ok,
            text: report.text,
        }
    }

    // ------------------------------------------------------------- pre-gate

    fn status(&mut self, id: RequestId, account: AccountId, scan_to: Option<u32>) -> Reply {
        let tag = account.tag();
        if let Some(m) = scan_to
            && let Err(r) = within_key_range("a scan to key index", m)
        {
            return Reply::Refused(r);
        }
        let scope = reconcile::scope_to(scan_to);
        let asked = self.stop_asked(id);
        let fresh;
        let (store, client, master) = match &self.session {
            Session::Locked => return Self::not_unlocked(),
            Session::Store(s) => {
                let client = match &s.client {
                    Some((_, c)) => c,
                    None => match self.client() {
                        Ok((_, c)) => {
                            fresh = c;
                            &fresh
                        }
                        Err(r) => return Reply::Refused(r),
                    },
                };
                (&s.store, client, s.master.as_ref())
            }
            Session::Wallet(w) => (w.wallet.store(), w.wallet.client(), w.master.as_ref()),
        };
        self.busy(id, Activity::AskingNode);
        // The command line's `status` (`cli::reconcile::account_status`):
        // the same two public calls, with a cancel the person can raise.
        let result = match recon::access_for(store, &tag, master) {
            Ok(access) => recon::reconcile_account_with(
                store,
                client,
                &tag,
                &access,
                &scope,
                &Cancel::when(&asked),
            ),
            Err(d) => Err(d),
        };
        // A walk told to stop comes back as a divergence whose search was
        // cut short; it says nothing about the account, so it is not shown
        // as a report or applied to the row.
        if asked() {
            return Reply::Refused(library(Error::Cancelled));
        }
        let state = match &result {
            Ok(status) => AccountState::from_status(status),
            Err(d) => AccountState::from_divergence(d),
        };
        // The command line's classification (`cli::status_outcome`): a
        // report about the account is an answer, not a refusal; a tag the
        // store does not hold is refused.
        let outcome = cli::status_outcome(&tag, result);
        if matches!(outcome, Outcome::NoSuchAccount { .. }) {
            return refused(RefusalKind::Library, text::page(&[], outcome));
        }
        let spendable = match &mut self.session {
            Session::Store(s) => {
                update_row(&mut s.rows, &tag, state.clone(), false, None);
                false
            }
            Session::Wallet(w) => {
                let spendable = w.wallet.accounts().iter().any(|(t, _)| *t == tag)
                    && matches!(state, AccountState::InSync { .. });
                update_row(&mut w.rows, &tag, state.clone(), spendable, None);
                spendable
            }
            Session::Locked => false,
        };
        Reply::Status {
            account,
            state,
            spendable,
            text: text::page(&[], outcome),
        }
    }

    /// Take the open store out of the session for an operation that writes
    /// it, with the node it needs. The session is left Locked; the caller
    /// reopens it.
    fn take_store(&mut self) -> Result<TakenStore<C::Transport>, Refusal> {
        if matches!(self.session, Session::Locked) {
            return Err(Self::lost());
        }
        let (_, client) = self.client()?;
        self.plan = None;
        match core::mem::replace(&mut self.session, Session::Locked) {
            Session::Locked => Err(Self::lost()),
            Session::Store(s) => Ok(TakenStore {
                dir: s.dir,
                store: s.store,
                master: s.master,
                client,
            }),
            Session::Wallet(w) => {
                let (store, _old) = w.wallet.into_parts();
                Ok(TakenStore {
                    dir: w.dir,
                    store,
                    master: w.master,
                    client,
                })
            }
        }
    }

    fn reconcile(&mut self, id: RequestId, account: AccountId, advance_to: u32) -> Reply {
        let tag = account.tag();
        if let Err(r) = within_key_range("an advance to key index", advance_to) {
            return Reply::Refused(r);
        }
        let TakenStore {
            dir,
            mut store,
            master,
            client,
        } = match self.take_store() {
            Ok(parts) => parts,
            Err(r) => return Reply::Refused(r),
        };
        self.busy(id, Activity::AskingNode);
        let asked = self.stop_asked(id);
        let reviewed = reconcile::advance_acknowledged_with_progress(
            &mut store,
            &client,
            &tag,
            master.as_ref(),
            advance_to,
            &Cancel::when(&asked),
            &mut |counted| self.progress(id, counted),
        );
        // The command line's `cmd_reconcile`: an error is `Outcome::Failed`.
        // A cancel wrote nothing and is no finding about the account, so it
        // gets no page of the library's.
        let (advanced_to, page, ok) = match reviewed {
            Ok(reviewed) => {
                let advanced = match &reviewed.outcome {
                    reconcile::Outcome::Advanced { index } => Some(*index),
                    _ => None,
                };
                let upgraded = store.upgraded_from();
                let (page, ok) = text::report(
                    &[],
                    Outcome::Reconciled {
                        tag,
                        advance_to,
                        reviewed,
                        upgraded,
                    },
                );
                (advanced, page, ok)
            }
            Err(Unfinished::Refused(e)) => {
                let (page, ok) = text::report(&[], Outcome::Failed(e));
                (None, page, ok)
            }
            Err(Unfinished::Cancelled) => (None, ADVANCE_CANCELLED.to_owned(), false),
        };
        let opened = self.reopen(id, dir, store, master);
        Reply::Reconciled {
            ok,
            advanced_to,
            text: page,
            opened,
        }
    }

    fn restore(&mut self, id: RequestId, account_index: u32, scan_to: Option<u32>) -> Reply {
        if let Some(m) = scan_to
            && let Err(r) = within_key_range("a scan to key index", m)
        {
            return Reply::Refused(r);
        }
        let TakenStore {
            dir,
            mut store,
            master,
            client,
        } = match self.take_store() {
            Ok(parts) => parts,
            Err(r) => return Reply::Refused(r),
        };
        self.busy(id, Activity::AskingNode);
        let asked = self.stop_asked(id);
        let (page, ok) = match master.as_ref() {
            None => text::report(&[], Outcome::RestoreNeedsMaster),
            Some(m) => {
                let restored = restore::restore_account_with_progress(
                    &mut store,
                    &client,
                    m,
                    account_index,
                    scan_to,
                    &Cancel::when(&asked),
                    &mut |counted| self.progress(id, counted),
                );
                match restored {
                    Ok(r) => text::report(
                        &[],
                        Outcome::Restored {
                            account: account_index,
                            found: r.found,
                            held_at: r.held_at,
                            upgraded: store.upgraded_from(),
                        },
                    ),
                    Err(Unfinished::Refused(failure)) => text::report(
                        &[],
                        Outcome::RestoreRefused {
                            account: account_index,
                            failure,
                        },
                    ),
                    // Wrote nothing, and is no finding about the account.
                    Err(Unfinished::Cancelled) => (RESTORE_CANCELLED.to_owned(), false),
                }
            }
        };
        let opened = self.reopen(id, dir, store, master);
        Reply::Restored {
            ok,
            text: page,
            opened,
        }
    }

    fn discover(&mut self, id: RequestId, to: u32) -> Reply {
        if let Err(r) = discover_bound(to) {
            return Reply::Refused(r);
        }
        let fresh;
        let (store, master, client) = match &self.session {
            Session::Locked => return Self::not_unlocked(),
            Session::Store(s) => {
                let client = match &s.client {
                    Some((_, c)) => c,
                    None => match self.client() {
                        Ok((_, c)) => {
                            fresh = c;
                            &fresh
                        }
                        Err(r) => return Reply::Refused(r),
                    },
                };
                (&s.store, s.master.as_ref(), client)
            }
            Session::Wallet(w) => (w.wallet.store(), w.master.as_ref(), w.wallet.client()),
        };
        let Some(master) = master else {
            return refused(
                RefusalKind::Library,
                text::page(&[], Outcome::DiscoverNeedsMaster),
            );
        };
        self.busy(id, Activity::AskingNode);
        let asked = self.stop_asked(id);
        let swept = discover::sweep_with_progress(
            store,
            client,
            master,
            to,
            &Cancel::when(&asked),
            &mut |counted| self.progress(id, counted),
        );
        match swept {
            Ok(sweep) => {
                let accounts = sweep
                    .sightings
                    .iter()
                    .map(|s| Discovered {
                        account_index: s.account,
                        id: AccountId::from_tag(s.tag),
                        ledger_balance: s.entry.map(|e| e.balance),
                        held: s.held.is_some(),
                    })
                    .collect();
                Reply::Discovered {
                    text: text::page(&[], Outcome::Discovered { sweep }),
                    accounts,
                }
            }
            Err(Unfinished::Refused(discover::SweepFailure::ChainUnreachable {
                account,
                searched,
                cause,
            })) => refused(
                RefusalKind::Library,
                text::page(
                    &[],
                    Outcome::SweepStopped {
                        account,
                        searched,
                        to,
                        cause,
                    },
                ),
            ),
            // A sweep that stopped short is not reported: a short extent
            // read as a whole one is an absence by omission.
            Err(Unfinished::Cancelled) => Reply::Refused(library(Error::Cancelled)),
        }
    }

    /// Ask the chosen node with `ask`: through the open wallet's own client
    /// when the wallet was opened against that node, or through a new one.
    fn ask_node<R>(&self, ask: impl FnOnce(&MeshClient<C::Transport>) -> R) -> Result<R, Refusal> {
        match &self.session {
            Session::Wallet(w) if self.node.as_deref() == Some(w.node.as_str()) => {
                Ok(ask(w.wallet.client()))
            }
            _ => self.client().map(|(_, client)| ask(&client)),
        }
    }

    /// `/network/status`, read whole: the tip, when it was solved, and the
    /// middleware's sync state.
    fn network_status(&self, id: RequestId) -> Reply {
        let read = self.ask_node(|client| {
            self.busy(id, Activity::AskingNode);
            client.network_status_full()
        });
        match read {
            Ok(Ok(status)) => Reply::Network {
                tip_index: status.tip.index,
                tip_hash: hex(&status.tip.hash),
                tip_time_ms: status.tip_timestamp_ms,
                sync: status.sync.map(|s| SyncState {
                    stage: explorer::shown(&s.stage),
                    synced: s.synced,
                }),
            },
            Ok(Err(e)) => Reply::Refused(library(e)),
            Err(r) => Reply::Refused(r),
        }
    }

    /// `/network/list`: the networks the node serves, by name.
    fn networks(&self, id: RequestId) -> Reply {
        let read = self.ask_node(|client| {
            self.busy(id, Activity::AskingNode);
            client.networks()
        });
        match read {
            Ok(Ok(list)) => Reply::Networks(
                list.iter()
                    .map(|n| NetworkName {
                        blockchain: explorer::shown(&n.blockchain),
                        network: explorer::shown(&n.network),
                    })
                    .collect(),
            ),
            Ok(Err(e)) => Reply::Refused(library(e)),
            Err(r) => Reply::Refused(r),
        }
    }

    /// The command line's `blocks`, for the wallet's network card.
    fn blocks(&self, id: RequestId) -> Reply {
        let outcome = match self.ask_node(|client| {
            self.busy(id, Activity::ReadingIndex);
            cli::cmd_blocks(client, explorer::CARD_BLOCKS)
        }) {
            Ok(outcome) => outcome,
            Err(r) => return Reply::Refused(r),
        };
        let read = match &outcome {
            Outcome::Blocks { tip, rows, .. } => {
                Some((tip.index, rows.iter().map(BlockSummary::of).collect()))
            }
            _ => None,
        };
        let text = text::page(&[], outcome);
        // Blocks are the node's own, not its index: no refusal here says
        // anything about the index.
        Reply::Blocks(match read {
            Some((tip, blocks)) => Ok(BlocksView { tip, blocks, text }),
            None => Err(ExplorerRefusal { index: None, text }),
        })
    }

    /// The command line's `mempool` with none of the queue read whole, for
    /// the wallet's network card.
    fn mempool(&self, id: RequestId) -> Reply {
        let outcome = match self.ask_node(|client| {
            self.busy(id, Activity::AskingNode);
            cli::cmd_mempool(client, 0)
        }) {
            Ok(outcome) => outcome,
            Err(r) => return Reply::Refused(r),
        };
        let read = match &outcome {
            Outcome::Mempool { total, .. } => Some(*total),
            _ => None,
        };
        let text = text::page(&[], outcome);
        // The queue is the node's own, not its index: no refusal here says
        // anything about the index, though the middleware answers its code 2
        // for a queue it could not read as well.
        Reply::Mempool(match read {
            Some(waiting) => Ok(MempoolView { waiting, text }),
            None => Err(ExplorerRefusal { index: None, text }),
        })
    }

    /// The command line's `recent-transactions` for every account in the
    /// store's order, the newest page of each; or, given `pages`, the page
    /// at each offset given, for the accounts named that the store holds.
    fn activity(&self, id: RequestId, pages: Option<&[(AccountId, u64)]>) -> Reply {
        let fresh;
        let (accounts, client) = match &self.session {
            Session::Locked => return Self::not_unlocked(),
            Session::Store(s) => {
                let client = match &s.client {
                    Some((_, c)) => c,
                    None => match self.client() {
                        Ok((_, c)) => {
                            fresh = c;
                            &fresh
                        }
                        Err(r) => return Reply::Refused(r),
                    },
                };
                (&s.rows, client)
            }
            Session::Wallet(w) => (&w.rows, w.wallet.client()),
        };
        let wanted: Vec<(AccountId, u64)> = accounts
            .iter()
            .filter_map(|row| match pages {
                None => Some((row.id, 0)),
                Some(pages) => pages
                    .iter()
                    .find(|(account, _)| *account == row.id)
                    .map(|&(_, from)| (row.id, from)),
            })
            .collect();
        let asked = self.stop_asked(id);
        self.busy(id, Activity::ReadingIndex);
        let count = u32::try_from(wanted.len()).unwrap_or(u32::MAX);
        let mut out = Vec::with_capacity(wanted.len());
        for (n, &(account, from)) in wanted.iter().enumerate() {
            // Between two requests: a history cut short is not shown as
            // a whole one.
            if asked() {
                return Reply::Refused(library(Error::Cancelled));
            }
            self.emit(Event::Progress {
                id,
                progress: Progress {
                    account: u32::try_from(n).unwrap_or(u32::MAX),
                    accounts: count,
                    position: 0,
                    ceiling: 0,
                },
            });
            let outcome = cli::cmd_recent_transactions_from(
                client,
                &account.tag(),
                explorer::HISTORY_ROWS,
                from,
            );
            let read = match &outcome {
                Outcome::RecentTransactions { page, from, .. } => {
                    Ok(AccountHistory::of(account, page, *from, String::new()))
                }
                Outcome::ExplorerFailed { cause } => Err(explorer::index_state(cause)),
                _ => Err(None),
            };
            let text = text::page(&[], outcome);
            match read {
                Ok(history) => out.push(AccountHistory { text, ..history }),
                // Every account is read from the same index: one refusal
                // is the answer for all of them.
                Err(index) => return Reply::Activity(Err(ExplorerRefusal { index, text })),
            }
        }
        if asked() {
            return Reply::Refused(library(Error::Cancelled));
        }
        Reply::Activity(Ok(out))
    }

    /// [`Command::Status`] for every account, applied only once all have
    /// answered.
    fn review(&mut self, id: RequestId) -> Reply {
        let asked = self.stop_asked(id);
        let scope = reconcile::scope_to(None);
        let fresh;
        let (store, client, master, tags) = match &self.session {
            Session::Locked => return Self::not_unlocked(),
            Session::Store(s) => {
                let client = match &s.client {
                    Some((_, c)) => c,
                    None => match self.client() {
                        Ok((_, c)) => {
                            fresh = c;
                            &fresh
                        }
                        Err(r) => return Reply::Refused(r),
                    },
                };
                (
                    &s.store,
                    client,
                    s.master.as_ref(),
                    s.rows.iter().map(|r| r.id.tag()).collect::<Vec<_>>(),
                )
            }
            Session::Wallet(w) => (
                w.wallet.store(),
                w.wallet.client(),
                w.master.as_ref(),
                w.rows.iter().map(|r| r.id.tag()).collect(),
            ),
        };
        self.busy(id, Activity::AskingNode);
        let count = u32::try_from(tags.len()).unwrap_or(u32::MAX);
        let mut found = Vec::with_capacity(tags.len());
        for (n, tag) in tags.iter().enumerate() {
            self.emit(Event::Progress {
                id,
                progress: Progress {
                    account: u32::try_from(n).unwrap_or(u32::MAX),
                    accounts: count,
                    position: 0,
                    ceiling: 0,
                },
            });
            let result = match recon::access_for(store, tag, master) {
                Ok(access) => recon::reconcile_account_with(
                    store,
                    client,
                    tag,
                    &access,
                    &scope,
                    &Cancel::when(&asked),
                ),
                Err(d) => Err(d),
            };
            // A walk told to stop is no finding about the account, and a
            // review cut short is not shown as a whole one.
            if asked() {
                return Reply::Refused(library(Error::Cancelled));
            }
            let state = match &result {
                Ok(status) => AccountState::from_status(status),
                Err(d) => AccountState::from_divergence(d),
            };
            let text = text::page(&[], cli::status_outcome(tag, result));
            found.push((*tag, state, text));
        }
        let mut reports = Vec::with_capacity(found.len());
        for (tag, state, text) in found {
            let spendable = match &mut self.session {
                Session::Store(s) => {
                    update_row(&mut s.rows, &tag, state.clone(), false, None);
                    false
                }
                Session::Wallet(w) => {
                    let spendable = w.wallet.accounts().iter().any(|(t, _)| *t == tag)
                        && matches!(state, AccountState::InSync { .. });
                    update_row(&mut w.rows, &tag, state.clone(), spendable, None);
                    spendable
                }
                Session::Locked => false,
            };
            reports.push(AccountReport {
                account: AccountId::from_tag(tag),
                state,
                spendable,
                text,
            });
        }
        Reply::Reviewed(reports)
    }
}

/// Write a signed spend's bytes to `path` as hex, the form
/// [`Command::SubmitArtifact`] and the command line's `submit` take, so they
/// are not lost (docs/PLAN.md section 4.9: the retry artifact can be lost,
/// so the app offers to save it as a file). Refuses to overwrite a file
/// that exists.
pub fn save_artifact(path: &Path, artifact_hex: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(artifact_hex.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()
}

/// [`save_artifact`] into `dir` under a name of its own, `stem.hex`, or
/// `stem-2.hex`, `stem-3.hex` and so on when that is taken, so no file is
/// ever overwritten, and answer with the path written (docs/DECISIONS.md
/// D27, item 5). `dir` is made when it is missing.
pub fn save_artifact_in(
    dir: &Path,
    stem: &str,
    artifact_hex: &str,
) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    for n in 1..=999u32 {
        let name = if n == 1 {
            format!("{stem}.hex")
        } else {
            format!("{stem}-{n}.hex")
        };
        let path = dir.join(name);
        match save_artifact(&path, artifact_hex) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!(
            "{stem}.hex and 998 more names after it are already taken in {}",
            dir.display()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The interleaving the predicate's memory is for: the idle period
    /// passes and stops a walk, the person touches the interface, and the
    /// request asks again.
    #[test]
    fn an_idle_stop_holds_for_the_rest_of_the_request() {
        let period = Duration::from_millis(200);
        let person = Arc::new(PersonActivity::new(period));
        let no_cancel = Arc::new(AtomicU64::new(0));
        let request = Arc::new(AtomicBool::new(false));
        let asked = stop_predicate(
            Arc::clone(&no_cancel),
            RequestId(1),
            Some(Arc::clone(&person)),
            Arc::clone(&request),
        );
        assert!(!asked(), "not idle yet");
        thread::sleep(period + Duration::from_millis(50));
        assert!(asked(), "the idle period has passed");
        person.mark();
        assert!(asked(), "a touch after the stop does not undo it");
        // A predicate made later for the same request (the wallet opening
        // after a restore or an advance) is stopped too.
        let reopen = stop_predicate(
            Arc::clone(&no_cancel),
            RequestId(1),
            Some(Arc::clone(&person)),
            Arc::clone(&request),
        );
        assert!(reopen(), "the reopening hears the same stop");
        // The next request starts afresh.
        let next = stop_predicate(
            no_cancel,
            RequestId(2),
            Some(person),
            Arc::new(AtomicBool::new(false)),
        );
        assert!(!next(), "the person is back; the next request runs");
    }

    #[test]
    fn a_cancel_stops_the_requests_sent_before_it_only() {
        let cancel = Arc::new(AtomicU64::new(3));
        let fresh = || Arc::new(AtomicBool::new(false));
        assert!(stop_predicate(
            Arc::clone(&cancel),
            RequestId(3),
            None,
            fresh()
        )());
        assert!(!stop_predicate(cancel, RequestId(4), None, fresh())());
    }

    #[test]
    fn reference_text_stops_at_the_first_nul() {
        let mut field = [0u8; 16];
        field[..3].copy_from_slice(b"ABC");
        assert_eq!(reference_text(&field), "ABC");
        assert_eq!(reference_text(&[0u8; 16]), "");
    }

    #[test]
    fn save_artifact_never_overwrites() {
        let path = std::env::temp_dir().join(format!("tawara-artifact-{}.hex", std::process::id()));
        let _ = std::fs::remove_file(&path);
        save_artifact(&path, "abcd").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "abcd\n");
        assert_eq!(
            save_artifact(&path, "ef").unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_artifact_saved_in_a_folder_takes_a_name_of_its_own() {
        let dir = std::env::temp_dir().join(format!("tawara-artifacts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = save_artifact_in(&dir, "spend", "abcd").unwrap();
        assert_eq!(first, dir.join("spend.hex"));
        let second = save_artifact_in(&dir, "spend", "ef").unwrap();
        assert_eq!(second, dir.join("spend-2.hex"));
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "abcd\n");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "ef\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
