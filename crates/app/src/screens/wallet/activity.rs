//! W10, Activity (rendering 04): every account's transactions from the
//! node's index, and one transaction's detail beside them
//! (docs/SCREENS.md W10).
//!
//! The rows are the index's, the newest 100 for each account and older ones
//! a page at a time on "Read older" (D30); a read again starts from the
//! newest. Spends the store has reserved and not settled are its own record,
//! listed
//! with no amount since the store keeps none. "Export CSV" needs a file
//! dialog and "Receipt verified" claims a check nothing makes (D27, items 3
//! and 5): neither is built.

use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, space, text_input};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::explorer::{AccountHistory, OperationView, Party};
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
        other => {
            body.push(match other {
                Some(Err(refusal)) => report::show(
                    page,
                    ReportKey::Explorer,
                    report::explorer(refusal, "its transaction index"),
                ),
                _ => ui::card(ui::helper(if read.reading {
                    "Reading the node's index…"
                } else if model.node.url.is_none() {
                    "No node is chosen: Activity reads the transactions from the node's index."
                } else {
                    "Not read yet."
                }))
                .into(),
            });
            // The store's own record needs no index: its spends still
            // settling are listed whatever the index answered.
            let pending = pending_rows(model, a);
            if !pending.is_empty() {
                let mut list = column![header(model), ui::divider()].spacing(sp::S4);
                for row in pending {
                    list = list.push(row);
                }
                body.push(ui::card(list).padding(sp::S12).into());
            }
        }
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
    let mut list = column![header(model), ui::divider()].spacing(sp::S4);
    let mut any = false;
    for row in pending_rows(model, a) {
        any = true;
        list = list.push(row);
    }
    let chosen = a
        .selected
        .as_ref()
        .and_then(|id| rows.iter().find(|r| r.is(id)))
        .or(rows.first());
    for r in &rows {
        any = true;
        let selected = chosen.is_some_and(|c| c.is(&r.id()));
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
    let unread: u64 = histories.iter().map(AccountHistory::unread).sum();
    if unread > 0 {
        let reading = model.activity.reading;
        list = list.push(
            row![
                container(ui::helper(format!(
                    "The index holds {unread} older {} for these accounts, not read yet. They \
                     are read 100 for each account at a time.",
                    if unread == 1 { "row" } else { "rows" }
                )))
                .width(Length::Fill),
                ui::button_with(
                    if reading { "Reading…" } else { "Read older" },
                    theme::Button::Secondary,
                    Size::Small,
                    None,
                    (model.busy.is_none() && !reading).then(|| WalletMsg::ReadOlderActivity.into()),
                ),
            ]
            .spacing(sp::S12)
            .align_y(Alignment::Center)
            .padding(sp::S12),
        );
    }
    let detail: Element<'a, Message> = match chosen {
        Some(r) => detail(model, page, r),
        None => ui::card(ui::helper("Choose a transaction to see it whole.")).into(),
    };
    pair(model, ui::card(list).padding(sp::S12).into(), detail, 7, 5)
}

/// The list's column heads.
fn header(model: &Model) -> Element<'_, Message> {
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
    .padding(Padding::from([sp::S10, sp::S12]))
    .into()
}

/// The store's spends reserved and not settled, as the filter and the
/// search admit them: its own record, which needs no index.
fn pending_rows<'a>(model: &'a Model, a: &ActivityPage) -> Vec<Element<'a, Message>> {
    let Some(w) = &model.wallet else {
        return Vec::new();
    };
    if !a.filter.shows_pending() || !a.search.trim().is_empty() {
        return Vec::new();
    }
    history::pending(&w.accounts)
        .map(|account| pending_row(model, account))
        .collect()
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
    let (title, detail) = describe(r);
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
    .on_press(WalletMsg::Select(r.id()).into())
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
        Kind::Own => match (payees.as_slice(), r.payers().next()) {
            // Seen from the account it left: to the store's other account.
            ([to], _) => format!("{} → {}", name(r.account), party(to)),
            // To several of the store's accounts at once: the amount is
            // theirs together, so no one of them is named for it.
            ([_, _, ..], _) => {
                format!("Batch between own accounts · {} destinations", payees.len())
            }
            // Only the receiving side is in the index's answer.
            ([], Some(from)) => format!("{} → {}", party(from), name(r.account)),
            ([], None) => "Between own accounts".to_owned(),
        },
        Kind::Reward => "Mining reward".to_owned(),
        Kind::Other => "Transaction".to_owned(),
    };
    let references = r.references();
    let detail = if r.kind == Kind::Own && payees.len() > 1 {
        format!(
            "From {} to {}",
            name(r.account),
            listed(payees.iter().map(|o| party(o)))
        )
    } else if r.kind == Kind::Own {
        "Between own accounts".to_owned()
    } else if references.is_empty() {
        format!("tx {}", short(&r.tx.id))
    } else {
        references.join(" · ")
    };
    (title, detail)
}

/// `names` as a person reads a list: "B and C", "B, C and D", and past
/// three, "B, C, D and 2 more".
fn listed(names: impl Iterator<Item = String>) -> String {
    const SHOWN: usize = 3;
    let names: Vec<String> = names.collect();
    let more = names.len().saturating_sub(SHOWN);
    let shown = &names[..names.len().min(SHOWN)];
    match (shown, more) {
        ([], _) => String::new(),
        ([one], 0) => one.clone(),
        (_, 0) => format!(
            "{} and {}",
            shown[..shown.len() - 1].join(", "),
            shown[shown.len() - 1]
        ),
        (_, more) => format!("{} and {more} more", shown.join(", ")),
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
    // A payment received carries its reference on the destination that
    // reached the account, not on the payer listed above.
    if r.kind == Kind::Received {
        let references = r.references();
        card = card.push(fact(
            "Reference",
            if references.is_empty() {
                "none".to_owned()
            } else {
                references.join(" · ")
            },
            !references.is_empty(),
        ));
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
    column![
        ui::card(card),
        report::show(page, ReportKey::History, report::history(r.history)),
    ]
    .spacing(sp::S16)
    .into()
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

#[cfg(test)]
mod tests {
    use tawara_wallet_core::explorer::{OperationKind, TransactionView};
    use tawara_wallet_core::view::AccountId;

    use super::*;

    fn op(kind: OperationKind, account: AccountId, amount: i128) -> OperationView {
        OperationView {
            kind,
            party: Party::Account(account),
            amount,
            memo: String::new(),
        }
    }

    fn history(account: AccountId, tx: &TransactionView) -> AccountHistory {
        AccountHistory {
            account,
            transactions: vec![tx.clone()],
            total: 1,
            next: 1,
            text: String::new(),
        }
    }

    #[test]
    fn a_transfer_to_several_own_accounts_names_none_of_them_for_the_whole() {
        use OperationKind::{Destination, Source};
        let [a, b, c] = [0xa1, 0xb2, 0xc3].map(|n| AccountId::from_tag([n; 20]));
        // A pays B 10 and C 20, with 5 back as change.
        let batch = TransactionView {
            id: "batch".to_owned(),
            block: Some(900),
            time_ms: None,
            operations: vec![
                op(Source, a, -35),
                op(Destination, b, 10),
                op(Destination, c, 20),
                op(Destination, a, 5),
            ],
        };
        let histories = [history(a, &batch), history(b, &batch), history(c, &batch)];
        let rows = history::rows(&histories);
        assert_eq!(rows.len(), 1, "listed once, from the account it left");
        assert_eq!((rows[0].kind, rows[0].amount), (Kind::Own, 30));
        let (title, detail) = describe(&rows[0]);
        assert_eq!(title, "Batch between own accounts · 2 destinations");
        assert_eq!(
            detail,
            format!("From {} to {} and {}", name(a), name(b), name(c))
        );

        // To one of them, the arrow names it.
        let single = TransactionView {
            id: "single".to_owned(),
            operations: vec![
                op(Source, a, -15),
                op(Destination, b, 10),
                op(Destination, a, 5),
            ],
            ..batch
        };
        let histories = [history(a, &single), history(b, &single)];
        let rows = history::rows(&histories);
        let (title, detail) = describe(&rows[0]);
        assert_eq!(title, format!("{} → {}", name(a), name(b)));
        assert_eq!(detail, "Between own accounts");
    }

    #[test]
    fn a_list_of_names_reads_as_a_person_writes_it() {
        let names = |n: usize| (0..n).map(|i| format!("N{i}"));
        assert_eq!(listed(names(0)), "");
        assert_eq!(listed(names(1)), "N0");
        assert_eq!(listed(names(2)), "N0 and N1");
        assert_eq!(listed(names(3)), "N0, N1 and N2");
        assert_eq!(listed(names(5)), "N0, N1, N2 and 2 more");
    }
}
