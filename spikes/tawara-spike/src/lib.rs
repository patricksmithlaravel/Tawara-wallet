//! Phase 1 feasibility spike: one small iced program for the desktop,
//! Android and iOS (docs/PLAN.md section 7). Throwaway quality by design;
//! nothing here ships.
//!
//! What it exercises:
//! - a password-mode text input whose value moves into a `Zeroizing` buffer
//!   on submit, and a plain text input for the soft keyboard, input-method
//!   composition and paste;
//! - a slow task on a background thread, with progress and cancellation;
//! - the clipboard, iced's own and (on mobile, where iced's is a stub) the
//!   platform's;
//! - the wallet library on the device (`libcheck`).
//!
//! With `Config::self_test` it drives itself and prints `SPIKE` lines
//! (`report`). See README.md for the checklist each platform's CI runs.

pub mod app;
#[cfg(unix)]
pub mod libcheck;
pub mod report;
pub mod worker;

#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "android")]
mod android_jni;
#[cfg(target_os = "ios")]
pub mod ios;

use std::path::PathBuf;

/// What the platform entry point hands the shared program.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub self_test: bool,
    /// App-private storage, where the library checks make their stores.
    pub data_dir: Option<PathBuf>,
    /// Platform directories the library checks probe without writing.
    pub probe_dirs: Vec<PathBuf>,
    /// Desktop only: where the self-test writes its in-app screenshot.
    pub screenshot: Option<PathBuf>,
    /// Skip the network check.
    pub offline: bool,
}

/// Forwards iced's and wgpu's `log` records to stderr, so the log shows
/// which adapter or renderer was chosen.
struct StderrLog;

impl log::Log for StderrLog {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Warn
            || (m.level() <= log::Level::Info
                && (m.target().starts_with("iced_wgpu") || m.target().starts_with("iced_renderer")))
    }

    fn log(&self, r: &log::Record<'_>) {
        if self.enabled(r.metadata()) {
            eprintln!("SPIKE log {} {}: {}", r.level(), r.target(), r.args());
        }
    }

    fn flush(&self) {}
}

/// The platform clipboard, on mobile: window_clipboard 0.5.1, which iced
/// uses, has stub backends for Android and iOS. Desktop uses iced's.
pub fn platform_clipboard_write(text: &str) -> bool {
    #[cfg(target_os = "android")]
    return android::clipboard_write(text);
    #[cfg(target_os = "ios")]
    return ios::pasteboard_write(text);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = text;
        false
    }
}

pub fn platform_clipboard_read() -> Option<String> {
    #[cfg(target_os = "android")]
    return android::clipboard_read();
    #[cfg(target_os = "ios")]
    return ios::pasteboard_read();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    None
}

/// The library checks, on the calling thread (a worker's).
pub fn library_checks(config: &Config) {
    #[cfg(unix)]
    match &config.data_dir {
        Some(dir) => match std::fs::create_dir_all(dir) {
            Ok(()) => libcheck::run(dir, &config.probe_dirs, !config.offline),
            Err(e) => {
                report::check("lib.data_dir", false, format!("{}: {e}", dir.display()));
            }
        },
        None => report::note("lib: no data directory; library checks skipped"),
    }
    #[cfg(not(unix))]
    {
        let _ = config;
        report::note("lib: library checks are unix-only in phase 1; skipped");
    }
}

pub fn run(config: Config) -> iced::Result {
    report::install_panic_hook();
    let _ = log::set_logger(&StderrLog).map(|()| log::set_max_level(log::LevelFilter::Info));
    report::note(format!(
        "start os={} arch={} self_test={} ICED_BACKEND={} data_dir={:?}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        config.self_test,
        std::env::var("ICED_BACKEND").unwrap_or_else(|_| "(unset)".into()),
        config.data_dir
    ));
    iced::application(
        move || app::Spike::new(config.clone()),
        app::Spike::update,
        app::Spike::view,
    )
    .subscription(app::Spike::subscription)
    .default_font(iced::Font::with_name("Fira Sans"))
    .title("Tawara spike")
    .window_size((420.0, 780.0))
    .run()
}

/// The Android entry point: android-activity calls it on its own thread
/// once the activity is created.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let config = android::init(&app);
    iced_winit::set_android_app(app);
    if let Err(error) = run(config) {
        report::check("iced.run", false, error);
    }
    // winit allows one event loop per process: a second android_main in this
    // process could not run, so the process ends with the loop.
    report::note("event loop returned; exiting the process");
    std::process::exit(0);
}
