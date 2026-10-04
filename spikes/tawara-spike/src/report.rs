//! One line per fact, on stderr, for CI to grep:
//! `SPIKE PASS|FAIL <name>: <detail>`, `SPIKE INFO <fact>` and, at the end of
//! the self-test, `SPIKE DONE pass=N fail=M`. On Android stderr goes to
//! logcat under the tag `RustStdoutStderr`; on the iOS simulator, to the file
//! `simctl launch --stderr` names. Never a password or other secret: lengths
//! only.

use std::sync::atomic::{AtomicU32, Ordering};

static PASSED: AtomicU32 = AtomicU32::new(0);
static FAILED: AtomicU32 = AtomicU32::new(0);

pub fn note(line: impl AsRef<str>) {
    eprintln!("SPIKE INFO {}", line.as_ref());
}

pub fn check(name: &str, ok: bool, detail: impl std::fmt::Display) -> bool {
    let counter = if ok { &PASSED } else { &FAILED };
    counter.fetch_add(1, Ordering::Relaxed);
    eprintln!(
        "SPIKE {} {name}: {detail}",
        if ok { "PASS" } else { "FAIL" }
    );
    ok
}

pub fn done() {
    eprintln!(
        "SPIKE DONE pass={} fail={}",
        PASSED.load(Ordering::Relaxed),
        FAILED.load(Ordering::Relaxed)
    );
}

pub fn exit_code() -> i32 {
    i32::from(FAILED.load(Ordering::Relaxed) > 0)
}

/// Panics become a FAIL line too, so a CI grep sees them on every platform.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        FAILED.fetch_add(1, Ordering::Relaxed);
        eprintln!("SPIKE FAIL panic: {info}");
        default(info);
    }));
}
