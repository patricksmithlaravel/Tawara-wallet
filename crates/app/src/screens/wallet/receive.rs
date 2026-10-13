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
mod tests {
    use super::*;
    use crate::app::{NARROWEST, Page, Screen};
    use crate::harness::{Driver, fits, texts};
    use tawara_wallet_core::preferences::Preferences;
    use tawara_wallet_core::view::AccountId;
    use tawara_wallet_core::{ReceiveView, sample};

    /// The window a code is drawn in: larger than the card, which is 37 modules
    /// across with its quiet zone at version 3.
    const PANE: (u32, u32) = (300, 300);
    const SIDE: usize = 37;

    /// The format information's mask, which the standard XORs into the BCH
    /// code of the EC level and mask (ISO/IEC 18004, 7.9.4).
    const FORMAT_MASK: u32 = 0x5412;

    /// The alphanumeric mode's characters, by their values.
    const ALPHANUMERIC: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

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

    #[test]
    fn the_phone_receive_page_fits_at_360() {
        let model = receiving(NARROWEST, sample::receive_view());
        assert_eq!(fits(crate::screens::view(&model), NARROWEST), Ok(()));
    }

    #[test]
    fn the_code_card_fits_inside_a_phone_card_at_360() {
        // The page's 16 on each side, the card's 1-pixel border on each side and
        // its 16 of padding on each side leave 294 of the 360 pixels for the card's
        // content. The window-edge check cannot see a card wider than that: a
        // container is clamped to the room it is given, so its own padding would
        // overflow unseen. So the card is drawn alone at the top-left of a window,
        // and its width is the white that runs along its middle row.
        const PHONE_CARD_CONTENT: usize = 360 - 2 * (16 + 1 + 16);
        const WINDOW: usize = 400;
        let view = sample::receive_view();
        let page = ReceivePage::answered(view.account, view);
        let code = page.code.as_deref().expect("a destination has a code");
        let pixels =
            Driver::<()>::new(code_card::<()>(code), (WINDOW as u32, WINDOW as u32)).draw();
        let middle = WINDOW / 2;
        let white = |x: usize| {
            pixels[(middle * WINDOW + x) * 4..][..3]
                .iter()
                .all(|&v| v > 250)
        };
        let width = (0..WINDOW).rev().find(|&x| white(x)).map_or(0, |x| x + 1);
        assert_eq!(
            width,
            37 * MODULE as usize,
            "the card is 37 modules across: a version 3 code and its quiet zone"
        );
        assert!(
            width <= PHONE_CARD_CONTENT,
            "the card is {width} pixels wide, and a phone's card holds {PHONE_CARD_CONTENT}"
        );
    }

    #[test]
    fn the_code_encodes_the_destination_and_nothing_else() {
        let view = sample::receive_view();
        assert_eq!(
            ReceivePage::code_text(&view),
            view.destination.as_deref(),
            "the text encoded is the destination the page prints"
        );
        let hex_only = ReceiveView {
            destination: None,
            ..view
        };
        assert_eq!(
            ReceivePage::code_text(&hex_only),
            None,
            "with only the hex form known there is no text to encode"
        );
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
    fn every_destination_has_a_code_at_version_3() {
        // Every destination is the 22-byte payload in Base58, so at most 31
        // characters. The extremes are the all-zero and the all-0xFF tags, and the
        // checksum does not change the length.
        for n in 0..=u8::MAX {
            let mut ones = [0xFF; 20];
            ones[0] = n;
            for tag in [[n; 20], ones] {
                let account = AccountId::from_tag(tag);
                let destination = account.destination().expect("a tag renders");
                assert!(destination.len() <= 31, "{destination} is too long");
                let view = ReceiveView {
                    account,
                    destination: Some(destination.clone()),
                    index: 0,
                    ledger_address: String::new(),
                    text: String::new(),
                };
                assert!(
                    ReceivePage::answered(account, view).code.is_some(),
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
            let view = ReceiveView {
                account,
                destination: Some(destination.clone()),
                index: 0,
                ledger_address: String::new(),
                text: String::new(),
            };
            let page = ReceivePage::answered(account, view);
            let code = page.code.as_deref().expect("a destination has a code");
            assert_eq!(read_back(code), Ok(destination));
        }
    }

    #[test]
    fn the_format_information_is_the_standards() {
        // ISO/IEC 18004's table: error correction M with mask 0, and L with mask 0.
        assert_eq!(format_code(0b00000), 0b101010000010010);
        assert_eq!(format_code(0b01000), 0b111011111000100);
    }

    /// The text the drawn code encodes, read back from its pixels: the card
    /// as the page draws it, at the top-left of a pane.
    fn read_back(code: &qr_code::Data) -> Result<String, String> {
        let pixels = Driver::<()>::new(code_card::<()>(code), PANE).draw();
        decode(&modules(&pixels, PANE.0 as usize, SIDE))
    }

    /// The modules of a `side` by `side` grid at the top-left of a window
    /// `width` pixels wide, dark where the pixel at each module's centre is.
    fn modules(pixels: &[u8], width: usize, side: usize) -> Vec<Vec<bool>> {
        let px = MODULE as usize;
        (0..side)
            .map(|r| {
                (0..side)
                    .map(|c| {
                        let at = ((r * px + px / 2) * width + c * px + px / 2) * 4;
                        let total: u32 = pixels[at..at + 3].iter().map(|&v| u32::from(v)).sum();
                        total < 3 * 128
                    })
                    .collect()
            })
            .collect()
    }

    /// The text a code encodes, read from its modules with the quiet zone
    /// included, by the standard's own layout (ISO/IEC 18004). It reads
    /// version 3 at error correction Q and nothing else. The code under test
    /// is drawn, not damaged, so nothing is corrected: every format bit, every
    /// block and every segment must be where the standard puts it.
    fn decode(grid: &[Vec<bool>]) -> Result<String, String> {
        const QUIET: usize = 4;
        const W: usize = 29;
        let side = grid.len();
        if side != W + 2 * QUIET || grid.iter().any(|row| row.len() != side) {
            return Err(format!("{side} modules across, not {}", W + 2 * QUIET));
        }
        for (r, row) in grid.iter().enumerate() {
            for (c, &dark) in row.iter().enumerate() {
                let inside =
                    (QUIET..side - QUIET).contains(&r) && (QUIET..side - QUIET).contains(&c);
                if dark && !inside {
                    return Err(format!("a dark module in the quiet zone at {r},{c}"));
                }
            }
        }
        let module = |r: usize, c: usize| grid[r + QUIET][c + QUIET];

        // The format information, twice: round the top-left finder, and split
        // between the bottom-left and the top-right. The first bit read is the
        // most significant.
        let first: [(usize, usize); 15] = [
            (8, 0),
            (8, 1),
            (8, 2),
            (8, 3),
            (8, 4),
            (8, 5),
            (8, 7),
            (8, 8),
            (7, 8),
            (5, 8),
            (4, 8),
            (3, 8),
            (2, 8),
            (1, 8),
            (0, 8),
        ];
        let second: Vec<(usize, usize)> = (W - 7..W)
            .rev()
            .map(|r| (r, 8))
            .chain((W - 8..W).map(|c| (8, c)))
            .collect();
        let read = |at: &[(usize, usize)]| {
            at.iter()
                .fold(0u32, |bits, &(r, c)| (bits << 1) | u32::from(module(r, c)))
        };
        let format = read(&first);
        if read(&second) != format {
            return Err("the two copies of the format information differ".to_owned());
        }
        let data = (0..32u32)
            .find(|&d| format_code(d) == format)
            .ok_or_else(|| format!("{format:015b} is no format information"))?;
        if data >> 3 != 0b11 {
            return Err(format!(
                "error correction bits {:02b}, not 11 (Q)",
                data >> 3
            ));
        }
        let mask = (data & 7) as usize;

        // The data and error correction codewords, read in the zigzag from the
        // bottom-right corner, skipping the function patterns and the timing
        // column. Each module is unmasked as it is read.
        let function = |r: usize, c: usize| {
            (r <= 8 && (c <= 8 || c >= W - 8))
                || (r >= W - 8 && c <= 8)
                || r == 6
                || c == 6
                || ((20..=24).contains(&r) && (20..=24).contains(&c))
        };
        let mut stream = Vec::new();
        let mut col = W - 1;
        let mut upward = true;
        loop {
            if col == 6 {
                col = 5;
            }
            let rows: Vec<usize> = if upward {
                (0..W).rev().collect()
            } else {
                (0..W).collect()
            };
            for r in rows {
                for c in [col, col - 1] {
                    if !function(r, c) {
                        stream.push(module(r, c) ^ mask_bit(mask, r, c));
                    }
                }
            }
            upward = !upward;
            if col <= 2 {
                break;
            }
            col -= 2;
        }

        // Version 3 at Q has two blocks of 17 data codewords, interleaved one
        // from each block in turn; the message is the first block then the second.
        if stream.len() < 34 * 8 {
            return Err(format!("{} data modules, not 272", stream.len()));
        }
        let mut message = [0u8; 34];
        for k in 0..34 {
            let byte = stream[8 * k..8 * k + 8]
                .iter()
                .fold(0u8, |v, &bit| (v << 1) | u8::from(bit));
            message[(k % 2) * 17 + k / 2] = byte;
        }

        let mut reader = Reader {
            bytes: &message,
            at: 0,
        };
        let mut text = Vec::new();
        loop {
            match reader.take(4)? {
                0b0000 => break,
                0b0001 => {
                    // Numeric: three digits in ten bits, then two in seven or one in four.
                    let mut left = reader.take(10)?;
                    while left >= 3 {
                        let v = reader.take(10)?;
                        if v > 999 {
                            return Err(format!("{v} is no three digits"));
                        }
                        text.extend(format!("{v:03}").bytes());
                        left -= 3;
                    }
                    if left > 0 {
                        let width = if left == 2 { 7 } else { 4 };
                        let v = reader.take(width)?;
                        if v >= 10usize.pow(left as u32) {
                            return Err(format!("{v} is no {left} digits"));
                        }
                        text.extend(format!("{v:0left$}").bytes());
                    }
                }
                0b0010 => {
                    // Alphanumeric: two characters in eleven bits, then one in six.
                    let mut left = reader.take(9)?;
                    while left >= 2 {
                        let v = reader.take(11)?;
                        text.push(alphanumeric(v / 45)?);
                        text.push(alphanumeric(v % 45)?);
                        left -= 2;
                    }
                    if left == 1 {
                        let v = reader.take(6)?;
                        text.push(alphanumeric(v)?);
                    }
                }
                0b0100 => {
                    // Byte: eight bits a character.
                    let n = reader.take(8)?;
                    for _ in 0..n {
                        text.push(reader.take(8)? as u8);
                    }
                }
                mode => return Err(format!("mode {mode:04b} is none of the three read here")),
            }
        }
        String::from_utf8(text).map_err(|e| e.to_string())
    }

    /// The bits of a message, read most significant first.
    struct Reader<'a> {
        bytes: &'a [u8],
        at: usize,
    }

    impl Reader<'_> {
        fn take(&mut self, n: usize) -> Result<usize, String> {
            if self.at + n > 8 * self.bytes.len() {
                return Err("the message ends inside a segment".to_owned());
            }
            let mut v = 0;
            for i in self.at..self.at + n {
                v = (v << 1) | usize::from((self.bytes[i / 8] >> (7 - i % 8)) & 1);
            }
            self.at += n;
            Ok(v)
        }
    }

    fn alphanumeric(i: usize) -> Result<u8, String> {
        ALPHANUMERIC
            .get(i)
            .copied()
            .ok_or_else(|| format!("{i} is no alphanumeric character"))
    }

    /// The format information the standard gives `data`, its two error
    /// correction bits and three mask bits: a BCH code of fifteen bits, masked
    /// with [`FORMAT_MASK`].
    fn format_code(data: u32) -> u32 {
        let mut rem = data << 10;
        for bit in (10..15).rev() {
            if rem & (1 << bit) != 0 {
                rem ^= 0x537 << (bit - 10);
            }
        }
        ((data << 10) | rem) ^ FORMAT_MASK
    }

    /// Whether the module at `r`, `c` is inverted by `mask`, by the standard's
    /// eight patterns (ISO/IEC 18004, 7.8.2).
    fn mask_bit(mask: usize, r: usize, c: usize) -> bool {
        match mask {
            0 => (r + c).is_multiple_of(2),
            1 => r.is_multiple_of(2),
            2 => c.is_multiple_of(3),
            3 => (r + c).is_multiple_of(3),
            4 => (r / 2 + c / 3).is_multiple_of(2),
            5 => (r * c) % 2 + (r * c) % 3 == 0,
            6 => ((r * c) % 2 + (r * c) % 3).is_multiple_of(2),
            _ => ((r + c) % 2 + (r * c) % 3).is_multiple_of(2),
        }
    }
}
