//! W10, Activity (rendering 04): every account's transactions from the
//! node's index, and one transaction's detail beside them
//! (docs/SCREENS.md W10).
//!
//! The rows are the index's, newest 100 for each account (D27, item 11);
//! spends the store has reserved and not settled are its own record, listed
//! with no amount since the store keeps none. The index's rows carry no
//! references: a transaction's are read from its block when it is chosen
//! (D29, item 3), and shown from then on. "Export CSV" needs a file
//! dialog and "Receipt verified" claims a check nothing makes (D27, items 3
//! and 5): neither is built.

use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, space, text_input};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::explorer::{AccountHistory, OperationView, Party, References};
use tawara_wallet_core::preferences::AmountUnit;
use tawara_wallet_core::view::AccountState;

use super::{frame, name, pair, report, short};
use crate::app::{ActivityPage, Message, Model, ReportKey, To, WalletMsg, WalletPage};
use crate::history::{self, Filter, Kind, Row};
use crate::icon::{self, Icon};
use crate::theme::{self, color, radius, space as sp};
use crate::ui::{self, Size, t, ty};

pub fn view<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    a: &'a ActivityPage,
) -> Element<'a, Message> {
    let idle = model.busy.is_none();
    let mut tabs = row![].spacing(sp::S8);
    for (filter, label) in [
        (Filter::All, "All"),
        (Filter::Sent, "Sent"),
        (Filter::Received, "Received"),
        (Filter::Pending, "Pending"),
    ] {
        tabs = tabs.push(ui::button_with(
            label,
            theme::Button::Segment {
                selected: a.filter == filter,
            },
            Size::Small,
            None,
            Some(WalletMsg::Filter(filter).into()),
        ));
    }
    let search = text_input("Address, block or transaction id", &a.search)
        .font(ty::FIELD.font)
        .size(ty::FIELD.size)
        .padding(Padding::from([sp::S10, sp::S14]))
        .width(Length::Fixed(300.0))
        .style(theme::field(radius::R12))
        .on_input(|s| WalletMsg::Search(s).into());
    let controls = row![
        tabs,
        space().width(Length::Fill),
        row![
            icon::icon(Icon::Search, 18.0, 2.0, color::TEXT_MUTED),
            search
        ]
        .spacing(sp::S8)
        .align_y(Alignment::Center),
    ]
    .align_y(Alignment::Center);
    let mut body: Vec<Element<'a, Message>> = vec![controls.into()];
    let read = &model.activity;
    match &read.last {
        Some(Ok(histories)) => body.push(listing(model, page, a, histories)),
        Some(Err(refusal)) => {
            body.push(report::show(
                page,
                ReportKey::Explorer,
                report::explorer(refusal, "its transaction index"),
            ));
        }
        None => body.push(
            ui::card(ui::helper(if read.reading {
                "Reading the node's index…"
            } else if model.node.url.is_none() {
                "No node is chosen: Activity reads the transactions from the node's index."
            } else {
                "Not read yet."
            }))
            .into(),
        ),
    }
    frame(
        model,
        page,
        "Activity",
        "From the node's transaction index · every account, newest first",
        vec![
            ui::button_with(
                if read.reading {
                    "Reading…"
                } else {
                    "Read again"
                },
                theme::Button::Secondary,
                Size::Header,
                Some(Icon::Refresh),
                (idle && !read.reading).then(|| WalletMsg::ReadActivity.into()),
            )
            .into(),
        ],
        body,
    )
}

/// The list and the detail of the one chosen.
fn listing<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    a: &'a ActivityPage,
    histories: &'a [AccountHistory],
) -> Element<'a, Message> {
    let query = a.search.trim().to_lowercase();
    let rows: Vec<Row<'a>> = history::rows(histories)
        .into_iter()
        .filter(|r| a.filter.admits(r.kind) && r.matches(&query))
        .collect();
    let mut list = column![
        row![
            head("Date", 3),
            head("Description", 7),
            head("Account", 4),
            container(t(
                format!("Amount ({})", ui::unit_name(model.prefs.unit)),
                ty::TABLE_HEADER,
                color::TEXT_MUTED
            ))
            .width(Length::FillPortion(4))
            .align_x(Alignment::End),
        ]
        .spacing(sp::S12)
        .padding(Padding::from([sp::S10, sp::S12])),
        ui::divider(),
    ]
    .spacing(sp::S4);
    let mut any = false;
    if a.filter.shows_pending()
        && query.is_empty()
        && let Some(w) = &model.wallet
    {
        for account in history::pending(&w.accounts) {
            any = true;
            list = list.push(pending_row(model, account));
        }
    }
    let chosen = a
        .selected
        .as_deref()
        .and_then(|id| rows.iter().find(|r| r.tx.id == id))
        .or(rows.first());
    for r in &rows {
        any = true;
        let selected = chosen.is_some_and(|c| c.tx.id == r.tx.id);
        list = list.push(index_row(model, r, selected));
    }
    if !any {
        list = list.push(
            container(ui::helper(if query.is_empty() && a.filter == Filter::All {
                "The node's index holds no transaction for this store's accounts. An account \
                 never paid has none."
            } else {
                "No transaction matches."
            }))
            .padding(sp::S12),
        );
    }
    let cut: u64 = histories
        .iter()
        .map(|h| h.total.saturating_sub(h.transactions.len() as u64))
        .sum();
    if cut > 0 {
        list = list.push(
            container(ui::helper(format!(
                "The index answers the newest 100 for each account; {cut} older are not shown."
            )))
            .padding(sp::S12),
        );
    }
    let detail: Element<'a, Message> = match chosen {
        Some(r) => detail(model, page, r),
        None => ui::card(ui::helper("Choose a transaction to see it whole.")).into(),
    };
    pair(model, ui::card(list).padding(sp::S12).into(), detail, 7, 5)
}

fn head<'a>(label: &'static str, portion: u16) -> Element<'a, Message> {
    t(label, ty::TABLE_HEADER, color::TEXT_MUTED)
        .width(Length::FillPortion(portion))
        .into()
}

/// A spend the store has reserved and not settled: its own record, with
/// no amount, since the store keeps none for an open reservation.
fn pending_row<'a>(
    model: &'a Model,
    account: &'a tawara_wallet_core::view::AccountRow,
) -> Element<'a, Message> {
    let (title, word) = match account.state {
        AccountState::SpendLanded { .. } => ("Spend landed: settle it", "landed"),
        _ => ("Spend settling", "settling"),
    };
    button(
        row![
            t(word, ty::TABLE_BODY, color::WARNING).width(Length::FillPortion(3)),
            column![
                t(title, ty::TABLE_NAME, color::TEXT_PRIMARY),
                t(
                    "From the store's own record, not the index",
                    ty::MONO_TINY,
                    color::TEXT_MUTED
                ),
            ]
            .spacing(sp::S2)
            .width(Length::FillPortion(7)),
            t(name(account.id), ty::TABLE_BODY, color::TEXT_SECONDARY)
                .width(Length::FillPortion(4)),
            container(t("not kept", ty::TABLE_BODY, color::TEXT_MUTED))
                .width(Length::FillPortion(4))
                .align_x(Alignment::End),
        ]
        .spacing(sp::S12)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([sp::S12, sp::S12]))
    .width(Length::Fill)
    .style(theme::button(theme::Button::Bare))
    .on_press_maybe(
        model
            .busy
            .is_none()
            .then(|| WalletMsg::Open(To::Account(account.id)).into()),
    )
    .into()
}

/// One of the index's rows.
fn index_row<'a>(model: &'a Model, r: &Row<'a>, selected: bool) -> Element<'a, Message> {
    let (title, mut detail) = describe(r);
    if let Some(found) = read_references(model, r) {
        let known: Vec<&str> = mine(r, found)
            .into_iter()
            .map(|d| d.memo.as_str())
            .filter(|m| !m.is_empty())
            .collect();
        if !known.is_empty() && r.kind != Kind::Own {
            detail = known.join(" · ");
        }
    }
    let (amount, ink) = amount(r, model.prefs.unit);
    let when =
        r.tx.time_ms
            .map_or_else(|| "—".to_owned(), |ms| ui::date_time(ms, model.zone));
    let content = row![
        t(when, ty::TABLE_BODY, color::TEXT_SECONDARY).width(Length::FillPortion(3)),
        column![
            ui::arrowed(title, ty::TABLE_NAME, color::TEXT_PRIMARY),
            t(detail, ty::MONO_TINY, color::TEXT_MUTED).wrapping(Wrapping::WordOrGlyph),
        ]
        .spacing(sp::S2)
        .width(Length::FillPortion(7)),
        t(name(r.account), ty::TABLE_BODY, color::TEXT_SECONDARY).width(Length::FillPortion(4)),
        container(
            t(amount, ty::TABLE_AMOUNT, ink)
                .wrapping(Wrapping::WordOrGlyph)
                .align_x(Alignment::End)
        )
        .width(Length::FillPortion(4))
        .align_x(Alignment::End),
    ]
    .spacing(sp::S12)
    .align_y(Alignment::Center);
    let boxed = container(content)
        .padding(Padding::from([sp::S12, sp::S12]))
        .width(Length::Fill);
    button(if selected {
        boxed.style(theme::row_selected)
    } else {
        boxed
    })
    .padding(0)
    .width(Length::Fill)
    .style(theme::button(theme::Button::Bare))
    .on_press(WalletMsg::Select(r.tx.id.clone()).into())
    .into()
}

/// A party, as a person reads it: an account by its shortened destination,
/// a ledger address said to be one, anything else as the node sent it.
pub(super) fn party(op: &OperationView) -> String {
    match &op.party {
        Party::Account(id) => name(*id),
        Party::Ledger(hex) => format!("ledger address {}", short(hex)),
        Party::Other(text) if text.is_empty() => "—".to_owned(),
        Party::Other(text) => text.clone(),
    }
}

/// A row's title and the line under it: who it went to or came from, and
/// the references the node sent, or the transaction's id when it sent
/// none.
pub(super) fn describe(r: &Row<'_>) -> (String, String) {
    let payees: Vec<&OperationView> = r.payees().collect();
    let title = match r.kind {
        Kind::Sent if payees.len() == 1 => format!("Sent to {}", party(payees[0])),
        Kind::Sent => format!("Batch send · {} destinations", payees.len()),
        Kind::Received => match r.payers().next() {
            Some(from) => format!("Received from {}", party(from)),
            None => "Received".to_owned(),
        },
        Kind::Own => match (r.payers().next(), payees.first()) {
            // Seen from the account it left: to the store's other account.
            (None, Some(to)) => format!("{} → {}", name(r.account), party(to)),
            // Only the receiving side is in the index's answer.
            (Some(from), _) => format!("{} → {}", party(from), name(r.account)),
            (None, None) => "Between own accounts".to_owned(),
        },
        Kind::Reward => "Mining reward".to_owned(),
        Kind::Other => "Transaction".to_owned(),
    };
    let references = r.references();
    let detail = if r.kind == Kind::Own {
        "Between own accounts".to_owned()
    } else if references.is_empty() {
        format!("tx {}", short(&r.tx.id))
    } else {
        references.join(" · ")
    };
    (title, detail)
}

/// The transaction's destinations as its block lists them, once its
/// references have been read and the block carries it.
fn read_references<'a>(model: &'a Model, r: &Row<'_>) -> Option<&'a [OperationView]> {
    match model.references.get(&r.tx.id)?.last.as_ref()? {
        Ok(References {
            destinations: Some(d),
            ..
        }) => Some(d),
        _ => None,
    }
}

/// The block's destinations that are this row's to show: what it paid,
/// for a spend; what reached the account, for a payment received.
fn mine<'a>(r: &Row<'_>, found: &'a [OperationView]) -> Vec<&'a OperationView> {
    let to_me = |d: &&OperationView| d.party == Party::Account(r.account);
    match r.kind {
        Kind::Received => found.iter().filter(to_me).collect(),
        _ => found.iter().filter(|d| !to_me(d)).collect(),
    }
}

/// A row's amount, signed, and its colour: received in the accent, sent in
/// the primary ink, a transfer between the store's own accounts unsigned.
pub(super) fn amount(r: &Row<'_>, unit: AmountUnit) -> (String, iced::Color) {
    let figure = ui::amount(r.amount.unsigned_abs(), unit);
    match r.kind {
        Kind::Own => (figure, color::TEXT_PRIMARY),
        _ if r.amount > 0 => (format!("+{figure}"), color::ACCENT),
        _ if r.amount < 0 => (format!("−{figure}"), color::TEXT_PRIMARY),
        _ => (figure, color::TEXT_SECONDARY),
    }
}

/// The chosen transaction, whole.
fn detail<'a>(model: &'a Model, page: &'a WalletPage, r: &Row<'a>) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let (title, _) = describe(r);
    let (figure, ink) = amount(r, unit);
    let mut card = column![
        row![
            t(title, ty::CARD_TITLE, color::TEXT_PRIMARY)
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
            ui::pill(
                r.tx.block.map_or_else(
                    || "in a block".to_owned(),
                    |b| format!("block {}", ui::group(&b.to_string()))
                ),
                ty::CHIP,
                color::ACCENT_SOFT,
                color::ACCENT,
                28.0,
            ),
        ]
        .spacing(sp::S12)
        .align_y(Alignment::Center),
        t(
            r.tx.time_ms.map_or_else(
                || "time not given".to_owned(),
                |ms| ui::full_time(ms, model.zone)
            ),
            ty::NOTE,
            color::TEXT_MUTED
        ),
        row![
            t(figure, ty::STAT_VALUE, ink),
            t(ui::unit_name(unit), ty::CHIP, color::ACCENT),
        ]
        .spacing(sp::S8)
        .align_y(Alignment::End),
    ]
    .spacing(sp::S12);
    let parties: Vec<&OperationView> = match r.kind {
        Kind::Received => r.payers().collect(),
        _ => r.payees().collect(),
    };
    let found = read_references(model, r);
    // Each payee's reference is the block's destination to the same party
    // for the same amount; a payment received carries its reference on
    // the destination that reached this account, shown below.
    let mut unmatched: Vec<&OperationView> = match (found, r.kind) {
        (Some(f), Kind::Sent | Kind::Own | Kind::Other) => mine(r, f),
        _ => Vec::new(),
    };
    if !parties.is_empty() {
        let mut list = column![].spacing(sp::S10);
        for op in parties {
            let shown = match &op.party {
                Party::Account(id) => id.destination().unwrap_or_else(|| id.hex()),
                _ => party(op),
            };
            let mut who = column![
                t(shown, ty::MONO_SMALL, color::TEXT_PRIMARY).wrapping(Wrapping::WordOrGlyph)
            ]
            .spacing(sp::S2);
            if !op.memo.is_empty() {
                who = who.push(t(op.memo.clone(), ty::MONO_TINY, color::TEXT_MUTED));
            } else if let Some(at) = unmatched
                .iter()
                .position(|d| d.party == op.party && d.amount == op.amount)
            {
                who = who.push(reference(&unmatched.remove(at).memo));
            }
            list = list.push(
                row![
                    who.width(Length::Fill),
                    t(
                        ui::amount(op.amount.unsigned_abs(), unit),
                        ty::TABLE_AMOUNT,
                        color::TEXT_PRIMARY
                    ),
                ]
                .spacing(sp::S12)
                .align_y(Alignment::Center),
            );
        }
        card = card.push(
            container(list)
                .padding(sp::S14)
                .width(Length::Fill)
                .style(theme::field_box),
        );
    }
    let confirmations = match (r.tx.block, model.node.tip.map(|(tip, _)| tip)) {
        (Some(b), Some(tip)) if tip >= b => format!(
            "{} · {} confirmations",
            ui::group(&b.to_string()),
            ui::group(&(tip - b + 1).to_string())
        ),
        (Some(b), _) => ui::group(&b.to_string()),
        (None, _) => "—".to_owned(),
    };
    if let Some(f) = found
        && r.kind == Kind::Received
    {
        let to_me: Vec<&str> = mine(r, f).iter().map(|d| d.memo.as_str()).collect();
        let shown = if to_me.iter().all(|m| m.is_empty()) {
            "none".to_owned()
        } else {
            to_me
                .into_iter()
                .filter(|m| !m.is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
        };
        card = card.push(fact("Reference", shown, true));
    }
    if let Some(line) = reading(model, r) {
        card = card.push(line);
    }
    card = card
        .push(fact("Transaction id", r.tx.id.clone(), true))
        .push(fact("Block", confirmations, false))
        .push(fact("Account", name(r.account), false));
    let fee = r.tx.fee();
    if fee > 0 {
        card = card.push(fact(
            "Fee",
            format!("{} {}", ui::amount(fee, unit), ui::unit_name(unit)),
            false,
        ));
    }
    let mut parts = column![ui::card(card)].spacing(sp::S16);
    match model.references.get(&r.tx.id).and_then(|e| e.last.as_ref()) {
        Some(Ok(read)) => {
            parts = parts.push(report::show(page, ReportKey::Block, report::block(read)));
        }
        Some(Err(refusal)) => {
            let what = r.tx.block.map_or_else(
                || "the block".to_owned(),
                |b| format!("block {}", ui::group(&b.to_string())),
            );
            parts = parts.push(report::show(
                page,
                ReportKey::Block,
                report::explorer(refusal, &what),
            ));
        }
        None => {}
    }
    parts
        .push(report::show(
            page,
            ReportKey::History,
            report::history(r.history),
        ))
        .into()
}

/// A destination's reference, read from the block.
fn reference<'a>(memo: &str) -> Element<'a, Message> {
    if memo.is_empty() {
        t("No reference", ty::NOTE, color::TEXT_MUTED).into()
    } else {
        t(
            format!("Reference {memo}"),
            ty::MONO_TINY,
            color::TEXT_SECONDARY,
        )
        .wrapping(Wrapping::WordOrGlyph)
        .into()
    }
}

/// Where the read of the transaction's references stands, while there is
/// something to say or do: none has been made (the row shown by default
/// was not clicked), one is on its way, or the node refused the last.
fn reading<'a>(model: &'a Model, r: &Row<'a>) -> Option<Element<'a, Message>> {
    // A mining reward pays no destination, so carries no reference.
    if r.kind == Kind::Reward {
        return None;
    }
    let read = model.references.get(&r.tx.id);
    if read.is_some_and(|e| matches!(e.last, Some(Ok(_))) && !e.reading) {
        return None;
    }
    let Some(block) = r.tx.block else {
        return Some(
            ui::helper("The index gives no block for it, so its references cannot be read.").into(),
        );
    };
    let number = ui::group(&block.to_string());
    if read.is_some_and(|e| e.reading) {
        return Some(ui::helper(format!("Reading block {number} for its references…")).into());
    }
    let (label, note) = if read.is_some() {
        (
            "Read again",
            format!("Block {number} was not served; its references are not known."),
        )
    } else {
        (
            "Read references",
            format!("The index carries none; block {number} does."),
        )
    };
    Some(
        row![
            ui::button_with(
                label,
                theme::Button::Secondary,
                Size::Small,
                None,
                (model.node.url.is_some())
                    .then(|| WalletMsg::ReadReferences(r.tx.id.clone()).into()),
            ),
            t(note, ty::NOTE, color::TEXT_SECONDARY)
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
        ]
        .spacing(sp::S12)
        .align_y(Alignment::Center)
        .into(),
    )
}

fn fact<'a>(label: &'static str, value: String, mono: bool) -> Element<'a, Message> {
    row![
        t(label, ty::BODY_SMALL, color::TEXT_SECONDARY).width(Length::FillPortion(2)),
        t(
            value,
            if mono { ty::MONO_SMALL } else { ty::TABLE_BODY },
            color::TEXT_PRIMARY
        )
        .wrapping(Wrapping::WordOrGlyph)
        .width(Length::FillPortion(5)),
    ]
    .spacing(sp::S12)
    .into()
}
