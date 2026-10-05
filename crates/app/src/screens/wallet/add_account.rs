//! W3, Add an account: a sweep of the derived accounts this store's seed
//! makes, what the chain holds for each, and adding the ones it holds that
//! the store does not (docs/SCREENS.md W3). The library adds a derived
//! account only where the chain already shows it, so a new one is paid
//! first, at the destination the sweep shows for it, and added after.

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, text_input};
use iced::{Alignment, Element, Length, Padding};

use super::{action, copy, frame, report, short};
use crate::app::{AddAccountPage, Message, Model, ReportKey, WalletMsg, WalletPage};
use crate::icon::Icon;
use crate::theme::{self, color, radius, space as sp};
use crate::ui::{self, Size, t, ty};

pub fn view<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    a: &'a AddAccountPage,
) -> Element<'a, Message> {
    let idle = model.busy.is_none();
    let mut to = text_input("", &a.to)
        .font(ty::MONO.font)
        .size(ty::MONO.size)
        .padding(Padding::from([sp::S12, sp::S14]))
        .width(Length::Fixed(120.0))
        .style(theme::field(radius::R12));
    if idle {
        to = to
            .on_input(|s| WalletMsg::DiscoverTo(s).into())
            .on_submit(WalletMsg::Discover.into());
    }
    let mut body = vec![
        ui::card(
            column![
                t(
                    "Every account this store derives from its seed has a place on the chain \
                     once it is paid. Discover asks the node about the first ones, one request \
                     each, and writes nothing; an account the chain holds can then be added. \
                     To start a new one, pay the next account the sweep shows, then discover \
                     again and add it.",
                    ty::BODY_SMALL,
                    color::TEXT_SECONDARY,
                ),
                row![
                    t(
                        "Derived accounts 0 to",
                        ty::FORM_LABEL,
                        color::TEXT_SECONDARY
                    ),
                    to,
                    action(
                        model,
                        "Discover accounts",
                        theme::Button::Primary,
                        Size::Medium,
                        Some(Icon::Search),
                        WalletMsg::Discover,
                    ),
                ]
                .spacing(sp::S12)
                .align_y(Alignment::Center),
                ui::helper(format!(
                    "Up to {}: a request to the node each.",
                    tawara_wallet_core::DISCOVER_MAX_TO
                )),
            ]
            .spacing(sp::S16),
        )
        .into(),
    ];
    if let Some((ok, text)) = &a.added {
        body.push(report::show(
            page,
            ReportKey::Added,
            report::added(*ok, text),
        ));
    }
    if let Some((text, rows)) = &a.found {
        // The first derived account nothing holds yet: where a new one
        // receives.
        let next = rows
            .iter()
            .find(|r| !r.held && r.ledger_balance.is_none())
            .map(|r| r.account_index);
        let mut table = column![
            row![
                head("#", 1),
                head("Destination", 7),
                head("On the chain", 4),
                head("", 4),
            ]
            .spacing(sp::S12)
            .padding(Padding::from([sp::S10, 0.0])),
            ui::divider(),
        ];
        for (n, found) in rows.iter().enumerate() {
            let shown = found.id.destination().unwrap_or_else(|| found.id.hex());
            let ledger = found.ledger_balance.map_or_else(
                || "not found".to_owned(),
                |b| {
                    format!(
                        "{} {}",
                        ui::amount(b, model.prefs.unit),
                        ui::unit_name(model.prefs.unit)
                    )
                },
            );
            let what: Element<'a, Message> = if found.held {
                t("In this store", ty::TABLE_BODY, color::ACCENT).into()
            } else if found.ledger_balance.is_some() {
                action(
                    model,
                    "Add account",
                    theme::Button::Primary,
                    Size::Small,
                    Some(Icon::Plus),
                    WalletMsg::Add(found.account_index),
                )
            } else if next == Some(found.account_index) {
                column![
                    t(
                        "Next account: pay it here",
                        ty::TABLE_BODY,
                        color::TEXT_PRIMARY
                    ),
                    copy(page, &shown, "Copy"),
                ]
                .spacing(sp::S6)
                .into()
            } else {
                t("—", ty::TABLE_BODY, color::TEXT_MUTED).into()
            };
            table = table.push(
                row![
                    t(
                        found.account_index.to_string(),
                        ty::MONO,
                        color::TEXT_PRIMARY
                    )
                    .width(Length::FillPortion(1)),
                    column![
                        t(short(&shown), ty::TABLE_NAME, color::TEXT_PRIMARY),
                        t(shown.clone(), ty::MONO_TINY, color::TEXT_MUTED)
                            .wrapping(Wrapping::WordOrGlyph),
                    ]
                    .spacing(sp::S2)
                    .width(Length::FillPortion(7)),
                    t(ledger, ty::TABLE_AMOUNT, color::TEXT_PRIMARY)
                        .wrapping(Wrapping::WordOrGlyph)
                        .width(Length::FillPortion(4)),
                    container(what).width(Length::FillPortion(4)),
                ]
                .spacing(sp::S12)
                .align_y(Alignment::Center)
                .padding(Padding::from([sp::S12, 0.0])),
            );
            if n + 1 < rows.len() {
                table = table.push(ui::divider());
            }
        }
        body.push(
            ui::card(column![ui::section_label("What the node holds"), table].spacing(sp::S8))
                .into(),
        );
        body.push(report::show(
            page,
            ReportKey::Found,
            report::found(text, rows),
        ));
    }
    frame(
        model,
        page,
        "Add an account",
        "Find the accounts this store's seed derives, and add the ones the chain holds.",
        Vec::new(),
        body,
    )
}

fn head<'a>(label: &'static str, portion: u16) -> Element<'a, Message> {
    t(label, ty::TABLE_HEADER, color::TEXT_MUTED)
        .width(Length::FillPortion(portion))
        .into()
}
