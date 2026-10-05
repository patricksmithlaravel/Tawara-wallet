//! The application: its state ([`Model`]), its messages ([`Message`]), how
//! a message changes the state ([`App::update`]), and the bridge from
//! wallet-core's worker into iced.
//!
//! Every library call goes through the worker (docs/PLAN.md section 3); the
//! application sends it [`Command`]s and turns its [`Event`]s into
//! messages. The views are functions of the [`Model`] alone
//! (`crate::screens`), so the screenshot example can draw any screen from
//! sample data without a worker (docs/DECISIONS.md D27, item 1).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iced::futures::channel::mpsc;
use iced::{Element, Event as IcedEvent, Subscription, Task, event, keyboard, mouse, touch};
use tawara_wallet_core::location::{self, Environment, Platform};
use tawara_wallet_core::preferences::{self, Preferences};
use tawara_wallet_core::view::WalletView;
use tawara_wallet_core::{
    Activity, Command, Config, Event, HttpsNode, LockReason, PhraseForDisplay, Progress, Refusal,
    Reply, RequestId, SecretText, WorkerHandle,
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
    /// What the worker is doing for a command the screen waits on.
    pub busy: Option<Busy>,
    pub node: NodeState,
    /// The open store, when one is.
    pub wallet: Option<WalletView>,
    /// The worker stopped: `Some(true)` when it panicked. Nothing more can
    /// be done in this run.
    pub stopped: Option<bool>,
}

/// What the worker is doing for the command the screen waits on.
#[derive(Clone, Debug)]
pub struct Busy {
    pub id: RequestId,
    pub activity: Activity,
    pub progress: Option<Progress>,
}

/// The node, as the application last saw it.
#[derive(Clone, Debug, Default)]
pub struct NodeState {
    /// The node chosen, when one is.
    pub url: Option<String>,
    /// The chain tip it last reported, and how long it took to answer.
    pub tip: Option<(u64, Duration)>,
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
    let dir = dir.trim();
    if dir.is_empty() {
        return None;
    }
    location::sync_warning(
        std::path::Path::new(dir),
        Platform::current(),
        &Environment::from_process(),
    )
    .map(|w| w.to_string())
}

/// S4 and S5: the phrase, and the words typed to confirm it.
#[derive(Debug)]
pub struct PhraseState {
    pub phrase: PhraseForDisplay,
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

/// The wallet's pages.
#[derive(Clone, Debug, Default)]
pub struct WalletPage {
    /// A refusal the last command on this page met, whole.
    pub error: Option<String>,
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
            busy: None,
            wallet: None,
            stopped: None,
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
        let remembered = self
            .prefs
            .store
            .clone()
            .filter(|s| tawara_wallet_core::store_exists(std::path::Path::new(s)));
        remembered.or_else(|| {
            self.default_dir
                .as_deref()
                .filter(|d| tawara_wallet_core::store_exists(d))
                .map(|d| d.display().to_string())
        })
    }

    /// Leave the current screen, zeroizing whatever secret it held.
    fn leave(&mut self) -> Screen {
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
        let (mut app, events) = App::start(Model::new(prefs, default_dir), prefs_path);
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
        (app, Task::run(rx, Message::Worker))
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
        let worker = self.worker.as_mut()?;
        match worker.handle.send(command) {
            Ok(id) => {
                worker.waiting.push((id, purpose));
                Some(id)
            }
            Err(_) => {
                self.model.stopped.get_or_insert(false);
                None
            }
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
            Message::Input => {
                if let Some(w) = &self.worker {
                    w.handle.touch();
                }
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
                    let command = Command::CreateBegin {
                        dir: PathBuf::from(f.dir.trim()),
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
                let dir = self.model.default_dir_text();
                let _ = self.model.leave();
                self.model.screen = Screen::NewWallet(PasswordForm::in_dir(dir));
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
                        dir: PathBuf::from(r.form.dir.trim()),
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
                        dir: PathBuf::from(u.dir.trim()),
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
                if let Screen::Wallet(p) = &mut m.screen {
                    p.error = None;
                }
                self.send(Command::Refresh, Purpose::Refresh);
                self.ask_tip();
            }
        }
        Task::none()
    }

    fn ask_tip(&mut self) {
        if self.model.node.url.is_some() {
            self.send(
                Command::NetworkStatus,
                Purpose::Network {
                    sent: Instant::now(),
                },
            );
        }
    }

    fn on_event(&mut self, event: Event) {
        match event {
            Event::Busy { id, activity } => {
                if self.waits_visibly(id) {
                    self.model.busy = Some(Busy {
                        id,
                        activity,
                        progress: None,
                    });
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
                let note = match reason {
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
            Event::Stopped { panicked } => {
                self.model.stopped = Some(panicked);
                self.model.wallet = None;
                self.worker = None;
            }
        }
    }

    /// Whether the screen shows the worker's activity for request `id`.
    fn waits_visibly(&self, id: RequestId) -> bool {
        self.worker.as_ref().is_some_and(|w| {
            w.waiting.iter().any(|(i, p)| {
                *i == id
                    && matches!(
                        p,
                        Purpose::CreateConfirm
                            | Purpose::CreateFromPhrase
                            | Purpose::Unlock
                            | Purpose::Refresh
                    )
            })
        })
    }

    fn on_reply(&mut self, purpose: Purpose, reply: Reply) {
        let m = &mut self.model;
        match (purpose, reply) {
            (Purpose::SetNode { form }, Reply::NodeSet { url, view }) => {
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
            }
            (Purpose::SetNode { form: true }, Reply::Refused(r)) => {
                if let Screen::Node(f) = &mut m.screen {
                    f.error = Some(r.text);
                }
            }
            (Purpose::SetNode { form: false }, Reply::Refused(r)) => {
                // A remembered node this version will not use: forget it, and
                // say why where the node is shown.
                m.node = NodeState {
                    url: None,
                    tip: None,
                    error: Some(r.text),
                };
                m.prefs.node = None;
                self.save_prefs();
            }
            (Purpose::ClearNode, Reply::NodeCleared { view }) => {
                m.node = NodeState::default();
                m.prefs.node = None;
                if let Some(v) = view {
                    m.wallet = Some(v);
                }
                if let Screen::Node(f) = &mut m.screen {
                    f.url.clear();
                }
                self.save_prefs();
            }
            (Purpose::Network { sent }, Reply::Network { tip_index, .. }) => {
                m.node.tip = Some((tip_index, sent.elapsed()));
                m.node.error = None;
            }
            (Purpose::Network { .. }, Reply::Refused(r)) => {
                m.node.tip = None;
                m.node.error = Some(r.text);
            }
            (
                Purpose::CreateBegin,
                Reply::CreatePhrase {
                    phrase,
                    confirm_positions,
                },
            ) => {
                let _ = m.leave();
                m.screen = Screen::Phrase(PhraseState {
                    phrase,
                    positions: confirm_positions,
                    written: false,
                    words: Default::default(),
                    error: None,
                });
            }
            (Purpose::CreateConfirm | Purpose::CreateFromPhrase, Reply::Created { opened, .. }) => {
                match opened {
                    Ok(view) => self.opened(view),
                    // The store was written; it could not be opened. Unlocking
                    // shows why, and nothing was lost.
                    Err(r) => self.model.unlock_screen(None, Some(r.text), None),
                }
            }
            (Purpose::Unlock, Reply::Unlocked(view)) | (Purpose::Refresh, Reply::Wallet(view)) => {
                self.opened(view);
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
        let m = &mut self.model;
        let _ = m.leave();
        // Remembered, so the next start offers to unlock this store wherever
        // it is (docs/DECISIONS.md D27, item 10).
        let dir = view.dir.display().to_string();
        let remember = m.prefs.store.as_deref() != Some(dir.as_str());
        if remember {
            m.prefs.store = Some(dir);
        }
        m.wallet = Some(view);
        m.screen = Screen::Wallet(WalletPage::default());
        if remember {
            self.save_prefs();
        }
        self.ask_tip();
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

/// The person did something: a key, a click, a scroll, a touch. Pointer
/// movement alone is not counted.
fn person_input(event: IcedEvent, _: event::Status, _: iced::window::Id) -> Option<Message> {
    match event {
        IcedEvent::Keyboard(keyboard::Event::KeyPressed { .. })
        | IcedEvent::Mouse(mouse::Event::ButtonPressed(_) | mouse::Event::WheelScrolled { .. })
        | IcedEvent::Touch(touch::Event::FingerPressed { .. }) => Some(Message::Input),
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
        pump(&mut app, &events, |a| {
            matches!(a.model.screen, Screen::Wallet(_))
        });
        let remembered = elsewhere.display().to_string();
        assert_eq!(app.model.prefs.store.as_deref(), Some(remembered.as_str()));

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
    fn a_secret_typed_is_never_in_a_message_log() {
        let message = Message::Password(typed("hunter2-hunter2"));
        assert!(!format!("{message:?}").contains("hunter2"));
    }
}
