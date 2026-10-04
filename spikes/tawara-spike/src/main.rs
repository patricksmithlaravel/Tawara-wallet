//! Desktop and iOS entry point. On iOS winit calls `UIApplicationMain`
//! itself and never returns, so this must be the process's `main`.
//!
//! Desktop: `tawara-spike [--self-test] [--offline] [--storage DIR]
//! [--screenshot PATH]`; `SPIKE_SELFTEST=1` and `SPIKE_DATA_DIR` work too.
//! `ICED_BACKEND=tiny-skia` forces the software renderer, `wgpu` the GPU
//! one; unset, iced tries wgpu and falls back to tiny-skia.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .map(std::path::PathBuf::from)
    };
    let self_test =
        std::env::var_os("SPIKE_SELFTEST").is_some() || args.iter().any(|a| a == "--self-test");
    let offline =
        std::env::var_os("SPIKE_OFFLINE").is_some() || args.iter().any(|a| a == "--offline");

    #[cfg(target_os = "ios")]
    let data_dir = tawara_spike::ios::data_dir();
    #[cfg(not(target_os = "ios"))]
    let data_dir = value("--storage")
        .or_else(|| std::env::var_os("SPIKE_DATA_DIR").map(std::path::PathBuf::from))
        .or_else(|| {
            Some(std::env::temp_dir().join(format!("tawara-spike-{}", std::process::id())))
        });

    if self_test && !cfg!(target_os = "ios") {
        // A hung self-test fails the job rather than timing it out.
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(300));
            tawara_spike::report::check("selftest.watchdog", false, "no result after 300 s");
            std::process::exit(3);
        });
    }

    let config = tawara_spike::Config {
        self_test,
        probe_dirs: data_dir.iter().cloned().collect(),
        data_dir,
        screenshot: value("--screenshot"),
        offline,
    };
    if let Err(error) = tawara_spike::run(config) {
        tawara_spike::report::check("iced.run", false, error);
        std::process::exit(2);
    }
    std::process::exit(tawara_spike::report::exit_code());
}
