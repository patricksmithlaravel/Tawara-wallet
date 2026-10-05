//! W1, the wallet (rendering 02): the balance, the one-time keys, the
//! accounts, the store's notice above them, and below them the newest
//! transactions from the node's index and the network card
//! (docs/SCREENS.md W1, W10).

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, space};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::preferences::AmountUnit;
use tawara_wallet_core::view::{AccountRow, AccountState, Total, WalletView};

use super::{action, activity, copy, destination, frame, kind, name, pair, report, status};
use crate::app::{Message, Model, ReportKey, To, WalletMsg, WalletPage, minutes};
use crate::history::{self, Kind};
use crate::icon::Icon;
use crate::theme::{self, color, space as sp};
use crate::ui::{self, Size, t, ty};

pub fn view<'a>(model: &'a Model, page: &'a WalletPage) -> Element<'a, Message> {
    let subtitle = format!(
        "Keystore unlocked · auto-locks after {} with nothing done",
        minutes(model.prefs.idle_lock)
    );
    let actions = vec![
        action(
            model,
            "Refresh",
            theme::Button::Secondary,
            Size::Header,
            Some(Icon::Refresh),
            Message::Refresh,
        ),
        action(
            model,
            "Receive",
            theme::Button::Secondary,
            Size::Header,
            Some(Icon::Receive),
            WalletMsg::Open(To::Receive(None)),
        ),
        action(
            model,
            "Send",
            theme::Button::Primary,
            Size::Header,
            Some(Icon::Send),
            WalletMsg::Open(To::Send(None)),
        ),
    ];
    let mut body = Vec::new();
    if let Some(wallet) = &model.wallet {
        if let Some(r) = report::notice(wallet) {
            body.push(report::show(page, ReportKey::Notice, r));
        }
        body.push(pair(
            model,
            balance(model, wallet),
            keys(model, &wallet.accounts),
            7,
            4,
        ));
        body.push(accounts(model, page, &wallet.accounts));
        body.push(pair(model, recent(model, page), network(model), 7, 4));
    }
    frame(model, page, "Wallet", subtitle, actions, body)
}

/// The total, as the 02 hero card sets it (5.22), with the number of
/// accounts and of spends still settling. An account with no known balance
/// (diverged, or not reconciled) is never counted as zero: while some are
/// unknown the card shows the known balance and says what it leaves out,
/// and while all are, it shows no figure. The settling chip counts spends
/// and shows no amount, because the store keeps none for an open
/// reservation (docs/DECISIONS.md D27, item 3).
fn balance<'a>(model: &'a Model, wallet: &'a WalletView) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let count = wallet.accounts.len();
    let (label, figure, note) = match wallet.total() {
        Total::Whole(sum) => ("Total balance", Some(sum), None),
        Total::Partial { known, unknown } => (
            "Known balance",
            Some(known),
            Some(format!(
                "The sum of {} of the {count} accounts. {} no known balance (— below), so the \
                 store's total is not known.",
                count - unknown,
                if unknown == 1 {
                    "One has".to_owned()
                } else {
                    format!("{unknown} have")
                }
            )),
        ),
        Total::Unknown => (
            "Total balance",
            None,
            Some(
                "Not known: no account has been reconciled against the node, so no balance is \
                 known."
                    .to_owned(),
            ),
        ),
    };
    let mut card = column![ui::section_label(label)].spacing(sp::S12);
    if let Some(nano) = figure {
        let shown = ui::amount(nano, unit);
        let (whole, frac) = match unit {
            AmountUnit::Mcm => shown
                .split_once('.')
                .map_or((shown.clone(), String::new()), |(w, f)| {
                    (w.to_owned(), format!(".{f}"))
                }),
            AmountUnit::NanoMcm => (shown.clone(), String::new()),
        };
        card = card
            .push(
                row![
                    t(whole, ty::BALANCE, color::TEXT_PRIMARY),
                    t(frac, ty::BALANCE_DECIMALS, color::TEXT_SECONDARY),
                    container(t(ui::unit_name(unit), ty::BALANCE_UNIT, color::ACCENT)).padding(
                        Padding {
                            left: sp::S12,
                            ..Padding::ZERO
                        }
                    ),
                ]
                .align_y(Alignment::End)
                .wrap(),
            )
            .push(t(
                format!("{} nanoMCM", ui::group(&nano.to_string())),
                ty::MONO_SMALL,
                color::TEXT_MUTED,
            ));
    } else {
        card = card.push(t("—", ty::BALANCE, color::TEXT_MUTED));
    }
    if let Some(note) = note {
        card = card.push(t(note, ty::BODY_SMALL, color::TEXT_SECONDARY));
    }
    let mut chips = row![ui::pill(
        if count == 1 {
            "1 account".to_owned()
        } else {
            format!("{count} accounts")
        },
        ty::CHIP,
        color::BG_RAISED,
        color::TEXT_SECONDARY,
        30.0,
    )]
    .spacing(sp::S8);
    let settling = wallet
        .accounts
        .iter()
        .filter(|a| {
            matches!(
                a.state,
                AccountState::SpendOutstanding { .. } | AccountState::SpendLanded { .. }
            )
        })
        .count();
    if settling > 0 {
        chips = chips.push(ui::pill(
            if settling == 1 {
                "1 spend settling".to_owned()
            } else {
                format!("{settling} spends settling")
            },
            ty::CHIP,
            color::WARNING_SOFT,
            color::WARNING,
            30.0,
        ));
    }
    container(card.push(chips))
        .padding(Padding::from([sp::S28, sp::S32]))
        .width(Length::Fill)
        .style(theme::hero_card)
        .into()
}

/// The one-time keys (02): each account's next key, and whether it is
/// ready to sign. Every spend signs with the account's next key, and the
/// index after it is written before the signature exists, so an index
/// never moves backwards.
fn keys<'a>(model: &'a Model, rows: &'a [AccountRow]) -> Element<'a, Message> {
    let mut list = column![].spacing(sp::S10);
    for (n, account) in rows.iter().enumerate() {
        let word: Element<'a, Message> = match &account.state {
            AccountState::InSync { .. } => t("ready", ty::TABLE_BODY, color::ACCENT).into(),
            AccountState::SpendOutstanding { .. } => {
                t("settling", ty::TABLE_BODY, color::WARNING).into()
            }
            AccountState::SpendLanded { .. } => t("landed", ty::TABLE_BODY, color::WARNING).into(),
            AccountState::Diverged { .. } => iced::widget::button(
                row![
                    t("Review", ty::LINK, color::WARNING),
                    // Poppins has no arrow; Montserrat has (D27, item 6).
                    t(
                        "→",
                        ui::Type {
                            font: crate::fonts::ARROW,
                            size: ty::LINK.size,
                        },
                        color::WARNING
                    ),
                ]
                .spacing(sp::S4),
            )
            .padding(0)
            .style(theme::button(theme::Button::Bare))
            .on_press_maybe(
                model
                    .busy
                    .is_none()
                    .then_some(WalletMsg::Open(To::Account(account.id)).into()),
            )
            .into(),
            AccountState::NotReconciled => {
                t("not reconciled", ty::TABLE_BODY, color::TEXT_MUTED).into()
            }
        };
        list = list.push(
            row![
                t(name(account.id), ty::TABLE_NAME, color::TEXT_PRIMARY).width(Length::Fill),
                t(format!("#{}", account.index), ty::MONO, color::TEXT_PRIMARY),
                container(word)
                    .width(Length::Fixed(96.0))
                    .align_x(Alignment::End),
            ]
            .spacing(sp::S12)
            .align_y(Alignment::Center),
        );
        if n + 1 < rows.len() {
            list = list.push(ui::divider());
        }
    }
    ui::card(
        column![
            row![
                ui::section_label("One-time keys"),
                space().width(Length::Fill),
                t("WOTS+", ty::NOTE, color::TEXT_MUTED),
            ],
            list,
            t(
                "Each spend signs with the account's next key, and the index after it is \
                 written to disk before the signature exists. An index never moves backwards.",
                ty::NOTE,
                color::TEXT_MUTED,
            ),
        ]
        .spacing(sp::S16),
    )
    .into()
}

/// The accounts table (02, `design/TOKENS.md` 5.12), with each account's
/// action: copying a reconciled one's destination, or opening the page of
/// one that needs a look. "Review" opens the account's page and never moves
/// a key index (docs/DECISIONS.md D27, item 3).
///
/// Its columns share the card's width in proportion, so it fits from the
/// rendering's 1440 px down to the smallest window, 1024 px (D27, item 9);
/// a destination or a balance too long for its column breaks between
/// characters rather than being cut off.
fn accounts<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    rows: &'a [AccountRow],
) -> Element<'a, Message> {
    const ACCOUNT: u16 = 4;
    const DESTINATION: u16 = 6;
    const BALANCE: u16 = 4;
    const KEY: u16 = 3;
    const STATUS: u16 = 4;
    const ACTION: f32 = 104.0;
    let unit = model.prefs.unit;
    let head = |label: &'static str, portion: u16, right: bool| {
        container(t(label, ty::TABLE_HEADER, color::TEXT_MUTED))
            .width(Length::FillPortion(portion))
            .align_x(if right {
                Alignment::End
            } else {
                Alignment::Start
            })
    };
    let mut table = column![
        row![
            head("Account", ACCOUNT, false),
            head("Destination", DESTINATION, false),
            head(
                if matches!(unit, AmountUnit::Mcm) {
                    "Balance (MCM)"
                } else {
                    "Balance (nanoMCM)"
                },
                BALANCE,
                true,
            ),
            head("Next key", KEY, false),
            head("Status", STATUS, false),
            space().width(Length::Fixed(ACTION)),
        ]
        .spacing(sp::S12)
        .padding(Padding::from([sp::S10, 0.0])),
        ui::divider(),
    ];
    for (n, account) in rows.iter().enumerate() {
        let shown = destination(account.id);
        let (word, ink) = status(&account.state);
        let balance = account
            .state
            .balance()
            .map_or_else(|| "—".to_owned(), |b| ui::amount(b, unit));
        let key = match &account.state {
            AccountState::Diverged {
                advance_to: Some(to),
                ..
            } => format!("#{} · ledger #{to}", account.index),
            _ => format!("#{}", account.index),
        };
        let open = WalletMsg::Open(To::Account(account.id));
        let act: Element<'a, Message> = match account.state {
            AccountState::InSync { .. } => copy(page, &shown, "Copy"),
            _ => action(
                model,
                "Review",
                if ink == color::WARNING {
                    theme::Button::WarningTonal
                } else {
                    theme::Button::Secondary
                },
                Size::Small,
                None,
                open.clone(),
            ),
        };
        table = table.push(
            row![
                iced::widget::button(
                    column![
                        t(name(account.id), ty::TABLE_NAME, color::TEXT_PRIMARY),
                        t(kind(account), ty::TINY, color::TEXT_MUTED),
                    ]
                    .spacing(sp::S2),
                )
                .padding(0)
                .style(theme::button(theme::Button::Bare))
                .on_press_maybe(model.busy.is_none().then(|| open.into()))
                .width(Length::FillPortion(ACCOUNT)),
                t(shown, ty::MONO, color::TEXT_SECONDARY)
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(Length::FillPortion(DESTINATION)),
                container(
                    t(balance, ty::TABLE_AMOUNT, color::TEXT_PRIMARY)
                        .wrapping(Wrapping::WordOrGlyph)
                        .align_x(Alignment::End),
                )
                .width(Length::FillPortion(BALANCE))
                .align_x(Alignment::End),
                t(key, ty::MONO, color::TEXT_PRIMARY)
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(Length::FillPortion(KEY)),
                row![
                    ui::dot(ink, 6.0),
                    t(word, ty::TABLE_BODY, ink).width(Length::Fill),
                ]
                .spacing(sp::S6)
                .align_y(Alignment::Center)
                .width(Length::FillPortion(STATUS)),
                container(act)
                    .width(Length::Fixed(ACTION))
                    .align_x(Alignment::End),
            ]
            .spacing(sp::S12)
            .align_y(Alignment::Center)
            .padding(Padding::from([sp::S14, 0.0])),
        );
        if n + 1 < rows.len() {
            table = table.push(ui::divider());
        }
    }
    container(
        column![
            row![
                ui::section_label("Accounts"),
                space().width(Length::Fill),
                action(
                    model,
                    "Add account",
                    theme::Button::Tonal,
                    Size::Small,
                    Some(Icon::Plus),
                    WalletMsg::Open(To::AddAccount),
                ),
            ]
            .align_y(Alignment::Center),
            table
        ]
        .spacing(sp::S8),
    )
    .padding(Padding {
        top: sp::S20,
        right: sp::S24,
        bottom: sp::S8,
        left: sp::S24,
    })
    .width(Length::Fill)
    .style(theme::card)
    .into()
}

/// How many of the newest transactions the dashboard lists.
const RECENT: usize = 4;

/// Recent activity (02): the newest transactions of every account from the
/// node's index, and the spends the store has reserved and not settled,
/// with no amount, since it keeps none.
fn recent<'a>(model: &'a Model, page: &'a WalletPage) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let mut list = column![].spacing(sp::S4);
    let mut shown = 0;
    if let Some(w) = &model.wallet {
        for account in history::pending(&w.accounts) {
            shown += 1;
            list = list.push(line(
                Icon::Clock,
                color::WARNING,
                "Spend settling".to_owned(),
                format!("{} · the store's own record", name(account.id)),
                t("settling", ty::TABLE_BODY, color::WARNING).into(),
                t("not kept", ty::TABLE_BODY, color::TEXT_MUTED).into(),
            ));
        }
    }
    match &model.activity.last {
        Some(Ok(histories)) => {
            for r in history::rows(histories)
                .iter()
                .take(RECENT.saturating_sub(shown))
            {
                shown += 1;
                let (title, detail) = activity::describe(r);
                let (amount, ink) = activity::amount(r, unit);
                let (glyph, tint) = match r.kind {
                    Kind::Sent => (Icon::Send, color::TEXT_SECONDARY),
                    Kind::Received | Kind::Reward => (Icon::Receive, color::ACCENT),
                    Kind::Own | Kind::Other => (Icon::Swap, color::TEXT_SECONDARY),
                };
                let when =
                    r.tx.time_ms
                        .map_or_else(|| "—".to_owned(), |ms| ui::short_date(ms, model.zone));
                list = list.push(line(
                    glyph,
                    tint,
                    title,
                    format!("{detail} · {when}"),
                    t(
                        r.tx.block.map_or_else(String::new, |b| {
                            format!("block {}", ui::group(&b.to_string()))
                        }),
                        ty::NOTE,
                        color::TEXT_MUTED,
                    )
                    .into(),
                    t(amount, ty::TABLE_AMOUNT, ink).into(),
                ));
            }
            if shown == 0 {
                list = list.push(ui::helper(
                    "The node's index holds no transaction for this store's accounts yet.",
                ));
            }
        }
        Some(Err(refusal)) => {
            list = list.push(report::show(
                page,
                ReportKey::Explorer,
                report::explorer(refusal, "its transaction index"),
            ));
        }
        None => {
            list = list.push(ui::helper(if model.activity.reading {
                "Reading the node's index…"
            } else {
                "Not read: no node is chosen, or it has not answered yet."
            }));
        }
    }
    ui::card(
        column![
            row![
                ui::section_label("Recent activity"),
                space().width(Length::Fill),
                ui::link(
                    "View all",
                    ty::LINK_SMALL,
                    Message::from(WalletMsg::Open(To::Activity))
                ),
            ]
            .align_y(Alignment::Center),
            list,
        ]
        .spacing(sp::S12),
    )
    .into()
}

/// One line of recent activity: a glyph, what it was and when, its state,
/// and its amount.
fn line<'a>(
    glyph: Icon,
    tint: iced::Color,
    title: String,
    detail: String,
    state: Element<'a, Message>,
    amount: Element<'a, Message>,
) -> Element<'a, Message> {
    row![
        container(crate::icon::icon(glyph, 16.0, 2.0, tint))
            .center(Length::Fixed(36.0))
            .style(theme::icon_tile(color::BG_RAISED, tint)),
        column![
            ui::arrowed(title, ty::ROW_TITLE, color::TEXT_PRIMARY),
            t(detail, ty::NOTE, color::TEXT_MUTED).wrapping(Wrapping::WordOrGlyph),
        ]
        .spacing(sp::S2)
        .width(Length::FillPortion(5)),
        container(state).width(Length::FillPortion(2)),
        container(amount)
            .width(Length::FillPortion(3))
            .align_x(Alignment::End),
    ]
    .spacing(sp::S12)
    .align_y(Alignment::Center)
    .padding(Padding::from([sp::S8, 0.0]))
    .into()
}

/// The network card (02): the chain's height, how long ago its last block
/// was made, and the newest blocks. A block's type, the difficulty and the
/// mempool are not read: the library does not serve them yet
/// (docs/DECISIONS.md D27, item 4).
fn network(model: &Model) -> Element<'_, Message> {
    let stat = |label: &'static str, value: String| {
        column![
            t(label, ty::NOTE, color::TEXT_MUTED),
            t(value, ty::STAT_VALUE, color::TEXT_PRIMARY),
        ]
        .spacing(sp::S4)
        .width(Length::Fill)
    };
    let mut card = column![ui::section_label("Network")].spacing(sp::S16);
    match &model.blocks.last {
        Some(Ok(view)) => {
            let last = view
                .blocks
                .first()
                .map_or_else(|| "—".to_owned(), |b| ui::age(model.clock_ms, b.time_ms));
            card = card.push(row![
                stat("Block height", ui::group(&view.tip.to_string())),
                stat("Last block", last),
            ]);
            let mut chips = row![].spacing(sp::S8);
            for (n, b) in view.blocks.iter().rev().enumerate() {
                let newest = n + 1 == view.blocks.len();
                let digits = b.index.to_string();
                let tail = digits[digits.len().saturating_sub(3)..].to_owned();
                chips = chips.push(
                    container(t(
                        tail,
                        ty::MONO_SMALL,
                        if newest {
                            color::ACCENT
                        } else {
                            color::TEXT_SECONDARY
                        },
                    ))
                    .center_x(Length::Fill)
                    .padding(Padding::from([sp::S10, 0.0]))
                    .style(theme::radio_card(newest)),
                );
            }
            card = card
                .push(t(
                    format!("Last {} blocks", view.blocks.len()),
                    ty::NOTE,
                    color::TEXT_MUTED,
                ))
                .push(chips);
        }
        Some(Err(_)) => {
            card = card.push(t(
                "The node did not serve its newest blocks.",
                ty::BODY_SMALL,
                color::TEXT_SECONDARY,
            ));
        }
        None => {
            card = card.push(ui::helper(if model.blocks.reading {
                "Reading the newest blocks…"
            } else {
                "Not read: no node is chosen, or it has not answered yet."
            }));
        }
    }
    ui::card(card).into()
}
