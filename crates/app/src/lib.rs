//! The Tawara application: iced state, messages, `update`, `view`,
//! subscriptions, screens and theme.
//!
//! It never calls the wallet library. Every library call goes through
//! `tawara-wallet-core`'s worker thread, and what comes back is a view model
//! that holds no secret (docs/PLAN.md section 3). The screens follow the
//! owner's renderings in `design/renderings/` (docs/PLAN.md section 5), and
//! where a rendering conflicts with the threat model in docs/PLAN.md section
//! 4, section 4 wins; docs/DECISIONS.md D27 and docs/SCREENS.md record
//! each case.
//!
//! - [`app`]: the model, the messages, the worker bridge and `update`.
//! - [`host`]: what a mobile shell supplies, and its word when the
//!   application leaves the foreground.
//! - [`screens`]: every screen, a function of the model alone, so that the
//!   screenshot example draws any screen from sample data.
//! - [`theme`], [`fonts`], [`icon`], [`ui`]: the renderings' tokens, the
//!   bundled faces, the line icons and the pieces the screens are built
//!   from.

pub mod app;
pub mod fonts;
pub mod history;
pub mod host;
pub mod icon;
pub mod screens;
pub mod theme;
pub mod ui;

/// The application's display name, as the owner gave it.
pub const DISPLAY_NAME: &str = "Tawara";

/// The window's size at start, the renderings' (`design/INDEX.md`), and the
/// smallest it may be: the medium width class (docs/DECISIONS.md D27, item
/// 9).
pub const WINDOW: (f32, f32) = (1440.0, 900.0);
pub const WINDOW_MIN: (f32, f32) = (1024.0, 700.0);

pub use host::{Host, left_foreground};

/// Run the application in a mobile shell (`crates/mobile`): Android or iOS,
/// in the window the system gives it, with what `host` supplies
/// (docs/DECISIONS.md D32).
pub fn run_in(host: Host) -> iced::Result {
    let mut application = iced::application(
        move || app::App::boot_in(&host),
        app::App::update,
        app::App::view,
    )
    .subscription(app::App::subscription)
    .theme(|_: &app::App| theme::theme())
    .default_font(fonts::BODY)
    .title(DISPLAY_NAME);
    for bytes in fonts::FILES {
        application = application.font(bytes);
    }
    application.run()
}

/// Run the application on the desktop.
pub fn run() -> iced::Result {
    let mut application = iced::application(app::App::boot, app::App::update, app::App::view)
        .subscription(app::App::subscription)
        .theme(|_: &app::App| theme::theme())
        .default_font(fonts::BODY)
        .title(DISPLAY_NAME)
        .window_size(WINDOW)
        .window(iced::window::Settings {
            size: iced::Size::new(WINDOW.0, WINDOW.1),
            min_size: Some(iced::Size::new(WINDOW_MIN.0, WINDOW_MIN.1)),
            ..iced::window::Settings::default()
        });
    for bytes in fonts::FILES {
        application = application.font(bytes);
    }
    application.run()
}
