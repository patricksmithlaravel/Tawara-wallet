//! What the worker reports.
//!
//! Plain data, apart from one value: [`Reply::CreatePhrase`] carries the new
//! store's recovery phrase, because showing it once is the requirement
//! (docs/PLAN.md section 4.3). It is a [`PhraseForDisplay`], which zeroizes
//! when dropped and prints nothing in `Debug`; an `Event` is therefore not
//! `Clone`, so the interface moves it into its state rather than copying it.

use core::fmt;
use std::path::PathBuf;

use crate::command::{PlanId, RequestId};
use crate::secret::PhraseForDisplay;
use crate::spend::SpendInputError;
use crate::view::{AccountId, AccountState, WalletView};

/// Something the worker reports.
#[derive(Debug)]
pub enum Event {
    /// A command has started something that takes time.
    Busy { id: RequestId, activity: Activity },
    /// A command finished. Every command gets exactly one.
    Done { id: RequestId, reply: Reply },
    /// The store was closed: its secret is dropped and its lock released.
    /// Sent whenever an open store closes, for any reason, including in
    /// answer to [`crate::Command::Lock`], and whenever a recovery phrase
    /// waiting for its confirmation is dropped by locking.
    Locked { reason: LockReason },
    /// The worker has stopped and will answer nothing more. `panicked` is
    /// true when it stopped because of a fault rather than a shutdown; the
    /// release profile unwinds, so every secret it held was zeroized on the
    /// way out (docs/DECISIONS.md D11).
    Stopped { panicked: bool },
}

/// What a slow command is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    /// Deriving the store's key from the password (Argon2id at the
    /// library's recommended cost: 64 MiB over three passes).
    DerivingKey,
    /// Waiting on the node.
    AskingNode,
}

/// Why the store closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockReason {
    /// [`crate::Command::Lock`].
    Asked,
    /// Nothing was done for the idle period.
    Idle,
    /// The app moved to the background (docs/PLAN.md section 4.1).
    Background,
    /// Another store was unlocked in its place.
    Replaced,
    /// The wallet was reopened (a refresh against a new node, an advance,
    /// a restore) and the library refused it. The worker opens the wallet
    /// only once an account has reconciled, so this means the chain moved in
    /// between. The library drops the store when it refuses, so it cannot
    /// stay open; the command's refusal says why, and unlocking again shows
    /// the state.
    WalletRefused,
    /// The store was reopened after an operation and could not be read
    /// back; the command's refusal says why.
    ReopenFailed,
    /// The worker is shutting down.
    Shutdown,
}

/// The answer to one command.
#[derive(Debug)]
pub enum Reply {
    /// The node was set. When it is a different node, an open store was
    /// detached from the previous one: `view` is the store afterwards, open
    /// but not reconciled against the new node until
    /// [`crate::Command::Refresh`].
    NodeSet {
        url: String,
        view: Option<WalletView>,
    },
    /// The node was forgotten, and an open store detached from it: `view`
    /// is the store afterwards.
    NodeCleared { view: Option<WalletView> },
    /// A new store's recovery phrase, to be shown once and then confirmed
    /// by the words at `confirm_positions` (one-based). Nothing is written.
    CreatePhrase {
        phrase: PhraseForDisplay,
        confirm_positions: [usize; 3],
    },
    /// A store was written in `dir` with `first` (account 0) in it, and
    /// opened with the same password: `opened` is the open store's view, or
    /// why it could not be opened (the store is written either way).
    Created {
        dir: PathBuf,
        first: AccountId,
        opened: Result<WalletView, Refusal>,
    },
    /// A pending phrase was dropped.
    CreateAbandoned,
    /// The store opened.
    Unlocked(WalletView),
    /// The store is closed (the [`Event::Locked`] says why).
    Locked,
    /// The store's state, after a refresh.
    Wallet(WalletView),
    /// An account's destination.
    Receive(ReceiveView),
    /// A spend, laid out and not yet signed.
    Planned(PlanView),
    /// The pending plan was dropped.
    PlanDiscarded,
    /// A spend was signed and submitted, or a reserved one re-signed and
    /// submitted.
    Sent(SentView),
    /// What settling found.
    Settled { text: String, view: WalletView },
    /// What the submitted artifact did.
    Submitted { accepted: bool, text: String },
    /// One account, reconciled now.
    Status {
        account: AccountId,
        state: AccountState,
        text: String,
    },
    /// What an acknowledged advance did. `ok` is whether the library counts
    /// it as done (the command line's exit status 0); `advanced_to` is the
    /// new index when it moved. `opened` is the store reopened afterwards,
    /// or why it could not be: then it is closed ([`Event::Locked`]), and
    /// the refusal carries the library's own report, the startup refusal
    /// when the library refused the wallet. The advance stands either way.
    Reconciled {
        ok: bool,
        advanced_to: Option<u32>,
        text: String,
        opened: Result<WalletView, Refusal>,
    },
    /// What a restore did, with `ok` and `opened` as for
    /// [`Reply::Reconciled`].
    Restored {
        ok: bool,
        text: String,
        opened: Result<WalletView, Refusal>,
    },
    /// What a discovery sweep found.
    Discovered {
        text: String,
        accounts: Vec<Discovered>,
    },
    /// The node's chain tip.
    Network { tip_index: u64, tip_hash: String },
    /// The command was refused; nothing it would have changed was changed
    /// unless `text` says otherwise.
    Refused(Refusal),
}

/// An account's destination, for receiving.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiveView {
    pub account: AccountId,
    /// Base58 over the tag and its CRC16: what to give a payer. `None` only
    /// if it cannot be rendered (see [`AccountId::destination`]).
    pub destination: Option<String>,
    /// The account's one-time key index.
    pub index: u32,
    /// The forty-byte entry the ledger will hold for the account at this
    /// index, in hex. Not a destination: no wallet takes it.
    pub ledger_address: String,
    /// The library's explanation of the two, word for word.
    pub text: String,
}

/// One destination of a planned spend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedDestination {
    pub to: AccountId,
    /// Rendered, whichever form was typed, to be compared against the payee.
    pub destination: String,
    pub amount: u64,
    /// The reference field's text; empty for none.
    pub reference: String,
}

/// A spend laid out, waiting for [`crate::Command::ConfirmSend`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanView {
    pub plan: PlanId,
    pub from: AccountId,
    /// In the order the person gave them.
    pub destinations: Vec<PlannedDestination>,
    pub send_total: u64,
    pub fee_total: u64,
    /// What returns to the account's next key.
    pub change_total: u64,
    /// The balance the plan was built against, from the ledger read it made.
    pub balance: u64,
    pub blk_to_live: u64,
    /// The change is zero: once this lands the node reports the account as
    /// not found, and every operation on it refuses until it is paid again.
    pub empties_account: bool,
}

/// A spend signed and submitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentView {
    pub from: AccountId,
    /// Whether the node's socket took the bytes. **A socket write, not a
    /// verdict**: whether the spend landed is settling's question.
    pub submitted: bool,
    /// The transaction id the node echoed, in hex, when it took them.
    pub tx_id: Option<String>,
    /// The signed bytes in hex: the retry artifact. The wallet does not keep
    /// them, and they are the only bytes that can move these funds while the
    /// reservation is open, so the interface offers to save them as a file
    /// ([`crate::save_artifact`]).
    pub artifact_hex: String,
    /// The library's page, with the three facts it never softens.
    pub text: String,
    /// The store's state afterwards.
    pub view: WalletView,
}

/// One derived account a discovery sweep asked about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovered {
    pub account_index: u32,
    pub id: AccountId,
    /// The balance the node reported, or `None` when it answered "account
    /// not found" (never funded, emptied, or a failed lookup: it cannot say
    /// which).
    pub ledger_balance: Option<u64>,
    /// Whether the store already holds it.
    pub held: bool,
}

/// A refused command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub kind: RefusalKind,
    /// What to show: the library's words where the library refused, the
    /// worker's otherwise.
    pub text: String,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// Why a command was refused, for the interface to choose a screen by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefusalKind {
    /// The command needs an open store.
    NotUnlocked,
    /// The command needs the wallet open (reconciled), and only the store
    /// is.
    WalletNotOpen,
    /// The command needs a node and none is chosen.
    NoNode,
    /// The node URL is not one this wallet uses.
    NodeRefused,
    /// Another process (another window, or the command-line wallet) holds
    /// the store's lock (docs/PLAN.md section 4.7).
    StoreInUse,
    /// The password does not open the store.
    WrongPassword,
    /// There is no store in the directory.
    NoStore,
    /// The library refused the directory: its owner, mode or access list, or
    /// a symbolic link.
    UnsafeDirectory,
    /// Creating: a store already exists there.
    Occupied,
    /// Creating: the password is shorter than the library's floor.
    PasswordTooShort,
    /// Creating: the two passwords differ.
    PasswordsDiffer,
    /// Creating: the confirmation words did not match.
    ConfirmationWrong,
    /// Creating: there is no pending phrase to confirm.
    NothingToConfirm,
    /// The spend as entered breaks one of the command line's rules.
    Spend(SpendInputError),
    /// The plan named is not the pending one.
    NoSuchPlan,
    /// The account diverged; no operation on it is permitted.
    Diverged,
    /// The wallet reopened to reconcile again and the library refused it.
    WalletRefused,
    /// The operating system's generator failed.
    Entropy,
    /// The person cancelled it.
    Cancelled,
    /// A number is outside what the worker takes: a scan or an advance
    /// further than [`crate::MAX_SCAN_TO`], or a discovery bound outside
    /// `1..=`[`crate::DISCOVER_MAX_TO`].
    OutOfRange,
    /// Any other refusal from the library; `text` is its own.
    Library,
}
