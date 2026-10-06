//! What the node's explorer endpoints say, as the interface shows it:
//! a tag's transactions from the node's index (Activity, docs/SCREENS.md
//! W10), and the newest blocks and the size of the node's queue (the
//! wallet's network card, W1).
//!
//! The rows are the library's (`mesh::codec`), read with the command
//! line's own calls (`cli::cmd_recent_transactions_from`, `cli::cmd_blocks`,
//! `cli::cmd_mempool`), and every read carries the library's page for it,
//! word for word. What
//! the interface works out from a row (which way it moved, by how much, for
//! one account) follows the command line's page for the same rows
//! (`cli::render`, `recent_transactions`), which is private there and so
//! repeated here, as the worker repeats the command line's other private
//! decisions (docs/DECISIONS.md D19).

use mochimo_crypto::Error;
use mochimo_crypto::mesh::codec::{
    self, MeshBlock, MeshTransaction, OP_DESTINATION, OP_FEE, OP_REWARD, OP_SOURCE, SearchPage,
};

use crate::view::AccountId;

/// The most rows the node's index answers for one tag in one request: one
/// page of an account's history. Older rows are read a page at a time, from
/// the offset the last page ended at (docs/DECISIONS.md D30).
pub const HISTORY_ROWS: u64 = 100;

/// How many of the newest blocks the wallet's network card shows.
pub const CARD_BLOCKS: u64 = 6;

/// What an operation in a transaction did, in the node's terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationKind {
    /// A mining reward: newly minted, not moved.
    Reward,
    /// Value leaving its source. The index debits the source its gross
    /// amount and lists the change as a destination of its own.
    Source,
    /// Value arriving.
    Destination,
    /// The fee paid to the miner.
    Fee,
    /// A kind this version does not know, as the node spelled it.
    Other(String),
}

/// Who an operation's address names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Party {
    /// A twenty-byte tag: an account, which has a destination.
    Account(AccountId),
    /// A forty-byte ledger address (the tag and its key's hash), in hex. Not
    /// a destination: it must never be offered as somewhere to send funds.
    Ledger(String),
    /// Anything else, as the node sent it, made safe to show.
    Other(String),
}

/// One operation of a transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationView {
    pub kind: OperationKind,
    pub party: Party,
    /// In nanoMCM; negative for a source debited.
    pub amount: i128,
    /// The reference the node carries for it, made safe to show; empty for
    /// none. The node sends a destination's reference as its whole
    /// sixteen-byte field, a short one padded with NUL bytes, on `/block` and
    /// on the index's rows alike; the padding is not part of it and is left
    /// off.
    pub memo: String,
}

/// One transaction as the node's index holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionView {
    /// The transaction's id, in hex.
    pub id: String,
    /// The block it landed in, when the index says.
    pub block: Option<u64>,
    /// When that block was made, in milliseconds since the epoch, when the
    /// index says.
    pub time_ms: Option<i64>,
    pub operations: Vec<OperationView>,
}

/// Which way a transaction moved value for one account, as the command
/// line's page says it (`in`, `out`, `both`, `--`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// The account received.
    In,
    /// The account was a source, and nothing came back to it.
    Out,
    /// The account was a source and a destination: a spend whose change came
    /// back to it, as the index lists every spend that leaves change.
    Both,
    /// Nothing in it names the account.
    Neither,
}

impl TransactionView {
    fn touches(op: &OperationView, account: AccountId) -> bool {
        op.party == Party::Account(account)
    }

    /// What the transaction did to `account`'s balance, in nanoMCM: the sum
    /// of the operations that name it, as the command line's page sums
    /// them. For a spend that is what left net of the change that came back.
    #[must_use]
    pub fn net(&self, account: AccountId) -> i128 {
        self.operations
            .iter()
            .filter(|o| Self::touches(o, account))
            .map(|o| o.amount)
            .sum()
    }

    /// Which way it moved value for `account`, as the command line's page
    /// says it.
    #[must_use]
    pub fn direction(&self, account: AccountId) -> Direction {
        let out = self
            .operations
            .iter()
            .any(|o| o.kind == OperationKind::Source && Self::touches(o, account));
        let back = self
            .operations
            .iter()
            .any(|o| o.kind == OperationKind::Destination && Self::touches(o, account));
        match (out, back) {
            (true, true) => Direction::Both,
            (true, false) => Direction::Out,
            (false, true) => Direction::In,
            (false, false) => Direction::Neither,
        }
    }

    /// The destinations other than `account` itself: where a spend from it
    /// went, its change left out.
    pub fn paid_out(&self, account: AccountId) -> impl Iterator<Item = &OperationView> {
        self.operations
            .iter()
            .filter(move |o| o.kind == OperationKind::Destination && !Self::touches(o, account))
    }

    /// The sources other than `account`: who paid it.
    pub fn paid_by(&self, account: AccountId) -> impl Iterator<Item = &OperationView> {
        self.operations
            .iter()
            .filter(move |o| o.kind == OperationKind::Source && !Self::touches(o, account))
    }

    /// The fee, in nanoMCM, as the node lists it.
    #[must_use]
    pub fn fee(&self) -> u128 {
        self.operations
            .iter()
            .filter(|o| o.kind == OperationKind::Fee)
            .map(|o| o.amount.unsigned_abs())
            .sum()
    }

    /// Whether it is a mining reward paid to `account`.
    #[must_use]
    pub fn rewards(&self, account: AccountId) -> bool {
        self.operations
            .iter()
            .any(|o| o.kind == OperationKind::Reward && Self::touches(o, account))
    }

    pub(crate) fn of(tx: &MeshTransaction) -> TransactionView {
        TransactionView {
            id: hex(&tx.hash),
            block: tx.block.map(|b| b.index),
            time_ms: tx.timestamp_ms,
            operations: tx
                .operations
                .iter()
                .map(|o| OperationView {
                    kind: match o.kind.as_str() {
                        OP_REWARD => OperationKind::Reward,
                        OP_SOURCE => OperationKind::Source,
                        OP_DESTINATION => OperationKind::Destination,
                        OP_FEE => OperationKind::Fee,
                        other => OperationKind::Other(shown(other)),
                    },
                    party: party(&o.address),
                    amount: o.amount,
                    memo: shown(o.memo.trim_end_matches('\0')),
                })
                .collect(),
        }
    }
}

/// One account's transactions, newest first: the pages read so far, from
/// the newest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountHistory {
    pub account: AccountId,
    /// [`HISTORY_ROWS`] a page, each transaction once.
    pub transactions: Vec<TransactionView>,
    /// How many the index holds for it in all, as the last page read said.
    pub total: u64,
    /// The offset the next older page starts at: how many rows, newest
    /// first, the pages read so far cover. A row the index gained at the top
    /// since the first page pushes the rest down, so a later page can repeat
    /// rows already read; it is counted here and listed once.
    pub next: u64,
    /// The library's page for each page read, word for word, one after
    /// another.
    pub text: String,
}

impl AccountHistory {
    /// Whether the index holds rows older than the pages read so far.
    #[must_use]
    pub fn more(&self) -> bool {
        self.total > self.next
    }

    /// How many rows the index holds that the pages read so far do not.
    #[must_use]
    pub fn unread(&self) -> u64 {
        self.total.saturating_sub(self.next)
    }

    /// Add `older`, the page read from [`AccountHistory::next`], below the
    /// rows already read: each transaction once, the total and the offset as
    /// that page says them, and its page after the others.
    pub fn extend(&mut self, older: AccountHistory) {
        for tx in older.transactions {
            if !self.transactions.iter().any(|t| t.id == tx.id) {
                self.transactions.push(tx);
            }
        }
        self.total = older.total;
        self.next = older.next;
        if !older.text.is_empty() {
            if !self.text.is_empty() {
                self.text.push_str("\n\n");
            }
            self.text.push_str(&older.text);
        }
    }

    /// The page read at offset `from`.
    pub(crate) fn of(
        account: AccountId,
        page: &SearchPage,
        from: u64,
        text: String,
    ) -> AccountHistory {
        AccountHistory {
            account,
            transactions: page.transactions.iter().map(TransactionView::of).collect(),
            total: page.total_count,
            next: from.saturating_add(page.transactions.len() as u64),
            text,
        }
    }
}

/// What kind of block a block is, by the C reference's own test as the
/// library applies it (`MeshBlock::kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    /// Transactions, solved by proof of work.
    Normal,
    /// No transactions: made when no block was solved in time.
    Pseudo,
    /// The ledger, at a block number whose low byte is zero.
    Neogenesis,
}

/// One block, as the newest blocks list it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockSummary {
    pub index: u64,
    /// In hex.
    pub hash: String,
    /// When it was made, in milliseconds since the epoch.
    pub time_ms: i64,
    /// How many transactions it carries, its reward among them.
    pub transactions: usize,
    /// Its kind; `None` when the node sent none of its own figures and its
    /// number does not make it a neogenesis block.
    pub kind: Option<BlockKind>,
    /// The difficulty it was solved at, when the node sent its figures.
    pub difficulty: Option<u32>,
}

impl BlockSummary {
    pub(crate) fn of(block: &MeshBlock) -> BlockSummary {
        BlockSummary {
            index: block.block.index,
            hash: hex(&block.block.hash),
            time_ms: block.timestamp_ms,
            transactions: block.transactions.len(),
            kind: block.kind().map(|k| match k {
                codec::BlockKind::Normal => BlockKind::Normal,
                codec::BlockKind::Pseudo => BlockKind::Pseudo,
                codec::BlockKind::Neogenesis => BlockKind::Neogenesis,
            }),
            difficulty: block.metadata.as_ref().map(|m| m.difficulty),
        }
    }
}

/// The chain's tip and the newest blocks below it, newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlocksView {
    pub tip: u64,
    pub blocks: Vec<BlockSummary>,
    /// The library's page for them, word for word.
    pub text: String,
}

/// The node's queue of transactions waiting to be mined: how many wait,
/// with none of them read whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MempoolView {
    pub waiting: usize,
    /// The library's page, word for word.
    pub text: String,
}

/// Why an explorer read answered nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerRefusal {
    /// What a refused search of the node's index says about that index;
    /// `None` for any other read, and for a refusal that says nothing about
    /// it.
    pub index: Option<IndexState>,
    /// The library's page, word for word.
    pub text: String,
}

/// What a refused search of the node's transaction index says about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexState {
    /// The node runs none: a deployment indexes only when configured to,
    /// and another node may.
    Absent,
    /// The node runs one, and it did not answer: its database is not
    /// connected, or the search failed. Asked again it may answer.
    Unavailable,
}

/// What `cause`, a refused search of the node's index, says about that
/// index, by the Mesh middleware's own answers (`mochimo-mesh` at
/// `ddc1ee5`). It registers `/search/transactions` only when its indexer is
/// enabled as it starts (`main.go`), so a node that runs none answers that
/// route with a 404; one that runs an indexer answers its internal error,
/// code 2, while the indexer's database is not connected and when a search
/// fails (`search_handler.go`). Code 1 is a request it could not decode.
///
/// The library reads a refused search the same way, for the page it
/// writes (`Outcome::SearchFailed`, `cli::search_refusal`), which says
/// which of the two it was in its own words; the interface's reading is
/// repeated here, as D19 has the worker repeat the command line's other
/// private decisions (docs/DECISIONS.md D30, item 4).
pub(crate) fn index_state(cause: &Error) -> Option<IndexState> {
    match cause {
        Error::HttpStatus { status: 404 } => Some(IndexState::Absent),
        Error::Mesh { code: 2, .. } => Some(IndexState::Unavailable),
        _ => None,
    }
}

/// An operation's address, as the command line's page reads one
/// (`cli::explorer_address`): a twenty-byte tag is an account, a forty-byte
/// one a ledger address, anything else is shown as it came.
fn party(text: &str) -> Party {
    let body = text.strip_prefix("0x").unwrap_or(text);
    let bytes = || -> Option<Vec<u8>> {
        (0..body.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(body.get(i..i + 2)?, 16).ok())
            .collect()
    };
    match (body.len(), bytes()) {
        (40, Some(b)) => {
            let mut tag = [0u8; 20];
            tag.copy_from_slice(&b);
            Party::Account(AccountId::from_tag(tag))
        }
        (80, Some(_)) => Party::Ledger(format!("0x{}", body.to_ascii_lowercase())),
        _ => Party::Other(shown(text)),
    }
}

/// Text from the node, made safe to show: the command line's rule for
/// external text (`cli::terminal_text`). Controls, the bidirectional
/// formatting characters that reorder what follows them, the characters
/// that draw with no width, and the backslash are written as escapes, so a
/// node cannot make one reference look like another.
pub(crate) fn shown(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        let bidi = matches!(
            ch,
            '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        );
        let zero_width = matches!(
            ch,
            '\u{00ad}' | '\u{180e}' | '\u{200b}'..='\u{200d}' | '\u{2060}'..='\u{2064}' | '\u{feff}'
        );
        if ch.is_control() || bidi || zero_width || ch == '\\' {
            out.extend(ch.escape_default());
        } else {
            out.push(ch);
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use mochimo_crypto::mesh::codec;

    use super::*;

    fn op(kind: &str, address: &str, amount: i128, memo: &str) -> codec::Operation {
        codec::Operation {
            index: 0,
            kind: kind.to_owned(),
            address: address.to_owned(),
            amount,
            memo: memo.to_owned(),
        }
    }

    fn tag_hex(n: u8) -> String {
        format!("0x{}", hex(&[n; 20]))
    }

    fn tx(ops: Vec<codec::Operation>) -> TransactionView {
        TransactionView::of(&MeshTransaction {
            hash: [7; 32],
            block: None,
            timestamp_ms: None,
            operations: ops,
            metadata: Vec::new(),
        })
    }

    #[test]
    fn a_spend_with_change_is_both_ways_and_nets_what_left() {
        let me = AccountId::from_tag([1; 20]);
        let t = tx(vec![
            op(OP_SOURCE, &tag_hex(1), -1_000_500, ""),
            op(OP_DESTINATION, &tag_hex(2), 400_000, "INV-1"),
            op(OP_DESTINATION, &tag_hex(1), 600_000, ""),
            op(OP_FEE, "", 500, ""),
        ]);
        assert_eq!(t.direction(me), Direction::Both);
        assert_eq!(t.net(me), -400_500);
        assert_eq!(t.fee(), 500);
        let out: Vec<_> = t.paid_out(me).collect();
        assert_eq!(out.len(), 1, "the change is not a payee");
        assert_eq!(out[0].memo, "INV-1");
        let them = AccountId::from_tag([2; 20]);
        assert_eq!(t.direction(them), Direction::In);
        assert_eq!(t.net(them), 400_000);
        assert_eq!(t.paid_by(them).count(), 1);
    }

    #[test]
    fn addresses_are_read_as_the_command_line_reads_them() {
        assert_eq!(
            party(&tag_hex(9)),
            Party::Account(AccountId::from_tag([9; 20]))
        );
        let ledger = format!("0x{}", "AB".repeat(40));
        assert_eq!(
            party(&ledger),
            Party::Ledger(format!("0x{}", "ab".repeat(40)))
        );
        assert_eq!(party("pool"), Party::Other("pool".to_owned()));
        assert_eq!(party(&"zz".repeat(20)), Party::Other("zz".repeat(20)));
    }

    #[test]
    fn a_reference_is_shown_without_its_field_padding() {
        let t = tx(vec![op(OP_DESTINATION, &tag_hex(2), 1, "INV-1\0\0\0\0\0")]);
        assert_eq!(t.operations[0].memo, "INV-1");
        // Only the padding: a NUL inside is the node's text, shown escaped.
        let t = tx(vec![op(OP_DESTINATION, &tag_hex(2), 1, "A\0B\0")]);
        assert_eq!(t.operations[0].memo, "A\\u{0}B");
    }

    #[test]
    fn text_from_the_node_cannot_disguise_itself() {
        assert_eq!(shown("INV-1"), "INV-1");
        assert_eq!(shown("A\u{202e}B"), "A\\u{202e}B");
        assert_eq!(shown("A\u{200b}B"), "A\\u{200b}B");
        assert_eq!(shown("A\nB\\"), "A\\nB\\\\");
    }

    #[test]
    fn a_refused_search_says_whether_the_node_runs_an_index() {
        let not_served = Error::HttpStatus { status: 404 };
        let down = Error::Mesh {
            code: 2,
            retriable: true,
        };
        let invalid = Error::Mesh {
            code: 1,
            retriable: false,
        };
        assert_eq!(index_state(&not_served), Some(IndexState::Absent));
        assert_eq!(index_state(&down), Some(IndexState::Unavailable));
        assert_eq!(
            index_state(&invalid),
            None,
            "an invalid request is not about the index"
        );
        assert_eq!(index_state(&Error::HttpStatus { status: 502 }), None);
    }
}
