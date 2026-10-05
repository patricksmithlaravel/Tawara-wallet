//! Draws every built screen headlessly into `design/screenshots/`, or checks
//! that the images there are what the screens draw now (docs/DECISIONS.md
//! D27, item 1).
//!
//! ```text
//! cargo run -p tawara-app --example screenshots            # write them
//! cargo run -p tawara-app --example screenshots -- --check # compare
//! ```
//!
//! Each screen is drawn from sample data (`tawara_wallet_core::sample` and
//! the forms below) with iced's software renderer, tiny-skia, at the size of
//! the rendering it follows and a scale of 1. Every piece of text names a
//! bundled face (`tawara_app::fonts`), so the pixels do not depend on the
//! fonts a machine has installed. `--check` compares pixels, not file bytes,
//! and writes what it drew beside the committed image when they differ.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use iced::advanced::renderer::{self, Headless};
use iced::advanced::{clipboard, graphics};
use iced::mouse::Cursor;
use iced::theme::Base;
use iced::{Event, Size, window};
use iced_runtime::user_interface::{Cache, UserInterface};
use tawara_app::app::{
    Back, Busy, Model, NodeForm, NodeState, PasswordForm, PhraseState, RestoreForm, Screen,
    StartChoice, UnlockForm, WalletPage,
};
use tawara_app::{fonts, screens, theme};
use tawara_wallet_core::location::SyncWarning;
use tawara_wallet_core::preferences::Preferences;
use tawara_wallet_core::{Activity, CONFIRM_POSITIONS, PhraseForDisplay, Progress};

/// The rendering's frame size for each screen (`design/INDEX.md`), and the
/// smallest window (`tawara_app::WINDOW_MIN`, docs/DECISIONS.md D27 item 9).
const FIRST_RUN: (u32, u32) = (1440, 900);
const DASHBOARD: (u32, u32) = (1440, 1160);
const SMALLEST: (u32, u32) = (1024, 700);
const DASHBOARD_NARROW: (u32, u32) = (1024, 1160);

const NODE: &str = "https://node.example";
const DIR: &str = "/home/you/.local/share/tawara/keystore";

fn base() -> Model {
    let prefs = Preferences {
        node: Some(NODE.to_owned()),
        ..Preferences::default()
    };
    let mut model = Model::new(prefs, Some(PathBuf::from(DIR)));
    model.node = NodeState {
        url: Some(NODE.to_owned()),
        tip: Some((871_173, Duration::from_millis(42))),
        error: None,
    };
    model.screen = Screen::Start {
        choice: StartChoice::Create,
    };
    model
}

fn with(screen: Screen) -> Model {
    let mut m = base();
    m.screen = screen;
    m
}

fn form(dir: &str, password: &str) -> PasswordForm {
    PasswordForm {
        dir: dir.to_owned(),
        password: password.to_owned(),
        again: password.to_owned(),
        error: None,
        synced: None,
    }
}

/// Every built screen: its file name, its size, and its model.
fn samples() -> Vec<(&'static str, (u32, u32), Model)> {
    let phrase = || PhraseState {
        phrase: PhraseForDisplay::example(),
        positions: CONFIRM_POSITIONS,
        written: true,
        words: Default::default(),
        error: None,
    };
    let synced = SyncWarning {
        service: "Dropbox",
        folder: PathBuf::from("/home/you/Dropbox"),
        only_if_enabled: false,
    }
    .to_string();
    let busy = |activity, progress| Busy {
        id: tawara_wallet_core::RequestId::example(),
        activity,
        progress,
    };
    let mut waiting = with(Screen::Unlock(UnlockForm::default()));
    waiting.busy = Some(busy(None, None));
    let mut opening = with(Screen::Unlock(UnlockForm::default()));
    opening.busy = Some(busy(Some(Activity::DerivingKey), None));
    let mut reconciling = with(Screen::Unlock(UnlockForm::default()));
    reconciling.busy = Some(busy(
        Some(Activity::AskingNode),
        Some(Progress {
            account: 2,
            accounts: 3,
            position: 3_840,
            ceiling: 10_017,
        }),
    ));
    let wallet = || {
        let mut m = with(Screen::Wallet(WalletPage::default()));
        m.wallet = Some(tawara_wallet_core::sample::wallet_view());
        m
    };
    vec![
        ("s1-get-started", FIRST_RUN, base()),
        (
            "s1-get-started-restore",
            FIRST_RUN,
            with(Screen::Start {
                choice: StartChoice::Restore,
            }),
        ),
        (
            "s2-node",
            FIRST_RUN,
            with(Screen::Node(NodeForm {
                url: NODE.to_owned(),
                error: None,
                back: Back::Start,
            })),
        ),
        (
            "s3-new-wallet",
            FIRST_RUN,
            with(Screen::NewWallet(form(DIR, "correct horse battery staple"))),
        ),
        (
            "s3-new-wallet-synced",
            FIRST_RUN,
            with(Screen::NewWallet(PasswordForm {
                synced: Some(synced),
                ..form("/home/you/Dropbox/tawara", "")
            })),
        ),
        ("s4-phrase", FIRST_RUN, with(Screen::Phrase(phrase()))),
        (
            "s5-confirm",
            FIRST_RUN,
            with(Screen::Confirm(PhraseState {
                words: ["abandon".into(), "abandon".into(), String::new()],
                ..phrase()
            })),
        ),
        (
            "s6-restore",
            FIRST_RUN,
            with(Screen::Restore(RestoreForm {
                form: form(DIR, "correct horse battery staple"),
                phrase: "abandon abandon abandon".to_owned(),
            })),
        ),
        (
            "s7-unlock",
            FIRST_RUN,
            with(Screen::Unlock(UnlockForm {
                dir: DIR.to_owned(),
                ..UnlockForm::default()
            })),
        ),
        (
            "s7-unlock-idle",
            FIRST_RUN,
            with(Screen::Unlock(UnlockForm {
                dir: DIR.to_owned(),
                note: Some(
                    "Locked after 5 minutes with nothing done. The store is closed and its \
                     key is gone from memory; unlock it to go on."
                        .to_owned(),
                ),
                ..UnlockForm::default()
            })),
        ),
        (
            "s7-unlock-no-folder",
            FIRST_RUN,
            with(Screen::Unlock(UnlockForm {
                dir: "/home/you/wallet".to_owned(),
                error: Some(
                    "there is no keystore at /home/you/wallet: the folder does not exist"
                        .to_owned(),
                ),
                ..UnlockForm::default()
            })),
        ),
        ("s8-waiting", FIRST_RUN, waiting),
        ("s8-opening", FIRST_RUN, opening),
        ("s8-reconciling", FIRST_RUN, reconciling),
        ("w1-wallet", DASHBOARD, wallet()),
        ("narrow-s1-get-started", SMALLEST, base()),
        ("narrow-w1-wallet", DASHBOARD_NARROW, wallet()),
    ]
}

/// The pixels of `model`'s window at `size`, as RGBA.
fn draw(renderer: &mut iced::Renderer, model: &Model, size: (u32, u32)) -> Vec<u8> {
    let theme = theme::theme();
    #[allow(clippy::cast_precision_loss)]
    let bounds = Size::new(size.0 as f32, size.1 as f32);
    let mut ui = UserInterface::build(screens::view(model), bounds, Cache::default(), renderer);
    let mut messages = Vec::new();
    let _ = ui.update(
        &[Event::Window(window::Event::RedrawRequested(
            iced::time::Instant::now(),
        ))],
        Cursor::Unavailable,
        renderer,
        &mut clipboard::Null,
        &mut messages,
    );
    let style = theme.base();
    ui.draw(
        renderer,
        &theme,
        &renderer::Style {
            text_color: style.text_color,
        },
        Cursor::Unavailable,
    );
    renderer.screenshot(Size::new(size.0, size.1), 1.0, style.background_color)
}

fn encode(rgba: &[u8], size: (u32, u32)) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size.0, size.1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

fn decode(path: &Path) -> Result<((u32, u32), Vec<u8>), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| format!("{}: too large", path.display()))?;
    let mut buf = vec![0; size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(format!("{}: not 8-bit RGBA", path.display()));
    }
    buf.truncate(info.buffer_size());
    Ok(((info.width, info.height), buf))
}

fn main() -> ExitCode {
    let mut check = false;
    let mut dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/screenshots");
    for arg in std::env::args().skip(1) {
        if arg == "--check" {
            check = true;
        } else {
            dir = PathBuf::from(arg);
        }
    }
    if let Ok(mut font_system) = graphics::text::font_system().write() {
        for bytes in fonts::FILES {
            font_system.load_font(std::borrow::Cow::Borrowed(bytes));
        }
    }
    let Some(mut renderer) = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        fonts::BODY,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    )) else {
        eprintln!("the tiny-skia renderer did not start");
        return ExitCode::FAILURE;
    };
    if !check && let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("{}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    let mut failed = false;
    for (name, size, model) in samples() {
        let rgba = draw(&mut renderer, &model, size);
        let path = dir.join(format!("{name}.png"));
        if check {
            match decode(&path) {
                Ok((drawn_size, committed)) if drawn_size == size && committed == rgba => {
                    println!("same     {name}");
                }
                Ok((drawn_size, committed)) => {
                    let differing = committed
                        .chunks(4)
                        .zip(rgba.chunks(4))
                        .filter(|(a, b)| a != b)
                        .count();
                    let fresh = std::env::temp_dir().join(format!("{name}.drawn.png"));
                    let _ = encode(&rgba, size).map(|bytes| std::fs::write(&fresh, bytes));
                    eprintln!(
                        "DIFFERS  {name}: committed {}x{}, drawn {}x{}, {differing} pixel(s) \
                         differ; what was drawn is at {}",
                        drawn_size.0,
                        drawn_size.1,
                        size.0,
                        size.1,
                        fresh.display()
                    );
                    failed = true;
                }
                Err(e) => {
                    eprintln!("MISSING  {name}: {e}");
                    failed = true;
                }
            }
        } else {
            match encode(&rgba, size).and_then(|bytes| {
                std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
            }) {
                Ok(()) => println!("wrote    {}", path.display()),
                Err(e) => {
                    eprintln!("{e}");
                    failed = true;
                }
            }
        }
    }
    if failed {
        eprintln!(
            "\nThe screenshots in {} are not what the screens draw. Run `cargo run -p \
             tawara-app --example screenshots` and commit the result.",
            dir.display()
        );
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
