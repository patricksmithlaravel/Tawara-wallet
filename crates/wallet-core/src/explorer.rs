//! What the node's explorer endpoints say, as the interface shows it:
//! a tag's transactions from the node's index (Activity, docs/SCREENS.md
//! W10) and the newest blocks (the wallet's network card, W1).
//!
//! The rows are the library's (`mesh::codec`), read with the command
//! line's own calls (`cli::cmd_recent_transactions`, `cli::cmd_blocks`),
//! and every read carries the library's page for it, word for word. What
//! the interface works out from a row (which way it moved, by how much, for
//! one account) follows the command line's page for the same rows
//! (`cli::render`, `recent_transactions`), which is private there and so
//! repeated here, as the worker repeats the command line's other private
//! decisions (docs/DECISIONS.md D19).

use mochimo_crypto::Error;
use mochimo_crypto::mesh::codec::{
    MeshBlock, MeshTransaction, OP_DESTINATION, OP_FEE, OP_REWARD, OP_SOURCE, SearchPage,
};

use crate::view::AccountId;

/// The most rows the node's index answers for one tag in one request, and
/// so the most Activity shows for each account: the endpoint takes no
/// offset the library can send (docs/DECISIONS.md D27, item 11).
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

/// One account's transactions, newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountHistory {
    pub account: AccountId,
    /// At most [`HISTORY_ROWS`].
    pub transactions: Vec<TransactionView>,
    /// How many the index holds for it in all.
    pub total: u64,
    /// The library's page for these rows, word for word.
    pub text: String,
}

impl AccountHistory {
    /// Whether the index holds more than it sent: older rows exist that no
    /// request can reach.
    #[must_use]
    pub fn more(&self) -> bool {
        self.total > self.transactions.len() as u64
    }

    pub(crate) fn of(account: AccountId, page: &SearchPage, text: String) -> AccountHistory {
        AccountHistory {
            account,
            transactions: page.transactions.iter().map(TransactionView::of).collect(),
            total: page.total_count,
            text,
        }
    }
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
}

impl BlockSummary {
    pub(crate) fn of(block: &MeshBlock) -> BlockSummary {
        BlockSummary {
            index: block.block.index,
            hash: hex(&block.block.hash),
            time_ms: block.timestamp_ms,
            transactions: block.transactions.len(),
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

/// Why an explorer read answered nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerRefusal {
    /// The node runs no transaction index, by the library's reading of its
    /// answer: a deployment indexes only when configured to, and another
    /// node may.
    pub no_index: bool,
    /// The library's page, word for word.
    pub text: String,
}

/// Whether `cause` says the node runs no transaction index: the command
/// line's reading of it (`cli::explorer_refusal`).
pub(crate) fn no_index(cause: &Error) -> bool {
    matches!(cause, Error::Mesh { code: 1, .. })
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
}
