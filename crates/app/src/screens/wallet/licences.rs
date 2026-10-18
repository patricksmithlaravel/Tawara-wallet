//! The licences, reached from Settings' About card (docs/DECISIONS.md D35;
//! docs/RELEASE.md section 4): the wallet's own licence, the Mochimo agreement
//! in `LICENSE.md` (docs/DECISIONS.md D10), and the notices of the software the
//! wallet includes, `THIRD-PARTY-NOTICES.txt`, which `ci/notices.sh` writes.
//! A file inside a phone app cannot be opened by the person using it, so both
//! texts are embedded when the application is built, word for word.
//!
//! The page is one scrollable column of monospace text in cards, each with a
//! copy button, so that it scrolls with a finger on a phone and with the wheel
//! on the desktop. The notices are long (about 8,000 lines), so each text is
//! cut into parts of [`LINES`] lines, and the page draws only the parts it
//! shows.
//!
//! The wording of the Settings row and of this page is new, and it awaits the
//! owner's approval (docs/RELEASE.md section 4).

use std::sync::OnceLock;

use iced::widget::text::Wrapping;
use iced::widget::{column, row};
use iced::{Alignment, Element, Length};

use super::{copy, frame};
use crate::app::{Message, Model, To, WalletMsg, WalletPage};
use crate::icon::Icon;
use crate::theme::{self, color, space as sp};
use crate::ui::{self, Size, t, ty};

/// The wallet's own licence: the Mochimo Cryptocurrency Engine License
/// Agreement, version 1.0, as the repository holds it (docs/DECISIONS.md D10).
const LICENCE: &str = include_str!("../../../../../LICENSE.md");

/// The notices of the software the wallet includes, as `ci/notices.sh` writes
/// them (docs/DECISIONS.md D35). The generator appends the licence above.
const NOTICES: &str = include_str!("../../../../../THIRD-PARTY-NOTICES.txt");

/// How many lines one part of a text holds (see the module's notes).
const LINES: usize = 40;

/// The parts of [`LICENCE`] and of [`NOTICES`], cut once.
static LICENCE_PARTS: OnceLock<Vec<&'static str>> = OnceLock::new();
static NOTICES_PARTS: OnceLock<Vec<&'static str>> = OnceLock::new();

/// The licences page (W11's Licences): the wallet's licence, then the third
/// party notices, each in a card under a Back that returns to Settings.
pub fn view<'a>(model: &'a Model, page: &'a WalletPage) -> Element<'a, Message> {
    // Back takes no part in the worker's work, but like the sidebar it leads
    // nowhere while the worker is busy.
    let to_settings: Option<Message> = model
        .busy
        .is_none()
        .then(|| WalletMsg::Open(To::Settings).into());
    let back: Element<'a, Message> = ui::button_with(
        "Back",
        theme::Button::Secondary,
        Size::Small,
        Some(Icon::ChevronLeft),
        to_settings,
    )
    .into();
    frame(
        model,
        page,
        "Licences",
        "The wallet's own licence, and the notices of the software it includes",
        vec![back],
        vec![
            text_card(
                model,
                page,
                "Tawara's licence",
                "Copy the licence",
                LICENCE,
                &LICENCE_PARTS,
            ),
            text_card(
                model,
                page,
                "Third-party notices",
                "Copy the notices",
                NOTICES,
                &NOTICES_PARTS,
            ),
        ],
    )
}

/// `whole`, under `title` and beside its copy button, in a card. Its parts
/// are drawn one after another in monospace, wrapped at words and, where a
/// word is wider than the line, at glyphs, so that nothing passes the edge of
/// a phone.
fn text_card<'a>(
    model: &Model,
    page: &'a WalletPage,
    title: &'a str,
    label: &'a str,
    whole: &'static str,
    cell: &'static OnceLock<Vec<&'static str>>,
) -> Element<'a, Message> {
    let compact = model.compact();
    let title_text = t(title, ty::CARD_TITLE, color::TEXT_PRIMARY);
    let copy_button = copy(page, whole, label);
    let head: Element<'a, Message> = if compact {
        column![title_text, copy_button].spacing(sp::S12).into()
    } else {
        row![title_text.width(Length::Fill), copy_button]
            .spacing(sp::S16)
            .align_y(Alignment::Center)
            .into()
    };
    let mut lines: iced::widget::Column<'a, Message> = column![].width(Length::Fill);
    for part in parts(whole, cell) {
        lines = lines.push(
            t(*part, ty::PAGE, color::TEXT_SECONDARY)
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
        );
    }
    ui::card_for(compact, column![head, lines].spacing(sp::S16)).into()
}

/// The parts of `whole`, cut once into `cell`.
fn parts(
    whole: &'static str,
    cell: &'static OnceLock<Vec<&'static str>>,
) -> &'static [&'static str] {
    cell.get_or_init(|| split(whole)).as_slice()
}

/// `whole` cut into parts of [`LINES`] lines, each without its last line
/// break, which the page's own layout puts back: joined with line breaks the
/// parts give `whole` again exactly.
fn split(whole: &'static str) -> Vec<&'static str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut lines = 0;
    for (at, byte) in whole.bytes().enumerate() {
        if byte == b'\n' {
            lines += 1;
            if lines == LINES {
                parts.push(&whole[start..at]);
                start = at + 1;
                lines = 0;
            }
        }
    }
    parts.push(&whole[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tawara_wallet_core::preferences::Preferences;
    use tawara_wallet_core::sample;

    use iced::advanced::widget::operation::scrollable::{RelativeOffset, snap_to};
    use iced::{Event, mouse};

    use super::*;
    use crate::app::{Page, Screen};
    use crate::harness::{
        Driver, finger_lift, finger_move, finger_press, fits_at, mouse_move, mouse_press,
        mouse_release, texts,
    };
    use crate::screens;

    /// A window of the desktop, as the wallet's first window is drawn.
    const DESKTOP: (f32, f32) = (1024.0, 700.0);

    /// A phone of the narrowest width and the shortest height the screens are
    /// held to (docs/DECISIONS.md D32 item 10 (b)).
    const PHONE: (f32, f32) = (360.0, 640.0);

    /// The sample store with its licences page shown, in a window `width`
    /// wide.
    fn licences_at(width: f32) -> Model {
        let mut model = Model::new(Preferences::default(), Some(PathBuf::from("/home/you")));
        model.wallet = Some(sample::wallet_view());
        model.width = width;
        model.screen = Screen::Wallet(WalletPage {
            page: Page::Licences,
            ..WalletPage::default()
        });
        model
    }

    /// The messages a press on the drawn text `label` produces in a window of
    /// `size`, as a person presses it.
    fn pressed(model: &Model, label: &str, size: (f32, f32)) -> Vec<Message> {
        let mut driver = Driver::new(screens::view(model), (size.0 as u32, size.1 as u32));
        let at = driver
            .text_bounds(label)
            .unwrap_or_else(|| panic!("`{label}` is not drawn"));
        let (messages, _, _) = driver.send(&[
            mouse_move(at.center_x(), at.center_y()),
            mouse_press(),
            mouse_release(),
        ]);
        messages
    }

    #[test]
    fn the_embedded_texts_are_the_files_and_the_notices_begin_as_the_generator_writes_them() {
        assert!(
            LICENCE.starts_with("# MOCHIMO CRYPTOCURRENCY ENGINE LICENSE AGREEMENT VERSION 1.0")
        );
        // The generator's header (docs/DECISIONS.md D35, item 2), and its end: it
        // appends LICENSE.md, so the notices end with the wallet's licence.
        assert!(NOTICES.starts_with("THIRD-PARTY NOTICES\n===================\n"));
        assert!(
            NOTICES.ends_with(LICENCE),
            "the notices end with LICENSE.md"
        );
    }

    #[test]
    fn the_parts_give_each_text_back_whole() {
        for (whole, cell) in [(LICENCE, &LICENCE_PARTS), (NOTICES, &NOTICES_PARTS)] {
            let parts = parts(whole, cell);
            assert_eq!(parts.join("\n"), whole);
            assert!(parts.iter().all(|part| part.matches('\n').count() < LINES));
        }
        // Splitting in the middle and at the very end keeps every break.
        assert_eq!(split("a\nb\nc").join("\n"), "a\nb\nc");
        assert_eq!(split("a\nb\n").join("\n"), "a\nb\n");
        assert_eq!(split("").join("\n"), "");
    }

    #[test]
    fn the_page_draws_every_part_of_both_texts_and_its_back_link() {
        let drawn: Vec<String> = texts(screens::view(&licences_at(DESKTOP.0)), DESKTOP)
            .into_iter()
            .map(|(text, _)| text)
            .collect();
        for (whole, cell) in [(LICENCE, &LICENCE_PARTS), (NOTICES, &NOTICES_PARTS)] {
            for part in parts(whole, cell) {
                assert!(
                    drawn.iter().any(|text| text == part),
                    "a part of the text is not drawn: {part:.60?}"
                );
            }
        }
        for line in [
            "Tawara includes the third-party software listed in this file.",
            "MOCHIMO CRYPTOCURRENCY ENGINE LICENSE AGREEMENT VERSION 1.0",
            "Back",
            "Licences",
        ] {
            assert!(
                drawn.iter().any(|text| text.contains(line)),
                "`{line}` is not drawn"
            );
        }
    }

    #[test]
    fn the_page_fits_a_phone_of_360_by_640_and_the_desktop_of_1024_by_700() {
        assert_eq!(
            fits_at(screens::view(&licences_at(PHONE.0)), PHONE.0, PHONE.1),
            Ok(())
        );
        assert_eq!(
            fits_at(screens::view(&licences_at(DESKTOP.0)), DESKTOP.0, DESKTOP.1),
            Ok(())
        );
    }

    #[test]
    fn back_leads_to_settings_on_the_desktop_and_on_a_phone() {
        for size in [DESKTOP, (360.0, 4000.0)] {
            let width = size.0;
            let messages = pressed(&licences_at(width), "Back", size);
            assert!(
                matches!(
                    messages.as_slice(),
                    [Message::Wallet(WalletMsg::Open(To::Settings))]
                ),
                "{messages:?}"
            );
        }
    }

    #[test]
    fn a_finger_drags_the_licences_on_a_phone() {
        // The text is one scrollable column with no touch of its own, so a finger
        // on it drags the page, as a phone's page is dragged: 200 pixels up scrolls
        // the page 200 down.
        let model = licences_at(PHONE.0);
        let mut driver = Driver::new(screens::view(&model), (PHONE.0 as u32, PHONE.1 as u32));
        let _ = driver.send(&[
            finger_press(1, 180.0, 500.0),
            finger_move(1, 180.0, 300.0),
            finger_lift(1, 180.0, 300.0),
        ]);
        let scrolled = driver
            .scroll_of(&screens::page_id())
            .map_or(0.0, |(_, offset)| offset.y);
        assert!(
            (scrolled - 200.0).abs() < 1.0,
            "a finger dragged 200 pixels, and the page scrolled {scrolled}"
        );
    }

    #[test]
    fn the_licences_are_scrolled_to_their_top_when_they_open() {
        // A page keeps the scroll of the one it replaces, since iced keeps a
        // scrollable's state with its place in the tree; the wallet's Open scrolls
        // the page to its top for the licences, and this is that operation.
        let model = licences_at(PHONE.0);
        let mut driver = Driver::new(screens::view(&model), (PHONE.0 as u32, PHONE.1 as u32));
        let _ = driver.send(&[
            mouse_move(180.0, 300.0),
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: -30.0 },
            }),
        ]);
        let scrolled = driver
            .scroll_of(&screens::page_id())
            .map_or(0.0, |(_, offset)| offset.y);
        assert!(scrolled > 0.0, "the page did not scroll: {scrolled}");
        driver.operate(Box::new(snap_to(
            screens::page_id(),
            RelativeOffset::START.into(),
        )));
        let top = driver
            .scroll_of(&screens::page_id())
            .map_or(0.0, |(_, offset)| offset.y);
        assert_eq!(top, 0.0, "the page is at its top");
    }

    #[test]
    fn each_copy_button_copies_its_whole_text() {
        let messages = pressed(&licences_at(DESKTOP.0), "Copy the licence", DESKTOP);
        assert!(
            matches!(
                messages.as_slice(),
                [Message::Wallet(WalletMsg::Copy(text))] if text == LICENCE
            ),
            "{messages:?}"
        );
        // The notices' button is below the licence, so the page is drawn tall
        // enough for it to be pressed.
        let tall = (DESKTOP.0, 20_000.0);
        let messages = pressed(&licences_at(DESKTOP.0), "Copy the notices", tall);
        assert!(
            matches!(
                messages.as_slice(),
                [Message::Wallet(WalletMsg::Copy(text))] if text == NOTICES
            ),
            "{messages:?}"
        );
    }
}
