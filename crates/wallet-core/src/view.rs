//! View models: what the worker reports, as plain data that holds no secret
//! (docs/PLAN.md section 3).
//!
//! Every type here is `Clone` and `Debug`, because nothing in it is
//! sensitive: tags, destinations, indexes, balances, and the library's own
//! report text. The interface reads these and never a library type.

use core::fmt;
use std::path::PathBuf;

use mochimo_crypto::account::AccountKind as LibraryKind;
use mochimo_crypto::addr::Tag;
use mochimo_crypto::recon::{AccountStatus, ChainPosition, Divergence, Expiry, Reservation};

/// An account's tag: the twenty bytes that name it on the ledger. Public,
/// not secret. Commands name accounts by this.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(Tag);

impl AccountId {
    /// The account with this tag. Any twenty bytes make an id; a command
    /// that names a tag the store does not hold is refused by the library.
    #[must_use]
    pub fn from_tag(tag: [u8; 20]) -> AccountId {
        AccountId(tag)
    }

    pub(crate) fn tag(self) -> Tag {
        self.0
    }

    /// The tag in hex with `0x`, the form the library uses when it names a
    /// tag it is not offering as somewhere to send funds.
    #[must_use]
    pub fn hex(self) -> String {
        let mut out = String::with_capacity(42);
        out.push_str("0x");
        for b in self.0 {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }

    /// The destination: Base58 over the tag and its CRC16, the form every
    /// Mochimo wallet takes. `None` only if the library cannot render it,
    /// which it documents as unreachable for a tag; a caller shows nothing
    /// rather than a second form (the library's `cli::destination` argues
    /// why).
    #[must_use]
    pub fn destination(self) -> Option<String> {
        mochimo_crypto::addr::tag_to_base58(&self.0).ok()
    }
}

impl fmt::Debug for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AccountId({})", self.hex())
    }
}

/// Where an account's keys come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountKind {
    /// Derived from the store's master seed.
    Derived,
    /// An imported root, held in the store.
    Imported,
}

impl From<LibraryKind> for AccountKind {
    fn from(kind: LibraryKind) -> AccountKind {
        match kind {
            LibraryKind::Derived => AccountKind::Derived,
            LibraryKind::Imported => AccountKind::Imported,
        }
    }
}

/// What reconciliation made of an open reservation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReservationState {
    /// The store records no figures for it (a store from before they were
    /// recorded).
    Unrecorded,
    /// It can still land.
    Live,
    /// It can no longer be accepted: the balance it was built on moved, or
    /// its block-to-live passed. Re-signing reproduces bytes the ledger will
    /// refuse.
    Dead { balance_moved: bool, expired: bool },
    /// Its expiry could not be read; the library's report says why.
    Unclassified,
}

impl From<&Reservation> for ReservationState {
    fn from(r: &Reservation) -> ReservationState {
        match r {
            Reservation::Unrecorded => ReservationState::Unrecorded,
            Reservation::Recorded(d) => {
                let expired = matches!(d.expiry, Expiry::Reached { .. });
                if d.balance_moved || expired {
                    ReservationState::Dead {
                        balance_moved: d.balance_moved,
                        expired,
                    }
                } else if matches!(d.expiry, Expiry::Unreadable { .. }) {
                    ReservationState::Unclassified
                } else {
                    ReservationState::Live
                }
            }
        }
    }
}

/// An account's state, as last observed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountState {
    /// The node holds the address this store derives at its index.
    InSync { balance: u64 },
    /// A spend is reserved and has not landed.
    SpendOutstanding {
        balance: u64,
        spent_index: u32,
        reservation: ReservationState,
    },
    /// The spend landed; settling records it and frees the account.
    SpendLanded {
        balance: u64,
        spent_index: u32,
        settled_index: u32,
    },
    /// Reconciliation could not explain the account, and no operation on it
    /// is permitted. `report` is the library's report, word for word.
    /// `advance_to` is the index an acknowledged advance would move it to,
    /// when advancing is the remedy.
    Diverged {
        /// What was found, for the interface to say in its own words; the
        /// report says it in full.
        kind: DivergenceKind,
        report: String,
        advance_to: Option<u32>,
    },
    /// Nothing has asked a node about it: no node is chosen.
    NotReconciled,
}

impl AccountState {
    pub(crate) fn from_status(status: &AccountStatus) -> AccountState {
        match status {
            AccountStatus::InSync { balance, .. } => AccountState::InSync { balance: *balance },
            AccountStatus::SpendOutstanding {
                spent_index,
                balance,
                reservation,
            } => AccountState::SpendOutstanding {
                balance: *balance,
                spent_index: spent_index.get(),
                reservation: reservation.into(),
            },
            AccountStatus::SpendLanded {
                spent_index,
                settled_index,
                balance,
            } => AccountState::SpendLanded {
                balance: *balance,
                spent_index: spent_index.get(),
                settled_index: settled_index.get(),
            },
        }
    }

    pub(crate) fn from_divergence(d: &Divergence) -> AccountState {
        AccountState::Diverged {
            kind: DivergenceKind::of(d),
            report: d.to_string(),
            advance_to: d.advance_target().map(|i| i.get()),
        }
    }

    /// The balance last observed, when one was.
    #[must_use]
    pub fn balance(&self) -> Option<u64> {
        match self {
            AccountState::InSync { balance }
            | AccountState::SpendOutstanding { balance, .. }
            | AccountState::SpendLanded { balance, .. } => Some(*balance),
            AccountState::Diverged { .. } | AccountState::NotReconciled => None,
        }
    }
}

/// What reconciliation found for an account it could not explain, as the
/// library tells the cases apart, so the interface can summarize the report
/// in its own words and name the causes that fit (docs/DECISIONS.md D28).
/// The report is the library's, whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DivergenceKind {
    /// The chain holds the account at this seed's key `gap` places ahead of
    /// the store's index: a spend landed that the store does not record.
    Ahead { gap: u32 },
    /// The chain holds it at this seed's key `gap` places behind the
    /// store's index.
    Behind { gap: u32 },
    /// None of the keys the search around the store's index reached is the
    /// address the chain holds.
    Unlocated,
    /// A spend is reserved, and the chain holds the account at neither the
    /// key that signed nor the change key.
    ReservationUnexplained,
    /// The node answered "account not found": no entry, a zero balance, or
    /// a lookup that failed, and it does not say which.
    NotFound,
    /// The node could not be reached for it.
    Unreachable,
    /// A derived account, and no master seed to derive its keys from.
    NoMaster,
    /// Reconciling it failed: a store error, an answer that did not parse.
    Failed,
}

impl DivergenceKind {
    fn of(d: &Divergence) -> DivergenceKind {
        let position = |found: &ChainPosition| match found {
            ChainPosition::Ahead { gap, .. } => DivergenceKind::Ahead { gap: *gap },
            ChainPosition::Behind { gap, .. } => DivergenceKind::Behind { gap: *gap },
            ChainPosition::Unlocated { .. } => DivergenceKind::Unlocated,
        };
        match d {
            Divergence::IndexMismatch { found, .. } => position(found),
            Divergence::ReservationUnexplained { .. } => DivergenceKind::ReservationUnexplained,
            Divergence::TagUnresolved { .. } => DivergenceKind::NotFound,
            Divergence::ChainUnreachable { .. } => DivergenceKind::Unreachable,
            Divergence::NoMasterForDerivedAccount { .. } => DivergenceKind::NoMaster,
            Divergence::CannotReconcile { .. } => DivergenceKind::Failed,
        }
    }
}

/// What a store's notice is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    /// No node is chosen (the worker's words).
    NoNode,
    /// The node was changed and nothing is reconciled against the new one
    /// yet (the worker's words).
    NodeChanged,
    /// The node did not answer (the worker's words, then the library's).
    NodeSilent,
    /// Reconciling was cancelled (the worker's words).
    Cancelled,
    /// The node could not be used (a refusal, in the library's or the
    /// worker's words).
    NodeRefused,
    /// Some accounts could not be reconciled and are set aside: the
    /// library's "THIS STORE IS NOT WHOLE". Their rows say how.
    NotWhole,
    /// The library would not open the wallet: its "WALLET WILL NOT START".
    WillNotStart,
}

/// The store's notice: what it is about, and its text, whole. It reads as
/// its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
}

impl core::ops::Deref for Notice {
    type Target = str;

    fn deref(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// One account in the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountRow {
    pub id: AccountId,
    pub kind: AccountKind,
    /// The one-time key index the store holds for it.
    pub index: u32,
    pub state: AccountState,
    /// Whether a spend may be planned from it now: the wallet opened, the
    /// account reconciled when it did, and it is in sync.
    pub spendable: bool,
    /// A derived account's number, as the command line's `restore
    /// --account N` names it, found by deriving from the store's seed
    /// (`crate::worker`, "Account numbers"). `None` for an imported
    /// account, for a store without its seed, and for a derived account
    /// not found within [`crate::DISCOVER_MAX_TO`].
    pub number: Option<u32>,
}

/// The unlocked store, as the interface shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletView {
    /// Where the store is.
    pub dir: PathBuf,
    pub accounts: Vec<AccountRow>,
    /// Whether the store opened as a wallet: every account reconciled
    /// against the node or was set aside as diverged, and spending is
    /// possible from the ones that reconciled. `false` when the wallet could
    /// not open (no node, an unreachable node, no account on the ledger yet,
    /// or every account diverged); the store is still open and its
    /// addresses can be shown.
    pub opened: bool,
    /// What the store as a whole needs said, when anything does: the
    /// library's notice that it is not whole, why the wallet would not open,
    /// or the worker's word that no node is chosen or answered. The
    /// interface says it in its own words and shows the text whole on
    /// request (docs/DECISIONS.md D28).
    pub notice: Option<Notice>,
}

/// What can be said about a store's total balance. An account that diverged
/// or was never reconciled has no known balance, so the sum of the others is
/// not the store's total and is never offered as one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Total {
    /// Every account's balance is known, and this is their sum.
    Whole(u128),
    /// Only some are: `known` is the sum of those, and `unknown` accounts
    /// are left out of it.
    Partial { known: u128, unknown: usize },
    /// No account's balance is known.
    Unknown,
}

impl WalletView {
    /// The store's total balance, as far as it is known.
    #[must_use]
    pub fn total(&self) -> Total {
        let mut known = 0u128;
        let mut unknown = 0;
        for account in &self.accounts {
            match account.state.balance() {
                Some(balance) => known += u128::from(balance),
                None => unknown += 1,
            }
        }
        if unknown == 0 {
            Total::Whole(known)
        } else if unknown == self.accounts.len() {
            Total::Unknown
        } else {
            Total::Partial { known, unknown }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_debug_show_the_tag() {
        let id = AccountId::from_tag([0xab; 20]);
        assert_eq!(id.hex(), format!("0x{}", "ab".repeat(20)));
        assert_eq!(
            format!("{id:?}"),
            format!("AccountId(0x{})", "ab".repeat(20))
        );
    }

    #[test]
    fn destination_is_base58_with_checksum() {
        let id = AccountId::from_tag([0x05; 20]);
        let d = id.destination().expect("renders");
        assert_eq!(
            mochimo_crypto::addr::tag_from_base58(&d).ok(),
            Some([0x05; 20])
        );
    }

    #[test]
    fn a_total_never_counts_an_unknown_balance_as_zero() {
        let row = |state| AccountRow {
            id: AccountId::from_tag([1; 20]),
            kind: AccountKind::Derived,
            index: 0,
            state,
            spendable: false,
            number: Some(0),
        };
        let view = WalletView {
            dir: PathBuf::new(),
            accounts: vec![
                row(AccountState::InSync { balance: u64::MAX }),
                row(AccountState::SpendLanded {
                    balance: 5,
                    spent_index: 1,
                    settled_index: 2,
                }),
                row(AccountState::NotReconciled),
            ],
            opened: false,
            notice: None,
        };
        // One account has no known balance: the sum of the others is not
        // the total.
        assert_eq!(
            view.total(),
            Total::Partial {
                known: u128::from(u64::MAX) + 5,
                unknown: 1,
            }
        );
        let mut whole = view.clone();
        whole.accounts.pop();
        assert_eq!(whole.total(), Total::Whole(u128::from(u64::MAX) + 5));
        let mut none = view;
        none.accounts.retain(|a| a.state.balance().is_none());
        assert_eq!(none.total(), Total::Unknown);
    }
}
