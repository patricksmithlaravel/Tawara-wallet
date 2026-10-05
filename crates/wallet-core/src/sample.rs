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
use mochimo_crypto::mesh::{LedgerEntry, TxId};
use mochimo_crypto::recon::{ChainPosition, Divergence};
use mochimo_crypto::tx::wire::Destination;

use crate::command::PlanId;
use crate::event::{Discovered, PlanView, PlannedDestination, ReceiveView, SentView};
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
        },
        AccountRow {
            id: AccountId::from_tag(tag(3)),
            kind: AccountKind::Imported,
            index: 31,
            state: AccountState::from_divergence(&diverged),
            spendable: false,
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

#[cfg(test)]
mod tests {
    use super::*;

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
