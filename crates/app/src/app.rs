//! The application: its state ([`Model`]), its messages ([`Message`]), how
//! a message changes the state ([`App::update`]), and the bridge from
//! wallet-core's worker into iced.
//!
//! Every library call goes through the worker (docs/PLAN.md section 3); the
//! application sends it [`Command`]s and turns its [`Event`]s into
//! messages. The views are functions of the [`Model`] alone
//! (`crate::screens`), so the screenshot example can draw any screen from
//! sample data without a worker (docs/DECISIONS.md D27, item 1).

mod wallet;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::host::{self, Host};
use iced::futures::channel::mpsc;
use iced::widget::operation;
use iced::{Element, Event as IcedEvent, Subscription, Task, event, keyboard, mouse, touch};
use tawara_wallet_core::explorer::{
    AccountHistory, BlockAt, BlocksView, ChainView, ExplorerRefusal, MempoolView, PendingView,
};
use tawara_wallet_core::location::{self, Environment, Platform};
use tawara_wallet_core::preferences::{self, Preferences};
use tawara_wallet_core::view::{AccountId, SettledAccount, WalletView, Watch};
use tawara_wallet_core::{
    Activity, Command, Config, Event, HttpsNode, LockReason, NetworkName, PhraseForDisplay,
    Progress, Refusal, RefusalKind, Reply, RequestId, SecretText, SyncState, WorkerHandle,
};

pub use wallet::{
    AccountPage, ActivityPage, AddAccountPage, BLOCK_ROWS, BlockPage, DestinationRow, Done,
    ExplorerPage, FormOf, Level, Missed, Page, QueuePage, ReceivePage, RecoveryPage, Remedy,
    ReportKey, ResignPage, SHARED, SendPage, SendStage, SentPage, SettingsPage, Signed, SpendForm,
    SubmitPage, TagPage, To, TransactionPage, WalletMsg, remedy, spend_request,
};

/// How often a poll asks the node about an account [`Watch::asks`] names: one
/// whose reservation is live and outstanding, or whose spend a failed read
/// left unanswered (docs/DECISIONS.md D38 item 2, as [`Watch`] extends it).
const POLL_EVERY: Duration = Duration::from_secs(60);

/// The accounts the worker settled on its own in answering with `reply`
/// (docs/DECISIONS.md D38), from the store's view the reply carries. Every
/// reply that opens or reads the wallet can carry them: a refresh, an unlock,
/// the read after a send, and the reopen after a restore or an advance. A
/// check's or a poll's own settlement is `Reply::Status::settled`, which the
/// check's answer shows itself.
fn settled_in(reply: &Reply) -> Vec<SettledAccount> {
    let view = match reply {
        Reply::Wallet(view) | Reply::Unlocked(view) | Reply::Settled { view, .. } => Some(view),
        Reply::Sent(sent) => Some(&sent.view),
        Reply::Created {
            opened: Ok(view), ..
        }
        | Reply::Reconciled {
            opened: Ok(view), ..
        }
        | Reply::Restored {
            opened: Ok(view), ..
        } => Some(view),
        Reply::NodeSet { view, .. } | Reply::NodeCleared { view } => view.as_ref(),
        _ => None,
    };
    view.map(|v| v.settled.clone()).unwrap_or_default()
}

/// What the application shows and holds. Plain data, apart from the
/// recovery phrase while it is being shown and confirmed, and the text of
/// the secret fields while they are being typed (see [`wipe`]).
#[derive(Debug)]
pub struct Model {
    pub screen: Screen,
    pub prefs: Preferences,
    /// Where a store goes when the person does not choose (D21).
    pub default_dir: Option<PathBuf>,
    /// Where "Save artifact" writes (docs/DECISIONS.md D27, item 5), when
    /// the system names a Downloads folder.
    pub downloads: Option<PathBuf>,
    /// The command the screen waits on, from when it is sent until its
    /// answer comes.
    pub busy: Option<Busy>,
    pub node: NodeState,
    /// The open store, when one is.
    pub wallet: Option<WalletView>,
    /// The spends signed this run whose page is not shown. A spend's
    /// signed bytes are the only ones that can move its reserved funds
    /// while the reservation is open, and the store does not keep them
    /// (docs/PLAN.md section 4.9), so leaving the page, however it is left
    /// (another page, the node screen, a lock, the worker stopping), keeps
    /// it: the wallet's pages offer it again while its bytes are not saved,
    /// and re-signing starts from what was typed for it. A saved spend is
    /// forgotten once its account is in sync again. Unsaved bytes are kept
    /// past that, since a reorganisation of the chain can revert a settle
    /// (docs/DECISIONS.md D38 item 5, as revised on 2026-10-13): they go
    /// only when they are saved, dismissed, or the wallet locks. The locks
    /// that release them are the person's, the idle one, the move to the
    /// background and the worker's shutdown; a store the worker closed after
    /// a failed read-back, or one another unlock replaced, keeps them, since
    /// no one asked for that and the bytes may not have been seen. A lock
    /// brings a saved spend's page back once its store is unlocked, and the
    /// stopped screen offers to save what is left. What the store showed is
    /// dropped from it, and whenever a spend is dropped its bytes are wiped,
    /// with the library's page, which repeats them.
    pub signed: Vec<Signed>,
    /// The worker stopped: `Some(true)` when it panicked. Nothing more can
    /// be done in this run.
    pub stopped: Option<bool>,
    /// The window's width in logical pixels. Below [`COMPACT`] the phone
    /// layout is drawn; from there up, the layouts of docs/DECISIONS.md D27,
    /// item 9, where rows of two cards stack below [`WIDE`]. A phone starts at
    /// [`NARROWEST`] until the window reports its size (docs/DECISIONS.md D32
    /// item 10 (b)).
    pub width: f32,
    /// What covers the edges of a phone's screen: the system's own UI, and
    /// the on-screen keyboard (docs/DECISIONS.md D32 item 10 (b)). Zero on
    /// the desktop, where the root is padded by nothing.
    pub safe_area: iced::mobile::SafeArea,
    /// The wall clock, in milliseconds since the epoch, moved on each
    /// second: what a block's age is told against.
    pub clock_ms: i64,
    /// The settlements the worker reported while no page of their account was
    /// shown: each one's banner waits for that page (docs/DECISIONS.md D38).
    settled_waiting: BTreeMap<AccountId, String>,
    /// When the last poll was sent, so that the polls come once a minute
    /// (docs/DECISIONS.md D38 item 2). A monotonic clock,
    /// not `clock_ms`: a wall clock set back would hold the polls back by as
    /// much.
    polled_at: Option<Instant>,
    /// The accounts a poll asks the node about, as the reads so far say
    /// (docs/DECISIONS.md D38 item 2; [`Watch`]). A lock and a stopped worker
    /// empty it.
    watch: Watch,
    /// Whether the window has the system's focus, as its last focus event
    /// said; a window starts with it. A poll asks only while it does, since
    /// D38 item 2 polls only in the foreground: a desktop window left behind
    /// others or minimised asks nothing until it is brought back.
    foreground: bool,
    /// The time zone dates are shown in: the system's.
    pub zone: crate::ui::Zone,
    /// The open store's transactions from the node's index (W1, W10).
    pub activity: Explored<Vec<AccountHistory>>,
    /// The node's newest blocks (W1's network card).
    pub blocks: Explored<BlocksView>,
    /// How many transactions wait in the node's queue (W1's network card).
    pub mempool: Explored<MempoolView>,
    /// The chain as the explorer shows it (E1).
    pub chain: Explored<ChainView>,
    /// The node's queue with its first transactions read whole (E1).
    pub pending: Explored<PendingView>,
    /// How many times a mobile shell has said the application left the
    /// foreground, counted on the shell's own thread, and how many of
    /// those this model has acted on (docs/DECISIONS.md D32 item 6).
    pub(crate) left: Arc<AtomicU64>,
    left_seen: u64,
    /// What this model has done for the moves to the background (the moves
    /// it has wiped for, the worker's `Backgrounded` it has taken), which a
    /// shell waits on before it
    /// lets the system suspend the process (docs/DECISIONS.md D32 item 6).
    pub(crate) wiped: Arc<host::Wiped>,
    /// In a mobile shell, the app-private folder every store must be
    /// inside: the one the system keeps out of backups and away from other
    /// apps, and on iOS protects (docs/DECISIONS.md D32 items 3 and 7).
    pub(crate) confined: Option<PathBuf>,
}

/// What an explorer read last answered, and whether another is on its way.
/// Reads go on in the background: the screen shows the last answer while
/// the next comes.
#[derive(Clone, Debug)]
pub struct Explored<T> {
    pub last: Option<Result<T, ExplorerRefusal>>,
    pub reading: bool,
    /// When the last answer came, by the wall clock ([`Model::clock_ms`]).
    pub read_ms: Option<i64>,
}

impl<T> Default for Explored<T> {
    fn default() -> Explored<T> {
        Explored {
            last: None,
            reading: false,
            read_ms: None,
        }
    }
}

/// The wall clock, in milliseconds since the epoch.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The narrowest window that shows two cards side by side.
pub const WIDE: f32 = 1280.0;

/// The width below which the phone layout is drawn: every phone held
/// upright is narrower, and the desktop's window is never narrower than
/// [`crate::WINDOW_MIN`]'s 1024 (docs/DECISIONS.md D32 item 10 (b)).
pub const COMPACT: f32 = 600.0;

/// The narrowest width the phone layout is drawn at (docs/DECISIONS.md D32
/// item 10 (b)): at it nothing is cut off and nothing scrolls sideways.
pub const NARROWEST: f32 = 360.0;

/// The command the screen waits on, and what the worker is doing for it.
#[derive(Clone, Debug)]
pub struct Busy {
    pub id: RequestId,
    /// `None` until the worker starts on it: it does one command at a time,
    /// and may be finishing an earlier one, such as a slow node's answer.
    pub activity: Option<Activity>,
    pub progress: Option<Progress>,
}

/// The node, as the application last saw it.
#[derive(Clone, Debug, Default)]
pub struct NodeState {
    /// The node chosen, when one is.
    pub url: Option<String>,
    /// The chain tip it last reported, and how long it took to answer.
    pub tip: Option<(u64, Duration)>,
    /// When that tip was solved, in milliseconds since the epoch.
    pub solved_ms: Option<i64>,
    /// The Mesh middleware's own sync state, as it last said it.
    pub sync: Option<SyncState>,
    /// The network it serves, by name, once it has said.
    pub network: Option<NetworkName>,
    /// Why it last did not answer, in the library's words.
    pub error: Option<String>,
}

/// Which screen is shown (docs/SCREENS.md).
#[derive(Debug)]
pub enum Screen {
    /// S1.
    Start { choice: StartChoice },
    /// S2.
    Node(NodeForm),
    /// S3.
    NewWallet(PasswordForm),
    /// S4. The phrase the worker made, shown once (docs/PLAN.md 4.3).
    Phrase(PhraseState),
    /// S5.
    Confirm(PhraseState),
    /// S6.
    Restore(RestoreForm),
    /// S7.
    Unlock(UnlockForm),
    /// The wallet, with the sidebar.
    Wallet(WalletPage),
}

/// The choice on the first screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartChoice {
    Create,
    Restore,
}

/// S2's form.
#[derive(Clone, Debug, Default)]
pub struct NodeForm {
    pub url: String,
    pub error: Option<String>,
    /// Where "Back" and a saved node lead.
    pub back: Back,
}

/// Where the node screen returns to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Back {
    #[default]
    Start,
    Unlock,
    Wallet,
}

/// S3's form, and the folder and passwords S6 shares with it.
#[derive(Debug, Default)]
pub struct PasswordForm {
    pub dir: String,
    pub password: String,
    pub again: String,
    pub error: Option<String>,
    /// The folder is inside one a cloud service syncs (docs/PLAN.md 4.5),
    /// in wallet-core's words.
    pub synced: Option<String>,
    /// Why the person is back here: a recovery phrase was discarded before
    /// it was confirmed, and nothing was created.
    pub note: Option<String>,
}

impl PasswordForm {
    fn in_dir(dir: String) -> PasswordForm {
        let synced = sync_note(&dir);
        PasswordForm {
            dir,
            synced,
            ..PasswordForm::default()
        }
    }
}

/// wallet-core's warning when `dir` is inside a folder a cloud service
/// syncs.
fn sync_note(dir: &str) -> Option<String> {
    if dir.trim().is_empty() {
        return None;
    }
    location::sync_warning(
        &store_dir(dir),
        Platform::current(),
        &Environment::from_process(),
    )
    .map(|w| w.to_string())
}

/// Whether `dir` lies strictly inside `root`, as the file system resolves
/// what exists of it: the deepest part of `dir` that exists is made
/// canonical, so neither a `..` nor a link leads out, and the rest, which
/// does not exist yet, must be plain names (`file_name` gives none for a
/// trailing `..`, and a `.` is no component).
fn within(dir: &Path, root: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let mut existing = dir;
    let mut rest = Vec::new();
    let resolved = loop {
        if let Ok(resolved) = existing.canonicalize() {
            break resolved;
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) if !name.is_empty() => {
                rest.push(name.to_owned());
                existing = parent;
            }
            _ => return false,
        }
    };
    let full = rest
        .iter()
        .rev()
        .fold(resolved, |path, name| path.join(name));
    full != root && full.starts_with(&root)
}

/// The folder typed in a field, as an absolute path: the store is opened,
/// shown and remembered by where it is, not by where the application was
/// started from. An empty field stays empty, for the library to refuse.
fn store_dir(typed: &str) -> PathBuf {
    let typed = Path::new(typed.trim());
    std::path::absolute(typed).unwrap_or_else(|_| typed.to_path_buf())
}

/// S4 and S5: the phrase, and the words typed to confirm it.
#[derive(Debug)]
pub struct PhraseState {
    pub phrase: PhraseForDisplay,
    /// The folder chosen on S3, kept for the way back to it.
    pub dir: String,
    pub positions: [usize; 3],
    /// The person said they wrote it down.
    pub written: bool,
    pub words: [String; 3],
    pub error: Option<String>,
}

/// S6's form.
#[derive(Debug, Default)]
pub struct RestoreForm {
    pub form: PasswordForm,
    pub phrase: String,
}

/// S7's form.
#[derive(Debug, Default)]
pub struct UnlockForm {
    pub dir: String,
    pub password: String,
    pub error: Option<String>,
    /// Why the store was locked, when the application locked it.
    pub note: Option<String>,
}

/// The wallet, with the sidebar: which page, and what it shows.
#[derive(Clone, Debug, Default)]
pub struct WalletPage {
    pub page: Page,
    /// A refusal the last command on this page met, whole.
    pub error: Option<String>,
    /// What was last put on the clipboard from this page, so its button
    /// can say so.
    pub copied: Option<String>,
    /// How far each of the page's reports is open; closed when absent.
    pub open: std::collections::BTreeMap<ReportKey, Level>,
}

/// Text typed into a secret field, on its way into the model. Its `Debug`
/// prints nothing of it, so no message log can show it.
///
/// `drawn` is the generation of the field it came from: the number of moves
/// to the background the model had acted on when the field was drawn (see
/// `Model::drawn`). A field drawn before the application left holds what
/// was typed then, and so does every message it makes, until it is drawn
/// again; such text is never taken (docs/DECISIONS.md D32 item 6).
#[derive(Clone)]
pub struct Typed {
    pub text: String,
    pub drawn: u64,
}

impl core::fmt::Debug for Typed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Typed(..)")
    }
}

/// An event from the worker, carried in a message. The event is taken out
/// once; the wrapper is `Clone` only because iced's messages are.
#[derive(Clone)]
pub struct WorkerEvent(Arc<Mutex<Option<Event>>>);

impl WorkerEvent {
    fn new(event: Event) -> WorkerEvent {
        WorkerEvent(Arc::new(Mutex::new(Some(event))))
    }

    fn take(&self) -> Option<Event> {
        self.0.lock().ok().and_then(|mut e| e.take())
    }
}

impl core::fmt::Debug for WorkerEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("WorkerEvent(..)")
    }
}

/// What the application reacts to.
#[derive(Clone, Debug)]
pub enum Message {
    /// The worker sent something.
    Worker(WorkerEvent),
    /// The person pressed a key, clicked, scrolled or touched: the idle
    /// period starts again (docs/DECISIONS.md D24).
    Input,
    /// Tab: the next field. Input too, as [`Message::Input`] is.
    FocusNext,
    /// The system's Back (Android): the shell takes the application to the
    /// background ([`Host::back`]).
    SystemBack,
    /// Shift-Tab: the previous field. Input too.
    FocusPrevious,
    /// Go to a screen.
    Go(Go),
    Choose(StartChoice),
    /// S1's "Continue".
    Continue,
    NodeUrl(String),
    SaveNode,
    ForgetNode,
    Dir(String),
    Password(Typed),
    Again(Typed),
    PhraseText(Typed),
    Word(usize, Typed),
    CreateWallet,
    Written(bool),
    ToConfirm,
    BackToPhrase,
    StartOver,
    ConfirmWords,
    RestoreWallet,
    UnlockWallet,
    /// Stop what the worker is doing (`WorkerHandle::cancel`).
    Cancel,
    Lock,
    Refresh,
    /// A wallet page (W1 to W9).
    Wallet(WalletMsg),
    /// A Paste button was tapped: the platform's clipboard is read for the
    /// field beside it (docs/DECISIONS.md D32 item 10 (g)).
    Paste(PasteInto),
    /// What the clipboard held when a Paste button was tapped, for the field
    /// that button was drawn beside. `None` when the read was refused.
    Pasted(PasteInto, Option<Clip>),
    /// The window opened or changed size: its width.
    Width(f32),
    /// The safe area changed: what the system covers at each edge, and the
    /// keyboard's height. Not the person's input, so it does not restart the
    /// idle period (docs/DECISIONS.md D32 item 10 (b)).
    SafeArea(iced::mobile::SafeArea),
    /// A second passed: the wall clock, in milliseconds since the epoch.
    /// Not the person's input.
    Tick(i64),
    /// The window gained (`true`) or lost (`false`) the system's focus. Not
    /// the person's input, so it does not restart the idle period: it says
    /// only whether the wallet is in the foreground, which a poll needs
    /// (docs/DECISIONS.md D38 item 2).
    Foreground(bool),
}

/// The field a Paste button fills (docs/DECISIONS.md D32 item 10 (g)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteInto {
    /// The destination of row `row` of the spend form (W4, W8), drawn on
    /// `form` when it had `rows` rows, so that a read which comes back once
    /// another form is shown, or once a row was added or removed, is
    /// dropped rather than put in another field.
    Destination {
        form: FormOf,
        row: usize,
        rows: usize,
    },
    /// The signed bytes (W9).
    SignedBytes,
    /// The recovery phrase (S6), as its field was drawn at the generation
    /// `drawn` (see [`Typed`]), so that a read which comes back after the
    /// application has left the foreground is refused.
    Phrase { drawn: u64 },
}

/// The text a Paste button's read took from the clipboard, on its way into
/// the model. Its `Debug` prints nothing of it, as [`Typed`]'s does: a copied
/// recovery phrase is a secret, and no message log may show one
/// (docs/DECISIONS.md D32 item 6).
#[derive(Clone)]
pub struct Clip(String);

impl core::fmt::Debug for Clip {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Clip(..)")
    }
}

/// The message that gives `into` what a Paste button's read took, or `None`
/// when the read was refused or holds nothing once it is cleaned, so that the
/// field stays as it is (docs/DECISIONS.md D32 item 10 (g) and (h)). The text
/// read is wiped once it has been cleaned.
fn pasted(into: PasteInto, read: Option<Clip>) -> Option<Message> {
    let mut read = read?.0;
    let message = match into {
        PasteInto::Destination { row, .. } => {
            let to = crate::paste::token(&read);
            (!to.is_empty()).then_some(Message::Wallet(WalletMsg::DestinationTo(row, to)))
        }
        PasteInto::SignedBytes => {
            let hex = crate::paste::hex(&read);
            (!hex.is_empty()).then_some(Message::Wallet(WalletMsg::ArtifactHex(hex)))
        }
        PasteInto::Phrase { drawn } => {
            let text = crate::paste::phrase(&read);
            (!text.is_empty()).then_some(Message::PhraseText(Typed { text, drawn }))
        }
    };
    wipe(&mut read);
    message
}

/// Whether a Paste read for `into` may still be put in its field
/// (docs/DECISIONS.md D32 item 10 (g)). The read can come back late, for
/// instance while iOS asks whether to allow the paste. A destination's is
/// taken only while nothing is being done for the page, as the field itself
/// is, and only by the form and row it was asked for; the signed bytes' only
/// on W9 while nothing is being done there. A phrase's read is checked by its
/// generation, as a typed phrase is (item 6).
fn takes(model: &Model, into: PasteInto) -> bool {
    let idle = model.busy.is_none();
    match into {
        PasteInto::Destination { form, row, rows } => {
            idle && row < rows && model.form_of() == Some((form, rows))
        }
        PasteInto::SignedBytes => {
            idle && matches!(
                &model.screen,
                Screen::Wallet(WalletPage {
                    page: Page::Submit(_),
                    ..
                })
            )
        }
        PasteInto::Phrase { .. } => true,
    }
}

/// The screens a message can go to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Go {
    Start,
    Node(Back),
    Unlock,
    Wallet,
}

/// What a sent command was for, to know what to do with its answer.
#[derive(Clone, Copy, Debug)]
enum Purpose {
    /// `SetNode`: from the node screen when `form`, at start-up otherwise.
    SetNode {
        form: bool,
    },
    ClearNode,
    Network {
        sent: Instant,
    },
    CreateBegin,
    CreateConfirm,
    CreateFromPhrase,
    Unlock,
    Refresh,
    Lock,
    Abandon,
    Receive,
    Discover,
    Restore,
    PlanSend,
    ConfirmSend,
    DiscardPlan,
    Settle,
    Resign,
    SubmitArtifact,
    Status,
    /// The network the node serves, in the background.
    Networks,
    /// The network card's blocks, in the background.
    Blocks,
    /// The network card's count of the node's queue, in the background.
    Mempool,
    /// The index's rows, in the background.
    Activity,
    /// The next older page of the index's rows, in the background.
    OlderActivity,
    /// Account recovery's report of every account.
    Review,
    /// The acknowledged advance.
    Reconcile,
    /// The explorer's chain, in the background.
    Chain,
    /// The explorer's queue, in the background.
    Pending,
    /// A block's page.
    Block(BlockAt),
    /// A tag's page.
    Tag(AccountId),
    /// An older page of a tag's history.
    TagHistory(AccountId),
    /// What a hash typed in the explorer names.
    Find([u8; 32]),
    /// The node's whole queue, for its own page.
    Queue,
    /// An account the poll asks about once a minute (`Watch::asks`): a
    /// settlement it finds is the worker's own, and a refusal says nothing
    /// (docs/DECISIONS.md D38).
    Poll,
}

impl Purpose {
    /// Whether the screen waits on the command: from when it is sent, its
    /// form gives way to S8 and its Cancel until the answer comes, so the
    /// person cannot start something else while it is queued.
    fn awaited(self) -> bool {
        matches!(
            self,
            Purpose::CreateBegin
                | Purpose::CreateConfirm
                | Purpose::CreateFromPhrase
                | Purpose::Unlock
                | Purpose::Refresh
                | Purpose::Discover
                | Purpose::Restore
                | Purpose::PlanSend
                | Purpose::ConfirmSend
                | Purpose::Settle
                | Purpose::Resign
                | Purpose::SubmitArtifact
                | Purpose::Status
                | Purpose::Review
                | Purpose::Reconcile
        )
    }

    /// Whether a wallet page sent the command (`wallet::on_wallet_reply`
    /// takes its answer).
    fn wallet_page(self) -> bool {
        matches!(
            self,
            Purpose::Receive
                | Purpose::Discover
                | Purpose::Restore
                | Purpose::PlanSend
                | Purpose::ConfirmSend
                | Purpose::DiscardPlan
                | Purpose::Settle
                | Purpose::Resign
                | Purpose::SubmitArtifact
                | Purpose::Status
                | Purpose::Review
                | Purpose::Reconcile
                | Purpose::Poll
                | Purpose::Block(_)
                | Purpose::Tag(_)
                | Purpose::TagHistory(_)
                | Purpose::Find(_)
                | Purpose::Queue
        )
    }
}

/// The worker's side of the application.
struct Worker {
    handle: WorkerHandle,
    waiting: Vec<(RequestId, Purpose)>,
}

/// The application: the model, and the worker it talks to.
pub struct App {
    pub model: Model,
    worker: Option<Worker>,
    prefs_path: Option<PathBuf>,
    /// How many times the idle period was started again: what the tests read
    /// to tell the person's input from what is not, without the wall clock.
    #[cfg(test)]
    touches: std::cell::Cell<u64>,
}

/// Empty a secret field so its text is zeroized, not just dropped:
/// `SecretText::take` moves the buffer into a zeroizing one and leaves the
/// field empty (docs/PLAN.md section 4.2).
pub fn wipe(field: &mut String) {
    drop(SecretText::take(field));
}

/// S3's note when a recovery phrase was dropped before it was confirmed,
/// `why` saying when.
fn phrase_discarded(why: &str) -> String {
    format!(
        "The recovery phrase was discarded {why}, before it was confirmed, and no store was \
         written. Choose the password again and continue to get a new phrase."
    )
}

/// Wipe every secret typed on `screen`: the passwords, the recovery phrase
/// and the words typed to confirm one. The one list of the fields that hold
/// them, for leaving a screen and for leaving the foreground.
fn wipe_typed(screen: &mut Screen) {
    match screen {
        Screen::NewWallet(f) => {
            wipe(&mut f.password);
            wipe(&mut f.again);
        }
        Screen::Restore(r) => {
            wipe(&mut r.form.password);
            wipe(&mut r.form.again);
            wipe(&mut r.phrase);
        }
        Screen::Unlock(u) => wipe(&mut u.password),
        Screen::Phrase(p) | Screen::Confirm(p) => {
            for w in &mut p.words {
                wipe(w);
            }
        }
        Screen::Start { .. } | Screen::Node(_) | Screen::Wallet(_) => {}
    }
}

/// Replace a secret field's text, zeroizing the old.
fn replace(field: &mut String, typed: Typed) {
    wipe(field);
    *field = typed.text;
}

/// Text typed or pasted for a field that is no longer shown, wiped as the
/// field's own text would be (docs/PLAN.md section 4.2).
fn discard(mut typed: Typed) {
    wipe(&mut typed.text);
}

/// The recovery phrase as RestoreWallet sends it: the field's text cleaned as
/// a paste into it is (docs/DECISIONS.md D32 item 10 (h)), and the field
/// wiped, so that the text as it was typed is not kept.
fn cleaned_phrase(field: &mut String) -> String {
    let phrase = crate::paste::phrase(field);
    wipe(field);
    phrase
}

impl Model {
    /// The model at start-up: the first screen, before the worker has said
    /// anything. When a store is there to open (the one last opened,
    /// wherever it is, or else the one in the default folder) it is the
    /// unlock screen for it; otherwise it is the start.
    #[must_use]
    /// The generation a secret field drawn now belongs to: the moves to
    /// the background acted on so far (see [`Typed`]).
    pub(crate) fn drawn(&self) -> u64 {
        self.left_seen
    }

    pub fn new(prefs: Preferences, default_dir: Option<PathBuf>) -> Model {
        let mut model = Model {
            screen: Screen::Start {
                choice: StartChoice::Create,
            },
            node: NodeState {
                url: prefs.node.clone(),
                ..NodeState::default()
            },
            prefs,
            default_dir,
            downloads: None,
            busy: None,
            wallet: None,
            signed: Vec::new(),
            stopped: None,
            left: Arc::new(AtomicU64::new(0)),
            left_seen: 0,
            wiped: Arc::default(),
            confined: None,
            width: crate::WINDOW.0,
            safe_area: iced::mobile::SafeArea::ZERO,
            clock_ms: now_ms(),
            settled_waiting: BTreeMap::new(),
            polled_at: None,
            watch: Watch::default(),
            foreground: true,
            zone: crate::ui::Zone::System,
            activity: Explored::default(),
            blocks: Explored::default(),
            mempool: Explored::default(),
            chain: Explored::default(),
            pending: Explored::default(),
        };
        if let Some(dir) = model.store_to_open() {
            model.screen = Screen::Unlock(UnlockForm {
                dir,
                ..UnlockForm::default()
            });
        }
        model
    }

    fn default_dir_text(&self) -> String {
        self.default_dir
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_default()
    }

    /// Whether the window is narrower than [`COMPACT`]: the phone layout is
    /// drawn (docs/DECISIONS.md D32 item 10 (b)).
    #[must_use]
    pub fn compact(&self) -> bool {
        self.width < COMPACT
    }

    /// Whether this application runs in a phone shell, where every store is
    /// kept in the app's own folder and no folder is chosen (docs/DECISIONS.md
    /// D32 item 10 (a)).
    #[must_use]
    pub(crate) fn phone(&self) -> bool {
        self.confined.is_some()
    }

    /// Confine this model to `private_dir`, the app-private folder a phone
    /// shell keeps out of backups and away from other apps (docs/DECISIONS.md
    /// D32 items 3 and 7, and item 10 (a)). The folder is no longer chosen: an
    /// unlock form takes the default folder. It touches no file, so the
    /// screenshot example can call it.
    pub fn confine(&mut self, private_dir: PathBuf) {
        self.confined = Some(private_dir);
        let dir = self.default_dir_text();
        if let Screen::Unlock(u) = &mut self.screen {
            u.dir = dir;
        }
    }

    /// The store to offer to unlock: the one last opened when it is still
    /// there, or else the one in the default folder when there is one. A
    /// phone offers only the default folder: a folder remembered from an
    /// earlier run is not one it can choose (docs/DECISIONS.md D32 item 10
    /// (a)).
    fn store_to_open(&self) -> Option<String> {
        // A relative folder means nothing from another working directory;
        // only an absolute one is remembered.
        let remembered = self
            .prefs
            .store
            .clone()
            .filter(|_| !self.phone())
            .filter(|s| {
                let s = Path::new(s);
                s.is_absolute() && tawara_wallet_core::store_exists(s)
            });
        remembered.or_else(|| {
            self.default_dir
                .as_deref()
                .filter(|d| tawara_wallet_core::store_exists(d))
                .map(|d| d.display().to_string())
        })
    }

    /// Leave the current screen, zeroizing whatever secret it held, and
    /// keeping a signed spend's page (see [`Model::signed`]).
    fn leave(&mut self) -> Screen {
        self.keep_signed(false);
        let mut old = core::mem::replace(
            &mut self.screen,
            Screen::Start {
                choice: StartChoice::Create,
            },
        );
        wipe_typed(&mut old);
        old
    }

    /// The application left the foreground: whatever secret is typed on
    /// the screen and not yet submitted is wiped where it is. The worker
    /// locks what it holds, but a password typed on S7 or a phrase typed on
    /// S6 is the interface's alone, and with no store open there is no
    /// `Locked` to clear it (docs/DECISIONS.md D32 item 6).
    ///
    /// A recovery phrase shown or being confirmed goes too, as the worker's
    /// lock drops its own copy, and the screen goes back to S3 saying so:
    /// it is the interface's to drop, and the shell does not let the
    /// process be suspended before it has (Tawara-mobile#1's second review).
    fn backgrounded(&mut self) {
        if matches!(self.screen, Screen::Phrase(_) | Screen::Confirm(_)) {
            self.back_to_new_wallet(Some(phrase_discarded(
                "when the application left the screen",
            )));
            return;
        }
        wipe_typed(&mut self.screen);
    }

    /// Back to S3, in the folder the pending phrase was for (the default
    /// one when there was none), with `note` saying why.
    fn back_to_new_wallet(&mut self, note: Option<String>) {
        let dir = match self.leave() {
            Screen::Phrase(p) | Screen::Confirm(p) => p.dir,
            // Already back here, when the interface dropped the phrase
            // before the worker's `Locked` came: the folder stays.
            Screen::NewWallet(f) => f.dir,
            _ => self.default_dir_text(),
        };
        self.screen = Screen::NewWallet(PasswordForm {
            note,
            ..PasswordForm::in_dir(dir)
        });
    }

    /// Why the folder on the screen cannot hold a store here: in a mobile
    /// shell, it is outside the app-private folder, where neither the
    /// backup exclusion nor iOS's file protection reaches (Tawara-mobile#1's
    /// third review). Nothing is sent, and nothing typed is taken.
    fn outside(&self) -> Option<String> {
        let typed = match &self.screen {
            Screen::NewWallet(f) => &f.dir,
            Screen::Restore(r) => &r.form.dir,
            Screen::Unlock(u) => &u.dir,
            _ => return None,
        };
        let root = self.confined.as_deref()?;
        (!within(&store_dir(typed), root)).then(|| {
            format!(
                "This store is outside the app's own folder, {}. The system keeps that folder \
                 out of backups and away from other apps, so Tawara does not open a store \
                 outside it.",
                root.display()
            )
        })
    }

    /// The unlock screen for `dir`, or the default folder. On a phone it is
    /// always the default folder (docs/DECISIONS.md D32 item 10 (a)).
    fn unlock_screen(&mut self, dir: Option<String>, error: Option<String>, note: Option<String>) {
        let _ = self.leave();
        let dir = if self.phone() {
            self.default_dir_text()
        } else {
            dir.or_else(|| self.store_to_open())
                .unwrap_or_else(|| self.default_dir_text())
        };
        self.screen = Screen::Unlock(UnlockForm {
            dir,
            password: String::new(),
            error,
            note,
        });
    }
}

impl App {
    /// Start the application: read the preferences, start the worker, and
    /// show the first screen.
    pub fn boot() -> (App, Task<Message>) {
        let env = Environment::from_process();
        let platform = Platform::current();
        App::begin(
            preferences::default_file(platform, &env).ok(),
            location::default_store_dir(platform, &env).ok(),
            location::downloads_dir(platform, &env),
        )
    }

    /// Start the application in a mobile shell: the store and the
    /// preferences in the shell's private directory, its clipboard for
    /// Copy, and its word when the application leaves the foreground
    /// (docs/DECISIONS.md D32).
    pub fn boot_in(host: &Host) -> (App, Task<Message>) {
        host::set(host);
        let (app, task) = App::hosted(&host.private_dir);
        host::on_leaving(app.leaving_hook());
        (app, task)
    }

    /// The application as a mobile shell runs it, in `private_dir`, before
    /// the shell's hooks are registered: the store and the preferences
    /// there, and every store confined to it.
    fn hosted(private_dir: &Path) -> (App, Task<Message>) {
        let (mut app, task) = App::begin(
            Some(private_dir.join(preferences::FILE_NAME)),
            Some(location::store_dir_in(private_dir)),
            None,
        );
        app.model.confine(private_dir.to_path_buf());
        // Before the window reports its size, a phone is drawn as the
        // narrowest one: its first frames are already the phone layout
        // (docs/DECISIONS.md D32 item 10 (b)).
        app.model.width = NARROWEST;
        // The first screen again, now that the store is the default folder's
        // alone: the unlock screen when a store is there, else the start.
        app.model.screen = match app.model.store_to_open() {
            Some(dir) => Screen::Unlock(UnlockForm {
                dir,
                ..UnlockForm::default()
            }),
            None => Screen::Start {
                choice: StartChoice::Create,
            },
        };
        (app, task)
    }

    /// What [`host::left_foreground`] does for this application: count the
    /// move first, on the shell's thread, so the next message the
    /// application takes already finds it; then the worker's lock. What the
    /// shell then waits for is the interface's taking of that lock's
    /// `Backgrounded`, or, with no worker, its wipe for the move.
    fn leaving_hook(&self) -> impl Fn() -> host::Leaving + Send + 'static {
        let left = Arc::clone(&self.model.left);
        let wiped = Arc::clone(&self.model.wiped);
        let worker = self.worker.as_ref().map(|w| w.handle.clone());
        move || {
            let this_move = left.fetch_add(1, Ordering::SeqCst) + 1;
            let done = match &worker {
                Some(handle) => host::Done::Taken(handle.background()),
                None => host::Done::Wiped(this_move),
            };
            host::Leaving::of(Some((Arc::clone(&wiped), done)))
        }
    }

    /// Read the preferences at `prefs_path`, start the worker, and show the
    /// first screen, with `default_dir` the store's folder by default and
    /// `downloads` where a signed spend is saved.
    fn begin(
        prefs_path: Option<PathBuf>,
        default_dir: Option<PathBuf>,
        downloads: Option<PathBuf>,
    ) -> (App, Task<Message>) {
        let prefs = prefs_path
            .as_deref()
            .map(preferences::load)
            .unwrap_or_default();
        let mut model = Model::new(prefs, default_dir);
        model.downloads = downloads;
        let (mut app, events) = App::start(model, prefs_path);
        let Some(events) = events else {
            return (app, Task::none());
        };
        // The worker's events arrive on a standard channel; a thread passes
        // them into a stream iced reads.
        let (tx, rx) = mpsc::unbounded();
        let bridged = std::thread::Builder::new()
            .name("tawara-events".into())
            .spawn(move || {
                for event in events {
                    if tx.unbounded_send(WorkerEvent::new(event)).is_err() {
                        break;
                    }
                }
            });
        if bridged.is_err() {
            app.model.stopped = Some(false);
        }
        // iced's thread-pool executor keeps no timer, so the clock is a
        // thread of its own, as the worker's events are.
        let (tick, ticks) = mpsc::unbounded();
        let _clock = std::thread::Builder::new()
            .name("tawara-clock".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                    if tick.unbounded_send(now_ms()).is_err() {
                        break;
                    }
                }
            });
        (
            app,
            Task::batch([
                Task::run(rx, Message::Worker),
                Task::run(ticks, Message::Tick),
            ]),
        )
    }

    /// The application over a new worker, with `model` as its first state,
    /// and the worker's events (`None` when it could not start). The
    /// remembered node is set first.
    fn start(
        mut model: Model,
        prefs_path: Option<PathBuf>,
    ) -> (App, Option<std::sync::mpsc::Receiver<Event>>) {
        let config = Config {
            idle_lock: model.prefs.idle_lock,
        };
        let Ok((handle, events)) = tawara_wallet_core::spawn(config, HttpsNode) else {
            model.stopped = Some(false);
            return (
                App {
                    model,
                    worker: None,
                    prefs_path,
                    #[cfg(test)]
                    touches: std::cell::Cell::default(),
                },
                None,
            );
        };
        let mut app = App {
            model,
            worker: Some(Worker {
                handle,
                waiting: Vec::new(),
            }),
            prefs_path,
            #[cfg(test)]
            touches: std::cell::Cell::default(),
        };
        if let Some(url) = app.model.prefs.node.clone() {
            app.send(Command::SetNode { url }, Purpose::SetNode { form: false });
        }
        (app, Some(events))
    }

    pub fn view(&self) -> Element<'_, Message> {
        crate::screens::view(&self.model)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            event::listen_with(person_input),
            iced::mobile::safe_area().map(Message::SafeArea),
        ])
    }

    fn send(&mut self, command: Command, purpose: Purpose) -> Option<RequestId> {
        // One command at a time is waited on: a second would take the
        // first's place in `busy`, and its Cancel and progress with it.
        if purpose.awaited() && self.model.busy.is_some() {
            return None;
        }
        let worker = self.worker.as_mut()?;
        match worker.handle.send(command) {
            Ok(id) => {
                worker.waiting.push((id, purpose));
                if purpose.awaited() {
                    self.model.busy = Some(Busy {
                        id,
                        activity: None,
                        progress: None,
                    });
                }
                Some(id)
            }
            Err(_) => {
                self.stop(false);
                None
            }
        }
    }

    /// The person did something: the idle period starts again.
    fn touch(&self) {
        #[cfg(test)]
        self.touches.set(self.touches.get() + 1);
        if let Some(w) = &self.worker {
            w.handle.touch();
        }
    }

    /// Asks the node about each account that [`Watch::asks`] names, once a
    /// minute, while a wallet is open and its window is in the foreground
    /// (docs/DECISIONS.md D38 item 2): an account whose reservation is live and
    /// outstanding, and one whose spend a failed read left unanswered, as
    /// [`Watch`] sets out. Each poll is noted in the watch, which bounds the
    /// polls of a spend in doubt.
    /// The worker's own read settles an account whose chain shows the change
    /// key, and the answer is shown as a settlement's banner when it does. A
    /// poll is not the person's input, so it does not restart the idle period
    /// (D24). Nothing shows for a poll that is refused or unanswered: the next
    /// one asks again. On the desktop a window that loses the focus asks
    /// nothing until it has it again. On a phone a move to the background locks
    /// the wallet, which leaves nothing open to poll and forgets which accounts
    /// were polled (docs/DECISIONS.md D32 item 6).
    fn poll(&mut self, now: Instant) {
        let accounts = self.polled_accounts(now);
        if accounts.is_empty() {
            return;
        }
        self.model.polled_at = Some(now);
        for account in accounts {
            let m = &mut self.model;
            if let Some(row) = m
                .wallet
                .as_ref()
                .and_then(|w| w.accounts.iter().find(|a| a.id == account))
            {
                m.watch.asked(account, &row.state);
            }
            self.send(
                Command::Status {
                    account,
                    scan_to: None,
                },
                Purpose::Poll,
            );
        }
    }

    /// The accounts a poll asks about at `now` (see [`App::poll`]): in an open
    /// wallet whose window is in the foreground, when a minute has passed since
    /// the last poll and nothing else is awaited, those that [`Watch::asks`]
    /// names. An account whose reservation is live is polled. One whose read
    /// failed is polled once a read showed its reservation live, when the store
    /// holds a reservation for it, or once a spend was signed for it in this
    /// run ([`Watch::signed`]), so a wallet opened during an outage polls its
    /// outstanding spend; an answer of "account not found", and a
    /// landed spend the automatic settle did not make, are polled at most
    /// twice more in a row, after which Refresh, Check now or the Settle button
    /// remain (docs/DECISIONS.md D38 item 2, as [`Watch`] extends it). A dead
    /// reservation is never polled, since it never settles. Any other diverged
    /// account is not polled either: it is for the person to look at, not for
    /// a timer to walk the chain for (docs/DECISIONS.md D19, D38).
    fn polled_accounts(&self, now: Instant) -> Vec<AccountId> {
        let m = &self.model;
        let due = m
            .polled_at
            .is_none_or(|at| now.saturating_duration_since(at) >= POLL_EVERY);
        // A poll waits behind nothing the person asked for, and never queues
        // a second behind one still on its way.
        if !due
            || !m.foreground
            || m.busy.is_some()
            || self.waiting_on(|p| matches!(p, Purpose::Poll))
        {
            return Vec::new();
        }
        // Only an open wallet settles: a store open on its own, with no node
        // or with the node not answering, has nothing a poll could settle.
        let Some(wallet) = m.wallet.as_ref().filter(|w| w.opened) else {
            return Vec::new();
        };
        wallet
            .accounts
            .iter()
            .filter(|a| m.watch.asks(a.id, &a.state))
            .map(|a| a.id)
            .collect()
    }

    fn save_prefs(&self) {
        if let Some(path) = &self.prefs_path {
            // A preference that cannot be written is lost at the next start,
            // and nothing else depends on it.
            let _ = preferences::save(path, &self.model.prefs);
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        // The application left the foreground since the last message: what
        // is typed and not submitted is wiped here, before this message is
        // looked at, and not when the worker's `Backgrounded` comes, which
        // a command still running can hold back while the person returns
        // and presses Unlock (PR #13's review; docs/DECISIONS.md D32 item
        // 6). A secret's text sent now was taken from the field as last
        // drawn, before the wipe, so it is dropped: the field is drawn
        // empty and typed again.
        let left = self.model.left.load(Ordering::SeqCst);
        // The clock's tick each second is such a message, so this runs
        // within a second of the move even while the worker is busy, and
        // the shell waiting to let the system suspend the process hears of
        // it.
        if left != self.model.left_seen {
            self.model.left_seen = left;
            self.model.backgrounded();
            self.model.wiped.wiped(left);
        }
        // iced takes a whole batch of messages against the fields as drawn
        // before it draws them again, so every one of them from a field
        // drawn before the latest move is refused, not only the first, and
        // its text wiped (PR #13's third review).
        if let Message::Password(typed)
        | Message::Again(typed)
        | Message::PhraseText(typed)
        | Message::Word(_, typed) = &message
            && typed.drawn != self.model.left_seen
        {
            if let Message::Password(mut typed)
            | Message::Again(mut typed)
            | Message::PhraseText(mut typed)
            | Message::Word(_, mut typed) = message
            {
                wipe(&mut typed.text);
            }
            return Task::none();
        }
        let m = &mut self.model;
        match message {
            Message::Worker(event) => {
                if let Some(event) = event.take() {
                    return self.on_event(event);
                }
            }
            Message::Input => self.touch(),
            Message::SystemBack => host::back(),
            // iced's fields leave Tab to the application.
            Message::FocusNext => {
                self.touch();
                return operation::focus_next();
            }
            Message::FocusPrevious => {
                self.touch();
                return operation::focus_previous();
            }
            Message::Go(Go::Start) => {
                let _ = m.leave();
                m.screen = Screen::Start {
                    choice: StartChoice::Create,
                };
            }
            Message::Go(Go::Unlock) => m.unlock_screen(None, None, None),
            Message::Go(Go::Wallet) => {
                if m.wallet.is_some() {
                    let _ = m.leave();
                    m.screen = Screen::Wallet(WalletPage::default());
                }
            }
            Message::Go(Go::Node(back)) => {
                let url = m.node.url.clone().unwrap_or_default();
                let _ = m.leave();
                m.screen = Screen::Node(NodeForm {
                    url,
                    error: None,
                    back,
                });
            }
            Message::Choose(choice) => {
                if let Screen::Start { choice: c } = &mut m.screen {
                    *c = choice;
                }
            }
            Message::Continue => {
                let choice = match &m.screen {
                    Screen::Start { choice } => *choice,
                    _ => return Task::none(),
                };
                let dir = m.default_dir_text();
                m.screen = match choice {
                    StartChoice::Create => Screen::NewWallet(PasswordForm::in_dir(dir)),
                    StartChoice::Restore => Screen::Restore(RestoreForm {
                        form: PasswordForm::in_dir(dir),
                        phrase: String::new(),
                    }),
                };
            }
            Message::NodeUrl(url) => {
                if let Screen::Node(f) = &mut m.screen {
                    f.url = url;
                    f.error = None;
                }
            }
            Message::SaveNode => {
                if let Screen::Node(f) = &m.screen {
                    let url = f.url.trim().to_owned();
                    self.send(Command::SetNode { url }, Purpose::SetNode { form: true });
                }
            }
            Message::ForgetNode => {
                self.send(Command::ClearNode, Purpose::ClearNode);
            }
            // A phone chooses no folder: the store is in the app's own one
            // (docs/DECISIONS.md D32 item 10 (a)).
            Message::Dir(_) if m.phone() => {}
            Message::Dir(dir) => match &mut m.screen {
                Screen::NewWallet(f) => {
                    f.synced = sync_note(&dir);
                    f.dir = dir;
                }
                Screen::Restore(r) => {
                    r.form.synced = sync_note(&dir);
                    r.form.dir = dir;
                }
                Screen::Unlock(u) => u.dir = dir,
                _ => {}
            },
            // Text for a field no longer shown, a Paste read that came back
            // after Back among it, is wiped rather than dropped as it is
            // (docs/PLAN.md section 4.2).
            Message::Password(typed) => match &mut m.screen {
                Screen::NewWallet(f) => replace(&mut f.password, typed),
                Screen::Restore(r) => replace(&mut r.form.password, typed),
                Screen::Unlock(u) => replace(&mut u.password, typed),
                _ => discard(typed),
            },
            Message::Again(typed) => match &mut m.screen {
                Screen::NewWallet(f) => replace(&mut f.again, typed),
                Screen::Restore(r) => replace(&mut r.form.again, typed),
                _ => discard(typed),
            },
            Message::PhraseText(typed) => match &mut m.screen {
                Screen::Restore(r) => replace(&mut r.phrase, typed),
                _ => discard(typed),
            },
            Message::Word(i, typed) => match &mut m.screen {
                Screen::Confirm(p) if i < p.words.len() => replace(&mut p.words[i], typed),
                _ => discard(typed),
            },
            Message::CreateWallet => {
                let outside = m.outside();
                if let (Screen::NewWallet(f), Some(why)) = (&mut m.screen, outside) {
                    f.error = Some(why);
                } else if let Screen::NewWallet(f) = &mut m.screen {
                    f.error = None;
                    f.note = None;
                    let command = Command::CreateBegin {
                        dir: store_dir(&f.dir),
                        password: SecretText::take(&mut f.password),
                        password_again: SecretText::take(&mut f.again),
                    };
                    self.send(command, Purpose::CreateBegin);
                }
            }
            Message::Written(yes) => {
                if let Screen::Phrase(p) = &mut m.screen {
                    p.written = yes;
                }
            }
            Message::ToConfirm => {
                if matches!(&m.screen, Screen::Phrase(p) if p.written)
                    && let Screen::Phrase(p) = m.leave()
                {
                    m.screen = Screen::Confirm(PhraseState { error: None, ..p });
                }
            }
            Message::BackToPhrase => {
                if let Screen::Confirm(p) = m.leave() {
                    m.screen = Screen::Phrase(p);
                }
            }
            Message::StartOver => {
                // The pending phrase is dropped in the worker and here.
                self.send(Command::CreateAbandon, Purpose::Abandon);
                self.model.back_to_new_wallet(None);
            }
            Message::ConfirmWords => {
                if let Screen::Confirm(p) = &mut m.screen {
                    p.error = None;
                    let mut answer = p
                        .words
                        .iter()
                        .map(|w| w.trim())
                        .collect::<Vec<_>>()
                        .join(" ");
                    for w in &mut p.words {
                        wipe(w);
                    }
                    let answer = SecretText::take(&mut answer);
                    self.send(Command::CreateConfirm { answer }, Purpose::CreateConfirm);
                }
            }
            Message::RestoreWallet => {
                let outside = m.outside();
                if let (Screen::Restore(r), Some(why)) = (&mut m.screen, outside) {
                    r.form.error = Some(why);
                } else if let Screen::Restore(r) = &mut m.screen {
                    r.form.error = None;
                    // The whole phrase is cleaned as a paste into it is, so a
                    // phrase typed or pasted one word to a line is read word by
                    // word (docs/DECISIONS.md D32 item 10 (h)).
                    let mut phrase = cleaned_phrase(&mut r.phrase);
                    let command = Command::CreateFromPhrase {
                        dir: store_dir(&r.form.dir),
                        password: SecretText::take(&mut r.form.password),
                        password_again: SecretText::take(&mut r.form.again),
                        phrase: SecretText::take(&mut phrase),
                    };
                    self.send(command, Purpose::CreateFromPhrase);
                }
            }
            Message::UnlockWallet => {
                let outside = m.outside();
                if let (Screen::Unlock(u), Some(why)) = (&mut m.screen, outside) {
                    u.error = Some(why);
                } else if let Screen::Unlock(u) = &mut m.screen {
                    u.error = None;
                    u.note = None;
                    let command = Command::Unlock {
                        dir: store_dir(&u.dir),
                        password: SecretText::take(&mut u.password),
                    };
                    self.send(command, Purpose::Unlock);
                }
            }
            Message::Cancel => {
                if let Some(w) = &self.worker {
                    w.handle.cancel();
                }
            }
            Message::Lock => {
                // Stop what is running and queued first, so the store does
                // not stay open behind a slow refresh (docs/PLAN.md 4.1): a
                // cancel reaches every command sent before it, and the Lock
                // sent after it runs as soon as the worker is free.
                if let Some(w) = &self.worker {
                    w.handle.cancel();
                }
                self.send(Command::Lock, Purpose::Lock);
            }
            Message::Refresh => {
                // One refresh at a time: a second would queue another
                // reconciliation behind it.
                if m.busy.is_some() {
                    return Task::none();
                }
                if let Screen::Wallet(p) = &mut m.screen {
                    p.error = None;
                }
                // The tip is asked for once the refresh has answered
                // (`on_reply`), not queued behind it now.
                self.send(Command::Refresh, Purpose::Refresh);
            }
            Message::Wallet(msg) => return self.on_wallet(msg),
            // The clipboard is read only on the tap, and the read comes back as
            // a message for the field the button was drawn beside
            // (docs/DECISIONS.md D32 item 10 (g)).
            Message::Paste(into) => {
                return iced::clipboard::read()
                    .map(move |read| Message::Pasted(into, read.map(Clip)));
            }
            // What the read held is handed on as the message it makes, so the
            // field's own rules apply to it, a phrase's generation among them
            // (docs/DECISIONS.md D32 item 6). A read that holds nothing changes
            // nothing.
            Message::Pasted(into, read) => {
                if !takes(m, into) {
                    if let Some(Clip(mut text)) = read {
                        wipe(&mut text);
                    }
                    return Task::none();
                }
                return match pasted(into, read) {
                    Some(message) => self.update(message),
                    None => Task::none(),
                };
            }
            Message::Width(width) => m.width = width,
            // Not the person's input: the idle period is not restarted here.
            // When the keyboard comes up or grows on a phone, the page is
            // scrolled so the focused field stands above it, once the page
            // has been laid out again above the keyboard (docs/DECISIONS.md D32
            // item 10 (b)).
            Message::SafeArea(area) => {
                let rose = area.keyboard > m.safe_area.keyboard;
                m.safe_area = area;
                if rose && m.compact() {
                    return iced::advanced::widget::operate(crate::screens::reveal_focused())
                        .discard();
                }
            }
            // The clock's second is what a poll waits for (docs/DECISIONS.md D38).
            // It is not the person's input.
            Message::Tick(ms) => {
                m.clock_ms = ms;
                self.poll(Instant::now());
            }
            Message::Foreground(focused) => m.foreground = focused,
        }
        Task::none()
    }

    /// Ask the node for its tip. How long it took to answer is measured
    /// from when the worker starts asking (its `Busy`), not from when the
    /// request was queued behind others.
    /// The network the node serves is asked for until it has said, one
    /// request at a time.
    fn ask_tip(&mut self) {
        if self.model.node.url.is_some() {
            self.send(
                Command::NetworkStatus,
                Purpose::Network {
                    sent: Instant::now(),
                },
            );
            if self.model.node.network.is_none()
                && !self.waiting_on(|p| matches!(p, Purpose::Networks))
            {
                self.send(Command::Networks, Purpose::Networks);
            }
            self.read_blocks();
        }
    }

    /// Whether a command sent for a purpose `is` holds is still unanswered.
    fn waiting_on(&self, is: impl Fn(Purpose) -> bool) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|w| w.waiting.iter().any(|&(_, p)| is(p)))
    }

    /// Read the newest blocks and count the node's queue for the wallet's
    /// network card, in the background, each unless a read of it is already
    /// on its way or no store is open to show it.
    fn read_blocks(&mut self) {
        if self.model.wallet.is_none() || self.model.node.url.is_none() {
            return;
        }
        if !self.model.blocks.reading && self.send(Command::Blocks, Purpose::Blocks).is_some() {
            self.model.blocks.reading = true;
        }
        if !self.model.mempool.reading && self.send(Command::Mempool, Purpose::Mempool).is_some() {
            self.model.mempool.reading = true;
        }
    }

    /// Read the explorer's chain and queue, in the background, each unless
    /// a read of it is already on its way. No store is needed, but the
    /// explorer is one of the wallet's pages.
    pub(crate) fn read_chain(&mut self) {
        if self.model.node.url.is_none() {
            return;
        }
        if !self.model.chain.reading && self.send(Command::Chain, Purpose::Chain).is_some() {
            self.model.chain.reading = true;
        }
        if !self.model.pending.reading
            && self
                .send(
                    Command::Pending {
                        count: tawara_wallet_core::explorer::PENDING_ROWS,
                    },
                    Purpose::Pending,
                )
                .is_some()
        {
            self.model.pending.reading = true;
        }
    }

    /// Read every account's transactions from the node's index, in the
    /// background, unless a read is already on its way.
    fn read_activity(&mut self) {
        if self.model.wallet.is_some()
            && self.model.node.url.is_some()
            && !self.model.activity.reading
            && self.send(Command::Activity, Purpose::Activity).is_some()
        {
            self.model.activity.reading = true;
        }
    }

    /// Read the next older page of every account whose history the index
    /// holds more of, in the background, unless a read is already on its
    /// way.
    pub(crate) fn read_older_activity(&mut self) {
        let Some(Ok(histories)) = &self.model.activity.last else {
            return;
        };
        let pages: Vec<_> = histories
            .iter()
            .filter(|h| h.more())
            .map(|h| (h.account, h.next))
            .collect();
        if !pages.is_empty()
            && !self.model.activity.reading
            && self
                .send(Command::OlderActivity(pages), Purpose::OlderActivity)
                .is_some()
        {
            self.model.activity.reading = true;
        }
    }

    fn on_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Busy { id, activity } => {
                if let Some(w) = &mut self.worker
                    && let Some((_, Purpose::Network { sent })) =
                        w.waiting.iter_mut().find(|(i, _)| *i == id)
                {
                    *sent = Instant::now();
                }
                if let Some(b) = &mut self.model.busy
                    && b.id == id
                {
                    b.activity = Some(activity);
                }
            }
            Event::Progress { id, progress } => {
                if let Some(b) = &mut self.model.busy
                    && b.id == id
                {
                    b.progress = Some(progress);
                }
            }
            Event::Done { id, reply } => {
                if self.model.busy.as_ref().is_some_and(|b| b.id == id) {
                    self.model.busy = None;
                }
                let purpose = self.worker.as_mut().and_then(|w| {
                    let at = w.waiting.iter().position(|(i, _)| *i == id)?;
                    Some(w.waiting.remove(at).1)
                });
                if let Some(purpose) = purpose {
                    return self.on_reply(purpose, reply);
                }
            }
            Event::Locked { reason } => {
                let dir = self
                    .model
                    .wallet
                    .take()
                    .map(|w| w.dir.display().to_string());
                // A lock wipes the signed bytes that were not saved, and drops the
                // page of such a spend when it is shown (docs/DECISIONS.md D38
                // item 5, as revised on 2026-10-13). A saved spend is kept as it
                // was. Only the locks the owner named release them (asked for,
                // idle, the background) and the shutdown, which ends the run.
                // A store the worker closed because it could not be read back
                // after a send or a settle, or one replaced by another unlock,
                // keeps them: no one asked for that close, and bytes just signed
                // may not have been seen yet.
                let releases = matches!(
                    reason,
                    LockReason::Asked
                        | LockReason::Idle
                        | LockReason::Background
                        | LockReason::Shutdown
                );
                let wiped = releases && !self.model.drop_unsaved_at_lock().is_empty();
                self.model.busy = None;
                // What the index holds for the store goes with it, and so does a
                // settlement's banner that waited for a page of the store
                // (docs/DECISIONS.md D38).
                self.model.settled_waiting.clear();
                // The polls of the store close with it: an unlock starts from
                // its own reads (docs/DECISIONS.md D38 item 2).
                self.model.watch.clear();
                self.model.activity = Explored::default();
                // With no store open, the lock dropped a recovery phrase
                // waiting to be confirmed (wallet-core's `Event::Locked`):
                // nothing was written, so the way on is to create again,
                // not to unlock a store that is not there.
                if dir.is_none()
                    && !matches!(reason, LockReason::Replaced | LockReason::ReopenFailed)
                {
                    let why = match reason {
                        LockReason::Idle => format!(
                            "after {} with nothing done",
                            minutes(self.model.prefs.idle_lock)
                        ),
                        LockReason::Background => "when the application left the screen".to_owned(),
                        _ => "when the wallet locked".to_owned(),
                    };
                    self.model.back_to_new_wallet(Some(phrase_discarded(&why)));
                    return Task::none();
                }
                let mut note = match reason {
                    LockReason::Idle => Some(format!(
                        "Locked after {} with nothing done. The store is closed and its key \
                         is gone from memory; unlock it to go on.",
                        minutes(self.model.prefs.idle_lock)
                    )),
                    LockReason::Background => {
                        Some("Locked when the application left the screen.".to_owned())
                    }
                    LockReason::ReopenFailed => Some(
                        "The store could not be read back after the last operation, so it \
                         was closed. Unlock it to see its state."
                            .to_owned(),
                    ),
                    LockReason::Asked | LockReason::Replaced | LockReason::Shutdown => None,
                };
                if wiped {
                    let gone = "Signed bytes that were not saved were wiped when the wallet \
                                locked. Nothing is written to disk, so they are gone: if the \
                                spend has not landed, its account's page offers Re-sign, which \
                                takes the same destinations, amounts, references, fee and \
                                block-to-live typed again.";
                    note = Some(match note {
                        Some(n) => format!("{n} {gone}"),
                        None => gone.to_owned(),
                    });
                }
                let exists = dir.is_some()
                    || self
                        .model
                        .default_dir
                        .as_deref()
                        .is_some_and(tawara_wallet_core::store_exists);
                if !matches!(reason, LockReason::Replaced) {
                    // A lock asked for while a spend was being signed runs
                    // after it, so its sent page may only just have been
                    // shown: it is kept, not closed with the screen.
                    if self.model.keep_signed(true) {
                        let kept = "A signed spend's page was open when the wallet locked. It \
                                    is kept, with the signed bytes, and shown again when this \
                                    store is unlocked.";
                        note = Some(match note {
                            Some(n) => format!("{n} {kept}"),
                            None => kept.to_owned(),
                        });
                    }
                    if exists || note.is_some() {
                        self.model.unlock_screen(dir, None, note);
                    } else {
                        let _ = self.model.leave();
                        self.model.screen = Screen::Start {
                            choice: StartChoice::Create,
                        };
                    }
                }
            }
            Event::Backgrounded { seq } => {
                self.model.backgrounded();
                // Taken in turn: every answer before it, a recovery phrase
                // among them, and the `Locked` that dropped it, have been
                // too. The shell may let the process be suspended.
                self.model.wiped.taken(seq);
            }
            Event::Stopped { panicked } => {
                self.stop(panicked);
                // Nothing more comes from the worker, and what it held was
                // dropped before this was sent.
                self.model.wiped.taken(u64::MAX);
            }
        }
        Task::none()
    }

    /// The worker has stopped (`panicked` when it faulted). Nothing more
    /// can be done in this run, and what the screens held goes with it:
    /// the open store's view, and the secret a form or the phrase screens
    /// held, zeroized. With no worker left to lock on idle, nothing secret
    /// would otherwise go before the application closed.
    fn stop(&mut self, panicked: bool) {
        let m = &mut self.model;
        m.stopped = Some(panicked || m.stopped == Some(true));
        drop(m.leave());
        m.busy = None;
        m.wallet = None;
        m.watch.clear();
        self.worker = None;
    }

    /// The worker's answer to a command. A saved spend kept for a reservation
    /// the answer shows settled is forgotten after it, and unsaved bytes are
    /// kept (see [`Model::signed`]).
    /// An account the worker settled on its own in answering has its banner
    /// shown once the answer has taken its page (docs/DECISIONS.md D38): on
    /// the account's page if that is the one shown, and otherwise when it is
    /// next opened. A refresh or an unlock shows the dashboard, and a send
    /// shows its sent page, so for those the banner waits.
    fn on_reply(&mut self, purpose: Purpose, reply: Reply) -> Task<Message> {
        let settled = settled_in(&reply);
        // A spend signed now holds a reservation, whatever its account's polls
        // were before (docs/DECISIONS.md D38 item 2; `Watch::signed`). The reads
        // the reply carries are noted below, as every reply's are.
        if let Reply::Sent(sent) = &reply {
            self.model.watch.signed(sent.from);
        }
        let task = if purpose.wallet_page() {
            self.on_wallet_reply(purpose, reply)
        } else {
            self.answer(purpose, reply);
            Task::none()
        };
        for settled in settled {
            self.model.show_settled(settled);
        }
        self.model.forget_settled();
        // The reads the answer carries are noted here, whatever page sent the
        // command, so that the next poll asks about what they left
        // (docs/DECISIONS.md D38 item 2).
        if let Some(view) = &self.model.wallet {
            self.model.watch.read_view(view);
        }
        task
    }

    /// An answer to a command a wallet page did not send.
    fn answer(&mut self, purpose: Purpose, reply: Reply) {
        let m = &mut self.model;
        match (purpose, reply) {
            (Purpose::SetNode { form }, Reply::NodeSet { url, view }) => {
                // Another node: another index and another chain's blocks.
                if m.node.url.as_deref() != Some(url.as_str()) {
                    m.activity = Explored::default();
                    m.blocks = Explored::default();
                    m.mempool = Explored::default();
                    m.chain = Explored::default();
                    m.pending = Explored::default();
                }
                m.node = NodeState {
                    url: Some(url.clone()),
                    ..NodeState::default()
                };
                m.prefs.node = Some(url);
                if let Some(v) = view {
                    m.wallet = Some(v);
                }
                if form && let Screen::Node(f) = &m.screen {
                    let back = f.back;
                    self.go_back(back);
                }
                self.save_prefs();
                self.ask_tip();
                self.read_activity();
            }
            // From the node screen, or from Settings' node card.
            (Purpose::SetNode { form: true }, Reply::Refused(r)) => {
                if let Screen::Node(f) = &mut m.screen {
                    f.error = Some(r.text);
                } else {
                    self.refused(r);
                }
            }
            // A refusal (no node, a cancel) leaves what was shown.
            (Purpose::Blocks, reply) => {
                m.blocks.reading = false;
                if let Reply::Blocks(read) = reply {
                    m.blocks.last = Some(read);
                }
            }
            (Purpose::Mempool, reply) => {
                m.mempool.reading = false;
                if let Reply::Mempool(read) = reply {
                    m.mempool.last = Some(read);
                }
            }
            (Purpose::Chain, reply) => {
                m.chain.reading = false;
                if let Reply::Chain(read) = reply {
                    m.chain.last = Some(read);
                    m.chain.read_ms = Some(m.clock_ms);
                }
            }
            (Purpose::Pending, reply) => {
                m.pending.reading = false;
                if let Reply::Pending(read) = reply {
                    m.pending.last = Some(read);
                    m.pending.read_ms = Some(m.clock_ms);
                }
            }
            (Purpose::Activity, reply) => {
                m.activity.reading = false;
                if let Reply::Activity(read) = reply {
                    m.activity.last = Some(read);
                }
            }
            // A page is added below the rows already read for its account;
            // a refusal leaves those rows as they were.
            (Purpose::OlderActivity, reply) => {
                m.activity.reading = false;
                if let Reply::Activity(Ok(pages)) = reply
                    && let Some(Ok(histories)) = &mut m.activity.last
                {
                    for page in pages {
                        if let Some(h) = histories.iter_mut().find(|h| h.account == page.account) {
                            h.extend(page);
                        }
                    }
                }
            }
            (Purpose::Networks, Reply::Networks(names)) => {
                m.node.network = names.into_iter().next();
            }
            (Purpose::SetNode { form: false }, Reply::Refused(r)) => {
                // A remembered node this version will not use: forget it, and
                // say why where the node is shown.
                m.node = NodeState {
                    error: Some(r.text),
                    ..NodeState::default()
                };
                m.prefs.node = None;
                self.save_prefs();
            }
            (Purpose::ClearNode, Reply::NodeCleared { view }) => {
                m.node = NodeState::default();
                m.activity = Explored::default();
                m.blocks = Explored::default();
                m.mempool = Explored::default();
                m.chain = Explored::default();
                m.pending = Explored::default();
                m.prefs.node = None;
                if let Some(v) = view {
                    m.wallet = Some(v);
                }
                if let Screen::Node(f) = &mut m.screen {
                    f.url.clear();
                }
                self.save_prefs();
            }
            (
                Purpose::Network { sent },
                Reply::Network {
                    tip_index,
                    tip_time_ms,
                    sync,
                    ..
                },
            ) => {
                m.node.tip = Some((tip_index, sent.elapsed()));
                m.node.solved_ms = Some(tip_time_ms);
                m.node.sync = sync;
                m.node.error = None;
            }
            (Purpose::Network { .. }, Reply::Refused(r)) => {
                m.node.tip = None;
                m.node.solved_ms = None;
                m.node.sync = None;
                m.node.error = Some(r.text);
            }
            (
                Purpose::CreateBegin,
                Reply::CreatePhrase {
                    phrase,
                    confirm_positions,
                },
            ) => {
                let dir = match m.leave() {
                    Screen::NewWallet(f) => f.dir,
                    _ => m.default_dir_text(),
                };
                m.screen = Screen::Phrase(PhraseState {
                    phrase,
                    dir,
                    positions: confirm_positions,
                    written: false,
                    words: Default::default(),
                    error: None,
                });
            }
            (
                Purpose::CreateConfirm | Purpose::CreateFromPhrase,
                Reply::Created { dir, opened, .. },
            ) => match opened {
                Ok(view) => self.opened(view),
                // The store was written; it could not be opened (another
                // process took its lock first, say). Unlocking the store
                // just made shows why, and nothing was lost; it is
                // remembered as if it had opened.
                Err(r) => {
                    self.remember(&dir);
                    let dir = Some(dir.display().to_string());
                    self.model.unlock_screen(dir, Some(r.text), None);
                }
            },
            (Purpose::Unlock, Reply::Unlocked(view)) => {
                self.opened(view);
                self.model.bring_back_signed();
            }
            (Purpose::Refresh, Reply::Wallet(view)) => self.opened(view),
            // The tip, once, after the refresh, however it went.
            (Purpose::Refresh, Reply::Refused(r)) => {
                self.refused(r);
                self.ask_tip();
            }
            // The worker holds no phrase to confirm, so the one on the
            // screen can no longer make a store: drop it, and back to S3 for
            // a new one. Every other refusal of the words leaves the phrase
            // waiting in the worker, and S5 shows it.
            (Purpose::CreateConfirm, Reply::Refused(r))
                if r.kind == RefusalKind::NothingToConfirm =>
            {
                self.model.back_to_new_wallet(None);
                if let Screen::NewWallet(f) = &mut self.model.screen {
                    f.error = Some(r.text);
                }
            }
            (_, Reply::Refused(r)) => self.refused(r),
            _ => {}
        }
    }

    fn go_back(&mut self, back: Back) {
        let m = &mut self.model;
        match back {
            Back::Start => {
                m.screen = Screen::Start {
                    choice: StartChoice::Create,
                };
            }
            Back::Unlock => m.unlock_screen(None, None, None),
            Back::Wallet => {
                m.screen = if m.wallet.is_some() {
                    Screen::Wallet(WalletPage::default())
                } else {
                    Screen::Start {
                        choice: StartChoice::Create,
                    }
                };
            }
        }
    }

    fn opened(&mut self, view: WalletView) {
        self.remember(&view.dir);
        let m = &mut self.model;
        let _ = m.leave();
        m.wallet = Some(view);
        m.screen = Screen::Wallet(WalletPage::default());
        self.ask_tip();
        self.read_activity();
    }

    /// Remember the store in `dir`, so the next start offers to unlock it
    /// wherever it is (docs/DECISIONS.md D27, item 10), and from wherever
    /// the application is started: the commands carry absolute folders, and
    /// this makes sure of it.
    fn remember(&mut self, dir: &Path) {
        let dir = std::path::absolute(dir)
            .unwrap_or_else(|_| dir.to_path_buf())
            .display()
            .to_string();
        if self.model.prefs.store.as_deref() != Some(dir.as_str()) {
            self.model.prefs.store = Some(dir);
            self.save_prefs();
        }
    }

    /// A refusal, shown where the command was sent from.
    fn refused(&mut self, r: Refusal) {
        let text = Some(r.text);
        match &mut self.model.screen {
            Screen::NewWallet(f) => f.error = text,
            Screen::Restore(f) => f.form.error = text,
            Screen::Unlock(f) => f.error = text,
            Screen::Confirm(p) => p.error = text,
            Screen::Node(f) => f.error = text,
            Screen::Wallet(p) => p.error = text,
            Screen::Start { .. } | Screen::Phrase(_) => {}
        }
    }
}

/// `period` in words, as the auto-lock choices are whole minutes.
#[must_use]
pub fn minutes(period: Duration) -> String {
    match period.as_secs() / 60 {
        1 => "1 minute".to_owned(),
        n => format!("{n} minutes"),
    }
}

/// The person did something: a key, a click, a scroll, a touch (pointer
/// movement alone is not counted); or the window opened or changed size. Tab and Shift-Tab move between the fields
/// as well, unless a widget took the key for itself.
fn person_input(event: IcedEvent, status: event::Status, _: iced::window::Id) -> Option<Message> {
    match event {
        IcedEvent::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Tab),
            modifiers,
            ..
        }) if status == event::Status::Ignored => Some(if modifiers.shift() {
            Message::FocusPrevious
        } else {
            Message::FocusNext
        }),
        // Android delivers Back as this key (docs/spikes/P1-REPORT.md).
        IcedEvent::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::BrowserBack),
            ..
        }) => Some(Message::SystemBack),
        IcedEvent::Keyboard(keyboard::Event::KeyPressed { .. })
        | IcedEvent::Mouse(mouse::Event::ButtonPressed(_) | mouse::Event::WheelScrolled { .. })
        | IcedEvent::Touch(touch::Event::FingerPressed { .. }) => Some(Message::Input),
        IcedEvent::Window(
            iced::window::Event::Opened { size, .. } | iced::window::Event::Resized(size),
        ) => Some(Message::Width(size.width)),
        // Whether the window is in the foreground, for the poll of a live
        // reservation (docs/DECISIONS.md D38 item 2). Not the person's input.
        IcedEvent::Window(iced::window::Event::Focused) => Some(Message::Foreground(true)),
        IcedEvent::Window(iced::window::Event::Unfocused) => Some(Message::Foreground(false)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::{Driver, mouse_move, mouse_press, mouse_release};
    use iced::advanced::widget::Operation;
    use iced::advanced::widget::operation::scrollable::{RelativeOffset, snap_to};
    use iced::futures::StreamExt;
    use iced::{Point, Rectangle};
    use iced_runtime::Action;
    use std::sync::mpsc::Receiver;

    /// The BIP39 all-`abandon` test vector: public, never funded.
    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon \
                          abandon abandon abandon abandon abandon abandon abandon abandon \
                          abandon abandon abandon abandon abandon abandon abandon art";
    const PASSWORD: &str = "correct horse battery";

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!(
                "tawara-app-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// An application over a real worker, no node chosen, with its default
    /// store in `scratch`.
    fn app(scratch: &Scratch) -> (App, Receiver<Event>) {
        let model = Model::new(Preferences::default(), Some(scratch.0.join("keystore")));
        let (app, events) = App::start(model, None);
        (app, events.expect("the worker starts"))
    }

    /// Hand the worker's events to the application until `done` says so.
    fn pump(app: &mut App, events: &Receiver<Event>, done: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(120);
        while !done(app) {
            let left = deadline.saturating_duration_since(Instant::now());
            let event = events.recv_timeout(left).expect("an event from the worker");
            let _ = app.update(Message::Worker(WorkerEvent::new(event)));
        }
    }

    /// Typed into a field drawn before any move to the background.
    fn typed(s: &str) -> Typed {
        Typed {
            text: s.to_owned(),
            drawn: 0,
        }
    }

    /// Typed into a field drawn before the latest move to the background
    /// that `app` has acted on: one generation behind.
    fn typed_now_before(app: &App, s: &str) -> Typed {
        Typed {
            text: s.to_owned(),
            drawn: app.model.drawn() - 1,
        }
    }

    /// Typed into a field as `app` draws it now.
    fn typed_now(app: &App, s: &str) -> Typed {
        Typed {
            text: s.to_owned(),
            drawn: app.model.drawn(),
        }
    }

    #[test]
    fn a_new_wallet_is_made_from_its_confirmed_phrase_and_opens() {
        let scratch = Scratch::new("create");
        let (mut app, events) = app(&scratch);
        assert!(matches!(app.model.screen, Screen::Start { .. }));
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::CreateWallet);
        match &app.model.screen {
            Screen::NewWallet(f) => assert!(f.password.is_empty() && f.again.is_empty()),
            other => panic!("expected the form, emptied, got {other:?}"),
        }
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Phrase(_))
        });
        let words: Vec<String> = match &app.model.screen {
            Screen::Phrase(p) => p.phrase.words().map(str::to_owned).collect(),
            _ => unreachable!(),
        };
        assert_eq!(words.len(), 24);

        // Continuing needs the person to say they wrote it down.
        let _ = app.update(Message::ToConfirm);
        assert!(matches!(app.model.screen, Screen::Phrase(_)));
        let _ = app.update(Message::Written(true));
        let _ = app.update(Message::ToConfirm);
        let positions = match &app.model.screen {
            Screen::Confirm(p) => p.positions,
            other => panic!("expected the confirmation, got {other:?}"),
        };

        // A wrong word keeps the phrase pending, and says so.
        for i in 0..3 {
            let _ = app.update(Message::Word(i, typed("wrong")));
        }
        let _ = app.update(Message::ConfirmWords);
        pump(
            &mut app,
            &events,
            |a| matches!(&a.model.screen, Screen::Confirm(p) if p.error.is_some()),
        );

        for (i, position) in positions.iter().enumerate() {
            let _ = app.update(Message::Word(i, typed(&words[position - 1])));
        }
        let _ = app.update(Message::ConfirmWords);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
        let view = app.model.wallet.as_ref().expect("the store is open");
        assert_eq!(view.accounts.len(), 1);
        assert!(!view.opened, "no node is chosen: {view:?}");
        assert!(
            view.notice
                .as_deref()
                .is_some_and(|n| n.contains("No node is chosen")),
            "{view:?}"
        );
    }

    #[test]
    fn a_restored_wallet_locks_and_unlocks_only_with_its_password() {
        let scratch = Scratch::new("restore");
        let (mut app, events) = app(&scratch);
        // Not the default folder: the application has to remember where it
        // is to offer it again.
        let elsewhere = scratch.0.join("elsewhere").join("keystore");
        let _ = app.update(Message::Choose(StartChoice::Restore));
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Dir(elsewhere.display().to_string()));
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::PhraseText(typed(PHRASE)));
        let _ = app.update(Message::RestoreWallet);
        match &app.model.screen {
            Screen::Restore(r) => assert!(r.phrase.is_empty() && r.form.password.is_empty()),
            other => panic!("expected the form, emptied, got {other:?}"),
        }
        // Waited on from the moment it is sent, before the worker has said
        // anything: the form gives way to S8 even while an earlier command
        // keeps the worker.
        assert!(
            app.model
                .busy
                .as_ref()
                .is_some_and(|b| b.activity.is_none()),
            "{:?}",
            app.model.busy
        );
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
        assert!(app.model.busy.is_none(), "{:?}", app.model.busy);
        let remembered = elsewhere.display().to_string();
        assert_eq!(app.model.prefs.store.as_deref(), Some(remembered.as_str()));

        // A second Refresh while the first is waited on sends nothing: the
        // page shows the first, with its Cancel, until it answers. The tip
        // is asked for once, when the refresh has answered, and nothing is
        // queued behind it. (The application is told a node is chosen so
        // that it asks; the worker has none and refuses, which does not
        // change the count.)
        app.model.node.url = Some("https://node.example".to_owned());
        let _ = app.update(Message::Refresh);
        let first = app.model.busy.as_ref().map(|b| b.id);
        assert!(first.is_some());
        let _ = app.update(Message::Refresh);
        assert_eq!(app.model.busy.as_ref().map(|b| b.id), first);
        let waiting = |a: &App, which: fn(&Purpose) -> bool| {
            a.worker
                .as_ref()
                .map_or(0, |w| w.waiting.iter().filter(|(_, p)| which(p)).count())
        };
        let refreshes = |p: &Purpose| matches!(p, Purpose::Refresh);
        let tips = |p: &Purpose| matches!(p, Purpose::Network { .. });
        assert_eq!(waiting(&app, refreshes), 1);
        assert_eq!(waiting(&app, tips), 0, "nothing queued behind the refresh");
        pump(&mut app, &events, |a| a.model.busy.is_none());
        assert_eq!(waiting(&app, tips), 1, "one tip, once the refresh answered");
        pump(&mut app, &events, |a| waiting(a, tips) == 0);
        app.model.node = NodeState::default();

        let _ = app.update(Message::Lock);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Unlock(_))
        });
        assert!(app.model.wallet.is_none(), "nothing of the store is kept");
        assert!(
            matches!(&app.model.screen, Screen::Unlock(u) if u.dir == remembered),
            "{:?}",
            app.model.screen
        );

        // At the next start, with nothing in the default folder, the store
        // last opened is the one offered; and from the start screen, opening
        // an existing wallet leads to it too.
        let next = Model::new(app.model.prefs.clone(), app.model.default_dir.clone());
        assert!(
            matches!(&next.screen, Screen::Unlock(u) if u.dir == remembered),
            "{:?}",
            next.screen
        );
        let _ = app.update(Message::Go(Go::Start));
        let _ = app.update(Message::Go(Go::Unlock));
        assert!(
            matches!(&app.model.screen, Screen::Unlock(u) if u.dir == remembered),
            "{:?}",
            app.model.screen
        );

        let _ = app.update(Message::Password(typed("not the password at all")));
        let _ = app.update(Message::UnlockWallet);
        pump(
            &mut app,
            &events,
            |a| matches!(&a.model.screen, Screen::Unlock(u) if u.error.is_some()),
        );

        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::UnlockWallet);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
        assert_eq!(app.model.wallet.as_ref().map(|w| w.accounts.len()), Some(1));
    }

    /// Apply the worker's events up to and including `Backgrounded`.
    fn through_backgrounded(app: &mut App, events: &Receiver<Event>) {
        loop {
            let event = events
                .recv_timeout(Duration::from_secs(30))
                .expect("Backgrounded from the worker");
            let last = matches!(event, Event::Backgrounded { .. });
            let _ = app.update(Message::Worker(WorkerEvent::new(event)));
            if last {
                return;
            }
        }
    }

    #[test]
    fn leaving_the_foreground_wipes_what_is_typed_even_with_nothing_open() {
        // PR #13's review: with the store already locked, the worker has
        // nothing to lock and sends no `Locked`, so a password typed on S7
        // was kept, and Unlock opened the wallet after the return.
        let scratch = Scratch::new("background-typed");
        let (mut app, events) = app(&scratch);
        let dir = scratch.0.join("keystore");
        let _ = app.update(Message::Choose(StartChoice::Restore));
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Dir(dir.display().to_string()));
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::PhraseText(typed(PHRASE)));
        let _ = app.update(Message::RestoreWallet);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
        let _ = app.update(Message::Lock);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Unlock(_))
        });

        // S7: the right password typed, not submitted.
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.worker.as_ref().expect("a worker").handle.background();
        through_backgrounded(&mut app, &events);
        match &app.model.screen {
            Screen::Unlock(u) => assert!(u.password.is_empty(), "the password was kept"),
            other => panic!("expected the unlock screen, got {other:?}"),
        }
        // Unlock now asks for the password again rather than opening.
        let _ = app.update(Message::UnlockWallet);
        pump(
            &mut app,
            &events,
            |a| !matches!(&a.model.screen, Screen::Unlock(u) if u.error.is_none()),
        );
        assert!(
            matches!(&app.model.screen, Screen::Unlock(u) if u.error.is_some()),
            "{:?}",
            app.model.screen
        );

        // S6: a phrase and passwords typed, not submitted, are wiped where
        // they are; the screen stays.
        let _ = app.update(Message::Go(Go::Start));
        let _ = app.update(Message::Choose(StartChoice::Restore));
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::PhraseText(typed(PHRASE)));
        let _ = app.worker.as_ref().expect("a worker").handle.background();
        through_backgrounded(&mut app, &events);
        match &app.model.screen {
            Screen::Restore(r) => {
                assert!(r.phrase.is_empty(), "the recovery phrase was kept");
                assert!(r.form.password.is_empty() && r.form.again.is_empty());
            }
            other => panic!("expected the restore screen, got {other:?}"),
        }
    }

    #[test]
    fn what_is_typed_is_wiped_before_the_next_input_whatever_the_worker_is_doing() {
        // PR #13's review: the worker's `Backgrounded` can come late, behind
        // a command still running. If the person returns and presses
        // Unlock first, the password typed before must not reach the worker.
        let scratch = Scratch::new("background-race");
        let (mut app, events) = app(&scratch);
        let dir = scratch.0.join("keystore");
        let _ = app.update(Message::Choose(StartChoice::Restore));
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Dir(dir.display().to_string()));
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::PhraseText(typed(PHRASE)));
        let _ = app.update(Message::RestoreWallet);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
        let _ = app.update(Message::Lock);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Unlock(_))
        });
        let _ = app.update(Message::Password(typed(PASSWORD)));

        // The shell says the application left, through what `boot_in`
        // registers; the worker's answer is not taken (it is held up), and
        // Unlock is pressed on the return.
        let _ = (app.leaving_hook())();
        let _ = app.update(Message::UnlockWallet);
        pump(
            &mut app,
            &events,
            |a| !matches!(&a.model.screen, Screen::Unlock(u) if u.error.is_none()),
        );
        assert!(
            matches!(&app.model.screen, Screen::Unlock(u) if u.error.is_some()),
            "the wallet opened with the password typed before leaving: {:?}",
            app.model.screen
        );

        // PR #13's third review: iced takes a batch of messages against the
        // fields as drawn before it draws them again. The first message of
        // the batch wipes; every secret's text after it, from the fields
        // drawn before leaving, is refused, however many there are, and an
        // Unlock in the same batch finds the field empty.
        let _ = app.update(Message::Password(typed_now(&app, PASSWORD)));
        let _ = (app.leaving_hook())();
        let _ = app.update(Message::Input);
        let _ = app.update(Message::Password(typed_now_before(
            &app,
            &format!("{PASSWORD}x"),
        )));
        let _ = app.update(Message::Password(typed_now_before(&app, PASSWORD)));
        match &app.model.screen {
            Screen::Unlock(u) => assert!(u.password.is_empty(), "stale text was taken"),
            other => panic!("expected the unlock screen, got {other:?}"),
        }
        let _ = app.update(Message::UnlockWallet);
        pump(&mut app, &events, |a| {
            matches!(&a.model.screen, Screen::Unlock(u) if u.error.is_some())
                || matches!(a.model.screen, Screen::Wallet(_))
        });
        assert!(
            matches!(&app.model.screen, Screen::Unlock(u) if u.error.is_some()),
            "{:?}",
            app.model.screen
        );

        // Typed into the fields as drawn again: taken as usual.
        let _ = app.update(Message::Password(typed_now(&app, PASSWORD)));
        assert!(matches!(&app.model.screen, Screen::Unlock(u) if u.password == PASSWORD));
    }

    #[test]
    fn a_shell_hears_the_move_done_only_once_the_interface_has_wiped() {
        // Tawara-mobile#1's second review: the worker's lock is not all a
        // shell must wait for before the system may suspend the process. A
        // recovery phrase shown, or a password typed, is the interface's.
        let scratch = Scratch::new("background-ack");
        let (mut app, events) = app(&scratch);
        let chosen = scratch.0.join("chosen").display().to_string();
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Dir(chosen.clone()));
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::CreateWallet);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Phrase(_))
        });

        let leaving = (app.leaving_hook())();
        // The worker locks on its own; the interface has taken nothing yet.
        assert!(
            !leaving.wait(Duration::from_millis(500)),
            "reported done while the interface still showed the phrase"
        );
        // The clock's next tick drops the phrase from the screen at once,
        // but the move is done only when the interface has taken the
        // worker's `Backgrounded` for it (Tawara-mobile#1's third review).
        let _ = app.update(Message::Tick(now_ms()));
        assert!(matches!(app.model.screen, Screen::NewWallet(_)));
        assert!(
            !leaving.wait(Duration::from_millis(300)),
            "reported done before the worker's events were taken"
        );
        through_backgrounded(&mut app, &events);
        assert!(leaving.wait(Duration::ZERO));
        let s3 = |app: &App| match &app.model.screen {
            Screen::NewWallet(f) => {
                assert_eq!(f.dir, chosen, "the folder chosen stays");
                assert!(
                    f.note
                        .as_deref()
                        .is_some_and(|n| n.contains("left the screen")),
                    "{:?}",
                    f.note
                );
            }
            other => panic!("expected S3, the phrase gone, got {other:?}"),
        };
        // The worker's own `Locked`, for the phrase it dropped, changed
        // nothing more.
        s3(&app);
    }

    #[test]
    fn a_stopped_worker_lets_the_shell_go() {
        // No `Backgrounded` comes from a worker that has stopped; what it
        // held was dropped before its `Stopped`, so a move is done once the
        // interface has taken that.
        let scratch = Scratch::new("background-stopped");
        let (mut app, events) = app(&scratch);
        // Made while the worker runs, as `boot_in` registers it.
        let hook = app.leaving_hook();
        app.worker.as_ref().expect("a worker").handle.shutdown();
        pump(&mut app, &events, |a| a.model.stopped.is_some());
        let leaving = hook();
        assert!(leaving.wait(Duration::from_secs(5)));
    }

    #[test]
    fn a_phrase_answered_after_the_move_is_dropped_before_the_move_is_done() {
        // Tawara-mobile#1's third review: the app leaves while the worker is
        // still making a recovery phrase. The interface wipes its fields at
        // once, but the phrase arrives after that, and must be gone before
        // the shell lets the process be suspended.
        let scratch = Scratch::new("background-phrase");
        let (mut app, events) = app(&scratch);
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::CreateWallet);
        // The phrase is made, and its answer is on its way to the interface,
        // not yet taken, when the app leaves: held here until then. (Leaving
        // before the worker starts on it would cancel it, and no phrase
        // would come at all.)
        let mut held = Vec::new();
        loop {
            let event = events
                .recv_timeout(Duration::from_secs(30))
                .expect("the phrase's answer");
            let answered = matches!(event, Event::Done { .. });
            held.push(event);
            if answered {
                break;
            }
        }
        let leaving = (app.leaving_hook())();
        let _ = app.update(Message::Tick(now_ms()));
        assert!(
            !leaving.wait(Duration::from_millis(500)),
            "done while the phrase's answer was still to be taken"
        );
        // Taken in turn: the phrase is shown, the lock drops it, and only
        // then is the move done.
        let mut shown = false;
        let mut take = |app: &mut App, event: Event| {
            let last = matches!(event, Event::Backgrounded { .. });
            let _ = app.update(Message::Worker(WorkerEvent::new(event)));
            shown |= matches!(app.model.screen, Screen::Phrase(_));
            if !last {
                assert!(!leaving.wait(Duration::ZERO), "done before Backgrounded");
            }
            last
        };
        for event in held {
            assert!(!take(&mut app, event));
        }
        while !take(
            &mut app,
            events
                .recv_timeout(Duration::from_secs(30))
                .expect("events from the worker"),
        ) {}
        assert!(
            shown,
            "the phrase was shown after the move, as in the review"
        );
        assert!(leaving.wait(Duration::ZERO));
        assert!(
            matches!(&app.model.screen, Screen::NewWallet(f)
                if f.note.as_deref().is_some_and(|n| n.contains("left the screen"))),
            "{:?}",
            app.model.screen
        );
    }

    #[test]
    fn a_folder_is_within_the_root_only_as_the_file_system_resolves_it() {
        let scratch = Scratch::new("within");
        let root = scratch.0.join("root");
        let other = scratch.0.join("other");
        std::fs::create_dir_all(root.join("there")).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        assert!(
            within(&root.join("keystore"), &root),
            "a store not yet made"
        );
        assert!(within(&root.join("there/savings"), &root));
        assert!(
            !within(&root, &root),
            "the folder itself holds the preferences"
        );
        assert!(!within(&other.join("keystore"), &root), "a sibling");
        assert!(!within(&root.join("../other/keystore"), &root), "out by ..");
        assert!(
            !within(&root.join("missing/../../other"), &root),
            "out by .. after a folder that does not exist"
        );
        assert!(!within(Path::new(""), &root));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&other, root.join("link")).unwrap();
            assert!(!within(&root.join("link/keystore"), &root), "out by a link");
        }
    }

    #[test]
    fn a_hosted_application_confines_stores_to_its_private_folder() {
        let scratch = Scratch::new("hosted");
        let (app, _) = App::hosted(&scratch.0);
        assert_eq!(app.model.confined.as_deref(), Some(scratch.0.as_path()));
        // The folder offered by default is one a store may be made in.
        let offered = app.model.default_dir_text();
        assert!(within(&store_dir(&offered), &scratch.0), "{offered}");
    }

    #[test]
    fn the_phone_layout_is_drawn_below_600_and_a_new_model_is_not_compact() {
        let mut model = Model::new(Preferences::default(), None);
        assert!(
            !model.compact(),
            "a new model starts at the desktop's width"
        );
        model.width = 599.9;
        assert!(model.compact());
        model.width = 600.0;
        assert!(!model.compact());
    }

    #[test]
    fn a_phone_is_compact_before_the_window_reports_its_size() {
        let scratch = Scratch::new("hosted-width");
        let (app, _) = App::hosted(&scratch.0);
        assert!(app.model.compact(), "the first frames are the phone layout");
        assert_eq!(app.model.width, NARROWEST);
        assert!(app.model.phone());
        assert!(!alone().model.phone(), "the desktop is not confined");
    }

    #[test]
    fn the_safe_area_is_stored_and_is_not_the_persons_input() {
        let area = iced::mobile::SafeArea::new(iced::Padding {
            top: 47.0,
            right: 0.0,
            bottom: 34.0,
            left: 0.0,
        })
        .with_keyboard(336.0);
        let mut app = alone();
        let _ = app.update(Message::SafeArea(area));
        assert_eq!(app.model.safe_area, area);

        // A safe area does not start the idle period again, and the person's
        // input does: counted where the worker is told, not by the wall clock.
        let touches = app.touches.get();
        let _ = app.update(Message::SafeArea(iced::mobile::SafeArea::ZERO));
        assert_eq!(app.touches.get(), touches, "the safe area is not input");
        let _ = app.update(Message::Input);
        assert_eq!(app.touches.get(), touches + 1, "the person's input is");

        // And with a worker: a phrase left alone is dropped by the idle lock,
        // however many safe areas come meanwhile. The lock is waited for, up
        // to ten seconds, rather than timed, so a slow machine only makes the
        // wait longer.
        let scratch = Scratch::new("safe-area-idle");
        let prefs = Preferences {
            idle_lock: Duration::from_millis(400),
            ..Preferences::default()
        };
        let model = Model::new(prefs, Some(scratch.0.join("keystore")));
        let (mut app, events) = App::start(model, None);
        let events = events.expect("the worker starts");
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::CreateWallet);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Phrase(_))
        });
        let started = Instant::now();
        while matches!(app.model.screen, Screen::Phrase(_))
            && started.elapsed() < Duration::from_secs(10)
        {
            let _ = app.update(Message::SafeArea(area));
            std::thread::sleep(Duration::from_millis(50));
            while let Ok(event) = events.try_recv() {
                let _ = app.update(Message::Worker(WorkerEvent::new(event)));
            }
        }
        assert!(
            !matches!(app.model.screen, Screen::Phrase(_)),
            "the idle lock dropped the phrase: the safe area did not restart it"
        );
    }

    /// W4 from the sample wallet's first account, with nothing typed yet.
    fn composing() -> Page {
        Page::Send(SendPage {
            from: Some(tawara_wallet_core::sample::wallet_view().accounts[0].id),
            form: SpendForm::default(),
            stage: SendStage::Compose,
        })
    }

    /// What W4's Paste button beside row `row` asks for, as `app` shows it.
    fn destination(app: &App, row: usize) -> PasteInto {
        let (form, rows) = app.model.form_of().expect("a spend form is shown");
        PasteInto::Destination { form, row, rows }
    }

    #[test]
    fn a_pasted_destination_is_trimmed_into_its_row() {
        // docs/DECISIONS.md D32 item 10 (g): the line break a copy brings is not
        // part of the destination.
        let mut app = on_wallet(composing());
        let into = destination(&app, 0);
        let _ = app.update(Message::Pasted(
            into,
            Some(Clip("  3vQB7B6MrGQZaxCuFg4oh\n".to_owned())),
        ));
        assert!(
            matches!(
                &wallet_page(&app).page,
                Page::Send(s) if s.form.rows[0].to == "3vQB7B6MrGQZaxCuFg4oh"
            ),
            "{:?}",
            wallet_page(&app).page
        );
    }

    #[test]
    fn pasted_signed_bytes_keep_their_hex_and_lose_their_whitespace() {
        // docs/DECISIONS.md D32 item 10 (g): hex copied in groups, or from a list
        // with line breaks, is the one run of bytes it is.
        let mut app = on_wallet(Page::Submit(SubmitPage::default()));
        let _ = app.update(Message::Pasted(
            PasteInto::SignedBytes,
            Some(Clip("4f2a 00ab\r\n\t11cd\n".to_owned())),
        ));
        assert!(
            matches!(&wallet_page(&app).page, Page::Submit(s) if s.hex == "4f2a00ab11cd"),
            "{:?}",
            wallet_page(&app).page
        );
    }

    #[test]
    fn a_refused_or_blank_read_changes_nothing() {
        // docs/DECISIONS.md D32 item 10 (g): an empty or refused read, or one of
        // whitespace alone, leaves its field as it is.
        let mut app = on_wallet(composing());
        let into = destination(&app, 0);
        for read in [None, Some(" \n ".to_owned())] {
            let _ = app.update(Message::Pasted(into, read.map(Clip)));
        }
        assert!(
            matches!(&wallet_page(&app).page, Page::Send(s) if s.form.rows[0].to.is_empty()),
            "{:?}",
            wallet_page(&app).page
        );

        let mut app = on_wallet(Page::Submit(SubmitPage::default()));
        for read in [None, Some(" \n\t".to_owned())] {
            let _ = app.update(Message::Pasted(PasteInto::SignedBytes, read.map(Clip)));
        }
        assert!(
            matches!(&wallet_page(&app).page, Page::Submit(s) if s.hex.is_empty()),
            "{:?}",
            wallet_page(&app).page
        );

        let mut app = alone();
        app.model.screen = Screen::Restore(RestoreForm {
            phrase: PHRASE.to_owned(),
            ..RestoreForm::default()
        });
        let drawn = app.model.drawn();
        for read in [None, Some(" \n\t ".to_owned())] {
            let _ = app.update(Message::Pasted(PasteInto::Phrase { drawn }, read.map(Clip)));
        }
        assert!(matches!(&app.model.screen, Screen::Restore(r) if r.phrase == PHRASE));
    }

    #[test]
    fn a_late_destination_read_goes_only_to_the_form_and_row_it_was_asked_for() {
        // docs/DECISIONS.md D32 item 10 (g): the read can come back after the
        // page has changed, for instance while iOS asks whether to allow the
        // paste. It is dropped then, not put in whatever form is shown.
        let read = || Some(Clip("3vQB7B6MrGQZaxCuFg4oh".to_owned()));
        let accounts = tawara_wallet_core::sample::wallet_view().accounts;
        let empty = |app: &App| match &wallet_page(app).page {
            Page::Send(s) => s.form.rows.iter().all(|r| r.to.is_empty()),
            Page::Resign(r) => r.form.rows.iter().all(|r| r.to.is_empty()),
            other => panic!("expected a spend form, got {other:?}"),
        };

        // Another account's form.
        let mut app = on_wallet(composing());
        let into = destination(&app, 0);
        if let Screen::Wallet(WalletPage {
            page: Page::Send(s),
            ..
        }) = &mut app.model.screen
        {
            s.from = Some(accounts[1].id);
        }
        let _ = app.update(Message::Pasted(into, read()));
        assert!(empty(&app), "another account's form");

        // W8's form, for the same account.
        let mut app = on_wallet(composing());
        let into = destination(&app, 0);
        app.model.screen = Screen::Wallet(WalletPage {
            page: Page::Resign(ResignPage {
                account: accounts[0].id,
                form: SpendForm::default(),
            }),
            ..WalletPage::default()
        });
        let _ = app.update(Message::Pasted(into, read()));
        assert!(empty(&app), "the re-sign form");

        // The same form, a row added or removed since.
        let mut app = on_wallet(composing());
        wallet(&mut app, WalletMsg::AddDestination);
        let into = destination(&app, 1);
        wallet(&mut app, WalletMsg::RemoveDestination(0));
        let _ = app.update(Message::Pasted(into, read()));
        assert!(empty(&app), "a row removed");

        // The page busy with the worker, as its fields are then.
        let mut app = on_wallet(composing());
        let into = destination(&app, 0);
        app.model.busy = Some(Busy {
            id: RequestId::example(),
            activity: None,
            progress: None,
        });
        let _ = app.update(Message::Pasted(into, read()));
        assert!(empty(&app), "the page busy");
        app.model.busy = None;
        let _ = app.update(Message::Pasted(into, read()));
        assert!(!empty(&app), "the form and row it was asked for take it");
    }

    #[test]
    fn a_late_signed_bytes_read_is_dropped_while_w9_is_busy() {
        let mut app = on_wallet(Page::Submit(SubmitPage::default()));
        app.model.busy = Some(Busy {
            id: RequestId::example(),
            activity: None,
            progress: None,
        });
        let _ = app.update(Message::Pasted(
            PasteInto::SignedBytes,
            Some(Clip("4f2a".to_owned())),
        ));
        assert!(
            matches!(&wallet_page(&app).page, Page::Submit(s) if s.hex.is_empty()),
            "{:?}",
            wallet_page(&app).page
        );
    }

    #[test]
    fn a_pasted_phrase_replaces_the_field_cleaned_of_its_line_breaks() {
        let mut app = alone();
        app.model.screen = Screen::Restore(RestoreForm::default());
        let drawn = app.model.drawn();
        let _ = app.update(Message::Pasted(
            PasteInto::Phrase { drawn },
            Some(Clip(PHRASE.replace(' ', "\n"))),
        ));
        assert!(matches!(&app.model.screen, Screen::Restore(r) if r.phrase == PHRASE));
    }

    #[test]
    fn a_pasted_phrase_read_after_a_move_away_is_refused() {
        // docs/DECISIONS.md D32 item 6: the read was asked for the field as it
        // was drawn, and the application left the foreground before it came
        // back. Once the field is drawn again, a paste into it is taken. The
        // refused text is wiped as well, which a test cannot observe.
        let mut app = alone();
        app.model.screen = Screen::Restore(RestoreForm::default());
        let drawn = app.model.drawn();
        app.model.left.fetch_add(1, Ordering::SeqCst);
        let _ = app.update(Message::Pasted(
            PasteInto::Phrase { drawn },
            Some(Clip(PHRASE.replace(' ', "\n"))),
        ));
        assert!(
            matches!(&app.model.screen, Screen::Restore(r) if r.phrase.is_empty()),
            "a read for the field as it was before the move is refused"
        );
        let drawn = app.model.drawn();
        let _ = app.update(Message::Pasted(
            PasteInto::Phrase { drawn },
            Some(Clip(PHRASE.replace(' ', "\n"))),
        ));
        assert!(matches!(&app.model.screen, Screen::Restore(r) if r.phrase == PHRASE));
    }

    #[test]
    fn the_phrase_sent_is_cleaned_as_a_paste_into_it_would_be_and_the_field_wiped() {
        // docs/DECISIONS.md D32 item 10 (h): RestoreWallet sends the phrase as a
        // paste into the field would have left it.
        let mut field = "abandon\nability\table  about \r\n".to_owned();
        assert_eq!(cleaned_phrase(&mut field), "abandon ability able about");
        assert!(field.is_empty());
    }

    #[test]
    fn a_pasted_read_is_never_in_a_message_log() {
        // docs/DECISIONS.md D32 item 6: a copied recovery phrase is a secret.
        let message = Message::Pasted(
            PasteInto::Phrase { drawn: 0 },
            Some(Clip("hunter2-hunter2".to_owned())),
        );
        assert!(!format!("{message:?}").contains("hunter2"));
    }

    #[test]
    fn the_keyboard_coming_up_on_a_phone_scrolls_to_the_focused_field() {
        // Only a rise of the keyboard in the compact layout: the operation then
        // runs over the page laid out above it (docs/DECISIONS.md D32 item 10
        // (b)).
        let with = |keyboard: f32| {
            iced::mobile::SafeArea::new(iced::Padding::ZERO).with_keyboard(keyboard)
        };
        let mut phone = alone();
        phone.model.width = NARROWEST;
        assert!(phone.update(Message::SafeArea(with(336.0))).units() > 0);
        assert_eq!(phone.update(Message::SafeArea(with(336.0))).units(), 0);
        assert_eq!(phone.update(Message::SafeArea(with(0.0))).units(), 0);
        let mut desktop = alone();
        assert_eq!(desktop.update(Message::SafeArea(with(336.0))).units(), 0);
    }

    #[test]
    fn a_phone_offers_no_remembered_folder_and_starts_on_the_first_screen() {
        // A folder remembered from an earlier run is not one a phone chooses,
        // and the default folder holds no store: the start screen.
        let scratch = Scratch::new("hosted-remembered");
        std::fs::create_dir_all(&scratch.0).unwrap();
        let elsewhere = scratch.0.join("elsewhere").join("keystore");
        let prefs = Preferences {
            store: Some(elsewhere.display().to_string()),
            ..Preferences::default()
        };
        preferences::save(&scratch.0.join(preferences::FILE_NAME), &prefs).unwrap();
        let (app, _) = App::hosted(&scratch.0);
        assert!(
            matches!(app.model.screen, Screen::Start { .. }),
            "{:?}",
            app.model.screen
        );
    }

    #[test]
    fn on_a_phone_the_folder_is_not_chosen_and_the_unlock_form_takes_the_default() {
        let private = std::env::temp_dir().join("tawara-phone-private");
        let mut app = alone();
        app.model.default_dir = Some(private.join("keystore"));
        app.model.confine(private.clone());
        let default = private.join("keystore").display().to_string();
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Dir("/somewhere/else".to_owned()));
        assert!(
            matches!(&app.model.screen, Screen::NewWallet(f) if f.dir == default),
            "{:?}",
            app.model.screen
        );

        app.model.screen = Screen::Unlock(UnlockForm {
            dir: "/somewhere/else".to_owned(),
            ..UnlockForm::default()
        });
        app.model.confine(private);
        assert!(
            matches!(&app.model.screen, Screen::Unlock(u) if u.dir == default),
            "{:?}",
            app.model.screen
        );
        let _ = app.update(Message::Dir("/somewhere/else".to_owned()));
        assert!(
            matches!(&app.model.screen, Screen::Unlock(u) if u.dir == default),
            "a phone ignores Dir"
        );
    }

    #[test]
    fn on_a_phone_a_store_is_made_or_opened_only_inside_the_private_folder() {
        // Tawara-mobile#1's third review: a store made elsewhere in the app's
        // container has neither the backup exclusion nor iOS's file
        // protection. A phone chooses no folder (docs/DECISIONS.md D32 item
        // 10 (a)): the store is in the default folder, and `outside` stays as
        // a second guard on that folder.
        let scratch = Scratch::new("confined");
        let (mut app, events) = app(&scratch);
        let root = scratch.0.join("private");
        std::fs::create_dir_all(&root).unwrap();
        app.model.confine(root.clone());
        let outside = scratch.0.join("SecondWallet").join("keystore");
        let inside = root.join("keystore");
        let refused = |app: &App| {
            let error = match &app.model.screen {
                Screen::NewWallet(f) | Screen::Restore(RestoreForm { form: f, .. }) => {
                    f.error.clone()
                }
                Screen::Unlock(u) => u.error.clone(),
                other => panic!("unexpected screen {other:?}"),
            };
            assert!(
                error
                    .as_deref()
                    .is_some_and(|e| e.contains("outside the app's own folder")),
                "{error:?}"
            );
            assert!(app.model.busy.is_none(), "a command was sent");
        };

        // S3, the default folder outside: refused, and a folder typed for it
        // is not taken.
        app.model.default_dir = Some(outside.clone());
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Dir(inside.display().to_string()));
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::CreateWallet);
        refused(&app);
        assert!(
            matches!(&app.model.screen, Screen::NewWallet(f)
                if f.password == PASSWORD && f.dir == outside.display().to_string()),
            "what was typed is kept, and the folder is not changed"
        );

        // S6, the same.
        let _ = app.update(Message::Go(Go::Start));
        let _ = app.update(Message::Choose(StartChoice::Restore));
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::PhraseText(typed(PHRASE)));
        let _ = app.update(Message::RestoreWallet);
        refused(&app);

        // The default folder inside: made, locked and unlocked, as anywhere.
        app.model.default_dir = Some(inside.clone());
        let _ = app.update(Message::Go(Go::Start));
        let _ = app.update(Message::Choose(StartChoice::Restore));
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::PhraseText(typed(PHRASE)));
        let _ = app.update(Message::RestoreWallet);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
        let _ = app.update(Message::Lock);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Unlock(_))
        });

        // S7: the default folder, and only that one, is offered.
        app.model.default_dir = Some(outside);
        let _ = app.update(Message::Go(Go::Unlock));
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::UnlockWallet);
        refused(&app);
        app.model.default_dir = Some(inside);
        let _ = app.update(Message::Go(Go::Unlock));
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::UnlockWallet);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
    }

    #[test]
    fn a_store_opened_by_a_relative_folder_is_remembered_by_where_it_is() {
        let here = std::env::current_dir().unwrap();
        assert_eq!(store_dir(" wallets/savings "), here.join("wallets/savings"));
        assert_eq!(
            store_dir(""),
            PathBuf::new(),
            "left for the library to refuse"
        );

        let mut app = alone();
        let mut view = tawara_wallet_core::sample::wallet_view();
        view.dir = PathBuf::from("wallets").join("savings");
        app.opened(view);
        let remembered = here.join("wallets").join("savings").display().to_string();
        assert_eq!(app.model.prefs.store.as_deref(), Some(remembered.as_str()));
    }

    fn key(named: keyboard::key::Named, modifiers: keyboard::Modifiers) -> IcedEvent {
        IcedEvent::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(named),
            modified_key: keyboard::Key::Named(named),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    }

    #[test]
    fn tab_and_shift_tab_move_between_fields_and_count_as_input() {
        use keyboard::Modifiers;
        use keyboard::key::Named;
        let window = iced::window::Id::unique();
        let input = |event, status| person_input(event, status, window);
        let ignored = event::Status::Ignored;
        assert!(matches!(
            input(key(Named::Tab, Modifiers::empty()), ignored),
            Some(Message::FocusNext)
        ));
        assert!(matches!(
            input(key(Named::Tab, Modifiers::SHIFT), ignored),
            Some(Message::FocusPrevious)
        ));
        // A widget that keeps Tab for itself keeps it; the key still counts.
        assert!(matches!(
            input(key(Named::Tab, Modifiers::empty()), event::Status::Captured),
            Some(Message::Input)
        ));
        assert!(matches!(
            input(key(Named::Enter, Modifiers::empty()), ignored),
            Some(Message::Input)
        ));
    }

    /// Which of a screen's focusable widgets is focused, and how many there
    /// are, in the order Tab goes through them.
    #[derive(Default)]
    struct Focused {
        total: usize,
        focused: Option<usize>,
    }

    impl iced::advanced::widget::Operation for Focused {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation),
        ) {
            operate(self);
        }

        fn focusable(
            &mut self,
            _: Option<&iced::advanced::widget::Id>,
            _: iced::Rectangle,
            state: &mut dyn iced::advanced::widget::operation::Focusable,
        ) {
            if state.is_focused() {
                self.focused = Some(self.total);
            }
            self.total += 1;
        }
    }

    #[test]
    fn tab_goes_through_each_forms_fields_in_order() {
        use iced::advanced::renderer::Headless;
        use iced::advanced::widget::operation::{Outcome, focusable};
        use iced_runtime::user_interface::{Cache, UserInterface};

        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("the software renderer starts");
        let form = || PasswordForm::in_dir("/a/folder".to_owned());
        let forms = [
            ("S2", Screen::Node(NodeForm::default()), 1),
            ("S3", Screen::NewWallet(form()), 3),
            (
                "S5",
                Screen::Confirm(PhraseState {
                    phrase: PhraseForDisplay::example(),
                    dir: String::new(),
                    positions: tawara_wallet_core::CONFIRM_POSITIONS,
                    written: true,
                    words: Default::default(),
                    error: None,
                }),
                3,
            ),
            (
                "S6",
                Screen::Restore(RestoreForm {
                    form: form(),
                    phrase: String::new(),
                }),
                4,
            ),
            ("S7", Screen::Unlock(UnlockForm::default()), 2),
        ];
        for (name, screen, fields) in forms {
            let mut model = Model::new(Preferences::default(), None);
            model.screen = screen;
            let mut ui = UserInterface::build(
                crate::screens::view(&model),
                iced::Size::new(1440.0, 900.0),
                Cache::default(),
                &mut renderer,
            );
            // As the runtime runs a widget operation: to its end.
            let mut run = |operation: Box<dyn iced::advanced::widget::Operation>| {
                let mut next = Some(operation);
                while let Some(mut operation) = next.take() {
                    ui.operate(&renderer, operation.as_mut());
                    if let Outcome::Chain(chained) = operation.finish() {
                        next = Some(chained);
                    }
                }
                let mut focused = Focused::default();
                ui.operate(&renderer, &mut focused);
                (focused.total, focused.focused)
            };
            for field in 0..fields {
                let tab = run(Box::new(focusable::focus_next()));
                assert_eq!(tab, (fields, Some(field)), "{name}: Tab to field {field}");
            }
            let back = run(Box::new(focusable::focus_previous()));
            assert_eq!(back, (fields, fields.checked_sub(2)), "{name}: Shift-Tab");
        }
    }

    /// An application with no worker, for the replies and events it is
    /// handed directly.
    fn alone() -> App {
        App {
            model: Model::new(Preferences::default(), None),
            worker: None,
            prefs_path: None,
            touches: std::cell::Cell::default(),
        }
    }

    #[test]
    fn a_store_made_but_not_opened_is_the_one_offered_to_unlock() {
        let mut app = alone();
        let made = std::env::temp_dir().join("tawara-made-elsewhere");
        app.model.default_dir = Some(std::env::temp_dir().join("tawara-default"));
        app.model.screen = Screen::Restore(RestoreForm {
            form: PasswordForm::in_dir(made.display().to_string()),
            phrase: String::new(),
        });
        let _ = app.on_reply(
            Purpose::CreateFromPhrase,
            Reply::Created {
                dir: made.clone(),
                first: tawara_wallet_core::view::AccountId::from_tag([0; 20]),
                opened: Err(Refusal {
                    kind: tawara_wallet_core::RefusalKind::StoreInUse,
                    text: "the store is in use".to_owned(),
                }),
            },
        );
        let made = made.display().to_string();
        assert!(
            matches!(
                &app.model.screen,
                Screen::Unlock(u) if u.dir == made && u.error.as_deref() == Some("the store is in use")
            ),
            "{:?}",
            app.model.screen
        );
        assert_eq!(app.model.prefs.store.as_deref(), Some(made.as_str()));
    }

    #[test]
    fn a_stopped_worker_takes_the_secrets_on_the_screen_with_it() {
        let screens = [
            Screen::Confirm(PhraseState {
                phrase: PhraseForDisplay::example(),
                dir: String::new(),
                positions: tawara_wallet_core::CONFIRM_POSITIONS,
                written: true,
                words: ["abandon".into(), "abandon".into(), String::new()],
                error: None,
            }),
            Screen::Phrase(PhraseState {
                phrase: PhraseForDisplay::example(),
                dir: String::new(),
                positions: tawara_wallet_core::CONFIRM_POSITIONS,
                written: false,
                words: Default::default(),
                error: None,
            }),
            Screen::Restore(RestoreForm {
                form: PasswordForm {
                    password: PASSWORD.to_owned(),
                    ..PasswordForm::default()
                },
                phrase: PHRASE.to_owned(),
            }),
            Screen::Unlock(UnlockForm {
                password: PASSWORD.to_owned(),
                ..UnlockForm::default()
            }),
        ];
        for screen in screens {
            let mut app = alone();
            app.model.screen = screen;
            app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
            let _ = app.on_event(Event::Stopped { panicked: true });
            assert_eq!(app.model.stopped, Some(true));
            assert!(app.model.wallet.is_none());
            assert!(
                matches!(app.model.screen, Screen::Start { .. }),
                "{:?}",
                app.model.screen
            );
        }
    }

    #[test]
    fn a_phrase_dropped_by_the_idle_lock_leads_back_to_creating_in_its_folder() {
        let scratch = Scratch::new("idle-create");
        let prefs = Preferences {
            idle_lock: Duration::from_millis(400),
            ..Preferences::default()
        };
        let model = Model::new(prefs, Some(scratch.0.join("keystore")));
        let (mut app, events) = App::start(model, None);
        let events = events.expect("the worker starts");
        let chosen = scratch.0.join("chosen").join("keystore");
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Dir(chosen.display().to_string()));
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::CreateWallet);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Phrase(_))
        });

        // Nothing is done; the idle lock drops the phrase.
        pump(&mut app, &events, |a| {
            !matches!(a.model.screen, Screen::Phrase(_))
        });
        let chosen = chosen.display().to_string();
        match &app.model.screen {
            Screen::NewWallet(f) => {
                assert_eq!(f.dir, chosen);
                assert!(
                    f.note
                        .as_deref()
                        .is_some_and(|n| n.contains("no store was written")),
                    "{:?}",
                    f.note
                );
            }
            other => panic!("expected S3 again, got {other:?}"),
        }
        assert!(!tawara_wallet_core::store_exists(Path::new(&chosen)));
    }

    #[test]
    fn a_phrase_the_worker_no_longer_holds_is_dropped_from_the_screen() {
        let confirming = || {
            Screen::Confirm(PhraseState {
                phrase: PhraseForDisplay::example(),
                dir: "/a/chosen/folder".to_owned(),
                positions: tawara_wallet_core::CONFIRM_POSITIONS,
                written: true,
                words: Default::default(),
                error: None,
            })
        };
        let refusal = |kind, text: &str| {
            Reply::Refused(Refusal {
                kind,
                text: text.to_owned(),
            })
        };

        // A refusal that leaves the phrase waiting keeps S5, with why.
        let mut app = alone();
        app.model.screen = confirming();
        let _ = app.on_reply(
            Purpose::CreateConfirm,
            refusal(RefusalKind::UnsafeDirectory, "unsafe. Nothing was created."),
        );
        assert!(
            matches!(&app.model.screen, Screen::Confirm(p) if p.error.is_some()),
            "{:?}",
            app.model.screen
        );

        // No phrase waiting: the one shown goes, and S3 says why.
        let mut app = alone();
        app.model.screen = confirming();
        let _ = app.on_reply(
            Purpose::CreateConfirm,
            refusal(
                RefusalKind::NothingToConfirm,
                "no phrase. Nothing was created.",
            ),
        );
        assert!(
            matches!(
                &app.model.screen,
                Screen::NewWallet(f)
                    if f.dir == "/a/chosen/folder"
                        && f.error.as_deref() == Some("no phrase. Nothing was created.")
            ),
            "{:?}",
            app.model.screen
        );
    }

    #[test]
    fn starting_over_keeps_the_folder_chosen() {
        let mut app = alone();
        app.model.screen = Screen::Confirm(PhraseState {
            phrase: PhraseForDisplay::example(),
            dir: "/a/chosen/folder".to_owned(),
            positions: tawara_wallet_core::CONFIRM_POSITIONS,
            written: true,
            words: Default::default(),
            error: None,
        });
        let _ = app.update(Message::StartOver);
        assert!(
            matches!(
                &app.model.screen,
                Screen::NewWallet(f) if f.dir == "/a/chosen/folder" && f.note.is_none()
            ),
            "{:?}",
            app.model.screen
        );
    }

    /// The sample store open on a wallet page, with no worker.
    fn on_wallet(page: Page) -> App {
        let mut app = alone();
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        app.model.screen = Screen::Wallet(WalletPage {
            page,
            ..WalletPage::default()
        });
        app
    }

    fn wallet_page(app: &App) -> &WalletPage {
        match &app.model.screen {
            Screen::Wallet(p) => p,
            other => panic!("expected the wallet, got {other:?}"),
        }
    }

    fn wallet(app: &mut App, msg: WalletMsg) {
        let _ = app.update(Message::Wallet(msg));
    }

    #[test]
    fn the_sent_page_is_shown_whatever_is_open_and_its_bytes_save_anew_each_time() {
        let scratch = Scratch::new("artifact");
        let mut app = on_wallet(Page::Receive(ReceivePage::default()));
        let sent = tawara_wallet_core::sample::sent_view();
        let _ = app.on_reply(Purpose::ConfirmSend, Reply::Sent(sent.clone()));
        let p = wallet_page(&app);
        assert!(
            matches!(&p.page, Page::Send(SendPage { stage: SendStage::Sent(s), .. })
                if s.sent == sent && !s.resigned),
            "{:?}",
            p.page
        );
        assert_eq!(
            p.open.get(&ReportKey::Sent),
            Some(&Level::Summary),
            "the three facts start open"
        );
        assert_eq!(app.model.wallet.as_ref(), Some(&sent.view));

        let saved = |app: &App| match &wallet_page(app).page {
            Page::Send(SendPage {
                stage: SendStage::Sent(s),
                ..
            }) => s.saved.clone(),
            _ => None,
        };
        // No Downloads folder: nothing is saved, and the page says so.
        wallet(&mut app, WalletMsg::SaveArtifact);
        assert!(matches!(saved(&app), Some(Err(_))), "{:?}", saved(&app));

        app.model.downloads = Some(scratch.0.join("Downloads"));
        wallet(&mut app, WalletMsg::SaveArtifact);
        let first = saved(&app).unwrap().unwrap();
        wallet(&mut app, WalletMsg::SaveArtifact);
        let second = saved(&app).unwrap().unwrap();
        assert_ne!(first, second, "a second save never overwrites the first");
        for path in [&first, &second] {
            assert_eq!(
                std::fs::read_to_string(path).unwrap(),
                format!("{}\n", sent.artifact_hex)
            );
            assert!(
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("tawara-spend-5e5e5e5e"),
                "{}",
                path.display()
            );
        }
    }

    /// What was typed for the sample spend.
    fn spend_typed() -> SpendForm {
        SpendForm {
            rows: vec![DestinationRow {
                to: "dest".to_owned(),
                amount: "1".to_owned(),
                reference: "PAYROLL-11".to_owned(),
            }],
            fee: "1000".to_owned(),
            ..SpendForm::default()
        }
    }

    /// The application on W6, the sample spend just signed from W5 with
    /// [`spend_typed`].
    fn just_sent() -> (App, tawara_wallet_core::SentView) {
        let plan = tawara_wallet_core::sample::plan_view();
        let mut app = on_wallet(Page::Send(SendPage {
            from: Some(plan.from),
            form: spend_typed(),
            stage: SendStage::Review(Box::new(plan)),
        }));
        let sent = tawara_wallet_core::sample::sent_view();
        let _ = app.on_reply(Purpose::ConfirmSend, Reply::Sent(sent.clone()));
        (app, sent)
    }

    fn sent_shown(app: &App) -> Option<(&SpendForm, &SentPage)> {
        match &wallet_page(app).page {
            Page::Send(SendPage {
                form,
                stage: SendStage::Sent(s),
                ..
            }) => Some((form, s)),
            _ => None,
        }
    }

    /// The application on W5 with a worker, the sample spend being signed:
    /// the worker's answer to it, then a lock for `reason`, reach the
    /// interface as events, in that order, as a lock queued behind a sign
    /// does (the worker finishes a spend it has started, then locks).
    fn signed_then_locked(
        scratch: &Scratch,
        reason: LockReason,
    ) -> (App, tawara_wallet_core::SentView) {
        let (mut app, _events) = app(scratch);
        let plan = tawara_wallet_core::sample::plan_view();
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        app.model.screen = Screen::Wallet(WalletPage {
            page: Page::Send(SendPage {
                from: Some(plan.from),
                form: spend_typed(),
                stage: SendStage::Review(Box::new(plan)),
            }),
            ..WalletPage::default()
        });
        let id = RequestId::example();
        app.worker
            .as_mut()
            .expect("a worker")
            .waiting
            .push((id, Purpose::ConfirmSend));
        app.model.busy = Some(Busy {
            id,
            activity: None,
            progress: None,
        });
        let sent = tawara_wallet_core::sample::sent_view();
        for event in [
            Event::Done {
                id,
                reply: Reply::Sent(sent.clone()),
            },
            Event::Locked { reason },
        ] {
            let _ = app.update(Message::Worker(WorkerEvent::new(event)));
        }
        (app, sent)
    }

    #[test]
    fn a_lock_while_signing_wipes_the_unsaved_sent_page() {
        // Lock is pressed while the spend waits on the node. The worker does
        // not stop a spend it has started: the spend answers, then the lock.
        // The bytes were not saved, so the lock wipes them (docs/DECISIONS.md
        // D38 item 5, as revised on 2026-10-13).
        let scratch = Scratch::new("lock-while-signing");
        let (mut app, sent) = signed_then_locked(&scratch, LockReason::Asked);
        assert_eq!(app.model.wallet, None);
        match &app.model.screen {
            Screen::Unlock(u) => {
                assert_eq!(u.dir, sent.view.dir.display().to_string());
                assert!(
                    u.note.as_deref().is_some_and(|n| n.contains("wiped")),
                    "{:?}",
                    u.note
                );
            }
            other => panic!("expected the unlock screen, got {other:?}"),
        }
        assert!(app.model.signed.is_empty(), "nothing unsaved is kept");

        // Its store unlocked: the page is not brought back, and nothing is
        // offered.
        let _ = app.on_reply(Purpose::Unlock, Reply::Unlocked(sent.view.clone()));
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
        assert_eq!(app.model.unsaved_signed().count(), 0);
        assert!(app.model.signed.is_empty());
    }

    #[test]
    fn a_lock_keeps_a_saved_sent_page_until_its_store_is_unlocked() {
        // The bytes are saved first, so the lock keeps the page as it was
        // left, and the unlock brings it back.
        let scratch = Scratch::new("lock-saved-sent");
        let (mut app, sent) = just_sent();
        app.model.downloads = Some(scratch.0.join("Downloads"));
        wallet(&mut app, WalletMsg::SaveArtifact);
        wallet(&mut app, WalletMsg::Report(ReportKey::Sent, Level::Full));
        let _ = app.on_event(Event::Locked {
            reason: LockReason::Asked,
        });
        assert_eq!(app.model.wallet, None);
        assert!(matches!(&app.model.screen, Screen::Unlock(u)
            if u.note.as_deref().is_some_and(|n| n.contains("signed spend"))));
        assert_eq!(app.model.signed.len(), 1);
        let kept = &app.model.signed[0];
        assert!(kept.resume);
        assert!(
            kept.sent.sent.view.accounts.is_empty() && kept.sent.sent.view.notice.is_none(),
            "what the store showed goes with the lock"
        );

        // Another store unlocked: the page waits for its own, and is not
        // offered there.
        let mut other = tawara_wallet_core::sample::wallet_view();
        other.dir = std::env::temp_dir().join("tawara-another-store");
        let _ = app.on_reply(Purpose::Unlock, Reply::Unlocked(other));
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
        assert_eq!(app.model.signed.len(), 1);
        assert_eq!(app.model.unsaved_signed().count(), 0);

        // Its own store unlocked: the page as it was left, with the bytes and
        // what was typed for them.
        let _ = app.on_reply(Purpose::Unlock, Reply::Unlocked(sent.view.clone()));
        let (form, s) = sent_shown(&app).expect("the sent page");
        assert_eq!(*form, spend_typed());
        assert_eq!(s.sent.artifact_hex, sent.artifact_hex);
        assert_eq!(s.sent.view, sent.view);
        assert_eq!(
            wallet_page(&app).open.get(&ReportKey::Sent),
            Some(&Level::Full)
        );
        assert!(app.model.signed.is_empty());

        // Left for another page and then locked: kept, not brought back by the
        // unlock, and not offered, since it is saved.
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        let _ = app.on_event(Event::Locked {
            reason: LockReason::Idle,
        });
        assert!(matches!(&app.model.screen, Screen::Unlock(u)
            if u.note.as_deref().is_some_and(|n| !n.contains("signed spend"))));
        let _ = app.on_reply(Purpose::Unlock, Reply::Unlocked(sent.view.clone()));
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
        assert_eq!(app.model.unsaved_signed().count(), 0);
    }

    #[test]
    fn a_signed_spend_left_any_way_is_kept_offered_and_resigned_from() {
        let scratch = Scratch::new("kept");
        let (mut app, sent) = just_sent();
        let from = sent.from;
        let hex = sent.artifact_hex.clone();

        // Left for the node screen, then back: the spend is offered.
        let _ = app.update(Message::Go(Go::Node(Back::Wallet)));
        assert!(matches!(app.model.screen, Screen::Node(_)));
        let _ = app.update(Message::Go(Go::Wallet));
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
        assert_eq!(app.model.unsaved_signed().count(), 1);

        // Opened again: the page as it was left.
        wallet(&mut app, WalletMsg::ShowSigned(hex.clone()));
        let (form, s) = sent_shown(&app).expect("the sent page");
        assert_eq!(*form, spend_typed());
        assert_eq!(s.sent.artifact_hex, hex);
        assert!(app.model.signed.is_empty());

        // Saved, then left: still kept, for re-signing, and no longer
        // offered.
        app.model.downloads = Some(scratch.0.join("Downloads"));
        wallet(&mut app, WalletMsg::SaveArtifact);
        wallet(&mut app, WalletMsg::Open(To::Account(from)));
        assert_eq!(app.model.signed.len(), 1);
        assert_eq!(app.model.unsaved_signed().count(), 0);

        // Re-signing starts from what was typed for it; another account's
        // from nothing.
        wallet(&mut app, WalletMsg::Open(To::Resign(from)));
        assert!(matches!(&wallet_page(&app).page,
            Page::Resign(ResignPage { form, .. }) if *form == spend_typed()));

        // Re-signed, the same bytes again: kept once, as the newest page, and
        // saved, since those bytes were (D38 item 5, as revised on 2026-10-13).
        let _ = app.on_reply(Purpose::Resign, Reply::Sent(sent.clone()));
        assert!(sent_shown(&app).is_some_and(|(_, s)| s.resigned && !s.unsaved()));
        let other = app.model.wallet.as_ref().unwrap().accounts[1].id;
        wallet(&mut app, WalletMsg::Open(To::Resign(other)));
        assert_eq!(app.model.signed.len(), 1);
        assert!(app.model.signed[0].sent.resigned);
        assert!(!app.model.signed[0].unsaved());
        assert_eq!(app.model.unsaved_signed().count(), 0);
        assert!(matches!(&wallet_page(&app).page,
            Page::Resign(ResignPage { form, .. }) if *form == SpendForm::default()));

        // Settling that finds the spend not landed keeps it. A check that
        // finds the account in sync forgets it, since its bytes are saved
        // (D38 item 5, as revised on 2026-10-13; unsaved bytes are kept past
        // that, as the tests below show).
        let _ = app.on_reply(
            Purpose::Settle,
            Reply::Settled {
                text: "not landed".to_owned(),
                view: sent.view.clone(),
            },
        );
        assert_eq!(app.model.signed.len(), 1);
        let _ = app.on_reply(Purpose::Status, in_sync_status(from));
        assert!(app.model.signed.is_empty());
        wallet(&mut app, WalletMsg::Open(To::Resign(from)));
        assert!(matches!(&wallet_page(&app).page,
            Page::Resign(ResignPage { form, .. }) if *form == SpendForm::default()));
    }

    /// A check's answer that finds `account` in sync.
    fn in_sync_status(account: AccountId) -> Reply {
        Reply::Status {
            account,
            state: tawara_wallet_core::view::AccountState::InSync { balance: 5 },
            spendable: true,
            text: "in sync".to_owned(),
            settled: None,
        }
    }

    /// `view` with `account` in sync and spendable, as a read after its
    /// settle shows it.
    fn in_sync(view: &mut tawara_wallet_core::view::WalletView, account: AccountId) {
        let row = view
            .accounts
            .iter_mut()
            .find(|a| a.id == account)
            .expect("the account is in the store");
        row.state = tawara_wallet_core::view::AccountState::InSync { balance: 1 };
        row.spendable = true;
    }

    #[test]
    fn an_automatic_settle_keeps_the_unsaved_bytes_offered() {
        // A refresh reads the account in sync and settles it itself (D38 item
        // 1). The bytes are not saved, so they stay offered: a reorganisation
        // can revert the settle, and then a saved copy may be the only way to
        // move the balance (D38 item 5, as revised on 2026-10-13).
        let (mut app, sent) = just_sent();
        let from = sent.from;
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        assert_eq!(app.model.unsaved_signed().count(), 1);

        let mut view = sent.view.clone();
        in_sync(&mut view, from);
        view.settled = vec![tawara_wallet_core::view::SettledAccount {
            account: from,
            text: "the library's page".to_owned(),
        }];
        let _ = app.on_reply(Purpose::Refresh, Reply::Wallet(view));
        assert_eq!(
            app.model.unsaved_signed().count(),
            1,
            "in sync, but not saved"
        );
        assert_eq!(
            app.model.signed[0].sent.sent.artifact_hex,
            sent.artifact_hex
        );
        assert!(app.model.settled_waiting.contains_key(&from));

        // A check that settles the account itself keeps them too.
        let _ = app.on_reply(
            Purpose::Status,
            Reply::Status {
                account: from,
                state: tawara_wallet_core::view::AccountState::InSync { balance: 5 },
                spendable: true,
                text: "settled".to_owned(),
                settled: Some(tawara_wallet_core::view::SettledAccount {
                    account: from,
                    text: "the library's page".to_owned(),
                }),
            },
        );
        assert_eq!(
            app.model.unsaved_signed().count(),
            1,
            "a check's settle keeps them too"
        );
    }

    #[test]
    fn a_manual_settle_keeps_the_unsaved_bytes_offered() {
        // Settle on the account's own page, the same rule as an automatic one
        // (D38 item 5, as revised on 2026-10-13).
        let (mut app, sent) = just_sent();
        wallet(&mut app, WalletMsg::Open(To::Account(sent.from)));
        assert_eq!(app.model.unsaved_signed().count(), 1);

        let mut view = sent.view.clone();
        in_sync(&mut view, sent.from);
        let _ = app.on_reply(
            Purpose::Settle,
            Reply::Settled {
                text: "settled: the library's page".to_owned(),
                view,
            },
        );
        assert!(matches!(wallet_page(&app).page, Page::Account(_)));
        assert_eq!(app.model.unsaved_signed().count(), 1);
    }

    #[test]
    fn saving_clears_the_unsaved_state_and_a_saved_spend_is_forgotten_once_in_sync() {
        let scratch = Scratch::new("saved-then-in-sync");
        let (mut app, sent) = just_sent();
        app.model.downloads = Some(scratch.0.join("Downloads"));
        wallet(&mut app, WalletMsg::SaveArtifact);
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        assert_eq!(app.model.signed.len(), 1);
        assert_eq!(
            app.model.unsaved_signed().count(),
            0,
            "saved, so no longer unsaved"
        );
        let _ = app.on_reply(Purpose::Status, in_sync_status(sent.from));
        assert!(
            app.model.signed.is_empty(),
            "a saved spend is forgotten once in sync"
        );
    }

    #[test]
    fn dismissing_unsaved_bytes_drops_and_wipes_them() {
        // On the sent page that shows them: Dismiss asks first, and the bytes
        // stay until it is confirmed.
        let (mut app, sent) = just_sent();
        wallet(&mut app, WalletMsg::AskDismiss);
        assert!(sent_shown(&app).is_some_and(|(_, s)| s.dismissing));
        wallet(&mut app, WalletMsg::CancelDismiss);
        let (_, s) = sent_shown(&app).expect("the page stays");
        assert!(!s.dismissing && s.sent.artifact_hex == sent.artifact_hex);
        wallet(&mut app, WalletMsg::AskDismiss);
        // Confirmed: the page is left with them.
        wallet(
            &mut app,
            WalletMsg::DismissSigned(sent.artifact_hex.clone()),
        );
        assert!(app.model.signed.is_empty());
        assert_eq!(app.model.unsaved_signed().count(), 0);
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));

        // Kept and not shown: dropped from the kept spends, and wiped as it
        // goes.
        let (mut app, sent) = just_sent();
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        let dropped = app.model.dismiss_signed(&sent.artifact_hex);
        assert_eq!(dropped.len(), 1);
        assert!(
            dropped
                .iter()
                .all(|s| s.sent.sent.artifact_hex.is_empty() && s.sent.sent.text.is_empty()),
            "the bytes are wiped, and the library's page that repeats them"
        );

        // Saved bytes are not dismissed: Dismiss is not asked for them.
        let scratch = Scratch::new("dismiss-saved");
        let (mut app, _) = just_sent();
        app.model.downloads = Some(scratch.0.join("Downloads"));
        wallet(&mut app, WalletMsg::SaveArtifact);
        wallet(&mut app, WalletMsg::AskDismiss);
        assert!(sent_shown(&app).is_some_and(|(_, s)| !s.dismissing));
        assert!(app.model.signed.is_empty());
        assert_eq!(app.model.unsaved_signed().count(), 0);
    }

    #[test]
    fn a_lock_asked_for_idle_background_or_at_shutdown_wipes_the_unsaved_bytes_shown_or_kept() {
        // The locks the owner named, and the shutdown that ends the run: nothing
        // unsaved survives one (D38 item 5, as revised on 2026-10-13).
        for reason in [
            LockReason::Asked,
            LockReason::Idle,
            LockReason::Background,
            LockReason::Shutdown,
        ] {
            let (mut app, _) = just_sent();
            let _ = app.on_event(Event::Locked { reason });
            assert!(app.model.signed.is_empty(), "{reason:?}: the shown bytes");

            let (mut app, sent) = just_sent();
            wallet(&mut app, WalletMsg::Open(To::Dashboard));
            let _ = app.on_event(Event::Locked { reason });
            assert!(app.model.signed.is_empty(), "{reason:?}: the kept bytes");
            let _ = app.on_reply(Purpose::Unlock, Reply::Unlocked(sent.view.clone()));
            assert_eq!(
                app.model.unsaved_signed().count(),
                0,
                "{reason:?}: nothing offered"
            );
        }
    }

    #[test]
    fn bytes_saved_before_a_resign_stay_saved_over_a_lock() {
        // The bytes are saved, the page left, and a re-sign gives the same
        // bytes again: they are on disk, so the re-signed page is saved, and
        // a lock keeps it to show again rather than wiping it (D38 item 5, as
        // revised on 2026-10-13).
        let scratch = Scratch::new("resign-saved-lock");
        let (mut app, sent) = just_sent();
        app.model.downloads = Some(scratch.0.join("Downloads"));
        wallet(&mut app, WalletMsg::SaveArtifact);
        wallet(&mut app, WalletMsg::Open(To::Resign(sent.from)));
        let _ = app.on_reply(Purpose::Resign, Reply::Sent(sent.clone()));
        assert!(sent_shown(&app).is_some_and(|(_, s)| !s.unsaved()));
        let _ = app.on_event(Event::Locked {
            reason: LockReason::Asked,
        });
        assert_eq!(app.model.signed.len(), 1);
        assert!(app.model.signed[0].resume && !app.model.signed[0].unsaved());
        assert!(matches!(&app.model.screen, Screen::Unlock(u)
            if u.note.as_deref().is_some_and(|n| !n.contains("wiped"))));
    }

    #[test]
    fn a_close_no_one_asked_for_keeps_the_unsaved_bytes() {
        // The worker closes a store it could not read back after a send's
        // settle, right after the send's answer, and an unlock replaces a
        // store: neither is a lock the person asked for or one the wallet
        // makes on idle or in the background, so bytes just signed are kept,
        // not wiped before they are seen (D38 item 5, as revised on
        // 2026-10-13).
        let scratch = Scratch::new("reopen-failed-keeps");
        let (mut app, sent) = signed_then_locked(&scratch, LockReason::ReopenFailed);
        assert_eq!(app.model.signed.len(), 1);
        let kept = &app.model.signed[0];
        assert!(kept.unsaved() && kept.resume);
        assert_eq!(kept.sent.sent.artifact_hex, sent.artifact_hex);
        assert_eq!(kept.sent.sent.text, sent.text);
        match &app.model.screen {
            Screen::Unlock(u) => {
                let note = u.note.as_deref().unwrap_or_default();
                assert!(note.contains("signed spend"), "{note}");
                assert!(!note.contains("wiped"), "{note}");
            }
            other => panic!("expected the unlock screen, got {other:?}"),
        }
        // Its store unlocked: the page as it was left, still unsaved.
        let _ = app.on_reply(Purpose::Unlock, Reply::Unlocked(sent.view.clone()));
        let (_, s) = sent_shown(&app).expect("the sent page");
        assert!(s.unsaved());
        assert_eq!(s.sent.artifact_hex, sent.artifact_hex);

        // Kept and not shown: a store replaced, or closed, keeps it offered.
        for reason in [LockReason::Replaced, LockReason::ReopenFailed] {
            let (mut app, sent) = just_sent();
            wallet(&mut app, WalletMsg::Open(To::Dashboard));
            let _ = app.on_event(Event::Locked { reason });
            assert_eq!(app.model.signed.len(), 1, "{reason:?}");
            assert_eq!(
                app.model.signed[0].sent.sent.artifact_hex, sent.artifact_hex,
                "{reason:?}"
            );
            let _ = app.on_reply(Purpose::Unlock, Reply::Unlocked(sent.view.clone()));
            assert_eq!(app.model.unsaved_signed().count(), 1, "{reason:?}");
        }
    }

    #[test]
    fn the_lock_wipes_what_it_drops_and_keeps_a_saved_spend() {
        let (mut app, _) = just_sent();
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        // A saved spend kept beside the unsaved one: the lock keeps it.
        let mut saved = app.model.signed[0].clone();
        saved.sent.saved = Some(Ok(std::env::temp_dir().join("tawara-saved-spend.hex")));
        app.model.signed.push(saved);

        let dropped = app.model.drop_unsaved_at_lock();
        assert_eq!(dropped.len(), 1, "the unsaved one");
        assert!(
            dropped
                .iter()
                .all(|s| s.sent.sent.artifact_hex.is_empty() && s.sent.sent.text.is_empty()),
            "the bytes are wiped, and the library's page that repeats them"
        );
        assert_eq!(app.model.signed.len(), 1);
        assert!(!app.model.signed[0].unsaved());
    }

    #[test]
    fn a_stopped_worker_leaves_the_signed_spends_to_save() {
        let scratch = Scratch::new("stopped-spend");
        let (mut app, sent) = just_sent();
        let _ = app.on_event(Event::Stopped { panicked: true });
        assert_eq!(app.model.stopped, Some(true));
        assert_eq!(app.model.wallet, None);
        assert_eq!(app.model.signed.len(), 1);

        app.model.downloads = Some(scratch.0.join("Downloads"));
        wallet(&mut app, WalletMsg::SaveSigned(sent.artifact_hex.clone()));
        let kept = &app.model.signed[0];
        let Some(Ok(path)) = &kept.sent.saved else {
            panic!("expected the bytes saved, got {:?}", kept.sent.saved);
        };
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            format!("{}\n", sent.artifact_hex)
        );
        wallet(&mut app, WalletMsg::Copy(sent.artifact_hex.clone()));
        assert!(app.model.signed[0].copied);
    }

    #[test]
    fn a_refused_signing_goes_back_to_the_form_with_why() {
        let plan = tawara_wallet_core::sample::plan_view();
        let mut app = on_wallet(Page::Send(SendPage {
            from: Some(plan.from),
            form: SpendForm::default(),
            stage: SendStage::Review(Box::new(plan)),
        }));
        let _ = app.on_reply(
            Purpose::ConfirmSend,
            Reply::Refused(Refusal {
                kind: RefusalKind::Library,
                text: "the balance moved".to_owned(),
            }),
        );
        let p = wallet_page(&app);
        assert!(matches!(
            &p.page,
            Page::Send(SendPage {
                stage: SendStage::Compose,
                ..
            })
        ));
        assert_eq!(p.error.as_deref(), Some("the balance moved"));
    }

    #[test]
    fn the_form_is_kept_through_review_and_edit() {
        let plan = tawara_wallet_core::sample::plan_view();
        let form = SpendForm {
            rows: vec![DestinationRow {
                to: "somewhere".to_owned(),
                amount: "1".to_owned(),
                reference: String::new(),
            }],
            ..SpendForm::default()
        };
        let mut app = on_wallet(Page::Send(SendPage {
            from: Some(plan.from),
            form: form.clone(),
            stage: SendStage::Compose,
        }));
        let _ = app.on_reply(Purpose::PlanSend, Reply::Planned(plan));
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Review(_),
                ..
            })
        ));
        wallet(&mut app, WalletMsg::Edit);
        assert!(
            matches!(&wallet_page(&app).page, Page::Send(SendPage {
                stage: SendStage::Compose,
                form: kept,
                ..
            }) if *kept == form),
            "{:?}",
            wallet_page(&app).page
        );
    }

    /// The sample's spend as a person types it: the payees the sample plan
    /// lays out, with their figures.
    fn typed_as_planned(plan: &tawara_wallet_core::PlanView) -> SpendForm {
        SpendForm {
            rows: plan
                .destinations
                .iter()
                .map(|d| DestinationRow {
                    to: d.destination.clone(),
                    amount: tawara_wallet_core::amount::format_mcm(d.amount)
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_owned(),
                    reference: d.reference.clone(),
                })
                .collect(),
            ..SpendForm::default()
        }
    }

    /// The window operations `task` asks the runtime to run, in order.
    fn operations(task: Task<Message>) -> Vec<Box<dyn Operation>> {
        let Some(stream) = iced_runtime::task::into_stream(task) else {
            return Vec::new();
        };
        iced::futures::executor::block_on(stream.collect::<Vec<_>>())
            .into_iter()
            .filter_map(|action| match action {
                Action::Widget(operation) => Some(operation),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_review_starts_at_the_top_of_its_page_on_a_phone_and_on_the_desktop() {
        // The form is scrolled to its end, and "Review spend" is pressed where
        // it is drawn. The review drawn over the form then starts at the top
        // of its page on each layout: the payee is in view, and "Sign & submit"
        // is not where "Review spend" was pressed (docs/SCREENS.md W5).
        for (width, size) in [(390.0, (390, 844)), (1440.0, (1440, 600))] {
            let plan = tawara_wallet_core::sample::plan_view();
            let mut app = on_wallet(Page::Send(SendPage {
                from: Some(plan.from),
                form: typed_as_planned(&plan),
                stage: SendStage::Compose,
            }));
            app.model.width = width;
            let page = crate::screens::page_id();
            let (messages, cache, pressed) = {
                let mut form = Driver::new(crate::screens::view(&app.model), size);
                form.operate(Box::new(snap_to(
                    page.clone(),
                    RelativeOffset {
                        x: None,
                        y: Some(1.0),
                    },
                )));
                let (_, scrolled) = form.scroll_of(&page).expect("the form is a page");
                assert!(scrolled.y > 0.0, "{width}: the form scrolls: {scrolled:?}");
                let label = form
                    .text_bounds("Review spend")
                    .expect("'Review spend' is drawn");
                let at = Point::new(label.center_x(), label.center_y() - scrolled.y);
                let (messages, _, _) =
                    form.send(&[mouse_move(at.x, at.y), mouse_press(), mouse_release()]);
                (messages, form.into_cache(), at)
            };
            assert!(
                matches!(messages.as_slice(), [Message::Wallet(WalletMsg::Review)]),
                "{width}: pressing it asks for the review: {messages:?}"
            );
            for message in messages {
                let _ = app.update(message);
            }

            // The worker lays the plan out. The page is drawn again, with the
            // state the form kept, and the task the answer returned runs.
            let task = app.on_reply(Purpose::PlanSend, Reply::Planned(plan.clone()));
            let mut review = Driver::with_cache(crate::screens::view(&app.model), size, cache);
            let operations = operations(task);
            assert_eq!(
                operations.len(),
                1,
                "{width}: entering the review returns a task"
            );
            for operation in operations {
                review.operate(operation);
            }
            let (viewport, scroll) = review.scroll_of(&page).expect("the review is a page");
            assert_eq!(
                scroll.y, 0.0,
                "{width}: the review starts at the top of its page"
            );
            let payee = review
                .text_bounds(&plan.destinations[0].destination)
                .expect("the payee is drawn");
            let shown = payee.y - scroll.y;
            assert!(
                shown >= viewport.y && shown + payee.height <= viewport.y + viewport.height,
                "{width}: the payee is in view at {shown}, in {viewport:?}"
            );
            let sign = review
                .text_bounds("Sign & submit")
                .expect("'Sign & submit' is drawn");
            let sign = Rectangle {
                y: sign.y - scroll.y,
                ..sign
            };
            assert!(
                !sign.contains(pressed),
                "{width}: 'Sign & submit' is not where 'Review spend' was: {sign:?}"
            );
        }
    }

    #[test]
    fn the_sent_page_starts_at_the_top_of_its_page_on_a_phone_and_on_the_desktop() {
        // The review is scrolled to its end, where "Sign & submit" is, and the
        // spend is signed. The sent page drawn in its place starts at the top
        // of its page on each layout, with its title in view, rather than at
        // the review's scroll (docs/SCREENS.md W6).
        for (width, size) in [(390.0, (390, 844)), (1440.0, (1440, 600))] {
            let plan = tawara_wallet_core::sample::plan_view();
            let mut app = on_wallet(Page::Send(SendPage {
                from: Some(plan.from),
                form: typed_as_planned(&plan),
                stage: SendStage::Review(Box::new(plan)),
            }));
            app.model.width = width;
            let page = crate::screens::page_id();
            let cache = {
                let mut review = Driver::new(crate::screens::view(&app.model), size);
                review.operate(Box::new(snap_to(
                    page.clone(),
                    RelativeOffset {
                        x: None,
                        y: Some(1.0),
                    },
                )));
                let (_, scrolled) = review.scroll_of(&page).expect("the review is a page");
                assert!(
                    scrolled.y > 0.0,
                    "{width}: the review scrolls: {scrolled:?}"
                );
                review.into_cache()
            };

            let sent = tawara_wallet_core::sample::sent_view();
            let task = app.on_reply(Purpose::ConfirmSend, Reply::Sent(sent));
            assert!(
                sent_shown(&app).is_some(),
                "{width}: the sent page is shown"
            );
            let mut shown = Driver::with_cache(crate::screens::view(&app.model), size, cache);
            // Without the task the sent page would keep the review's scroll,
            // which is the fault the task is for.
            let (_, kept) = shown.scroll_of(&page).expect("the sent page is a page");
            assert!(
                kept.y > 0.0,
                "{width}: the sent page is drawn at the review's scroll: {kept:?}"
            );
            let operations = operations(task);
            assert_eq!(
                operations.len(),
                1,
                "{width}: showing the sent page returns a task"
            );
            for operation in operations {
                shown.operate(operation);
            }
            let (viewport, scroll) = shown.scroll_of(&page).expect("the sent page is a page");
            assert_eq!(
                scroll.y, 0.0,
                "{width}: the sent page starts at the top of its page"
            );
            // The page's title, which the header sets in capitals.
            let title = shown.text_bounds("SENT").expect("the title is drawn");
            assert!(
                title.y >= viewport.y && title.y + title.height <= viewport.y + viewport.height,
                "{width}: the title is in view at {title:?}, in {viewport:?}"
            );
        }
    }

    #[test]
    fn the_form_starts_at_the_top_of_its_page_when_the_review_is_left_for_it() {
        // The review is scrolled to its end, where "Sign & submit" and "Edit
        // the spend" are. Whether the signing is refused or the spend is
        // edited, the form drawn in the review's place starts at the top of
        // its page on each layout, and a refusal's reason, which the page
        // shows at its top, is in view (docs/SCREENS.md W4).
        let refusal = "the balance moved";
        for refused in [true, false] {
            for (width, size) in [(390.0, (390, 844)), (1440.0, (1440, 600))] {
                let plan = tawara_wallet_core::sample::plan_view();
                let mut app = on_wallet(Page::Send(SendPage {
                    from: Some(plan.from),
                    form: typed_as_planned(&plan),
                    stage: SendStage::Review(Box::new(plan)),
                }));
                app.model.width = width;
                let page = crate::screens::page_id();
                let cache = {
                    let mut review = Driver::new(crate::screens::view(&app.model), size);
                    review.operate(Box::new(snap_to(
                        page.clone(),
                        RelativeOffset {
                            x: None,
                            y: Some(1.0),
                        },
                    )));
                    let (_, scrolled) = review.scroll_of(&page).expect("the review is a page");
                    assert!(
                        scrolled.y > 0.0,
                        "{width}: the review scrolls: {scrolled:?}"
                    );
                    review.into_cache()
                };

                let task = if refused {
                    app.on_reply(
                        Purpose::ConfirmSend,
                        Reply::Refused(Refusal {
                            kind: RefusalKind::Library,
                            text: refusal.to_owned(),
                        }),
                    )
                } else {
                    app.on_wallet(WalletMsg::Edit)
                };
                let how = if refused { "refused" } else { "edited" };
                assert!(
                    matches!(
                        &wallet_page(&app).page,
                        Page::Send(SendPage {
                            stage: SendStage::Compose,
                            ..
                        })
                    ),
                    "{how} at {width}: the form is shown"
                );
                let mut form = Driver::with_cache(crate::screens::view(&app.model), size, cache);
                // Without the task the form would keep the review's scroll,
                // which is the fault the task is for.
                let (_, kept) = form.scroll_of(&page).expect("the form is a page");
                assert!(
                    kept.y > 0.0,
                    "{how} at {width}: the form is drawn at the review's scroll: {kept:?}"
                );
                let operations = operations(task);
                assert_eq!(
                    operations.len(),
                    1,
                    "{how} at {width}: showing the form returns a task"
                );
                for operation in operations {
                    form.operate(operation);
                }
                let (viewport, scroll) = form.scroll_of(&page).expect("the form is a page");
                assert_eq!(
                    scroll.y, 0.0,
                    "{how} at {width}: the form starts at the top of its page"
                );
                if refused {
                    let why = form.text_bounds(refusal).expect("the refusal is drawn");
                    assert!(
                        why.y >= viewport.y && why.y + why.height <= viewport.y + viewport.height,
                        "{width}: the refusal is in view at {why:?}, in {viewport:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_plan_is_forgotten_when_its_review_is_left() {
        let scratch = Scratch::new("discard");
        let (mut app, events) = app(&scratch);
        let plan = tawara_wallet_core::sample::plan_view();
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        let review = |app: &mut App| {
            app.model.screen = Screen::Wallet(WalletPage {
                page: Page::Send(SendPage {
                    from: Some(plan.from),
                    form: SpendForm::default(),
                    stage: SendStage::Review(Box::new(plan.clone())),
                }),
                ..WalletPage::default()
            });
        };
        let discards = |a: &App| {
            a.worker.as_ref().map_or(0, |w| {
                w.waiting
                    .iter()
                    .filter(|(_, p)| matches!(p, Purpose::DiscardPlan))
                    .count()
            })
        };
        review(&mut app);
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        assert_eq!(discards(&app), 1);
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));

        review(&mut app);
        wallet(&mut app, WalletMsg::Edit);
        assert_eq!(discards(&app), 2);

        // A plan laid out after its page was left is forgotten as it comes.
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        assert_eq!(discards(&app), 2, "the form held no plan");
        let _ = app.on_reply(Purpose::PlanSend, Reply::Planned(plan.clone()));
        assert_eq!(discards(&app), 3);
        pump(&mut app, &events, |a| discards(a) == 0);
    }

    #[test]
    fn nothing_moves_while_the_page_waits_on_the_worker() {
        let mut app = on_wallet(Page::Dashboard);
        app.model.busy = Some(Busy {
            id: RequestId::example(),
            activity: None,
            progress: None,
        });
        wallet(&mut app, WalletMsg::Open(To::Send(None)));
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
    }

    #[test]
    fn a_report_opens_into_its_summary_then_its_full_text() {
        let mut app = on_wallet(Page::Dashboard);
        let level = |a: &App| {
            wallet_page(a)
                .open
                .get(&ReportKey::Notice)
                .copied()
                .unwrap_or_default()
        };
        assert_eq!(level(&app), Level::Closed);
        wallet(
            &mut app,
            WalletMsg::Report(ReportKey::Notice, Level::Summary),
        );
        assert_eq!(level(&app), Level::Summary);
        wallet(&mut app, WalletMsg::Report(ReportKey::Notice, Level::Full));
        assert_eq!(level(&app), Level::Full);
        // Another page and back: closed again.
        wallet(&mut app, WalletMsg::Open(To::Receive(None)));
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        assert_eq!(level(&app), Level::Closed);
    }

    #[test]
    fn a_check_updates_the_account_and_keeps_what_it_found() {
        use tawara_wallet_core::view::{AccountState, DivergenceKind};
        let accounts = tawara_wallet_core::sample::wallet_view().accounts;
        assert!(accounts[0].spendable && !accounts[1].spendable);
        let checked = |app: &mut App, n: usize, state: AccountState, spendable: bool| {
            let _ = app.on_reply(
                Purpose::Status,
                Reply::Status {
                    account: accounts[n].id,
                    state,
                    spendable,
                    text: "checked".to_owned(),
                    settled: None,
                },
            );
            app.model.wallet.as_ref().unwrap().accounts[n].clone()
        };
        let mut app = on_wallet(Page::Account(AccountPage {
            account: accounts[1].id,
            report: None,
        }));
        let row = checked(&mut app, 1, AccountState::InSync { balance: 5 }, false);
        assert_eq!(row.state, AccountState::InSync { balance: 5 });
        assert!(
            !row.spendable,
            "an account set aside when the wallet opened stays aside"
        );
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Account(AccountPage {
                report: Some((Done::Checked, text)),
                ..
            }) if text == "checked"
        ));

        // A check the node did not answer, then one it did: the account is
        // spendable again, as the worker says, and Send offers it.
        let unreachable = AccountState::Diverged {
            kind: DivergenceKind::Unreachable,
            report: "no answer".to_owned(),
            advance_to: None,
        };
        assert!(!checked(&mut app, 0, unreachable, false).spendable);
        assert!(checked(&mut app, 0, AccountState::InSync { balance: 9 }, true).spendable);
        wallet(&mut app, WalletMsg::Open(To::Send(Some(accounts[0].id))));
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage { from, .. }) if *from == Some(accounts[0].id)
        ));
        wallet(&mut app, WalletMsg::DestinationTo(0, "dest".to_owned()));
        wallet(&mut app, WalletMsg::Amount(0, "1".to_owned()));
        wallet(&mut app, WalletMsg::Review);
        assert_eq!(wallet_page(&app).error, None, "laid out from it");
    }

    #[test]
    fn a_spend_asked_from_an_account_that_cannot_spend_keeps_it_and_says_why() {
        let accounts = tawara_wallet_core::sample::wallet_view().accounts;
        let (able, aside) = (accounts[0].id, accounts[1].id);
        assert!(accounts[0].spendable && !accounts[1].spendable);
        let mut app = on_wallet(Page::Account(AccountPage {
            account: aside,
            report: None,
        }));
        let from = |app: &App| match &wallet_page(app).page {
            Page::Send(s) => s.from,
            other => panic!("expected the send page, got {other:?}"),
        };

        // Asked for by name: kept, never swapped for the account that can.
        wallet(&mut app, WalletMsg::Open(To::Send(Some(aside))));
        assert_eq!(from(&app), Some(aside));
        wallet(&mut app, WalletMsg::DestinationTo(0, "dest".to_owned()));
        wallet(&mut app, WalletMsg::Amount(0, "1".to_owned()));
        wallet(&mut app, WalletMsg::Review);
        assert!(
            wallet_page(&app)
                .error
                .as_deref()
                .is_some_and(|e| e.starts_with("This account cannot spend now")),
            "{:?}",
            wallet_page(&app).error
        );
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Compose,
                ..
            })
        ));

        // Another chosen on purpose: that one, and it is laid out.
        wallet(&mut app, WalletMsg::From(able));
        wallet(&mut app, WalletMsg::Review);
        assert_eq!(from(&app), Some(able));
        assert_eq!(wallet_page(&app).error, None);

        // With none asked for, the first that can spend.
        wallet(&mut app, WalletMsg::Open(To::Send(None)));
        assert_eq!(from(&app), Some(able));
    }

    #[test]
    fn the_index_and_the_blocks_are_kept_until_the_store_or_the_node_changes() {
        use tawara_wallet_core::explorer::IndexState;
        let mut app = on_wallet(Page::Activity(ActivityPage::default()));
        app.model.node.url = Some("https://node.example".to_owned());
        app.model.activity.reading = true;
        let _ = app.on_reply(
            Purpose::Activity,
            Reply::Activity(Ok(tawara_wallet_core::sample::activity())),
        );
        assert!(!app.model.activity.reading);
        assert!(matches!(&app.model.activity.last, Some(Ok(h)) if h.len() == 3));
        let _ = app.on_reply(
            Purpose::Blocks,
            Reply::Blocks(Ok(tawara_wallet_core::sample::blocks())),
        );
        assert!(matches!(&app.model.blocks.last, Some(Ok(b)) if b.tip == 871_173));

        // A refusal that is not the node's answer (a cancel) leaves what
        // was shown.
        app.model.activity.reading = true;
        let _ = app.on_reply(
            Purpose::Activity,
            Reply::Refused(Refusal {
                kind: RefusalKind::Cancelled,
                text: "cancelled".to_owned(),
            }),
        );
        assert!(!app.model.activity.reading);
        assert!(matches!(&app.model.activity.last, Some(Ok(_))));

        // A node with no index says so, for every account.
        let _ = app.on_reply(
            Purpose::Activity,
            Reply::Activity(Err(tawara_wallet_core::sample::index_refusal(
                IndexState::Absent,
            ))),
        );
        assert!(
            matches!(&app.model.activity.last, Some(Err(r)) if r.index == Some(IndexState::Absent))
        );

        // Another node: another index and another chain.
        let _ = app.on_reply(
            Purpose::SetNode { form: true },
            Reply::NodeSet {
                url: "https://other.example".to_owned(),
                view: None,
            },
        );
        assert!(app.model.activity.last.is_none() && app.model.blocks.last.is_none());

        // The store locked: what the index held for it goes with it.
        let _ = app.on_reply(
            Purpose::Activity,
            Reply::Activity(Ok(tawara_wallet_core::sample::activity())),
        );
        let _ = app.on_event(Event::Locked {
            reason: LockReason::Asked,
        });
        assert!(app.model.activity.last.is_none());
    }

    /// What the worker has been sent and not yet answered, by purpose.
    fn waiting(app: &App) -> Vec<String> {
        app.worker
            .as_ref()
            .map(|w| {
                w.waiting
                    .iter()
                    .map(|(_, p)| {
                        let name = format!("{p:?}");
                        name.split([' ', '{']).next().unwrap_or("").to_owned()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn the_cards_read_the_network_the_queue_and_the_sync_state() {
        use tawara_wallet_core::sample;
        let scratch = Scratch::new("network-reads");
        let (mut app, _events) = app(&scratch);
        app.model.wallet = Some(sample::wallet_view());
        // The worker is given no node, so nothing leaves this machine: what
        // is checked is what the application asks for.
        app.model.node.url = Some("https://node.example".to_owned());
        app.ask_tip();
        assert_eq!(
            waiting(&app),
            ["Network", "Networks", "Blocks", "Mempool"],
            "the tip, the network's name, the blocks and the queue"
        );
        assert!(app.model.blocks.reading && app.model.mempool.reading);
        // Asked again before any answer: the reads on their way are not
        // asked for twice.
        app.ask_tip();
        assert_eq!(
            waiting(&app),
            ["Network", "Networks", "Blocks", "Mempool", "Network"]
        );

        let _ = app.on_reply(
            Purpose::Network {
                sent: Instant::now(),
            },
            Reply::Network {
                tip_index: 871_173,
                tip_hash: "ab".repeat(32),
                tip_time_ms: sample::TIP_MS,
                sync: Some(SyncState {
                    stage: "synchronized".to_owned(),
                    synced: true,
                }),
            },
        );
        assert_eq!(app.model.node.solved_ms, Some(sample::TIP_MS));
        assert!(app.model.node.sync.as_ref().is_some_and(|s| s.synced));
        let mainnet = NetworkName {
            blockchain: "mochimo".to_owned(),
            network: "mainnet".to_owned(),
        };
        let _ = app.on_reply(Purpose::Networks, Reply::Networks(vec![mainnet.clone()]));
        assert_eq!(app.model.node.network, Some(mainnet));
        let _ = app.on_reply(Purpose::Mempool, Reply::Mempool(Ok(sample::mempool())));
        assert!(!app.model.mempool.reading);
        assert!(matches!(&app.model.mempool.last, Some(Ok(m)) if m.waiting == 14));

        // A node that stops answering: no tip, so no solve time or sync
        // state either. The network it serves is still the one it said.
        let _ = app.on_reply(
            Purpose::Network {
                sent: Instant::now(),
            },
            Reply::Refused(Refusal {
                kind: RefusalKind::Library,
                text: "connection refused".to_owned(),
            }),
        );
        assert_eq!(app.model.node.solved_ms, None);
        assert_eq!(app.model.node.sync, None);
        assert!(app.model.node.network.is_some());

        // Known, the network is not asked for again.
        let before = waiting(&app).len();
        app.model.blocks.reading = false;
        app.model.mempool.reading = false;
        app.ask_tip();
        assert!(!waiting(&app)[before..].contains(&"Networks".to_owned()));

        // Another node: its own network and its own queue.
        let _ = app.on_reply(
            Purpose::SetNode { form: true },
            Reply::NodeSet {
                url: "https://other.example".to_owned(),
                view: None,
            },
        );
        assert!(app.model.node.network.is_none() && app.model.mempool.last.is_none());
    }

    #[test]
    fn older_activity_is_read_on_from_each_account_with_more_and_added_below() {
        use tawara_wallet_core::sample;
        let scratch = Scratch::new("older-activity");
        let (mut app, _events) = app(&scratch);
        app.model.wallet = Some(sample::wallet_view());
        app.model.node.url = Some("https://node.example".to_owned());
        let mut histories = sample::activity();
        app.model.activity.last = Some(Ok(histories.clone()));
        wallet(&mut app, WalletMsg::ReadOlderActivity);
        assert!(
            waiting(&app).is_empty(),
            "every history is whole: nothing to read"
        );

        histories[0].total = histories[0].next + 40;
        app.model.activity.last = Some(Ok(histories.clone()));
        wallet(&mut app, WalletMsg::ReadOlderActivity);
        assert_eq!(waiting(&app), ["OlderActivity"]);
        assert!(app.model.activity.reading);
        wallet(&mut app, WalletMsg::ReadOlderActivity);
        assert_eq!(waiting(&app), ["OlderActivity"], "one read at a time");

        // A refusal (a cancel) leaves the rows as they were.
        let _ = app.on_reply(
            Purpose::OlderActivity,
            Reply::Refused(Refusal {
                kind: RefusalKind::Cancelled,
                text: "cancelled".to_owned(),
            }),
        );
        assert!(!app.model.activity.reading);
        assert!(matches!(&app.model.activity.last, Some(Ok(h)) if *h == histories));

        // The page is added below its own account's rows, each once.
        let mut page = histories[0].clone();
        let repeated = page.transactions[0].clone();
        let mut older = page.transactions[1].clone();
        older.id = "older".to_owned();
        older.block = Some(1);
        page.transactions = vec![repeated, older];
        page.next = histories[0].total;
        page.text = "the older page".to_owned();
        let rows = histories[0].transactions.len();
        app.model.activity.reading = true;
        let _ = app.on_reply(Purpose::OlderActivity, Reply::Activity(Ok(vec![page])));
        assert!(!app.model.activity.reading);
        let Some(Ok(now)) = &app.model.activity.last else {
            panic!("the rows read");
        };
        assert_eq!(now[0].transactions.len(), rows + 1);
        assert_eq!(
            now[0].transactions.last().map(|t| t.id.as_str()),
            Some("older")
        );
        assert!(!now[0].more());
        assert!(now[0].text.ends_with("the older page"));
        assert_eq!(now[1..], histories[1..], "the other accounts as they were");
    }

    #[test]
    fn settings_change_the_unit_and_the_auto_lock_and_keep_them() {
        use tawara_wallet_core::preferences::AmountUnit;
        let mut app = on_wallet(Page::Settings(SettingsPage::default()));
        wallet(&mut app, WalletMsg::Unit(AmountUnit::NanoMcm));
        assert_eq!(app.model.prefs.unit, AmountUnit::NanoMcm);
        wallet(&mut app, WalletMsg::AutoLock(10));
        assert_eq!(app.model.prefs.idle_lock, Duration::from_secs(600));
        wallet(&mut app, WalletMsg::AutoLock(7));
        assert_eq!(
            app.model.prefs.idle_lock,
            Duration::from_secs(600),
            "a period not offered is not taken"
        );
        wallet(&mut app, WalletMsg::SaveNode);
        assert!(wallet_page(&app).error.is_some(), "no address typed");
    }

    #[test]
    fn recovery_shows_every_report_whole_and_advances_only_as_typed_and_confirmed() {
        let reports = tawara_wallet_core::sample::review();
        let paused = reports[2].account;
        let reconciled = reports[0].account;
        let mut app = on_wallet(Page::Account(AccountPage {
            account: paused,
            report: None,
        }));
        wallet(&mut app, WalletMsg::Open(To::Recovery(Some(paused))));
        assert!(matches!(&wallet_page(&app).page,
            Page::Recovery(r) if r.target == Some(paused) && r.reports.is_none()));

        let _ = app.on_reply(Purpose::Review, Reply::Reviewed(reports.clone()));
        let p = wallet_page(&app);
        assert!(matches!(&p.page, Page::Recovery(r) if r.reports.as_ref() == Some(&reports)));
        for n in 0..3 {
            assert_eq!(
                p.open.get(&ReportKey::Review(n)),
                Some(&Level::Summary),
                "every account's report opens to its summary"
            );
        }
        let unread = |app: &App| match &wallet_page(app).page {
            Page::Recovery(r) => r.unread(),
            _ => None,
        };
        assert_eq!(unread(&app), Some(3));
        let row = |app: &App, id| {
            app.model
                .wallet
                .as_ref()
                .unwrap()
                .accounts
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .state
                .clone()
        };
        assert_eq!(
            row(&app, paused),
            reports[2].state,
            "the view follows the review"
        );

        let error = |app: &App| wallet_page(app).error.clone().unwrap_or_default();
        // Typed and confirmed, and not every full report opened: refused,
        // whichever are open (D29).
        wallet(&mut app, WalletMsg::RecoveryIndex("33".to_owned()));
        wallet(&mut app, WalletMsg::Confirm(true));
        wallet(&mut app, WalletMsg::Advance);
        assert!(error(&app).starts_with("Open every"), "{}", error(&app));
        wallet(
            &mut app,
            WalletMsg::Report(ReportKey::Review(2), Level::Full),
        );
        wallet(
            &mut app,
            WalletMsg::Report(ReportKey::Review(0), Level::Full),
        );
        // A summary opened is not the full output.
        wallet(
            &mut app,
            WalletMsg::Report(ReportKey::Review(1), Level::Summary),
        );
        assert_eq!(unread(&app), Some(1));
        wallet(&mut app, WalletMsg::Advance);
        assert!(error(&app).starts_with("Open every"), "{}", error(&app));
        wallet(
            &mut app,
            WalletMsg::Report(ReportKey::Review(1), Level::Full),
        );
        assert_eq!(unread(&app), Some(0));
        // Closing a report read leaves it read.
        wallet(
            &mut app,
            WalletMsg::Report(ReportKey::Review(1), Level::Closed),
        );
        assert_eq!(unread(&app), Some(0));
        wallet(&mut app, WalletMsg::Confirm(false));
        wallet(&mut app, WalletMsg::RecoveryIndex(String::new()));
        // Nothing typed.
        wallet(&mut app, WalletMsg::Advance);
        assert!(error(&app).starts_with("Confirm"), "{}", error(&app));
        // Typed and not confirmed.
        wallet(&mut app, WalletMsg::RecoveryIndex("33".to_owned()));
        wallet(&mut app, WalletMsg::Advance);
        assert!(error(&app).starts_with("Confirm"), "{}", error(&app));
        // Confirmed and not a number.
        wallet(&mut app, WalletMsg::Confirm(true));
        wallet(
            &mut app,
            WalletMsg::RecoveryIndex("thirty-three".to_owned()),
        );
        wallet(&mut app, WalletMsg::Advance);
        assert!(
            error(&app).starts_with("Type the key index"),
            "{}",
            error(&app)
        );
        // Typed and confirmed: sent, so the page holds no error.
        wallet(&mut app, WalletMsg::RecoveryIndex("33".to_owned()));
        wallet(&mut app, WalletMsg::Advance);
        assert_eq!(wallet_page(&app).error, None);

        // An account whose report names no index is never advanced.
        wallet(&mut app, WalletMsg::Target(reconciled));
        assert!(matches!(&wallet_page(&app).page,
            Page::Recovery(r) if r.index.is_empty() && !r.confirmed));
        wallet(&mut app, WalletMsg::Confirm(true));
        wallet(&mut app, WalletMsg::RecoveryIndex("50".to_owned()));
        wallet(&mut app, WalletMsg::Advance);
        assert!(error(&app).contains("names no index"), "{}", error(&app));

        // The advance answered: its page, the store as it is now, and the
        // form cleared.
        let mut after = tawara_wallet_core::sample::wallet_view();
        after.accounts[2].state = tawara_wallet_core::view::AccountState::InSync { balance: 1 };
        let _ = app.on_reply(
            Purpose::Reconcile,
            Reply::Reconciled {
                ok: true,
                advanced_to: Some(33),
                text: "advanced".to_owned(),
                opened: Ok(after.clone()),
            },
        );
        assert_eq!(app.model.wallet.as_ref(), Some(&after));
        assert!(matches!(&wallet_page(&app).page,
            Page::Recovery(r) if r.result == Some((true, "advanced".to_owned()))
                && r.index.is_empty() && !r.confirmed));

        // A further search answers with that account's report, replacing it;
        // a report that changed folds to its summary and is to be read
        // again.
        assert_eq!(unread(&app), Some(0));
        let _ = app.on_reply(
            Purpose::Status,
            Reply::Status {
                account: paused,
                state: tawara_wallet_core::view::AccountState::InSync { balance: 1 },
                spendable: false,
                text: "found".to_owned(),
                settled: None,
            },
        );
        assert!(matches!(&wallet_page(&app).page,
            Page::Recovery(r) if r.reports.as_ref().is_some_and(|all| all
                .iter()
                .any(|a| a.account == paused && a.text == "found"))));
        assert_eq!(unread(&app), Some(1));
        assert_eq!(
            wallet_page(&app).open.get(&ReportKey::Review(2)),
            Some(&Level::Summary)
        );
        assert_eq!(
            wallet_page(&app).open.get(&ReportKey::Review(0)),
            Some(&Level::Full),
            "the other reports stay as they were"
        );

        // Read again: every report is to be opened again.
        let _ = app.on_reply(Purpose::Review, Reply::Reviewed(reports.clone()));
        assert_eq!(unread(&app), Some(3));
    }

    #[test]
    fn receive_shows_the_destination_the_worker_gives() {
        let scratch = Scratch::new("receive");
        let (mut app, events) = app(&scratch);
        let _ = app.update(Message::Choose(StartChoice::Restore));
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::PhraseText(typed(PHRASE)));
        let _ = app.update(Message::RestoreWallet);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
        let first = app.model.wallet.as_ref().unwrap().accounts[0].id;
        wallet(&mut app, WalletMsg::Open(To::Receive(None)));
        pump(&mut app, &events, |a| {
            matches!(
                &wallet_page(a).page,
                Page::Receive(ReceivePage { view: Some(_), .. })
            )
        });
        let Page::Receive(ReceivePage {
            view: Some(view), ..
        }) = &wallet_page(&app).page
        else {
            unreachable!()
        };
        assert_eq!(view.account, first);
        assert_eq!(view.destination, first.destination());
    }

    #[test]
    fn a_secret_typed_is_never_in_a_message_log() {
        let message = Message::Password(typed("hunter2-hunter2"));
        assert!(!format!("{message:?}").contains("hunter2"));
    }

    fn explorer(e: ExplorerPage) -> App {
        on_wallet(Page::Explorer(e))
    }

    fn search(app: &mut App, text: &str) {
        wallet(app, WalletMsg::ExplorerTyped(text.to_owned()));
        wallet(app, WalletMsg::ExplorerFind);
    }

    /// An account in no sample store: the sample block's first payee.
    fn payee() -> AccountId {
        tawara_wallet_core::sample::block().spends[0]
            .destinations()
            .find_map(|o| match o.party {
                tawara_wallet_core::explorer::Party::Account(id) => Some(id),
                _ => None,
            })
            .expect("a payee")
    }

    #[test]
    fn the_explorers_search_opens_a_block_or_an_account_or_says_why_not() {
        use tawara_wallet_core::explorer::BlockAt;
        let mut app = explorer(ExplorerPage::default());
        search(&mut app, "0");
        match &wallet_page(&app).page {
            Page::Explorer(e) => {
                assert!(e.invalid.as_deref().is_some_and(|w| w.contains("block 0")));
            }
            other => panic!("expected the explorer, got {other:?}"),
        }
        // Typing again puts the refusal away.
        wallet(&mut app, WalletMsg::ExplorerTyped("871172".to_owned()));
        match &wallet_page(&app).page {
            Page::Explorer(e) => assert_eq!(e.invalid, None),
            other => panic!("expected the explorer, got {other:?}"),
        }
        wallet(&mut app, WalletMsg::ExplorerFind);
        match &wallet_page(&app).page {
            Page::Block(b) => {
                assert_eq!(b.at, BlockAt::Number(871_172));
                assert!(b.read.is_none(), "read when it comes");
            }
            other => panic!("expected the block, got {other:?}"),
        }
        let mut app = explorer(ExplorerPage::default());
        search(&mut app, &payee().destination().expect("an address"));
        match &wallet_page(&app).page {
            Page::Tag(t) => assert!(t.account == payee() && t.typed, "typed"),
            other => panic!("expected the account, got {other:?}"),
        }
    }

    #[test]
    fn what_a_hash_names_is_opened_and_a_miss_stays_on_the_explorer() {
        use tawara_wallet_core::explorer::{BlockAt, Found};
        use tawara_wallet_core::sample;
        const A: [u8; 32] = [1; 32];
        const B: [u8; 32] = [2; 32];
        let finding = |hash| {
            explorer(ExplorerPage {
                finding: Some(hash),
                ..ExplorerPage::default()
            })
        };
        let (tx, text) = sample::transaction();
        let mut app = finding(A);
        let _ = app.on_reply(
            Purpose::Find(A),
            Reply::Found(Box::new(Found::Transaction {
                transaction: tx.clone(),
                text,
            })),
        );
        match &wallet_page(&app).page {
            Page::Transaction(t) => {
                assert_eq!(t.transaction, tx);
                assert!(t.from_index && t.text.is_some());
            }
            other => panic!("expected the transaction, got {other:?}"),
        }
        let mut app = finding(A);
        let _ = app.on_reply(
            Purpose::Find(A),
            Reply::Found(Box::new(Found::Block(Box::new(sample::block())))),
        );
        match &wallet_page(&app).page {
            Page::Block(b) => {
                assert_eq!(b.at, BlockAt::Number(871_172), "read again by its number");
                assert!(matches!(b.read, Some(Ok(_))));
            }
            other => panic!("expected the block, got {other:?}"),
        }
        let mut app = finding(A);
        let (transaction, block) = sample::not_found([1; 32]);
        let _ = app.on_reply(
            Purpose::Find(A),
            Reply::Found(Box::new(Found::Neither {
                searched: true,
                transaction,
                block,
            })),
        );
        match &wallet_page(&app).page {
            Page::Explorer(e) => {
                assert!(e.finding.is_none());
                assert!(e.missed.as_ref().is_some_and(|m| m.searched));
            }
            other => panic!("expected the explorer, got {other:?}"),
        }
        // The explorer opened afresh since it asked: nothing waits there.
        let mut app = explorer(ExplorerPage::default());
        let (tx, text) = sample::transaction();
        let _ = app.on_reply(
            Purpose::Find(A),
            Reply::Found(Box::new(Found::Transaction {
                transaction: tx,
                text,
            })),
        );
        assert!(matches!(wallet_page(&app).page, Page::Explorer(_)));
        // Another page opened since: the answer has nowhere to go.
        let mut app = on_wallet(Page::Dashboard);
        let (tx, text) = sample::transaction();
        let _ = app.on_reply(
            Purpose::Find(A),
            Reply::Found(Box::new(Found::Transaction {
                transaction: tx,
                text,
            })),
        );
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
        // The explorer opened again while A was looked for, and B asked
        // since: A's answer is not B's, and B's is still taken.
        let mut app = finding(B);
        let (tx, text) = sample::transaction();
        let _ = app.on_reply(
            Purpose::Find(A),
            Reply::Found(Box::new(Found::Transaction {
                transaction: tx.clone(),
                text: text.clone(),
            })),
        );
        match &wallet_page(&app).page {
            Page::Explorer(e) => assert_eq!(e.finding, Some(B), "still waiting on B"),
            other => panic!("expected the explorer, got {other:?}"),
        }
        let _ = app.on_reply(
            Purpose::Find(A),
            Reply::Refused(Refusal {
                kind: RefusalKind::Library,
                text: "A's refusal".to_owned(),
            }),
        );
        assert!(wallet_page(&app).error.is_none(), "A's refusal is not B's");
        let _ = app.on_reply(
            Purpose::Find(B),
            Reply::Found(Box::new(Found::Transaction {
                transaction: tx.clone(),
                text,
            })),
        );
        match &wallet_page(&app).page {
            Page::Transaction(t) => assert_eq!(t.transaction, tx),
            other => panic!("expected B's transaction, got {other:?}"),
        }
    }

    #[test]
    fn a_block_or_an_account_is_read_onto_the_page_that_asked_for_it() {
        use tawara_wallet_core::explorer::{AccountHistory, BlockAt, TransactionView};
        use tawara_wallet_core::sample;
        let mut app = on_wallet(Page::Block(BlockPage {
            at: BlockAt::Number(871_172),
            read: None,
            rows_from: 0,
        }));
        let _ = app.on_reply(
            Purpose::Block(BlockAt::Number(871_171)),
            Reply::Block(Ok(Box::new(sample::block()))),
        );
        assert!(
            matches!(&wallet_page(&app).page, Page::Block(b) if b.read.is_none()),
            "another block's answer"
        );
        let _ = app.on_reply(
            Purpose::Block(BlockAt::Number(871_172)),
            Reply::Block(Ok(Box::new(sample::block()))),
        );
        assert!(matches!(&wallet_page(&app).page, Page::Block(b) if b.read.is_some()));
        wallet(&mut app, WalletMsg::BlockRows(BLOCK_ROWS));
        assert!(matches!(&wallet_page(&app).page, Page::Block(b) if b.rows_from == BLOCK_ROWS));
        wallet(&mut app, WalletMsg::OpenSpend(0));
        match &wallet_page(&app).page {
            Page::Transaction(t) => {
                assert_eq!(t.transaction, sample::block().spends[0]);
                assert!(!t.from_index, "as its block lists it");
                assert!(t.tip.is_some(), "the tip read with its block");
                assert_eq!(t.tip, sample::block().tip);
            }
            other => panic!("expected the transaction, got {other:?}"),
        }

        let mut view = sample::tag_view();
        let account = view.account;
        if let Ok(h) = &mut view.history {
            h.total = h.transactions.len() as u64 + 1;
        }
        let mut app = on_wallet(Page::Tag(TagPage {
            account,
            read: None,
            reading_older: false,
            typed: false,
        }));
        let _ = app.on_reply(Purpose::Tag(payee()), Reply::Tag(Box::new(view.clone())));
        assert!(
            matches!(&wallet_page(&app).page, Page::Tag(t) if t.read.is_none()),
            "another account's answer"
        );
        let _ = app.on_reply(Purpose::Tag(account), Reply::Tag(Box::new(view)));
        // An older page goes below the rows read; a refusal leaves them.
        if let Screen::Wallet(WalletPage {
            page: Page::Tag(t), ..
        }) = &mut app.model.screen
        {
            t.reading_older = true;
        }
        let rows = |app: &App| match &wallet_page(app).page {
            Page::Tag(TagPage {
                read: Some(v),
                reading_older,
                ..
            }) => (
                v.history.as_ref().map_or(0, |h| h.transactions.len()),
                *reading_older,
            ),
            other => panic!("expected the account, got {other:?}"),
        };
        let before = rows(&app).0;
        let _ = app.on_reply(
            Purpose::TagHistory(account),
            Reply::TagHistory(Err(sample::index_refusal(
                tawara_wallet_core::explorer::IndexState::Unavailable,
            ))),
        );
        assert_eq!(rows(&app), (before, false));
        let older = AccountHistory {
            account,
            transactions: vec![TransactionView {
                id: "older".to_owned(),
                block: Some(1),
                time_ms: None,
                operations: Vec::new(),
            }],
            total: before as u64 + 1,
            next: before as u64 + 1,
            text: String::new(),
        };
        let _ = app.on_reply(Purpose::TagHistory(account), Reply::TagHistory(Ok(older)));
        assert_eq!(rows(&app), (before + 1, false));
        wallet(&mut app, WalletMsg::OpenTagRow(before));
        match &wallet_page(&app).page {
            Page::Transaction(t) => {
                assert_eq!(t.transaction.id, "older");
                assert!(t.from_index);
            }
            other => panic!("expected the transaction, got {other:?}"),
        }
    }

    #[test]
    fn the_whole_queue_is_read_onto_its_page_and_opens_each_transaction() {
        use tawara_wallet_core::sample;
        let mut app = explorer(ExplorerPage::default());
        wallet(&mut app, WalletMsg::Open(To::Queue));
        assert!(matches!(&wallet_page(&app).page, Page::Queue(q) if q.read.is_none()));
        let _ = app.on_reply(Purpose::Queue, Reply::Pending(Ok(sample::queue())));
        match &wallet_page(&app).page {
            Page::Queue(QueuePage { read: Some(Ok(p)) }) => {
                assert_eq!(p.rows.len(), p.waiting, "every one read whole");
            }
            other => panic!("expected the queue, got {other:?}"),
        }
        // A row opens from the page's own read, not the overview's.
        wallet(&mut app, WalletMsg::OpenPending(13));
        match &wallet_page(&app).page {
            Page::Transaction(t) => {
                let last = sample::queue().rows[13].transaction.clone();
                assert_eq!(Some(t.transaction.clone()), last);
                assert!(!t.from_index, "as the queue lists it");
            }
            other => panic!("expected the transaction, got {other:?}"),
        }
        // The queue's answer goes nowhere once its page is left.
        let mut app = on_wallet(Page::Dashboard);
        let _ = app.on_reply(Purpose::Queue, Reply::Pending(Ok(sample::queue())));
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
    }

    #[test]
    fn sending_to_an_account_is_offered_only_for_one_the_person_typed() {
        let tag = |typed| {
            on_wallet(Page::Tag(TagPage {
                account: payee(),
                read: None,
                reading_older: false,
                typed,
            }))
        };
        // Reached through a link: the node named it, so nothing opens.
        let mut app = tag(false);
        wallet(&mut app, WalletMsg::SendTo(payee()));
        assert!(matches!(wallet_page(&app).page, Page::Tag(_)));
        // Typed, but asked for another account than the page shows.
        let mut app = tag(true);
        let other = tawara_wallet_core::sample::wallet_view().accounts[0].id;
        wallet(&mut app, WalletMsg::SendTo(other));
        assert!(matches!(wallet_page(&app).page, Page::Tag(_)));
        let mut app = tag(true);
        wallet(&mut app, WalletMsg::SendTo(payee()));
        match &wallet_page(&app).page {
            Page::Send(s) => {
                assert_eq!(s.form.rows.len(), 1);
                assert_eq!(Some(s.form.rows[0].to.clone()), payee().destination());
            }
            other => panic!("expected the send form, got {other:?}"),
        }
    }

    #[test]
    fn the_explorers_chain_is_kept_until_the_node_changes() {
        use tawara_wallet_core::sample;
        let mut app = alone();
        app.model.clock_ms = 1_234;
        app.model.chain.reading = true;
        let _ = app.on_reply(Purpose::Chain, Reply::Chain(Ok(sample::chain())));
        assert!(!app.model.chain.reading);
        assert_eq!(app.model.chain.read_ms, Some(1_234));
        assert!(matches!(app.model.chain.last, Some(Ok(_))));
        let _ = app.on_reply(Purpose::Pending, Reply::Pending(Ok(sample::pending())));
        assert!(matches!(app.model.pending.last, Some(Ok(_))));
        let _ = app.on_reply(
            Purpose::SetNode { form: false },
            Reply::NodeSet {
                url: "https://another.example".to_owned(),
                view: None,
            },
        );
        assert!(app.model.chain.last.is_none() && app.model.pending.last.is_none());
    }

    /// The sample store's second account: outstanding, with a live reservation.
    fn live_account() -> AccountId {
        tawara_wallet_core::sample::wallet_view().accounts[1].id
    }

    /// The worker has answered every command sent so far: its queue is empty.
    fn answered(app: &mut App) {
        if let Some(w) = app.worker.as_mut() {
            w.waiting.clear();
        }
    }

    #[test]
    fn a_spend_signed_in_this_run_is_polled_whatever_its_account_watched_before() {
        // An unlock that finds the account in sync ends its polls (docs/DECISIONS.md
        // D38 item 2). A spend signed after it holds a reservation whose first read
        // failed, so the account is polled from the sign's reply; a re-sign is
        // polled the same way.
        use tawara_wallet_core::view::{AccountState, ReservationState};
        let mut app = alone();
        let account = tawara_wallet_core::sample::wallet_view().accounts[0].id;
        let polled = |app: &App| app.polled_accounts(Instant::now()).contains(&account);
        let _ = app.on_reply(
            Purpose::Unlock,
            Reply::Unlocked(tawara_wallet_core::sample::wallet_view()),
        );
        assert!(
            !polled(&app),
            "an account an unlock found in sync is not polled"
        );

        // The sign's reply: its first read could not read the spend's expiry.
        let mut sent = tawara_wallet_core::sample::sent_view();
        sent.view.accounts[0].set_state(AccountState::SpendOutstanding {
            balance: 1,
            spent_index: 49,
            reservation: ReservationState::Unclassified,
        });
        let _ = app.on_reply(Purpose::ConfirmSend, Reply::Sent(sent.clone()));
        assert!(polled(&app), "a spend signed in this run is polled");

        // Settled, and in sync again: not polled. Re-signed, with the first read
        // failing again: polled.
        let _ = app.on_reply(
            Purpose::Status,
            Reply::Status {
                account,
                state: AccountState::InSync { balance: 1 },
                spendable: true,
                text: "in sync".to_owned(),
                settled: None,
            },
        );
        assert!(!polled(&app), "a settled account is not polled");
        let _ = app.on_reply(Purpose::Resign, Reply::Sent(sent));
        assert!(polled(&app), "a re-sign is polled as a sign is");
    }

    #[test]
    fn a_poll_asks_about_a_live_reservation_once_a_minute_and_is_not_input() {
        let scratch = Scratch::new("poll");
        let (mut app, _events) = app(&scratch);
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        let touched = app.touches.get();
        // The tick polls, by the monotonic clock.
        let _ = app.update(Message::Tick(100_000));
        assert!(app.waiting_on(|p| matches!(p, Purpose::Poll)), "sent");
        let first = app.model.polled_at.expect("the poll's time is kept");
        assert_eq!(
            app.touches.get(),
            touched,
            "a poll is not the person's input"
        );
        // Not again while the last one is on its way, nor within the minute,
        // whatever the wall clock says: one set back, or forward, moves
        // nothing.
        answered(&mut app);
        let _ = app.update(Message::Tick(0));
        let _ = app.update(Message::Tick(10_000_000));
        assert!(
            !app.waiting_on(|p| matches!(p, Purpose::Poll)),
            "a minute has not passed"
        );
        app.poll(first + Duration::from_secs(30));
        assert!(!app.waiting_on(|p| matches!(p, Purpose::Poll)));
        assert_eq!(app.model.polled_at, Some(first));
        // Again once a minute has passed.
        app.poll(first + Duration::from_secs(60));
        assert!(
            app.waiting_on(|p| matches!(p, Purpose::Poll)),
            "a minute has passed"
        );
        assert_eq!(app.touches.get(), touched, "still not the person's input");
    }

    #[test]
    fn a_window_without_the_focus_polls_nothing_until_it_has_it_again() {
        // D38 item 2: the poll runs only in the foreground. A desktop window
        // behind others, or minimised, keeps the wallet open until the idle
        // lock, and asks the node nothing meanwhile.
        let scratch = Scratch::new("poll-focus");
        let (mut app, _events) = app(&scratch);
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        let touched = app.touches.get();
        let now = Instant::now();
        assert_eq!(
            person_input(
                IcedEvent::Window(iced::window::Event::Unfocused),
                event::Status::Ignored,
                iced::window::Id::unique(),
            )
            .map(|m| matches!(m, Message::Foreground(false))),
            Some(true)
        );
        let _ = app.update(Message::Foreground(false));
        assert_eq!(app.touches.get(), touched, "not the person's input");
        app.poll(now);
        assert!(!app.waiting_on(|p| matches!(p, Purpose::Poll)));
        assert_eq!(app.polled_accounts(now), Vec::<AccountId>::new());
        assert_eq!(app.model.polled_at, None);
        // Back in the foreground, it polls at the next tick.
        assert_eq!(
            person_input(
                IcedEvent::Window(iced::window::Event::Focused),
                event::Status::Ignored,
                iced::window::Id::unique(),
            )
            .map(|m| matches!(m, Message::Foreground(true))),
            Some(true)
        );
        let _ = app.update(Message::Foreground(true));
        assert_eq!(app.touches.get(), touched, "not the person's input");
        let _ = app.update(Message::Tick(now_ms()));
        assert!(app.waiting_on(|p| matches!(p, Purpose::Poll)), "sent");
    }

    #[test]
    fn a_poll_asks_about_a_live_reservation_and_stops_on_lock() {
        use tawara_wallet_core::view::{AccountState, DivergenceKind, ReservationState};
        let scratch = Scratch::new("poll-stops");
        let (mut app, _events) = app(&scratch);
        let account = live_account();
        let outstanding = |reservation| AccountState::SpendOutstanding {
            balance: 1,
            spent_index: 6,
            reservation,
        };
        let cases = [
            ("live", outstanding(ReservationState::Live), vec![account]),
            (
                "dead, the balance moved",
                outstanding(ReservationState::Dead {
                    balance_moved: true,
                    expired: false,
                }),
                vec![],
            ),
            (
                "dead, expired",
                outstanding(ReservationState::Dead {
                    balance_moved: false,
                    expired: true,
                }),
                vec![],
            ),
            (
                "not known to be live",
                outstanding(ReservationState::Unrecorded),
                vec![],
            ),
            (
                "landed, with no read noted",
                AccountState::SpendLanded {
                    balance: 1,
                    spent_index: 6,
                    settled_index: 7,
                },
                vec![],
            ),
            ("in sync", AccountState::InSync { balance: 1 }, vec![]),
            (
                "unreachable, with no read noted",
                AccountState::Diverged {
                    kind: DivergenceKind::Unreachable,
                    report: "no answer".to_owned(),
                    advance_to: None,
                },
                vec![],
            ),
        ];
        // Each state is put on the row with no read of it noted in the watch,
        // so only a live reservation is polled. What a noted read starts is
        // for the tests after this one.
        for (label, state, polled) in cases {
            let mut wallet = tawara_wallet_core::sample::wallet_view();
            wallet.accounts[1].state = state;
            app.model.wallet = Some(wallet);
            app.model.polled_at = None;
            assert_eq!(app.polled_accounts(Instant::now()), polled, "{label}");
        }

        // With no wallet open there is nothing to poll.
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        app.model.polled_at = None;
        assert_eq!(app.polled_accounts(Instant::now()), vec![account]);
        app.model.settled_waiting.insert(account, "kept".to_owned());
        let _ = app.on_event(Event::Locked {
            reason: LockReason::Background,
        });
        assert!(app.model.wallet.is_none());
        assert!(
            app.model.settled_waiting.is_empty(),
            "a lock drops the banners"
        );
        assert_eq!(app.polled_accounts(Instant::now()), Vec::<AccountId>::new());
        let _ = app.update(Message::Tick(500_000));
        assert!(
            !app.waiting_on(|p| matches!(p, Purpose::Poll)),
            "nothing is sent"
        );
    }

    #[test]
    fn a_poll_keeps_asking_through_transient_failures_after_a_live_read_and_drops_on_any_other() {
        use tawara_wallet_core::view::{
            AccountState, DivergenceKind, ReservationState, SettledAccount,
        };
        let scratch = Scratch::new("poll-watch");
        let (mut app, _events) = app(&scratch);
        let account = live_account();
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        let touched = app.touches.get();
        let outstanding = |reservation: ReservationState| AccountState::SpendOutstanding {
            balance: 3_180_000_000_000,
            spent_index: 6,
            reservation,
        };
        let live = outstanding(ReservationState::Live);
        let silent = AccountState::Diverged {
            kind: DivergenceKind::Unreachable,
            report: "no answer".to_owned(),
            advance_to: None,
        };
        // The worker's answer to a poll of the account, taken as the app takes it.
        let answer = |app: &mut App, state: AccountState, settled: Option<SettledAccount>| {
            let _ = app.on_reply(
                Purpose::Poll,
                Reply::Status {
                    account,
                    state,
                    spendable: false,
                    text: "checked".to_owned(),
                    settled,
                },
            );
        };
        // Whether the next tick sends a poll: the last one's time is forgotten,
        // so that a poll is due, and its answer is taken.
        let polls = |app: &mut App| {
            app.model.polled_at = None;
            answered(app);
            let _ = app.update(Message::Tick(now_ms()));
            let sent = app.waiting_on(|p| matches!(p, Purpose::Poll));
            answered(app);
            sent
        };
        // The first read the app takes of the account here shows its reservation
        // live. A wallet opened while the node fails for the account is the
        // next test's.
        answer(&mut app, live.clone(), None);
        assert!(polls(&mut app), "a live reservation is polled");

        // The node does not answer for the account, and is asked again.
        answer(&mut app, silent.clone(), None);
        assert!(
            polls(&mut app),
            "a node that does not answer does not stop the polls"
        );
        // The expiry cannot be read, and is asked again.
        answer(&mut app, outstanding(ReservationState::Unclassified), None);
        assert!(
            polls(&mut app),
            "an expiry that cannot be read does not stop the polls"
        );
        // The reservation is live again.
        answer(&mut app, live.clone(), None);
        assert!(polls(&mut app));

        // A real divergence ends the polls, and a node that then does not
        // answer does not restart them.
        answer(
            &mut app,
            AccountState::Diverged {
                kind: DivergenceKind::ReservationUnexplained,
                report: "unexplained".to_owned(),
                advance_to: None,
            },
            None,
        );
        assert!(!polls(&mut app), "a divergence is not polled");
        answer(&mut app, silent.clone(), None);
        assert!(!polls(&mut app), "nor is a node that then does not answer");
        // A live read starts them again.
        answer(&mut app, live.clone(), None);
        assert!(polls(&mut app));

        // The spend lands and the poll settles it: nothing is polled afterwards.
        answer(
            &mut app,
            AccountState::InSync {
                balance: 3_180_000_000_000,
            },
            Some(SettledAccount {
                account,
                text: "settled: the library's page".to_owned(),
            }),
        );
        assert!(!polls(&mut app), "a settled account is not polled");
        answer(&mut app, silent.clone(), None);
        assert!(!polls(&mut app));

        // A lock forgets the polls, and the next unlock starts from its reads.
        answer(&mut app, live.clone(), None);
        assert!(polls(&mut app));
        let _ = app.on_event(Event::Locked {
            reason: LockReason::Background,
        });
        assert!(app.model.wallet.is_none());
        // The row says the store holds no reservation, so only what the watch
        // kept from before the lock could poll it.
        let mut wallet = tawara_wallet_core::sample::wallet_view();
        wallet.accounts[1].state = silent.clone();
        wallet.accounts[1].reserved = false;
        let _ = app.on_reply(Purpose::Refresh, Reply::Wallet(wallet));
        assert!(
            !polls(&mut app),
            "a node that does not answer after a lock does not restart the polls"
        );
        assert_eq!(
            app.touches.get(),
            touched,
            "a poll is not the person's input"
        );
    }

    #[test]
    fn a_poll_asks_about_a_spend_a_failed_first_read_left_and_bounds_a_spend_in_doubt() {
        use tawara_wallet_core::view::{AccountState, DivergenceKind, ReservationState};
        let scratch = Scratch::new("poll-first-read");
        let (mut app, _events) = app(&scratch);
        let account = live_account();
        let diverged = |kind| AccountState::Diverged {
            kind,
            report: "the library's report".to_owned(),
            advance_to: None,
        };
        let silent = diverged(DivergenceKind::Unreachable);
        let not_found = diverged(DivergenceKind::NotFound);
        let landed = AccountState::SpendLanded {
            balance: 2_640_000_000_000,
            spent_index: 6,
            settled_index: 7,
        };
        let live = AccountState::SpendOutstanding {
            balance: 3_180_000_000_000,
            spent_index: 6,
            reservation: ReservationState::Live,
        };
        let answer = |app: &mut App, state: AccountState| {
            let _ = app.on_reply(
                Purpose::Poll,
                Reply::Status {
                    account,
                    state,
                    spendable: false,
                    text: "checked".to_owned(),
                    settled: None,
                },
            );
        };
        let polls = |app: &mut App| {
            app.model.polled_at = None;
            answered(app);
            let _ = app.update(Message::Tick(now_ms()));
            let sent = app.waiting_on(|p| matches!(p, Purpose::Poll));
            answered(app);
            sent
        };
        // A wallet opens while the node does not answer, or answers "account
        // not found", for the account whose spend is outstanding: the store
        // holds its reservation, so it is polled from that first read.
        for first in [&silent, &not_found] {
            let _ = app.on_event(Event::Locked {
                reason: LockReason::Asked,
            });
            let mut wallet = tawara_wallet_core::sample::wallet_view();
            wallet.accounts[1].state = first.clone();
            assert!(wallet.accounts[1].reserved, "the sample's spend");
            let _ = app.on_reply(Purpose::Refresh, Reply::Wallet(wallet));
            assert!(polls(&mut app), "{first:?} at open");
            answer(&mut app, silent.clone());
            assert!(polls(&mut app), "{first:?}, then no answer");
        }

        // "Account not found" leaves the spend in doubt: it is asked about twice
        // more in a row, and the third such read ends the polls.
        answer(&mut app, live.clone());
        for n in 0..2 {
            answer(&mut app, not_found.clone());
            assert!(polls(&mut app), "not found, poll {n}");
        }
        answer(&mut app, not_found.clone());
        assert!(!polls(&mut app), "the third read not found ends the polls");
        answer(&mut app, silent.clone());
        assert!(!polls(&mut app), "and a silent one does not restart them");

        // A landed spend the automatic settle did not make is asked about in the
        // same way, so a settle the node interrupted is tried again.
        answer(&mut app, live.clone());
        for n in 0..2 {
            answer(&mut app, landed.clone());
            assert!(polls(&mut app), "landed, poll {n}");
        }
        answer(&mut app, landed.clone());
        assert!(!polls(&mut app), "the third landed read ends the polls");

        // A row whose store holds no reservation is not polled on a failed read.
        let _ = app.on_event(Event::Locked {
            reason: LockReason::Asked,
        });
        let mut wallet = tawara_wallet_core::sample::wallet_view();
        wallet.accounts[1].state = silent.clone();
        wallet.accounts[1].reserved = false;
        let _ = app.on_reply(Purpose::Refresh, Reply::Wallet(wallet));
        assert!(!polls(&mut app));
    }

    #[test]
    fn a_settlement_a_poll_or_a_check_finds_is_the_banner_on_its_page_or_waits_for_it() {
        use tawara_wallet_core::view::{AccountState, SettledAccount};
        let account = live_account();
        let in_sync = AccountState::InSync {
            balance: 3_180_000_000_000,
        };
        let answer = |purpose: Purpose, settled: Option<SettledAccount>, text: &str| {
            (
                purpose,
                Reply::Status {
                    account,
                    state: in_sync.clone(),
                    spendable: true,
                    text: text.to_owned(),
                    settled,
                },
            )
        };
        let settled = || {
            Some(SettledAccount {
                account,
                text: "the library's page".to_owned(),
            })
        };
        let banner = |app: &App| match &wallet_page(app).page {
            Page::Account(a) => a.report.clone(),
            _ => None,
        };
        let said = Some((Done::Settled, "the library's page".to_owned()));

        // On the account's own page the banner is shown at once.
        let mut app = on_wallet(Page::Account(AccountPage {
            account,
            report: None,
        }));
        let (purpose, reply) = answer(Purpose::Poll, settled(), "checked");
        let _ = app.on_reply(purpose, reply);
        assert_eq!(banner(&app), said);
        assert!(app.model.settled_waiting.is_empty());
        assert_eq!(
            app.model.wallet.as_ref().unwrap().accounts[1].state,
            in_sync,
            "the row shows the state the worker reports"
        );

        // Elsewhere it waits for the account's page, and is shown once that is
        // opened, and then only once.
        let mut app = on_wallet(Page::Dashboard);
        let (purpose, reply) = answer(Purpose::Poll, settled(), "checked");
        let _ = app.on_reply(purpose, reply);
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
        wallet(&mut app, WalletMsg::Open(To::Account(account)));
        assert_eq!(banner(&app), said);
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        wallet(&mut app, WalletMsg::Open(To::Account(account)));
        assert_eq!(banner(&app), None);

        // A check that settles the account says so, in place of what it found;
        // a check that settles nothing says what it found.
        let mut app = on_wallet(Page::Account(AccountPage {
            account,
            report: None,
        }));
        let (purpose, reply) = answer(Purpose::Status, settled(), "checked");
        let _ = app.on_reply(purpose, reply);
        assert_eq!(banner(&app), said);
        let (purpose, reply) = answer(Purpose::Status, None, "checked again");
        let _ = app.on_reply(purpose, reply);
        assert_eq!(
            banner(&app),
            Some((Done::Checked, "checked again".to_owned()))
        );
    }

    #[test]
    fn a_refresh_that_settles_an_account_leaves_its_banner_for_its_page() {
        use tawara_wallet_core::view::{AccountState, SettledAccount};
        let account = live_account();
        let mut app = on_wallet(Page::Account(AccountPage {
            account,
            report: None,
        }));
        let mut view = tawara_wallet_core::sample::wallet_view();
        view.accounts[1].state = AccountState::InSync { balance: 1 };
        view.settled = vec![SettledAccount {
            account,
            text: "the library's page".to_owned(),
        }];
        let _ = app.on_reply(Purpose::Refresh, Reply::Wallet(view));
        // A refresh shows the dashboard, as it always has.
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
        wallet(&mut app, WalletMsg::Open(To::Account(account)));
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Account(AccountPage {
                report: Some((Done::Settled, text)),
                ..
            }) if text == "the library's page"
        ));
    }

    #[test]
    fn a_settlement_in_the_answer_to_a_send_or_a_restore_waits_for_its_page() {
        use tawara_wallet_core::view::SettledAccount;
        let account = live_account();
        let settled = vec![SettledAccount {
            account,
            text: "the library's page".to_owned(),
        }];
        // The read after a send settled it: the sent page is shown, and the
        // banner waits for the account's page.
        let mut app = on_wallet(Page::Dashboard);
        let mut sent = tawara_wallet_core::sample::sent_view();
        sent.view.settled.clone_from(&settled);
        let _ = app.on_reply(Purpose::ConfirmSend, Reply::Sent(sent));
        assert!(matches!(wallet_page(&app).page, Page::Send(_)));
        assert_eq!(
            app.model.settled_waiting.get(&account).map(String::as_str),
            Some("the library's page")
        );
        // The reopen after a restore settled it.
        let mut app = on_wallet(Page::Dashboard);
        let mut view = tawara_wallet_core::sample::wallet_view();
        view.settled = settled;
        let _ = app.on_reply(
            Purpose::Restore,
            Reply::Restored {
                ok: true,
                text: "restored".to_owned(),
                opened: Ok(view),
            },
        );
        wallet(&mut app, WalletMsg::Open(To::Account(account)));
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Account(AccountPage {
                report: Some((Done::Settled, text)),
                ..
            }) if text == "the library's page"
        ));
    }

    #[test]
    fn a_poll_of_a_store_open_on_its_own_asks_nothing() {
        let scratch = Scratch::new("poll-store");
        let (mut app, _events) = app(&scratch);
        let mut wallet = tawara_wallet_core::sample::wallet_view();
        wallet.opened = false;
        app.model.wallet = Some(wallet);
        assert_eq!(app.polled_accounts(Instant::now()), Vec::<AccountId>::new());
    }

    #[test]
    fn a_poll_that_is_refused_or_unanswered_says_nothing() {
        let account = live_account();
        let mut app = on_wallet(Page::Account(AccountPage {
            account,
            report: None,
        }));
        let _ = app.on_reply(
            Purpose::Poll,
            Reply::Refused(Refusal {
                kind: RefusalKind::Library,
                text: "the chain did not answer".to_owned(),
            }),
        );
        assert_eq!(wallet_page(&app).error, None, "nothing is shown");
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Account(AccountPage { report: None, .. })
        ));
    }
}
