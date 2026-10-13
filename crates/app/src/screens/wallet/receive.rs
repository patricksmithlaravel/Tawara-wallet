//! W2, Receive: an account's destination, to give a payer, with the
//! library's own explanation of it and of the ledger address beside it
//! (docs/SCREENS.md W2).

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

use super::{account_choice, copy, frame, report};
use crate::app::{Message, Model, ReceivePage, ReportKey, WalletMsg, WalletPage};
use crate::theme::{color, space as sp};
use crate::ui::{self, t, ty};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{NARROWEST, Page, Screen};
    use crate::harness::fits;
    use tawara_wallet_core::preferences::Preferences;
    use tawara_wallet_core::sample;

    #[test]
    fn the_phone_receive_page_fits_at_360() {
        let wallet = sample::wallet_view();
        let mut model = Model::new(Preferences::default(), None);
        model.width = NARROWEST;
        model.screen = Screen::Wallet(WalletPage {
            page: Page::Receive(ReceivePage {
                account: Some(wallet.accounts[0].id),
                view: Some(sample::receive_view()),
            }),
            ..WalletPage::default()
        });
        model.wallet = Some(wallet);
        assert_eq!(fits(crate::screens::view(&model), NARROWEST), Ok(()));
    }
}
