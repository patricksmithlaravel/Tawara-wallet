//! Sample view models, for drawing the screens without a store or a node:
//! the screenshots (docs/DECISIONS.md D27, item 1).
//!
//! Every text in them is the library's own, rendered from sample values by
//! the same code that renders a real one, so a screenshot shows the words a
//! person will read. The tags are made-up bytes that belong to no one, and
//! the views are plain data: nothing here opens a store, asks a node or
//! holds a secret.

use std::path::PathBuf;

use mochimo_crypto::account::{StreamId, WotsIndex};
use mochimo_crypto::recon::{ChainPosition, Divergence};

use crate::view::{AccountId, AccountKind, AccountRow, AccountState, ReservationState, WalletView};

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

/// An open store of three accounts: one reconciled, one with a spend
/// outstanding, and one the chain shows two keys ahead, set aside with the
/// library's report; and the library's notice that the store is not whole.
#[must_use]
pub fn wallet_view() -> WalletView {
    let diverged = behind(tag(3), 31, 85_135_096_906);
    let rows = vec![
        AccountRow {
            id: AccountId::from_tag(tag(1)),
            kind: AccountKind::Derived,
            index: 49,
            state: AccountState::InSync {
                balance: 9_215_402_118_000,
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
        notice: crate::text::standing_notice(&[diverged]),
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
    view.notice = Some(crate::worker::NODE_SILENT.to_owned());
    view
}

#[cfg(test)]
mod tests {
    use super::*;

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
