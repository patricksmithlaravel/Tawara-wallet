//! W7, an account: its figures, and the panel for its state with the
//! actions that state allows (docs/PLAN.md section 4.9; docs/SCREENS.md,
//! "The states that fail closed"). Nothing on this page moves a key index:
//! a diverged account's remedy is the account-recovery screen, reached on
//! purpose (docs/DECISIONS.md D19, D27 item 3).

use iced::widget::text::Wrapping;
use iced::widget::{column, row};
use iced::{Alignment, Element, Length};
use tawara_wallet_core::view::{AccountRow, AccountState, DivergenceKind, ReservationState};

use super::{action, copy, destination, figure, frame, kind, name, report, row_of, status};
use crate::app::{AccountPage, Message, Model, ReportKey, To, WalletMsg, WalletPage};
use crate::icon::Icon;
use crate::theme::{self, color, space as sp};
use crate::ui::{self, Size, t, ty};

pub fn view<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    a: &'a AccountPage,
) -> Element<'a, Message> {
    let Some(account) = row_of(model, a.account) else {
        return frame(
            model,
            page,
            "Account",
            "This store holds no such account.",
            Vec::new(),
            Vec::new(),
        );
    };
    let unit = model.prefs.unit;
    let shown = destination(account.id);
    let (word, ink) = status(&account.state);
    let figures = ui::card(
        column![
            row![
                ui::section_label("Destination"),
                iced::widget::space().width(Length::Fill),
                copy(page, &shown, "Copy"),
            ]
            .align_y(Alignment::Center),
            t(shown.clone(), ty::MONO_WORD, color::TEXT_PRIMARY).wrapping(Wrapping::WordOrGlyph),
            ui::divider(),
            figure(
                "Balance",
                account.state.balance().map_or_else(
                    || "not known".to_owned(),
                    |b| format!("{} {}", ui::amount(b, unit), ui::unit_name(unit))
                )
            ),
            figure("Next key", format!("#{}", account.index)),
            figure("Kind", kind(account).to_owned()),
            row![
                t("State", ty::BODY_SMALL, color::TEXT_SECONDARY).width(Length::Fill),
                ui::dot(ink, 6.0),
                t(word, ty::TABLE_BODY, ink),
            ]
            .spacing(sp::S6)
            .align_y(Alignment::Center),
        ]
        .spacing(sp::S12),
    );
    let mut body = vec![figures.into(), panel(model, page, account)];
    if let Some((done, text)) = &a.report {
        body.push(report::show(
            page,
            ReportKey::Done,
            report::done(*done, account, text),
        ));
    }
    // The name is part of a Base58 destination, so it is not set in the
    // title's capitals.
    frame(
        model,
        page,
        "Account",
        format!("{} · {}", name(account.id), kind(account)),
        vec![action(
            model,
            "Check now",
            theme::Button::Secondary,
            Size::Header,
            Some(Icon::Refresh),
            WalletMsg::Check(account.id),
        )],
        body,
    )
}

/// The panel for the account's state (section 4.9): what it means, and
/// the actions it allows.
fn panel<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    account: &'a AccountRow,
) -> Element<'a, Message> {
    let id = account.id;
    let button = |label: &'a str, variant, msg: WalletMsg| {
        action(model, label, variant, Size::Medium, None, msg)
    };
    let (callout, buttons): (Element<'a, Message>, Vec<Element<'a, Message>>) = match &account.state
    {
        AccountState::InSync { .. } if account.spendable => (
            ui::accent_callout(
                Icon::ShieldCheck,
                "Reconciled",
                "The chain holds this account at the key this store expects.",
            ),
            vec![
                button(
                    "Send from it",
                    theme::Button::Primary,
                    WalletMsg::Open(To::Send(Some(id))),
                ),
                button(
                    "Receive",
                    theme::Button::Secondary,
                    WalletMsg::Open(To::Receive(Some(id))),
                ),
            ],
        ),
        // In sync now, and set aside when the wallet opened: it spends only
        // once a refresh opens the wallet with it.
        AccountState::InSync { .. } => (
            ui::accent_callout(
                Icon::ShieldCheck,
                "Reconciled, not yet spendable",
                "The chain holds this account at the key this store expects. It was set aside \
                 when the wallet opened, so it spends again once Refresh on the wallet's page \
                 opens the wallet with it.",
            ),
            vec![button(
                "Receive",
                theme::Button::Secondary,
                WalletMsg::Open(To::Receive(Some(id))),
            )],
        ),
        AccountState::SpendOutstanding { reservation, .. } => {
            let why = match reservation {
                ReservationState::Live => {
                    "A spend from this account is signed and waiting to land. Settle once the \
                     chain shows it landed; until then the account does not spend again. If \
                     the signed bytes were lost, re-sign the same spend, or submit the bytes \
                     saved from the sent page."
                }
                ReservationState::Dead {
                    balance_moved: true,
                    ..
                } => {
                    "This reservation can no longer be accepted: the balance it was built on \
                     has moved. Re-signing would reproduce bytes the ledger refuses. Settle once \
                     the chain shows how it was resolved."
                }
                ReservationState::Dead { .. } => {
                    "This reservation can no longer be accepted: its block-to-live has passed. \
                     Re-signing would reproduce bytes the ledger refuses. Settle once the chain \
                     shows how it was resolved."
                }
                ReservationState::Unrecorded => {
                    "A spend from this account is reserved, and the store records no figures \
                     for it (a store from before they were recorded). Check now for the wallet \
                     library's report; settle once the chain shows it landed."
                }
                ReservationState::Unclassified => {
                    "A spend from this account is reserved, and its expiry could not be read. \
                     Check now for the wallet library's report on it; settle once the chain \
                     shows it landed."
                }
            };
            (
                ui::warning_callout("Reserved and not settled", why),
                vec![
                    button(
                        "Settle",
                        theme::Button::WarningPrimary,
                        WalletMsg::Settle(id),
                    ),
                    button(
                        "Re-sign",
                        theme::Button::Secondary,
                        WalletMsg::Open(To::Resign(id)),
                    ),
                    button(
                        "Submit saved artifact",
                        theme::Button::Secondary,
                        WalletMsg::Open(To::Submit),
                    ),
                ],
            )
        }
        AccountState::SpendLanded { .. } => (
            ui::warning_callout(
                "The spend landed",
                "The chain shows the change key: the spend from this account landed. Settle \
                 records it and frees the account to spend again.",
            ),
            vec![button(
                "Settle",
                theme::Button::WarningPrimary,
                WalletMsg::Settle(id),
            )],
        ),
        AccountState::Diverged {
            kind,
            report: text,
            advance_to,
        } => {
            let parts = column![report::show(
                page,
                ReportKey::Diverged,
                report::diverged(*kind, text)
            )]
            .spacing(sp::S12);
            // Account recovery is reached on purpose, from here or from
            // Settings, and moves nothing until the person types the index
            // and confirms (docs/DECISIONS.md D19).
            let recover = || {
                button(
                    "Account recovery",
                    theme::Button::WarningTonal,
                    WalletMsg::Open(To::Recovery(Some(id))),
                )
            };
            (
                parts.into(),
                match kind {
                    DivergenceKind::NotFound => vec![button(
                        "Receive to it again",
                        theme::Button::Secondary,
                        WalletMsg::Open(To::Receive(Some(id))),
                    )],
                    _ if advance_to.is_some() => vec![recover()],
                    DivergenceKind::Unlocated => vec![recover()],
                    _ => Vec::new(),
                },
            )
        }
        AccountState::NotReconciled => (
            ui::warning_callout(
                "Not reconciled",
                "No node has been asked about this account, so its balance and state are not \
                 known and it cannot spend. Choose a node, or Refresh once it answers.",
            ),
            Vec::new(),
        ),
    };
    let mut panel = column![callout].spacing(sp::S16);
    if !buttons.is_empty() {
        let mut line = row![].spacing(sp::S10);
        for b in buttons {
            line = line.push(b);
        }
        panel = panel.push(line);
    }
    ui::card(panel).into()
}
