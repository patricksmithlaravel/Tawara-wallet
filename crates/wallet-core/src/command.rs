//! What the interface asks the worker to do.
//!
//! Every [`Command`] is answered by exactly one [`crate::Event::Done`]
//! carrying the same [`RequestId`]. A command that takes time (a key
//! derivation, a node) is preceded by an [`crate::Event::Busy`]. Commands
//! run one at a time, in the order they were sent.
//!
//! Commands that carry a secret take a [`SecretText`], which the interface
//! builds with [`SecretText::take`] so the text field is emptied as the
//! command is made. A command is not `Clone`, so a secret in one is never
//! duplicated by the channel.

use std::path::PathBuf;

use crate::secret::SecretText;
use crate::spend::SpendRequest;
use crate::view::AccountId;

/// Which request an event answers. Assigned by the handle, in sending order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestId(pub(crate) u64);

impl RequestId {
    /// An id for sample data (the screenshots, docs/DECISIONS.md D27 item
    /// 1). The worker numbers requests from 1, so this one never answers
    /// anything.
    #[must_use]
    pub const fn example() -> RequestId {
        RequestId(0)
    }
}

/// A planned spend waiting for confirmation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlanId(pub(crate) u64);

/// What the interface asks for.
#[derive(Debug)]
pub enum Command {
    /// Choose the node to ask (docs/PLAN.md section 4.8). There is no
    /// default. `https://` only, or `http://` to the loopback interface.
    /// Takes effect for the next command that asks a node. A different node
    /// detaches an open store from the previous one, dropping a pending
    /// plan; nothing can be sent until [`Command::Refresh`] reconciles the
    /// store against the new node.
    SetNode { url: String },
    /// Forget the node. Nothing asks a node until another is set, and an
    /// open store is detached from it as for [`Command::SetNode`].
    ClearNode,

    /// Start a new store in `dir`: check that nothing is there and that the
    /// password is long enough and typed the same twice, then make a
    /// recovery phrase and answer with it, to be shown once
    /// ([`crate::Reply::CreatePhrase`]). **Nothing is written yet.**
    CreateBegin {
        dir: PathBuf,
        password: SecretText,
        password_again: SecretText,
    },
    /// The words at the confirmation positions, separated by spaces. When
    /// they match, the store is written, account 0 is put in it, and it is
    /// opened with the password from [`Command::CreateBegin`]. When they do
    /// not, nothing is written and the phrase stays pending, so the
    /// interface can show it again. It stays pending too when the library
    /// refuses to write the store (an unsafe folder, a store there since),
    /// so the folder can be put right and the words given again; only a
    /// written store, [`Command::CreateAbandon`] or a lock drops it.
    CreateConfirm { answer: SecretText },
    /// Drop a pending phrase; nothing is written.
    CreateAbandon,
    /// Make a store from a recovery phrase the person already has (12 to 24
    /// words), and open it. The interface shows [`crate::SCHEME_WARNING`]
    /// before the phrase is typed.
    CreateFromPhrase {
        dir: PathBuf,
        password: SecretText,
        password_again: SecretText,
        phrase: SecretText,
    },

    /// Open the store in `dir` with its password, and reconcile it against
    /// the node when one is chosen. Any store already open is locked first.
    /// When the library will not open the wallet, the store stays open on
    /// its own with the library's page as its notice; nothing needs the
    /// password twice.
    Unlock { dir: PathBuf, password: SecretText },
    /// Lock: drop the store, its secret and its lock, and any pending phrase
    /// or plan. Like every command it runs after the ones sent before it;
    /// [`crate::WorkerHandle::cancel`] first stops those, as
    /// [`crate::WorkerHandle::background`] does.
    Lock,
    /// Ask the node about every account again. With the wallet open, each
    /// account's status is read fresh; with only the store open, opening
    /// the wallet is tried again.
    Refresh,

    /// An account's destination, and the address the ledger will hold for
    /// it at its current index. No node is asked.
    Receive { account: AccountId },

    /// Lay out a spend and answer with what it would do. Nothing is
    /// reserved or signed.
    PlanSend { spend: SpendRequest },
    /// Reserve the key the plan names, sign, and submit. The plan is
    /// checked again against the store, the tip and a fresh ledger read
    /// first; a refusal leaves the key unused.
    ConfirmSend { plan: PlanId },
    /// Forget a planned spend.
    DiscardPlan,
    /// Settle an account's reservation when the chain shows it landed.
    Settle { account: AccountId },
    /// Reproduce a reserved spend's signed bytes from its parameters (the
    /// destinations, amounts, references, fee and block-to-live used when
    /// it was made) and submit them: the one recovery for lost bytes.
    Resign { spend: SpendRequest },
    /// Submit signed bytes saved earlier, as hex. Opens no store.
    SubmitArtifact { artifact_hex: String },

    /// Reconcile one account now and report, whatever its state. `scan_to`
    /// sets how far the search for where the chain holds it goes (key
    /// indices `0..=scan_to`, beside the window around the store's index),
    /// at most [`crate::MAX_KEY_INDEX`]. Every index walked costs one key
    /// derivation, so a far one is a long wait; it can be cancelled.
    Status {
        account: AccountId,
        scan_to: Option<u32>,
    },
    /// The acknowledged advance for a diverged account: move it to
    /// `advance_to` only if the library's live report names exactly that
    /// index. The whole store is reconciled and reported first.
    /// `advance_to` is at most [`crate::MAX_KEY_INDEX`]. It can be cancelled
    /// until it writes, and a cancel writes nothing.
    Reconcile { account: AccountId, advance_to: u32 },
    /// Find where derived account `account_index` sits on the chain and put
    /// it in the store at that index. `scan_to`, at most
    /// [`crate::MAX_KEY_INDEX`], widens the search as for
    /// [`Command::Status`]. It can be cancelled until it writes, and a
    /// cancel writes nothing.
    Restore {
        account_index: u32,
        scan_to: Option<u32>,
    },
    /// Ask the node about derived accounts `0..=to`, `to` from 1 to
    /// [`crate::DISCOVER_MAX_TO`] as at the command line; nothing is
    /// written. A cancel stops it between two requests, and it then reports
    /// nothing found: a sweep that stopped short is not a sweep.
    Discover { to: u32 },

    /// The node's chain tip.
    NetworkStatus,
}
