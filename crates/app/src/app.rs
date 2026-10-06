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

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iced::futures::channel::mpsc;
use iced::widget::operation;
use iced::{Element, Event as IcedEvent, Subscription, Task, event, keyboard, mouse, touch};
use tawara_wallet_core::explorer::{AccountHistory, BlocksView, ExplorerRefusal, MempoolView};
use tawara_wallet_core::location::{self, Environment, Platform};
use tawara_wallet_core::preferences::{self, Preferences};
use tawara_wallet_core::view::WalletView;
use tawara_wallet_core::{
    Activity, Command, Config, Event, HttpsNode, LockReason, NetworkName, PhraseForDisplay,
    Progress, Refusal, RefusalKind, Reply, RequestId, SecretText, SyncState, WorkerHandle,
};

pub use wallet::{
    AccountPage, ActivityPage, AddAccountPage, DestinationRow, Done, Level, Page, ReceivePage,
    RecoveryPage, Remedy, ReportKey, ResignPage, SendPage, SendStage, SentPage, SettingsPage,
    Signed, SpendForm, SubmitPage, To, WalletMsg, remedy, spend_request,
};

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
    /// re-signing starts from what was typed for it, a lock brings it back
    /// once its store is unlocked, and the stopped screen offers to save
    /// it. It is forgotten once its account is in sync again. Nothing in it
    /// is secret, and what the store showed is dropped from it.
    pub signed: Vec<Signed>,
    /// The worker stopped: `Some(true)` when it panicked. Nothing more can
    /// be done in this run.
    pub stopped: Option<bool>,
    /// The window's width in logical pixels: below [`WIDE`], rows of two
    /// cards stack (docs/DECISIONS.md D27, item 9).
    pub width: f32,
    /// The wall clock, in milliseconds since the epoch, moved on each
    /// second: what a block's age is told against.
    pub clock_ms: i64,
    /// The time zone dates are shown in: the system's.
    pub zone: crate::ui::Zone,
    /// The open store's transactions from the node's index (W1, W10).
    pub activity: Explored<Vec<AccountHistory>>,
    /// The node's newest blocks (W1's network card).
    pub blocks: Explored<BlocksView>,
    /// How many transactions wait in the node's queue (W1's network card).
    pub mempool: Explored<MempoolView>,
}

/// What an explorer read last answered, and whether another is on its way.
/// Reads go on in the background: the screen shows the last answer while
/// the next comes.
#[derive(Clone, Debug)]
pub struct Explored<T> {
    pub last: Option<Result<T, ExplorerRefusal>>,
    pub reading: bool,
}

impl<T> Default for Explored<T> {
    fn default() -> Explored<T> {
        Explored {
            last: None,
            reading: false,
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
#[derive(Clone)]
pub struct Typed(pub String);

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
    /// The window opened or changed size: its width.
    Width(f32),
    /// A second passed: the wall clock, in milliseconds since the epoch.
    /// Not the person's input.
    Tick(i64),
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
}

/// Empty a secret field so its text is zeroized, not just dropped:
/// `SecretText::take` moves the buffer into a zeroizing one and leaves the
/// field empty (docs/PLAN.md section 4.2).
pub fn wipe(field: &mut String) {
    drop(SecretText::take(field));
}

/// Replace a secret field's text, zeroizing the old.
fn replace(field: &mut String, typed: Typed) {
    wipe(field);
    *field = typed.0;
}

impl Model {
    /// The model at start-up: the first screen, before the worker has said
    /// anything. When a store is there to open (the one last opened,
    /// wherever it is, or else the one in the default folder) it is the
    /// unlock screen for it; otherwise it is the start.
    #[must_use]
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
            width: crate::WINDOW.0,
            clock_ms: now_ms(),
            zone: crate::ui::Zone::System,
            activity: Explored::default(),
            blocks: Explored::default(),
            mempool: Explored::default(),
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

    /// The store to offer to unlock: the one last opened when it is still
    /// there, or else the one in the default folder when there is one.
    fn store_to_open(&self) -> Option<String> {
        // A relative folder means nothing from another working directory;
        // only an absolute one is remembered.
        let remembered = self.prefs.store.clone().filter(|s| {
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
        let old = core::mem::replace(
            &mut self.screen,
            Screen::Start {
                choice: StartChoice::Create,
            },
        );
        match old {
            Screen::NewWallet(mut f) => {
                wipe(&mut f.password);
                wipe(&mut f.again);
                Screen::NewWallet(f)
            }
            Screen::Restore(mut r) => {
                wipe(&mut r.form.password);
                wipe(&mut r.form.again);
                wipe(&mut r.phrase);
                Screen::Restore(r)
            }
            Screen::Unlock(mut u) => {
                wipe(&mut u.password);
                Screen::Unlock(u)
            }
            Screen::Confirm(mut p) => {
                for w in &mut p.words {
                    wipe(w);
                }
                Screen::Confirm(p)
            }
            other => other,
        }
    }

    /// Back to S3, in the folder the pending phrase was for (the default
    /// one when there was none), with `note` saying why.
    fn back_to_new_wallet(&mut self, note: Option<String>) {
        let dir = match self.leave() {
            Screen::Phrase(p) | Screen::Confirm(p) => p.dir,
            _ => self.default_dir_text(),
        };
        self.screen = Screen::NewWallet(PasswordForm {
            note,
            ..PasswordForm::in_dir(dir)
        });
    }

    /// The unlock screen for `dir`, or the default folder.
    fn unlock_screen(&mut self, dir: Option<String>, error: Option<String>, note: Option<String>) {
        let _ = self.leave();
        let dir = dir
            .or_else(|| self.store_to_open())
            .unwrap_or_else(|| self.default_dir_text());
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
        let prefs_path = preferences::default_file(platform, &env).ok();
        let prefs = prefs_path
            .as_deref()
            .map(preferences::load)
            .unwrap_or_default();
        let default_dir = location::default_store_dir(platform, &env).ok();
        let mut model = Model::new(prefs, default_dir);
        model.downloads = location::downloads_dir(platform, &env);
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
        event::listen_with(person_input)
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
        if let Some(w) = &self.worker {
            w.handle.touch();
        }
    }

    fn save_prefs(&self) {
        if let Some(path) = &self.prefs_path {
            // A preference that cannot be written is lost at the next start,
            // and nothing else depends on it.
            let _ = preferences::save(path, &self.model.prefs);
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let m = &mut self.model;
        match message {
            Message::Worker(event) => {
                if let Some(event) = event.take() {
                    self.on_event(event);
                }
            }
            Message::Input => self.touch(),
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
            Message::Password(typed) => match &mut m.screen {
                Screen::NewWallet(f) => replace(&mut f.password, typed),
                Screen::Restore(r) => replace(&mut r.form.password, typed),
                Screen::Unlock(u) => replace(&mut u.password, typed),
                _ => {}
            },
            Message::Again(typed) => match &mut m.screen {
                Screen::NewWallet(f) => replace(&mut f.again, typed),
                Screen::Restore(r) => replace(&mut r.form.again, typed),
                _ => {}
            },
            Message::PhraseText(typed) => {
                if let Screen::Restore(r) = &mut m.screen {
                    replace(&mut r.phrase, typed);
                }
            }
            Message::Word(i, typed) => {
                if let Screen::Confirm(p) = &mut m.screen
                    && let Some(w) = p.words.get_mut(i)
                {
                    replace(w, typed);
                }
            }
            Message::CreateWallet => {
                if let Screen::NewWallet(f) = &mut m.screen {
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
                if let Screen::Restore(r) = &mut m.screen {
                    r.form.error = None;
                    let command = Command::CreateFromPhrase {
                        dir: store_dir(&r.form.dir),
                        password: SecretText::take(&mut r.form.password),
                        password_again: SecretText::take(&mut r.form.again),
                        phrase: SecretText::take(&mut r.phrase),
                    };
                    self.send(command, Purpose::CreateFromPhrase);
                }
            }
            Message::UnlockWallet => {
                if let Screen::Unlock(u) = &mut m.screen {
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
            Message::Width(width) => m.width = width,
            Message::Tick(ms) => m.clock_ms = ms,
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

    fn on_event(&mut self, event: Event) {
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
                    self.on_reply(purpose, reply);
                }
            }
            Event::Locked { reason } => {
                let dir = self
                    .model
                    .wallet
                    .take()
                    .map(|w| w.dir.display().to_string());
                self.model.busy = None;
                // What the index holds for the store goes with it.
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
                    self.model.back_to_new_wallet(Some(format!(
                        "The recovery phrase was discarded {why}, before it was confirmed, and \
                         no store was written. Choose the password again and continue to get a \
                         new phrase."
                    )));
                    return;
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
                                    store is unlocked: save the bytes then.";
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
            Event::Stopped { panicked } => self.stop(panicked),
        }
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
        self.worker = None;
    }

    /// The worker's answer to a command. A spend kept for a reservation the
    /// answer shows settled is forgotten after it (see [`Model::signed`]).
    fn on_reply(&mut self, purpose: Purpose, reply: Reply) {
        if purpose.wallet_page() {
            self.on_wallet_reply(purpose, reply);
        } else {
            self.answer(purpose, reply);
        }
        self.model.forget_settled();
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
        IcedEvent::Keyboard(keyboard::Event::KeyPressed { .. })
        | IcedEvent::Mouse(mouse::Event::ButtonPressed(_) | mouse::Event::WheelScrolled { .. })
        | IcedEvent::Touch(touch::Event::FingerPressed { .. }) => Some(Message::Input),
        IcedEvent::Window(
            iced::window::Event::Opened { size, .. } | iced::window::Event::Resized(size),
        ) => Some(Message::Width(size.width)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn typed(s: &str) -> Typed {
        Typed(s.to_owned())
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
        app.on_reply(
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
            app.on_event(Event::Stopped { panicked: true });
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
        app.on_reply(
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
        app.on_reply(
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
        app.on_reply(Purpose::ConfirmSend, Reply::Sent(sent.clone()));
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
        app.on_reply(Purpose::ConfirmSend, Reply::Sent(sent.clone()));
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

    #[test]
    fn a_lock_while_signing_keeps_the_sent_page_until_its_store_is_unlocked() {
        // Lock is pressed while the spend waits on the node. The worker does
        // not stop a spend it has started: the spend answers, then the lock.
        let (mut app, sent) = just_sent();
        wallet(&mut app, WalletMsg::Report(ReportKey::Sent, Level::Full));
        app.on_event(Event::Locked {
            reason: LockReason::Asked,
        });
        assert_eq!(app.model.wallet, None);
        match &app.model.screen {
            Screen::Unlock(u) => {
                assert_eq!(u.dir, sent.view.dir.display().to_string());
                assert!(
                    u.note
                        .as_deref()
                        .is_some_and(|n| n.contains("signed spend")),
                    "{:?}",
                    u.note
                );
            }
            other => panic!("expected the unlock screen, got {other:?}"),
        }
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
        app.on_reply(Purpose::Unlock, Reply::Unlocked(other));
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
        assert_eq!(app.model.signed.len(), 1);
        assert_eq!(app.model.unsaved_signed().count(), 0);

        // Its own store unlocked: the page as it was left, with the bytes
        // and what was typed for them.
        app.on_reply(Purpose::Unlock, Reply::Unlocked(sent.view.clone()));
        let (form, s) = sent_shown(&app).expect("the sent page");
        assert_eq!(*form, spend_typed());
        assert_eq!(s.sent.artifact_hex, sent.artifact_hex);
        assert_eq!(s.sent.view, sent.view);
        assert_eq!(
            wallet_page(&app).open.get(&ReportKey::Sent),
            Some(&Level::Full)
        );
        assert!(app.model.signed.is_empty());

        // Left for another page and then locked: kept and offered, not
        // brought back by the unlock.
        wallet(&mut app, WalletMsg::Open(To::Dashboard));
        app.on_event(Event::Locked {
            reason: LockReason::Idle,
        });
        assert!(matches!(&app.model.screen, Screen::Unlock(u)
            if u.note.as_deref().is_some_and(|n| !n.contains("signed spend"))));
        app.on_reply(Purpose::Unlock, Reply::Unlocked(sent.view.clone()));
        assert!(matches!(wallet_page(&app).page, Page::Dashboard));
        assert_eq!(app.model.unsaved_signed().count(), 1);
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

        // Re-signed, the same bytes again: kept once, as the newest page.
        app.on_reply(Purpose::Resign, Reply::Sent(sent.clone()));
        assert!(sent_shown(&app).is_some_and(|(_, s)| s.resigned));
        let other = app.model.wallet.as_ref().unwrap().accounts[1].id;
        wallet(&mut app, WalletMsg::Open(To::Resign(other)));
        assert_eq!(app.model.signed.len(), 1);
        assert!(app.model.signed[0].sent.resigned);
        assert!(matches!(&wallet_page(&app).page,
            Page::Resign(ResignPage { form, .. }) if *form == SpendForm::default()));

        // Settling that finds the spend not landed keeps it; a check that
        // finds the account in sync forgets it.
        app.on_reply(
            Purpose::Settle,
            Reply::Settled {
                text: "not landed".to_owned(),
                view: sent.view.clone(),
            },
        );
        assert_eq!(app.model.signed.len(), 1);
        app.on_reply(
            Purpose::Status,
            Reply::Status {
                account: from,
                state: tawara_wallet_core::view::AccountState::InSync { balance: 5 },
                spendable: true,
                text: "in sync".to_owned(),
            },
        );
        assert!(app.model.signed.is_empty());
        wallet(&mut app, WalletMsg::Open(To::Resign(from)));
        assert!(matches!(&wallet_page(&app).page,
            Page::Resign(ResignPage { form, .. }) if *form == SpendForm::default()));
    }

    #[test]
    fn a_stopped_worker_leaves_the_signed_spends_to_save() {
        let scratch = Scratch::new("stopped-spend");
        let (mut app, sent) = just_sent();
        app.on_event(Event::Stopped { panicked: true });
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
        app.on_reply(
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
        app.on_reply(Purpose::PlanSend, Reply::Planned(plan));
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
        app.on_reply(Purpose::PlanSend, Reply::Planned(plan.clone()));
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
            app.on_reply(
                Purpose::Status,
                Reply::Status {
                    account: accounts[n].id,
                    state,
                    spendable,
                    text: "checked".to_owned(),
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
        app.on_reply(
            Purpose::Activity,
            Reply::Activity(Ok(tawara_wallet_core::sample::activity())),
        );
        assert!(!app.model.activity.reading);
        assert!(matches!(&app.model.activity.last, Some(Ok(h)) if h.len() == 3));
        app.on_reply(
            Purpose::Blocks,
            Reply::Blocks(Ok(tawara_wallet_core::sample::blocks())),
        );
        assert!(matches!(&app.model.blocks.last, Some(Ok(b)) if b.tip == 871_173));

        // A refusal that is not the node's answer (a cancel) leaves what
        // was shown.
        app.model.activity.reading = true;
        app.on_reply(
            Purpose::Activity,
            Reply::Refused(Refusal {
                kind: RefusalKind::Cancelled,
                text: "cancelled".to_owned(),
            }),
        );
        assert!(!app.model.activity.reading);
        assert!(matches!(&app.model.activity.last, Some(Ok(_))));

        // A node with no index says so, for every account.
        app.on_reply(
            Purpose::Activity,
            Reply::Activity(Err(tawara_wallet_core::sample::index_refusal(
                IndexState::Absent,
            ))),
        );
        assert!(
            matches!(&app.model.activity.last, Some(Err(r)) if r.index == Some(IndexState::Absent))
        );

        // Another node: another index and another chain.
        app.on_reply(
            Purpose::SetNode { form: true },
            Reply::NodeSet {
                url: "https://other.example".to_owned(),
                view: None,
            },
        );
        assert!(app.model.activity.last.is_none() && app.model.blocks.last.is_none());

        // The store locked: what the index held for it goes with it.
        app.on_reply(
            Purpose::Activity,
            Reply::Activity(Ok(tawara_wallet_core::sample::activity())),
        );
        app.on_event(Event::Locked {
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

        app.on_reply(
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
        app.on_reply(Purpose::Networks, Reply::Networks(vec![mainnet.clone()]));
        assert_eq!(app.model.node.network, Some(mainnet));
        app.on_reply(Purpose::Mempool, Reply::Mempool(Ok(sample::mempool())));
        assert!(!app.model.mempool.reading);
        assert!(matches!(&app.model.mempool.last, Some(Ok(m)) if m.waiting == 14));

        // A node that stops answering: no tip, so no solve time or sync
        // state either. The network it serves is still the one it said.
        app.on_reply(
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
        app.on_reply(
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
        app.on_reply(
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
        app.on_reply(Purpose::OlderActivity, Reply::Activity(Ok(vec![page])));
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

        app.on_reply(Purpose::Review, Reply::Reviewed(reports.clone()));
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
        app.on_reply(
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
        app.on_reply(
            Purpose::Status,
            Reply::Status {
                account: paused,
                state: tawara_wallet_core::view::AccountState::InSync { balance: 1 },
                spendable: false,
                text: "found".to_owned(),
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
        app.on_reply(Purpose::Review, Reply::Reviewed(reports.clone()));
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
}
