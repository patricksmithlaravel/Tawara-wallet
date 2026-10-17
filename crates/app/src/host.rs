//! What a mobile shell (`crates/mobile`) hands the application, and what it
//! tells it (docs/DECISIONS.md D32). The desktop needs none of this.
//!
//! A shell gives the app-private directory, where the store and the
//! preferences live, the platform's clipboard, its share sheet, and what
//! Android's Back does.
//! Copy goes through the shell's own write, not iced's: iced's reaches the
//! phones at the pin, but it sets none of the protection that
//! docs/DECISIONS.md D32 item 9 (d) puts in the shell's write. It calls
//! [`left_foreground`] when the system takes the application off the screen.
//! On iOS it also calls [`screenshot_taken`] and [`capture_changed`], which
//! the recovery phrase's screen reads (docs/DECISIONS.md D44 item 3).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::Duration;

/// What a mobile shell supplies.
#[derive(Clone, Debug)]
pub struct Host {
    /// The app-private directory: Android's `no_backup/`, iOS's
    /// Application Support (docs/DECISIONS.md D21, D32 item 7). The store
    /// goes in its own folder inside it, and the preferences beside that.
    pub private_dir: PathBuf,
    /// Puts text on the platform's clipboard.
    pub copy: fn(&str),
    /// Offers `text` to the system's share sheet, with `name` as the file's
    /// name or the subject (docs/DECISIONS.md D32 item 10 (i)). A phone has
    /// no Downloads folder for Save artifact to write to, and the sheet is
    /// how it hands text to Files, a note or a message. iOS presents
    /// `UIActivityViewController`; Android sends `ACTION_SEND` as `text/plain`
    /// through `Intent.createChooser`. `Ok` says only that the platform was
    /// asked to show the sheet, not that it was shown. The call returns before
    /// the platform presents the sheet (UIKit does so 2.4 s after the call on a
    /// first press and 0.6 s on a second, on the simulator), so a refusal the
    /// platform makes then is not reported. The application cannot learn what
    /// was done with the sheet, so the bytes still count as not saved. `Err`
    /// says why the platform was not asked (docs/DECISIONS.md D32 item 10 (i),
    /// as amended on 2026-10-13).
    pub share: fn(name: &str, text: &str) -> Result<(), String>,
    /// Android's Back: the application goes to the background, as Android
    /// 12 and later do themselves for a launcher's first activity. It is
    /// never finished. When Android destroys the Activity anyway, the
    /// process still ends after Destroy, because a restart in the same
    /// process would need the Activity's handles, the host statics and
    /// FLAG_SECURE rebuilt (docs/DECISIONS.md D32 item 9 (d)). The fork's
    /// winit can build its event loop again, so that choice is Tawara's,
    /// not winit's.
    pub back: fn(),
    /// Whether the screen is being recorded or mirrored, as the system reports
    /// it (`UIScreen.isCaptured` on iOS), read once when the application
    /// starts. Later changes come through [`capture_changed`]. `None` where a
    /// shell supplies none, as Android does, and keeps FLAG_SECURE
    /// (docs/DECISIONS.md D32 item 4, D44 items 3 and 4).
    pub captured: Option<fn() -> bool>,
}

/// What a shell's share sheet is: [`Host::share`].
type Sheet = fn(&str, &str) -> Result<(), String>;

/// The shell's clipboard, share sheet and Back, once a hosted application
/// has started.
static COPY: OnceLock<fn(&str)> = OnceLock::new();
static SHARE: OnceLock<Sheet> = OnceLock::new();
static BACK: OnceLock<fn()> = OnceLock::new();

/// What [`left_foreground`] does: the running worker's lock.
type Lock = Box<dyn Fn() -> Leaving + Send>;

/// A move to the background on its way (docs/DECISIONS.md D32 item 6). A
/// shell whose process the system may suspend keeps it running until
/// [`Leaving::wait`] says the interface has done with the move: taken the
/// worker's `Backgrounded` for it, which comes after the worker has locked
/// and after every answer that came before (a recovery phrase among them,
/// dropped by the lock that follows it); or, with no worker, wiped what is
/// typed. Either way the fields' own wipe on the interface's next message
/// comes first (Tawara-mobile#1's reviews).
#[derive(Debug)]
#[must_use = "a shell that may be suspended waits on it"]
pub struct Leaving(Option<(Arc<Wiped>, Done)>);

/// What the interface must have done for a [`Leaving`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Done {
    /// Taken the worker's `Backgrounded` with this number.
    Taken(u64),
    /// With no worker: wiped for this many moves.
    Wiped(u64),
}

impl Leaving {
    /// Wait up to `within` for the interface to have done with the move.
    /// `true` once it has, or when there was nothing to do; `false` when it
    /// is still waited on after `within`.
    pub fn wait(&self, within: Duration) -> bool {
        self.0
            .as_ref()
            .is_none_or(|(wiped, done)| wiped.wait_for(*done, within))
    }

    pub(crate) fn of(wiped: Option<(Arc<Wiped>, Done)>) -> Leaving {
        Leaving(wiped)
    }
}

/// What the interface has done for the moves to the background: the moves
/// it has wiped what is typed for, and the last of the worker's
/// `Backgrounded` it has taken. The application's thread raises them, and a
/// shell's thread waits on them.
#[derive(Debug, Default)]
pub(crate) struct Wiped {
    state: Mutex<(u64, u64)>,
    raised: Condvar,
}

impl Wiped {
    /// The interface has wiped for `moves` moves.
    pub(crate) fn wiped(&self, moves: u64) {
        self.raise(|state| state.0 = state.0.max(moves));
    }

    /// The interface has taken the worker's `Backgrounded` numbered `seq`,
    /// and so every event before it. `u64::MAX` when the worker has stopped:
    /// nothing more comes from it.
    pub(crate) fn taken(&self, seq: u64) {
        self.raise(|state| state.1 = state.1.max(seq));
    }

    fn raise(&self, change: impl FnOnce(&mut (u64, u64))) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        change(&mut state);
        self.raised.notify_all();
    }

    /// Wait up to `within` for `done`.
    fn wait_for(&self, done: Done, within: Duration) -> bool {
        let reached = |state: &(u64, u64)| match done {
            Done::Taken(seq) => state.1 >= seq,
            Done::Wiped(moves) => state.0 >= moves,
        };
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let (state, _) = self
            .raised
            .wait_timeout_while(state, within, |state| !reached(state))
            .unwrap_or_else(PoisonError::into_inner);
        reached(&state)
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
        None => Leaving::of(None),
    }
}

/// `lock` is what [`left_foreground`] does from now on.
pub(crate) fn on_leaving(lock: impl Fn() -> Leaving + Send + 'static) {
    *LOCK.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(lock));
}

pub(crate) fn set(host: &Host) {
    let _ = COPY.set(host.copy);
    let _ = SHARE.set(host.share);
    let _ = BACK.set(host.back);
}

/// The system's Back, where a shell has one.
pub(crate) fn back() {
    if let Some(back) = BACK.get() {
        back();
    }
}

/// The shell's share sheet, where a shell has one: `None` with no shell, as
/// on the desktop, which saves its file to the Downloads folder instead
/// (docs/DECISIONS.md D32 item 10 (i)).
pub(crate) fn share(name: &str, text: &str) -> Option<Result<(), String>> {
    SHARE.get().map(|sheet| sheet(name, text))
}

/// Puts `text` on the shell's clipboard. `false` when there is no shell's,
/// and iced's is the one to use.
pub(crate) fn copy(text: &str) -> bool {
    COPY.get().map(|copy| copy(text)).is_some()
}

/// Where [`screenshot_taken`] and [`capture_changed`] write: the capture state
/// of the hosted application, once it has started.
static CAPTURE: Mutex<Option<Arc<Capture>>> = Mutex::new(None);

/// A screenshot was taken. iOS says so after the capture, through
/// `UIApplicationUserDidTakeScreenshotNotification`, so it cannot block the
/// capture. The application then warns on the recovery phrase's screen, when
/// the words were on it (docs/DECISIONS.md D44 item 3). A screenshot taken
/// before the application has started is not counted.
pub fn screenshot_taken() {
    if let Some(capture) = registered() {
        capture.shot();
    }
}

/// The screen is being recorded or mirrored (`captured`), or no longer is,
/// as iOS reports it through `UIScreenCapturedDidChangeNotification`. The
/// application hides the recovery phrase's words while it is captured. The
/// report is best effort: iOS can read the screen as not captured while a
/// recording runs (docs/DECISIONS.md D44 item 3). A change before the
/// application has started is not kept; [`Host::captured`] gives that state.
pub fn capture_changed(captured: bool) {
    if let Some(capture) = registered() {
        capture.set_captured(captured);
    }
}

/// The capture state [`screenshot_taken`] and [`capture_changed`] write to, if
/// an application has started.
fn registered() -> Option<Arc<Capture>> {
    CAPTURE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// What the shell has reported about the screen's capture. The application
/// reads it on its UI thread, at each message, as it reads the moves to the
/// background: a screenshot is a change in the count, however many came since
/// the last read, and the capture state is the last one the shell gave.
#[derive(Debug, Default)]
pub(crate) struct Capture {
    shots: AtomicU64,
    captured: AtomicBool,
}

impl Capture {
    /// A screenshot was taken.
    pub(crate) fn shot(&self) {
        self.shots.fetch_add(1, Ordering::SeqCst);
    }

    /// The screen is captured (`true`) or no longer is.
    pub(crate) fn set_captured(&self, captured: bool) {
        self.captured.store(captured, Ordering::SeqCst);
    }

    /// How many screenshots have been taken, and whether the screen is
    /// captured now.
    pub(crate) fn read(&self) -> (u64, bool) {
        (
            self.shots.load(Ordering::SeqCst),
            self.captured.load(Ordering::SeqCst),
        )
    }
}

/// Makes `capture` the state [`screenshot_taken`] and [`capture_changed`]
/// write to. Where the shell can read the screen's state, `captured` gives it
/// as the application starts. It is read after `capture` is registered, so a
/// change made in between is not lost (docs/DECISIONS.md D44 item 3).
pub(crate) fn on_capture(capture: Arc<Capture>, captured: Option<fn() -> bool>) {
    *CAPTURE.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&capture));
    if let Some(read) = captured {
        capture.set_captured(read());
    }
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
            Leaving::of(None)
        });
        let _ = left_foreground();
        let counter = Arc::clone(&second);
        on_leaving(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Leaving::of(None)
        });
        let _ = left_foreground();
        let _ = left_foreground();
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(second.load(Ordering::SeqCst), 2, "the worker started last");
    }

    #[test]
    fn a_screenshot_is_counted_and_the_capture_state_is_the_last_one_given() {
        let capture = Capture::default();
        assert_eq!(capture.read(), (0, false));
        capture.shot();
        capture.shot();
        capture.set_captured(true);
        assert_eq!(capture.read(), (2, true));
        capture.set_captured(false);
        assert_eq!(capture.read(), (2, false));
    }

    #[test]
    fn the_shell_reaches_the_registered_capture_and_reads_its_start_state() {
        fn captured_at_start() -> bool {
            true
        }
        // Nothing registered: the shell's calls do nothing, and do not panic.
        screenshot_taken();
        capture_changed(false);
        let capture = Arc::new(Capture::default());
        on_capture(Arc::clone(&capture), Some(captured_at_start));
        assert_eq!(capture.read(), (0, true), "the state at start is read");
        screenshot_taken();
        capture_changed(false);
        assert_eq!(capture.read(), (1, false));
    }
}
