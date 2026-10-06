//! How the wallet library's pages are shown (docs/DECISIONS.md D28, the
//! owner's decision): a slim banner, which opens into a summary in Tawara's
//! words with the causes that commonly lie behind it, so nobody is alarmed
//! by what is usually ordinary; and at the summary's foot, a link to the
//! library's full output, word for word.
//!
//! The summaries are the application's own text (docs/DECISIONS.md D25).
//! Each says what the library's page says and nothing it does not: the
//! cases come from what the worker reports (`DivergenceKind`,
//! `NoticeKind`, the replies' own fields), never from reading the page.

use iced::widget::{button, column, container, row};
use iced::{Alignment, Element, Length};
use tawara_wallet_core::explorer::{AccountHistory, ExplorerRefusal, IndexState};
use tawara_wallet_core::view::{AccountRow, AccountState, DivergenceKind, NoticeKind, WalletView};
use tawara_wallet_core::{AccountReport, Discovered, PlanView, SentView};

use super::name;
use crate::app::{Done, Level, Message, Missed, ReportKey, WalletMsg, WalletPage};
use crate::icon::{self, Icon};
use crate::theme::{self, color, space as sp};
use crate::ui::{self, t, ty};

/// How a report is coloured.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Something stopped, or needs the person.
    Warning,
    /// Something to know.
    Note,
}

/// A report: its banner, its summary, the causes that commonly lie behind
/// it, and the library's page.
pub struct Report<'a> {
    pub tone: Tone,
    pub title: String,
    pub summary: Vec<String>,
    /// What the list of causes is headed.
    pub causes_title: &'static str,
    pub causes: Vec<&'static str>,
    /// The library's output, whole; `None` where the text is the
    /// application's own and the summary already says all of it.
    pub full: Option<&'a str>,
}

impl<'a> Report<'a> {
    fn new(tone: Tone, title: impl Into<String>) -> Report<'a> {
        Report {
            tone,
            title: title.into(),
            summary: Vec::new(),
            causes_title: "Common causes",
            causes: Vec::new(),
            full: None,
        }
    }

    fn says(mut self, line: impl Into<String>) -> Report<'a> {
        self.summary.push(line.into());
        self
    }

    fn causes(mut self, causes: &[&'static str]) -> Report<'a> {
        for c in causes {
            if !self.causes.contains(c) {
                self.causes.push(c);
            }
        }
        self
    }

    fn full(mut self, text: &'a str) -> Report<'a> {
        self.full = Some(text);
        self
    }
}

/// `r` as the page shows it, open as far as the page has it open.
pub fn show<'a>(page: &WalletPage, key: ReportKey, r: Report<'a>) -> Element<'a, Message> {
    let level = page.open.get(&key).copied().unwrap_or_default();
    let (lead, ink, style): (Icon, _, fn(&iced::Theme) -> container::Style) = match r.tone {
        Tone::Warning => (Icon::Warning, color::WARNING, theme::callout_warning),
        Tone::Note => (Icon::ShieldCheck, color::ACCENT, theme::callout_accent),
    };
    let body_ink = match r.tone {
        Tone::Warning => color::TEXT_ON_WARNING_SOFT,
        Tone::Note => color::TEXT_ON_ACCENT_SOFT,
    };
    let open = level != Level::Closed;
    let banner = button(
        row![
            icon::icon(lead, 18.0, 2.0, ink),
            t(r.title, ty::ROW_TITLE, color::TEXT_PRIMARY).width(Length::Fill),
            t(
                if open { "Less" } else { "What this means" },
                ty::LINK_SMALL,
                ink
            ),
            icon::icon(
                if open {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                16.0,
                2.0,
                ink
            ),
        ]
        .spacing(sp::S10)
        .align_y(Alignment::Center),
    )
    .padding(0)
    .width(Length::Fill)
    .style(theme::button(theme::Button::Bare))
    .on_press(WalletMsg::Report(key, if open { Level::Closed } else { Level::Summary }).into());
    let mut card = column![banner].spacing(sp::S12);
    if open {
        let mut summary = column![].spacing(sp::S8);
        for line in r.summary {
            summary = summary.push(t(line, ty::BODY_SMALL, body_ink).width(Length::Fill));
        }
        card = card.push(summary);
        if !r.causes.is_empty() {
            let mut causes =
                column![t(r.causes_title, ty::FORM_LABEL, color::TEXT_PRIMARY)].spacing(sp::S6);
            for cause in r.causes {
                causes = causes.push(
                    row![
                        t("•", ty::BODY_SMALL, ink),
                        t(cause, ty::BODY_SMALL, body_ink).width(Length::Fill),
                    ]
                    .spacing(sp::S8),
                );
            }
            card = card.push(causes);
        }
        if let Some(full) = r.full {
            let whole = level == Level::Full;
            card = card.push(ui::link(
                if whole {
                    "Hide the wallet library's full report"
                } else {
                    "Show the wallet library's full report"
                },
                ty::LINK_SMALL,
                WalletMsg::Report(key, if whole { Level::Summary } else { Level::Full }).into(),
            ));
            if whole {
                card = card.push(ui::library_page(full));
            }
        }
    }
    container(card)
        .padding([sp::S14, sp::S16])
        .width(Length::Fill)
        .style(style)
        .into()
}

// ---------------------------------------------------------------- causes

const NEVER_A_CRASH: &str = "It is never a crash during a send: the key index is saved before \
                             any signature exists, so an interrupted send shows up as a \
                             reservation, not as this.";
const OLDER_COPY: &str = "This store is an older copy: restored from a backup, or copied before \
                          a spend was made from the newer one.";
const SECOND_WALLET: &str = "Another wallet holds the same recovery phrase and spent from this \
                             account.";
const NOT_LANDED: &str = "A spend from this account has not landed yet.";
const MOVED_AHEAD: &str = "This store was moved ahead of the chain.";
const OTHER_CHAIN: &str = "The node answers for another network, or is behind the chain.";
const SPENT_FURTHER: &str = "The account has spent more times than the search reaches.";
const NOT_THIS_SEED: &str = "This recovery phrase is not the one that made the account.";
const NOT_PAID: &str = "It has not been paid yet: a new account is unknown to the node until it \
                        first receives something.";
const EMPTIED: &str = "It was emptied: the ledger keeps no entry for an account at zero.";
const LOOKUP: &str = "The node serves another network, or the lookup failed: try again later, \
                      or another node.";
const NODE_DOWN: &str = "The node is down, busy or slow.";
const OFFLINE: &str = "This computer is offline.";
const NODE_ADDRESS: &str = "The node's address is mistyped or has changed: check it with Change \
                            node.";
const DAMAGED: &str = "The store's files could not be read as written.";
const ODD_ANSWER: &str = "The node gave an answer that could not be understood: try again, or \
                          another node.";

/// What a divergence of `kind` is, in a few words.
fn in_brief(kind: DivergenceKind) -> String {
    match kind {
        DivergenceKind::Ahead { gap } => format!("the chain is {} ahead of the store", keys(gap)),
        DivergenceKind::Behind { gap } => format!("the chain is {} behind the store", keys(gap)),
        DivergenceKind::Unlocated => {
            "the chain's address is not among the keys searched".to_owned()
        }
        DivergenceKind::ReservationUnexplained => {
            "something moved it that this store did not".to_owned()
        }
        DivergenceKind::NotFound => "the node does not find it".to_owned(),
        DivergenceKind::Unreachable => "the node could not be reached for it".to_owned(),
        DivergenceKind::NoMaster => "its keys cannot be derived".to_owned(),
        DivergenceKind::Failed => "reconciling it failed".to_owned(),
    }
}

fn keys(n: u32) -> String {
    if n == 1 {
        "1 key".to_owned()
    } else {
        format!("{n} keys")
    }
}

/// The causes that commonly lie behind a divergence of `kind`.
fn causes_of(kind: DivergenceKind) -> &'static [&'static str] {
    match kind {
        DivergenceKind::Ahead { .. } => &[OLDER_COPY, SECOND_WALLET, NEVER_A_CRASH],
        DivergenceKind::Behind { .. } => &[NOT_LANDED, MOVED_AHEAD, OTHER_CHAIN],
        DivergenceKind::Unlocated => &[SPENT_FURTHER, NOT_THIS_SEED, OTHER_CHAIN],
        DivergenceKind::ReservationUnexplained => &[SECOND_WALLET, OLDER_COPY],
        DivergenceKind::NotFound => &[NOT_PAID, EMPTIED, LOOKUP],
        DivergenceKind::Unreachable => &[NODE_DOWN, OFFLINE],
        DivergenceKind::NoMaster => &[],
        DivergenceKind::Failed => &[DAMAGED, ODD_ANSWER],
    }
}

/// A diverged account's report (W7).
pub fn diverged(kind: DivergenceKind, report: &str) -> Report<'_> {
    let (tone, title, summary) = match kind {
        DivergenceKind::Ahead { gap } => (
            Tone::Warning,
            format!(
                "Spending paused: the chain is {} ahead of this store",
                keys(gap)
            ),
            "A spend from this account landed that this store does not record. Tawara pauses \
             the account rather than sign with a key the chain has already used, so nothing is \
             at risk while it waits, and the other accounts are unaffected. Once you are sure no \
             other wallet uses this recovery phrase, account recovery moves the store to the key \
             the chain shows.",
        ),
        DivergenceKind::Behind { gap } => (
            Tone::Warning,
            format!(
                "Spending paused: the chain is {} behind this store",
                keys(gap)
            ),
            "The chain shows this account at an earlier key than this store holds. Tawara \
             pauses it rather than sign from a key the chain does not expect; the other \
             accounts are unaffected.",
        ),
        DivergenceKind::Unlocated => (
            Tone::Warning,
            "Spending paused: the chain's address is not among the keys searched".to_owned(),
            "The address the chain holds for this account is none of the keys this store \
             reached around its index. Tawara pauses it; the other accounts are unaffected.",
        ),
        DivergenceKind::ReservationUnexplained => (
            Tone::Warning,
            "Spending paused: something moved this account that this store did not".to_owned(),
            "A spend from this account is reserved, and the chain shows the account at neither \
             the key that signed it nor its change key. Tawara pauses it; the other accounts \
             are unaffected.",
        ),
        DivergenceKind::NotFound => (
            Tone::Note,
            "The node does not find this account".to_owned(),
            "The node answered \"account not found\". It says that for an account it holds no \
             entry for, for one at a zero balance, and when a lookup fails, and it does not say \
             which. Receiving to the account brings it back.",
        ),
        DivergenceKind::Unreachable => (
            Tone::Warning,
            "The node could not be reached for this account".to_owned(),
            "Nothing was reconciled for it, and nothing is wrong with the account itself. \
             Refresh once the node answers.",
        ),
        DivergenceKind::NoMaster => (
            Tone::Warning,
            "This store cannot derive this account's keys".to_owned(),
            "It is a derived account, and the store holds no master seed to derive its keys \
             from.",
        ),
        DivergenceKind::Failed => (
            Tone::Warning,
            "Reconciling this account failed".to_owned(),
            "The store could not be read for it, or the node's answer could not be \
             understood; the full report says which. The other accounts are unaffected.",
        ),
    };
    Report::new(tone, title)
        .says(summary)
        .causes(causes_of(kind))
        .full(report)
}

/// The store's notice (W1).
pub fn notice(wallet: &WalletView) -> Option<Report<'_>> {
    let notice = wallet.notice.as_ref()?;
    let paused: Vec<(&AccountRow, DivergenceKind)> = wallet
        .accounts
        .iter()
        .filter_map(|a| match a.state {
            AccountState::Diverged { kind, .. } => Some((a, kind)),
            _ => None,
        })
        .collect();
    let count = wallet.accounts.len();
    let report = match notice.kind {
        NoticeKind::NotWhole | NoticeKind::WillNotStart => {
            let n = paused.len();
            let all_unfound = paused.iter().all(|(_, k)| *k == DivergenceKind::NotFound);
            let title = if all_unfound {
                if n == 1 {
                    "The node does not find 1 account".to_owned()
                } else {
                    format!("The node does not find {n} accounts")
                }
            } else if notice.kind == NoticeKind::WillNotStart {
                "The wallet did not open: its accounts need a look".to_owned()
            } else if n == 1 {
                "1 account paused: look at it before spending from it".to_owned()
            } else {
                format!("{n} accounts paused: look at them before spending from them")
            };
            let mut r = Report::new(
                if all_unfound {
                    Tone::Note
                } else {
                    Tone::Warning
                },
                title,
            );
            r = if all_unfound {
                r.says(
                    "The node answered \"account not found\". That is what a new account looks \
                     like until it is first paid, and what an emptied one looks like; nothing \
                     has been lost.",
                )
            } else {
                r.says(format!(
                    "Tawara could not match {} of this store's {count} accounts against the \
                     chain, so nothing can be sent from {} until {} looked at. Pausing protects \
                     the funds: no one-time key is ever used twice. {}",
                    if n == 1 {
                        "1".to_owned()
                    } else {
                        n.to_string()
                    },
                    if n == 1 { "it" } else { "them" },
                    if n == 1 { "it is" } else { "they are" },
                    if n == count {
                        ""
                    } else {
                        "The other accounts are unaffected."
                    }
                ))
            };
            for (account, kind) in &paused {
                r = r.says(format!("{}: {}.", name(account.id), in_brief(*kind)));
                r = r.causes(causes_of(*kind));
            }
            r.says("Each account's own page, from the list below, has its summary and report.")
        }
        NoticeKind::NoNode => {
            Report::new(Tone::Warning, "No node chosen").says(notice.text.clone())
        }
        NoticeKind::NodeChanged => {
            Report::new(Tone::Note, "The node was changed").says(notice.text.clone())
        }
        NoticeKind::Cancelled => {
            Report::new(Tone::Note, "Reconciling was cancelled").says(notice.text.clone())
        }
        NoticeKind::NodeSilent => Report::new(Tone::Warning, "The node did not answer")
            .says(first_paragraph(&notice.text))
            .causes(&[NODE_DOWN, OFFLINE, NODE_ADDRESS]),
        NoticeKind::NodeRefused => Report::new(Tone::Warning, "The node could not be used")
            .says(first_paragraph(&notice.text))
            .causes(&[NODE_ADDRESS, NODE_DOWN]),
    };
    // The worker's own texts are the summary itself; the library's pages
    // are offered whole.
    Some(match notice.kind {
        NoticeKind::NoNode | NoticeKind::NodeChanged | NoticeKind::Cancelled => report,
        _ => report.full(&notice.text),
    })
}

/// A text's first paragraph, its lines joined.
fn first_paragraph(text: &str) -> String {
    ui::reflow(text.split("\n\n").next().unwrap_or(text))
}

/// The spend laid out and not signed (W5).
pub fn plan(p: &PlanView, index: Option<u32>) -> Report<'_> {
    let mut r = Report::new(
        if p.empties_account {
            Tone::Warning
        } else {
            Tone::Note
        },
        if p.empties_account {
            "Not signed yet, and this empties the account"
        } else {
            "Not signed yet: check every destination against its payee"
        },
    )
    .says(
        "Nothing has been signed and no key has been reserved. Compare each destination, \
         character for character, with what the payee's wallet shows; they are listed in the \
         order that goes on the wire.",
    )
    .says(format!(
        "Signing first reserves {}. From then, these exact figures (each destination, amount \
         and reference, the fee and the block-to-live) are the only spend that key will sign: \
         re-signing later needs them exactly.",
        index.map_or_else(|| "the account's key".to_owned(), |i| format!("key #{i}"))
    ));
    if p.empties_account {
        r = r.says(
            "The change is zero, so this empties the account. Once it lands the node answers \
             \"account not found\" for it, and nothing can be done with it until it is paid \
             again.",
        );
    }
    r.full(&p.text)
}

/// The spend signed (W6): the three facts, never softened (docs/PLAN.md
/// section 4.9).
pub fn sent(s: &SentView) -> Report<'_> {
    let r = if s.submitted {
        Report::new(
            Tone::Warning,
            "Written to the node's socket: not yet accepted by the network",
        )
        .says(
            "Tawara wrote the signed spend to the node's socket. That is not the network \
             accepting it: the node checks it after Tawara has gone, and if it refuses, nothing \
             says so here.",
        )
    } else {
        Report::new(
            Tone::Warning,
            "Signed, and not written to the node's socket",
        )
        .says(
            "The spend is signed, and writing it to the node's socket failed; the full report \
             says why. The bytes on this page are the spend: Submit saved artifact sends them \
             again.",
        )
    };
    r.says(
        "Every check that can run without the node passed. The ledger, the balance and the \
         block-to-live are the node's to check.",
    )
    .says(
        "The signed bytes exist only on this page; the store does not keep them. Save them: \
         until the spend settles they are the only bytes that can move these funds, and \
         re-signing needs exactly the same destinations, amounts, references, fee and \
         block-to-live.",
    )
    .says("Settle from the account's page once the chain has moved, to learn what happened.")
    .full(&s.text)
}

/// An account's destination (W2).
pub fn receive(text: &str) -> Report<'_> {
    Report::new(Tone::Note, "About this destination")
        .says(
            "The destination is what a payer needs: Base58 with a checksum, so a mistyped one \
             is refused rather than paid. It comes from this store alone; no node was asked, \
             and it is right whether or not the account has been paid yet.",
        )
        .says(
            "The ledger address in the full report is not a destination: no wallet takes it, \
             this one included.",
        )
        .full(text)
}

/// A discovery sweep (W3).
pub fn found<'a>(text: &'a str, rows: &[Discovered]) -> Report<'a> {
    let held = rows.iter().filter(|r| r.ledger_balance.is_some()).count();
    let to = rows.iter().map(|r| r.account_index).max().unwrap_or(0);
    let mut r = Report::new(
        Tone::Note,
        format!(
            "Searched accounts 0 to {to}: the node holds {held} of {}",
            rows.len()
        ),
    )
    .says(
        "Nothing was written. An account the chain holds and this store does not can be added \
         from the list.",
    );
    if held < rows.len() {
        r = r.says(
            "\"Not found\" is what the node says for an account it holds no entry for. It does \
             not mean the account does not exist.",
        );
        r.causes_title = "Why the node may not find one";
        r = r.causes(&[NOT_PAID, EMPTIED, LOOKUP, NOT_THIS_SEED]);
    }
    r.full(text)
}

/// An account added, or not (W3).
pub fn added(ok: bool, text: &str) -> Report<'_> {
    if ok {
        Report::new(Tone::Note, "Account added")
            .says("The store now holds it at the key the chain shows; it is in the wallet's list.")
            .full(text)
    } else {
        Report::new(Tone::Warning, "The account was not added")
            .says("Nothing was written. The full report says why.")
            .full(text)
    }
}

/// Saved bytes submitted (W9).
pub fn submitted(accepted: bool, text: &str) -> Report<'_> {
    if accepted {
        Report::new(
            Tone::Warning,
            "Written to the node's socket: not yet accepted by the network",
        )
        .says(
            "Only the bytes' layout was checked here. The node checks the rest after Tawara has \
             gone, and if it refuses, nothing says so here.",
        )
        .says("Settle from the account's page once the chain has moved, to learn what happened.")
        .full(text)
    } else {
        Report::new(Tone::Warning, "Not submitted")
            .says("Nothing reached the node's socket. The full report says why.")
            .full(text)
    }
}

/// What a check or a settlement found (W7), read from the account's state
/// afterwards.
pub fn done<'a>(done: Done, account: &AccountRow, text: &'a str) -> Report<'a> {
    let r = match (done, &account.state) {
        (Done::Settled, AccountState::InSync { .. }) => {
            Report::new(Tone::Note, "Settled: the account can spend again")
                .says("The chain shows the change key: the spend landed, and the store records it.")
        }
        (Done::Settled, AccountState::SpendOutstanding { .. }) => {
            Report::new(Tone::Warning, "Not settled yet: the spend has not landed").says(
                "The chain still holds the key that signed. Settle again once it has moved; \
                 until then the account does not spend again.",
            )
        }
        (Done::Settled, _) => Report::new(Tone::Note, "Settlement")
            .says("The account's state above is what the chain showed."),
        (Done::Checked, _) => Report::new(Tone::Note, "Checked against the chain just now")
            .says("The account's state above is what the chain showed."),
    };
    r.full(text)
}

/// What the node's index holds for one account (W10): what its rows are,
/// and the library's page for them.
pub fn history(h: &AccountHistory) -> Report<'_> {
    let mut r = Report::new(
        Tone::Note,
        format!("The node's index for {}", name(h.account)),
    )
    .says(
        "These are the transactions the node's index holds for this account, newest first. It \
         answers at most 100, and no request reaches older ones.",
    )
    .says(
        "The index counts a spend's source at its gross amount and lists the change as a \
         destination of its own. The amounts shown are what each transaction did to the \
         account, net of its change.",
    );
    if h.more() {
        r = r.says(format!(
            "It holds {} for this account in all; the oldest {} are not shown.",
            h.total,
            h.total - h.transactions.len() as u64
        ));
    }
    r.full(&h.text)
}

const NO_INDEX: &str = "Transactions are read from the node's transaction index, which a node \
                        runs only when its operator sets one up. Nothing is wrong with the store \
                        or its accounts.";
const OTHER_NODE: &str = "Another node may run one: Change node, in the sidebar or in Settings.";
const INDEX_DOWN: &str = "The node answered, and runs a transaction index, but the index did not: \
                          its database is not connected yet, or not at all, or the search failed. \
                          Nothing is wrong with the store or its accounts.";
const READ_AGAIN: &str = "Read again in a while: an index that is starting answers once its \
                          database is connected.";
const OTHER_INDEX: &str = "Another node's index may answer: Change node, in the sidebar or in \
                           Settings.";

/// An explorer read the node did not serve: `what` it was for (W1, W10).
pub fn explorer<'a>(refusal: &'a ExplorerRefusal, what: &str) -> Report<'a> {
    let r = match refusal.index {
        Some(IndexState::Absent) => {
            let mut r = Report::new(Tone::Note, "This node keeps no transaction index")
                .says(NO_INDEX)
                .causes(&[OTHER_NODE]);
            r.causes_title = "What helps";
            r
        }
        Some(IndexState::Unavailable) => {
            let mut r = Report::new(
                Tone::Warning,
                "This node's transaction index did not answer",
            )
            .says(INDEX_DOWN)
            .causes(&[READ_AGAIN, OTHER_INDEX]);
            r.causes_title = "What helps";
            r
        }
        None => Report::new(Tone::Warning, format!("The node did not serve {what}"))
            .says("Nothing was read, and nothing was written.")
            .causes(&[NODE_DOWN, OFFLINE, NODE_ADDRESS]),
    };
    r.full(&refusal.text)
}

/// What an explorer page read (E1 to E4), said in a line or two, and the
/// library's page for it.
pub fn read<'a>(title: impl Into<String>, says: &[&str], text: &'a str) -> Report<'a> {
    let mut r = Report::new(Tone::Note, title);
    for line in says {
        r = r.says(*line);
    }
    r.full(text)
}

const ABOVE_TIP: &str = "The block is above the node's tip: the chain has not reached it yet.";
const ARCHIVE: &str = "A node serves a block by its hash only from its archive of blocks, which \
                       may not hold it. By its number, it serves any block up to its tip.";

/// A block the node did not serve (E2), asked for by its hash or by its
/// number.
pub fn block_refused(refusal: &ExplorerRefusal, by_hash: bool) -> Report<'_> {
    Report::new(Tone::Warning, "The node did not serve this block")
        .says("Nothing was read, and nothing was written.")
        .causes(&[if by_hash { ARCHIVE } else { ABOVE_TIP }])
        .causes(&[NODE_DOWN, OFFLINE, NODE_ADDRESS])
        .full(&refusal.text)
}

/// What the node's index said of a hash that named nothing (E1).
pub fn missed_transaction(m: &Missed) -> Report<'_> {
    if !m.searched {
        return explorer(&m.transaction, "its transaction index");
    }
    let mut r = Report::new(
        Tone::Note,
        "The node's index holds no transaction with this id",
    )
    .says(
        "The index answered, and holds no transaction with this id. That is not the same as \
         there being none: an index holds what its node has read.",
    )
    .causes(&[
        "The id is mistyped. An id has 64 hex digits.",
        "The transaction is still waiting in a node's queue, or was never accepted.",
        OTHER_INDEX,
    ]);
    r.causes_title = "What it can mean";
    r.full(&m.transaction.text)
}

/// What the node said of a block with a hash that named nothing (E1).
pub fn missed_block(m: &Missed) -> Report<'_> {
    let mut r = Report::new(Tone::Note, "The node serves no block with this hash")
        .says(ARCHIVE)
        .causes(&[NODE_DOWN, OFFLINE]);
    r.causes_title = "Or";
    r.full(&m.block.text)
}

/// One account's report on the recovery page (W12): what the chain shows
/// for it now.
pub fn review(a: &AccountReport) -> Report<'_> {
    let r = match &a.state {
        AccountState::Diverged { kind, .. } => return diverged(*kind, &a.text),
        AccountState::InSync { .. } if a.spendable => Report::new(
            Tone::Note,
            "Reconciled: the chain holds it at the key this store expects",
        )
        .says("Nothing needs doing for it."),
        AccountState::InSync { .. } => Report::new(
            Tone::Note,
            "Reconciled: the chain holds it at the key this store expects",
        )
        .says(
            "It was set aside when the wallet opened, so it spends again once Refresh opens \
             the wallet with it.",
        ),
        AccountState::SpendOutstanding { .. } => {
            Report::new(Tone::Warning, "A spend is reserved and not settled").says(
                "Settle it from the account's page once the chain shows it landed, or re-sign \
                 it there if its signed bytes were lost.",
            )
        }
        AccountState::SpendLanded { .. } => Report::new(Tone::Warning, "The spend landed")
            .says("Settling it, from the account's page, records it and frees the account."),
        AccountState::NotReconciled => {
            Report::new(Tone::Note, "Not reconciled").says("No node answered for it.")
        }
    };
    r.full(&a.text)
}

/// What an acknowledged advance did (W12).
pub fn advanced(ok: bool, text: &str) -> Report<'_> {
    let r = if ok {
        Report::new(Tone::Note, "The advance is done, as the library reports it")
            .says("The reports above are read again from the chain.")
    } else {
        Report::new(Tone::Warning, "Not advanced").says(
            "The library did not move the account: an advance goes only to the index its live \
             report names. Its page says what it found.",
        )
    };
    r.full(text)
}

/// A refusal too long to read as a line: its first paragraph, and the rest
/// on request. `None` for one that reads as a line.
pub fn refused(text: &str) -> Option<Report<'_>> {
    let long = text.trim().contains('\n') || text.len() > 240;
    long.then(|| {
        Report::new(Tone::Warning, "Refused")
            .says(first_paragraph(text))
            .full(text)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tawara_wallet_core::sample;
    use tawara_wallet_core::view::Notice;

    #[test]
    fn a_store_not_whole_is_summarized_with_the_causes_that_fit() {
        let wallet = sample::wallet_view();
        let r = notice(&wallet).expect("the sample store is not whole");
        assert!(r.tone == Tone::Warning);
        assert_eq!(
            r.title,
            "1 account paused: look at it before spending from it"
        );
        assert!(
            r.summary
                .iter()
                .any(|l| l.contains("the chain is 2 keys ahead")),
            "{:?}",
            r.summary
        );
        assert!(r.causes.contains(&OLDER_COPY) && r.causes.contains(&SECOND_WALLET));
        assert_eq!(r.full, wallet.notice.as_deref());
    }

    #[test]
    fn accounts_the_node_does_not_find_are_told_calmly() {
        let mut wallet = sample::wallet_view();
        for a in &mut wallet.accounts {
            a.state = AccountState::Diverged {
                kind: DivergenceKind::NotFound,
                report: "account not found".to_owned(),
                advance_to: None,
            };
        }
        let r = notice(&wallet).unwrap();
        assert!(r.tone == Tone::Note);
        assert_eq!(r.title, "The node does not find 3 accounts");
        assert_eq!(r.causes.first(), Some(&NOT_PAID));
    }

    #[test]
    fn the_workers_own_words_are_the_summary_and_the_librarys_are_offered() {
        let silent = sample::unreconciled_view();
        let r = notice(&silent).unwrap();
        assert_eq!(r.title, "The node did not answer");
        assert!(r.causes.contains(&NODE_DOWN));
        assert!(r.full.is_some());

        let mut no_node = sample::unreconciled_view();
        no_node.notice = Some(Notice {
            kind: NoticeKind::NoNode,
            text: "No node is chosen.".to_owned(),
        });
        let r = notice(&no_node).unwrap();
        assert_eq!(r.summary, vec!["No node is chosen.".to_owned()]);
        assert_eq!(r.full, None, "nothing more to show");
    }

    #[test]
    fn each_kind_of_divergence_names_its_own_causes() {
        for kind in [
            DivergenceKind::Ahead { gap: 1 },
            DivergenceKind::Behind { gap: 2 },
            DivergenceKind::Unlocated,
            DivergenceKind::ReservationUnexplained,
            DivergenceKind::NotFound,
            DivergenceKind::Unreachable,
            DivergenceKind::NoMaster,
            DivergenceKind::Failed,
        ] {
            let r = diverged(kind, "the report");
            assert!(!r.title.is_empty() && !r.summary.is_empty());
            assert_eq!(r.causes, causes_of(kind).to_vec());
            assert_eq!(r.full, Some("the report"));
        }
        assert!(
            diverged(DivergenceKind::Ahead { gap: 1 }, "")
                .title
                .contains("1 key ahead")
        );
    }

    #[test]
    fn the_sent_summary_states_the_three_facts() {
        let s = sample::sent_view();
        let r = sent(&s);
        assert!(r.title.contains("not yet accepted"), "{}", r.title);
        let all = r.summary.join(" ");
        assert!(all.contains("not the network accepting it"), "{all}");
        assert!(all.contains("Every check that can run without the node passed"));
        assert!(all.contains("Save them"));
        assert_eq!(r.full, Some(s.text.as_str()));
    }

    #[test]
    fn a_long_refusal_folds_and_a_short_one_reads_as_a_line() {
        assert!(refused("wrong password").is_none());
        let long = "first paragraph\nstill the first\n\nthe second";
        let r = refused(long).unwrap();
        assert_eq!(
            r.summary,
            vec!["first paragraph still the first".to_owned()]
        );
        assert_eq!(r.full, Some(long));
    }
}
