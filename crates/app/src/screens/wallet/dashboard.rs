//! W1, the wallet (rendering 02): the balance, the one-time keys, the
//! accounts, the store's notice above them, and below them the newest
//! transactions from the node's index and the network card
//! (docs/SCREENS.md W1, W10).

use iced::widget::text::{Shaping, Text, Wrapping};
use iced::widget::{Id, column, container, row, space};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::explorer::BlockKind;
use tawara_wallet_core::preferences::AmountUnit;
use tawara_wallet_core::view::{AccountRow, AccountState, Total, WalletView};

use super::{action, activity, copy, destination, frame, kind, name, pair, report, status};
use crate::app::{Message, Model, ReportKey, To, WalletMsg, WalletPage, minutes};
use crate::history::{self, Kind};
use crate::icon::Icon;
use crate::tap::tap;
use crate::theme::{self, color, space as sp};
use crate::ui::{self, Size, t, ty};

pub fn view<'a>(model: &'a Model, page: &'a WalletPage) -> Element<'a, Message> {
    let subtitle = format!(
        "Keystore unlocked · auto-locks after {} with nothing done",
        minutes(model.prefs.idle_lock)
    );
    let mut body = Vec::new();
    if let Some(wallet) = &model.wallet {
        if let Some(r) = report::notice(wallet) {
            body.push(report::show_in(model.compact(), page, ReportKey::Notice, r));
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
    frame(model, page, "Wallet", subtitle, header_actions(model), body)
}

/// The page's header buttons. A phone holds Receive and Send as tabs of its
/// bar, so its header keeps Refresh alone (docs/SCREENS.md, "Compact layout").
fn header_actions<'a>(model: &Model) -> Vec<Element<'a, Message>> {
    let mut actions = vec![action(
        model,
        "Refresh",
        theme::Button::Secondary,
        Size::Header,
        Some(Icon::Refresh),
        Message::Refresh,
    )];
    if !model.compact() {
        actions.push(action(
            model,
            "Receive",
            theme::Button::Secondary,
            Size::Header,
            Some(Icon::Receive),
            WalletMsg::Open(To::Receive(None)),
        ));
        actions.push(action(
            model,
            "Send",
            theme::Button::Primary,
            Size::Header,
            Some(Icon::Send),
            WalletMsg::Open(To::Send(None)),
        ));
    }
    actions
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
        // On a phone the whole figure is set as large as its card lets it stay
        // on one line, because a figure wider than the card runs past the
        // window and is cut off (docs/DECISIONS.md D32 item 10 (b)). The
        // desktop's figure is set as it always was.
        let size = if model.compact() {
            figure_size(&whole, balance_room(model.width))
        } else {
            ty::BALANCE.size
        };
        card = card
            .push(
                row![
                    figure_text(whole, size),
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
    // On a phone the hero is padded 20 rather than 28 by 32, so that its
    // figures keep the room a 360-pixel window leaves them (docs/SCREENS.md,
    // "Compact layout").
    let padding = if model.compact() {
        Padding::from(sp::S20)
    } else {
        Padding::from([sp::S28, sp::S32])
    };
    container(card.push(chips))
        .padding(padding)
        .width(Length::Fill)
        .style(theme::hero_card)
        .into()
}

/// The balance's figure, `whole`, at `size`. Its glyphs are placed by their
/// advances alone (basic shaping), which is what [`figure_size`] sums. iced
/// shapes ASCII text that way unless told otherwise, but the advanced shaping
/// the `advanced-shaping` feature would make the default applies the face's
/// kerning, and Montserrat Bold sets a 4 or an 8 further from the comma after
/// it (4,444,444,444 is nearly 1 per cent wider), which could carry a figure a
/// few pixels past its room (docs/DECISIONS.md D32 item 10 (b)).
fn figure_text<'a>(whole: String, size: f32) -> Text<'a> {
    t(
        whole,
        ui::Type {
            size,
            ..ty::BALANCE
        },
        color::TEXT_PRIMARY,
    )
    .shaping(Shaping::Basic)
}

/// The width a phone's balance figure has at a window `width` logical pixels
/// wide: the window less the page's margins of 16 and the hero card's padding
/// of 20 on each side (docs/SCREENS.md, "Compact layout").
fn balance_room(width: f32) -> f32 {
    width - 2.0 * (sp::S16 + sp::S20)
}

/// The size a phone's balance figure is set at: the largest whole size, no
/// larger than the balance's own, at which `figure` fits `room` on one line.
/// The width is the sum of the glyphs' advances, which is the width the figure
/// is drawn at because [`figure_text`] sets it with basic shaping, without
/// kerning; the drawn width is measured by
/// `the_longest_amount_in_each_unit_fits_a_phone_card_at_360_and_390`.
fn figure_size(figure: &str, room: f32) -> f32 {
    let em: f32 = figure.chars().map(glyph_em).sum();
    if em <= 0.0 {
        return ty::BALANCE.size;
    }
    (room / em).floor().clamp(1.0, ty::BALANCE.size)
}

/// A glyph's advance in ems in the balance's face, Montserrat Bold
/// (`fonts::DISPLAY_BOLD`), read from `crates/app/fonts/Montserrat-Bold.ttf`.
/// Its figures are proportional, so each digit has its own advance. Any other
/// glyph is taken at the widest digit's, so that a figure is never sized too
/// large for the face.
fn glyph_em(glyph: char) -> f32 {
    match glyph {
        '0' => 0.679,
        '1' => 0.392,
        '2' => 0.590,
        '3' => 0.592,
        '4' => 0.689,
        '5' => 0.595,
        '6' => 0.637,
        '7' => 0.620,
        '8' => 0.660,
        '9' => 0.637,
        ',' => 0.262,
        _ => 0.689,
    }
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
    ui::card_for(
        model.compact(),
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
    if model.compact() {
        return stacked(model, page, rows);
    }
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

/// The accounts on a phone (docs/SCREENS.md, "Compact layout"): one item per
/// account, separated by dividers. An item's name and state are on its first
/// line, its destination under them, and its balance, next key and action on
/// the last. An item opens its account when the wallet is idle and it is
/// tapped. It acts on release, so a drag that starts on it scrolls the page
/// (docs/DECISIONS.md D32 item 10 (d)), and the item's own Copy or Review
/// button takes its own click.
fn stacked<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    rows: &'a [AccountRow],
) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let idle = model.busy.is_none();
    let mut list = column![].width(Length::Fill);
    for (n, account) in rows.iter().enumerate() {
        let shown = destination(account.id);
        let (word, ink) = status(&account.state);
        let balance = account
            .state
            .balance()
            .map_or_else(|| "—".to_owned(), |b| ui::amount(b, unit));
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
                WalletMsg::Open(To::Account(account.id)),
            ),
        };
        let item = tap(container(
            column![
                row![
                    column![
                        t(name(account.id), ty::TABLE_NAME, color::TEXT_PRIMARY),
                        t(kind(account), ty::TINY, color::TEXT_MUTED),
                    ]
                    .spacing(sp::S2)
                    .width(Length::Fill),
                    row![ui::dot(ink, 6.0), t(word, ty::TABLE_BODY, ink)]
                        .spacing(sp::S6)
                        .align_y(Alignment::Center),
                ]
                .spacing(sp::S12)
                .align_y(Alignment::Center)
                .width(Length::Fill),
                t(shown, ty::MONO_SMALL, color::TEXT_SECONDARY)
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(Length::Fill),
                row![
                    t(balance, ty::TABLE_AMOUNT, color::TEXT_PRIMARY)
                        .wrapping(Wrapping::WordOrGlyph),
                    space().width(Length::Fill),
                    t(
                        format!("next key #{}", account.index),
                        ty::NOTE,
                        color::TEXT_MUTED,
                    ),
                    container(act).id(action_id(n)),
                ]
                .spacing(sp::S12)
                .align_y(Alignment::Center)
                .width(Length::Fill),
            ]
            .spacing(sp::S8)
            .width(Length::Fill),
        )
        .padding(Padding::from([sp::S12, 0.0]))
        .width(Length::Fill)
        .id(row_id(n)))
        .on_tap_maybe(idle.then(|| Message::from(WalletMsg::Open(To::Account(account.id)))));
        list = list.push(item);
        if n + 1 < rows.len() {
            list = list.push(ui::divider());
        }
    }
    ui::card_for(
        true,
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
            list,
        ]
        .spacing(sp::S8),
    )
    .into()
}

/// The ids of account `n`'s row and of its action on a phone. They draw
/// nothing; the tests find a row and its action where they are drawn.
fn row_id(n: usize) -> Id {
    Id::from(format!("account-row-{n}"))
}

fn action_id(n: usize) -> Id {
    Id::from(format!("account-action-{n}"))
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
                model.compact(),
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
                let mut amount = t(amount, ty::TABLE_AMOUNT, ink);
                if model.compact() {
                    // A long amount breaks between its glyphs rather than
                    // running past the row (docs/SCREENS.md, "Compact layout").
                    amount = amount.wrapping(Wrapping::WordOrGlyph);
                }
                list = list.push(line(
                    model.compact(),
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
                    amount.into(),
                ));
            }
            if shown == 0 {
                list = list.push(ui::helper(
                    "The node's index holds no transaction for this store's accounts yet.",
                ));
            }
        }
        Some(Err(refusal)) => {
            list = list.push(report::show_in(
                model.compact(),
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
    ui::card_for(
        model.compact(),
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
    compact: bool,
    glyph: Icon,
    tint: iced::Color,
    title: String,
    detail: String,
    state: Element<'a, Message>,
    amount: Element<'a, Message>,
) -> Element<'a, Message> {
    let icon = container(crate::icon::icon(glyph, 16.0, 2.0, tint))
        .center(Length::Fixed(36.0))
        .style(theme::icon_tile(color::BG_RAISED, tint));
    let text = column![
        ui::arrowed(title, ty::ROW_TITLE, color::TEXT_PRIMARY),
        t(detail, ty::NOTE, color::TEXT_MUTED).wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(sp::S2);
    if compact {
        // On a phone the amount stands over the state, at the right, and the
        // title and the detail take what is left (docs/SCREENS.md, "Compact
        // layout").
        return row![
            icon,
            text.width(Length::Fill),
            column![amount, state]
                .spacing(sp::S2)
                .align_x(Alignment::End),
        ]
        .spacing(sp::S12)
        .align_y(Alignment::Center)
        .padding(Padding::from([sp::S8, 0.0]))
        .into();
    }
    row![
        icon,
        text.width(Length::FillPortion(5)),
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
/// was made, the difficulty it was solved at, how many transactions wait in
/// the node's queue, and the newest blocks by kind.
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
            let newest = view.blocks.first();
            let last =
                newest.map_or_else(|| "—".to_owned(), |b| ui::age(model.clock_ms, b.time_ms));
            let difficulty = newest
                .and_then(|b| b.difficulty)
                .map_or_else(|| "—".to_owned(), |d| d.to_string());
            let waiting = match &model.mempool.last {
                Some(Ok(m)) => ui::group(&m.waiting.to_string()),
                _ => "—".to_owned(),
            };
            card = card
                .push(row![
                    stat("Block height", ui::group(&view.tip.to_string())),
                    stat("Last block", last),
                ])
                .push(row![
                    stat("Difficulty", difficulty),
                    stat("Mempool", waiting)
                ]);
            let mut tiles = row![].spacing(sp::S6);
            for (n, b) in view.blocks.iter().rev().enumerate() {
                let tip = n + 1 == view.blocks.len();
                let digits = b.index.to_string();
                let tail = digits[digits.len().saturating_sub(3)..].to_owned();
                let (fill, ink) = match b.kind {
                    Some(BlockKind::Neogenesis) => (color::ACCENT, color::TEXT_ON_ACCENT),
                    Some(BlockKind::Pseudo) => (color::WARNING_SOFT, color::WARNING),
                    Some(BlockKind::Normal) => (color::BG_RAISED, color::TEXT_PRIMARY),
                    // The node sent none of its figures: no kind is shown.
                    None => (color::BG_SURFACE, color::TEXT_SECONDARY),
                };
                let edge = match (tip, b.kind) {
                    (true, _) => color::ACCENT,
                    (false, None) => color::BORDER_SUBTLE,
                    (false, Some(_)) => fill,
                };
                tiles = tiles.push(
                    container(t(tail, ty::MONO_TINY, ink))
                        .center(Length::Fill)
                        .height(Length::Fixed(44.0))
                        .style(theme::block_tile(fill, edge)),
                );
            }
            card = card.push(
                column![
                    t(
                        format!("Last {} blocks", view.blocks.len()),
                        ty::NOTE,
                        color::TEXT_MUTED,
                    ),
                    tiles,
                ]
                .spacing(sp::S8),
            );
            if view.blocks.iter().any(|b| b.kind.is_some()) {
                let key = |fill, label: &'static str| {
                    row![
                        container(space())
                            .width(Length::Fixed(8.0))
                            .height(Length::Fixed(8.0))
                            .style(theme::swatch(fill)),
                        t(label, ty::TINY, color::TEXT_MUTED),
                    ]
                    .spacing(sp::S6)
                    .align_y(Alignment::Center)
                };
                card = card.push(
                    row![
                        key(color::ACCENT, "Neogenesis"),
                        key(color::WARNING, "Pseudo"),
                        key(color::SWATCH_NORMAL, "Normal"),
                    ]
                    .spacing(sp::S14),
                );
            }
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
    ui::card_for(model.compact(), card).into()
}

#[cfg(test)]
mod tests {
    use iced::Rectangle;
    use iced::advanced::widget::Operation;
    use iced_runtime::user_interface::{Cache, UserInterface};
    use tawara_wallet_core::preferences::{AmountUnit, Preferences};
    use tawara_wallet_core::{Activity, Progress, RequestId, sample};

    use super::*;
    use crate::app::{Busy, Explored, Level, NARROWEST, Page, Screen};
    use crate::harness::{
        Driver, finger_lift, finger_move, finger_press, fits, mouse_move, mouse_press,
        mouse_release,
    };

    /// A phone's window tall enough for the first account row to be drawn
    /// above the foot of the page.
    const TALL: (u32, u32) = (360, 1200);

    /// The sample store on the wallet page, in a phone 360 logical pixels
    /// wide: what the screenshot example puts in its model for W1.
    fn phone() -> Model {
        let mut model = Model::new(Preferences::default(), None);
        model.width = NARROWEST;
        model.screen = Screen::Wallet(WalletPage {
            page: Page::Dashboard,
            ..WalletPage::default()
        });
        model.wallet = Some(sample::wallet_view());
        model.activity = read(sample::activity());
        model.blocks = read(sample::blocks());
        model.mempool = read(sample::mempool());
        model
    }

    /// A read that has answered with `value`.
    fn read<T>(value: T) -> Explored<T> {
        Explored {
            last: Some(Ok(value)),
            ..Explored::default()
        }
    }

    /// The bounds in the window of the container that carries `id`, as
    /// `root` draws it in a window of `size`, or `None` when no such container
    /// is drawn. The layout is built by the interface itself, so the bounds are
    /// those a press would meet.
    fn bounds_of(root: Element<'_, Message>, size: (u32, u32), id: &Id) -> Option<Rectangle> {
        struct Find<'a> {
            id: &'a Id,
            found: Option<Rectangle>,
        }
        impl Operation for Find<'_> {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }

            fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
                if id == Some(self.id) {
                    self.found = Some(bounds);
                }
            }
        }
        let mut renderer = crate::harness::renderer();
        let bounds = iced::Size::new(size.0 as f32, size.1 as f32);
        let mut ui = UserInterface::build(root, bounds, Cache::default(), &mut renderer);
        let mut find = Find { id, found: None };
        ui.operate(&renderer, &mut find);
        find.found
    }

    /// Where account row `n` is drawn on a phone in `TALL`.
    fn row_at(model: &Model, n: usize) -> Rectangle {
        bounds_of(crate::screens::view(model), TALL, &row_id(n))
            .expect("on a phone each account is a row that can be tapped")
    }

    #[test]
    fn the_phone_wallet_fits_at_360_in_each_state_the_example_draws() {
        let mut unreconciled = phone();
        unreconciled.wallet = Some(sample::unreconciled_view());
        unreconciled.node.tip = None;
        unreconciled.node.error = Some("the node did not answer".to_owned());
        let mut refreshing = phone();
        refreshing.busy = Some(Busy {
            id: RequestId::example(),
            activity: Some(Activity::AskingNode),
            progress: Some(Progress {
                account: 1,
                accounts: 3,
                position: 0,
                ceiling: 0,
            }),
        });
        let mut noticed = phone();
        if let Screen::Wallet(page) = &mut noticed.screen {
            page.open.insert(ReportKey::Notice, Level::Summary);
        }
        for (label, model) in [
            ("the sample store", phone()),
            ("unreconciled", unreconciled),
            ("refreshing", refreshing),
            ("notice open", noticed),
        ] {
            assert_eq!(
                fits(crate::screens::view(&model), NARROWEST),
                Ok(()),
                "{label}"
            );
        }
    }

    #[test]
    fn the_phone_wallet_fits_at_360_and_390_in_nanomcm() {
        // In nanoMCM the sample balance, 12,395,402,118,000, is wider at 56 than
        // the card a 360 window leaves it, so its words run past the window's
        // edge unless the figure is set smaller (docs/DECISIONS.md D32 item 10
        // (b)).
        for width in [NARROWEST, 390.0] {
            let mut model = phone();
            model.width = width;
            model.prefs.unit = AmountUnit::NanoMcm;
            assert_eq!(
                fits(crate::screens::view(&model), width),
                Ok(()),
                "nanoMCM at {width}"
            );
            // The window's edge is not the card's: a figure a little wider
            // than its card, still inside the window, passes the check above.
            // So the figure's box is held inside the card, the page's margin
            // of 16 and the card's padding of 20 in from each side, and is
            // as wide as its words, which it would not be if they were
            // squeezed into it.
            let total = match model.wallet.as_ref().map(WalletView::total) {
                Some(Total::Whole(sum) | Total::Partial { known: sum, .. }) => sum,
                other => panic!("the sample shows a figure: {other:?}"),
            };
            let figure = ui::amount(total, AmountUnit::NanoMcm);
            let (_, bounds) = crate::harness::texts(crate::screens::view(&model), (width, 844.0))
                .into_iter()
                .find(|(content, _)| *content == figure)
                .expect("the figure is drawn");
            let inset = sp::S16 + sp::S20;
            let needs = natural_width(&figure, figure_size(&figure, balance_room(width)));
            assert!(
                bounds.x >= inset - 0.5 && bounds.x + bounds.width <= width - inset + 0.5,
                "at {width}: the figure's box {bounds:?} passes its card"
            );
            assert!(
                bounds.width + 0.5 >= needs,
                "at {width}: the figure needs {needs}, more than its box {bounds:?}"
            );
        }
    }

    #[test]
    fn a_phone_keeps_refresh_as_its_only_header_action() {
        // Receive and Send are the phone's tabs, so the header keeps one
        // button; the desktop keeps all three.
        assert_eq!(header_actions(&phone()).len(), 1);
        let mut desktop = phone();
        desktop.width = 1440.0;
        assert_eq!(header_actions(&desktop).len(), 3);
    }

    #[test]
    fn on_a_phone_each_account_is_a_row_across_the_card_under_the_one_before() {
        // The card's inner width at 360: 16 pixels of page margin and 16 of
        // card padding on each side.
        let inner = NARROWEST - 4.0 * sp::S16;
        let model = phone();
        let rows: Vec<Rectangle> = (0..3).map(|n| row_at(&model, n)).collect();
        for row in &rows {
            assert!(
                (row.width - inner).abs() < 0.5,
                "{row:?} is not {inner} wide"
            );
        }
        for pair in rows.windows(2) {
            assert!(
                pair[1].y >= pair[0].y + pair[0].height,
                "{:?} is not below {:?}",
                pair[1],
                pair[0]
            );
        }
    }

    #[test]
    fn an_account_row_opens_its_account_when_it_is_tapped() {
        let model = phone();
        let id = sample::wallet_view().accounts[0].id;
        let at = row_at(&model, 0);
        let mut driver = Driver::new(crate::screens::view(&model), TALL);
        let (messages, _, _) = driver.send(&[
            mouse_move(at.x + 20.0, at.y + at.height / 2.0),
            mouse_press(),
            mouse_release(),
        ]);
        assert!(
            matches!(
                messages.as_slice(),
                [Message::Wallet(WalletMsg::Open(To::Account(opened)))] if *opened == id
            ),
            "{messages:?}"
        );
    }

    #[test]
    fn a_drag_that_starts_on_an_account_row_scrolls_and_opens_nothing() {
        let model = phone();
        let at = row_at(&model, 0);
        let (x, y) = (at.x + 20.0, at.y + at.height / 2.0);
        let mut driver = Driver::new(crate::screens::view(&model), TALL);
        let (messages, statuses, _) = driver.send(&[
            finger_press(1, x, y),
            finger_move(1, x, y - 60.0),
            finger_lift(1, x, y - 60.0),
        ]);
        assert!(messages.is_empty(), "the drag opened it: {messages:?}");
        // The page took the move for a scroll: it is longer than the window,
        // so a scrollable that moves its content captures the move.
        assert_eq!(
            statuses[1],
            iced::event::Status::Captured,
            "the page did not scroll with the finger"
        );
    }

    #[test]
    fn the_copy_on_an_account_row_copies_and_opens_nothing() {
        let model = phone();
        let id = sample::wallet_view().accounts[0].id;
        let at = bounds_of(crate::screens::view(&model), TALL, &action_id(0))
            .expect("on a phone the first account's action is drawn");
        let mut driver = Driver::new(crate::screens::view(&model), TALL);
        let (messages, _, _) = driver.send(&[
            mouse_move(at.x + at.width / 2.0, at.y + at.height / 2.0),
            mouse_press(),
            mouse_release(),
        ]);
        assert!(
            matches!(
                messages.as_slice(),
                [Message::Wallet(WalletMsg::Copy(text))] if *text == destination(id)
            ),
            "{messages:?}"
        );
    }

    /// The width `text` takes in the figure's face at `size`, on one line.
    /// It is drawn in a window far wider than the figure, so nothing is
    /// squeezed and the box is the width the words need.
    fn natural_width(text: &str, size: f32) -> f32 {
        let figure: Element<'_, ()> = figure_text(text.to_owned(), size).into();
        crate::harness::texts(figure, (4000.0, 200.0))
            .into_iter()
            .find(|(content, _)| content == text)
            .map(|(_, bounds)| bounds.width)
            .expect("the figure is drawn")
    }

    #[test]
    fn the_longest_amount_in_each_unit_fits_a_phone_card_at_360_and_390() {
        // The card shows the store's total, a sum of u64 balances held as a
        // u128 (`Total`): the largest one account holds, u64::MAX, the sum of
        // two such accounts, and u128::MAX, the largest the total can be, in
        // each unit. In MCM the figure is the whole part before the point,
        // which is the widest.
        let totals = [u128::from(u64::MAX), 2 * u128::from(u64::MAX), u128::MAX];
        for width in [NARROWEST, 390.0] {
            let room = balance_room(width);
            for (unit, total) in [AmountUnit::NanoMcm, AmountUnit::Mcm]
                .into_iter()
                .flat_map(|unit| totals.map(|total| (unit, total)))
            {
                let shown = ui::amount(total, unit);
                let whole = match unit {
                    AmountUnit::Mcm => shown
                        .split_once('.')
                        .map_or(shown.as_str(), |(whole, _)| whole)
                        .to_owned(),
                    AmountUnit::NanoMcm => shown,
                };
                let size = figure_size(&whole, room);
                assert!(
                    (1.0..=ty::BALANCE.size).contains(&size),
                    "{unit:?} {total} at {width}: {size}"
                );
                let needs = natural_width(&whole, size);
                assert!(
                    needs <= room + 0.5,
                    "{unit:?} at {width}: {whole} needs {needs} in {room}"
                );
            }
        }
    }

    #[test]
    fn the_mcm_figure_keeps_the_balances_own_size_on_a_phone() {
        // 12,395 MCM fits its card at 56, so the MCM images are unchanged. So
        // does any whole part of up to six digits, of which 444,444 is the
        // widest; from seven digits a figure can be set smaller, as 4,444,444
        // is at 360 (docs/SCREENS.md, "Compact layout").
        for width in [NARROWEST, 390.0] {
            for whole in ["12,395", "444,444", "999,999"] {
                assert_eq!(
                    figure_size(whole, balance_room(width)),
                    ty::BALANCE.size,
                    "{whole} at {width}"
                );
            }
        }
        assert_eq!(figure_size("4,444,444", balance_room(NARROWEST)), 53.0);
    }

    #[test]
    fn the_nanomcm_sample_is_set_smaller_so_that_it_stays_on_one_line() {
        // 12,395,402,118,000 is 520 pixels at 56, and the card has 288 at 360
        // and 318 at 390.
        let sample = "12,395,402,118,000";
        assert_eq!(figure_size(sample, balance_room(NARROWEST)), 30.0);
        assert_eq!(figure_size(sample, balance_room(390.0)), 34.0);
        assert!(natural_width(sample, 30.0) <= balance_room(NARROWEST));
        assert!(natural_width(sample, 34.0) <= balance_room(390.0));
    }
}
