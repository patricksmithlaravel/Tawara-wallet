//! W2, Receive: an account's destination, to give a payer, with the
//! library's own explanation of it and of the ledger address beside it
//! (docs/SCREENS.md W2).

use iced::widget::text::Wrapping;
use iced::widget::{column, row};
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
    let accounts = model
        .wallet
        .as_ref()
        .map_or(&[][..], |w| w.accounts.as_slice());
    let mut body = vec![
        ui::card(
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
            body.push(
                ui::card(
                    column![
                        ui::section_label("Destination"),
                        row![
                            t(shown.clone(), ty::MONO_WORD, color::TEXT_PRIMARY)
                                .wrapping(Wrapping::WordOrGlyph)
                                .width(Length::Fill),
                            copy(page, &shown, "Copy"),
                        ]
                        .spacing(sp::S12)
                        .align_y(Alignment::Center),
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
            body.push(report::show(
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
