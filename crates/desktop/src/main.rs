//! Tawara for Windows, macOS and Linux.
//!
//! Phase 0 holds the entry point's place in the workspace; the window opens
//! in phase 3, when the screens exist.

fn main() {
    println!(
        "{} {}: no interface yet; the screens arrive in phase 3.",
        tawara_app::DISPLAY_NAME,
        env!("CARGO_PKG_VERSION")
    );
}
