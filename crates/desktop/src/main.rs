//! Tawara for Windows, macOS and Linux.

fn main() {
    if let Err(error) = tawara_app::run() {
        eprintln!("{}: {error}", tawara_app::DISPLAY_NAME);
        std::process::exit(1);
    }
}
