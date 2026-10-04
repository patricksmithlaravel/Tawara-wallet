//! The slow task: a real OS thread, progress back over a channel, and two
//! ways to stop it (a shared flag the thread polls, and the iced task's abort
//! handle, which drops the receiver so the thread's next send fails). This is
//! the shape wallet-core's worker will have.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use iced::futures::channel::{mpsc, oneshot};
use iced::task::{Handle, Task};

#[derive(Debug, Clone)]
pub enum Event {
    Progress { step: u32, of: u32 },
    Finished { steps: u32, elapsed_ms: u128 },
    Cancelled { at_step: u32 },
}

pub struct Running {
    cancel: Arc<AtomicBool>,
    handle: Handle,
}

impl Running {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.handle.abort();
    }
}

/// `steps` units of `step_ms` of CPU-bound work each, on a dedicated thread.
pub fn start<M: Send + 'static>(
    steps: u32,
    step_ms: u64,
    to_message: fn(Event) -> M,
) -> (Task<M>, Running) {
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::unbounded();
    let flag = cancel.clone();
    let spawned = std::thread::Builder::new()
        .name("spike-worker".into())
        .spawn(move || {
            let t0 = Instant::now();
            for step in 1..=steps {
                if flag.load(Ordering::Relaxed) {
                    let _ = tx.unbounded_send(Event::Cancelled { at_step: step });
                    crate::report::note(format!("worker stopped by the flag before step {step}"));
                    return;
                }
                burn(Duration::from_millis(step_ms));
                if tx
                    .unbounded_send(Event::Progress { step, of: steps })
                    .is_err()
                {
                    crate::report::note(format!("worker stopped: receiver dropped at step {step}"));
                    return;
                }
            }
            let _ = tx.unbounded_send(Event::Finished {
                steps,
                elapsed_ms: t0.elapsed().as_millis(),
            });
        });
    if let Err(e) = spawned {
        crate::report::check("task.spawn", false, e);
    }
    let (task, handle) = Task::run(rx, to_message).abortable();
    (task, Running { cancel, handle })
}

/// Busy work rather than sleep, so the UI thread competes for CPU as it
/// will during an Argon2 unlock.
fn burn(d: Duration) {
    let end = Instant::now() + d;
    let mut x = 0u64;
    while Instant::now() < end {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
    }
    std::hint::black_box(x);
}

/// A message after `ms`. iced's `time::every` needs the tokio or smol
/// executor, and the application uses thread-pool.
pub fn after<M: Send + 'static>(ms: u64, message: M) -> Task<M> {
    let (tx, rx) = oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        let _ = tx.send(());
    });
    Task::perform(rx, move |_| message)
}

/// Runs `f` on its own thread and resolves to `message` when it returns.
pub fn on_thread<M: Send + 'static>(f: impl FnOnce() + Send + 'static, message: M) -> Task<M> {
    let (tx, rx) = oneshot::channel::<()>();
    std::thread::spawn(move || {
        f();
        let _ = tx.send(());
    });
    Task::perform(rx, move |_| message)
}
