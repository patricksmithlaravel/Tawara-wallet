//! The explorer (renderings 06, 07 and 08, and 04's detail): the chain and
//! the node's queue (E1), one block (E2), one account by its tag (E3) and
//! one transaction (E4), each read from the node (docs/SCREENS.md E1 to
//! E4).
//!
//! Everything here is the node's, read through the library: what it serves
//! of its chain, its queue and its transaction index. Nothing is the
//! store's, apart from saying which accounts are the store's own ("Yours",
//! "Your account"), and nothing on these pages writes. The figures the node
//! does not send (a block's solve time, the average, the next neogenesis,
//! confirmations) are worked out as the page says, from what it did send.

use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, space, text_input};
use iced::{Alignment, Color, Element, Length, Padding};
use tawara_wallet_core::explorer::{
    self, BlockAt, BlockDetail, BlockKind, BlockSummary, ChainView, LedgerRead, OperationKind,
    OperationView, Party, TagView, TransactionView,
};
use tawara_wallet_core::view::AccountId;

use super::activity::party;
use super::{copy, frame, name, pair, report, settings, short};
use crate::app::{
    BLOCK_ROWS, BlockPage, ExplorerPage, Message, Model, QueuePage, ReportKey, TagPage, To,
    TransactionPage, WalletMsg, WalletPage,
};
use crate::icon::{self, Icon};
use crate::theme::{self, color, space as sp};
use crate::ui::{self, Size, t, ty};

/// A link to `to`, in `face`: a block's number, an account's name.
fn link<'a>(content: String, face: ui::Type, to: impl Into<Message>) -> Element<'a, Message> {
    button(ui::label(content, face))
        .padding(0)
        .style(theme::button(theme::Button::Link))
        .on_press(to.into())
        .into()
}

fn open(to: To) -> WalletMsg {
    WalletMsg::Open(to)
}

/// A block's number, grouped, as a link to its page.
fn block_link<'a>(index: u64, face: ui::Type) -> Element<'a, Message> {
    link(
        ui::group(&index.to_string()),
        face,
        open(To::Block(BlockAt::Number(index))),
    )
}

/// Who an operation names, as a link to the account's page when it names
/// one; a ledger address is not an account to look up, nor a destination.
fn party_link<'a>(op: &OperationView, face: ui::Type) -> Element<'a, Message> {
    match &op.party {
        Party::Account(id) => link(name(*id), face, open(To::Tag(*id))),
        _ => t(party(op), face, color::TEXT_SECONDARY).into(),
    }
}

/// `ms` as a solve time reads: "58s", "1m 06s", "1h 02m".
fn duration(ms: i64) -> String {
    let secs = ms.max(0) / 1_000;
    match secs {
        ..60 => format!("{secs}s"),
        60..3_600 => format!("{}m {:02}s", secs / 60, secs % 60),
        _ => format!("{}h {:02}m", secs / 3_600, secs % 3_600 / 60),
    }
}

/// A block's kind as a pill: neogenesis green, pseudo amber, normal raised;
/// a block the node sent no figures for has none.
fn kind_pill<'a>(kind: Option<BlockKind>) -> Element<'a, Message> {
    let (word, fill, ink) = match kind {
        Some(BlockKind::Neogenesis) => ("Neogenesis", color::ACCENT_SOFT, color::ACCENT),
        Some(BlockKind::Pseudo) => ("Pseudo", color::WARNING_SOFT, color::WARNING),
        Some(BlockKind::Normal) => ("Normal", color::BG_RAISED, color::TEXT_PRIMARY),
        None => ("Not sent", color::BG_SURFACE, color::TEXT_MUTED),
    };
    ui::pill(word, ty::BADGE, fill, ink, 22.0)
}

/// How tall a figure's tile is, whether or not it has a line under its
/// value, so tiles side by side line up.
const TILE: f32 = 104.0;

/// An account's destination shortened to its first and last four
/// characters, for a narrow column ("3kPx…Wq7R", as 06 shortens a miner).
fn brief(id: AccountId) -> String {
    let full = id.destination().unwrap_or_else(|| id.hex());
    let chars: Vec<char> = full.chars().collect();
    if chars.len() <= 10 {
        return full;
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// A figure in a tile: its label, its value, and a line under it.
fn stat<'a>(
    label: &'static str,
    value: String,
    under: Option<String>,
    accent: bool,
) -> Element<'a, Message> {
    let mut body = column![
        t(label, ty::NOTE, color::TEXT_MUTED),
        t(
            value,
            ty::STAT_VALUE,
            if accent {
                color::ACCENT
            } else {
                color::TEXT_PRIMARY
            }
        )
        .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(sp::S6);
    if let Some(under) = under {
        body = body.push(t(under, ty::TINY, color::TEXT_SECONDARY));
    }
    container(body)
        .padding(sp::S16)
        .width(Length::Fill)
        .height(Length::Fixed(TILE))
        .style(if accent {
            theme::stat_tile_accent
        } else {
            theme::stat_tile
        })
        .into()
}

/// Rows of tiles: all in one row in a wide window, `per` to a row below.
fn tiles<'a>(model: &Model, tiles: Vec<Element<'a, Message>>, per: usize) -> Element<'a, Message> {
    let per = if model.width >= crate::app::WIDE {
        tiles.len().max(1)
    } else {
        per.max(1)
    };
    let mut rows = column![].spacing(sp::S12);
    let mut current = row![].spacing(sp::S12);
    let mut n = 0;
    for tile in tiles {
        current = current.push(tile);
        n += 1;
        if n == per {
            rows = rows.push(current.height(Length::Shrink));
            current = row![].spacing(sp::S12);
            n = 0;
        }
    }
    if n > 0 {
        rows = rows.push(current);
    }
    rows.into()
}

/// The path back to the explorer: "Explorer / Blocks / 871,172".
fn crumbs<'a>(section: &'static str, here: String) -> Element<'a, Message> {
    row![
        link("Explorer".to_owned(), ty::BODY_SMALL, open(To::Explorer)),
        t("/", ty::BODY_SMALL, color::TEXT_MUTED),
        t(section, ty::BODY_SMALL, color::TEXT_MUTED),
        t("/", ty::BODY_SMALL, color::TEXT_MUTED),
        t(here, ty::BODY_SMALL, color::TEXT_PRIMARY),
    ]
    .spacing(sp::S8)
    .align_y(Alignment::Center)
    .into()
}

/// A column head, `portion` parts of the row's width.
fn head<'a>(label: String, portion: u16, end: bool) -> Element<'a, Message> {
    let cell = container(t(label, ty::TABLE_HEADER, color::TEXT_MUTED))
        .width(Length::FillPortion(portion));
    if end {
        cell.align_x(Alignment::End).into()
    } else {
        cell.into()
    }
}

/// A cell, `portion` parts of the row's width.
fn cell<'a>(
    content: impl Into<Element<'a, Message>>,
    portion: u16,
    end: bool,
) -> Element<'a, Message> {
    let cell = container(content).width(Length::FillPortion(portion));
    if end {
        cell.align_x(Alignment::End).into()
    } else {
        cell.into()
    }
}

fn table_row<'a>(cells: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut r = row![].spacing(sp::S12).align_y(Alignment::Center);
    for c in cells {
        r = r.push(c);
    }
    container(r)
        .padding(Padding::from([sp::S12, 0.0]))
        .width(Length::Fill)
        .into()
}

/// Whether `party` is one of the open store's accounts.
fn ours(model: &Model, party: Option<&Party>) -> bool {
    match (party, &model.wallet) {
        (Some(Party::Account(id)), Some(w)) => w.accounts.iter().any(|a| a.id == *id),
        _ => false,
    }
}

/// The tip as the explorer last read it, or as the node last said.
fn tip(model: &Model) -> Option<u64> {
    match &model.chain.last {
        Some(Ok(c)) => Some(c.tip),
        _ => model.node.tip.map(|(tip, _)| tip),
    }
}

/// How many blocks stand on `block`, itself among them, at the higher of
/// the tip read with the page a transaction was opened from and the
/// explorer's own, which can be older (E4).
fn confirmations(block: u64, read: Option<u64>, explorer: Option<u64>) -> Option<u64> {
    read.max(explorer)?.checked_sub(block)?.checked_add(1)
}

// ---- E1 --------------------------------------------------------------------

/// E1: the chain, the node's queue, and the search field.
pub fn overview<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    e: &'a ExplorerPage,
) -> Element<'a, Message> {
    let network = model.node.network.as_ref().map_or_else(
        || "The chain".to_owned(),
        |n| format!("{} · {}", n.blockchain, n.network),
    );
    let reading = model.chain.reading || model.pending.reading;
    let mut actions: Vec<Element<'a, Message>> = Vec::new();
    if let Some(at) = model.chain.read_ms {
        actions.push(
            container(
                row![
                    ui::dot(color::ACCENT, 8.0),
                    t(
                        format!("Read {}", ui::age(model.clock_ms, at)),
                        ty::CHIP,
                        color::TEXT_SECONDARY
                    ),
                ]
                .spacing(sp::S8)
                .align_y(Alignment::Center),
            )
            .padding(Padding::from([sp::S8, sp::S14]))
            .style(theme::pill(color::BG_SURFACE, color::TEXT_SECONDARY))
            .into(),
        );
    }
    actions.push(
        ui::button_with(
            if reading { "Reading…" } else { "Read again" },
            theme::Button::Secondary,
            Size::Header,
            Some(Icon::Refresh),
            (!reading && model.node.url.is_some()).then(|| WalletMsg::ReadChain.into()),
        )
        .into(),
    );
    let mut body = vec![search(e)];
    if let Some(m) = &e.missed {
        body.push(report::show(
            page,
            ReportKey::Explorer,
            report::missed_transaction(m),
        ));
        body.push(report::show(
            page,
            ReportKey::Missed,
            report::missed_block(m),
        ));
    }
    match &model.chain.last {
        Some(Ok(chain)) => {
            body.push(figures(model, chain));
            let right = column![queue(model), kinds()].spacing(sp::S24);
            body.push(pair(model, latest(model, chain), right.into(), 7, 5));
            body.push(report::show(
                page,
                ReportKey::Read,
                report::read(
                    "The node's page for these blocks",
                    &[
                        "The newest blocks, read down from the node's tip, one request each. A \
                         block's solve time is its time less the time of the block below it; the \
                         average is over the hundred blocks below the tip.",
                    ],
                    &chain.text,
                ),
            ));
        }
        Some(Err(refusal)) => body.push(report::show(
            page,
            ReportKey::Read,
            report::explorer(refusal, "its newest blocks"),
        )),
        None => body.push(
            ui::card(ui::helper(if model.chain.reading {
                "Reading the chain…"
            } else if model.node.url.is_none() {
                "No node is chosen: the explorer reads the chain from it."
            } else {
                "Not read yet."
            }))
            .into(),
        ),
    }
    frame(
        model,
        page,
        "Explorer",
        format!("{network}, read through your node"),
        actions,
        body,
    )
}

/// The search field: a block's number or hash, a transaction's id, an
/// address or a tag.
fn search(e: &ExplorerPage) -> Element<'_, Message> {
    let field = text_input(
        "Block number or hash, transaction id, address or tag",
        &e.search,
    )
    .font(ty::FIELD.font)
    .size(ty::FIELD.size)
    .padding(Padding::from([sp::S10, 0.0]))
    .style(theme::bare_field)
    .on_input(|s| WalletMsg::ExplorerTyped(s).into())
    .on_submit(WalletMsg::ExplorerFind.into());
    let bar = container(
        row![
            icon::icon(Icon::Search, 18.0, 2.0, color::TEXT_MUTED),
            field,
            ui::button_with(
                if e.finding.is_some() {
                    "Looking…"
                } else {
                    "Search"
                },
                theme::Button::Primary,
                Size::Medium,
                None,
                e.finding.is_none().then(|| WalletMsg::ExplorerFind.into()),
            ),
        ]
        .spacing(sp::S12)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([sp::S6, sp::S8]).left(sp::S16))
    .width(Length::Fill)
    .style(theme::card);
    let mut col = column![bar].spacing(sp::S8);
    if let Some(why) = &e.invalid {
        col = col
            .push(t(why.clone(), ty::BODY_SMALL, color::WARNING).wrapping(Wrapping::WordOrGlyph));
    }
    col.into()
}

/// The chain's figures (06's tiles).
fn figures<'a>(model: &'a Model, chain: &'a ChainView) -> Element<'a, Message> {
    let newest = chain.blocks.first();
    let waiting = match (&model.pending.last, &model.mempool.last) {
        (Some(Ok(p)), _) => Some(p.waiting),
        (_, Some(Ok(m))) => Some(m.waiting),
        _ => None,
    };
    let next = explorer::next_neogenesis(chain.tip);
    tiles(
        model,
        vec![
            stat(
                "Block height",
                ui::group(&chain.tip.to_string()),
                None,
                false,
            ),
            stat(
                "Last block",
                newest.map_or_else(|| "—".to_owned(), |b| ui::age(model.clock_ms, b.time_ms)),
                None,
                false,
            ),
            stat(
                "Average solve · last 100",
                chain.average_ms.map_or_else(|| "—".to_owned(), duration),
                None,
                false,
            ),
            stat(
                "Difficulty",
                newest
                    .and_then(|b| b.difficulty)
                    .map_or_else(|| "—".to_owned(), |d| d.to_string()),
                None,
                false,
            ),
            stat(
                "Mempool",
                waiting.map_or_else(|| "—".to_owned(), |w| ui::group(&w.to_string())),
                waiting.map(|_| "transactions waiting".to_owned()),
                false,
            ),
            stat(
                "Next neogenesis",
                ui::group(&next.to_string()),
                Some(format!("in {} blocks", next - chain.tip)),
                true,
            ),
        ],
        3,
    )
}

/// The newest blocks (06's table).
fn latest<'a>(model: &'a Model, chain: &'a ChainView) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let mut list = column![
        ui::section_label("Latest blocks"),
        table_row(vec![
            head("Block".to_owned(), 3, false),
            head("Type".to_owned(), 4, false),
            head("Txs".to_owned(), 2, true),
            head("Solve".to_owned(), 3, true),
            head("Miner".to_owned(), 4, false),
            head(format!("Reward ({})", ui::unit_name(unit)), 5, true),
            head("Age".to_owned(), 3, true),
        ]),
        ui::divider(),
    ]
    .spacing(sp::S4);
    for b in &chain.blocks {
        list = list.push(block_row(model, b)).push(ui::divider());
    }
    ui::card(list).into()
}

fn block_row<'a>(model: &'a Model, b: &'a BlockSummary) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let spends = b.transactions - usize::from(b.reward.is_some());
    let miner: Element<'a, Message> = match &b.reward {
        Some(r) => match &r.to {
            Party::Account(id) => link(brief(*id), ty::MONO_SMALL, open(To::Tag(*id))),
            other => t(
                match other {
                    Party::Ledger(hex) => short(hex),
                    Party::Other(text) => text.clone(),
                    Party::Account(_) => String::new(),
                },
                ty::MONO_SMALL,
                color::TEXT_SECONDARY,
            )
            .into(),
        },
        None => t("—", ty::MONO_SMALL, color::TEXT_MUTED).into(),
    };
    table_row(vec![
        cell(block_link(b.index, ty::MONO_SMALL), 3, false),
        cell(kind_pill(b.kind), 4, false),
        cell(
            t(spends.to_string(), ty::TABLE_BODY, color::TEXT_PRIMARY),
            2,
            true,
        ),
        cell(
            t(
                b.solve_ms.map_or_else(|| "—".to_owned(), duration),
                ty::TABLE_BODY,
                color::TEXT_SECONDARY,
            ),
            3,
            true,
        ),
        cell(miner, 4, false),
        cell(
            t(
                b.reward
                    .as_ref()
                    .map_or_else(|| "—".to_owned(), |r| ui::amount(r.amount, unit)),
                ty::TABLE_AMOUNT,
                color::TEXT_PRIMARY,
            ),
            5,
            true,
        ),
        cell(
            t(
                ui::age(model.clock_ms, b.time_ms)
                    .trim_end_matches(" ago")
                    .to_owned(),
                ty::TABLE_BODY,
                color::TEXT_SECONDARY,
            ),
            3,
            true,
        ),
    ])
}

/// The node's queue (06's mempool card).
fn queue(model: &Model) -> Element<'_, Message> {
    let unit = model.prefs.unit;
    let mut card = column![].spacing(sp::S4);
    match &model.pending.last {
        Some(Ok(p)) => {
            card = card.push(
                row![
                    ui::section_label("Mempool"),
                    space().width(Length::Fill),
                    t(
                        format!("{} waiting", ui::group(&p.waiting.to_string())),
                        ty::NOTE,
                        color::TEXT_MUTED
                    ),
                ]
                .align_y(Alignment::Center),
            );
            if p.waiting == 0 {
                card = card.push(ui::helper("This node's queue is empty."));
            }
            for (n, r) in p.rows.iter().enumerate() {
                card = card.push(pending_row(r, n, unit)).push(ui::divider());
            }
            let unread = p.waiting.saturating_sub(p.rows.len());
            if unread > 0 {
                card = card.push(
                    container(ui::helper(format!(
                        "{} more waiting, not read. The queue is this node's own; another node's \
                         may differ.",
                        ui::group(&unread.to_string())
                    )))
                    .padding(Padding::from([sp::S8, 0.0])),
                );
            }
            if p.waiting > 0 {
                card = card.push(link(
                    "View all pending".to_owned(),
                    ty::LINK,
                    open(To::Queue),
                ));
            }
        }
        Some(Err(_)) => {
            card = card.push(ui::section_label("Mempool")).push(t(
                "The node did not serve its queue.",
                ty::BODY_SMALL,
                color::TEXT_SECONDARY,
            ));
        }
        None => {
            card = card.push(ui::section_label("Mempool")).push(ui::helper(
                if model.pending.reading {
                    "Reading the queue…"
                } else {
                    "Not read yet."
                },
            ));
        }
    }
    ui::card(card).into()
}

fn pending_row(
    r: &explorer::PendingRow,
    n: usize,
    unit: tawara_wallet_core::preferences::AmountUnit,
) -> Element<'_, Message> {
    let Some(tx) = &r.transaction else {
        return container(
            column![
                t(short(&r.id), ty::MONO_SMALL, color::TEXT_SECONDARY),
                t(
                    "Left the queue before it was read: mined since, or dropped.",
                    ty::TINY,
                    color::TEXT_MUTED
                ),
            ]
            .spacing(sp::S2),
        )
        .padding(Padding::from([sp::S10, 0.0]))
        .into();
    };
    let destinations = tx.destinations().count();
    let content = row![
        column![
            t(short(&r.id), ty::MONO_SMALL, color::TEXT_PRIMARY),
            t(
                format!(
                    "{destinations} {} · fee {}",
                    if destinations == 1 {
                        "destination"
                    } else {
                        "destinations"
                    },
                    ui::amount(tx.fee(), unit)
                ),
                ty::TINY,
                color::TEXT_MUTED
            ),
        ]
        .spacing(sp::S2)
        .width(Length::Fill),
        t(
            ui::amount(tx.sent(), unit),
            ty::TABLE_AMOUNT,
            color::TEXT_PRIMARY
        ),
    ]
    .spacing(sp::S12)
    .align_y(Alignment::Center);
    button(content)
        .padding(Padding::from([sp::S10, 0.0]))
        .width(Length::Fill)
        .style(theme::button(theme::Button::Bare))
        .on_press(WalletMsg::OpenPending(n).into())
        .into()
}

/// What each kind of block is (06's legend), in the library's words.
fn kinds<'a>() -> Element<'a, Message> {
    let kind = |k, words: &'static str| {
        row![
            container(kind_pill(Some(k))).width(Length::Fixed(110.0)),
            t(words, ty::BODY_SMALL, color::TEXT_SECONDARY)
                .wrapping(Wrapping::Word)
                .width(Length::Fill),
        ]
        .spacing(sp::S12)
    };
    ui::card(
        column![
            ui::section_label("Block types"),
            kind(
                BlockKind::Neogenesis,
                "At every 256th block, it carries the ledger."
            ),
            kind(
                BlockKind::Pseudo,
                "Made when no block was solved in time: no transactions, and no proof of work is \
                 checked for it."
            ),
            kind(
                BlockKind::Normal,
                "Transactions, solved by proof of work. The node sends a haiku for it, made from \
                 its nonce."
            ),
        ]
        .spacing(sp::S14),
    )
    .into()
}

/// E1's "View all pending": every transaction waiting in the node's queue,
/// read whole up to the command line's most for one read.
pub fn queue_page<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    q: &'a QueuePage,
) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let mut body = vec![crumbs("Mempool", "All pending".to_owned())];
    let subtitle = match &q.read {
        Some(Ok(p)) => {
            let mut list = column![
                table_row(vec![
                    head("Transaction id".to_owned(), 6, false),
                    head("From".to_owned(), 4, false),
                    head("Destinations".to_owned(), 3, true),
                    head(format!("Amount ({})", ui::unit_name(unit)), 4, true),
                    head("Fee".to_owned(), 3, true),
                ]),
                ui::divider(),
            ]
            .spacing(sp::S4);
            if p.waiting == 0 {
                list = list.push(
                    container(ui::helper("This node's queue is empty."))
                        .padding(Padding::from([sp::S12, 0.0])),
                );
            }
            for (n, r) in p.rows.iter().enumerate() {
                list = list.push(queued(model, r, n)).push(ui::divider());
            }
            let unread = p.waiting.saturating_sub(p.rows.len());
            if unread > 0 {
                list = list.push(
                    container(ui::helper(format!(
                        "{} more waiting, not read: one read takes at most {} whole, as the \
                         command line's does.",
                        ui::group(&unread.to_string()),
                        explorer::QUEUE_ROWS
                    )))
                    .padding(Padding::from([sp::S12, 0.0])),
                );
            }
            body.push(
                ui::card(list)
                    .padding(Padding::from([sp::S20, sp::S24]))
                    .into(),
            );
            body.push(report::show(
                page,
                ReportKey::Read,
                report::read(
                    "The node's page for its queue",
                    &[
                        "Each transaction as the node's queue holds it: its source at what left \
                         it, net of its change, which is not listed. The queue is this node's \
                         own; another node's may differ.",
                    ],
                    &p.text,
                ),
            ));
            format!(
                "{} {} waiting in this node's queue",
                ui::group(&p.waiting.to_string()),
                if p.waiting == 1 {
                    "transaction"
                } else {
                    "transactions"
                }
            )
        }
        Some(Err(refusal)) => {
            body.push(report::show(
                page,
                ReportKey::Explorer,
                report::explorer(refusal, "its queue"),
            ));
            "Not served".to_owned()
        }
        None => {
            body.push(ui::card(ui::helper("Reading the queue…")).into());
            "Reading…".to_owned()
        }
    };
    frame(
        model,
        page,
        "Mempool",
        subtitle,
        vec![
            ui::button_with(
                if q.read.is_none() {
                    "Reading…"
                } else {
                    "Read again"
                },
                theme::Button::Secondary,
                Size::Header,
                Some(Icon::Refresh),
                q.read.is_some().then(|| open(To::Queue).into()),
            )
            .into(),
        ],
        body,
    )
}

/// One transaction of the queue's page, opening whole (E4); one that left
/// the queue before it was read says so.
fn queued<'a>(model: &'a Model, r: &'a explorer::PendingRow, n: usize) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let Some(tx) = &r.transaction else {
        return table_row(vec![
            cell(
                t(short(&r.id), ty::MONO_SMALL, color::TEXT_SECONDARY),
                6,
                false,
            ),
            cell(
                t(
                    "Left the queue before it was read: mined since, or dropped.",
                    ty::TINY,
                    color::TEXT_MUTED,
                ),
                14,
                false,
            ),
        ]);
    };
    let mut id = row![link(
        short(&r.id),
        ty::MONO_SMALL,
        WalletMsg::OpenPending(n)
    )]
    .spacing(sp::S8)
    .align_y(Alignment::Center);
    if ours(model, tx.source()) {
        id = id.push(ui::pill(
            "Yours",
            ty::BADGE,
            color::ACCENT,
            color::TEXT_ON_ACCENT,
            20.0,
        ));
    }
    let source: Element<'a, Message> = match tx
        .operations
        .iter()
        .find(|o| o.kind == OperationKind::Source)
    {
        Some(op) => party_link(op, ty::MONO_SMALL),
        None => t("—", ty::MONO_SMALL, color::TEXT_MUTED).into(),
    };
    table_row(vec![
        cell(id, 6, false),
        cell(source, 4, false),
        cell(
            t(
                tx.destinations().count().to_string(),
                ty::TABLE_BODY,
                color::TEXT_PRIMARY,
            ),
            3,
            true,
        ),
        cell(
            t(
                ui::amount(tx.sent(), unit),
                ty::TABLE_AMOUNT,
                color::TEXT_PRIMARY,
            ),
            4,
            true,
        ),
        cell(
            t(
                ui::amount(tx.fee(), unit),
                ty::TABLE_BODY,
                color::TEXT_SECONDARY,
            ),
            3,
            true,
        ),
    ])
}

// ---- E2 --------------------------------------------------------------------

/// E2: one block, whole.
pub fn block<'a>(model: &'a Model, page: &'a WalletPage, b: &'a BlockPage) -> Element<'a, Message> {
    let (title, here) = match b.at {
        BlockAt::Number(n) => {
            let n = ui::group(&n.to_string());
            (format!("Block {n}"), n)
        }
        BlockAt::Hash(h) => {
            let hex: String = h.iter().map(|x| format!("{x:02x}")).collect();
            ("Block".to_owned(), short(&hex))
        }
    };
    let mut body = vec![crumbs("Blocks", here)];
    let mut actions = Vec::new();
    let subtitle = match &b.read {
        Some(Ok(detail)) => {
            let index = detail.summary.index;
            actions.push(kind_pill(detail.summary.kind));
            // The node's number for the block is any u64: the blocks either
            // side are offered only where they can be numbered, and block 0
            // is never asked for (the node serves its newest for it).
            let older = index.checked_sub(1).filter(|n| *n > 0);
            actions.push(
                ui::button_with(
                    ui::group(&index.saturating_sub(1).to_string()),
                    theme::Button::Secondary,
                    Size::Small,
                    Some(Icon::ChevronLeft),
                    older.map(|n| open(To::Block(BlockAt::Number(n))).into()),
                )
                .into(),
            );
            let newer = index
                .checked_add(1)
                .filter(|_| detail.tip.or_else(|| tip(model)).is_none_or(|t| index < t));
            actions.push(
                ui::button_with(
                    ui::group(&index.saturating_add(1).to_string()),
                    theme::Button::Secondary,
                    Size::Small,
                    Some(Icon::ChevronRight),
                    newer.map(|n| open(To::Block(BlockAt::Number(n))).into()),
                )
                .into(),
            );
            body.extend(block_body(model, page, detail));
            let mut line = ui::full_time(detail.summary.time_ms, model.zone);
            if let Some(n) = detail.confirmations() {
                line.push_str(&format!(
                    " · {} {}",
                    ui::group(&n.to_string()),
                    if n == 1 {
                        "confirmation"
                    } else {
                        "confirmations"
                    }
                ));
            }
            line
        }
        Some(Err(refusal)) => {
            body.push(report::show(
                page,
                ReportKey::Explorer,
                report::block_refused(refusal, matches!(b.at, BlockAt::Hash(_))),
            ));
            "Not served".to_owned()
        }
        None => {
            body.push(ui::card(ui::helper("Reading the block…")).into());
            "Reading…".to_owned()
        }
    };
    let title = match &b.read {
        Some(Ok(detail)) => format!("Block {}", ui::group(&detail.summary.index.to_string())),
        _ => title,
    };
    frame(model, page, &title, subtitle, actions, body)
}

fn block_body<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    d: &'a BlockDetail,
) -> Vec<Element<'a, Message>> {
    let unit = model.prefs.unit;
    let haiku: Element<'a, Message> = {
        let lines = d
            .figures
            .as_ref()
            .map(|f| f.haiku.as_slice())
            .unwrap_or_default();
        let mut col = column![t(
            "Block haiku · as the node sent it",
            ty::SECTION_LABEL,
            color::ACCENT
        )]
        .spacing(sp::S8);
        if lines.is_empty() {
            col = col.push(t(
                match (d.summary.kind, &d.figures) {
                    (_, None) => "The node sent none of this block's figures.",
                    (Some(BlockKind::Normal), _) => "The node sent no haiku for this block.",
                    _ => "Only a normal block carries a haiku.",
                },
                ty::BODY,
                color::TEXT_SECONDARY,
            ));
        } else {
            for line in lines {
                col = col
                    .push(t(line.clone(), ty::HAIKU, color::TEXT_PRIMARY).wrapping(Wrapping::Word));
            }
            // For correctness: the text is the node's, and nothing here
            // checks it against the nonce it is made from.
            col = col.push(t(
                "The node sends this haiku; Tawara does not check it against the block's nonce.",
                ty::TINY,
                color::TEXT_MUTED,
            ));
        }
        container(col.spacing(sp::S10))
            .padding(sp::S32)
            .width(Length::Fill)
            // As tall as the three rows of figures beside it.
            .height(Length::Fixed(3.0 * TILE + 2.0 * sp::S12))
            .center_y(Length::Fixed(3.0 * TILE + 2.0 * sp::S12))
            .style(theme::hero_card)
            .into()
    };
    let figures = column![
        row![
            stat("Transactions", d.spends.len().to_string(), None, false),
            stat(
                "Solve time",
                d.summary.solve_ms.map_or_else(|| "—".to_owned(), duration),
                None,
                false
            ),
        ]
        .spacing(sp::S12),
        row![
            stat(
                "Difficulty",
                d.summary
                    .difficulty
                    .map_or_else(|| "—".to_owned(), |x| x.to_string()),
                None,
                false
            ),
            stat(
                "Miner reward",
                d.summary
                    .reward
                    .as_ref()
                    .map_or_else(|| "none".to_owned(), |r| ui::amount(r.amount, unit)),
                d.summary
                    .reward
                    .as_ref()
                    .map(|_| ui::unit_name(unit).to_owned()),
                false
            ),
        ]
        .spacing(sp::S12),
        row![
            stat("Fees", ui::amount(d.fees, unit), None, false),
            stat("Destinations paid", d.paid().to_string(), None, false),
        ]
        .spacing(sp::S12),
    ]
    .spacing(sp::S12);
    let mut facts = column![].spacing(sp::S4);
    let fact = |label: &'static str, value: Element<'a, Message>| {
        column![
            row![
                t(label, ty::BODY_SMALL, color::TEXT_SECONDARY).width(Length::FillPortion(2)),
                container(value).width(Length::FillPortion(9)),
            ]
            .spacing(sp::S12)
            .align_y(Alignment::Center)
            .padding(Padding::from([sp::S12, 0.0])),
            ui::divider(),
        ]
    };
    let mono = |text: String| -> Element<'a, Message> {
        t(text, ty::MONO_SMALL, color::TEXT_PRIMARY)
            .wrapping(Wrapping::WordOrGlyph)
            .into()
    };
    facts = facts.push(fact("Block hash", mono(d.summary.hash.clone())));
    facts = facts.push(fact(
        "Previous block",
        if d.parent > 0 {
            row![
                block_link(d.parent, ty::MONO_SMALL),
                t(short(&d.parent_hash), ty::MONO_SMALL, color::TEXT_MUTED),
            ]
            .spacing(sp::S10)
            .into()
        } else {
            mono(d.parent_hash.clone())
        },
    ));
    if let Some(f) = &d.figures {
        facts = facts
            .push(fact("Merkle root", mono(f.root.clone())))
            .push(fact("Nonce", mono(f.nonce.clone())))
            .push(fact(
                "Size",
                t(
                    format!(
                        "{} bytes · least fee {} {}",
                        ui::group(&f.size.to_string()),
                        ui::amount(f.minimum_fee, unit),
                        ui::unit_name(unit)
                    ),
                    ty::TABLE_BODY,
                    color::TEXT_PRIMARY,
                )
                .into(),
            ));
    }
    if let Some(r) = &d.summary.reward {
        let miner: Element<'a, Message> = match &r.to {
            Party::Account(id) => link(
                id.destination().unwrap_or_else(|| id.hex()),
                ty::MONO_SMALL,
                open(To::Tag(*id)),
            ),
            _ => mono(party(&OperationView {
                kind: OperationKind::Reward,
                party: r.to.clone(),
                amount: 0,
                memo: String::new(),
            })),
        };
        facts = facts.push(fact("Miner", miner));
    }
    vec![
        pair(model, haiku, figures.into(), 1, 1),
        ui::card(facts)
            .padding(Padding::from([sp::S8, sp::S24]))
            .into(),
        spends(model, page, d),
        report::show(
            page,
            ReportKey::Read,
            report::read(
                "The node's page for this block",
                &[
                    "The block as the node serves it. Each transaction lists its source at what \
                     left it, net of its change, which is not listed; the reward is newly minted, \
                     not moved.",
                ],
                &d.text,
            ),
        ),
    ]
}

/// A block's transactions, [`BLOCK_ROWS`] at a time.
fn spends<'a>(model: &'a Model, page: &'a WalletPage, d: &'a BlockDetail) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let from = match &page.page {
        crate::app::Page::Block(b) => b.rows_from,
        _ => 0,
    }
    .min(d.spends.len().saturating_sub(1) / BLOCK_ROWS * BLOCK_ROWS);
    let to = (from + BLOCK_ROWS).min(d.spends.len());
    let mut list = column![
        row![
            ui::section_label("Transactions"),
            space().width(Length::Fill),
            t(
                if d.spends.is_empty() {
                    "none".to_owned()
                } else {
                    format!(
                        "{}–{} of {}",
                        from + 1,
                        to,
                        ui::group(&d.spends.len().to_string())
                    )
                },
                ty::NOTE,
                color::TEXT_MUTED
            ),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(sp::S4);
    if d.spends.is_empty() {
        list = list.push(
            container(ui::helper(match d.summary.kind {
                Some(BlockKind::Pseudo) => "A pseudo-block carries no transactions.",
                Some(BlockKind::Neogenesis) => {
                    "A neogenesis block carries the ledger, not transactions."
                }
                _ => "This block carries no transactions besides its reward.",
            }))
            .padding(Padding::from([sp::S12, 0.0])),
        );
        return ui::card(list)
            .padding(Padding::from([sp::S20, sp::S24]))
            .into();
    }
    list = list
        .push(table_row(vec![
            head("Transaction id".to_owned(), 6, false),
            head("From".to_owned(), 4, false),
            head("Destinations".to_owned(), 3, true),
            head(format!("Amount ({})", ui::unit_name(unit)), 4, true),
            head("Fee".to_owned(), 3, true),
        ]))
        .push(ui::divider());
    for (n, tx) in d.spends.iter().enumerate().skip(from).take(BLOCK_ROWS) {
        let mut id = row![link(short(&tx.id), ty::MONO_SMALL, WalletMsg::OpenSpend(n))]
            .spacing(sp::S8)
            .align_y(Alignment::Center);
        let yours = ours(model, tx.source());
        if yours {
            id = id.push(ui::pill(
                "Yours",
                ty::BADGE,
                color::ACCENT,
                color::TEXT_ON_ACCENT,
                20.0,
            ));
        }
        let source: Element<'a, Message> = match tx
            .operations
            .iter()
            .find(|o| o.kind == OperationKind::Source)
        {
            Some(op) => party_link(op, ty::MONO_SMALL),
            None => t("—", ty::MONO_SMALL, color::TEXT_MUTED).into(),
        };
        let line = table_row(vec![
            cell(id, 6, false),
            cell(source, 4, false),
            cell(
                t(
                    tx.destinations().count().to_string(),
                    ty::TABLE_BODY,
                    color::TEXT_PRIMARY,
                ),
                3,
                true,
            ),
            cell(
                t(
                    ui::amount(tx.sent(), unit),
                    ty::TABLE_AMOUNT,
                    color::TEXT_PRIMARY,
                ),
                4,
                true,
            ),
            cell(
                t(
                    ui::amount(tx.fee(), unit),
                    ty::TABLE_BODY,
                    color::TEXT_SECONDARY,
                ),
                3,
                true,
            ),
        ]);
        list = list.push(if yours {
            container(line)
                .padding(Padding::from([0.0, sp::S8]))
                .style(theme::row_selected)
                .into()
        } else {
            line
        });
        list = list.push(ui::divider());
    }
    if d.spends.len() > BLOCK_ROWS {
        list = list.push(
            row![
                space().width(Length::Fill),
                ui::button_with(
                    "Previous",
                    theme::Button::Secondary,
                    Size::Small,
                    None,
                    (from > 0).then(|| WalletMsg::BlockRows(from - BLOCK_ROWS).into()),
                ),
                ui::button_with(
                    "Next",
                    theme::Button::Secondary,
                    Size::Small,
                    None,
                    (to < d.spends.len()).then(|| WalletMsg::BlockRows(to).into()),
                ),
            ]
            .spacing(sp::S8)
            .padding(Padding::from([sp::S12, 0.0]).bottom(0.0)),
        );
    }
    ui::card(list)
        .padding(Padding::from([sp::S20, sp::S24]))
        .into()
}

// ---- E3 --------------------------------------------------------------------

/// E3: one account, by its tag: what the ledger holds for it, its three
/// forms, and its history from the node's index.
pub fn tag<'a>(model: &'a Model, page: &'a WalletPage, tp: &'a TagPage) -> Element<'a, Message> {
    let account = tp.account;
    let base58 = account.destination().unwrap_or_else(|| account.hex());
    let own = model
        .wallet
        .as_ref()
        .and_then(|w| w.accounts.iter().find(|a| a.id == account));
    let mut actions: Vec<Element<'a, Message>> = Vec::new();
    if let Some(row) = own {
        actions.push(ui::pill(
            format!("Your account · {}", super::kind(row)),
            ty::CHIP,
            color::ACCENT,
            color::TEXT_ON_ACCENT,
            28.0,
        ));
    }
    actions.push(copy(page, &base58, "Copy"));
    // Only for an account the person typed: one reached through a link was
    // named by the node (docs/DECISIONS.md D31, item 5).
    if tp.typed {
        actions.push(
            ui::button_with(
                "Send to this account",
                theme::Button::Primary,
                Size::Header,
                Some(Icon::Send),
                model
                    .busy
                    .is_none()
                    .then(|| WalletMsg::SendTo(account).into()),
            )
            .into(),
        );
    }
    let mut body = vec![crumbs("Accounts", name(account))];
    match &tp.read {
        Some(view) => body.extend(tag_body(model, page, tp, view, own)),
        None => body.push(ui::card(ui::helper("Reading the account…")).into()),
    }
    frame(model, page, "Account", base58, actions, body)
}

fn tag_body<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    tp: &'a TagPage,
    view: &'a TagView,
    own: Option<&'a tawara_wallet_core::view::AccountRow>,
) -> Vec<Element<'a, Message>> {
    let unit = model.prefs.unit;
    let history = view.history.as_ref().ok();
    let balance = match &view.ledger {
        LedgerRead::Held { balance, .. } => ui::amount(*balance, unit),
        _ => "—".to_owned(),
    };
    let mut figures = vec![stat(
        "Balance",
        balance,
        matches!(view.ledger, LedgerRead::Held { .. }).then(|| ui::unit_name(unit).to_owned()),
        false,
    )];
    // Known only for this store's accounts, so shown only for them.
    if let Some(row) = own {
        figures.push(stat(
            "Current key on ledger",
            settings::on_ledger(row).map_or_else(|| "—".to_owned(), |i| format!("#{i}")),
            None,
            false,
        ));
    }
    figures.extend([
        stat(
            "Transactions",
            history.map_or_else(|| "—".to_owned(), |h| ui::group(&h.total.to_string())),
            history.map(|_| "in the node's index".to_owned()),
            false,
        ),
        stat(
            "Last active",
            history
                .and_then(|h| h.transactions.first())
                .and_then(|t| t.block)
                .map_or_else(|| "—".to_owned(), |b| ui::group(&b.to_string())),
            None,
            false,
        ),
    ]);
    let mut body = vec![tiles(model, figures, 2)];
    if view.ledger == LedgerRead::Unresolved {
        body.push(report::show(page, ReportKey::Ledger, report::unresolved()));
    }
    body.push(forms(view));
    match &view.history {
        Ok(h) => {
            body.push(tag_history(model, tp, view.account, h));
            body.push(report::show(page, ReportKey::History, report::history(h)));
        }
        Err(refusal) => body.push(report::show(
            page,
            ReportKey::Explorer,
            report::explorer(refusal, "its transaction index"),
        )),
    }
    body
}

/// One account, three forms (08): the address to share, the tag that never
/// changes, and the ledger address that changes with every spend.
fn forms(view: &TagView) -> Element<'_, Message> {
    let account = view.account;
    let form = |title: &'static str, note: &'static str, value: Element<'static, Message>| {
        column![
            row![
                column![
                    t(title, ty::TABLE_NAME, color::TEXT_PRIMARY),
                    t(note, ty::TINY, color::TEXT_MUTED).wrapping(Wrapping::Word),
                ]
                .spacing(sp::S2)
                .width(Length::FillPortion(3)),
                container(value).width(Length::FillPortion(9)),
            ]
            .spacing(sp::S16)
            .align_y(Alignment::Center)
            .padding(Padding::from([sp::S14, 0.0])),
            ui::divider(),
        ]
    };
    let mono = |text: String, ink: Color| -> Element<'static, Message> {
        t(text, ty::MONO_SMALL, ink)
            .wrapping(Wrapping::WordOrGlyph)
            .into()
    };
    let ledger: Element<'static, Message> = match &view.ledger {
        LedgerRead::Held { address, .. } => {
            // The tag, then the hash of the key it is held at now.
            let (tag, key) = address.split_at(address.len().min(42));
            row![
                mono(tag.to_owned(), color::TEXT_PRIMARY),
                mono(key.to_owned(), color::ACCENT),
            ]
            .wrap()
            .into()
        }
        LedgerRead::Unresolved => t(
            "Not found, or the lookup failed: see above.",
            ty::BODY_SMALL,
            color::TEXT_SECONDARY,
        )
        .wrapping(Wrapping::Word)
        .into(),
        LedgerRead::Refused(why) => t(why.clone(), ty::BODY_SMALL, color::WARNING)
            .wrapping(Wrapping::WordOrGlyph)
            .into(),
    };
    ui::card(
        column![
            ui::section_label("One account, three forms"),
            form(
                "Address",
                "Share this · it carries a checksum",
                mono(
                    account.destination().unwrap_or_else(|| account.hex()),
                    color::TEXT_PRIMARY
                ),
            ),
            form(
                "Tag (hex)",
                "The account's 20 bytes · never changes",
                mono(account.hex(), color::TEXT_PRIMARY),
            ),
            form(
                "Ledger address",
                "The tag and the hash of its current key · changes with every spend · not \
                 somewhere to send funds",
                ledger,
            ),
        ]
        .spacing(sp::S4),
    )
    .padding(Padding::from([sp::S20, sp::S24]))
    .into()
}

/// The account's transactions from the node's index (08's history).
fn tag_history<'a>(
    model: &'a Model,
    tp: &'a TagPage,
    account: AccountId,
    h: &'a tawara_wallet_core::explorer::AccountHistory,
) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let mut list = column![
        row![
            ui::section_label("History"),
            space().width(Length::Fill),
            t(
                "From the node's transaction index",
                ty::NOTE,
                color::TEXT_MUTED
            ),
        ]
        .align_y(Alignment::Center),
        table_row(vec![
            head("Block".to_owned(), 3, false),
            head("Transaction".to_owned(), 4, false),
            head("Counterparty".to_owned(), 5, false),
            head("Reference".to_owned(), 4, false),
            head(format!("Amount ({})", ui::unit_name(unit)), 4, true),
        ]),
        ui::divider(),
    ]
    .spacing(sp::S4);
    if h.transactions.is_empty() {
        list = list.push(
            container(ui::helper(
                "The node's index holds no transaction for this account.",
            ))
            .padding(Padding::from([sp::S12, 0.0])),
        );
    }
    for (n, tx) in h.transactions.iter().enumerate() {
        let (who, reference) = counterparty(tx, account);
        let net = tx.net(account);
        let figure = ui::amount(net.unsigned_abs(), unit);
        let (amount, ink) = match net {
            1.. => (format!("+{figure}"), color::ACCENT),
            ..0 => (format!("−{figure}"), color::TEXT_PRIMARY),
            0 => (figure, color::TEXT_SECONDARY),
        };
        list = list
            .push(table_row(vec![
                cell(
                    match tx.block {
                        Some(b) => block_link(b, ty::MONO_SMALL),
                        None => t("—", ty::MONO_SMALL, color::TEXT_MUTED).into(),
                    },
                    3,
                    false,
                ),
                cell(
                    link(short(&tx.id), ty::MONO_SMALL, WalletMsg::OpenTagRow(n)),
                    4,
                    false,
                ),
                cell(who, 5, false),
                cell(
                    t(reference, ty::MONO_SMALL, color::TEXT_SECONDARY)
                        .wrapping(Wrapping::WordOrGlyph),
                    4,
                    false,
                ),
                cell(t(amount, ty::TABLE_AMOUNT, ink), 4, true),
            ]))
            .push(ui::divider());
    }
    if h.more() {
        let unread = h.unread();
        list = list.push(
            row![
                container(ui::helper(format!(
                    "The index holds {} older {} for this account, not read yet. They are read \
                     100 at a time.",
                    ui::group(&unread.to_string()),
                    if unread == 1 { "row" } else { "rows" }
                )))
                .width(Length::Fill),
                ui::button_with(
                    if tp.reading_older {
                        "Reading…"
                    } else {
                        "Read older"
                    },
                    theme::Button::Secondary,
                    Size::Small,
                    None,
                    (!tp.reading_older).then(|| WalletMsg::ReadOlderTag.into()),
                ),
            ]
            .spacing(sp::S12)
            .align_y(Alignment::Center)
            .padding(Padding::from([sp::S12, 0.0])),
        );
    }
    ui::card(list)
        .padding(Padding::from([sp::S20, sp::S24]))
        .into()
}

/// Who a transaction moved value with, for `account`, and the references
/// on what reached it or what it paid: the payer of a payment received;
/// the payee of a spend, or how many it paid.
fn counterparty<'a>(tx: &'a TransactionView, account: AccountId) -> (Element<'a, Message>, String) {
    let more = |n: usize| {
        if n > 1 {
            format!(" +{}", n - 1)
        } else {
            String::new()
        }
    };
    let memos = |ops: Vec<&OperationView>| {
        let refs: Vec<&str> = ops
            .iter()
            .map(|o| o.memo.as_str())
            .filter(|m| !m.is_empty())
            .collect();
        refs.first()
            .map_or_else(|| "—".to_owned(), |r| format!("{r}{}", more(refs.len())))
    };
    if tx.rewards(account) {
        return (
            t("Mining reward", ty::MONO_SMALL, color::TEXT_SECONDARY).into(),
            "—".to_owned(),
        );
    }
    let payees: Vec<&OperationView> = tx.paid_out(account).collect();
    let payers: Vec<&OperationView> = tx.paid_by(account).collect();
    let paid_out = tx.net(account) < 0 || payers.is_empty();
    if paid_out && !payees.is_empty() {
        let who: Element<'a, Message> = if payees.len() == 1 {
            party_link(payees[0], ty::MONO_SMALL)
        } else {
            t(
                format!("{} destinations", payees.len()),
                ty::MONO_SMALL,
                color::TEXT_SECONDARY,
            )
            .into()
        };
        return (who, memos(payees));
    }
    let received: Vec<&OperationView> = tx
        .destinations()
        .filter(|o| o.party == Party::Account(account))
        .collect();
    let who: Element<'a, Message> = match payers.first() {
        Some(op) if payers.len() == 1 => party_link(op, ty::MONO_SMALL),
        Some(op) => row![
            party_link(op, ty::MONO_SMALL),
            t(more(payers.len()), ty::MONO_SMALL, color::TEXT_MUTED)
        ]
        .into(),
        None => t("—", ty::MONO_SMALL, color::TEXT_MUTED).into(),
    };
    (who, memos(received))
}

// ---- E4 --------------------------------------------------------------------

/// E4: one transaction, whole, as the index, a block or the queue listed
/// it.
pub fn transaction<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    tp: &'a TransactionPage,
) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let tx = &tp.transaction;
    let mut body = vec![crumbs("Transactions", short(&tx.id))];
    let mut head = column![].spacing(sp::S8);
    match tx.block {
        Some(b) => {
            let mut line = row![
                t("In block", ty::BODY_SMALL, color::TEXT_SECONDARY),
                block_link(b, ty::BODY_SMALL)
            ]
            .spacing(sp::S8)
            .align_y(Alignment::Center);
            if let Some(n) = confirmations(b, tp.tip, tip(model)) {
                line = line.push(t(
                    format!(
                        "· {} {}",
                        ui::group(&n.to_string()),
                        if n == 1 {
                            "confirmation"
                        } else {
                            "confirmations"
                        }
                    ),
                    ty::BODY_SMALL,
                    color::TEXT_MUTED,
                ));
            }
            head = head.push(line);
        }
        None => {
            head = head.push(t(
                "Waiting in the node's queue: in no block yet.",
                ty::BODY_SMALL,
                color::WARNING,
            ));
        }
    }
    if let Some(ms) = tx.time_ms {
        head = head.push(t(
            ui::full_time(ms, model.zone),
            ty::NOTE,
            color::TEXT_MUTED,
        ));
    }
    let mut card = column![head].spacing(sp::S20);
    for (kind, title) in [
        (OperationKind::Source, "From"),
        (OperationKind::Destination, "To"),
        (OperationKind::Reward, "Reward"),
    ] {
        let ops: Vec<&OperationView> = tx.operations.iter().filter(|o| o.kind == kind).collect();
        if ops.is_empty() {
            continue;
        }
        let mut list = column![ui::section_label(title)].spacing(sp::S10);
        for op in ops {
            let mut who = column![party_link(op, ty::MONO_SMALL)].spacing(sp::S2);
            if ours(model, Some(&op.party)) {
                who = who.push(t("this store's account", ty::TINY, color::ACCENT));
            }
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
    let mut facts = column![
        row![
            t("Transaction id", ty::BODY_SMALL, color::TEXT_SECONDARY)
                .width(Length::FillPortion(2)),
            t(tx.id.clone(), ty::MONO_SMALL, color::TEXT_PRIMARY)
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::FillPortion(7)),
        ]
        .spacing(sp::S12),
    ]
    .spacing(sp::S10);
    let fee = tx.fee();
    if fee > 0 {
        facts = facts.push(
            row![
                t("Fee", ty::BODY_SMALL, color::TEXT_SECONDARY).width(Length::FillPortion(2)),
                t(
                    format!("{} {}", ui::amount(fee, unit), ui::unit_name(unit)),
                    ty::TABLE_BODY,
                    color::TEXT_PRIMARY
                )
                .width(Length::FillPortion(7)),
            ]
            .spacing(sp::S12),
        );
    }
    card = card.push(facts).push(ui::helper(if tp.from_index {
        "As the node's index lists it: the source at its gross amount, and the change back to it \
         as a destination of its own."
    } else {
        "As its block or the node's queue lists it: the source at what left it, net of its \
         change, which is not listed."
    }));
    body.push(ui::card(card).into());
    if let Some(text) = &tp.text {
        body.push(report::show(
            page,
            ReportKey::Read,
            report::read(
                "The node's index's page for this transaction",
                &["The transaction as the node's index holds it."],
                text,
            ),
        ));
    }
    frame(
        model,
        page,
        "Transaction",
        short(&tx.id),
        vec![copy(page, &tx.id, "Copy id")],
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_solve_time_reads_as_the_rendering_writes_it() {
        assert_eq!(duration(58_000), "58s");
        assert_eq!(duration(66_000), "1m 06s");
        assert_eq!(duration(949_000), "15m 49s");
        assert_eq!(duration(3_720_000), "1h 02m");
        assert_eq!(duration(-5), "0s", "a clock behind its parent's");
    }

    #[test]
    fn a_transactions_confirmations_are_counted_from_the_newer_tip() {
        // Its block read at 1,001, the explorer's chain at 1,000.
        assert_eq!(confirmations(1_001, Some(1_001), Some(1_000)), Some(1));
        assert_eq!(confirmations(995, Some(1_001), Some(1_000)), Some(7));
        // The explorer read since, further on.
        assert_eq!(confirmations(995, Some(1_001), Some(1_003)), Some(9));
        assert_eq!(confirmations(1_001, None, Some(1_000)), None, "above it");
        assert_eq!(confirmations(5, None, None), None, "no tip read");
        assert_eq!(confirmations(0, Some(u64::MAX), None), None, "overflows");
    }
}
