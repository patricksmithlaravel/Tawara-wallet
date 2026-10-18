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
use crate::screens::phrase_words_drawn;
use iced::futures::channel::mpsc;
use iced::widget::operation;
use iced::{Element, Event as IcedEvent, Subscription, Task, event, keyboard, mouse, touch};
use tawara_wallet_core::explorer::{
    AccountHistory, BlockAt, BlocksView, ChainView, ExplorerRefusal, MempoolView, PendingView,
};
use tawara_wallet_core::location::{self, Environment, Platform};
use tawara_wallet_core::preferences::{self, DeviceUnlock, Preferences};
use tawara_wallet_core::store_identifier;
use tawara_wallet_core::view::{AccountId, Reads, SettledAccount, WalletView, Watch};
use tawara_wallet_core::{
    Activity, Command, Config, Event, HttpsNode, LockReason, NetworkName, PhraseForDisplay,
    Progress, Refusal, RefusalKind, Reply, RequestId, SecretText, SyncState, WorkerHandle,
};
use zeroize::Zeroizing;

pub use wallet::{
    AccountPage, ActivityPage, AddAccountPage, BLOCK_ROWS, BlockPage, DestinationRow, Done,
    ExplorerPage, FormOf, Level, Missed, Page, QueuePage, ReceivePage, RecoveryPage, RefusedField,
    Remedy, ReportKey, ResignPage, SHARED, Scanning, SendPage, SendStage, SentPage, SettingsPage,
    Signed, SpendForm, SubmitMode, SubmitPage, TagPage, To, TransactionPage, WalletMsg, remedy,
    spend_request,
};

/// How often a poll asks the node about an account [`Watch::asks`] names: one
/// whose reservation is live and outstanding, or whose spend a failed read
/// left unanswered (docs/DECISIONS.md D38 item 2, as [`Watch`] extends it).
const POLL_EVERY: Duration = Duration::from_secs(60);

/// The accounts an answer reads afresh (see [`Reads`]): a check or a poll
/// reads its account, a send the account it spent from (the read after the
/// sign), a settle the account it settled, and an opened or refreshed store
/// every account, as a recovery review does too, since it reconciles every
/// account and updates each row. Any other answer reads nothing, and the rows
/// it carries are the store as it was (docs/DECISIONS.md D38 item 2;
/// docs/SCREENS.md W7).
fn reads_of(purpose: Purpose, reply: &Reply) -> Reads {
    match (purpose, reply) {
        (_, Reply::Status { account, .. }) => Reads::One(*account),
        (_, Reply::Sent(sent)) => Reads::One(sent.from),
        (Purpose::Settle(account), Reply::Settled { .. }) => Reads::One(account),
        // The read after a saved copy is written reads its account, as the read
        // after a send does (docs/DECISIONS.md D38 item 5 (b)).
        (Purpose::Resubmit(account), Reply::Resubmitted { .. }) => Reads::One(account),
        (
            _,
            Reply::Wallet(_)
            | Reply::Unlocked(_)
            | Reply::Created { opened: Ok(_), .. }
            | Reply::Reconciled { opened: Ok(_), .. }
            | Reply::Restored { opened: Ok(_), .. },
        ) => Reads::Every,
        // A recovery review reconciles every account the store holds, and
        // updates each row from what it found.
        (Purpose::Review, Reply::Reviewed(_)) => Reads::Every,
        _ => Reads::Nothing,
    }
}

/// The accounts the worker settled on its own in answering with `reply`
/// (docs/DECISIONS.md D38), from the store's view the reply carries. Every
/// reply that opens or reads the wallet can carry them: a refresh, an unlock,
/// the read after a send or after a saved copy is written, and the reopen after
/// a restore or an advance. A check's or a poll's own settlement is
/// `Reply::Status::settled`, which the check's answer shows itself.
fn settled_in(reply: &Reply) -> Vec<SettledAccount> {
    let view = match reply {
        Reply::Wallet(view) | Reply::Unlocked(view) | Reply::Settled { view, .. } => Some(view),
        Reply::Sent(sent) => Some(&sent.view),
        Reply::Resubmitted { view, .. } => Some(view),
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
    /// The preferences file was there at start-up but could not be read. It
    /// may hold a node the person chose, or `none` for one they forgot, so it
    /// is not written over for an ordinary change, such as the unit or the
    /// store last opened: only once the person chooses or forgets a node
    /// again (docs/DECISIONS.md D43). S2 and Settings say so while it holds,
    /// and offer Forget, which is the way out of it.
    pub prefs_held: bool,
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
    /// with the library's page, which repeats them. They are kept in the order
    /// they were signed: leaving or showing a page never reorders them, and a
    /// re-sign keeps its spend's place ([`SentPage::order`]).
    pub signed: Vec<Signed>,
    /// The place the next spend signed in this run takes in the signing history
    /// ([`SentPage::order`]).
    next_signing: u64,
    /// The worker stopped: `Some(true)` when it panicked. Nothing more can
    /// be done in this run. `Some(false)` is also a stop whose reason is not
    /// yet known, since a command could not be sent to the worker
    /// (`App::lost_worker`): until its notice says which, the unsaved bytes
    /// are kept, as a panic keeps them.
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
    /// What a mobile shell has said about the screen's capture: screenshots
    /// taken, and whether the screen is recorded or mirrored (docs/DECISIONS.md
    /// D44 item 3). The shell writes it on its own thread.
    pub(crate) capture: Arc<host::Capture>,
    /// The screenshots this model has acted on, as [`Model::capture`] counts
    /// them.
    shots_seen: u64,
    /// Whether the screen is being recorded or mirrored, as the shell last said.
    /// While it is, the recovery phrase's words are drawn as a placeholder
    /// (docs/DECISIONS.md D44 item 3). A shell says so through
    /// [`capture_changed`](crate::capture_changed); the screenshots example
    /// sets it to draw that placeholder.
    pub captured: bool,
    /// The window's height in logical pixels, as the window last reported it.
    /// With the safe area it decides a phone's frame for the recovery phrase
    /// (docs/DECISIONS.md D44 items 1 and 2).
    pub(crate) height: f32,
    /// In a mobile shell, the app-private folder every store must be
    /// inside: the one the system keeps out of backups and away from other
    /// apps, and on iOS protects (docs/DECISIONS.md D32 items 3 and 7).
    pub(crate) confined: Option<PathBuf>,
    /// The unlock with the device's own check, where the shell offers one
    /// (docs/DECISIONS.md D45). `None` on the desktop and on Android, where
    /// nothing is offered and nothing changes.
    pub device: Option<host::Device>,
    /// The offer of the device unlock is waiting for the person's answer. It
    /// comes once, after a password unlock opened the store, and the password
    /// it would keep is held in `pending_keep` until it is answered
    /// (docs/DECISIONS.md D45 item 4).
    pub device_offer: bool,
    /// The password of the password unlock under way, copied only while the
    /// offer may still be made (docs/DECISIONS.md D45 item 4). It is dropped
    /// when the unlock is refused, when the offer is answered, and when the
    /// store locks or the application leaves the foreground.
    pending_keep: Option<Kept>,
    /// How many asks of the device's check there have been. Each is numbered,
    /// so an answer to one that has since been dropped is recognised and
    /// ignored (docs/DECISIONS.md D45 item 4).
    device_asks: u64,
    /// The ask whose answer is awaited, if one is.
    device_asked: Option<u64>,
    /// What the device unlock last found, for the screens to say (provisional
    /// wording, for the owner's approval; docs/DECISIONS.md D45 item 6).
    pub device_note: Option<String>,
    /// The QR scanner the shell gives, where it gives one: phones only, as
    /// docs/DECISIONS.md D42 item 1 decides. `None` on the desktop, which offers
    /// no scan.
    pub scanner: Option<crate::scan::Scanner>,
    /// The scan under way, if one is, for the destination row it fills
    /// (docs/DECISIONS.md D42 item 4).
    pub scanning: Option<wallet::Scanning>,
    /// How many scans have been begun. Each scan is numbered, so a reply to one
    /// that has ended is recognised and dropped (docs/DECISIONS.md D42).
    scan_asks: u64,
    /// The camera's latest frame, as the scanner's preview page shows it, where
    /// the application draws the preview (docs/DECISIONS.md D42 item 3).
    pub scan_preview: Option<iced::widget::image::Handle>,
}

/// A password copied for the device unlock's offer. Its `Debug` prints nothing
/// of it, as [`Typed`]'s does, and the copy is wiped when it is dropped.
struct Kept(Zeroizing<String>);

impl Kept {
    /// A copy of `typed`, as the field holds it.
    fn copy_of(typed: &str) -> Kept {
        Kept(Zeroizing::new(typed.to_owned()))
    }

    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl core::fmt::Debug for Kept {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Kept(..)")
    }
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
    /// S0, on a phone alone, when the phone's folder holds no store: the
    /// brand panel and "Get started", which leads to S1 (docs/DECISIONS.md
    /// D37).
    Welcome,
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
    /// The person pressed "Show the phrase" (docs/DECISIONS.md D44). On a phone
    /// too short for the library's warning and the phrase on one frame, the
    /// warning comes first, on its own, and the words only after this. A
    /// screenshot clears it, so that the words wait for the button again.
    pub shown: bool,
    /// A screenshot was taken while the words were drawn (docs/DECISIONS.md D44
    /// items 1 and 3). On a phone the warning then takes the words' place until
    /// "Show the phrase" is pressed, held sideways too; on a desktop, which no
    /// phone shell confines, it comes above them. The mark
    /// stays with this screen, and goes with it.
    pub photographed: bool,
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
    /// W1 on a phone: the account whose row is open in place into a card, if
    /// one is (docs/DECISIONS.md D39 item 4). A page is replaced when it is
    /// left, so the card closes then.
    pub expanded: Option<AccountId>,
    /// Page 1 of a phone's send flow: whether the account list is open under
    /// the From line, by "Change" (docs/DECISIONS.md D40 item 1). A page is
    /// replaced when it is left, so the list closes then.
    pub choosing: bool,
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
    /// S4's "Show the phrase" (docs/DECISIONS.md D44).
    ShowPhrase,
    ToConfirm,
    BackToPhrase,
    StartOver,
    ConfirmWords,
    RestoreWallet,
    UnlockWallet,
    /// S7's unlock with the device's own check, once the person has turned it
    /// on (docs/DECISIONS.md D45 item 4).
    UnlockWithDevice,
    /// The shell's answer to an ask of the device's check, from its own thread
    /// (docs/DECISIONS.md D45 item 4).
    Released(host::Answer),
    /// The answer to the offer of the device unlock: `true` keeps the password
    /// on the device, `false` declines (docs/DECISIONS.md D45 item 4).
    DeviceOffer(bool),
    /// Turns the device unlock off, and deletes the password kept for it
    /// (docs/DECISIONS.md D45 item 4).
    TurnOffDeviceUnlock,
    /// Offers the device unlock again, at the next unlock with the password,
    /// once it has been declined or turned off (docs/DECISIONS.md D45 item 4;
    /// provisional, for the owner's approval).
    OfferDeviceUnlockAgain,
    /// The application's life changed (`iced::mobile::lifecycle`). S7 asks the
    /// device's check once, when the application comes to the foreground
    /// (docs/DECISIONS.md D45 item 4).
    Lifecycle(iced::mobile::LifecycleEvent),
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
    /// A Scan button was tapped beside destination row `row`: a QR code is read
    /// to fill that row (docs/DECISIONS.md D42). Only a phone's destination
    /// rows offer it.
    Scan(usize),
    /// The shell's reply to scan number `ask`, begun for destination row `row`:
    /// the text it read, or `None` when the scan was cancelled or refused. A
    /// reply to a scan that is no longer under way is dropped (docs/DECISIONS.md
    /// D42 item 4).
    Scanned(usize, u64, Option<crate::scan::Code>),
    /// The scan under way is stopped by the person, as its page's Cancel
    /// (docs/DECISIONS.md D42).
    CancelScan,
    /// The camera's preview is due a frame: the latest the shell has given is
    /// taken, while a scan draws its preview in the application (docs/DECISIONS.md
    /// D42 item 3).
    Preview,
    /// The window opened or changed size: its width and height.
    Size(iced::Size),
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
    /// At start-up nothing is written, and nothing is asked of the node it
    /// sets: its tip waits for a wallet to open, or for Check now or Refresh
    /// (docs/DECISIONS.md D43).
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
    /// The unlock with the password the device released (docs/DECISIONS.md D45
    /// item 4): as `Unlock`, but a refusal of the password forgets it.
    KeptUnlock,
    Refresh,
    Lock,
    Abandon,
    Receive,
    Discover,
    Restore,
    PlanSend,
    ConfirmSend,
    DiscardPlan,
    /// The person's Settle of an account: its answer reads that account
    /// (see [`reads_of`]).
    Settle(AccountId),
    Resign,
    SubmitArtifact,
    /// The saved copy of a settle the chain reverted, sent from W9 for this
    /// account: its answer reads that account (see [`reads_of`]).
    Resubmit(AccountId),
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
                | Purpose::KeptUnlock
                | Purpose::Refresh
                | Purpose::Discover
                | Purpose::Restore
                | Purpose::PlanSend
                | Purpose::ConfirmSend
                | Purpose::Settle(_)
                | Purpose::Resign
                | Purpose::SubmitArtifact
                | Purpose::Resubmit(_)
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
                | Purpose::Settle(_)
                | Purpose::Resign
                | Purpose::SubmitArtifact
                | Purpose::Resubmit(_)
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
    /// A command could not be sent to the worker, whose queue is gone: it has
    /// stopped or is stopping, and its `Stopped` has not come yet. Until that
    /// does, whether it panicked is not known (see [`App::lost_worker`]).
    lost: bool,
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

impl Drop for App {
    /// The application is gone, and with it the unsaved bytes it still holds.
    /// The worker's `Locked` and `Stopped` for the shutdown reach the
    /// application only while it runs, so the release does not wait for them
    /// (docs/SCREENS.md W6).
    fn drop(&mut self) {
        self.model.release_unsaved();
    }
}

/// Empty a secret field so its text is zeroized, not just dropped:
/// `SecretText::take` moves the buffer into a zeroizing one and leaves the
/// field empty (docs/PLAN.md section 4.2).
pub fn wipe(field: &mut String) {
    drop(SecretText::take(field));
}

/// Why a node the person chose or forgot was not saved.
#[derive(Debug)]
enum NotSaved {
    /// `boot` found no folder for the preferences, so nothing can be written
    /// in this run, and trying again cannot help (PR #29's review).
    NoFolder,
    /// The file could not be written.
    Write(std::io::Error),
}

/// The note on S2, or on Settings, when the node was forgotten but that could
/// not be saved (docs/DECISIONS.md D43). With no folder, forgetting it again
/// cannot help, so the note says what the next start does instead (PR #29's
/// review). Provisional wording, for the owner's approval.
fn forget_unsaved(why: &NotSaved) -> String {
    match why {
        NotSaved::NoFolder => "The node is forgotten for this run only, because there is no \
             folder for your settings. The next time Tawara starts, it uses the default node."
            .to_owned(),
        NotSaved::Write(e) => format!(
            "The node is forgotten, but that could not be saved ({e}). When Tawara next starts \
             it may use a node again: forget it again then."
        ),
    }
}

/// The note on S2, or on Settings, when the node was chosen and is in use but
/// could not be saved. The next start would use another node, so the person
/// is told now rather than finding out then (PR #29's review;
/// docs/DECISIONS.md D43). Provisional wording, for the owner's approval.
fn choice_unsaved(why: &NotSaved) -> String {
    match why {
        NotSaved::NoFolder => "This node is used for this run only, because there is no folder \
             for your settings. The next time Tawara starts, it uses the default node."
            .to_owned(),
        NotSaved::Write(e) => format!(
            "This node is in use, but it could not be saved ({e}). When Tawara next starts it \
             may use another node: choose this one again then."
        ),
    }
}

/// The title of the note S2 and Settings draw while the preferences file
/// could not be read (docs/DECISIONS.md D43). It says "settings", the page's
/// own name, not a term the person was never shown (PR #29's review).
/// Provisional wording, for the owner's approval.
pub const HELD_TITLE: &str = "Your settings could not be read";

/// The body of that note: that no node is in use, not even the default, so
/// that S2's text about the default is not read as the node in use; what is
/// not saved while the file is held; and what ends it (PR #29's review).
/// Provisional wording, for the owner's approval.
pub const HELD_BODY: &str = "No node is in use, not the default, and your other settings are not \
     saved. Choosing or forgetting a node saves them again, replacing the file that could not \
     be read.";

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
        // Signed bytes pasted on W9 may be a copy of unsaved bytes (Copy hex),
        // and go as those do (docs/DECISIONS.md D38 item 5; docs/SCREENS.md W9).
        Screen::Wallet(WalletPage {
            page: Page::Submit(s),
            ..
        }) => wipe(&mut s.hex),
        Screen::Welcome | Screen::Start { .. } | Screen::Node(_) | Screen::Wallet(_) => {}
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

#[cfg(test)]
thread_local! {
    /// The unsaved spends a release has wiped on this thread. Once the
    /// application is gone nothing is left to look at, so a test reads this to
    /// see that the release on its drop ran.
    static RELEASED_IN_TESTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// What S7 says of a signed spend's page that a lock kept, to show again once
/// its store is unlocked (docs/SCREENS.md W6).
const KEPT_NOTE: &str = "A signed spend's page was open when the wallet locked. It is kept, \
                         with the signed bytes, and shown again when this store is unlocked.";

/// What S7 says of unsaved signed bytes that a lock or a release wiped
/// (docs/SCREENS.md W6).
const WIPED_NOTE: &str = "Signed bytes that were not saved were wiped when the wallet \
                          locked. Nothing is written to disk, so they are gone: if the \
                          spend has not landed, its account's page offers Re-sign, which \
                          takes the same destinations, amounts, references, fee and \
                          block-to-live typed again.";

/// The reason the system's check gives for asking for the kept password.
/// Provisional wording, for the owner's approval (docs/DECISIONS.md D45 item 6).
const UNLOCK_REASON: &str = "unlock Tawara";

/// What the device unlock says when the kept password is not there for this
/// store, and the unlock with the device is turned off. Provisional wording, for
/// the owner's approval (docs/DECISIONS.md D45 item 6).
pub(crate) const DEVICE_MISSING_NOTE: &str = "No password is kept on this device for this store, so \
     the unlock with the device is off. Type the password to unlock.";

/// What the device unlock says when the kept password no longer opens the store:
/// it is deleted, and the unlock with the device is turned off. Provisional
/// wording, for the owner's approval (docs/DECISIONS.md D45 item 6).
pub(crate) const DEVICE_WRONG_NOTE: &str = "The password kept on this device no longer opens this \
     store, so it has been deleted and the unlock with the device is off. Type the \
     password to unlock.";

/// What the device unlock says when the kept password could not be deleted: the
/// unlock stays on, so that deleting can be tried again. Provisional wording, for
/// the owner's approval (docs/DECISIONS.md D45 item 6).
fn device_forget_failed_note(why: &str) -> String {
    format!(
        "The password kept on this device could not be deleted ({why}). The unlock with \
         the device stays on until it is, so that it can be tried again."
    )
}

/// What the device unlock says when the password could not be kept on this
/// device. Provisional wording, for the owner's approval (docs/DECISIONS.md D45
/// item 6).
fn device_keep_failed_note(why: &str) -> String {
    format!("The password could not be kept on this device ({why}). Nothing else has changed.")
}

/// What the device unlock says when the store's identifier could not be read, so
/// the check is not asked. Provisional wording, for the owner's approval
/// (docs/DECISIONS.md D45 item 6).
fn device_identifier_failed_note(why: &str) -> String {
    format!(
        "The store's identifier could not be read ({why}), so the unlock with the device is not used."
    )
}

/// What the device unlock says when the check could not release the password.
/// Provisional wording, for the owner's approval (docs/DECISIONS.md D45 item 6).
fn device_release_failed_note(why: &str) -> String {
    format!("The device could not release the password ({why}).")
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
            prefs_held: false,
            default_dir,
            downloads: None,
            busy: None,
            wallet: None,
            signed: Vec::new(),
            next_signing: 0,
            stopped: None,
            left: Arc::new(AtomicU64::new(0)),
            left_seen: 0,
            wiped: Arc::default(),
            capture: Arc::default(),
            shots_seen: 0,
            captured: false,
            confined: None,
            device: None,
            device_offer: false,
            pending_keep: None,
            device_asks: 0,
            device_asked: None,
            device_note: None,
            scanner: None,
            scanning: None,
            scan_asks: 0,
            scan_preview: None,
            width: crate::WINDOW.0,
            height: crate::WINDOW.1,
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

    /// Whether the store is unlocked with the device's own check: the shell
    /// offers it and the person has turned it on (docs/DECISIONS.md D45).
    #[must_use]
    pub fn device_unlock_on(&self) -> bool {
        self.device.is_some() && self.prefs.device_unlock == DeviceUnlock::On
    }

    /// Drops what the device unlock holds for the screen that is going: the
    /// password waiting for the offer, the offer itself, and an ask whose
    /// answer is not yet in, which an answer after this ignores (docs/DECISIONS.md
    /// D45 item 4). A lock, a move to the background and a stopped worker do it.
    fn release_device(&mut self) {
        self.pending_keep = None;
        self.device_offer = false;
        self.device_asked = None;
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

    /// Whether this is a phone whose own folder already holds a store. That
    /// store is the only one the phone can open, so S7 is its first screen,
    /// and S1 and S3 are not offered (docs/DECISIONS.md D37).
    #[must_use]
    pub(crate) fn phone_holds_store(&self) -> bool {
        self.phone() && self.store_to_open().is_some()
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
        // The password kept for the device's offer, and an ask the check has not
        // answered, go with the foreground (docs/DECISIONS.md D45 item 4). So does
        // a scan under way, whose camera the shell stopped as it left
        // (docs/DECISIONS.md D42 item 4).
        self.release_device();
        self.stop_scan();
        if matches!(self.screen, Screen::Phrase(_) | Screen::Confirm(_)) {
            self.back_to_new_wallet(Some(phrase_discarded(
                "when the application left the screen",
            )));
            return;
        }
        wipe_typed(&mut self.screen);
    }

    /// A screenshot was taken, as the shell reported it. The recovery phrase on
    /// S4 is marked for its warning if its words are drawn at that moment
    /// (docs/DECISIONS.md D44 item 3): on a phone that waits for "Show the
    /// phrase", they are not, and nothing is marked. A marked phrase hides its
    /// words again, behind the warning, until "Show the phrase" is pressed
    /// (items 1 and 3). Any other screen has no phrase to mark.
    fn photographed(&mut self) {
        let drawn = match &self.screen {
            Screen::Phrase(p) => phrase_words_drawn(self, p),
            _ => false,
        };
        if let (true, Screen::Phrase(p)) = (drawn, &mut self.screen) {
            p.photographed = true;
            p.shown = false;
        }
    }

    /// Releases the unsaved bytes that a lock releases, when no lock comes for
    /// them: a store the worker closed for a read-back that failed keeps them,
    /// and nothing is open for a move to the background or a shutdown to close,
    /// so no `Locked` comes (docs/DECISIONS.md D38 item 5; docs/SCREENS.md W6).
    /// They are wiped, shown or kept, and S7 says so: its note no longer says a
    /// kept page comes back once no kept spend is left to come back.
    fn release_unsaved(&mut self) {
        let released = self.drop_unsaved_at_lock();
        #[cfg(test)]
        RELEASED_IN_TESTS.with(|n| n.set(n.get() + released.len()));
        if released.is_empty() {
            return;
        }
        let resumes = self.signed.iter().any(|s| s.resume);
        let Screen::Unlock(u) = &mut self.screen else {
            return;
        };
        let note = u.note.take().unwrap_or_default();
        let note = if resumes {
            note
        } else {
            note.replace(KEPT_NOTE, "")
        };
        let note = note.trim();
        u.note = Some(if note.is_empty() {
            WIPED_NOTE.to_owned()
        } else if note.contains(WIPED_NOTE) {
            note.to_owned()
        } else {
            format!("{note} {WIPED_NOTE}")
        });
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
    /// Copy, its word when the application leaves the foreground, and its
    /// word on the screen's capture (docs/DECISIONS.md D32, D44 item 3).
    pub fn boot_in(host: &Host) -> (App, Task<Message>) {
        host::set(host);
        let (mut app, task) = App::hosted(&host.private_dir);
        // The shell's scanner replies come into the interface on their own
        // channel, as the device's answers do (docs/DECISIONS.md D42). Where the
        // shell gives no scan, the sender is dropped here and the channel ends
        // at once.
        let (replies, scanned) = mpsc::unbounded();
        app.model.scanner = crate::scan::Scanner::of(host, replies);
        host::on_leaving(app.leaving_hook());
        host::on_capture(Arc::clone(&app.model.capture), host.captured);
        // The shell answers an ask of the device's check from its own thread, and
        // the answer comes into the interface on this channel as a message
        // (docs/DECISIONS.md D45 item 4). Where the shell offers no device unlock,
        // the sender is dropped here and the channel ends at once.
        let (answers, received) = mpsc::unbounded();
        app.model.device = host::Device::of(host, answers);
        (
            app,
            Task::batch([
                task,
                Task::run(received, Message::Released),
                Task::run(scanned, |(row, ask, text)| Message::Scanned(row, ask, text)),
            ]),
        )
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
        // alone: the unlock screen when a store is there, else the welcome
        // screen, which leads to the start (docs/DECISIONS.md D37).
        app.model.screen = match app.model.store_to_open() {
            Some(dir) => Screen::Unlock(UnlockForm {
                dir,
                ..UnlockForm::default()
            }),
            None => Screen::Welcome,
        };
        (app, task)
    }

    /// What [`host::left_foreground`] does for this application: count the
    /// move first, on the shell's thread, so the next message the
    /// application takes already finds it; then the worker's lock. What the
    /// shell then waits for is the interface's taking of that lock's
    /// `Backgrounded`, or, with no worker, its wipe for the move. A scan under
    /// way has its camera stopped on the shell's thread too, since the
    /// interface may not run again before the system suspends the process
    /// (docs/DECISIONS.md D42 item 4).
    fn leaving_hook(&self) -> impl Fn() -> host::Leaving + Send + 'static {
        let left = Arc::clone(&self.model.left);
        let wiped = Arc::clone(&self.model.wiped);
        let worker = self.worker.as_ref().map(|w| w.handle.clone());
        let cancel_scan = self
            .model
            .scanner
            .as_ref()
            .map(crate::scan::Scanner::cancel_fn);
        move || {
            if let Some(cancel) = cancel_scan {
                cancel();
            }
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
        let loaded = prefs_path.as_deref().map(preferences::load_checked);
        let held = loaded.as_ref().is_some_and(|l| l.unreadable);
        let prefs = loaded.map(|l| l.prefs).unwrap_or_default();
        let mut model = Model::new(prefs, default_dir);
        model.downloads = downloads;
        let (mut app, events) = App::start(model, prefs_path);
        app.model.prefs_held = held;
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
                lost: false,
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
        // The lifecycle is listened to only where a shell offers the device
        // unlock, so the desktop's subscriptions are what they were.
        let lifecycle = if self.model.device.is_some() {
            iced::mobile::lifecycle().map(Message::Lifecycle)
        } else {
            Subscription::none()
        };
        // The camera's frames are taken at each redraw, and only while a scan
        // draws its preview in the application (docs/DECISIONS.md D42 item 3).
        // The desktop has no scan, so it has no such subscription.
        let frames = if self.model.previewing() {
            iced::window::frames().map(|_| Message::Preview)
        } else {
            Subscription::none()
        };
        Subscription::batch([
            event::listen_with(person_input),
            iced::mobile::safe_area().map(Message::SafeArea),
            lifecycle,
            frames,
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
                self.lost_worker();
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

    /// Save an ordinary change. A preference that cannot be written is lost
    /// at the next start, and nothing else depends on it; it is written with
    /// the next change that is. Nothing is written over a file that could
    /// not be read (`Model::prefs_held`).
    fn save_prefs(&self) {
        if self.model.prefs_held {
            return;
        }
        if let Some(path) = &self.prefs_path {
            let _ = preferences::save(path, &self.model.prefs);
        }
    }

    /// Save the node the person just chose or forgot. That is theirs to say,
    /// so it is written even over a file that could not be read, and from
    /// then on the file is written as usual (docs/DECISIONS.md D43). The
    /// caller says when the write failed.
    fn save_node_choice(&mut self) -> Result<(), NotSaved> {
        self.model.prefs_held = false;
        match &self.prefs_path {
            Some(path) => preferences::save(path, &self.model.prefs).map_err(NotSaved::Write),
            // `boot` finds no folder for the preferences where neither an
            // absolute XDG_DATA_HOME nor an absolute HOME is set. Nothing can be
            // written there, so a node chosen or forgotten in this run is gone
            // at the next start: that is a failure to say, not a saved choice
            // (PR #29's review, P2-1; docs/DECISIONS.md D43).
            None => Err(NotSaved::NoFolder),
        }
    }

    /// Put `note`, about a node choice that was not saved, on the screen the
    /// choice was made on: the node screen, or Settings, whose page draws its
    /// error at the top (its node card does not draw the node's error).
    fn note_unsaved(&mut self, note: String) {
        match &mut self.model.screen {
            Screen::Node(f) => f.error = Some(note),
            Screen::Wallet(p) => p.error = Some(note),
            _ => self.model.node.error = Some(note),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.apply(message);
        // A scan goes on only while the page it was begun on is shown, and
        // unchanged: the camera stops as soon as it is not (docs/DECISIONS.md
        // D42 item 4). Every message is looked at, so a change made by any of
        // them is noticed, not only the ones that begin with the page.
        let stale = self
            .model
            .scanning
            .as_ref()
            .is_some_and(|scanning| !self.scan_fits(scanning));
        if stale {
            self.model.stop_scan();
        }
        task
    }

    fn apply(&mut self, message: Message) -> Task<Message> {
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
            // With no worker left, this is the move's only wipe. It leaves the
            // unsaved bytes of a worker that stopped on a fault, for the stopped
            // screen to save until Tawara closes (docs/DECISIONS.md D38 item 5;
            // docs/SCREENS.md W6).
            self.model.backgrounded();
            self.model.wiped.wiped(left);
        }
        // What the shell has said about the screen's capture is read at each
        // message too, on this thread (docs/DECISIONS.md D44 item 3): a
        // screenshot is noted on the recovery phrase's screen when its words
        // are drawn, and the capture state is kept for the words' placeholder.
        let (shots, captured) = self.model.capture.read();
        self.model.captured = captured;
        if shots != self.model.shots_seen {
            self.model.shots_seen = shots;
            self.model.photographed();
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
            // On a phone's pages 2 to 4, the system's Back steps back one page
            // of the send flow, and does not leave the application
            // (docs/DECISIONS.md D40 item 3). Everywhere else, and on page 1,
            // it is the shell's.
            Message::SystemBack => {
                if self.model.send_steps_back() {
                    return self.on_wallet(WalletMsg::Back);
                }
                host::back();
            }
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
                // On a phone with a store, the start screens are not offered:
                // S7 is its first screen (docs/DECISIONS.md D37).
                if !m.phone_holds_store() {
                    let _ = m.leave();
                    m.screen = Screen::Start {
                        choice: StartChoice::Create,
                    };
                }
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
            Message::ShowPhrase => {
                if let Screen::Phrase(p) = &mut m.screen {
                    p.shown = true;
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
                // The password is copied for the device's offer only while the
                // offer may still be made: the device offers its check, and the
                // unlock has not been offered yet (docs/DECISIONS.md D45 item 4).
                let offerable =
                    m.device.is_some() && m.prefs.device_unlock == DeviceUnlock::Unoffered;
                if let (Screen::Unlock(u), Some(why)) = (&mut m.screen, outside) {
                    u.error = Some(why);
                } else if let Screen::Unlock(u) = &mut m.screen {
                    u.error = None;
                    u.note = None;
                    // A password unlock is a new attempt, so what the device
                    // unlock last found is no longer what the screen says.
                    m.device_note = None;
                    let kept =
                        (offerable && !u.password.is_empty()).then(|| Kept::copy_of(&u.password));
                    let command = Command::Unlock {
                        dir: store_dir(&u.dir),
                        password: SecretText::take(&mut u.password),
                    };
                    if self.send(command, Purpose::Unlock).is_some() {
                        self.model.pending_keep = kept;
                    }
                }
            }
            Message::UnlockWithDevice => self.unlock_with_device(),
            Message::Released(answer) => return self.released(answer),
            Message::DeviceOffer(keep) => self.answer_offer(keep),
            Message::TurnOffDeviceUnlock => self.turn_off_device_unlock(),
            Message::OfferDeviceUnlockAgain => self.offer_device_unlock_again(),
            // Only a return to the foreground asks: a Face ID prompt makes the
            // application inactive, and an ask on `Active` would loop
            // (docs/DECISIONS.md D45 item 4).
            Message::Lifecycle(event) => {
                if matches!(event, iced::mobile::LifecycleEvent::Foreground) {
                    self.unlock_with_device();
                }
            }
            Message::Cancel => {
                if let Some(w) = &self.worker {
                    w.handle.cancel();
                }
            }
            Message::Lock => {
                // A scan under way stops with the store (docs/DECISIONS.md D42
                // item 4).
                m.stop_scan();
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
            // A Scan button starts a scan for its row, and the shell's reply
            // comes back as `Scanned` (docs/DECISIONS.md D42).
            Message::Scan(row) => return self.begin_scan(row),
            Message::Scanned(row, ask, text) => return self.scanned(row, ask, text),
            Message::CancelScan => m.stop_scan(),
            Message::Preview => m.take_preview(),
            Message::Size(size) => {
                m.width = size.width;
                m.height = size.height;
            }
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
                    return self.on_reply(purpose, *reply);
                }
            }
            Event::Locked { reason } => {
                // The store is closed, so the password kept for the device's offer
                // goes with it, and an ask the check has not answered is dropped
                // (docs/DECISIONS.md D45 item 4). A scan under way stops with it
                // (docs/DECISIONS.md D42 item 4).
                self.model.release_device();
                self.model.stop_scan();
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
                // may not have been seen yet. A lock that comes after a send
                // failed, while the stop's reason is not yet known, keeps them
                // too, unless it is the shutdown's: the worker may have sent it
                // just before it panicked, and a panic keeps them until Tawara
                // closes (see `App::lost_worker`).
                let lost = self.worker.as_ref().is_some_and(|w| w.lost);
                let releases = match reason {
                    LockReason::Shutdown => true,
                    LockReason::Asked | LockReason::Idle | LockReason::Background => !lost,
                    LockReason::Replaced | LockReason::ReopenFailed => false,
                };
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
                    note = Some(match note {
                        Some(n) => format!("{n} {WIPED_NOTE}"),
                        None => WIPED_NOTE.to_owned(),
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
                        note = Some(match note {
                            Some(n) => format!("{n} {KEPT_NOTE}"),
                            None => KEPT_NOTE.to_owned(),
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
                // A store the worker closed for a read-back that failed sends no
                // `Locked` for the move, so the unsaved bytes it kept are released
                // here, before the move is taken (docs/SCREENS.md W6). With a
                // store open, the worker's `Locked` comes first and has wiped
                // them, with S7's note (wallet-core's test
                // `every_move_to_the_background_is_reported_after_any_lock`), so
                // this finds nothing more. A worker whose stop is not yet known
                // keeps them, a move's notice included, since a panic keeps them:
                // its `Stopped` releases them if the stop was graceful (see
                // `App::lost_worker`).
                let lost = self.worker.as_ref().is_some_and(|w| w.lost);
                if !lost {
                    self.model.release_unsaved();
                }
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

    /// A command could not be sent, because the worker's queue is gone: it
    /// has stopped or is stopping, and its `Stopped` has not come yet. A worker
    /// that panics drops its queue before its notice reaches the interface, so
    /// this may be a panic's window as well as a graceful stop's, and the
    /// reason is not known until the notice comes. Until it does, the unsaved
    /// bytes are kept, through a move to the background and any lock but the
    /// shutdown's (see the `Locked` arm of `App::on_event`): a panic keeps them
    /// until Tawara closes, and a graceful stop releases them (docs/DECISIONS.md
    /// D38 item 5; docs/SCREENS.md W6). The stopped screen shows at once. The
    /// worker is kept meanwhile, so the answers it has already sent are still
    /// taken, a spend's among them, since they may hold the only copy of its
    /// bytes.
    fn lost_worker(&mut self) {
        if let Some(w) = &mut self.worker {
            w.lost = true;
        }
        let m = &mut self.model;
        if m.stopped.is_none() {
            m.stopped = Some(false);
        }
        // The page shown is kept with the other spends, as a stop keeps it.
        drop(m.leave());
        m.busy = None;
        m.wallet = None;
        m.watch.clear();
    }

    /// The worker has stopped, as its `Stopped` says (`panicked` when it
    /// faulted). Nothing more can be done in this run, and what the screens
    /// held goes with it: the open store's view, and the secret a form or the
    /// phrase screens held, zeroized. With no worker left to lock on idle,
    /// nothing secret would otherwise go before the application closed. The
    /// unsaved bytes go only with a graceful stop: a panic keeps them until
    /// Tawara closes (docs/DECISIONS.md D38 item 5). This notice also settles
    /// a stop that [`App::lost_worker`] left of unknown reason.
    fn stop(&mut self, panicked: bool) {
        let m = &mut self.model;
        m.stopped = Some(panicked || m.stopped == Some(true));
        m.release_device();
        drop(m.leave());
        // A graceful shutdown ends the run, so the unsaved bytes go, as its lock
        // would take them, even when no `Locked` came (docs/SCREENS.md W6). A
        // worker that panicked keeps them, to save from the stopped card.
        if !panicked {
            m.release_unsaved();
        }
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
        // A spend signed now holds a reservation, whatever its account's polls were
        // before (docs/DECISIONS.md D38 item 2, as docs/SCREENS.md W7 extends it;
        // `Watch::signed`). The reads the reply carries are noted below, as every
        // reply's are.
        if let Reply::Sent(sent) = &reply {
            self.model.watch.signed(sent.from);
        }
        // The accounts the answer reads, taken before the answer is applied.
        let reads = reads_of(purpose, &reply);
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
        // (docs/DECISIONS.md D38 item 2). Only the accounts it read: the other
        // rows are the store as it was, and noting them again could end another
        // account's watch before that account's own answer has come.
        if let Some(view) = &self.model.wallet {
            self.model.watch.read_answer(view, reads);
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
                let back = match &m.screen {
                    Screen::Node(f) if form => Some(f.back),
                    _ => None,
                };
                // A node the person chose is saved and asked now. At start-up
                // the node is the one already in the preferences, the default
                // when none was chosen: nothing changed, so nothing is written,
                // and nothing is asked until a wallet opens or the person asks
                // (docs/DECISIONS.md D43).
                // A chosen node that cannot be saved may not be the one the
                // next start uses: with no folder for the preferences, that is
                // always the default. So the screen it was chosen on stays,
                // saying so, rather than going back as if it were saved (PR
                // #29's review).
                if form {
                    match self.save_node_choice() {
                        Ok(()) => {
                            if let Some(back) = back {
                                self.go_back(back);
                            }
                        }
                        Err(why) => self.note_unsaved(choice_unsaved(&why)),
                    }
                    self.ask_tip();
                    self.read_activity();
                }
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
                // The address typed, and what was said of the last try, are
                // about a node no longer wanted, so both are cleared on the two
                // screens Forget is drawn on: Settings no longer keeps "Type
                // the node's address" above a node just forgotten (PR #29's
                // review).
                match &mut m.screen {
                    Screen::Node(f) => {
                        f.url.clear();
                        f.error = None;
                    }
                    Screen::Wallet(WalletPage {
                        page: Page::Settings(s),
                        error,
                        ..
                    }) => {
                        s.node.clear();
                        *error = None;
                    }
                    _ => {}
                }
                // A forgotten node must stay forgotten (docs/DECISIONS.md
                // D43). If that cannot be written, the next start would use
                // the node from before, or the default, so the screen Forget
                // was pressed on says so. The next change that is written
                // saves `none` with it.
                if let Err(why) = self.save_node_choice() {
                    self.note_unsaved(forget_unsaved(&why));
                }
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
                    shown: false,
                    photographed: false,
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
                self.make_offer();
            }
            // A password that did not open the store is not copied for the offer.
            (Purpose::Unlock, Reply::Refused(r)) => {
                self.model.pending_keep = None;
                self.refused(r);
            }
            (Purpose::Unlock, _) => self.model.pending_keep = None,
            // The password the device released opened the store: as a typed one
            // does, and nothing is offered, since the device unlock is on.
            (Purpose::KeptUnlock, Reply::Unlocked(view)) => {
                self.opened(view);
                self.model.bring_back_signed();
            }
            // The kept password is forgotten only when it is the password that
            // was refused: another refusal, such as a store in use, leaves it
            // (docs/DECISIONS.md D45 item 4).
            (Purpose::KeptUnlock, Reply::Refused(r)) => {
                if r.kind == RefusalKind::WrongPassword && self.delete_kept() {
                    self.set_device_unlock(DeviceUnlock::Off);
                    self.model.device_note = Some(DEVICE_WRONG_NOTE.to_owned());
                }
                self.refused(r);
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
            Back::Wallet if m.wallet.is_some() => {
                m.screen = Screen::Wallet(WalletPage::default());
            }
            Back::Unlock => m.unlock_screen(None, None, None),
            // A phone that holds a store is not offered the start screens,
            // whichever way back leads there: its first screen is S7
            // (docs/DECISIONS.md D37).
            Back::Start | Back::Wallet if m.phone_holds_store() => {
                m.unlock_screen(None, None, None);
            }
            Back::Start | Back::Wallet => {
                m.screen = Screen::Start {
                    choice: StartChoice::Create,
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
            Screen::Welcome | Screen::Start { .. } | Screen::Phrase(_) => {}
        }
    }

    /// Stores the device unlock's setting, and writes the preferences.
    fn set_device_unlock(&mut self, state: DeviceUnlock) {
        self.model.prefs.device_unlock = state;
        self.save_prefs();
    }

    /// Deletes the kept password, and `true` once none is kept. Where the
    /// deletion fails, the note says why, and the caller leaves the unlock on, so
    /// that it can be tried again (docs/DECISIONS.md D45 item 4). With no device
    /// there is nothing that could be kept, so there is nothing to delete.
    fn delete_kept(&mut self) -> bool {
        let Some(device) = self.model.device.clone() else {
            return true;
        };
        match device.forget() {
            Ok(()) => true,
            Err(why) => {
                self.model.device_note = Some(device_forget_failed_note(&why));
                false
            }
        }
    }

    /// No password is kept for this store, or the one kept belongs to another
    /// store: whatever is kept is deleted, and the unlock with the device is
    /// turned off (docs/DECISIONS.md D45 item 4). The form is then what is left.
    fn device_missing(&mut self) {
        if self.delete_kept() {
            self.set_device_unlock(DeviceUnlock::Off);
            self.model.device_note = Some(DEVICE_MISSING_NOTE.to_owned());
        }
    }

    /// The unlock with the device, from S7 (docs/DECISIONS.md D45 item 4). The
    /// store's identifier is read, and the device's check is asked for the kept
    /// password, with [`UNLOCK_REASON`]. Nothing is asked unless the unlock is
    /// on, the form is shown, and no other ask or command is awaited.
    fn unlock_with_device(&mut self) {
        let m = &mut self.model;
        if !m.device_unlock_on()
            || m.device_asked.is_some()
            || m.busy.is_some()
            || !matches!(m.screen, Screen::Unlock(_))
        {
            return;
        }
        let (Some(device), Some(private)) = (m.device.clone(), m.confined.clone()) else {
            return;
        };
        m.device_note = None;
        let identifier = match store_identifier::read(&private) {
            Ok(Some(identifier)) => identifier,
            // Nothing can be kept under an identifier this folder does not have,
            // so the check is not asked.
            Ok(None) => return self.device_missing(),
            Err(why) => {
                m.device_note = Some(device_identifier_failed_note(&format!("{why}")));
                return;
            }
        };
        m.device_asks += 1;
        let ask = m.device_asks;
        m.device_asked = Some(ask);
        device.ask(&identifier, UNLOCK_REASON, ask);
    }

    /// The shell's answer to an ask of the device's check (docs/DECISIONS.md D45
    /// item 4). An answer whose ask has been dropped, by a lock or a move to the
    /// background, is ignored, and so is the password it carries, which is wiped
    /// as it is dropped here. A released password unlocks the store as a typed
    /// one does (`UnlockWallet`).
    fn released(&mut self, answer: host::Answer) -> Task<Message> {
        let Some((ask, released)) = answer.take() else {
            return Task::none();
        };
        if self.model.device_asked != Some(ask) {
            return Task::none();
        }
        self.model.device_asked = None;
        match released {
            host::Released::Password(secret) => {
                let dir = match &self.model.screen {
                    Screen::Unlock(u) => store_dir(&u.dir),
                    // The person has left the form: the password goes unused.
                    _ => return Task::none(),
                };
                self.send(
                    Command::Unlock {
                        dir,
                        password: secret,
                    },
                    Purpose::KeptUnlock,
                );
            }
            host::Released::Declined => {}
            host::Released::Missing => self.device_missing(),
            host::Released::Failed(why) => {
                self.model.device_note = Some(device_release_failed_note(&why));
            }
        }
        Task::none()
    }

    /// After a password unlock opened the store, the offer of the device unlock is
    /// made, once (docs/DECISIONS.md D45 item 4). The copy of the password taken
    /// when the unlock was sent is what the offer would keep, and the offer is
    /// recorded as made as it is shown, so it is not made again even if the
    /// application stops before the person answers. Otherwise the copy goes.
    fn make_offer(&mut self) {
        let m = &mut self.model;
        let offer = m.pending_keep.is_some()
            && m.device.is_some()
            && m.prefs.device_unlock == DeviceUnlock::Unoffered;
        if !offer {
            m.pending_keep = None;
            return;
        }
        m.device_offer = true;
        self.set_device_unlock(DeviceUnlock::Off);
    }

    /// The person's answer to the offer (docs/DECISIONS.md D45 item 4). Kept, the
    /// password is kept on this device under the store's identifier, which is
    /// drawn and written the first time, and the unlock with the device is on.
    /// Declined, nothing is kept.
    fn answer_offer(&mut self, keep: bool) {
        // Only an offer that is shown is answered. The copy of a password waits
        // for an unlock that has opened the store, and nothing else may take it
        // (docs/DECISIONS.md D45 item 4).
        if !self.model.device_offer {
            return;
        }
        self.model.device_offer = false;
        let Some(kept) = self.model.pending_keep.take() else {
            return;
        };
        // Declined, the copy is wiped as it is dropped here.
        if !keep {
            return;
        }
        self.model.device_note = None;
        let (Some(device), Some(private)) =
            (self.model.device.clone(), self.model.confined.clone())
        else {
            return;
        };
        let outcome = store_identifier::make(&private)
            .map_err(|why| format!("{why}"))
            .and_then(|identifier| device.keep(&identifier, kept.as_str()));
        match outcome {
            Ok(()) => self.set_device_unlock(DeviceUnlock::On),
            // The unlock stays off: the offer was made, and nothing is kept.
            Err(why) => self.model.device_note = Some(device_keep_failed_note(&why)),
        }
    }

    /// Turns the device unlock off. The kept password is deleted first, and the
    /// setting is off only once it is; where the deletion fails, the unlock stays
    /// on and the note says why (docs/DECISIONS.md D45 item 4).
    fn turn_off_device_unlock(&mut self) {
        if self.model.prefs.device_unlock != DeviceUnlock::On {
            return;
        }
        self.model.device_note = None;
        if self.delete_kept() {
            self.set_device_unlock(DeviceUnlock::Off);
        }
    }

    /// Offers the device unlock again, after it was declined or turned off: the
    /// setting goes back to not offered, which writes nothing, so the offer comes
    /// after the next password unlock that opens the store, as the first one did
    /// (docs/DECISIONS.md D45 item 4). Nothing is kept until that unlock and the
    /// offer's answer. With an offer on the screen already, there is nothing to
    /// offer again.
    fn offer_device_unlock_again(&mut self) {
        if self.model.device.is_none()
            || self.model.prefs.device_unlock != DeviceUnlock::Off
            || self.model.device_offer
        {
            return;
        }
        self.model.device_note = None;
        self.set_device_unlock(DeviceUnlock::Unoffered);
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
        ) => Some(Message::Size(size)),
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

    /// An application over a real worker, with its default store in
    /// `scratch`. Its preferences say `none`, not the default node: a test
    /// that opens or reads a wallet must not reach a real node, and the
    /// default is tested where it is chosen (docs/DECISIONS.md D43).
    fn app(scratch: &Scratch) -> (App, Receiver<Event>) {
        let prefs = Preferences {
            node: None,
            ..Preferences::default()
        };
        let model = Model::new(prefs, Some(scratch.0.join("keystore")));
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
        // No node from here on, as the worker's: this test's application starts
        // with none (see `app`), so the wallet below is opened with no node.
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

    // ---- QR scanning on a phone (docs/DECISIONS.md D42) ----

    use crate::host::ScanPreview;

    thread_local! {
        /// How many times the shell's camera was stopped on this thread.
        static CANCELS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        /// The reply a scan was left waiting for, to be sent later.
        static HELD: std::cell::RefCell<Option<crate::host::ScanDone>> =
            const { std::cell::RefCell::new(None) };
    }

    type Replies =
        iced::futures::channel::mpsc::UnboundedReceiver<(usize, u64, Option<crate::scan::Code>)>;

    fn count_cancel() {
        CANCELS.with(|c| c.set(c.get() + 1));
    }

    fn cancels() -> u32 {
        CANCELS.with(std::cell::Cell::get)
    }

    /// A shell whose scan reads the first destination of the sample wallet, at
    /// once.
    fn scan_destination(done: crate::host::ScanDone) -> Result<(), String> {
        done(Some(a_destination().into_bytes()));
        Ok(())
    }

    /// A scan that reads text which is not a destination, at once.
    fn scan_junk(done: crate::host::ScanDone) -> Result<(), String> {
        done(Some(b"hello".to_vec()));
        Ok(())
    }

    /// A scan that reads more text than a destination can be, at once.
    fn scan_overlong(done: crate::host::ScanDone) -> Result<(), String> {
        done(Some(vec![b'1'; crate::scan::MAX_BYTES + 1]));
        Ok(())
    }

    /// A scan that reads bytes which are not UTF-8, at once.
    fn scan_not_text(done: crate::host::ScanDone) -> Result<(), String> {
        done(Some(vec![0xff, b'1']));
        Ok(())
    }

    /// A scan the shell ends with no text, at once: cancelled, or not allowed.
    fn scan_nothing(done: crate::host::ScanDone) -> Result<(), String> {
        done(None);
        Ok(())
    }

    /// A scan whose reply is left waiting, to be sent by [`reply_later`].
    fn scan_later(done: crate::host::ScanDone) -> Result<(), String> {
        HELD.with(|h| *h.borrow_mut() = Some(done));
        Ok(())
    }

    /// A camera that will not start.
    fn scan_refused(_: crate::host::ScanDone) -> Result<(), String> {
        Err("the camera is not allowed".to_owned())
    }

    /// Sends the reply that [`scan_later`] left waiting, as the shell would.
    fn reply_later(text: Option<Vec<u8>>) {
        let done = HELD
            .with(|h| h.borrow_mut().take())
            .expect("a scan is waiting for its reply");
        done(text);
    }

    /// A phone's Host that scans with `start`, as a shell that offers scanning
    /// gives it.
    fn scanning_host(start: crate::host::ScanStart, preview: crate::host::ScanPreview) -> Host {
        Host {
            scan: Some(start),
            cancel_scan: Some(count_cancel),
            scan_preview: Some(preview),
            ..fake_host(PathBuf::from("/nonexistent"))
        }
    }

    /// A phone on page 1 of the send form, with two destination rows, and a
    /// scanner that `start` drives. The shell's replies come on the returned
    /// receiver, as the application takes them.
    fn scanning_phone(
        start: crate::host::ScanStart,
        preview: crate::host::ScanPreview,
    ) -> (App, Replies) {
        let form = SpendForm {
            rows: vec![DestinationRow::default(), DestinationRow::default()],
            ..SpendForm::default()
        };
        let mut app = on_phone(send_page(SendStage::To(Default::default()), form));
        let (replies, received) = iced::futures::channel::mpsc::unbounded();
        app.model.scanner = crate::scan::Scanner::of(&scanning_host(start, preview), replies);
        (app, received)
    }

    /// Gives the interface the replies the shell has sent, as it takes them.
    fn take_replies(app: &mut App, received: &mut Replies) {
        while let Ok((row, ask, text)) = received.try_recv() {
            let _ = app.update(Message::Scanned(row, ask, text));
        }
    }

    /// The destination rows of the send form shown.
    fn rows_of(app: &App) -> Vec<DestinationRow> {
        match &wallet_page(app).page {
            Page::Send(s) => s.form.rows.clone(),
            other => panic!("the send form is not shown: {other:?}"),
        }
    }

    #[test]
    fn a_scan_fills_the_row_it_was_begun_for_and_no_other() {
        let (mut app, mut received) = scanning_phone(scan_destination, ScanPreview::Shell);
        let _ = app.update(Message::Scan(1));
        take_replies(&mut app, &mut received);
        let rows = rows_of(&app);
        assert_eq!(rows[0], DestinationRow::default(), "row 1 is not touched");
        assert_eq!(rows[1].to, a_destination());
        assert!(app.model.scanning.is_none(), "the scan is over");
        assert!(refusals_of(&app).is_empty());
    }

    #[test]
    fn a_read_stops_the_camera_as_its_reply_is_taken() {
        let (mut app, mut received) = scanning_phone(scan_destination, ScanPreview::Shell);
        let _ = app.update(Message::Scan(1));
        let before = cancels();
        take_replies(&mut app, &mut received);
        assert_eq!(
            rows_of(&app)[1].to,
            a_destination(),
            "the read fills its row"
        );
        assert_eq!(cancels(), before + 1, "and the camera stops with the scan");
    }

    #[test]
    fn a_text_that_is_not_a_destination_is_refused_under_its_row() {
        let (mut app, mut received) = scanning_phone(scan_junk, ScanPreview::Shell);
        let _ = app.update(Message::Scan(0));
        take_replies(&mut app, &mut received);
        let refused = refusals_of(&app);
        let why = refused
            .get(&(0, RefusedField::Address))
            .expect("refused under the address of row 1");
        assert!(why.contains("is not a destination"), "{why}");
        assert_eq!(rows_of(&app)[0].to, "", "nothing is put in the row");
    }

    #[test]
    fn an_overlong_or_non_utf8_text_is_refused_under_its_row() {
        let starts: [crate::host::ScanStart; 2] = [scan_overlong, scan_not_text];
        for start in starts {
            let (mut app, mut received) = scanning_phone(start, ScanPreview::Shell);
            let _ = app.update(Message::Scan(0));
            take_replies(&mut app, &mut received);
            assert!(
                refusals_of(&app).contains_key(&(0, RefusedField::Address)),
                "refused under the address"
            );
            assert_eq!(rows_of(&app)[0].to, "");
        }
    }

    #[test]
    fn a_scan_cancelled_or_refused_by_the_shell_fills_nothing() {
        let (mut app, mut received) = scanning_phone(scan_nothing, ScanPreview::Shell);
        let _ = app.update(Message::Scan(0));
        take_replies(&mut app, &mut received);
        assert!(
            refusals_of(&app).is_empty(),
            "a cancelled scan says nothing"
        );
        assert_eq!(rows_of(&app)[0].to, "");
        assert!(app.model.scanning.is_none());

        let (mut app, _received) = scanning_phone(scan_refused, ScanPreview::Shell);
        let _ = app.update(Message::Scan(0));
        let why = refusals_of(&app)
            .get(&(0, RefusedField::Address))
            .cloned()
            .expect("a camera that will not start says why, under the row");
        assert!(why.contains("not allowed"), "{why}");
        assert!(app.model.scanning.is_none());
    }

    #[test]
    fn a_reply_to_a_scan_that_has_ended_is_dropped_even_when_another_is_under_way() {
        let (mut app, mut received) = scanning_phone(scan_later, ScanPreview::Shell);
        let _ = app.update(Message::Scan(0));
        let first = HELD
            .with(|h| h.borrow_mut().take())
            .expect("the first scan waits");
        let _ = app.update(Message::CancelScan);
        let _ = app.update(Message::Scan(0));
        let before = cancels();
        first(Some(a_destination().into_bytes()));
        take_replies(&mut app, &mut received);
        assert_eq!(
            rows_of(&app)[0].to,
            "",
            "the earlier scan's text is dropped"
        );
        assert!(
            app.model.scanning.is_some(),
            "the scan under way still waits"
        );
        assert_eq!(
            cancels(),
            before,
            "and the camera of the scan under way does not stop"
        );
    }

    #[test]
    fn a_reply_after_the_row_the_scan_fills_changed_is_dropped() {
        let (mut app, mut received) = scanning_phone(scan_later, ScanPreview::Shell);
        let _ = app.update(Message::Scan(0));
        let _ = app.update(Message::Wallet(WalletMsg::DestinationTo(
            0,
            "typed".to_owned(),
        )));
        assert!(app.model.scanning.is_none(), "a changed row ends the scan");
        reply_later(Some(a_destination().into_bytes()));
        take_replies(&mut app, &mut received);
        assert_eq!(rows_of(&app)[0].to, "typed", "the late reply is dropped");
    }

    #[test]
    fn a_reply_after_a_row_was_added_or_the_page_was_left_is_dropped() {
        let (mut app, mut received) = scanning_phone(scan_later, ScanPreview::Shell);
        let _ = app.update(Message::Scan(0));
        let _ = app.update(Message::Wallet(WalletMsg::AddDestination));
        assert!(app.model.scanning.is_none(), "the rows changed");
        reply_later(Some(a_destination().into_bytes()));
        take_replies(&mut app, &mut received);
        assert_eq!(rows_of(&app)[0].to, "");

        let (mut app, mut received) = scanning_phone(scan_later, ScanPreview::Shell);
        let _ = app.update(Message::Scan(0));
        let _ = app.update(Message::Wallet(WalletMsg::Open(To::Dashboard)));
        assert!(app.model.scanning.is_none(), "the page was left");
        reply_later(Some(a_destination().into_bytes()));
        take_replies(&mut app, &mut received);
        assert!(
            matches!(&wallet_page(&app).page, Page::Dashboard),
            "the late reply does not bring the form back"
        );
    }

    #[test]
    fn a_scan_ends_when_the_destinations_are_continued_past() {
        let (mut app, _received) = scanning_phone(scan_later, ScanPreview::Shell);
        let _ = app.update(Message::Scan(0));
        let before = cancels();
        let _ = app.update(Message::Wallet(WalletMsg::DestinationTo(
            1,
            a_destination(),
        )));
        assert!(app.model.scanning.is_some(), "another row does not end it");
        let _ = app.update(Message::Wallet(WalletMsg::Continue));
        assert!(app.model.scanning.is_none(), "the amounts page ends it");
        assert_eq!(cancels(), before + 1, "and the camera stops");
    }

    #[test]
    fn leaving_the_page_locking_or_backgrounding_stops_the_camera() {
        let (mut app, _received) = scanning_phone(scan_later, ScanPreview::Shell);
        let _ = app.update(Message::Scan(0));
        let before = cancels();
        let _ = app.update(Message::Wallet(WalletMsg::Open(To::Dashboard)));
        assert_eq!(cancels(), before + 1, "leaving the page");
        assert!(app.model.scanning.is_none());

        let (mut app, _received) = scanning_phone(scan_later, ScanPreview::Shell);
        let _ = app.update(Message::Scan(0));
        let before = cancels();
        let _ = app.update(Message::Lock);
        assert_eq!(cancels(), before + 1, "a lock");
        assert!(app.model.scanning.is_none());

        // The shell's own move to the background stops the camera on its thread,
        // and the application drops the scan at its next message.
        let (mut app, _received) = scanning_phone(scan_later, ScanPreview::Shell);
        let _ = app.update(Message::Scan(0));
        let before = cancels();
        let leave = app.leaving_hook();
        let _ = leave();
        assert_eq!(cancels(), before + 1, "the move to the background");
        let _ = app.update(Message::Tick(0));
        assert!(app.model.scanning.is_none(), "the scan is dropped");
    }

    #[test]
    fn the_desktop_offers_no_scan() {
        let mut app = on_wallet(composing());
        assert!(!app.model.scan_offered());
        let _ = app.update(Message::Scan(0));
        assert!(app.model.scanning.is_none(), "nothing is begun");
        assert!(app.model.scanner.is_none());
    }

    /// The shell's frames are one shared slot, so the tests that give frames take
    /// turns with each other.
    static FRAME_TURNS: Mutex<()> = Mutex::new(());

    #[test]
    fn a_preview_frame_is_shown_while_a_scan_draws_its_preview_in_the_application() {
        let _turn = FRAME_TURNS.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _received) = scanning_phone(scan_later, ScanPreview::Application);
        let _ = app.update(Message::Scan(0));
        assert!(app.model.previewing(), "the frames are taken");
        crate::host::preview_frame(2, 2, vec![0; 16]);
        let _ = app.update(Message::Preview);
        assert!(
            app.model.scan_preview.is_some(),
            "the latest frame is shown"
        );

        // A frame whose bytes are not its size is refused by the shell's entry
        // point, so nothing new is shown.
        app.model.scan_preview = None;
        crate::host::preview_frame(2, 2, vec![0; 3]);
        let _ = app.update(Message::Preview);
        assert!(app.model.scan_preview.is_none(), "a bad frame is dropped");

        let _ = app.update(Message::CancelScan);
        assert!(app.model.scan_preview.is_none());
        assert!(app.model.scanning.is_none(), "Cancel ends the scan");
    }

    #[test]
    fn the_preview_goes_with_the_scan_when_its_reply_comes() {
        let _turn = FRAME_TURNS.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, mut received) = scanning_phone(scan_later, ScanPreview::Application);
        let _ = app.update(Message::Scan(0));
        crate::host::preview_frame(2, 2, vec![0; 16]);
        let _ = app.update(Message::Preview);
        assert!(app.model.scan_preview.is_some());
        reply_later(Some(a_destination().into_bytes()));
        take_replies(&mut app, &mut received);
        assert!(
            app.model.scan_preview.is_none(),
            "the scan ended with its reply"
        );
        assert!(app.model.scanning.is_none());
        assert_eq!(rows_of(&app)[0].to, a_destination());
    }

    #[test]
    fn a_scanned_text_is_never_in_a_message_log() {
        let message = Message::Scanned(0, 1, Some(crate::scan::Code(b"0xSECRETTEXT".to_vec())));
        let log = format!("{message:?}");
        assert!(!log.contains("SECRETTEXT"), "{log}");
        assert!(log.contains("Code(..)"), "{log}");
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
        // and the default folder holds no store: the welcome screen (S0).
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
            matches!(app.model.screen, Screen::Welcome),
            "{:?}",
            app.model.screen
        );
    }

    /// The snapshot file of a store, which `tawara_wallet_core::store_exists`
    /// looks for, as the library's `keystore::occupied` does. The interface
    /// only checks that the file is there, so a file of this name, empty, is
    /// enough for these tests: nothing here opens it.
    const STORE_SNAPSHOT: &str = "accounts.mks";

    /// The app-private folders the two phones' shells give `App::hosted`, as
    /// paths under a scratch folder: Android's `no_backup`, beside the app's
    /// `files`, and iOS's `Library/Application Support/Tawara` in the app's
    /// container, whose name has a space in it (Tawara-mobile's
    /// `crates/mobile/src/android.rs` and `ios.rs`). The store is looked for
    /// in a `keystore` folder inside either.
    const PHONE_LAYOUTS: [&str; 2] = [
        "data/user/0/com.patricksmithlaravel.tawara/no_backup",
        "var/mobile/Containers/Data/Application/0A1B2C3D/Library/Application Support/Tawara",
    ];

    /// A phone's private folder in `layout` under `scratch`, holding a store
    /// when `store` says so, and the store's folder as S7 names it.
    fn phone_folder(scratch: &Scratch, layout: &str, store: bool) -> (PathBuf, String) {
        let private = scratch.0.join(layout);
        let keystore = private.join("keystore");
        std::fs::create_dir_all(&private).unwrap();
        if store {
            std::fs::create_dir_all(&keystore).unwrap();
            std::fs::write(keystore.join(STORE_SNAPSHOT), b"").unwrap();
        }
        (private, keystore.display().to_string())
    }

    #[test]
    fn a_hosted_phone_with_no_store_starts_on_the_welcome_screen() {
        // The phone's own folder holds no store: S0 is the first screen, and
        // its "Get started" leads on to S1 (docs/DECISIONS.md D37), in either
        // phone's folder layout.
        let scratch = Scratch::new("hosted-welcome");
        for layout in PHONE_LAYOUTS {
            let (private, _) = phone_folder(&scratch, layout, false);
            let (mut app, _) = App::hosted(&private);
            assert!(
                matches!(app.model.screen, Screen::Welcome),
                "{layout}: {:?}",
                app.model.screen
            );
            assert!(!app.model.phone_holds_store(), "{layout}");
            let _ = app.update(Message::Go(Go::Start));
            assert!(
                matches!(app.model.screen, Screen::Start { .. }),
                "{layout}: {:?}",
                app.model.screen
            );
        }
    }

    #[test]
    fn a_hosted_phone_with_a_store_opens_on_unlock_and_go_start_does_nothing() {
        // The phone's own folder holds a store: S7 is its first screen, and
        // Go::Start leaves it there, so S1 and S3 are not reached
        // (docs/DECISIONS.md D37), in either phone's folder layout.
        let scratch = Scratch::new("hosted-store");
        for layout in PHONE_LAYOUTS {
            let (private, folder) = phone_folder(&scratch, layout, true);
            let (mut app, _) = App::hosted(&private);
            assert!(app.model.phone_holds_store(), "{layout}");
            assert!(
                matches!(&app.model.screen, Screen::Unlock(u) if u.dir == folder),
                "{layout}: {:?}",
                app.model.screen
            );
            let _ = app.update(Message::Go(Go::Start));
            assert!(
                matches!(&app.model.screen, Screen::Unlock(u) if u.dir == folder),
                "{layout}: {:?}",
                app.model.screen
            );
        }
    }

    #[test]
    fn a_hosted_phone_with_a_store_goes_back_from_the_node_to_unlock_not_to_the_start() {
        // Back from S2 leads to S1 only where S1 is offered: a phone that holds
        // a store goes back to S7, whichever screen S2 was opened from
        // (docs/DECISIONS.md D37).
        let scratch = Scratch::new("hosted-store-back");
        let (private, folder) = phone_folder(&scratch, PHONE_LAYOUTS[0], true);
        let (mut app, _) = App::hosted(&private);
        for back in [Back::Start, Back::Wallet] {
            app.model.screen = Screen::Node(NodeForm::default());
            app.go_back(back);
            assert!(
                matches!(&app.model.screen, Screen::Unlock(u) if u.dir == folder),
                "{back:?}: {:?}",
                app.model.screen
            );
        }
    }

    /// What a tap on S2's "Back" sends, for a node screen opened from `back`.
    fn node_back_link(app: &mut App, back: Back, size: (u32, u32)) -> Vec<Message> {
        app.model.screen = Screen::Node(NodeForm {
            url: String::new(),
            error: None,
            back,
        });
        let mut screen = Driver::new(crate::screens::view(&app.model), size);
        let at = screen.text_bounds("Back").expect("'Back' is drawn");
        let (messages, _, _) = screen.send(&[
            mouse_move(at.center_x(), at.center_y()),
            mouse_press(),
            mouse_release(),
        ]);
        messages
    }

    #[test]
    fn s2s_back_link_leads_a_phone_with_a_store_to_unlock_and_the_desktop_to_the_start() {
        // The link S2 draws leads where `go_back` does: on a phone that holds a
        // store, to S7, since a link to S1 would do nothing there; on the
        // desktop, to S1 as before (docs/DECISIONS.md D37).
        let scratch = Scratch::new("hosted-store-link");
        let (private, folder) = phone_folder(&scratch, PHONE_LAYOUTS[1], true);
        let (mut phone, _) = App::hosted(&private);
        for back in [Back::Start, Back::Wallet] {
            let messages = node_back_link(&mut phone, back, (360, 844));
            assert!(
                matches!(messages.as_slice(), [Message::Go(Go::Unlock)]),
                "{back:?}: {messages:?}"
            );
            for message in messages {
                let _ = phone.update(message);
            }
            assert!(
                matches!(&phone.model.screen, Screen::Unlock(u) if u.dir == folder),
                "{back:?}: {:?}",
                phone.model.screen
            );
        }

        let mut desktop = alone();
        let messages = node_back_link(&mut desktop, Back::Start, (1440, 900));
        assert!(
            matches!(messages.as_slice(), [Message::Go(Go::Start)]),
            "{messages:?}"
        );
    }

    #[test]
    fn on_the_desktop_go_start_offers_the_start_screen_whatever_store_there_is() {
        // The desktop is not confined, so no store changes what Go::Start
        // offers (docs/DECISIONS.md D37).
        let scratch = Scratch::new("desktop-store");
        let keystore = scratch.0.join("keystore");
        std::fs::create_dir_all(&keystore).unwrap();
        std::fs::write(keystore.join(STORE_SNAPSHOT), b"").unwrap();
        let mut app = alone();
        app.model.default_dir = Some(keystore.clone());
        app.model.screen = Screen::Unlock(UnlockForm {
            dir: keystore.display().to_string(),
            ..UnlockForm::default()
        });
        assert!(!app.model.phone_holds_store());
        let _ = app.update(Message::Go(Go::Start));
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
                    shown: true,
                    photographed: false,
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
            // Each form starts from a fresh first run, which has the default node
            // chosen (docs/DECISIONS.md D43); a form's own fields are what is counted.
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
                shown: true,
                photographed: false,
                words: ["abandon".into(), "abandon".into(), String::new()],
                error: None,
            }),
            Screen::Phrase(PhraseState {
                phrase: PhraseForDisplay::example(),
                dir: String::new(),
                positions: tawara_wallet_core::CONFIRM_POSITIONS,
                written: false,
                shown: false,
                photographed: false,
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
                shown: true,
                photographed: false,
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
    fn show_the_phrase_marks_the_words_shown_and_only_on_the_phrase() {
        // docs/DECISIONS.md D44: "Show the phrase" moves a short phone past the
        // library's warning. It changes nothing else, and does nothing on any
        // other screen.
        let mut app = alone();
        app.model.screen = Screen::Phrase(PhraseState {
            phrase: PhraseForDisplay::example(),
            dir: "/a/chosen/folder".to_owned(),
            positions: tawara_wallet_core::CONFIRM_POSITIONS,
            written: false,
            words: Default::default(),
            error: None,
            shown: false,
            photographed: false,
        });
        let _ = app.update(Message::ShowPhrase);
        assert!(
            matches!(&app.model.screen, Screen::Phrase(p) if p.shown && !p.written),
            "{:?}",
            app.model.screen
        );

        app.model.screen = Screen::Start {
            choice: StartChoice::Create,
        };
        let _ = app.update(Message::ShowPhrase);
        assert!(
            matches!(app.model.screen, Screen::Start { .. }),
            "{:?}",
            app.model.screen
        );
    }

    #[test]
    fn a_screenshot_is_noted_on_the_phrase_only_while_its_words_are_drawn() {
        // docs/DECISIONS.md D44 item 3: a screenshot taken while the words are
        // not drawn (a short phone's warning, before "Show the phrase") is noted
        // on no phrase, and one taken once they are drawn is noted.
        let phrase = |shown| {
            Screen::Phrase(PhraseState {
                phrase: PhraseForDisplay::example(),
                dir: "/a/chosen/folder".to_owned(),
                positions: tawara_wallet_core::CONFIRM_POSITIONS,
                written: false,
                words: Default::default(),
                error: None,
                shown,
                photographed: false,
            })
        };
        let noted = |app: &App| matches!(&app.model.screen, Screen::Phrase(p) if p.photographed);

        // A phone of `height` logical pixels, with the phone's insets.
        let phone_at = |height: f32| {
            let mut app = alone();
            app.model.width = NARROWEST;
            app.model.confine(PathBuf::from("/a/private"));
            app.model.safe_area = iced::mobile::SafeArea::new(iced::Padding {
                top: 47.0,
                right: 0.0,
                bottom: 34.0,
                left: 0.0,
            });
            app.model.height = height;
            app
        };

        // A short phone: 800 pixels less the insets is 719, under one frame.
        let mut app = phone_at(800.0);
        app.model.screen = phrase(false);
        app.model.capture.shot();
        let _ = app.update(Message::Tick(0));
        assert!(!noted(&app), "the words were not drawn");
        assert!(
            matches!(&app.model.screen, Screen::Phrase(p) if !p.shown && !p.photographed),
            "nothing else changed: {:?}",
            app.model.screen
        );
        let _ = app.update(Message::ShowPhrase);
        assert!(!noted(&app), "a screenshot before Show is not the words'");
        app.model.capture.shot();
        let _ = app.update(Message::Tick(0));
        assert!(noted(&app), "the words were drawn");
        // The warning takes the words' place, so they wait for Show again, and a
        // screenshot while they wait changes nothing (docs/DECISIONS.md D44 items
        // 1 and 3).
        assert!(
            matches!(&app.model.screen, Screen::Phrase(p) if !p.shown),
            "{:?}",
            app.model.screen
        );
        app.model.capture.shot();
        let _ = app.update(Message::Tick(0));
        assert!(
            matches!(&app.model.screen, Screen::Phrase(p) if p.photographed && !p.shown),
            "{:?}",
            app.model.screen
        );
        let _ = app.update(Message::ShowPhrase);
        assert!(
            matches!(&app.model.screen, Screen::Phrase(p) if p.photographed && p.shown),
            "{:?}",
            app.model.screen
        );
        app.model.capture.shot();
        let _ = app.update(Message::Tick(0));
        assert!(
            matches!(&app.model.screen, Screen::Phrase(p) if p.photographed && !p.shown),
            "a screenshot of the words hides them again: {:?}",
            app.model.screen
        );

        // A phone of 844 has its words on one frame, and a screenshot hides them
        // until Show is pressed as well.
        let mut app = phone_at(844.0);
        app.model.screen = phrase(false);
        app.model.capture.shot();
        let _ = app.update(Message::Tick(0));
        assert!(
            noted(&app),
            "the words are drawn on one frame, without Show"
        );
        assert!(
            matches!(&app.model.screen, Screen::Phrase(p) if !p.shown),
            "{:?}",
            app.model.screen
        );

        // A phone held sideways, 844 by 390, draws the desktop's frame. Its words
        // are drawn there, so a screenshot is noted, and the warning takes their
        // place until "Show the phrase" is pressed, as on a tall phone
        // (docs/DECISIONS.md D44 items 1 and 3). A screenshot while they wait is
        // not noted again.
        let mut app = phone_at(390.0);
        app.model.width = 844.0;
        app.model.screen = phrase(false);
        app.model.capture.shot();
        let _ = app.update(Message::Tick(0));
        assert!(noted(&app), "the words are drawn held sideways");
        assert!(
            matches!(&app.model.screen, Screen::Phrase(p) if !p.shown),
            "{:?}",
            app.model.screen
        );
        // The page draws the warning in the words' place, not the words above it.
        let drawn = crate::harness::texts(crate::screens::view(&app.model), (844.0, 390.0));
        assert!(
            !drawn.iter().any(|(t, _)| t == "abandon"),
            "the words are hidden: {drawn:?}"
        );
        assert!(
            drawn.iter().any(|(t, _)| t == "Show the phrase"),
            "the warning's button is drawn: {drawn:?}"
        );
        app.model.capture.shot();
        let _ = app.update(Message::Tick(0));
        assert!(
            matches!(&app.model.screen, Screen::Phrase(p) if p.photographed && !p.shown),
            "a screenshot while the warning shows changes nothing: {:?}",
            app.model.screen
        );
        let _ = app.update(Message::ShowPhrase);
        assert!(
            matches!(&app.model.screen, Screen::Phrase(p) if p.photographed && p.shown),
            "{:?}",
            app.model.screen
        );
        app.model.capture.shot();
        let _ = app.update(Message::Tick(0));
        assert!(
            matches!(&app.model.screen, Screen::Phrase(p) if p.photographed && !p.shown),
            "a screenshot of the words hides them again: {:?}",
            app.model.screen
        );

        let hidden = crate::harness::texts(crate::screens::view(&app.model), (844.0, 390.0));
        assert!(
            !hidden.iter().any(|(t, _)| t == "abandon"),
            "a screenshot after Show hides the words on the page: {hidden:?}"
        );

        // The desktop draws the words from the start.
        let mut app = alone();
        app.model.screen = phrase(false);
        app.model.capture.shot();
        let _ = app.update(Message::Tick(0));
        assert!(noted(&app));

        // No phrase on the screen: nothing to note, and nothing changes.
        let mut app = alone();
        app.model.capture.shot();
        let _ = app.update(Message::Tick(0));
        assert!(matches!(app.model.screen, Screen::Start { .. }));
    }

    #[test]
    fn the_capture_the_shell_reports_reaches_the_model_at_its_next_message() {
        let mut app = alone();
        app.model.capture.set_captured(true);
        assert!(!app.model.captured, "read at the next message, not before");
        let _ = app.update(Message::Tick(0));
        assert!(app.model.captured);
        app.model.capture.set_captured(false);
        let _ = app.update(Message::Input);
        assert!(!app.model.captured);
    }

    #[test]
    fn the_window_reports_its_height_with_its_width() {
        let mut app = alone();
        let _ = app.update(Message::Size(iced::Size::new(390.0, 844.0)));
        assert_eq!((app.model.width, app.model.height), (390.0, 844.0));
    }

    #[test]
    fn starting_over_keeps_the_folder_chosen() {
        let mut app = alone();
        app.model.screen = Screen::Confirm(PhraseState {
            phrase: PhraseForDisplay::example(),
            dir: "/a/chosen/folder".to_owned(),
            positions: tawara_wallet_core::CONFIRM_POSITIONS,
            written: true,
            shown: true,
            photographed: false,
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
    fn choosing_another_account_on_receive_drops_the_last_accounts_code() {
        let accounts = tawara_wallet_core::sample::wallet_view().accounts;
        let (first, second) = (accounts[0].id, accounts[1].id);
        let answer = |account| tawara_wallet_core::ReceiveView {
            account,
            destination: account.destination(),
            ..tawara_wallet_core::sample::receive_view()
        };
        let mut app = on_wallet(Page::Receive(ReceivePage::answered(first, answer(first))));
        wallet(&mut app, WalletMsg::ReceiveFor(second));
        let Page::Receive(r) = &wallet_page(&app).page else {
            unreachable!()
        };
        assert_eq!(r.account, Some(second));
        assert!(r.view.is_none() && r.code.is_none(), "{r:?}");

        // A late answer for the first account is not the page's.
        let _ = app.on_reply(Purpose::Receive, Reply::Receive(answer(first)));
        let Page::Receive(r) = &wallet_page(&app).page else {
            unreachable!()
        };
        assert!(r.view.is_none() && r.code.is_none(), "{r:?}");

        let _ = app.on_reply(Purpose::Receive, Reply::Receive(answer(second)));
        let Page::Receive(r) = &wallet_page(&app).page else {
            unreachable!()
        };
        assert_eq!(r.view.as_ref().map(|v| v.account), Some(second));
        assert!(
            r.code.is_some(),
            "the second account's destination has a code"
        );
    }

    #[test]
    fn a_row_opened_in_place_is_one_at_a_time_and_closes_when_the_page_is_left() {
        // W1 on a phone (docs/DECISIONS.md D39, item 4): one account's row is
        // open at a time, and the page forgets it when it is left. Send from it
        // opens Send with that account as From.
        let accounts = tawara_wallet_core::sample::wallet_view().accounts;
        let (first, second) = (accounts[0].id, accounts[1].id);
        let mut app = on_wallet(Page::Dashboard);
        wallet(&mut app, WalletMsg::Expand(Some(first)));
        assert_eq!(wallet_page(&app).expanded, Some(first));
        wallet(&mut app, WalletMsg::Expand(Some(second)));
        assert_eq!(
            wallet_page(&app).expanded,
            Some(second),
            "one row at a time"
        );
        wallet(&mut app, WalletMsg::Expand(None));
        assert_eq!(wallet_page(&app).expanded, None);

        wallet(&mut app, WalletMsg::Expand(Some(first)));
        wallet(&mut app, WalletMsg::Open(To::Settings));
        assert!(matches!(wallet_page(&app).page, Page::Settings(_)));
        assert_eq!(wallet_page(&app).expanded, None, "closed when left");

        wallet(&mut app, WalletMsg::Expand(Some(first)));
        wallet(&mut app, WalletMsg::Open(To::Send(Some(first))));
        assert!(
            matches!(&wallet_page(&app).page, Page::Send(s) if s.from == Some(first)),
            "{:?}",
            wallet_page(&app).page
        );
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
                reply: Box::new(Reply::Sent(sent.clone())),
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

        // Re-signed, the same bytes again: kept once, in the place it held, and
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
            Purpose::Settle(sent.from),
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

    /// What was typed for the second of the two spends [`two_spends`] signs:
    /// other parameters than [`spend_typed`].
    fn second_form() -> SpendForm {
        SpendForm {
            rows: vec![DestinationRow {
                to: "other".to_owned(),
                amount: "2".to_owned(),
                reference: String::new(),
            }],
            fee: "1000".to_owned(),
            ..SpendForm::default()
        }
    }

    /// Signs `sent` from the review of `plan`, typed as `form`, and leaves its
    /// page for the dashboard, so that the spend is kept.
    fn signed_and_left(
        app: &mut App,
        plan: tawara_wallet_core::PlanView,
        form: SpendForm,
        sent: tawara_wallet_core::SentView,
    ) {
        app.model.screen = Screen::Wallet(WalletPage {
            page: Page::Send(SendPage {
                from: Some(plan.from),
                form,
                stage: SendStage::Review(Box::new(plan)),
            }),
            ..WalletPage::default()
        });
        let _ = app.on_reply(Purpose::ConfirmSend, Reply::Sent(sent));
        wallet(app, WalletMsg::Open(To::Dashboard));
    }

    /// Two spends this run signed from the sample account, in this order, with
    /// the dashboard shown after both. The first, typed as [`spend_typed`], was
    /// left unsaved and has settled since, so its account's next key is the
    /// second's. The second, typed as `second`, holds the account's open
    /// reservation, and is left unsaved. Their reservations are at the one-time
    /// key indexes 49 and 50. Returns the application and the two spends.
    fn two_spends(
        second: SpendForm,
    ) -> (
        App,
        tawara_wallet_core::SentView,
        tawara_wallet_core::SentView,
    ) {
        use tawara_wallet_core::view::{AccountState, ReservationState, SettledAccount};
        let mut app = alone();
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        let plan = tawara_wallet_core::sample::plan_view();
        let first = tawara_wallet_core::sample::sent_view();
        signed_and_left(&mut app, plan.clone(), spend_typed(), first.clone());
        // The first lands, and the check settles it on its own.
        let _ = app.on_reply(
            Purpose::Status,
            Reply::Status {
                account: first.from,
                state: AccountState::InSync { balance: 1 },
                spendable: true,
                text: "settled".to_owned(),
                settled: Some(SettledAccount {
                    account: first.from,
                    text: "settled: the library's page".to_owned(),
                }),
            },
        );
        let mut next = tawara_wallet_core::sample::sent_view();
        next.spent_index = Some(50);
        next.artifact_hex = "bb".repeat(8);
        next.view.accounts[0].set_state(AccountState::SpendOutstanding {
            balance: 1,
            spent_index: 50,
            reservation: ReservationState::Live,
        });
        signed_and_left(&mut app, plan, second, next.clone());
        (app, first, next)
    }

    /// The bytes of the kept spends, in the order the application keeps them.
    fn kept_bytes(app: &App) -> Vec<String> {
        app.model
            .signed
            .iter()
            .map(|s| s.sent.sent.artifact_hex.clone())
            .collect()
    }

    #[test]
    fn a_re_sign_offers_the_spend_its_reservation_holds_and_leaving_pages_keeps_the_order() {
        // D38, as docs/SCREENS.md W6 and W8 state the rules: the first spend is left
        // unsaved and settles, and the second, with other parameters, holds the
        // account's reservation. Reopening the first, and then the account's page,
        // reorders nothing, and the re-sign offers the second's parameters. The library
        // accepts those for its reservation, as the worker test
        // `a_re_sign_is_for_the_spend_whose_reservation_is_open_and_the_library_refuses_another`
        // shows.
        let (mut app, first, second) = two_spends(second_form());
        let from = first.from;
        let both = vec![first.artifact_hex.clone(), second.artifact_hex.clone()];
        assert_eq!(kept_bytes(&app), both);
        wallet(&mut app, WalletMsg::ShowSigned(first.artifact_hex.clone()));
        wallet(&mut app, WalletMsg::Open(To::Account(from)));
        assert_eq!(kept_bytes(&app), both, "navigation reorders nothing");
        wallet(&mut app, WalletMsg::Open(To::Resign(from)));
        assert!(
            matches!(&wallet_page(&app).page,
                Page::Resign(ResignPage { form, .. }) if *form == second_form()),
            "{:?}",
            wallet_page(&app).page
        );
    }

    #[test]
    fn a_re_sign_offers_nothing_once_the_spend_holding_the_reservation_is_dismissed() {
        // D38, as docs/SCREENS.md W8 states the rule: the second spend holds the
        // account's reservation, and is dismissed, its bytes wiped. The first is
        // another spend, still kept: the re-sign offers nothing for the reservation,
        // not the first spend's parameters, which the library would refuse.
        let (mut app, first, second) = two_spends(second_form());
        wallet(
            &mut app,
            WalletMsg::DismissSigned(second.artifact_hex.clone()),
        );
        assert_eq!(kept_bytes(&app), vec![first.artifact_hex.clone()]);
        wallet(&mut app, WalletMsg::Open(To::Resign(first.from)));
        assert!(
            matches!(&wallet_page(&app).page,
                Page::Resign(ResignPage { form, .. }) if *form == SpendForm::default()),
            "{:?}",
            wallet_page(&app).page
        );
    }

    #[test]
    fn a_re_sign_keeps_the_place_its_spend_holds_in_the_signing_order() {
        // D38, as docs/SCREENS.md W6 states the rule: a re-sign gives the same bytes
        // again, so its spend keeps the place it held, though a spend of another
        // account was signed after it.
        use tawara_wallet_core::view::{AccountState, ReservationState};
        let (mut app, first, second) = two_spends(second_form());
        let other = app.model.wallet.as_ref().map(|w| w.accounts[1].id);
        let mut third = tawara_wallet_core::sample::sent_view();
        third.from = other.expect("the store is open");
        third.spent_index = Some(6);
        third.artifact_hex = "cc".repeat(8);
        third.view = second.view.clone();
        third.view.accounts[1].set_state(AccountState::SpendOutstanding {
            balance: 1,
            spent_index: 6,
            reservation: ReservationState::Live,
        });
        signed_and_left(
            &mut app,
            tawara_wallet_core::sample::plan_view(),
            SpendForm::default(),
            third.clone(),
        );
        wallet(&mut app, WalletMsg::Open(To::Resign(second.from)));
        let _ = app.on_reply(Purpose::Resign, Reply::Sent(second.clone()));
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        assert_eq!(
            kept_bytes(&app),
            vec![
                first.artifact_hex.clone(),
                second.artifact_hex.clone(),
                third.artifact_hex.clone(),
            ]
        );
        assert!(app.model.signed[1].sent.resigned);
    }

    #[test]
    fn a_re_signed_spend_is_not_offered_beside_its_own_page() {
        // docs/SCREENS.md W6: a re-sign's page takes the place of its spend's
        // kept copy, so no "Open it" banner offers the spend shown. Left, it is
        // kept once, and offered again.
        let (mut app, sent) = just_sent();
        let from = sent.from;
        wallet(&mut app, WalletMsg::Open(To::Account(from)));
        assert_eq!(app.model.unsaved_signed().count(), 1);
        wallet(&mut app, WalletMsg::Open(To::Resign(from)));
        let _ = app.on_reply(Purpose::Resign, Reply::Sent(sent.clone()));
        assert!(sent_shown(&app).is_some_and(|(_, s)| s.resigned && s.unsaved()));
        assert_eq!(
            app.model.unsaved_signed().count(),
            0,
            "no banner offers the spend its own page shows"
        );
        assert!(app.model.signed.is_empty());
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        assert_eq!(kept_bytes(&app), vec![sent.artifact_hex.clone()]);
        assert_eq!(app.model.unsaved_signed().count(), 1);
    }

    #[test]
    fn a_re_sign_that_could_not_read_its_reservation_keeps_the_index_it_had() {
        // docs/SCREENS.md W8: a re-sign whose read of the store failed reports
        // no index. The spend keeps the one its sign reported, so the next
        // re-sign still offers what was typed for it, not an empty form.
        let (mut app, sent) = just_sent();
        let from = sent.from;
        wallet(&mut app, WalletMsg::Open(To::Account(from)));
        wallet(&mut app, WalletMsg::Open(To::Resign(from)));
        let mut again = sent.clone();
        again.spent_index = None;
        let _ = app.on_reply(Purpose::Resign, Reply::Sent(again));
        assert!(sent_shown(&app).is_some_and(|(_, s)| s.sent.spent_index == Some(49)));
        wallet(&mut app, WalletMsg::Open(To::Resign(from)));
        assert!(
            matches!(&wallet_page(&app).page,
                Page::Resign(ResignPage { form, .. }) if *form == spend_typed()),
            "{:?}",
            wallet_page(&app).page
        );
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
            Purpose::Settle(sent.from),
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
    fn a_move_to_the_background_releases_the_bytes_a_failed_read_back_kept() {
        // A store the worker closed for a read-back that failed keeps its unsaved
        // bytes, and sends no `Locked` for the move: the move wipes them, and S7
        // says so (docs/DECISIONS.md D38 item 5; docs/SCREENS.md W6).
        let scratch = Scratch::new("background-kept");
        let (mut app, sent) = signed_then_locked(&scratch, LockReason::ReopenFailed);
        assert_eq!(app.model.signed.len(), 1, "kept, as the close leaves it");
        let _ = app.on_event(Event::Backgrounded { seq: 1 });
        assert!(
            app.model.signed.is_empty(),
            "the kept bytes went with the move"
        );
        match &app.model.screen {
            Screen::Unlock(u) => {
                let note = u.note.as_deref().unwrap_or_default();
                assert!(note.contains("wiped"), "{note}");
                assert!(!note.contains("shown again"), "{note}");
            }
            other => panic!("expected the unlock screen, got {other:?}"),
        }
        // Unlocked again: the page does not come back, and nothing is offered.
        let _ = app.on_reply(Purpose::Unlock, Reply::Unlocked(sent.view.clone()));
        assert!(sent_shown(&app).is_none());
        assert_eq!(app.model.unsaved_signed().count(), 0);
    }

    #[test]
    fn a_move_to_the_background_wipes_a_shown_unsaved_page_when_no_lock_comes() {
        // The page is shown, and the move's `Locked` does not come: the move
        // wipes it all the same (docs/SCREENS.md W6).
        let (mut app, _) = just_sent();
        let _ = app.on_event(Event::Backgrounded { seq: 1 });
        assert!(
            app.model.signed.is_empty(),
            "the shown bytes went with the move"
        );
        assert!(sent_shown(&app).is_none(), "and so did their page");
    }

    #[test]
    fn a_graceful_shutdown_releases_the_unsaved_bytes_and_a_panic_keeps_them() {
        // The shutdown ends the run, so unsaved bytes go even when no `Locked`
        // came with it; a worker that panicked keeps them to save (docs/
        // DECISIONS.md D38 item 5; docs/SCREENS.md W6).
        let scratch = Scratch::new("shutdown-kept-panic");
        let (mut app, _) = signed_then_locked(&scratch, LockReason::ReopenFailed);
        let _ = app.on_event(Event::Stopped { panicked: true });
        assert_eq!(app.model.signed.len(), 1, "a panic keeps the kept bytes");
        assert!(app.model.signed[0].unsaved());

        let scratch = Scratch::new("shutdown-kept");
        let (mut app, _) = signed_then_locked(&scratch, LockReason::ReopenFailed);
        let _ = app.on_event(Event::Stopped { panicked: false });
        assert!(
            app.model.signed.is_empty(),
            "the shutdown released the kept bytes"
        );

        let (mut app, _) = just_sent();
        let _ = app.on_event(Event::Stopped { panicked: false });
        assert!(app.model.signed.is_empty(), "and the shown bytes");
    }

    /// Stops the worker as a panic leaves it to the interface: its queue goes
    /// before any notice of the stop reaches the interface, so a command sent
    /// now fails. The notice is not read here; a test hands it on itself.
    fn queue_gone(app: &App) {
        let handle = &app.worker.as_ref().expect("a worker").handle;
        handle.shutdown();
        let deadline = Instant::now() + Duration::from_secs(30);
        while handle.send(Command::Review).is_ok() {
            assert!(Instant::now() < deadline, "the worker's queue did not go");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_send_that_fails_before_a_panic_notice_keeps_the_unsaved_bytes() {
        // The owner's review (P1): a worker that panics drops its queue before its
        // `Stopped` reaches the interface, so a command sent in between fails. The
        // failure releases nothing: the notice that follows says whether the worker
        // panicked, and a panic keeps the bytes (docs/DECISIONS.md D38 item 5;
        // docs/SCREENS.md W6).
        let scratch = Scratch::new("send-fails-then-panic");
        let (mut app, _) = signed_then_locked(&scratch, LockReason::ReopenFailed);
        assert_eq!(app.model.signed.len(), 1, "kept, as the close leaves it");
        queue_gone(&app);
        assert!(app.send(Command::Review, Purpose::Review).is_none());
        assert_eq!(app.model.signed.len(), 1, "the failure released nothing");
        assert!(app.model.signed[0].unsaved());
        assert_eq!(app.model.stopped, Some(false), "the stop shows at once");

        let _ = app.on_event(Event::Stopped { panicked: true });
        assert_eq!(app.model.stopped, Some(true));
        assert_eq!(app.model.signed.len(), 1, "the panic keeps the bytes");
        assert!(app.model.signed[0].unsaved());
    }

    #[test]
    fn a_graceful_stop_after_a_failed_send_releases_the_unsaved_bytes() {
        // The counterpart: the notice says the stop was graceful, so the bytes the
        // failed send kept are released, as the shutdown releases them.
        let scratch = Scratch::new("send-fails-then-graceful");
        let (mut app, _) = signed_then_locked(&scratch, LockReason::ReopenFailed);
        queue_gone(&app);
        assert!(app.send(Command::Review, Purpose::Review).is_none());
        assert_eq!(
            app.model.signed.len(),
            1,
            "not yet: the reason is not known"
        );

        let _ = app.on_event(Event::Stopped { panicked: false });
        assert_eq!(app.model.stopped, Some(false));
        assert!(app.model.signed.is_empty(), "a graceful stop releases them");
    }

    #[test]
    fn a_move_after_a_failed_send_with_no_notice_keeps_the_bytes_until_the_app_ends() {
        // A failed send and then no notice at all: the move to the background
        // releases nothing, since the reason is not known and a panic keeps the
        // bytes. Leaving the application releases them (docs/DECISIONS.md D38
        // item 5).
        let scratch = Scratch::new("send-fails-then-move");
        let (mut app, _) = signed_then_locked(&scratch, LockReason::ReopenFailed);
        queue_gone(&app);
        assert!(app.send(Command::Review, Purpose::Review).is_none());
        app.model.left.fetch_add(1, Ordering::SeqCst);
        let _ = app.update(Message::Input);
        assert_eq!(app.model.signed.len(), 1, "the move released nothing");
        assert!(app.model.signed[0].unsaved());

        let before = RELEASED_IN_TESTS.with(std::cell::Cell::get);
        drop(app);
        assert_eq!(
            RELEASED_IN_TESTS.with(std::cell::Cell::get) - before,
            1,
            "leaving the application released the kept bytes"
        );
    }

    #[test]
    fn a_move_notice_after_a_failed_send_releases_nothing_and_a_panic_keeps_the_bytes() {
        // The worker's `Backgrounded` for a move it took before it stopped can come
        // after the failed send. For a store its read-back closed, that notice
        // would release the bytes, so it must not while the reason is unknown; the
        // panic that follows keeps them.
        let scratch = Scratch::new("send-fails-then-backgrounded");
        let (mut app, _) = signed_then_locked(&scratch, LockReason::ReopenFailed);
        queue_gone(&app);
        assert!(app.send(Command::Review, Purpose::Review).is_none());

        let _ = app.on_event(Event::Backgrounded { seq: 1 });
        assert_eq!(app.model.signed.len(), 1, "the notice released nothing");
        let _ = app.on_event(Event::Stopped { panicked: true });
        assert_eq!(app.model.signed.len(), 1, "and the panic keeps them");
    }

    /// The application with a store its read-back closed and one unsaved spend
    /// kept, after a send failed for a queue that went: then the worker's
    /// `Locked` for `reason`, sent before it stopped, is taken.
    fn locked_after_a_failed_send(scratch: &Scratch, reason: LockReason) -> App {
        let (mut app, _) = signed_then_locked(scratch, LockReason::ReopenFailed);
        queue_gone(&app);
        assert!(app.send(Command::Review, Purpose::Review).is_none());
        let _ = app.on_event(Event::Locked { reason });
        app
    }

    #[test]
    fn a_lock_notice_after_a_failed_send_releases_nothing_and_a_panic_keeps_the_bytes() {
        // A lock the worker sent just before it panicked can reach the interface
        // after a send failed. While the stop's reason is not known, the lock
        // keeps the bytes, as a move's notice does; the panic that follows keeps
        // them (docs/DECISIONS.md D38 item 5; docs/SCREENS.md W6).
        for reason in [LockReason::Asked, LockReason::Idle, LockReason::Background] {
            let scratch = Scratch::new("send-fails-then-locked");
            let mut app = locked_after_a_failed_send(&scratch, reason);
            assert_eq!(
                app.model.signed.len(),
                1,
                "the lock ({reason:?}) released nothing"
            );
            assert!(app.model.signed[0].unsaved());
            let _ = app.on_event(Event::Stopped { panicked: true });
            assert_eq!(app.model.signed.len(), 1, "and the panic keeps them");
            assert!(app.model.signed[0].unsaved());
        }
    }

    #[test]
    fn a_lock_notice_after_a_failed_send_then_a_graceful_stop_releases_the_bytes() {
        // The counterpart: the notice that follows says the stop was graceful, so
        // the bytes the lock kept are released then.
        let scratch = Scratch::new("send-fails-locked-graceful");
        let mut app = locked_after_a_failed_send(&scratch, LockReason::Background);
        assert_eq!(
            app.model.signed.len(),
            1,
            "not yet: the reason is not known"
        );
        let _ = app.on_event(Event::Stopped { panicked: false });
        assert!(app.model.signed.is_empty(), "a graceful stop releases them");
    }

    #[test]
    fn a_shutdown_lock_after_a_failed_send_releases_the_bytes() {
        // A shutdown's lock says the stop is graceful, so it releases the bytes,
        // whether or not a send failed first.
        let scratch = Scratch::new("send-fails-then-shutdown-lock");
        let app = locked_after_a_failed_send(&scratch, LockReason::Shutdown);
        assert!(app.model.signed.is_empty(), "the shutdown releases them");
    }

    #[test]
    fn a_spend_answered_after_a_failed_send_is_taken_and_its_bytes_are_kept() {
        // The worker signed a spend and then stopped, so the answer comes after the
        // failed send. It is taken, and its bytes are kept with the stop: they are
        // not lost to a queue dropped first.
        let scratch = Scratch::new("send-fails-then-answer");
        let (mut app, _events) = app(&scratch);
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        let id = RequestId::example();
        app.worker
            .as_mut()
            .expect("a worker")
            .waiting
            .push((id, Purpose::ConfirmSend));
        queue_gone(&app);
        assert!(app.send(Command::Review, Purpose::Review).is_none());

        let sent = tawara_wallet_core::sample::sent_view();
        let _ = app.on_event(Event::Done {
            id,
            reply: Box::new(Reply::Sent(sent.clone())),
        });
        assert!(sent_shown(&app).is_some(), "the answer is taken");

        let _ = app.on_event(Event::Stopped { panicked: true });
        assert_eq!(app.model.signed.len(), 1, "kept with the stop");
        assert_eq!(
            app.model.signed[0].sent.sent.artifact_hex,
            sent.artifact_hex
        );
        assert!(app.model.signed[0].unsaved());
    }

    #[test]
    fn leaving_the_application_releases_the_unsaved_bytes_it_still_holds() {
        // The application's end, with the store closed for a read-back that
        // failed and no event reaching it: its drop releases the kept bytes
        // (docs/DECISIONS.md D38 item 5; docs/SCREENS.md W6).
        let scratch = Scratch::new("drop-kept");
        let (app, _) = signed_then_locked(&scratch, LockReason::ReopenFailed);
        let before = RELEASED_IN_TESTS.with(std::cell::Cell::get);
        drop(app);
        assert_eq!(
            RELEASED_IN_TESTS.with(std::cell::Cell::get) - before,
            1,
            "the kept bytes went with the application"
        );
    }

    /// Marks the spend's page shown on W6 as saved to a file.
    fn save_shown(app: &mut App) {
        let Screen::Wallet(WalletPage {
            page:
                Page::Send(SendPage {
                    stage: SendStage::Sent(s),
                    ..
                }),
            ..
        }) = &mut app.model.screen
        else {
            panic!("expected the sent page");
        };
        s.saved = Some(Ok(std::env::temp_dir().join("tawara-saved-shown.hex")));
    }

    #[test]
    fn a_saved_spend_stays_through_a_move_and_a_shutdown_with_no_lock() {
        // docs/SCREENS.md W6: only unsaved bytes go with a move to the
        // background or a shutdown; a saved spend is kept, shown or not.
        let scratch = Scratch::new("saved-kept-boundaries");
        let (mut app, sent) = signed_then_locked(&scratch, LockReason::ReopenFailed);
        app.model.signed[0].sent.saved =
            Some(Ok(std::env::temp_dir().join("tawara-saved-kept.hex")));
        let _ = app.on_event(Event::Backgrounded { seq: 1 });
        assert_eq!(app.model.signed.len(), 1, "the move keeps a saved spend");
        assert_eq!(
            app.model.signed[0].sent.sent.artifact_hex,
            sent.artifact_hex
        );
        let _ = app.on_event(Event::Stopped { panicked: false });
        assert_eq!(app.model.signed.len(), 1, "and so does the shutdown");
        assert_eq!(
            app.model.signed[0].sent.sent.artifact_hex,
            sent.artifact_hex
        );

        let (mut app, sent) = just_sent();
        save_shown(&mut app);
        let _ = app.on_event(Event::Backgrounded { seq: 1 });
        let (_, shown) = sent_shown(&app).expect("a saved page stays shown");
        assert_eq!(shown.sent.artifact_hex, sent.artifact_hex);
    }

    #[test]
    fn leaving_the_application_releases_a_shown_unsaved_page_and_keeps_a_saved_one() {
        // docs/SCREENS.md W6: the drop releases unsaved bytes shown as well as
        // kept, and a saved spend is not released.
        let (app, _) = just_sent();
        let before = RELEASED_IN_TESTS.with(std::cell::Cell::get);
        drop(app);
        assert_eq!(
            RELEASED_IN_TESTS.with(std::cell::Cell::get) - before,
            1,
            "the shown bytes went with the application"
        );

        let (mut app, _) = just_sent();
        save_shown(&mut app);
        let before = RELEASED_IN_TESTS.with(std::cell::Cell::get);
        drop(app);
        assert_eq!(
            RELEASED_IN_TESTS.with(std::cell::Cell::get) - before,
            0,
            "a saved page is not released"
        );
    }

    #[test]
    fn a_move_with_a_store_open_wipes_at_its_lock_and_s7_says_so() {
        // The worker sends `Locked` before `Backgrounded` (wallet-core's
        // `every_move_to_the_background_is_reported_after_any_lock`): the lock
        // wipes the shown bytes and S7 says so, and the move's release finds
        // nothing more (docs/SCREENS.md W6).
        let (mut app, _) = just_sent();
        let _ = app.on_event(Event::Locked {
            reason: LockReason::Background,
        });
        let before = RELEASED_IN_TESTS.with(std::cell::Cell::get);
        let _ = app.on_event(Event::Backgrounded { seq: 1 });
        assert_eq!(RELEASED_IN_TESTS.with(std::cell::Cell::get) - before, 0);
        assert!(app.model.signed.is_empty());
        match &app.model.screen {
            Screen::Unlock(u) => {
                let note = u.note.as_deref().unwrap_or_default();
                assert!(note.contains(WIPED_NOTE), "{note}");
            }
            other => panic!("expected the unlock screen, got {other:?}"),
        }
    }

    #[test]
    fn a_stopped_worker_keeps_the_unsaved_bytes_through_a_move() {
        // docs/DECISIONS.md D38 item 5; docs/SCREENS.md W6: a worker that
        // stopped on a fault keeps unsaved bytes until Tawara closes, a move
        // to the background included, for the stopped screen to save.
        let (mut app, sent) = just_sent();
        let _ = app.on_event(Event::Stopped { panicked: true });
        app.model.left.fetch_add(1, Ordering::SeqCst);
        let _ = app.update(Message::Tick(now_ms()));
        assert_eq!(app.model.signed.len(), 1);
        assert_eq!(
            app.model.signed[0].sent.sent.artifact_hex,
            sent.artifact_hex
        );
    }

    #[test]
    fn signed_bytes_pasted_on_w9_are_wiped_with_a_move_to_the_background() {
        // A pasted artifact may be a copy of unsaved bytes, so it goes as they
        // do (docs/DECISIONS.md D38 item 5): wiped where it is, with the typed
        // secrets, on a move to the background, and when its page or screen is
        // left.
        let mut app = on_wallet(Page::Submit(SubmitPage::default()));
        let _ = app.update(Message::Pasted(
            PasteInto::SignedBytes,
            Some(Clip("4f2a00ab11cd".to_owned())),
        ));
        assert!(matches!(&wallet_page(&app).page, Page::Submit(s) if !s.hex.is_empty()));
        app.model.backgrounded();
        assert!(
            matches!(&wallet_page(&app).page, Page::Submit(s) if s.hex.is_empty()),
            "{:?}",
            wallet_page(&app).page
        );
    }

    #[test]
    fn a_saved_copy_written_to_the_node_shows_on_w9_and_its_account_as_read() {
        // docs/DECISIONS.md D38 item 5 (b): the answer carries the store as the
        // read after the write left it, so the account's row is what the chain
        // holds now, and W9 shows the written copy's report.
        let view = tawara_wallet_core::sample::wallet_view();
        let account = view.accounts[2].id;
        let mut app = on_wallet(Page::Submit(SubmitPage {
            hex: "4f2a00ab11cd".to_owned(),
            mode: SubmitMode::Resubmit(account),
            result: None,
        }));
        let mut read = view;
        read.accounts[2].state = tawara_wallet_core::view::AccountState::InSync { balance: 1 };
        let text = "The saved copy was written to the node's socket, and the node echoed its \
                    transaction id, 5e5e."
            .to_owned();
        let _ = app.on_reply(
            Purpose::Resubmit(account),
            Reply::Resubmitted {
                tx_id: "5e5e".to_owned(),
                text: text.clone(),
                view: read,
            },
        );
        assert!(
            matches!(&wallet_page(&app).page, Page::Submit(s) if s.result == Some((true, text.clone()))),
            "{:?}",
            wallet_page(&app).page
        );
        assert!(
            matches!(
                &app.model.wallet,
                Some(w) if w.accounts[2].state
                    == tawara_wallet_core::view::AccountState::InSync { balance: 1 }
            ),
            "the account is read again"
        );
    }

    #[test]
    fn a_saved_copy_the_library_refuses_is_read_again_quietly() {
        // A write that failed after the node received the copy leaves it there,
        // so the account is read again, quietly, as a sign the library refused
        // is (docs/DECISIONS.md D38 items 2 and 5 (b)). A refusal the wallet makes
        // itself, for a locked store, reads nothing.
        use tawara_wallet_core::{Refusal, RefusalKind};
        let scratch = Scratch::new("resubmit-refused");
        let (mut app, _events) = app(&scratch);
        let view = tawara_wallet_core::sample::wallet_view();
        let account = view.accounts[2].id;
        app.model.wallet = Some(view);
        app.model.screen = Screen::Wallet(WalletPage {
            page: Page::Submit(SubmitPage {
                hex: "4f2a00ab11cd".to_owned(),
                mode: SubmitMode::Resubmit(account),
                result: None,
            }),
            ..WalletPage::default()
        });
        let _ = app.on_reply(
            Purpose::Resubmit(account),
            Reply::Refused(Refusal {
                kind: RefusalKind::NotUnlocked,
                text: "the wallet is locked".to_owned(),
            }),
        );
        assert!(!app.waiting_on(|p| matches!(p, Purpose::Poll)));
        let _ = app.on_reply(
            Purpose::Resubmit(account),
            Reply::Refused(Refusal {
                kind: RefusalKind::Library,
                text: "the library refused".to_owned(),
            }),
        );
        assert!(app.waiting_on(|p| matches!(p, Purpose::Poll)), "read again");
        assert_eq!(
            wallet_page(&app).error.as_deref(),
            Some("the library refused")
        );
    }

    #[test]
    fn a_saved_copy_the_library_refuses_shows_its_words_on_w9() {
        // The refusal is the library's own text, as W9's error. The page keeps
        // no result, since nothing was written (docs/DECISIONS.md D38 item 5 (b)).
        let account = tawara_wallet_core::sample::wallet_view().accounts[2].id;
        let mut app = on_wallet(Page::Submit(SubmitPage {
            hex: "4f2a00ab11cd".to_owned(),
            mode: SubmitMode::Resubmit(account),
            result: None,
        }));
        let words = "the balance at the source is 9 nanoMCM, not the 8 the settled spend was \
                     built against, so the saved artifact can no longer be accepted. Nothing was \
                     submitted."
            .to_owned();
        let _ = app.on_reply(
            Purpose::Resubmit(account),
            Reply::Refused(Refusal {
                kind: RefusalKind::Library,
                text: words.clone(),
            }),
        );
        assert_eq!(wallet_page(&app).error.as_deref(), Some(words.as_str()));
        assert!(
            matches!(&wallet_page(&app).page, Page::Submit(s) if s.result.is_none()),
            "{:?}",
            wallet_page(&app).page
        );
    }

    #[test]
    fn an_opened_store_reads_every_account_however_it_was_opened() {
        // view::Reads: an opened or refreshed store reads every account, a
        // store just created among them (docs/DECISIONS.md D38 item 2;
        // docs/SCREENS.md W7).
        let view = tawara_wallet_core::sample::wallet_view();
        let first = tawara_wallet_core::view::AccountId::from_tag([0; 20]);
        for purpose in [Purpose::CreateConfirm, Purpose::CreateFromPhrase] {
            let reply = Reply::Created {
                dir: view.dir.clone(),
                first,
                opened: Ok(view.clone()),
            };
            assert_eq!(reads_of(purpose, &reply), Reads::Every, "{purpose:?}");
        }
        let refused = Reply::Created {
            dir: view.dir.clone(),
            first,
            opened: Err(Refusal {
                kind: RefusalKind::StoreInUse,
                text: "the store is in use".to_owned(),
            }),
        };
        assert_eq!(reads_of(Purpose::CreateConfirm, &refused), Reads::Nothing);
    }

    #[test]
    fn a_recovery_review_reads_every_account_it_reconciled() {
        // view::Reads: a review reconciles every account of the store and updates
        // each row, so its answer reads them all (docs/DECISIONS.md D38 item 2).
        let reports = tawara_wallet_core::sample::review();
        assert_eq!(
            reads_of(Purpose::Review, &Reply::Reviewed(reports)),
            Reads::Every
        );
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

    /// A destination that is one: the library's own rendering of a tag.
    fn a_destination() -> String {
        AccountId::from_tag([9; 20])
            .destination()
            .expect("a tag renders")
    }

    /// The sample's account that can spend, as a send page names it.
    fn sample_from() -> AccountId {
        tawara_wallet_core::sample::wallet_view()
            .accounts
            .first()
            .expect("the sample has an account")
            .id
    }

    /// A spend form of one destination, `to`, for `amount` MCM.
    fn spend_to(to: &str, amount: &str) -> SpendForm {
        SpendForm {
            rows: vec![DestinationRow {
                to: to.to_owned(),
                amount: amount.to_owned(),
                reference: String::new(),
            }],
            ..SpendForm::default()
        }
    }

    /// A destination row with only its address typed.
    fn to_row(to: &str) -> DestinationRow {
        DestinationRow {
            to: to.to_owned(),
            amount: String::new(),
            reference: String::new(),
        }
    }

    /// A send page in `stage`, with `form` typed and the sample's account.
    fn send_page(stage: SendStage, form: SpendForm) -> Page {
        Page::Send(SendPage {
            from: Some(sample_from()),
            form,
            stage,
        })
    }

    /// Refusals by row and field, as Continue leaves them on the destinations
    /// page.
    fn refusals_at(
        at: &[((usize, RefusedField), &str)],
    ) -> std::collections::BTreeMap<(usize, RefusedField), String> {
        at.iter()
            .map(|(key, why)| (*key, (*why).to_owned()))
            .collect()
    }

    /// The refusals the destinations page shown holds.
    fn refusals_of(app: &App) -> std::collections::BTreeMap<(usize, RefusedField), String> {
        match &wallet_page(app).page {
            Page::Send(SendPage {
                stage: SendStage::To(refused),
                ..
            }) => refused.clone(),
            other => panic!("the destinations page is not shown: {other:?}"),
        }
    }

    /// The sample store on a send page, on a phone: its folder is confined,
    /// and there is no worker.
    fn on_phone(page: Page) -> App {
        let mut app = on_wallet(page);
        app.model
            .confine(std::env::temp_dir().join("tawara-phone-folder"));
        app
    }

    /// The sample store on a send page, on a phone, over a real worker whose
    /// stores are in `scratch`: what it is sent is waiting for its answer.
    fn on_phone_with_worker(scratch: &Scratch, page: Page) -> (App, Receiver<Event>) {
        let (mut app, events) = app(scratch);
        app.model.confine(scratch.0.clone());
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        app.model.screen = Screen::Wallet(WalletPage {
            page,
            ..WalletPage::default()
        });
        (app, events)
    }

    /// How many commands the worker was sent, and has not answered, for
    /// which `is` holds.
    fn waiting_for(app: &App, is: fn(&Purpose) -> bool) -> usize {
        app.worker
            .as_ref()
            .map_or(0, |w| w.waiting.iter().filter(|(_, p)| is(p)).count())
    }

    /// Shows `text` as the page's refusal, as a command it sent would.
    fn set_error(app: &mut App, text: &str) {
        match &mut app.model.screen {
            Screen::Wallet(p) => p.error = Some(text.to_owned()),
            other => panic!("the wallet is not shown: {other:?}"),
        }
    }

    #[test]
    fn a_phone_opens_its_send_flow_on_page_one_and_a_desktop_on_its_form() {
        let mut phone = on_phone(Page::Dashboard);
        wallet(&mut phone, WalletMsg::Open(To::Send(None)));
        assert!(
            matches!(
                &wallet_page(&phone).page,
                Page::Send(SendPage {
                    stage: SendStage::To(refused),
                    form,
                    ..
                }) if refused.is_empty() && *form == SpendForm::default()
            ),
            "{:?}",
            wallet_page(&phone).page
        );

        let mut desktop = on_wallet(Page::Dashboard);
        wallet(&mut desktop, WalletMsg::Open(To::Send(None)));
        assert!(matches!(
            &wallet_page(&desktop).page,
            Page::Send(SendPage {
                stage: SendStage::Compose,
                ..
            })
        ));
    }

    #[test]
    fn continue_with_destinations_that_are_right_shows_the_amounts_at_their_top() {
        let mut app = on_phone(send_page(
            SendStage::To(Default::default()),
            spend_to(&a_destination(), ""),
        ));
        let task = app.update(WalletMsg::Continue.into());
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Amount,
                ..
            })
        ));
        assert_eq!(operations(task).len(), 1, "the amounts start at their top");
    }

    #[test]
    fn continue_with_a_destination_that_is_not_one_shows_why_under_it_and_stays() {
        let bad = "not-a-destination";
        let form = SpendForm {
            rows: vec![to_row(&a_destination()), to_row(bad)],
            ..SpendForm::default()
        };
        let mut app = on_phone(send_page(SendStage::To(Default::default()), form));
        let task = app.update(WalletMsg::Continue.into());
        let why = tawara_wallet_core::spend::parse_destination(bad, 2)
            .expect_err("it is not a destination")
            .to_string();
        assert_eq!(
            refusals_of(&app),
            refusals_at(&[((1, RefusedField::Address), why.as_str())]),
            "under its field"
        );
        assert!(wallet_page(&app).error.is_none());
        assert_eq!(
            operations(task).len(),
            1,
            "the first refusal is brought into view"
        );

        // Typed again, it is checked again, and Continue then shows the amounts.
        wallet(&mut app, WalletMsg::DestinationTo(1, a_destination()));
        assert!(refusals_of(&app).is_empty());
        let _ = app.update(WalletMsg::Continue.into());
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Amount,
                ..
            })
        ));
    }

    #[test]
    fn continue_with_no_destination_says_so_beside_it() {
        let mut app = on_phone(send_page(
            SendStage::To(Default::default()),
            SpendForm::default(),
        ));
        let task = app.update(WalletMsg::Continue.into());
        assert_eq!(
            wallet_page(&app).error.as_deref(),
            Some("Give at least one destination.")
        );
        assert!(refusals_of(&app).is_empty());
        assert!(operations(task).is_empty());
    }

    #[test]
    fn continue_pressed_at_the_foot_brings_the_first_refusal_into_view() {
        // Three destinations, the first refused, and the page scrolled to its
        // foot, where Continue is pressed. The refusal under the first field is
        // brought into view, not left above it (docs/DECISIONS.md D40 item 4).
        // A reference the node refuses is brought into view the same way.
        let not = "not-a-destination";
        let why_not = tawara_wallet_core::spend::parse_destination(not, 1)
            .expect_err("it is not a destination")
            .to_string();
        let why_reference = tawara_wallet_core::spend::parse_reference("abc", 1)
            .expect_err("the node refuses it")
            .to_string();
        let reference = DestinationRow {
            to: a_destination(),
            amount: String::new(),
            reference: "abc".to_owned(),
        };
        let cases = [
            (
                vec![
                    to_row(not),
                    to_row(&a_destination()),
                    to_row(&a_destination()),
                ],
                why_not,
            ),
            (
                vec![
                    reference,
                    to_row(&a_destination()),
                    to_row(&a_destination()),
                ],
                why_reference,
            ),
        ];
        for (rows, why) in cases {
            let form = SpendForm {
                rows,
                ..SpendForm::default()
            };
            let mut app = on_phone(send_page(SendStage::To(Default::default()), form));
            app.model.width = 390.0;
            let mut at_foot = Driver::new(crate::screens::view(&app.model), (390, 844));
            at_foot.operate(Box::new(snap_to(
                crate::screens::page_id(),
                RelativeOffset::END.into(),
            )));
            let cache = at_foot.into_cache();

            let task = app.update(WalletMsg::Continue.into());
            let mut drawn = Driver::with_cache(crate::screens::view(&app.model), (390, 844), cache);
            assert!(
                !shown_on_page(&mut drawn, &why),
                "the refusal is above the foot's view: {why}"
            );
            for operation in operations(task) {
                drawn.operate(operation);
            }
            assert!(
                shown_on_page(&mut drawn, &why),
                "the refusal is in view once Continue is pressed: {why}"
            );
        }
    }

    #[test]
    fn change_opens_the_account_list_on_page_one_only_and_a_choice_or_continue_shuts_it() {
        // "Change" opens the list under the From line on a phone's page 1, and
        // only while the wallet is idle. Choosing an account, or Continue, shuts
        // it (docs/DECISIONS.md D40 item 1).
        let mut app = on_phone(send_page(
            SendStage::To(Default::default()),
            spend_to(&a_destination(), ""),
        ));
        let second = tawara_wallet_core::sample::wallet_view().accounts[1].id;
        app.model
            .wallet
            .as_mut()
            .expect("the sample wallet")
            .accounts[1]
            .spendable = true;
        let from = |app: &App| match &wallet_page(app).page {
            Page::Send(s) => s.from,
            other => panic!("expected the send page, got {other:?}"),
        };
        let choosing = |app: &App| wallet_page(app).choosing;

        wallet(&mut app, WalletMsg::ChooseAccount(true));
        assert!(choosing(&app));
        wallet(&mut app, WalletMsg::From(second));
        assert_eq!(from(&app), Some(second));
        assert!(!choosing(&app), "a choice shuts the list");

        wallet(&mut app, WalletMsg::ChooseAccount(true));
        wallet(&mut app, WalletMsg::Continue);
        assert!(!choosing(&app), "page 2 has no list");
        assert_eq!(from(&app), Some(second));

        // Page 2 has no list to open, and a busy wallet opens none.
        wallet(&mut app, WalletMsg::ChooseAccount(true));
        assert!(!choosing(&app));
        wallet(&mut app, WalletMsg::Back);
        app.model.busy = Some(Busy {
            id: RequestId::example(),
            activity: None,
            progress: None,
        });
        wallet(&mut app, WalletMsg::ChooseAccount(true));
        assert!(!choosing(&app), "not while the worker answers");
        assert_eq!(from(&app), Some(second));
    }

    #[test]
    fn an_amount_with_no_address_asks_for_the_address_under_it() {
        let form = SpendForm {
            rows: vec![DestinationRow {
                to: " ".to_owned(),
                amount: "5".to_owned(),
                reference: String::new(),
            }],
            ..SpendForm::default()
        };
        let mut app = on_phone(send_page(SendStage::To(Default::default()), form));
        let _ = app.update(WalletMsg::Continue.into());
        assert_eq!(
            refusals_of(&app),
            refusals_at(&[(
                (0, RefusedField::Address),
                "Give this destination's address."
            )])
        );
    }

    #[test]
    fn a_reference_the_node_refuses_is_shown_under_its_own_field() {
        let form = SpendForm {
            rows: vec![DestinationRow {
                to: a_destination(),
                amount: String::new(),
                reference: "abc".to_owned(),
            }],
            ..SpendForm::default()
        };
        let mut app = on_phone(send_page(SendStage::To(Default::default()), form));
        let task = app.update(WalletMsg::Continue.into());
        assert_eq!(
            refusals_of(&app).keys().copied().collect::<Vec<_>>(),
            [(0, RefusedField::Reference)],
            "the address is right, so only the reference is refused"
        );
        assert_eq!(
            operations(task).len(),
            1,
            "the refusal is brought into view"
        );

        // Typed again, it is checked again, and Continue then shows the amounts.
        wallet(&mut app, WalletMsg::Reference(0, "AB-00-EF".to_owned()));
        assert!(refusals_of(&app).is_empty());
        let _ = app.update(WalletMsg::Continue.into());
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Amount,
                ..
            })
        ));
    }

    #[test]
    fn a_changed_field_forgets_its_refusal_and_a_removed_row_forgets_them_all() {
        let mut app = on_phone(send_page(
            SendStage::To(refusals_at(&[
                ((0, RefusedField::Address), "first"),
                ((0, RefusedField::Reference), "second"),
                ((1, RefusedField::Address), "third"),
            ])),
            SpendForm {
                rows: vec![to_row("x"), to_row("y")],
                ..SpendForm::default()
            },
        ));
        wallet(&mut app, WalletMsg::DestinationTo(0, a_destination()));
        assert_eq!(
            refusals_of(&app),
            refusals_at(&[
                ((0, RefusedField::Reference), "second"),
                ((1, RefusedField::Address), "third"),
            ]),
            "the address is checked again, and only it"
        );
        wallet(&mut app, WalletMsg::Reference(0, "AB-00-EF".to_owned()));
        assert_eq!(
            refusals_of(&app),
            refusals_at(&[((1, RefusedField::Address), "third")]),
            "and so is the reference, when it is typed again"
        );
        wallet(&mut app, WalletMsg::AddDestination);
        assert_eq!(
            refusals_of(&app),
            refusals_at(&[((1, RefusedField::Address), "third")]),
            "adding moves none"
        );
        wallet(&mut app, WalletMsg::RemoveDestination(2));
        assert!(
            refusals_of(&app).is_empty(),
            "removing one moves those after it"
        );
    }

    #[test]
    fn back_from_the_amounts_keeps_what_was_typed_and_drops_the_review_spend_refusal() {
        let form = spend_to(&a_destination(), "1.5");
        let mut app = on_phone(send_page(SendStage::Amount, form.clone()));
        set_error(&mut app, "Destination 1: the amount is not a number.");
        let task = app.update(WalletMsg::Back.into());
        assert!(
            matches!(
                &wallet_page(&app).page,
                Page::Send(SendPage {
                    stage: SendStage::To(refused),
                    form: kept,
                    ..
                }) if refused.is_empty() && *kept == form
            ),
            "{:?}",
            wallet_page(&app).page
        );
        assert!(wallet_page(&app).error.is_none());
        assert_eq!(
            operations(task).len(),
            1,
            "the destinations start at their top"
        );
    }

    #[test]
    fn back_from_the_review_gives_the_plan_up_and_keeps_what_was_typed() {
        let scratch = Scratch::new("back-review");
        let plan = tawara_wallet_core::sample::plan_view();
        let form = typed_as_planned(&plan);
        let (mut app, _events) = on_phone_with_worker(
            &scratch,
            send_page(SendStage::Review(Box::new(plan)), form.clone()),
        );
        let task = app.update(WalletMsg::Back.into());
        assert!(
            matches!(
                &wallet_page(&app).page,
                Page::Send(SendPage {
                    stage: SendStage::Amount,
                    form: kept,
                    ..
                }) if *kept == form
            ),
            "{:?}",
            wallet_page(&app).page
        );
        assert_eq!(operations(task).len(), 1, "the amounts start at their top");
        assert_eq!(
            waiting_for(&app, |p| matches!(p, Purpose::DiscardPlan)),
            1,
            "the worker is asked to forget the plan"
        );
    }

    #[test]
    fn back_from_the_signing_page_keeps_the_plan_for_the_review() {
        let scratch = Scratch::new("back-sign");
        let plan = tawara_wallet_core::sample::plan_view();
        let id = plan.plan;
        let (mut app, _events) = on_phone_with_worker(
            &scratch,
            send_page(
                SendStage::Sign(Box::new(plan)),
                spend_to(&a_destination(), "1"),
            ),
        );
        let _ = app.update(WalletMsg::Back.into());
        assert!(
            matches!(
                &wallet_page(&app).page,
                Page::Send(SendPage {
                    stage: SendStage::Review(p),
                    ..
                }) if p.plan == id
            ),
            "{:?}",
            wallet_page(&app).page
        );
        assert_eq!(
            waiting_for(&app, |p| matches!(p, Purpose::DiscardPlan)),
            0,
            "the plan is kept"
        );
    }

    #[test]
    fn back_changes_nothing_on_the_destinations_page_or_while_the_worker_answers() {
        let mut app = on_phone(send_page(
            SendStage::To(Default::default()),
            spend_to(&a_destination(), ""),
        ));
        let _ = app.update(WalletMsg::Back.into());
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::To(_),
                ..
            })
        ));

        let mut app = on_phone(send_page(
            SendStage::Amount,
            spend_to(&a_destination(), "1"),
        ));
        app.model.busy = Some(Busy {
            id: RequestId::example(),
            activity: None,
            progress: None,
        });
        let _ = app.update(WalletMsg::Back.into());
        let _ = app.update(Message::SystemBack);
        assert!(
            matches!(
                &wallet_page(&app).page,
                Page::Send(SendPage {
                    stage: SendStage::Amount,
                    ..
                })
            ),
            "{:?}",
            wallet_page(&app).page
        );
    }

    #[test]
    fn leaving_the_signing_page_any_other_way_gives_the_plan_up() {
        let scratch = Scratch::new("leave-sign");
        let plan = tawara_wallet_core::sample::plan_view();
        let form = typed_as_planned(&plan);
        let (mut app, _events) =
            on_phone_with_worker(&scratch, send_page(SendStage::Sign(Box::new(plan)), form));
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
        assert_eq!(
            waiting_for(&app, |p| matches!(p, Purpose::DiscardPlan)),
            1,
            "the worker is asked to forget the plan"
        );
    }

    #[test]
    fn the_send_tab_tapped_mid_flow_keeps_the_form_and_the_page_on_a_phone() {
        let scratch = Scratch::new("send-tab-flow");
        let plan = tawara_wallet_core::sample::plan_view();
        let form = typed_as_planned(&plan);
        let (mut app, _events) = on_phone_with_worker(
            &scratch,
            send_page(SendStage::Review(Box::new(plan)), form.clone()),
        );
        wallet(&mut app, WalletMsg::Open(To::Send(None)));
        assert!(
            matches!(
                &wallet_page(&app).page,
                Page::Send(SendPage {
                    stage: SendStage::Review(_),
                    form: kept,
                    ..
                }) if *kept == form
            ),
            "{:?}",
            wallet_page(&app).page
        );
        assert_eq!(
            waiting_for(&app, |p| matches!(p, Purpose::DiscardPlan)),
            0,
            "the plan is kept"
        );

        let mut app = on_phone(send_page(SendStage::Amount, form.clone()));
        wallet(&mut app, WalletMsg::Open(To::Send(None)));
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Amount,
                form: kept,
                ..
            }) if *kept == form
        ));

        let mut app = on_phone(send_page(
            SendStage::To(refusals_at(&[((0, RefusedField::Address), "why")])),
            form.clone(),
        ));
        wallet(&mut app, WalletMsg::Open(To::Send(None)));
        assert_eq!(
            refusals_of(&app),
            refusals_at(&[((0, RefusedField::Address), "why")])
        );
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage { form: kept, .. }) if *kept == form
        ));
    }

    #[test]
    fn the_send_tab_after_a_sent_spend_starts_an_empty_form_on_a_phone() {
        let sent = SentPage {
            sent: tawara_wallet_core::sample::sent_view(),
            order: 0,
            resigned: false,
            saved: None,
            dismissing: false,
        };
        let mut app = on_phone(send_page(
            SendStage::Sent(Box::new(sent)),
            spend_to(&a_destination(), "1"),
        ));
        wallet(&mut app, WalletMsg::Open(To::Send(None)));
        assert!(
            matches!(
                &wallet_page(&app).page,
                Page::Send(SendPage {
                    stage: SendStage::To(refused),
                    form,
                    ..
                }) if refused.is_empty() && *form == SpendForm::default()
            ),
            "{:?}",
            wallet_page(&app).page
        );
    }

    #[test]
    fn system_back_steps_back_pages_two_to_four_and_page_one_is_the_shells() {
        let scratch = Scratch::new("system-back");
        let plan = tawara_wallet_core::sample::plan_view();
        let form = typed_as_planned(&plan);
        let (mut app, _events) = on_phone_with_worker(
            &scratch,
            send_page(SendStage::Sign(Box::new(plan)), form.clone()),
        );
        // Page 4 to page 3: the plan is kept.
        let _ = app.update(Message::SystemBack);
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Review(_),
                ..
            })
        ));
        assert_eq!(waiting_for(&app, |p| matches!(p, Purpose::DiscardPlan)), 0);

        // Page 3 to page 2: the plan is given up.
        let _ = app.update(Message::SystemBack);
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Amount,
                form: kept,
                ..
            }) if *kept == form
        ));
        assert_eq!(waiting_for(&app, |p| matches!(p, Purpose::DiscardPlan)), 1);

        // Page 2 to page 1, with what was typed.
        let _ = app.update(Message::SystemBack);
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::To(refused),
                form: kept,
                ..
            }) if refused.is_empty() && *kept == form
        ));

        // Page 1 is the shell's Back, which the flow does not take, so the
        // page is left as it is.
        assert!(!app.model.send_steps_back(), "page one is the shell's");
        let _ = app.update(Message::SystemBack);
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::To(_),
                ..
            })
        ));
    }

    #[test]
    fn on_the_desktop_system_back_is_the_shells_on_every_page() {
        let plan = tawara_wallet_core::sample::plan_view();
        let form = typed_as_planned(&plan);
        let mut app = on_wallet(send_page(SendStage::Review(Box::new(plan)), form));
        let _ = app.update(Message::SystemBack);
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Review(_),
                ..
            })
        ));
    }

    #[test]
    fn review_spend_on_the_amounts_page_says_why_beside_it_and_stays_there() {
        let mut app = on_phone(send_page(
            SendStage::Amount,
            spend_to(&a_destination(), "a lot"),
        ));
        let _ = app.update(WalletMsg::Review.into());
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Amount,
                ..
            })
        ));
        assert!(
            wallet_page(&app).error.is_some(),
            "why is beside Review spend"
        );
    }

    #[test]
    fn review_spend_on_the_amounts_page_asks_the_worker_to_plan_and_the_plan_is_the_review() {
        let scratch = Scratch::new("review-spend");
        let plan = tawara_wallet_core::sample::plan_view();
        let form = typed_as_planned(&plan);
        let (mut app, _events) = on_phone_with_worker(&scratch, send_page(SendStage::Amount, form));
        wallet(&mut app, WalletMsg::Review);
        assert_eq!(
            waiting_for(&app, |p| matches!(p, Purpose::PlanSend)),
            1,
            "the worker is asked to plan"
        );
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Amount,
                ..
            })
        ));
        let task = app.on_reply(Purpose::PlanSend, Reply::Planned(plan));
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Review(_),
                ..
            })
        ));
        assert_eq!(operations(task).len(), 1, "the review starts at its top");
    }

    #[test]
    fn a_plan_is_the_review_from_the_amounts_and_is_forgotten_on_the_destinations() {
        let plan = tawara_wallet_core::sample::plan_view();
        let form = typed_as_planned(&plan);
        let scratch = Scratch::new("plan-amounts");
        let (mut app, _events) =
            on_phone_with_worker(&scratch, send_page(SendStage::Amount, form.clone()));
        let task = app.on_reply(Purpose::PlanSend, Reply::Planned(plan.clone()));
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Review(_),
                ..
            })
        ));
        assert_eq!(operations(task).len(), 1);

        // A plan that comes back once the destinations page is shown is
        // forgotten as it comes.
        let scratch = Scratch::new("plan-destinations");
        let (mut app, _events) =
            on_phone_with_worker(&scratch, send_page(SendStage::To(Default::default()), form));
        let _ = app.on_reply(Purpose::PlanSend, Reply::Planned(plan));
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::To(_),
                ..
            })
        ));
        assert_eq!(waiting_for(&app, |p| matches!(p, Purpose::DiscardPlan)), 1);
    }

    #[test]
    fn continue_to_signing_shows_the_plan_on_page_four_on_a_phone_only() {
        let plan = tawara_wallet_core::sample::plan_view();
        let form = typed_as_planned(&plan);
        let id = plan.plan;
        let mut app = on_phone(send_page(
            SendStage::Review(Box::new(plan.clone())),
            form.clone(),
        ));
        let task = app.update(WalletMsg::ToSigning.into());
        assert!(
            matches!(
                &wallet_page(&app).page,
                Page::Send(SendPage {
                    stage: SendStage::Sign(p),
                    ..
                }) if p.plan == id
            ),
            "{:?}",
            wallet_page(&app).page
        );
        assert_eq!(operations(task).len(), 1, "page four starts at its top");

        let mut desktop = on_wallet(send_page(SendStage::Review(Box::new(plan)), form));
        let _ = desktop.update(WalletMsg::ToSigning.into());
        assert!(matches!(
            &wallet_page(&desktop).page,
            Page::Send(SendPage {
                stage: SendStage::Review(_),
                ..
            })
        ));
    }

    #[test]
    fn sign_is_pressed_on_page_four_on_a_phone_and_not_on_the_review_above_it() {
        let scratch = Scratch::new("sign-page-four");
        let plan = tawara_wallet_core::sample::plan_view();
        let form = typed_as_planned(&plan);
        let (mut app, _events) =
            on_phone_with_worker(&scratch, send_page(SendStage::Review(Box::new(plan)), form));
        wallet(&mut app, WalletMsg::Sign);
        assert_eq!(
            waiting_for(&app, |p| matches!(p, Purpose::ConfirmSend)),
            0,
            "the review does not sign"
        );
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Review(_),
                ..
            })
        ));

        wallet(&mut app, WalletMsg::ToSigning);
        wallet(&mut app, WalletMsg::Sign);
        assert_eq!(
            waiting_for(&app, |p| matches!(p, Purpose::ConfirmSend)),
            1,
            "page four signs"
        );
        assert!(app.model.busy.is_some(), "and waits on the worker");
    }

    #[test]
    fn a_refused_signing_on_a_phone_goes_back_to_the_amounts_with_why() {
        let scratch = Scratch::new("refused-phone");
        let plan = tawara_wallet_core::sample::plan_view();
        let form = typed_as_planned(&plan);
        let (mut app, _events) =
            on_phone_with_worker(&scratch, send_page(SendStage::Sign(Box::new(plan)), form));
        let task = app.on_reply(
            Purpose::ConfirmSend,
            Reply::Refused(Refusal {
                kind: RefusalKind::Library,
                text: "the balance moved".to_owned(),
            }),
        );
        assert!(
            matches!(
                &wallet_page(&app).page,
                Page::Send(SendPage {
                    stage: SendStage::Amount,
                    ..
                })
            ),
            "{:?}",
            wallet_page(&app).page
        );
        assert_eq!(
            wallet_page(&app).error.as_deref(),
            Some("the balance moved"),
            "beside Review spend"
        );
        // The page opens at its end, where the refusal is drawn beside "Review
        // spend": at its top the refusal is a screen or more below the view.
        app.model.width = 390.0;
        {
            let mut drawn = Driver::new(crate::screens::view(&app.model), (390, 844));
            assert!(
                !shown_on_page(&mut drawn, "the balance moved"),
                "at its top, the refusal is out of view"
            );
            for operation in operations(task) {
                drawn.operate(operation);
            }
            assert!(
                shown_on_page(&mut drawn, "the balance moved"),
                "at its end, the refusal is in view"
            );
        }

        // The worker dropped the plan, so leaving the amounts gives up nothing.
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        assert_eq!(
            waiting_for(&app, |p| matches!(p, Purpose::DiscardPlan)),
            0,
            "no plan is left to forget"
        );
    }

    #[test]
    fn review_spend_with_an_amount_and_no_address_goes_back_to_that_destination() {
        // Page 2 takes an amount for a destination that page 1 passed, as it had
        // nothing typed in it then. "Review spend" asks for its address under
        // its own field on page 1, and asks the worker for no plan
        // (docs/DECISIONS.md D40 item 7 (b)).
        let scratch = Scratch::new("review-no-address");
        let form = SpendForm {
            rows: vec![
                DestinationRow {
                    to: a_destination(),
                    amount: "1".to_owned(),
                    reference: String::new(),
                },
                DestinationRow {
                    to: String::new(),
                    amount: "5".to_owned(),
                    reference: String::new(),
                },
            ],
            ..SpendForm::default()
        };
        let (mut app, _events) = on_phone_with_worker(&scratch, send_page(SendStage::Amount, form));
        wallet(&mut app, WalletMsg::Review);
        assert_eq!(
            waiting_for(&app, |p| matches!(p, Purpose::PlanSend)),
            0,
            "no plan is asked for"
        );
        assert_eq!(
            refusals_of(&app),
            refusals_at(&[(
                (1, RefusedField::Address),
                "Give this destination's address."
            )])
        );
        assert!(wallet_page(&app).error.is_none());

        // Given its address, the spend is planned from page 2 as before.
        wallet(&mut app, WalletMsg::DestinationTo(1, a_destination()));
        wallet(&mut app, WalletMsg::Continue);
        wallet(&mut app, WalletMsg::Review);
        assert_eq!(
            waiting_for(&app, |p| matches!(p, Purpose::PlanSend)),
            1,
            "the worker is asked to plan"
        );
    }

    #[test]
    fn a_destination_is_pasted_on_page_one_and_a_read_that_comes_back_on_page_two_is_dropped() {
        let app = on_phone(send_page(
            SendStage::To(Default::default()),
            spend_to("", ""),
        ));
        let (form, rows) = app
            .model
            .form_of()
            .expect("the destinations page takes a paste");
        assert!(takes(
            &app.model,
            PasteInto::Destination { form, row: 0, rows }
        ));

        let app = on_phone(send_page(
            SendStage::Amount,
            spend_to(&a_destination(), "1"),
        ));
        assert!(
            app.model.form_of().is_none(),
            "the amounts page has no destination to paste into"
        );
        assert!(!takes(
            &app.model,
            PasteInto::Destination {
                form: FormOf::Send(Some(sample_from())),
                row: 0,
                rows: 1,
            }
        ));
    }

    #[test]
    fn the_amounts_page_takes_each_amount_the_fee_and_the_block_to_live() {
        let mut app = on_phone(send_page(SendStage::Amount, spend_to(&a_destination(), "")));
        wallet(&mut app, WalletMsg::Amount(0, "2.5".to_owned()));
        wallet(&mut app, WalletMsg::Fee("1000".to_owned()));
        wallet(&mut app, WalletMsg::BlockToLive("42".to_owned()));
        assert!(
            matches!(
                &wallet_page(&app).page,
                Page::Send(SendPage { form, .. })
                    if form.rows[0].amount == "2.5"
                        && form.fee == "1000"
                        && form.blk_to_live == "42"
            ),
            "{:?}",
            wallet_page(&app).page
        );
    }

    #[test]
    fn on_the_desktop_the_phone_steps_change_nothing_and_the_send_tab_empties_the_form() {
        let plan = tawara_wallet_core::sample::plan_view();
        let form = typed_as_planned(&plan);
        let mut app = on_wallet(send_page(SendStage::Compose, form.clone()));
        for msg in [WalletMsg::Continue, WalletMsg::Back, WalletMsg::ToSigning] {
            let _ = app.update(msg.into());
        }
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Compose,
                form: kept,
                ..
            }) if *kept == form
        ));

        wallet(&mut app, WalletMsg::Open(To::Send(None)));
        assert!(
            matches!(
                &wallet_page(&app).page,
                Page::Send(SendPage {
                    stage: SendStage::Compose,
                    form: kept,
                    ..
                }) if *kept == SpendForm::default()
            ),
            "the Send tab builds an empty form on a desktop: {:?}",
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

    /// The window operations `task` asks the runtime to run, in order. A task
    /// that `operate` built keeps its stream open for the output of the
    /// operation, which a test never runs, so the stream is read only as far as
    /// it is ready: a task that ends on its own gives all of its operations.
    fn operations(task: Task<Message>) -> Vec<Box<dyn Operation>> {
        let Some(mut stream) = iced_runtime::task::into_stream(task) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        while let Some(Some(action)) = iced::futures::future::FutureExt::now_or_never(stream.next())
        {
            if let Action::Widget(operation) = action {
                found.push(operation);
            }
        }
        found
    }

    /// Whether the text `what` is drawn within the window of the page as
    /// `drawn` scrolls it: its top and its bottom both between the window's.
    fn shown_on_page(drawn: &mut Driver<'_, Message>, what: &str) -> bool {
        let (window, scroll) = drawn
            .scroll_of(&crate::screens::page_id())
            .expect("the page is drawn");
        let text = drawn
            .text_bounds(what)
            .unwrap_or_else(|| panic!("{what:?} is not drawn"));
        let top = text.y - scroll.y;
        top >= window.y && top + text.height <= window.y + window.height
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
            reverted_settle: false,
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
    fn a_reply_reads_only_its_own_account_so_a_watch_ends_on_that_accounts_answer() {
        // The owner's review (docs/DECISIONS.md D38 item 2): B is on its final
        // follow-up poll, and A's answer comes first. It carries B's cached row,
        // which is no read of B, so B stays watched; B's own answer then decides
        // it: a silent one continues the watch, and a third doubtful one in a
        // row ends it.
        use tawara_wallet_core::view::{AccountState, DivergenceKind, ReservationState};
        let mut app = on_wallet(Page::Dashboard);
        let view = app.model.wallet.clone().expect("the sample store");
        let (a, b) = (view.accounts[0].id, view.accounts[1].id);
        let live = AccountState::SpendOutstanding {
            balance: 3_180_000_000_000,
            spent_index: 6,
            reservation: ReservationState::Live,
        };
        let diverged = |kind| AccountState::Diverged {
            kind,
            report: "the library's report".to_owned(),
            advance_to: None,
            reverted_settle: false,
        };
        let silent = diverged(DivergenceKind::Unreachable);
        let not_found = diverged(DivergenceKind::NotFound);
        let answer = |app: &mut App, account: AccountId, state: AccountState| {
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
        // Both accounts are polled from their live reads.
        answer(&mut app, a, live.clone());
        answer(&mut app, b, live.clone());
        // B answers "account not found", and its two polls in doubt are sent:
        // the second is B's final follow-up poll, and it is still on its way.
        answer(&mut app, b, not_found.clone());
        app.model.watch.asked(b, &not_found);
        answer(&mut app, b, not_found.clone());
        app.model.watch.asked(b, &not_found);
        // A's answer comes first, and carries B's cached row: B is still watched.
        answer(&mut app, a, live.clone());
        assert!(
            app.model.watch.asks(b, &silent),
            "B's own answer has not come"
        );
        // B's own answer is silent: the watch continues, within the bound.
        answer(&mut app, b, silent.clone());
        assert!(
            app.model.watch.asks(b, &silent),
            "a silent answer continues it"
        );
        app.model.watch.asked(b, &silent);
        // B's third doubtful answer in a row ends its polls.
        answer(&mut app, b, not_found.clone());
        assert!(
            !app.model.watch.asks(b, &not_found),
            "the third doubtful answer ends it"
        );
        assert!(
            !app.model.watch.asks(b, &silent),
            "and a silent one does not restart it"
        );
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
    fn a_sign_the_library_refused_reads_its_account_again() {
        // The library records a reservation before it signs, so a sign it
        // refused may have left one with no bytes. The account is read again,
        // quietly, as a poll is (D38 item 2). A refusal of the worker's own, for
        // a plan that is gone, reaches no reservation and reads nothing.
        use tawara_wallet_core::{Refusal, RefusalKind};
        let scratch = Scratch::new("refused-sign");
        let (mut app, _events) = app(&scratch);
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        let reviewing = |app: &mut App| {
            let plan = tawara_wallet_core::sample::plan_view();
            app.model.screen = Screen::Wallet(WalletPage {
                page: Page::Send(SendPage {
                    from: Some(plan.from),
                    form: spend_typed(),
                    stage: SendStage::Review(Box::new(plan)),
                }),
                ..WalletPage::default()
            });
        };
        reviewing(&mut app);
        let _ = app.on_reply(
            Purpose::ConfirmSend,
            Reply::Refused(Refusal {
                kind: RefusalKind::NoSuchPlan,
                text: "plan it again".to_owned(),
            }),
        );
        assert!(!app.waiting_on(|p| matches!(p, Purpose::Poll)));
        reviewing(&mut app);
        let _ = app.on_reply(
            Purpose::ConfirmSend,
            Reply::Refused(Refusal {
                kind: RefusalKind::Library,
                text: "the library refused".to_owned(),
            }),
        );
        assert!(app.waiting_on(|p| matches!(p, Purpose::Poll)), "read again");
        assert!(matches!(
            &wallet_page(&app).page,
            Page::Send(SendPage {
                stage: SendStage::Compose,
                ..
            })
        ));
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
                    reverted_settle: false,
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
            reverted_settle: false,
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
                reverted_settle: false,
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
            reverted_settle: false,
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
    fn a_recovery_review_that_finds_a_live_reservation_restarts_an_ended_watch() {
        // The owner's review (P2; docs/DECISIONS.md D38 item 2): a recovery review
        // reconciles every account and updates its rows, so its reads reset the
        // watch. An account whose watch ended, and which the review finds with a
        // live reservation, is polled afresh: a transient failure keeps the polls
        // going, and the poll that finds the spend landed settles it.
        use tawara_wallet_core::view::{
            AccountState, DivergenceKind, ReservationState, SettledAccount,
        };
        let scratch = Scratch::new("review-restarts");
        let (mut app, _events) = app(&scratch);
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        let account = live_account();
        let in_sync = AccountState::InSync {
            balance: 3_180_000_000_000,
        };
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
        // A read showed the account in sync, so its watch ended.
        answer(&mut app, in_sync.clone(), None);
        assert!(!polls(&mut app), "an account in sync is not polled");

        // A recovery review finds its reservation live, and the polls start again.
        let mut reports = tawara_wallet_core::sample::review();
        let found = reports
            .iter_mut()
            .find(|r| r.account == account)
            .expect("the sample's spend");
        found.state = AccountState::SpendOutstanding {
            balance: 3_180_000_000_000,
            spent_index: 6,
            reservation: ReservationState::Live,
        };
        let _ = app.on_reply(Purpose::Review, Reply::Reviewed(reports));
        assert!(polls(&mut app), "the review's live reservation is polled");

        // The node does not answer, and the polls go on.
        let silent = AccountState::Diverged {
            kind: DivergenceKind::Unreachable,
            report: "no answer".to_owned(),
            advance_to: None,
            reverted_settle: false,
        };
        answer(&mut app, silent, None);
        assert!(polls(&mut app), "a transient failure keeps the polls going");

        // The next poll finds the spend landed, and settles it.
        answer(
            &mut app,
            in_sync,
            Some(SettledAccount {
                account,
                text: "settled: the library's page".to_owned(),
            }),
        );
        assert!(!polls(&mut app), "a settled account is not polled");
        assert!(
            app.model.settled_waiting.contains_key(&account),
            "the settlement's banner waits for the account's page"
        );
    }

    #[test]
    fn a_recovery_review_starts_the_count_of_a_spend_in_doubt_afresh() {
        // The bound of docs/DECISIONS.md D38 item 2 counts the polls of a spend in
        // doubt, and the third read in doubt in a row ends them. A review that finds
        // the reservation live again starts that count afresh, as a sign does, so
        // the account is polled through the bound once more.
        use tawara_wallet_core::view::{AccountState, DivergenceKind, ReservationState};
        let scratch = Scratch::new("review-bound");
        let (mut app, _events) = app(&scratch);
        app.model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        let account = live_account();
        let live = AccountState::SpendOutstanding {
            balance: 3_180_000_000_000,
            spent_index: 6,
            reservation: ReservationState::Live,
        };
        let not_found = AccountState::Diverged {
            kind: DivergenceKind::NotFound,
            report: "account not found".to_owned(),
            advance_to: None,
            reverted_settle: false,
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
        // Whether the next tick sends a poll, as the test above has it.
        let polls = |app: &mut App| {
            app.model.polled_at = None;
            answered(app);
            let _ = app.update(Message::Tick(now_ms()));
            let sent = app.waiting_on(|p| matches!(p, Purpose::Poll));
            answered(app);
            sent
        };
        // A recovery review that finds the reservation live.
        let review = |app: &mut App| {
            let mut reports = tawara_wallet_core::sample::review();
            let found = reports
                .iter_mut()
                .find(|r| r.account == account)
                .expect("the sample's spend");
            found.state = live.clone();
            let _ = app.on_reply(Purpose::Review, Reply::Reviewed(reports));
        };
        // Each round: two polls in doubt, and the third read in doubt ends them.
        // The second round starts after the first has ended them.
        for round in 0..2 {
            review(&mut app);
            assert!(
                polls(&mut app),
                "round {round}: the review's reservation is polled"
            );
            for n in 0..2 {
                answer(&mut app, not_found.clone());
                assert!(polls(&mut app), "round {round}: not found, poll {n}");
            }
            answer(&mut app, not_found.clone());
            assert!(
                !polls(&mut app),
                "round {round}: the third read in doubt ends the polls"
            );
        }
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

    /// The application over `prefs`, saving to `path`, with no worker: the
    /// replies it is handed are all it does.
    fn alone_saving(prefs: Preferences, path: PathBuf) -> App {
        App {
            model: Model::new(prefs, None),
            worker: None,
            prefs_path: Some(path),
            touches: std::cell::Cell::default(),
        }
    }

    #[test]
    fn a_fresh_start_chooses_the_default_node_and_asks_it_nothing() {
        // The worker is given the default. Setting it reads nothing: building a
        // node's transport opens no socket, and the start-up answer asks for no
        // tip, network, index or queue (docs/DECISIONS.md D43).
        let (mut app, events) = App::start(Model::new(Preferences::default(), None), None);
        let events = events.expect("the worker starts");
        assert_eq!(
            app.model.node.url.as_deref(),
            Some(preferences::DEFAULT_NODE)
        );
        pump(&mut app, &events, |a| {
            !waiting(a).iter().any(|p| p == "SetNode")
        });
        assert_eq!(
            app.model.node.url.as_deref(),
            Some(preferences::DEFAULT_NODE)
        );
        assert!(
            waiting(&app).is_empty(),
            "start-up asks the node nothing: {:?}",
            waiting(&app)
        );
        assert!(app.model.node.tip.is_none() && app.model.node.network.is_none());
    }

    #[test]
    fn nothing_is_asked_of_the_remembered_node_at_start_up() {
        // On loopback, so that a broken rule reaches nothing beyond this
        // machine. The rule does not depend on the address.
        let loopback = "http://127.0.0.1:9";
        let prefs = Preferences {
            node: Some(loopback.to_owned()),
            ..Preferences::default()
        };
        let (mut app, events) = App::start(Model::new(prefs, None), None);
        let events = events.expect("the worker starts");
        pump(&mut app, &events, |a| {
            !waiting(a).iter().any(|p| p == "SetNode")
        });
        assert_eq!(app.model.node.url.as_deref(), Some(loopback));
        assert!(
            waiting(&app).is_empty(),
            "start-up asks the node nothing: {:?}",
            waiting(&app)
        );
        // Check now asks it, and its answers come back.
        app.ask_tip();
        assert!(waiting(&app).iter().any(|p| p == "Network"));
        pump(&mut app, &events, |a| waiting(a).is_empty());
    }

    #[test]
    fn forgetting_the_node_is_kept_across_a_restart() {
        let scratch = Scratch::new("forget-node");
        let path = scratch.0.join("preferences");
        // The first start: the default is chosen, asked nothing, and nothing
        // is written.
        let (mut app, events) =
            App::start(Model::new(Preferences::default(), None), Some(path.clone()));
        let events = events.expect("the worker starts");
        pump(&mut app, &events, |a| {
            !waiting(a).iter().any(|p| p == "SetNode")
        });
        assert!(!path.exists(), "nothing is written at start-up");
        // Forgotten: the worker drops the node, and the choice is saved.
        let _ = app.update(Message::ForgetNode);
        pump(&mut app, &events, |a| waiting(a).is_empty());
        assert_eq!(app.model.node.url, None);
        assert_eq!(preferences::load(&path).node, None, "saved as none");
        // The next start reads it back: no node is set, so none is asked.
        let (next, _events) = App::start(
            Model::new(preferences::load(&path), None),
            Some(path.clone()),
        );
        assert_eq!(next.model.node.url, None);
        assert!(waiting(&next).is_empty(), "no node, so nothing is asked");
    }

    #[test]
    fn a_chosen_node_is_saved_and_a_start_up_node_is_not_written() {
        let scratch = Scratch::new("chosen-node");
        let path = scratch.0.join("preferences");
        let mut app = alone_saving(Preferences::default(), path.clone());
        // Start-up sets the default, which is what the preferences hold: nothing
        // changed, so nothing is written.
        let _ = app.on_reply(
            Purpose::SetNode { form: false },
            Reply::NodeSet {
                url: preferences::DEFAULT_NODE.to_owned(),
                view: None,
            },
        );
        assert!(!path.exists(), "a start-up node is not written");
        // The person chooses another: it is saved, and read back.
        let _ = app.on_reply(
            Purpose::SetNode { form: true },
            Reply::NodeSet {
                url: "https://other.example".to_owned(),
                view: None,
            },
        );
        assert_eq!(
            preferences::load(&path).node.as_deref(),
            Some("https://other.example")
        );
        // The person forgets it: saved as none, and not replaced by the default.
        let _ = app.on_reply(Purpose::ClearNode, Reply::NodeCleared { view: None });
        assert_eq!(app.model.prefs.node, None);
        assert_eq!(preferences::load(&path).node, None);
    }

    #[test]
    fn a_remembered_node_the_worker_refuses_is_forgotten_not_replaced() {
        let scratch = Scratch::new("refused-node");
        let path = scratch.0.join("preferences");
        let prefs = Preferences {
            node: Some("http://plain.example".to_owned()),
            ..Preferences::default()
        };
        let mut app = alone_saving(prefs, path.clone());
        let _ = app.on_reply(
            Purpose::SetNode { form: false },
            Reply::Refused(Refusal {
                kind: RefusalKind::NodeRefused,
                text: "plaintext".to_owned(),
            }),
        );
        assert_eq!(app.model.prefs.node, None, "forgotten, not the default");
        assert_eq!(preferences::load(&path).node, None);
    }

    #[test]
    fn forgetting_the_node_says_so_when_it_could_not_be_saved() {
        let scratch = Scratch::new("forget-unsaved");
        // A folder where the file should be: nothing can be written there.
        let path = scratch.0.join("preferences");
        std::fs::create_dir_all(&path).unwrap();
        let mut app = alone_saving(Preferences::default(), path);
        app.model.screen = Screen::Node(NodeForm {
            url: preferences::DEFAULT_NODE.to_owned(),
            error: None,
            back: Back::Start,
        });
        let _ = app.on_reply(Purpose::ClearNode, Reply::NodeCleared { view: None });
        assert_eq!(app.model.prefs.node, None, "forgotten for this run");
        assert!(
            matches!(
                &app.model.screen,
                Screen::Node(f) if f.url.is_empty()
                    && f.error.as_deref().is_some_and(|e| e.contains("could not be saved"))
            ),
            "{:?}",
            app.model.screen
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_kept_until_a_node_is_chosen_or_forgotten() {
        let scratch = Scratch::new("prefs-held");
        let path = scratch.0.join("preferences");
        let bytes = b"node = https://mine.example\n\xff\xfe\n".to_vec();
        std::fs::write(&path, &bytes).unwrap();
        // As `begin` starts: no node, and the file held.
        let loaded = preferences::load_checked(&path);
        assert!(loaded.unreadable);
        let mut app = alone_saving(loaded.prefs, path.clone());
        app.model.prefs_held = loaded.unreadable;
        assert_eq!(app.model.prefs.node, None, "not the default");
        // Ordinary changes are not written over it.
        app.remember(&scratch.0.join("keystore"));
        app.model.prefs.unit = preferences::AmountUnit::NanoMcm;
        app.save_prefs();
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "the file is kept");
        // Forgetting the node is the person's word: it is written, with the
        // changes made since, and the file is written as usual from then on.
        let _ = app.on_reply(Purpose::ClearNode, Reply::NodeCleared { view: None });
        let back = preferences::load_checked(&path);
        assert!(!back.unreadable);
        assert_eq!(back.prefs.node, None);
        assert_eq!(back.prefs.unit, preferences::AmountUnit::NanoMcm);
        assert!(back.prefs.store.is_some());
        app.model.prefs.unit = preferences::AmountUnit::Mcm;
        app.save_prefs();
        assert_eq!(preferences::load(&path).unit, preferences::AmountUnit::Mcm);
    }

    #[test]
    fn choosing_a_node_writes_over_a_file_that_cannot_be_read() {
        let scratch = Scratch::new("prefs-held-chosen");
        let path = scratch.0.join("preferences");
        std::fs::write(&path, b"\xff\xfe\n").unwrap();
        let loaded = preferences::load_checked(&path);
        let mut app = alone_saving(loaded.prefs, path.clone());
        app.model.prefs_held = loaded.unreadable;
        let _ = app.on_reply(
            Purpose::SetNode { form: true },
            Reply::NodeSet {
                url: "https://other.example".to_owned(),
                view: None,
            },
        );
        assert_eq!(
            preferences::load(&path).node.as_deref(),
            Some("https://other.example")
        );
    }

    /// The texts `model` draws in a window of `size`.
    fn drawn_text(model: &Model, size: (u32, u32)) -> Vec<String> {
        crate::harness::texts(crate::screens::view(model), (size.0 as f32, size.1 as f32))
            .into_iter()
            .map(|(text, _)| text)
            .collect()
    }

    /// The messages a press on the drawn text `label` produces in a window of
    /// `size`, as a person presses it: the pointer goes to the middle of the
    /// text, and the button goes down and comes up there. Panics when `label`
    /// is not drawn.
    fn press_drawn(model: &Model, label: &str, size: (u32, u32)) -> Vec<Message> {
        let mut driver = Driver::new(crate::screens::view(model), size);
        let at = driver
            .text_bounds(label)
            .unwrap_or_else(|| panic!("`{label}` is not drawn"));
        let (messages, _, _) = driver.send(&[
            mouse_move(at.center_x(), at.center_y()),
            mouse_press(),
            mouse_release(),
        ]);
        messages
    }

    /// The application as `begin` starts it over the preferences file at
    /// `path` when the file is there but cannot be read: no node, and the file
    /// held (docs/DECISIONS.md D43).
    fn held_app(path: &Path) -> (App, Receiver<Event>) {
        let loaded = preferences::load_checked(path);
        assert!(loaded.unreadable, "{}", path.display());
        let (mut app, events) =
            App::start(Model::new(loaded.prefs, None), Some(path.to_path_buf()));
        app.model.prefs_held = loaded.unreadable;
        (app, events.expect("the worker starts"))
    }

    /// Settings, with its node card drawn for no node set.
    fn settings_without_node(model: &mut Model) {
        model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        model.screen = Screen::Wallet(WalletPage {
            page: Page::Settings(SettingsPage {
                node: String::new(),
            }),
            ..WalletPage::default()
        });
    }

    /// A window tall enough to draw Settings whole, so that its node card is
    /// on the page and nothing scrolls out of reach.
    const SETTINGS_WINDOW: (u32, u32) = (1440, 2000);

    #[test]
    fn forgetting_the_node_says_so_when_no_folder_was_found_for_the_preferences() {
        // `boot` passes no path where neither an absolute XDG_DATA_HOME nor an
        // absolute HOME is set. Nothing can be written there, so a node forgotten
        // in that run is back at the next start: the node screen, where Forget
        // was pressed, says so (PR #29's review, P2-1; docs/DECISIONS.md D43).
        let (mut app, events) = App::start(Model::new(Preferences::default(), None), None);
        let events = events.expect("the worker starts");
        pump(&mut app, &events, |a| {
            !waiting(a).iter().any(|p| p == "SetNode")
        });
        app.model.screen = Screen::Node(NodeForm {
            url: preferences::DEFAULT_NODE.to_owned(),
            error: None,
            back: Back::Start,
        });
        let messages = press_drawn(&app.model, "Forget the node", (1440, 900));
        assert!(
            matches!(messages.as_slice(), [Message::ForgetNode]),
            "{messages:?}"
        );
        for message in messages {
            let _ = app.update(message);
        }
        pump(&mut app, &events, |a| waiting(a).is_empty());
        assert_eq!(app.model.prefs.node, None, "forgotten for this run");
        // Forgetting again cannot help with no folder, so the note does not
        // say to, and says what the next start does instead (PR #29's review).
        assert!(
            matches!(
                &app.model.screen,
                Screen::Node(f) if f.error.as_deref().is_some_and(|e| {
                    e.contains("for this run only")
                        && e.contains("uses the default node")
                        && !e.contains("again")
                })
            ),
            "{:?}",
            app.model.screen
        );
        assert!(
            drawn_text(&app.model, (1440, 900))
                .iter()
                .any(|t| t.contains("forgotten for this run only")),
            "the screen draws it"
        );
    }

    /// What the worker answers to `SetNode` from a form, given to `app` as
    /// it comes: the node is set, and it is not asked here, so no test
    /// reaches the network.
    fn node_set(app: &mut App, url: &str) {
        let _ = app.on_reply(
            Purpose::SetNode { form: true },
            Reply::NodeSet {
                url: url.to_owned(),
                view: None,
            },
        );
    }

    #[test]
    fn choosing_a_node_says_so_when_no_folder_was_found_for_the_preferences() {
        // A chosen node that cannot be saved is not the one the next start
        // uses: with no folder, that is the default. The screen it was chosen
        // on stays and says so, rather than going back as if it were saved
        // (PR #29's review; docs/DECISIONS.md D43). `alone` has no path, as
        // `boot` passes none when it finds no folder.
        let mut app = alone();
        app.model.screen = Screen::Node(NodeForm {
            url: "https://mine.example".to_owned(),
            error: None,
            back: Back::Start,
        });
        let messages = press_drawn(&app.model, "Use this node", (1440, 900));
        assert!(
            matches!(messages.as_slice(), [Message::SaveNode]),
            "{messages:?}"
        );
        for message in messages {
            let _ = app.update(message);
        }
        node_set(&mut app, "https://mine.example");
        assert_eq!(
            app.model.node.url.as_deref(),
            Some("https://mine.example"),
            "in use for this run"
        );
        assert!(
            matches!(
                &app.model.screen,
                Screen::Node(f) if f.error.as_deref().is_some_and(|e| {
                    e.contains("used for this run only") && e.contains("uses the default node")
                })
            ),
            "{:?}",
            app.model.screen
        );
        assert!(
            drawn_text(&app.model, (1440, 900))
                .iter()
                .any(|t| t.contains("used for this run only")),
            "the node screen draws it"
        );

        // Settings' Save says the same, at the top of its page.
        settings_without_node(&mut app.model);
        if let Screen::Wallet(WalletPage {
            page: Page::Settings(s),
            ..
        }) = &mut app.model.screen
        {
            s.node = "https://other.example".to_owned();
        }
        let messages = press_drawn(&app.model, "Save", SETTINGS_WINDOW);
        assert!(
            matches!(messages.as_slice(), [Message::Wallet(WalletMsg::SaveNode)]),
            "{messages:?}"
        );
        for message in messages {
            let _ = app.update(message);
        }
        node_set(&mut app, "https://other.example");
        assert!(
            drawn_text(&app.model, SETTINGS_WINDOW)
                .iter()
                .any(|t| t.contains("used for this run only")),
            "Settings draws it"
        );
    }

    #[test]
    fn a_saved_choice_goes_back_from_the_node_screen_and_says_nothing() {
        // The other side of the note above: a choice that is saved goes back
        // as before, with no note anywhere (PR #29's review).
        let scratch = Scratch::new("chosen-saved-back");
        let path = scratch.0.join("preferences");
        let mut app = alone_saving(Preferences::default(), path.clone());
        app.model.screen = Screen::Node(NodeForm {
            url: "https://mine.example".to_owned(),
            error: None,
            back: Back::Start,
        });
        for message in press_drawn(&app.model, "Use this node", (1440, 900)) {
            let _ = app.update(message);
        }
        node_set(&mut app, "https://mine.example");
        assert!(
            matches!(app.model.screen, Screen::Start { .. }),
            "{:?}",
            app.model.screen
        );
        assert_eq!(app.model.node.error, None);
        assert_eq!(
            preferences::load(&path).node.as_deref(),
            Some("https://mine.example")
        );
    }

    #[test]
    fn forgetting_on_the_node_card_leaves_no_word_of_the_address_before() {
        // Save with nothing typed says to type an address; a Forget that then
        // succeeds leaves no such word, nor the address, above a node just
        // forgotten (PR #29's review).
        let scratch = Scratch::new("held-forget-stale");
        let path = scratch.0.join("preferences");
        std::fs::write(&path, b"node = https://mine.example\n\xff\xfe\n").unwrap();
        let (mut app, events) = held_app(&path);
        settings_without_node(&mut app.model);
        for message in press_drawn(&app.model, "Save", SETTINGS_WINDOW) {
            let _ = app.update(message);
        }
        let said = "Type the node's address, starting https://.";
        assert!(
            drawn_text(&app.model, SETTINGS_WINDOW)
                .iter()
                .any(|t| t == said),
            "Save with nothing typed says so"
        );
        if let Screen::Wallet(WalletPage {
            page: Page::Settings(s),
            ..
        }) = &mut app.model.screen
        {
            s.node = "https://half".to_owned();
        }
        for message in press_drawn(&app.model, "Forget the node", SETTINGS_WINDOW) {
            let _ = app.update(message);
        }
        pump(&mut app, &events, |a| waiting(a).is_empty());
        assert!(!app.model.prefs_held, "the file is no longer held");
        assert!(
            matches!(
                &app.model.screen,
                Screen::Wallet(WalletPage {
                    page: Page::Settings(s),
                    error: None,
                    ..
                }) if s.node.is_empty()
            ),
            "{:?}",
            app.model.screen
        );
        assert!(
            !drawn_text(&app.model, SETTINGS_WINDOW)
                .iter()
                .any(|t| t == said),
            "the page no longer says it"
        );
    }

    #[test]
    fn an_unreadable_file_is_released_by_forgetting_the_node_on_the_node_screen() {
        let scratch = Scratch::new("held-forget-node-screen");
        let path = scratch.0.join("preferences");
        std::fs::write(&path, b"node = https://mine.example\n\xff\xfe\n").unwrap();
        let (mut app, events) = held_app(&path);
        app.model.screen = Screen::Node(NodeForm {
            url: String::new(),
            error: None,
            back: Back::Start,
        });
        // The screen says why no node is in use, and offers Forget: the choice
        // of none, which is what releases the file (PR #29's review, P2-2;
        // docs/DECISIONS.md D43).
        let drawn = drawn_text(&app.model, (1440, 900));
        assert!(drawn.iter().any(|t| t == HELD_TITLE), "{drawn:?}");
        // Its text does not say the wallet uses the default above a note that
        // says no node is in use (PR #29's review).
        assert!(
            drawn
                .iter()
                .any(|t| t.starts_with("The wallet uses no node now")),
            "{drawn:?}"
        );
        assert!(
            !drawn.iter().any(|t| t.starts_with("By default")),
            "{drawn:?}"
        );
        let messages = press_drawn(&app.model, "Forget the node", (1440, 900));
        assert!(
            matches!(messages.as_slice(), [Message::ForgetNode]),
            "{messages:?}"
        );
        for message in messages {
            let _ = app.update(message);
        }
        pump(&mut app, &events, |a| waiting(a).is_empty());
        assert!(!app.model.prefs_held, "the file is no longer held");
        let back = preferences::load_checked(&path);
        assert!(!back.unreadable, "the file is written again");
        assert_eq!(back.prefs.node, None, "saved as none");
    }

    #[test]
    fn an_unreadable_file_is_released_by_forgetting_the_node_on_the_node_card() {
        let scratch = Scratch::new("held-forget-node-card");
        let path = scratch.0.join("preferences");
        std::fs::write(&path, b"node = https://mine.example\n\xff\xfe\n").unwrap();
        let (mut app, events) = held_app(&path);
        settings_without_node(&mut app.model);
        // Settings says why, and its node card offers Forget as S2 does (PR #29's
        // review, P2-2; docs/DECISIONS.md D43).
        let drawn = drawn_text(&app.model, SETTINGS_WINDOW);
        assert!(drawn.iter().any(|t| t == HELD_TITLE), "{drawn:?}");
        let messages = press_drawn(&app.model, "Forget the node", SETTINGS_WINDOW);
        assert!(
            matches!(messages.as_slice(), [Message::ForgetNode]),
            "{messages:?}"
        );
        for message in messages {
            let _ = app.update(message);
        }
        pump(&mut app, &events, |a| waiting(a).is_empty());
        assert!(!app.model.prefs_held, "the file is no longer held");
        let back = preferences::load_checked(&path);
        assert!(!back.unreadable, "the file is written again");
        assert_eq!(back.prefs.node, None, "saved as none");
        assert!(
            !drawn_text(&app.model, SETTINGS_WINDOW)
                .iter()
                .any(|t| t == "Forget the node"),
            "nothing is left to forget once it is"
        );
    }

    #[test]
    fn forgetting_on_the_node_card_says_so_when_the_preferences_cannot_be_written() {
        let scratch = Scratch::new("held-forget-unwritten");
        // A folder where the file should be: it is there but cannot be read, and
        // what is written in its place cannot replace it.
        let path = scratch.0.join("preferences");
        std::fs::create_dir_all(&path).unwrap();
        let (mut app, events) = held_app(&path);
        settings_without_node(&mut app.model);
        let messages = press_drawn(&app.model, "Forget the node", SETTINGS_WINDOW);
        for message in messages {
            let _ = app.update(message);
        }
        pump(&mut app, &events, |a| waiting(a).is_empty());
        assert!(
            matches!(
                &app.model.screen,
                Screen::Wallet(p) if p.error.as_deref().is_some_and(|e| e.contains("could not be saved"))
            ),
            "{:?}",
            app.model.screen
        );
        assert!(
            drawn_text(&app.model, SETTINGS_WINDOW)
                .iter()
                .any(|t| t.contains("could not be saved")),
            "the page says so"
        );
    }

    #[test]
    fn forget_the_node_is_drawn_where_it_does_something_and_no_more() {
        // S2 offers it while a node is set or the file is held; Settings' node
        // card only while the file is held (docs/SCREENS.md, S2 and W11).
        let (mut app, _events) = App::start(
            Model::new(
                Preferences {
                    node: None,
                    ..Preferences::default()
                },
                None,
            ),
            None,
        );
        let forget = |app: &App| {
            drawn_text(&app.model, SETTINGS_WINDOW)
                .iter()
                .any(|t| t == "Forget the node")
        };
        app.model.screen = Screen::Node(NodeForm {
            url: String::new(),
            error: None,
            back: Back::Start,
        });
        assert!(!forget(&app), "no node set, and nothing held");
        app.model.node.url = Some("https://node.example".to_owned());
        assert!(forget(&app), "a node is set");
        app.model.node.url = None;
        settings_without_node(&mut app.model);
        assert!(!forget(&app), "Settings with no node set, and nothing held");
        app.model.prefs_held = true;
        assert!(forget(&app), "Settings while the file is held");
        // A node set is not enough on Settings: its card offers Forget only
        // as the way out of a held file (PR #29's review).
        app.model.prefs_held = false;
        app.model.node.url = Some("https://node.example".to_owned());
        assert!(
            drawn_text(&app.model, SETTINGS_WINDOW)
                .iter()
                .any(|t| t == "Check now"),
            "the node card is drawn"
        );
        assert!(!forget(&app), "Settings with a node set, and nothing held");
    }

    // The device unlock (docs/DECISIONS.md D45). A `Host` holds plain function
    // pointers, so the fake shell's functions keep what they were asked, and
    // what they are to answer, in thread-locals. Each test runs on a thread of
    // its own, and the functions are called on the test's thread.

    /// What the fake shell's check answers next, once it is asked. Without one
    /// queued it declines, as a person who walks away from the prompt does.
    #[derive(Clone, Copy)]
    enum Next {
        Password(&'static str),
        Declined,
        Missing,
        Failed(&'static str),
    }

    thread_local! {
        static NEXT: std::cell::RefCell<Option<Next>> = const { std::cell::RefCell::new(None) };
        static ASKED: std::cell::RefCell<Vec<([u8; 32], String)>> =
            const { std::cell::RefCell::new(Vec::new()) };
        static KEPT: std::cell::RefCell<Vec<([u8; 32], String)>> =
            const { std::cell::RefCell::new(Vec::new()) };
        static FORGOTTEN: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        static KEEP_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static FORGET_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    fn fake_check() -> Option<host::DeviceCheck> {
        Some(host::DeviceCheck::FaceId)
    }

    fn fake_keep(identifier: &[u8; 32], password: &str) -> Result<(), String> {
        if KEEP_FAILS.with(std::cell::Cell::get) {
            return Err("the keychain refused".to_owned());
        }
        KEPT.with(|k| k.borrow_mut().push((*identifier, password.to_owned())));
        Ok(())
    }

    fn fake_release(
        identifier: &[u8; 32],
        reason: &str,
        answer: Box<dyn FnOnce(host::Released) + Send>,
    ) {
        ASKED.with(|a| a.borrow_mut().push((*identifier, reason.to_owned())));
        let next = NEXT
            .with(|n| n.borrow_mut().take())
            .unwrap_or(Next::Declined);
        let released = match next {
            Next::Password(text) => {
                let mut text = text.to_owned();
                host::Released::Password(SecretText::take(&mut text))
            }
            Next::Declined => host::Released::Declined,
            Next::Missing => host::Released::Missing,
            Next::Failed(why) => host::Released::Failed(why.to_owned()),
        };
        // The shell answers from a thread of its own, as its prompt does.
        std::thread::spawn(move || answer(released));
    }

    fn fake_forget() -> Result<(), String> {
        FORGOTTEN.with(|f| f.set(f.get() + 1));
        if FORGET_FAILS.with(std::cell::Cell::get) {
            return Err("the keychain refused the delete".to_owned());
        }
        Ok(())
    }

    fn copy_nothing(_: &str) {}

    fn share_nothing(_: &str, _: &str) -> Result<(), String> {
        Ok(())
    }

    fn back_nothing() {}

    /// A phone's Host, with every device field, as a shell that offers the
    /// unlock gives it.
    fn fake_host(private_dir: PathBuf) -> Host {
        Host {
            private_dir,
            copy: copy_nothing,
            share: share_nothing,
            back: back_nothing,
            captured: None,
            device_check: Some(fake_check),
            keep_password: Some(fake_keep),
            release_password: Some(fake_release),
            forget_password: Some(fake_forget),
            scan: None,
            cancel_scan: None,
            scan_preview: None,
        }
    }

    /// A phone's application over a real worker, as a phone shell runs it: its
    /// store and its preferences in `scratch`, and the fake shell's device
    /// unlock. The answers the shell gives come on the returned receiver.
    fn phone_with(
        scratch: &Scratch,
        prefs: Preferences,
    ) -> (
        App,
        Receiver<Event>,
        iced::futures::channel::mpsc::UnboundedReceiver<host::Answer>,
    ) {
        let mut model = Model::new(prefs, Some(scratch.0.join("keystore")));
        model.confine(scratch.0.clone());
        let (answers, received) = iced::futures::channel::mpsc::unbounded();
        model.device = host::Device::of(&fake_host(scratch.0.clone()), answers);
        let (app, events) = App::start(model, Some(scratch.0.join(preferences::FILE_NAME)));
        (app, events.expect("the worker starts"), received)
    }

    /// A phone's application with no node, so that nothing is asked of one.
    fn phone(
        scratch: &Scratch,
    ) -> (
        App,
        Receiver<Event>,
        iced::futures::channel::mpsc::UnboundedReceiver<host::Answer>,
    ) {
        phone_with(
            scratch,
            Preferences {
                node: None,
                ..Preferences::default()
            },
        )
    }

    /// The store restored with `PASSWORD` in the folder the application
    /// offers, and locked: S7 is then shown, with the store's password to type.
    fn restored_and_locked(app: &mut App, events: &Receiver<Event>) {
        let _ = app.update(Message::Choose(StartChoice::Restore));
        let _ = app.update(Message::Continue);
        let _ = app.update(Message::Password(typed(PASSWORD)));
        let _ = app.update(Message::Again(typed(PASSWORD)));
        let _ = app.update(Message::PhraseText(typed(PHRASE)));
        let _ = app.update(Message::RestoreWallet);
        pump(app, events, |a| matches!(a.model.screen, Screen::Wallet(_)));
        let _ = app.update(Message::Lock);
        pump(app, events, |a| matches!(a.model.screen, Screen::Unlock(_)));
    }

    /// `password` typed on S7 and submitted, until the store opens or the form
    /// says why it did not.
    fn unlock_typed(app: &mut App, events: &Receiver<Event>, password: &str) {
        let _ = app.update(Message::Password(typed_now(app, password)));
        let _ = app.update(Message::UnlockWallet);
        pump(app, events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
                || matches!(&a.model.screen, Screen::Unlock(u) if u.error.is_some())
        });
    }

    /// The offer after `PASSWORD` opened the store, accepted: the password is
    /// kept, the unlock is on, and the store is locked again on S7.
    fn kept_and_locked(app: &mut App, events: &Receiver<Event>) {
        unlock_typed(app, events, PASSWORD);
        assert!(app.model.device_offer, "the offer waits");
        let _ = app.update(Message::DeviceOffer(true));
        let _ = app.update(Message::Lock);
        pump(app, events, |a| matches!(a.model.screen, Screen::Unlock(_)));
    }

    /// The answer the shell gave on its thread, once it has come.
    fn next_answer(
        answers: &mut iced::futures::channel::mpsc::UnboundedReceiver<host::Answer>,
    ) -> host::Answer {
        use iced::futures::channel::mpsc::TryRecvError;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match answers.try_recv() {
                Ok(answer) => return answer,
                Err(TryRecvError::Closed) => panic!("the answers were closed"),
                Err(TryRecvError::Empty) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(TryRecvError::Empty) => panic!("no answer came"),
            }
        }
    }

    /// Delivers the shell's answer to the next ask, as the interface takes it.
    fn deliver(
        app: &mut App,
        answers: &mut iced::futures::channel::mpsc::UnboundedReceiver<host::Answer>,
    ) {
        let answer = next_answer(answers);
        let _ = app.update(Message::Released(answer));
    }

    fn asked() -> Vec<([u8; 32], String)> {
        ASKED.with(|a| a.borrow().clone())
    }

    fn kept() -> Vec<([u8; 32], String)> {
        KEPT.with(|k| k.borrow().clone())
    }

    fn forgotten() -> u32 {
        FORGOTTEN.with(std::cell::Cell::get)
    }

    /// The unlock is on for the store in `scratch`, as an accepted offer leaves
    /// it, with its identifier made. Written as the offer would write it.
    fn turned_on(app: &mut App, scratch: &Scratch) -> [u8; 32] {
        let identifier = store_identifier::make(&scratch.0).expect("an identifier");
        app.set_device_unlock(DeviceUnlock::On);
        identifier
    }

    /// The fake shell's state is the test's own: reset at the test's start.
    fn reset_shell() {
        NEXT.with(|n| *n.borrow_mut() = None);
        ASKED.with(|a| a.borrow_mut().clear());
        KEPT.with(|k| k.borrow_mut().clear());
        FORGOTTEN.with(|f| f.set(0));
        KEEP_FAILS.with(|f| f.set(false));
        FORGET_FAILS.with(|f| f.set(false));
    }

    #[test]
    fn the_device_unlock_is_offered_once_after_a_password_unlock() {
        reset_shell();
        let scratch = Scratch::new("device-offer");
        let (mut app, events, _answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Unoffered);

        unlock_typed(&mut app, &events, PASSWORD);
        assert!(
            app.model.device_offer,
            "the offer waits once the store opened"
        );
        let path = scratch.0.join(preferences::FILE_NAME);
        assert_eq!(
            preferences::load(&path).device_unlock,
            DeviceUnlock::Off,
            "the offer is recorded as made, as it is shown"
        );
        assert!(
            kept().is_empty(),
            "nothing is kept until the person accepts"
        );

        // Accepting keeps the password under the store's identifier, which is
        // drawn and written now, and turns the unlock on.
        let _ = app.update(Message::DeviceOffer(true));
        assert!(!app.model.device_offer);
        assert!(app.model.device_unlock_on());
        assert_eq!(preferences::load(&path).device_unlock, DeviceUnlock::On);
        let identifier = store_identifier::read(&scratch.0)
            .expect("readable")
            .expect("drawn with the offer");
        assert_eq!(kept(), vec![(identifier, PASSWORD.to_owned())]);
        assert!(app.model.pending_keep.is_none(), "the copy is wiped");
    }

    #[test]
    fn an_answer_with_no_offer_shown_does_not_keep_a_password_in_flight() {
        reset_shell();
        let scratch = Scratch::new("device-early");
        let (mut app, events, _answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        // The unlock is sent, and no answer has come: nothing is offered yet.
        let _ = app.update(Message::Password(typed_now(&app, PASSWORD)));
        let _ = app.update(Message::UnlockWallet);
        assert!(
            app.model.pending_keep.is_some(),
            "the copy waits for the reply"
        );
        let _ = app.update(Message::DeviceOffer(true));
        assert!(
            kept().is_empty(),
            "no password is kept before the store opens"
        );
        assert!(
            app.model.pending_keep.is_some(),
            "and the copy is not taken"
        );
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
        assert!(
            app.model.device_offer,
            "the offer comes once the store opened"
        );
    }

    #[test]
    fn the_offer_is_not_made_again_after_a_restart() {
        reset_shell();
        let scratch = Scratch::new("device-restart");
        let (mut app, events, _answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        unlock_typed(&mut app, &events, PASSWORD);
        let _ = app.update(Message::DeviceOffer(false));
        let _ = app.update(Message::Lock);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Unlock(_))
        });
        drop(app);

        // The next start reads the preferences the offer left.
        let path = scratch.0.join(preferences::FILE_NAME);
        let prefs = Preferences {
            node: None,
            ..preferences::load(&path)
        };
        let (mut app, events, _answers) = phone_with(&scratch, prefs);
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Off);
        unlock_typed(&mut app, &events, PASSWORD);
        assert!(!app.model.device_offer, "offered once, and not again");
    }

    #[test]
    fn declining_the_offer_keeps_nothing() {
        reset_shell();
        let scratch = Scratch::new("device-decline");
        let (mut app, events, _answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        unlock_typed(&mut app, &events, PASSWORD);
        let _ = app.update(Message::DeviceOffer(false));
        assert!(!app.model.device_offer);
        assert!(app.model.pending_keep.is_none(), "the copy is wiped");
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Off);
        assert!(kept().is_empty(), "no password is kept");
        assert!(
            !scratch.0.join(store_identifier::FILE_NAME).exists(),
            "and no identifier is written for one"
        );
    }

    #[test]
    fn a_device_unlock_sends_unlock_with_the_password_it_released() {
        reset_shell();
        let scratch = Scratch::new("device-unlock");
        let (mut app, events, mut answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        unlock_typed(&mut app, &events, PASSWORD);
        let _ = app.update(Message::DeviceOffer(true));
        let _ = app.update(Message::Lock);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Unlock(_))
        });
        let identifier = store_identifier::read(&scratch.0).unwrap().unwrap();

        NEXT.with(|n| *n.borrow_mut() = Some(Next::Password(PASSWORD)));
        let _ = app.update(Message::UnlockWithDevice);
        assert_eq!(
            asked(),
            vec![(identifier, UNLOCK_REASON.to_owned())],
            "the check is asked for the store's identifier, with the reason"
        );
        deliver(&mut app, &mut answers);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
        assert!(
            app.model.device_unlock_on(),
            "a good unlock keeps the password"
        );
        assert_eq!(forgotten(), 0);
    }

    #[test]
    fn a_wrong_kept_password_is_forgotten_and_the_form_is_shown() {
        reset_shell();
        let scratch = Scratch::new("device-wrong");
        let (mut app, events, mut answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        kept_and_locked(&mut app, &events);

        NEXT.with(|n| *n.borrow_mut() = Some(Next::Password("not the password")));
        let _ = app.update(Message::UnlockWithDevice);
        deliver(&mut app, &mut answers);
        pump(
            &mut app,
            &events,
            |a| matches!(&a.model.screen, Screen::Unlock(u) if u.error.is_some()),
        );
        assert_eq!(forgotten(), 1, "the kept password is deleted");
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Off);
        assert_eq!(
            preferences::load(&scratch.0.join(preferences::FILE_NAME)).device_unlock,
            DeviceUnlock::Off,
            "and the setting is written"
        );
        assert_eq!(app.model.device_note.as_deref(), Some(DEVICE_WRONG_NOTE));
        assert!(app.model.busy.is_none());
    }

    #[test]
    fn a_mismatched_identifier_forgets_the_password_and_shows_the_form() {
        reset_shell();
        let scratch = Scratch::new("device-mismatch");
        let (mut app, events, mut answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        kept_and_locked(&mut app, &events);

        NEXT.with(|n| *n.borrow_mut() = Some(Next::Missing));
        let _ = app.update(Message::UnlockWithDevice);
        deliver(&mut app, &mut answers);
        assert_eq!(forgotten(), 1, "what is kept is deleted");
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Off);
        assert_eq!(app.model.device_note.as_deref(), Some(DEVICE_MISSING_NOTE));
        assert!(app.model.busy.is_none(), "no unlock was sent");
        assert!(matches!(&app.model.screen, Screen::Unlock(u) if u.error.is_none()));
    }

    #[test]
    fn without_a_store_identifier_the_check_is_not_asked() {
        reset_shell();
        let scratch = Scratch::new("device-no-identifier");
        let (mut app, events, _answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        app.set_device_unlock(DeviceUnlock::On);

        let _ = app.update(Message::UnlockWithDevice);
        assert!(
            asked().is_empty(),
            "nothing can be kept under no identifier"
        );
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Off);
        assert_eq!(app.model.device_note.as_deref(), Some(DEVICE_MISSING_NOTE));
    }

    #[test]
    fn a_declined_check_keeps_the_password() {
        reset_shell();
        let scratch = Scratch::new("device-declined");
        let (mut app, events, mut answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        kept_and_locked(&mut app, &events);

        NEXT.with(|n| *n.borrow_mut() = Some(Next::Declined));
        let _ = app.update(Message::UnlockWithDevice);
        deliver(&mut app, &mut answers);
        assert!(app.model.device_unlock_on(), "still on");
        assert_eq!(forgotten(), 0, "nothing is deleted");
        assert!(app.model.busy.is_none());
        assert!(matches!(&app.model.screen, Screen::Unlock(u) if u.error.is_none()));
    }

    #[test]
    fn a_failed_check_says_why_and_keeps_the_password() {
        reset_shell();
        let scratch = Scratch::new("device-failed");
        let (mut app, events, mut answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        kept_and_locked(&mut app, &events);

        NEXT.with(|n| *n.borrow_mut() = Some(Next::Failed("the keychain is locked")));
        let _ = app.update(Message::UnlockWithDevice);
        deliver(&mut app, &mut answers);
        assert!(
            app.model
                .device_note
                .as_deref()
                .is_some_and(|n| n.contains("the keychain is locked")),
            "{:?}",
            app.model.device_note
        );
        assert!(app.model.device_unlock_on());
        assert_eq!(forgotten(), 0);
    }

    #[test]
    fn turning_the_unlock_off_forgets_the_password() {
        reset_shell();
        let scratch = Scratch::new("device-off");
        let (mut app, events, _answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        turned_on(&mut app, &scratch);

        let _ = app.update(Message::TurnOffDeviceUnlock);
        assert_eq!(forgotten(), 1, "the password is deleted");
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Off);
        assert_eq!(
            preferences::load(&scratch.0.join(preferences::FILE_NAME)).device_unlock,
            DeviceUnlock::Off
        );
    }

    #[test]
    fn a_password_that_cannot_be_deleted_leaves_the_unlock_on() {
        reset_shell();
        let scratch = Scratch::new("device-undeleted");
        let (mut app, events, _answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        turned_on(&mut app, &scratch);

        FORGET_FAILS.with(|f| f.set(true));
        let _ = app.update(Message::TurnOffDeviceUnlock);
        assert_eq!(
            app.model.prefs.device_unlock,
            DeviceUnlock::On,
            "not off yet"
        );
        assert!(
            app.model
                .device_note
                .as_deref()
                .is_some_and(|n| n.contains("could not be deleted")),
            "{:?}",
            app.model.device_note
        );

        FORGET_FAILS.with(|f| f.set(false));
        let _ = app.update(Message::TurnOffDeviceUnlock);
        assert_eq!(
            app.model.prefs.device_unlock,
            DeviceUnlock::Off,
            "tried again"
        );
        assert_eq!(forgotten(), 2);
    }

    #[test]
    fn a_password_released_after_the_app_left_the_foreground_is_not_used() {
        reset_shell();
        let scratch = Scratch::new("device-stale");
        let (mut app, events, mut answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        kept_and_locked(&mut app, &events);

        NEXT.with(|n| *n.borrow_mut() = Some(Next::Password(PASSWORD)));
        let _ = app.update(Message::UnlockWithDevice);
        // The application leaves the foreground while the check is asked.
        app.model.left.fetch_add(1, Ordering::SeqCst);
        let _ = app.update(Message::Input);
        assert_eq!(app.model.device_asked, None, "the ask is dropped");
        deliver(&mut app, &mut answers);
        assert!(app.model.busy.is_none(), "the password was not sent");
        assert!(matches!(&app.model.screen, Screen::Unlock(_)));
    }

    #[test]
    fn a_second_ask_waits_for_the_first_answer() {
        reset_shell();
        let scratch = Scratch::new("device-twice");
        let (mut app, events, mut answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        kept_and_locked(&mut app, &events);

        NEXT.with(|n| *n.borrow_mut() = Some(Next::Declined));
        let _ = app.update(Message::UnlockWithDevice);
        let _ = app.update(Message::UnlockWithDevice);
        assert_eq!(asked().len(), 1, "one prompt at a time");
        deliver(&mut app, &mut answers);
        assert!(app.model.device_asked.is_none());
    }

    #[test]
    fn the_desktop_is_unchanged() {
        reset_shell();
        let scratch = Scratch::new("desktop-unchanged");
        // The desktop's application: no shell, so no device unlock.
        let (mut app, events) = app(&scratch);
        restored_and_locked(&mut app, &events);
        assert!(app.model.device.is_none());
        unlock_typed(&mut app, &events, PASSWORD);
        assert!(matches!(app.model.screen, Screen::Wallet(_)));
        assert!(!app.model.device_offer, "nothing is offered");
        assert!(app.model.pending_keep.is_none(), "nothing is copied");
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Unoffered);
        let _ = app.update(Message::DeviceOffer(true));
        let _ = app.update(Message::UnlockWithDevice);
        let _ = app.update(Message::TurnOffDeviceUnlock);
        assert!(asked().is_empty() && kept().is_empty() && forgotten() == 0);
        assert!(!scratch.0.join(store_identifier::FILE_NAME).exists());
    }

    #[test]
    fn the_offer_waits_on_the_wallet_and_a_lock_takes_it_away() {
        reset_shell();
        let scratch = Scratch::new("device-lock-offer");
        let (mut app, events, _answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        unlock_typed(&mut app, &events, PASSWORD);
        assert!(app.model.device_offer);
        assert!(
            app.model.pending_keep.is_some(),
            "the copy waits for the answer"
        );

        let _ = app.update(Message::Lock);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Unlock(_))
        });
        assert!(!app.model.device_offer, "the offer ends with the lock");
        assert!(app.model.pending_keep.is_none(), "and the copy is wiped");
        assert_eq!(
            app.model.prefs.device_unlock,
            DeviceUnlock::Off,
            "still offered"
        );
        assert!(kept().is_empty());
    }

    #[test]
    fn no_secret_appears_in_debug_output() {
        reset_shell();
        let scratch = Scratch::new("device-debug");
        let (mut app, events, _answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        unlock_typed(&mut app, &events, PASSWORD);
        assert!(app.model.pending_keep.is_some(), "the copy is held");
        let shown = format!("{:?}", app.model);
        assert!(!shown.contains(PASSWORD), "the model prints no password");
        assert!(format!("{:?}", app.model.pending_keep).contains("Kept(..)"));
        let answer = host::Answer::new(
            1,
            host::Released::Password(SecretText::take(&mut String::from(PASSWORD))),
        );
        assert_eq!(
            format!("{:?}", Message::Released(answer)),
            "Released(Answer(..))"
        );
    }

    // The device unlock's screens and their messages (docs/DECISIONS.md D45
    // item 4). The check is asked on a return to the foreground, once.

    #[test]
    fn a_return_to_the_foreground_asks_the_check_once_and_an_activation_asks_nothing() {
        reset_shell();
        let scratch = Scratch::new("device-foreground");
        let (mut app, events, mut answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        kept_and_locked(&mut app, &events);

        NEXT.with(|n| *n.borrow_mut() = Some(Next::Declined));
        let _ = app.update(Message::Lifecycle(iced::mobile::LifecycleEvent::Foreground));
        assert_eq!(asked().len(), 1, "the check is asked on the return");
        deliver(&mut app, &mut answers);
        // A Face ID prompt makes the application inactive, and it is active again
        // after it: neither asks.
        let _ = app.update(Message::Lifecycle(iced::mobile::LifecycleEvent::Inactive));
        let _ = app.update(Message::Lifecycle(iced::mobile::LifecycleEvent::Active));
        assert_eq!(asked().len(), 1, "an activation asks nothing");
        assert!(
            app.model.device_unlock_on(),
            "a declined check keeps the unlock"
        );
        assert!(matches!(&app.model.screen, Screen::Unlock(_)));
    }

    #[test]
    fn a_return_to_the_foreground_asks_nothing_on_a_wallet_page_or_without_the_unlock() {
        reset_shell();
        let scratch = Scratch::new("device-foreground-quiet");
        let (mut app, events, mut answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        // The unlock is off: nothing is asked on a return to S7.
        let _ = app.update(Message::Lifecycle(iced::mobile::LifecycleEvent::Foreground));
        assert!(asked().is_empty(), "the unlock is not on");

        kept_and_locked(&mut app, &events);
        NEXT.with(|n| *n.borrow_mut() = Some(Next::Password(PASSWORD)));
        let _ = app.update(Message::Lifecycle(iced::mobile::LifecycleEvent::Foreground));
        deliver(&mut app, &mut answers);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
        // Unlocked, a return to the foreground asks nothing.
        let _ = app.update(Message::Lifecycle(iced::mobile::LifecycleEvent::Foreground));
        assert_eq!(asked().len(), 1, "a wallet page asks nothing");
    }

    #[test]
    fn a_device_check_is_not_asked_by_a_desktop_return_to_the_foreground() {
        reset_shell();
        let scratch = Scratch::new("device-desktop-foreground");
        let (mut app, events) = app(&scratch);
        restored_and_locked(&mut app, &events);
        let _ = app.update(Message::Lifecycle(iced::mobile::LifecycleEvent::Foreground));
        let _ = app.update(Message::UnlockWithDevice);
        assert!(asked().is_empty(), "the desktop offers no device unlock");
        assert!(matches!(&app.model.screen, Screen::Unlock(_)));
    }

    #[test]
    fn offering_again_after_a_decline_offers_the_unlock_at_the_next_password_unlock() {
        reset_shell();
        let scratch = Scratch::new("device-again");
        let (mut app, events, _answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        unlock_typed(&mut app, &events, PASSWORD);
        let _ = app.update(Message::DeviceOffer(false));
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Off);
        let _ = app.update(Message::Lock);
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Unlock(_))
        });

        let _ = app.update(Message::OfferDeviceUnlockAgain);
        let path = scratch.0.join(preferences::FILE_NAME);
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Unoffered);
        assert_eq!(
            preferences::load(&path).device_unlock,
            DeviceUnlock::Unoffered
        );
        assert!(kept().is_empty(), "nothing is kept by asking again");

        unlock_typed(&mut app, &events, PASSWORD);
        assert!(app.model.device_offer, "the offer comes again");
        let _ = app.update(Message::DeviceOffer(true));
        assert!(app.model.device_unlock_on(), "and it is taken");
        assert_eq!(kept().len(), 1, "the password is kept once it is");
    }

    #[test]
    fn offering_again_does_nothing_unless_the_unlock_is_off_and_no_offer_is_shown() {
        reset_shell();
        let scratch = Scratch::new("device-again-guard");
        let (mut app, events, _answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);

        let _ = app.update(Message::OfferDeviceUnlockAgain);
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Unoffered);

        turned_on(&mut app, &scratch);
        let _ = app.update(Message::OfferDeviceUnlockAgain);
        assert_eq!(
            app.model.prefs.device_unlock,
            DeviceUnlock::On,
            "already on"
        );

        let _ = app.update(Message::TurnOffDeviceUnlock);
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Off);
        let _ = app.update(Message::OfferDeviceUnlockAgain);
        assert_eq!(app.model.prefs.device_unlock, DeviceUnlock::Unoffered);
        unlock_typed(&mut app, &events, PASSWORD);
        assert!(app.model.device_offer, "the offer is on the wallet page");
        let _ = app.update(Message::OfferDeviceUnlockAgain);
        assert_eq!(
            app.model.prefs.device_unlock,
            DeviceUnlock::Off,
            "an offer on the screen is answered, not replaced"
        );
    }

    #[test]
    fn a_password_unlock_clears_what_the_device_unlock_last_found() {
        reset_shell();
        let scratch = Scratch::new("device-note-clears");
        let (mut app, events, mut answers) = phone(&scratch);
        restored_and_locked(&mut app, &events);
        kept_and_locked(&mut app, &events);

        NEXT.with(|n| *n.borrow_mut() = Some(Next::Missing));
        let _ = app.update(Message::UnlockWithDevice);
        deliver(&mut app, &mut answers);
        assert_eq!(app.model.device_note.as_deref(), Some(DEVICE_MISSING_NOTE));

        unlock_typed(&mut app, &events, PASSWORD);
        assert!(
            app.model.device_note.is_none(),
            "a password unlock starts afresh"
        );
    }
}
