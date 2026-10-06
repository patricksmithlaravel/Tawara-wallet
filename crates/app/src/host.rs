//! What a mobile shell (`crates/mobile`) hands the application, and what it
//! tells it (docs/DECISIONS.md D32). The desktop needs none of this.
//!
//! A shell gives the app-private directory, where the store and the
//! preferences live, and the platform's clipboard, which iced's does not
//! reach on Android or iOS (docs/spikes/P1-REPORT.md), and what Android's
//! Back does. It calls [`left_foreground`] when the system takes the
//! application off the screen.

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use tawara_wallet_core::Locking;

/// What a mobile shell supplies.
#[derive(Clone, Debug)]
pub struct Host {
    /// The app-private directory: Android's `no_backup/`, iOS's
    /// Application Support (docs/DECISIONS.md D21, D32 item 7). The store
    /// goes in its own folder inside it, and the preferences beside that.
    pub private_dir: PathBuf,
    /// Puts text on the platform's clipboard.
    pub copy: fn(&str),
    /// Android's Back: the application goes to the background, as Android
    /// 12 and later do themselves for a launcher's first activity. It is
    /// never finished, since winit cannot start it again in the same
    /// process (docs/spikes/P1-REPORT.md, A18).
    pub back: fn(),
}

/// The shell's clipboard and Back, once a hosted application has started.
static COPY: OnceLock<fn(&str)> = OnceLock::new();
static BACK: OnceLock<fn()> = OnceLock::new();

/// What [`left_foreground`] does: the running worker's lock.
type Lock = Box<dyn Fn() -> Leaving + Send>;

/// A move to the background on its way (docs/DECISIONS.md D32 item 6). The
/// worker locks after the command it is running, which a node request or a
/// spend being submitted can hold; a shell whose process the system may
/// suspend keeps it running until [`Leaving::wait`] says the lock is done.
#[derive(Debug)]
#[must_use = "a shell that may be suspended waits on it"]
pub struct Leaving {
    locking: Option<Locking>,
    /// The interface's wipe, and the move it must have acted on.
    wiped: Option<(Arc<Wiped>, u64)>,
}

impl Leaving {
    /// Wait up to `within`, in all, for the lock and for the interface's
    /// wipe of what it holds (Tawara-mobile#1's second review). `true` once
    /// both are done, or when there was nothing to do; `false` when either
    /// is still waited on after `within`.
    pub fn wait(&self, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        self.locking
            .as_ref()
            .is_none_or(|locking| locking.wait(within))
            && self.wiped.as_ref().is_none_or(|(wiped, wanted)| {
                wiped.wait_for(*wanted, deadline.saturating_duration_since(Instant::now()))
            })
    }

    pub(crate) fn of(locking: Option<Locking>, wiped: Option<(Arc<Wiped>, u64)>) -> Leaving {
        Leaving { locking, wiped }
    }
}

/// How many moves to the background the interface has wiped for: the
/// password, phrase and words typed, and a recovery phrase shown. The
/// application's thread raises it, and a shell's thread waits on it.
#[derive(Debug, Default)]
pub(crate) struct Wiped {
    moves: Mutex<u64>,
    raised: Condvar,
}

impl Wiped {
    /// The interface has wiped for `moves` moves.
    pub(crate) fn set(&self, moves: u64) {
        let mut done = self.moves.lock().unwrap_or_else(PoisonError::into_inner);
        *done = (*done).max(moves);
        self.raised.notify_all();
    }

    /// Wait up to `within` for the wipe of move `wanted`.
    fn wait_for(&self, wanted: u64, within: Duration) -> bool {
        let done = self.moves.lock().unwrap_or_else(PoisonError::into_inner);
        let (done, _) = self
            .raised
            .wait_timeout_while(done, within, |done| *done < wanted)
            .unwrap_or_else(PoisonError::into_inner);
        *done >= wanted
    }
}

static LOCK: Mutex<Option<Lock>> = Mutex::new(None);

/// The application has left the foreground: stop what can be stopped and
/// lock, at once, on the calling thread (docs/PLAN.md section 4.1). It does
/// not wait for iced, which may not run again before the system suspends
/// the process, nor for the lock itself: the returned [`Leaving`] says when
/// that is done. Nothing happens before the worker has started.
pub fn left_foreground() -> Leaving {
    match LOCK.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
        Some(lock) => lock(),
        None => Leaving::of(None, None),
    }
}

/// `lock` is what [`left_foreground`] does from now on.
pub(crate) fn on_leaving(lock: impl Fn() -> Leaving + Send + 'static) {
    *LOCK.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(lock));
}

pub(crate) fn set(host: &Host) {
    let _ = COPY.set(host.copy);
    let _ = BACK.set(host.back);
}

/// The system's Back, where a shell has one.
pub(crate) fn back() {
    if let Some(back) = BACK.get() {
        back();
    }
}

/// Puts `text` on the shell's clipboard. `false` when there is no shell's,
/// and iced's is the one to use.
pub(crate) fn copy(text: &str) -> bool {
    COPY.get().map(|copy| copy(text)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn leaving_the_foreground_locks_with_whatever_was_registered_last() {
        // Nothing registered: nothing to do, no panic, and nothing to wait
        // for.
        assert!(left_foreground().wait(Duration::ZERO));
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&first);
        on_leaving(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Leaving::of(None, None)
        });
        let _ = left_foreground();
        let counter = Arc::clone(&second);
        on_leaving(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Leaving::of(None, None)
        });
        let _ = left_foreground();
        let _ = left_foreground();
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(second.load(Ordering::SeqCst), 2, "the worker started last");
    }
}
