//! Sample view models, for drawing the screens without a store or a node:
//! the screenshots (docs/DECISIONS.md D27, item 1).
//!
//! Every text in them is the library's own, rendered from sample values by
//! the same code that renders a real one, so a screenshot shows the words a
//! person will read. The tags are made-up bytes that belong to no one, and
//! the views are plain data: nothing here opens a store, asks a node or
//! holds a secret.

use std::path::PathBuf;

use mochimo_crypto::account::{AccountKind as LibraryKind, StreamId, WotsIndex};
use mochimo_crypto::cli::discover::{Sighting, Sweep};
use mochimo_crypto::cli::outcome::{Outcome, Shipped};
use mochimo_crypto::mesh::codec::{
    MeshBlock, MeshTransaction, OP_DESTINATION, OP_FEE, OP_REWARD, OP_SOURCE, Operation, SearchPage,
};
use mochimo_crypto::mesh::{ChainTip, LedgerEntry, TxId};
use mochimo_crypto::recon::{AccountStatus, ChainPosition, Divergence, Reservation};
use mochimo_crypto::tx::wire::Destination;

use crate::command::PlanId;
use crate::event::{
    AccountReport, Discovered, PlanView, PlannedDestination, ReceiveView, SentView,
};
use crate::explorer::{AccountHistory, BlockSummary, BlocksView};
use crate::text;
use crate::view::{
    AccountId, AccountKind, AccountRow, AccountState, Notice, NoticeKind, ReservationState,
    WalletView,
};
use crate::worker::hex;

/// A made-up tag: twenty bytes from `seed`.
fn tag(seed: u8) -> [u8; 20] {
    let mut t = [0u8; 20];
    for (i, b) in t.iter_mut().enumerate() {
        *b = seed
            .wrapping_mul(31)
            .wrapping_add(u8::try_from(i).unwrap_or(0).wrapping_mul(17))
            | 1;
    }
    t
}

fn index(n: u32) -> WotsIndex {
    let mut i = WotsIndex::ZERO;
    for _ in 0..n {
        i = i.advanced().unwrap_or(i);
    }
    i
}

/// An account two keys behind the chain: the library's report for it, and
/// the index its acknowledged advance would move it to.
fn behind(t: [u8; 20], local: u32, balance: u64) -> Divergence {
    let mut local_address = [0u8; 40];
    local_address[..20].copy_from_slice(&t);
    local_address[20..].fill(0x5a);
    let mut chain_address = local_address;
    chain_address[20..].fill(0xb7);
    Divergence::IndexMismatch {
        tag: t,
        local: index(local),
        local_address,
        chain_address,
        balance,
        stream: StreamId::from_bytes(t),
        found: ChainPosition::Ahead {
            index: index(local + 2),
            gap: 2,
        },
        reverted_settle: None,
    }
}

/// The sample store's diverged account: the chain shows it two keys ahead.
fn diverged() -> Divergence {
    behind(tag(3), 31, 85_135_096_906)
}

/// The ledger address a made-up tag holds at some key: the tag, then
/// twenty bytes of `fill` standing for the key's half.
fn address(t: [u8; 20], fill: u8) -> [u8; 40] {
    let mut a = [0u8; 40];
    a[..20].copy_from_slice(&t);
    a[20..].fill(fill);
    a
}

/// The first account's balance in the sample store.
const FIRST_BALANCE: u64 = 9_215_402_118_000;

/// An open store of three accounts: one reconciled, one with a spend
/// outstanding, and one the chain shows two keys ahead, set aside with the
/// library's report; and the library's notice that the store is not whole.
#[must_use]
pub fn wallet_view() -> WalletView {
    let diverged = diverged();
    let rows = vec![
        AccountRow {
            id: AccountId::from_tag(tag(1)),
            kind: AccountKind::Derived,
            index: 49,
            state: AccountState::InSync {
                balance: FIRST_BALANCE,
            },
            spendable: true,
            number: Some(0),
        },
        AccountRow {
            id: AccountId::from_tag(tag(2)),
            kind: AccountKind::Derived,
            index: 6,
            state: AccountState::SpendOutstanding {
                balance: 3_180_000_000_000,
                spent_index: 6,
                reservation: ReservationState::Live,
            },
            spendable: false,
            number: Some(1),
        },
        AccountRow {
            id: AccountId::from_tag(tag(3)),
            kind: AccountKind::Imported,
            index: 31,
            state: AccountState::from_divergence(&diverged),
            spendable: false,
            number: None,
        },
    ];
    WalletView {
        dir: PathBuf::from("/home/you/.local/share/tawara/keystore"),
        accounts: rows,
        opened: true,
        notice: crate::text::standing_notice(&[diverged]).map(|text| Notice {
            kind: NoticeKind::NotWhole,
            text,
        }),
    }
}

/// The same store opened while its node did not answer: no account was
/// reconciled, so no balance is known, and the worker's notice says why.
#[must_use]
pub fn unreconciled_view() -> WalletView {
    let mut view = wallet_view();
    for row in &mut view.accounts {
        row.state = AccountState::NotReconciled;
        row.spendable = false;
    }
    view.opened = false;
    view.notice = Some(Notice {
        kind: NoticeKind::NodeSilent,
        text: crate::worker::NODE_SILENT.to_owned(),
    });
    view
}

/// The first account's receive page: its destination, its key index, the
/// ledger address that index makes, and the library's words for the two.
#[must_use]
pub fn receive_view() -> ReceiveView {
    let t = tag(1);
    let held = address(t, 0x3c);
    ReceiveView {
        account: AccountId::from_tag(t),
        destination: AccountId::from_tag(t).destination(),
        index: 49,
        ledger_address: hex(&held),
        text: text::page(
            &[],
            Outcome::Address {
                tag: t,
                address: held,
                index: index(49),
            },
        ),
    }
}

/// A reference in its sixteen-byte field, zero-filled.
fn reference(text: &str) -> [u8; 16] {
    let mut field = [0u8; 16];
    field[..text.len()].copy_from_slice(text.as_bytes());
    field
}

/// What the sample spend sends: two payees, each with its reference.
fn payees() -> Vec<(Destination, &'static str)> {
    vec![
        (
            Destination {
                tag: tag(7),
                reference: reference("PAYROLL-11"),
                amount: 420_000_000_000,
            },
            "PAYROLL-11",
        ),
        (
            Destination {
                tag: tag(8),
                reference: reference("PAYROLL-12"),
                amount: 380_000_000_000,
            },
            "PAYROLL-12",
        ),
    ]
}

/// The sample spend's totals: sent, the fee at the node's floor, and the
/// change back to the first account's next key.
fn totals() -> (u64, u64, u64) {
    let send: u64 = payees().iter().map(|(d, _)| d.amount).sum();
    let fee = 2 * mochimo_crypto::consts::MFEE;
    (send, fee, FIRST_BALANCE - send - fee)
}

/// A spend from the first account to two payees, laid out and not signed:
/// the library's "not signed" page, with the store's standing divergence in
/// front of it as the worker writes it.
#[must_use]
pub fn plan_view() -> PlanView {
    let (send_total, fee_total, change_total) = totals();
    let destinations = payees();
    let text = text::page(
        &[diverged()],
        Outcome::Planned {
            source: tag(1),
            destinations: destinations.iter().map(|(d, _)| d.clone()).collect(),
            send_total,
            fee_total,
            change_total,
            blk_to_live: 0,
        },
    );
    PlanView {
        plan: PlanId::example(),
        from: AccountId::from_tag(tag(1)),
        destinations: destinations
            .iter()
            .map(|(d, r)| PlannedDestination {
                to: AccountId::from_tag(d.tag),
                destination: AccountId::from_tag(d.tag).destination().unwrap_or_default(),
                amount: d.amount,
                reference: (*r).to_owned(),
            })
            .collect(),
        send_total,
        fee_total,
        change_total,
        balance: FIRST_BALANCE,
        blk_to_live: 0,
        empties_account: false,
        text,
    }
}

/// Made-up bytes the size of a signed spend, standing for one: the sample
/// shows how much the retry artifact is, not one that could be sent.
fn signed_bytes() -> Vec<u8> {
    (0u32..2_412)
        .map(|i| u8::try_from((i.wrapping_mul(2_654_435_761) >> 13) & 0xff).unwrap_or(0))
        .collect()
}

/// The sample spend signed and written to the node's socket: the library's
/// page with its three facts, the retry artifact, and the store afterwards,
/// the first account now holding a reservation.
#[must_use]
pub fn sent_view() -> SentView {
    let (send_total, fee_total, change_total) = totals();
    let wire = signed_bytes();
    let id = TxId([0x5e; 32]);
    let text = text::page(
        &[diverged()],
        Outcome::Sent {
            shipped: Shipped {
                source: tag(1),
                destinations: payees().into_iter().map(|(d, _)| d).collect(),
                blk_to_live: 0,
                wire: wire.clone(),
                submitted: Ok(id),
            },
            send_total,
            fee_total,
            change_total,
            upgraded: None,
        },
    );
    let mut view = wallet_view();
    view.accounts[0].state = AccountState::SpendOutstanding {
        balance: FIRST_BALANCE,
        spent_index: 49,
        reservation: ReservationState::Live,
    };
    view.accounts[0].spendable = false;
    view.accounts[0].index = 50;
    SentView {
        from: AccountId::from_tag(tag(1)),
        submitted: true,
        tx_id: Some(hex(&id.0)),
        artifact_hex: hex(&wire),
        text,
        view,
    }
}

/// A sweep of derived accounts 0 to 4: the two the store holds, a third
/// the chain holds and the store does not, and two the node does not know.
/// The library's page, and the rows the worker makes of it.
#[must_use]
pub fn discovered() -> (String, Vec<Discovered>) {
    let seen = |account: u32, t: [u8; 20], balance: Option<u64>, held: Option<u32>| Sighting {
        account,
        tag: t,
        entry: balance.map(|balance| LedgerEntry {
            address: address(t, 0x2d),
            balance,
        }),
        held: held.map(|i| (LibraryKind::Derived, index(i))),
    };
    let sweep = Sweep {
        to: 4,
        sightings: vec![
            seen(0, tag(1), Some(FIRST_BALANCE), Some(49)),
            seen(1, tag(2), Some(3_180_000_000_000), Some(6)),
            seen(2, tag(4), Some(25_000_000_000), None),
            seen(3, tag(5), None, None),
            seen(4, tag(6), None, None),
        ],
    };
    let rows = sweep
        .sightings
        .iter()
        .map(|s| Discovered {
            account_index: s.account,
            id: AccountId::from_tag(s.tag),
            ledger_balance: s.entry.map(|e| e.balance),
            held: s.held.is_some(),
        })
        .collect();
    (text::page(&[], Outcome::Discovered { sweep }), rows)
}

/// The library's page for the sample spend's saved bytes, submitted again
/// with no store open.
#[must_use]
pub fn submitted_text() -> String {
    text::page(
        &[],
        Outcome::Submitted {
            source: tag(1),
            bytes: signed_bytes(),
            submitted: Ok(TxId([0x5e; 32])),
        },
    )
}

/// One transaction as the node's index spells it: `ops` are (kind, tag or
/// none, amount in nanoMCM).
fn indexed(
    id: u8,
    block: u64,
    at_ms: i64,
    ops: &[(&str, Option<[u8; 20]>, i128)],
) -> MeshTransaction {
    MeshTransaction {
        hash: [id; 32],
        block: Some(ChainTip {
            index: block,
            hash: [u8::try_from(block % 251).unwrap_or(0); 32],
        }),
        timestamp_ms: Some(at_ms),
        operations: ops
            .iter()
            .enumerate()
            .map(|(i, (kind, t, amount))| Operation {
                index: i as u64,
                kind: (*kind).to_owned(),
                address: t.map(|t| format!("0x{}", hex(&t))).unwrap_or_default(),
                amount: *amount,
                memo: String::new(),
            })
            .collect(),
        metadata: Vec::new(),
    }
}

/// A spend from `from` as the index lists it: the source debited gross, the
/// payees, the change back to `from`, and the fee at the node's floor.
fn spend(
    id: u8,
    block: u64,
    at_ms: i64,
    from: [u8; 20],
    to: &[([u8; 20], i128)],
    change: i128,
) -> MeshTransaction {
    let fee = i128::from(mochimo_crypto::consts::MFEE) * to.len() as i128;
    let sent: i128 = to.iter().map(|(_, a)| a).sum();
    let mut ops = vec![(OP_SOURCE, Some(from), -(sent + fee + change))];
    ops.extend(to.iter().map(|(t, a)| (OP_DESTINATION, Some(*t), *a)));
    ops.push((OP_DESTINATION, Some(from), change));
    ops.push((OP_FEE, None, fee));
    indexed(id, block, at_ms, &ops)
}

const MCM: i128 = 1_000_000_000;

/// `tx` with `reference` on its destination to `to`, as the indexer stores
/// one: the whole sixteen-byte field, padded with NULs.
fn referenced(mut tx: MeshTransaction, to: [u8; 20], reference: &str) -> MeshTransaction {
    let address = format!("0x{}", hex(&to));
    for op in &mut tx.operations {
        if op.kind == OP_DESTINATION && op.address == address {
            op.memo = format!("{reference:\0<16}");
        }
    }
    tx
}

/// What the node's index holds for each account of the sample store, newest
/// first, and the library's page for each. Some rows carry a reference on a
/// destination, as the index does when the spend gave one.
#[must_use]
pub fn activity() -> Vec<AccountHistory> {
    let own = spend(
        0x4b,
        869_130,
        1_790_439_640_000,
        tag(2),
        &[(tag(1), 500 * MCM)],
        2_680 * MCM,
    );
    let rows = [
        (
            tag(1),
            vec![
                referenced(
                    spend(
                        0x91,
                        870_668,
                        1_791_036_200_000,
                        tag(1),
                        &[(tag(7), 420 * MCM), (tag(8), 380 * MCM)],
                        9_215 * MCM,
                    ),
                    tag(7),
                    "INV-0412",
                ),
                referenced(
                    spend(
                        0x37,
                        870_645,
                        1_791_029_405_000,
                        tag(9),
                        &[(tag(1), 420 * MCM)],
                        1_200 * MCM,
                    ),
                    tag(1),
                    "ORDER-77",
                ),
                spend(
                    0x62,
                    870_190,
                    1_790_850_600_000,
                    tag(1),
                    &[(tag(10), 300 * MCM)],
                    10_015 * MCM,
                ),
                own.clone(),
            ],
        ),
        (
            tag(2),
            vec![
                own,
                spend(
                    0x2c,
                    867_780,
                    1_789_989_330_000,
                    tag(11),
                    &[(tag(2), 1_200 * MCM)],
                    40 * MCM,
                ),
                spend(
                    0x15,
                    865_850,
                    1_789_411_622_000,
                    tag(2),
                    &[(tag(12), 410 * MCM)],
                    2_000 * MCM,
                ),
            ],
        ),
        (
            tag(3),
            vec![indexed(
                0x7e,
                868_870,
                1_790_323_331_000,
                &[(OP_REWARD, Some(tag(3)), 85_135_096_906)],
            )],
        ),
    ];
    rows.into_iter()
        .map(|(t, transactions)| {
            let page = SearchPage {
                total_count: transactions.len() as u64,
                transactions,
                next_offset: None,
            };
            let text = text::page(
                &[],
                Outcome::RecentTransactions {
                    tag: t,
                    from: 0,
                    page: Box::new(page.clone()),
                },
            );
            AccountHistory::of(AccountId::from_tag(t), &page, text)
        })
        .collect()
}

/// The time of the sample chain's tip, in milliseconds since the epoch.
pub const TIP_MS: i64 = 1_791_186_252_000;

/// The six newest blocks of the sample chain, read down from its tip, and
/// the library's page for them.
#[must_use]
pub fn blocks() -> BlocksView {
    let tip = 871_173;
    let gaps = [0, 312, 289, 405, 251, 338];
    let counts = [3, 1, 5, 2, 1, 4];
    let mut at = TIP_MS;
    let mut rows = Vec::new();
    for (n, (gap, count)) in gaps.iter().zip(counts).enumerate() {
        at -= gap * 1_000;
        let index = tip - n as u64;
        rows.push(MeshBlock {
            block: ChainTip {
                index,
                hash: [u8::try_from(index % 251).unwrap_or(0); 32],
            },
            parent: ChainTip {
                index: index - 1,
                hash: [u8::try_from((index - 1) % 251).unwrap_or(0); 32],
            },
            timestamp_ms: at,
            transactions: (0..count)
                .map(|i| MeshTransaction {
                    hash: [u8::try_from(i).unwrap_or(0); 32],
                    block: None,
                    timestamp_ms: None,
                    operations: Vec::new(),
                    metadata: Vec::new(),
                })
                .collect(),
            metadata: None,
        });
    }
    let blocks = rows.iter().map(BlockSummary::of).collect();
    let tip = ChainTip {
        index: tip,
        hash: [u8::try_from(tip % 251).unwrap_or(0); 32],
    };
    BlocksView {
        tip: tip.index,
        blocks,
        text: text::page(
            &[],
            Outcome::Blocks {
                count: crate::explorer::CARD_BLOCKS,
                tip,
                rows,
            },
        ),
    }
}

/// Every account of the sample store reconciled now, as account recovery
/// shows them first: the library's report for each.
#[must_use]
pub fn review() -> Vec<AccountReport> {
    let found: Vec<([u8; 20], Result<AccountStatus, Divergence>)> = vec![
        (
            tag(1),
            Ok(AccountStatus::InSync {
                index: index(49),
                address: address(tag(1), 0x3c),
                balance: FIRST_BALANCE,
            }),
        ),
        (
            tag(2),
            Ok(AccountStatus::SpendOutstanding {
                spent_index: index(6),
                balance: 3_180_000_000_000,
                reservation: Reservation::Unrecorded,
            }),
        ),
        (tag(3), Err(diverged())),
    ];
    found
        .into_iter()
        .map(|(t, result)| {
            let state = match &result {
                Ok(status) => AccountState::from_status(status),
                Err(d) => AccountState::from_divergence(d),
            };
            AccountReport {
                account: AccountId::from_tag(t),
                spendable: matches!(state, AccountState::InSync { .. }),
                state,
                text: text::page(&[], mochimo_crypto::cli::status_outcome(&t, result)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_explorer_samples_carry_the_librarys_pages() {
        use crate::explorer::Direction;
        let histories = activity();
        assert_eq!(histories.len(), 3);
        let first = AccountId::from_tag(tag(1));
        let batch = &histories[0].transactions[0];
        assert_eq!(batch.direction(first), Direction::Both);
        assert_eq!(
            batch.net(first),
            -(800 * MCM + 2 * i128::from(mochimo_crypto::consts::MFEE))
        );
        assert_eq!(batch.paid_out(first).count(), 2);
        assert!(histories[2].transactions[0].rewards(AccountId::from_tag(tag(3))));
        assert!(
            histories
                .iter()
                .all(|h| h.text.contains("recent transactions"))
        );

        let b = blocks();
        assert_eq!(b.blocks.len(), 6);
        assert_eq!(b.blocks[0].index, b.tip);
        assert_eq!(b.blocks[0].time_ms, TIP_MS);

        let r = review();
        assert_eq!(r.len(), 3);
        assert!(matches!(
            r[2].state,
            AccountState::Diverged {
                advance_to: Some(33),
                ..
            }
        ));
        assert!(r.iter().all(|a| !a.text.is_empty()));
    }

    #[test]
    fn the_wallet_samples_carry_the_librarys_pages() {
        let receive = receive_view();
        assert_eq!(receive.index, 49);
        assert!(receive.destination.is_some());
        assert!(!receive.text.is_empty());

        let plan = plan_view();
        assert_eq!(plan.destinations.len(), 2);
        assert_eq!(
            plan.send_total + plan.fee_total + plan.change_total,
            plan.balance
        );
        assert!(
            plan.text.contains("THIS STORE IS NOT WHOLE"),
            "{}",
            plan.text
        );

        let sent = sent_view();
        assert!(sent.submitted);
        assert!(
            sent.text.contains(&sent.artifact_hex),
            "the page carries the artifact"
        );
        assert!(matches!(
            sent.view.accounts[0].state,
            AccountState::SpendOutstanding { .. }
        ));

        let (text, rows) = discovered();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows.iter().filter(|r| r.held).count(), 2);
        assert!(!text.is_empty());
        assert!(!submitted_text().is_empty());
    }

    #[test]
    fn the_sample_carries_the_librarys_words() {
        let view = wallet_view();
        assert_eq!(view.accounts.len(), 3);
        let notice = view.notice.expect("a store that is not whole says so");
        assert!(notice.starts_with("THIS STORE IS NOT WHOLE"), "{notice}");
        assert!(
            matches!(
                &view.accounts[2].state,
                AccountState::Diverged {
                    advance_to: Some(33),
                    ..
                }
            ),
            "{:?}",
            view.accounts[2].state
        );
    }
}
