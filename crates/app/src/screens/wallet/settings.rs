//! W11, Settings (rendering 05): the accounts and their one-time keys, the
//! keystore, the node, the display and what this build is
//! (docs/SCREENS.md W11).
//!
//! What the rendering offers and the wallet does not: "Advance to #N" on a
//! paused account is "Review", which opens account recovery (W12) and
//! never moves an index from here (docs/DECISIONS.md D19, D27 item 3);
//! "Recovery phrase: Show" and "Encrypted keystore file: Export" are not
//! built, since the phrase is shown once and a copy of the store is the
//! rollback that reuses a key (docs/PLAN.md sections 4.3, 4.5); "Password:
//! Change", the network's name and "Transaction search" need the library
//! first (D27, item 4); "Theme" is not built, since there is one (item 8).

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, space, text_input};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::preferences::{AmountUnit, IDLE_LOCK_MINUTES};
use tawara_wallet_core::view::{AccountRow, AccountState, DivergenceKind};

use super::{action, frame, kind, name, pair, status};
use crate::app::{Message, Model, SettingsPage, To, WalletMsg, WalletPage};
use crate::icon::{self, Icon};
use crate::theme::{self, color, radius, space as sp};
use crate::ui::{self, Size, t, ty};

pub fn view<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    s: &'a SettingsPage,
) -> Element<'a, Message> {
    let mut body = Vec::new();
    if let Some(wallet) = &model.wallet {
        body.push(accounts(model, &wallet.accounts));
    }
    body.push(pair(model, keystore(model), node(model, s), 1, 1));
    body.push(pair(model, display(model), about(), 1, 1));
    frame(
        model,
        page,
        "Settings",
        "Keystore, one-time keys and the node this wallet talks to",
        Vec::new(),
        body,
    )
}

/// The key index the chain shows for an account, when its state says one:
/// the same as the store's when they agree, the key that signed while a
/// spend is outstanding, the change key once it landed, and where a
/// divergence found it.
fn on_ledger(account: &AccountRow) -> Option<u32> {
    match &account.state {
        AccountState::InSync { .. } => Some(account.index),
        AccountState::SpendOutstanding { spent_index, .. } => Some(*spent_index),
        AccountState::SpendLanded { settled_index, .. } => Some(*settled_index),
        AccountState::Diverged {
            kind: DivergenceKind::Ahead { gap },
            ..
        } => account.index.checked_add(*gap),
        AccountState::Diverged {
            kind: DivergenceKind::Behind { gap },
            ..
        } => account.index.checked_sub(*gap),
        AccountState::Diverged { .. } | AccountState::NotReconciled => None,
    }
}

/// Accounts and one-time keys (05): each account's key on this device and
/// on the ledger, compared at every unlock and refresh.
fn accounts<'a>(model: &'a Model, rows: &'a [AccountRow]) -> Element<'a, Message> {
    let head = |label: &'static str, portion: u16| {
        t(label, ty::TABLE_HEADER, color::TEXT_MUTED).width(Length::FillPortion(portion))
    };
    let mut table = column![
        row![
            head("Account", 4),
            head("Tag (hex)", 5),
            head("On this device", 3),
            head("On ledger", 3),
            head("Status", 4),
            space().width(Length::Fixed(96.0)),
        ]
        .spacing(sp::S12)
        .padding(Padding::from([sp::S10, sp::S12])),
        ui::divider(),
    ]
    .spacing(sp::S4);
    for account in rows {
        let (word, ink) = status(&account.state);
        let paused = matches!(account.state, AccountState::Diverged { .. });
        let ledger = on_ledger(account).map_or_else(|| "—".to_owned(), |i| format!("#{i}"));
        let hex = account.id.hex();
        let tag = format!("{}…{}", &hex[..10], &hex[hex.len() - 8..]);
        let act: Element<'a, Message> = if paused {
            action(
                model,
                "Review",
                theme::Button::WarningTonal,
                Size::Small,
                None,
                WalletMsg::Open(To::Recovery(Some(account.id))),
            )
        } else {
            space().into()
        };
        let line = row![
            column![
                t(name(account.id), ty::TABLE_NAME, color::TEXT_PRIMARY),
                t(kind(account), ty::TINY, color::TEXT_MUTED),
            ]
            .spacing(sp::S2)
            .width(Length::FillPortion(4)),
            t(tag, ty::MONO_SMALL, color::TEXT_SECONDARY)
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::FillPortion(5)),
            t(format!("#{}", account.index), ty::MONO, color::TEXT_PRIMARY)
                .width(Length::FillPortion(3)),
            t(
                ledger,
                ty::MONO,
                if paused {
                    color::WARNING
                } else {
                    color::TEXT_PRIMARY
                }
            )
            .width(Length::FillPortion(3)),
            t(word, ty::TABLE_BODY, ink).width(Length::FillPortion(4)),
            container(act)
                .width(Length::Fixed(96.0))
                .align_x(Alignment::End),
        ]
        .spacing(sp::S12)
        .align_y(Alignment::Center);
        let boxed = container(line)
            .padding(Padding::from([sp::S12, sp::S12]))
            .width(Length::Fill);
        table = table.push(if paused {
            boxed.style(theme::row_warning)
        } else {
            boxed
        });
    }
    ui::card(
        column![
            row![
                column![
                    t(
                        "Accounts & one-time keys",
                        ty::CARD_TITLE,
                        color::TEXT_PRIMARY
                    ),
                    t(
                        "Each stored key index is compared with the ledger at every unlock and \
                         refresh. A paused account is never moved from here: Review opens \
                         account recovery, which shows every account's report first.",
                        ty::NOTE,
                        color::TEXT_MUTED
                    ),
                ]
                .spacing(sp::S4)
                .width(Length::Fill),
                action(
                    model,
                    "Check again",
                    theme::Button::Secondary,
                    Size::Small,
                    Some(Icon::Refresh),
                    Message::Refresh,
                ),
            ]
            .spacing(sp::S16)
            .align_y(Alignment::Center),
            table,
        ]
        .spacing(sp::S12),
    )
    .into()
}

fn card_head<'a>(lead: Icon, title: &'a str, subtitle: String) -> Element<'a, Message> {
    row![
        container(icon::icon(lead, 20.0, 2.0, color::TEXT_ON_ACCENT))
            .center(Length::Fixed(40.0))
            .style(theme::icon_tile(color::ACCENT, color::TEXT_ON_ACCENT)),
        column![
            t(title, ty::CARD_TITLE, color::TEXT_PRIMARY),
            t(subtitle, ty::NOTE, color::TEXT_MUTED),
        ]
        .spacing(sp::S2),
    ]
    .spacing(sp::S14)
    .align_y(Alignment::Center)
    .into()
}

/// A labelled line of a settings card, its control or value on the right.
fn setting<'a>(
    label: &'a str,
    note: Option<String>,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut words = column![t(label, ty::BODY, color::TEXT_PRIMARY)].spacing(sp::S2);
    if let Some(note) = note {
        words = words.push(t(note, ty::NOTE, color::TEXT_MUTED).wrapping(Wrapping::WordOrGlyph));
    }
    row![words.width(Length::Fill), control]
        .spacing(sp::S16)
        .align_y(Alignment::Center)
        .into()
}

/// The keystore (05): where it is, how its key is made, and the auto-lock.
fn keystore(model: &Model) -> Element<'_, Message> {
    let kdf = tawara_wallet_core::NEW_STORE_KDF;
    let mut periods = row![].spacing(sp::S4);
    for m in IDLE_LOCK_MINUTES {
        periods = periods.push(ui::button_with(
            match m {
                1 => "1 min",
                2 => "2 min",
                5 => "5 min",
                10 => "10 min",
                _ => "15 min",
            },
            theme::Button::Segment {
                selected: model.prefs.idle_lock.as_secs() == m * 60,
            },
            Size::Small,
            None,
            Some(WalletMsg::AutoLock(m).into()),
        ));
    }
    let dir = model
        .wallet
        .as_ref()
        .map_or_else(String::new, |w| w.dir.display().to_string());
    ui::card(
        column![
            card_head(
                Icon::Lock,
                "Keystore",
                "Argon2id and ChaCha20-Poly1305 · on this device only".to_owned()
            ),
            setting("Folder", Some(dir), space().width(Length::Shrink).into()),
            ui::divider(),
            setting(
                "Key derivation",
                Some(format!(
                    "Argon2id at {} MiB over {} passes and {} lane, for a new store; a store \
                     made elsewhere keeps its own.",
                    kdf.memory_kib / 1024,
                    kdf.passes,
                    kdf.lanes
                )),
                space().width(Length::Shrink).into()
            ),
            ui::divider(),
            setting(
                "Auto-lock",
                Some("After this long with nothing done".to_owned()),
                periods.into()
            ),
            ui::divider(),
            ui::helper(
                "The recovery phrase is shown once, when the store is made, and the store is \
                 never exported: a copy of it is the rollback that signs with a key twice.",
            ),
        ]
        .spacing(sp::S14),
    )
    .into()
}

/// The node (05): its address, whether it answers, and its tip.
fn node<'a>(model: &'a Model, s: &'a SettingsPage) -> Element<'a, Message> {
    let idle = model.busy.is_none();
    let mut field = text_input("https://", &s.node)
        .font(ty::MONO.font)
        .size(ty::MONO.size)
        .padding(Padding::from([sp::S12, sp::S14]))
        .width(Length::Fill)
        .style(theme::field(radius::R12));
    if idle {
        field = field
            .on_input(|u| WalletMsg::NodeTyped(u).into())
            .on_submit(WalletMsg::SaveNode.into());
    }
    let (state, ink) = match (&model.node.url, &model.node.tip, &model.node.error) {
        (None, ..) => ("None chosen".to_owned(), color::TEXT_MUTED),
        (Some(_), _, Some(_)) => ("Not answering".to_owned(), color::WARNING),
        (Some(_), Some((_, took)), None) => (
            format!("Answering · {} ms", took.as_millis()),
            color::ACCENT,
        ),
        (Some(_), None, None) => ("Not asked yet".to_owned(), color::TEXT_MUTED),
    };
    let tip = model
        .node
        .tip
        .map_or_else(|| "—".to_owned(), |(tip, _)| ui::group(&tip.to_string()));
    let tile = |label: &'static str, value: String, ink: iced::Color| {
        container(
            column![
                t(label, ty::NOTE, color::TEXT_MUTED),
                t(value, ty::MONO, ink).wrapping(Wrapping::WordOrGlyph),
            ]
            .spacing(sp::S4),
        )
        .padding(sp::S14)
        .width(Length::Fill)
        .style(theme::field_box)
    };
    ui::card(
        column![
            card_head(
                Icon::Server,
                "Node",
                "Balances, tag lookups, submits and the index".to_owned()
            ),
            t("Address", ty::FORM_LABEL, color::TEXT_SECONDARY),
            row![
                field,
                action(
                    model,
                    "Save",
                    theme::Button::Primary,
                    Size::Medium,
                    None,
                    WalletMsg::SaveNode,
                ),
            ]
            .spacing(sp::S10)
            .align_y(Alignment::Center),
            ui::helper("https:// only, or http:// to this computer. There is no default node."),
            row![
                tile("Status", state, ink),
                tile("Tip", tip, color::TEXT_PRIMARY)
            ]
            .spacing(sp::S12),
            action(
                model,
                "Check now",
                theme::Button::Secondary,
                Size::Small,
                Some(Icon::Refresh),
                WalletMsg::CheckNode,
            ),
        ]
        .spacing(sp::S12),
    )
    .into()
}

/// Display (05): the unit amounts are shown in.
fn display(model: &Model) -> Element<'_, Message> {
    let unit = |label: &'static str, u: AmountUnit| {
        ui::button_with(
            label,
            theme::Button::Segment {
                selected: model.prefs.unit == u,
            },
            Size::Small,
            None,
            Some(WalletMsg::Unit(u).into()),
        )
    };
    ui::card(
        column![
            t("Display", ty::CARD_TITLE, color::TEXT_PRIMARY),
            setting(
                "Amounts",
                Some("Typed amounts are MCM whichever is shown.".to_owned()),
                row![
                    unit("MCM", AmountUnit::Mcm),
                    unit("nanoMCM", AmountUnit::NanoMcm)
                ]
                .spacing(sp::S4)
                .into(),
            ),
        ]
        .spacing(sp::S16),
    )
    .into()
}

/// About (05): what this build is.
fn about<'a>() -> Element<'a, Message> {
    let fact = |label: &'static str, value: String| {
        row![
            t(label, ty::BODY_SMALL, color::TEXT_MUTED).width(Length::FillPortion(2)),
            t(value, ty::BODY_SMALL, color::TEXT_PRIMARY)
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::FillPortion(3)),
        ]
        .spacing(sp::S12)
    };
    ui::card(
        column![
            t("About", ty::CARD_TITLE, color::TEXT_PRIMARY),
            fact("Signatures", "WOTS+ one-time keys".to_owned()),
            fact("Addresses", "20-byte tag · Base58 with CRC-16".to_owned()),
            fact("Tawara", format!("version {}", env!("CARGO_PKG_VERSION"))),
            fact(
                "Wallet library",
                format!(
                    "mochimo-crypto at {}",
                    &tawara_wallet_core::LIBRARY_REV[..7]
                )
            ),
        ]
        .spacing(sp::S12),
    )
    .into()
}
