//! W2, Receive: an account's destination, to give a payer, with the
//! library's own explanation of it and of the ledger address beside it, and
//! the destination as a QR code for a payer to scan (docs/SCREENS.md W2;
//! docs/DECISIONS.md D41).

use iced::border;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, qr_code, row};
use iced::{Alignment, Background, Color, Element, Length, Shadow};

use super::{account_choice, copy, frame, report};
use crate::app::{Message, Model, ReceivePage, ReportKey, WalletMsg, WalletPage};
use crate::theme::{color, radius, space as sp};
use crate::ui::{self, t, ty};

/// The side of a module, in logical pixels (docs/DECISIONS.md D41). A version 3
/// code with its quiet zone is 37 modules across, so at 8 it would be 296
/// pixels wide; a phone's card has 294 inside its border and padding at 360
/// wide. At 7 it is 259.
const MODULE: f32 = 7.0;

/// The quiet zone, four modules on every side as the standard asks
/// (ISO/IEC 18004): the widget draws two of them round the code, and the white
/// card adds the other two.
const MARGIN: f32 = 2.0 * MODULE;

pub fn view<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    r: &'a ReceivePage,
) -> Element<'a, Message> {
    let compact = model.compact();
    let accounts = model
        .wallet
        .as_ref()
        .map_or(&[][..], |w| w.accounts.as_slice());
    let mut body = vec![
        ui::card_for(
            compact,
            column![
                ui::section_label("Account"),
                account_choice(model, accounts.iter().map(|a| a.id), r.account, |id| {
                    WalletMsg::ReceiveFor(id).into()
                }),
            ]
            .spacing(sp::S12),
        )
        .into(),
    ];
    match &r.view {
        Some(view) => {
            let shown = view
                .destination
                .clone()
                .unwrap_or_else(|| view.account.hex());
            let text = t(shown.clone(), ty::MONO_WORD, color::TEXT_PRIMARY)
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill);
            // On a phone Copy sits beneath the destination, at its end, so the
            // destination keeps the whole width (docs/SCREENS.md, "Compact
            // layout").
            let line: Element<'a, Message> = if compact {
                column![
                    text,
                    container(copy(page, &shown, "Copy"))
                        .width(Length::Fill)
                        .align_x(Alignment::End),
                ]
                .spacing(sp::S8)
                .into()
            } else {
                row![text, copy(page, &shown, "Copy")]
                    .spacing(sp::S12)
                    .align_y(Alignment::Center)
                    .into()
            };
            body.push(
                ui::card_for(
                    compact,
                    column![
                        ui::section_label("Destination"),
                        line,
                        t(
                            format!(
                                "Base58 with its checksum: what a payer types or pastes. At key \
                                 index #{}.",
                                view.index
                            ),
                            ty::NOTE,
                            color::TEXT_MUTED,
                        ),
                    ]
                    .spacing(sp::S12),
                )
                .into(),
            );
            // The code is only ever the destination the page prints above: with
            // only the hex form known there is no code (docs/DECISIONS.md D41).
            if let Some(code) = &r.code {
                body.push(
                    ui::card_for(
                        compact,
                        column![
                            ui::section_label("QR code"),
                            code_card::<Message>(code),
                            ui::helper("The destination above as a QR code, for a payer to scan."),
                        ]
                        .spacing(sp::S12),
                    )
                    .into(),
                );
            }
            body.push(report::show_in(
                compact,
                page,
                ReportKey::Receive,
                report::receive(&view.text),
            ));
        }
        None if r.account.is_some() => {
            body.push(ui::helper("Reading the store…").into());
        }
        None => {}
    }
    frame(
        model,
        page,
        "Receive",
        "Give a payer an account's destination. No node is asked.",
        Vec::new(),
        body,
    )
}

/// The code on a white card, black on white whatever the theme is
/// (docs/DECISIONS.md D41). A dark theme's own text and background would draw
/// the code inverted, and most scanners do not read an inverted code. The card
/// is left-aligned, so its edges fall on whole pixels.
fn code_card<'a, M: 'a>(code: &'a qr_code::Data) -> Element<'a, M> {
    container(qr_code(code).cell_size(MODULE).style(code_style))
        .padding(MARGIN)
        .style(|_| container::Style {
            text_color: None,
            background: Some(Background::Color(Color::WHITE)),
            border: border::rounded(radius::R8),
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// The colours of the code: black modules on a white background.
fn code_style(_: &iced::Theme) -> qr_code::Style {
    qr_code::Style {
        cell: Color::BLACK,
        background: Color::WHITE,
    }
}

#[cfg(test)]
mod reader;

#[cfg(test)]
mod tests {
    use super::reader::{SIDE, decode, in_grid, placement};
    use super::*;
    use crate::app::{NARROWEST, Page, Screen};
    use crate::harness::{Driver, fits, texts};
    use tawara_wallet_core::preferences::Preferences;
    use tawara_wallet_core::view::AccountId;
    use tawara_wallet_core::{ReceiveView, sample};

    /// The window a code is drawn in on its own: larger than the card, which
    /// is 37 modules across with its quiet zone at version 3.
    const PANE: (u32, u32) = (300, 300);

    /// The model on the Receive page at `width`, with the first account's
    /// answer `view`, as the worker gives it.
    fn receiving(width: f32, view: ReceiveView) -> Model {
        let wallet = sample::wallet_view();
        let mut model = Model::new(Preferences::default(), None);
        model.width = width;
        model.screen = Screen::Wallet(WalletPage {
            page: Page::Receive(ReceivePage::answered(wallet.accounts[0].id, view)),
            ..WalletPage::default()
        });
        model.wallet = Some(wallet);
        model
    }

    /// The Receive page's own state in `model`.
    fn receive_page(model: &Model) -> &ReceivePage {
        match &model.screen {
            Screen::Wallet(WalletPage {
                page: Page::Receive(r),
                ..
            }) => r,
            _ => unreachable!("the model is on the Receive page"),
        }
    }

    /// The answer for an account whose destination is `destination`, which
    /// need not be one a tag renders to.
    fn view_of(account: AccountId, destination: &str) -> ReceiveView {
        ReceiveView {
            account,
            destination: Some(destination.to_owned()),
            index: 0,
            ledger_address: String::new(),
            text: String::new(),
        }
    }

    #[test]
    fn the_phone_receive_page_fits_at_360() {
        let model = receiving(NARROWEST, sample::receive_view());
        assert_eq!(fits(crate::screens::view(&model), NARROWEST), Ok(()));
    }

    #[test]
    fn the_code_card_is_37_modules_across() {
        // The card drawn alone at the top-left of a window, measured by the white
        // that runs along its middle row.
        let view = sample::receive_view();
        let page = ReceivePage::answered(view.account, view);
        let code = page.code.as_deref().expect("a destination has a code");
        let pixels = Driver::<()>::new(code_card::<()>(code), PANE).draw();
        let width = PANE.0 as usize;
        let middle = PANE.1 as usize / 2;
        let white = |x: usize| is_white(&pixels, width, x, middle);
        let across = (0..width).rev().find(|&x| white(x)).map_or(0, |x| x + 1);
        assert_eq!(
            across,
            SIDE * MODULE as usize,
            "the card is 37 modules across: a version 3 code and its quiet zone"
        );
    }

    #[test]
    fn the_phone_page_draws_the_whole_code_at_360_and_it_reads_back() {
        // The window-edge check cannot see a card that is given less room than it
        // needs: a container is clamped to the room it is given, so the code would
        // overflow its white card unseen. So the whole phone page is drawn at 360,
        // tall enough that nothing scrolls, and the card's white must still be 37
        // modules across, with the code inside it reading back as the
        // destination. The page's margins, the card's border and its padding are
        // all in that room, as they are drawn.
        const WIDTH: usize = NARROWEST as usize;
        const HEIGHT: usize = 1600;
        let view = sample::receive_view();
        let destination = view
            .destination
            .clone()
            .expect("the sample has a destination");
        let model = receiving(NARROWEST, view);
        assert!(model.compact(), "the phone layout is drawn at 360");
        let pixels =
            Driver::<Message>::new(crate::screens::view(&model), (WIDTH as u32, HEIGHT as u32))
                .draw();
        let white = |x: usize, y: usize| is_white(&pixels, WIDTH, x, y);

        // The longest run of white on any row, and where it starts.
        let (across, left) = (0..HEIGHT)
            .flat_map(|y| {
                let mut runs = Vec::new();
                let mut x = 0;
                while x < WIDTH {
                    let start = x;
                    while x < WIDTH && white(x, y) {
                        x += 1;
                    }
                    runs.push((x - start, start));
                    x += 1;
                }
                runs
            })
            .max()
            .expect("the page has rows");
        assert_eq!(
            across,
            SIDE * MODULE as usize,
            "the code's card is clamped to {across} pixels on a phone at 360"
        );

        // The card's top: the first row of the four white modules above the
        // code, down its middle.
        let quiet = 4 * MODULE as usize;
        let middle = left + across / 2;
        let top = (0..HEIGHT - quiet)
            .find(|&y| (y..y + quiet).all(|y| white(middle, y)))
            .expect("the card's quiet zone is drawn");
        let grid = modules(&pixels, WIDTH, (left, top));
        assert_eq!(decode(&grid), Ok(destination));
    }

    #[test]
    fn the_desktop_receive_page_fits_in_the_smallest_window() {
        let width = crate::WINDOW_MIN.0;
        let model = receiving(width, sample::receive_view());
        assert_eq!(fits(crate::screens::view(&model), width), Ok(()));
    }

    #[test]
    fn the_code_is_built_for_a_destination_and_not_for_the_hex_form_alone() {
        let view = sample::receive_view();
        let account = view.account;
        assert!(ReceivePage::answered(account, view.clone()).code.is_some());

        let hex_only = ReceiveView {
            destination: None,
            ..view
        };
        let model = receiving(crate::WINDOW_MIN.0, hex_only);
        assert!(receive_page(&model).code.is_none());
        let drawn = texts(crate::screens::view(&model), crate::WINDOW_MIN);
        assert!(
            drawn.iter().any(|(text, _)| *text == account.hex()),
            "the hex form is printed when nothing else is known"
        );
        assert!(
            !drawn.iter().any(|(text, _)| *text == "QR CODE"),
            "no code is labelled when there is no destination to encode"
        );
    }

    #[test]
    fn the_page_prints_the_destination_and_the_code_encodes_it() {
        let view = sample::receive_view();
        let destination = view
            .destination
            .clone()
            .expect("the sample has a destination");
        let model = receiving(crate::WINDOW_MIN.0, view);
        let printed = texts(crate::screens::view(&model), crate::WINDOW_MIN)
            .iter()
            .any(|(text, _)| *text == destination);
        assert!(printed, "the page does not print {destination}");
        let code = receive_page(&model)
            .code
            .as_deref()
            .expect("a destination has a code");
        assert_eq!(read_back(code), Ok(destination));
    }

    #[test]
    fn destinations_of_512_sampled_tags_have_a_code_at_version_3() {
        // Every destination is the 22-byte payload in Base58, so at most 31
        // characters: 31 Base58 digits reach 58^31, which passes 256^22, and 30
        // do not. That bound is checked here by logarithms; the tags below are a
        // sample that includes the extremes, the all-zero and the all-0xFF tags.
        let bits = |digits: f64| digits * 58f64.log2();
        assert!(bits(31.0) > 22.0 * 8.0 && bits(30.0) < 22.0 * 8.0);
        for n in 0..=u8::MAX {
            let mut ones = [0xFF; 20];
            ones[0] = n;
            for tag in [[n; 20], ones] {
                let account = AccountId::from_tag(tag);
                let destination = account.destination().expect("a tag renders");
                assert!(destination.len() <= 31, "{destination} is too long");
                assert!(
                    ReceivePage::answered(account, view_of(account, &destination))
                        .code
                        .is_some(),
                    "{destination} has no code"
                );
            }
        }
    }

    #[test]
    fn the_extreme_destinations_decode_to_themselves() {
        for tag in [[0x00; 20], [0xFF; 20]] {
            let account = AccountId::from_tag(tag);
            let destination = account.destination().expect("a tag renders");
            let page = ReceivePage::answered(account, view_of(account, &destination));
            let code = page.code.as_deref().expect("a destination has a code");
            assert_eq!(read_back(code), Ok(destination));
        }
    }

    #[test]
    fn a_destination_of_capitals_and_digits_alone_decodes_to_itself() {
        // The encoder chooses each segment's mode, and a text of capitals and
        // digits alone goes in the alphanumeric mode, not as bytes. Base58 has
        // such texts, and 31 of them still fit version 3 at Q.
        let account = sample::receive_view().account;
        let destination = "9U2VH66SWRKEJPPATCCFJMVJHVYFHT1";
        let page = ReceivePage::answered(account, view_of(account, destination));
        let code = page.code.as_deref().expect("the text has a code");
        assert_eq!(read_back(code), Ok(destination.to_owned()));
    }

    #[test]
    fn any_codeword_module_changed_makes_the_code_unreadable() {
        // Each block is checked against its error correction codewords, so one
        // module of the data or of the error correction changed is an error,
        // never another text; the seven remainder modules carry nothing.
        let view = sample::receive_view();
        let page = ReceivePage::answered(view.account, view);
        let code = page.code.as_deref().expect("a destination has a code");
        let grid = drawn(code);
        assert!(decode(&grid).is_ok());
        let codewords = placement();
        for &at in &codewords[..codewords.len() - 7] {
            let (r, c) = in_grid(at);
            let mut changed = grid.clone();
            changed[r][c] = !changed[r][c];
            let read = decode(&changed);
            assert!(
                read.is_err(),
                "the module at {r},{c} changed reads {read:?}"
            );
        }
    }

    /// The text the drawn code encodes, read back from its pixels: the card
    /// as the page draws it, at the top-left of a pane.
    fn read_back(code: &qr_code::Data) -> Result<String, String> {
        decode(&drawn(code))
    }

    /// The modules of `code` as the card draws it at the top-left of a pane.
    fn drawn(code: &qr_code::Data) -> Vec<Vec<bool>> {
        let pixels = Driver::<()>::new(code_card::<()>(code), PANE).draw();
        modules(&pixels, PANE.0 as usize, (0, 0))
    }

    /// Whether the pixel at `x`, `y` of a window `width` pixels wide is white.
    fn is_white(pixels: &[u8], width: usize, x: usize, y: usize) -> bool {
        pixels[(y * width + x) * 4..][..3].iter().all(|&v| v > 250)
    }

    /// The modules of a code and its quiet zone whose top-left corner is at
    /// `origin` in a window `width` pixels wide, dark where the pixel at each
    /// module's centre is.
    fn modules(pixels: &[u8], width: usize, origin: (usize, usize)) -> Vec<Vec<bool>> {
        let px = MODULE as usize;
        (0..SIDE)
            .map(|r| {
                (0..SIDE)
                    .map(|c| {
                        let y = origin.1 + r * px + px / 2;
                        let x = origin.0 + c * px + px / 2;
                        let at = (y * width + x) * 4;
                        let total: u32 = pixels[at..at + 3].iter().map(|&v| u32::from(v)).sum();
                        total < 3 * 128
                    })
                    .collect()
            })
            .collect()
    }
}
