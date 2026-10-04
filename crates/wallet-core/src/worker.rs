//! The worker: one thread that owns the store and the wallet, takes
//! [`Command`]s and sends [`Event`]s (docs/PLAN.md section 3).
//!
//! # Sessions
//!
//! The worker is in one of three states:
//!
//! - **Locked.** No store is open. Nothing secret is held, except a phrase
//!   waiting for its confirmation while a store is being created.
//! - **Store.** A `Keystore` is open: its master seed is in memory and its
//!   lock is held, but the wallet did not open, so nothing can be spent. The
//!   store's accounts and destinations can be shown, and the pre-gate
//!   operations run here (status, the acknowledged advance, restore,
//!   discovery). This is where a store waits when no node is chosen, when
//!   the node does not answer, when none of its accounts is on the ledger
//!   yet (a store just created and not yet paid), and when the library
//!   refused to open the wallet because every account diverged.
//! - **Wallet.** `Wallet::open` reconciled every account: the ones the node
//!   confirmed can spend, the ones it could not explain are set aside and
//!   refused by name (the library's I4).
//!
//! # Why the worker checks before it opens the wallet
//!
//! `Wallet::open` takes the store by value and, when it refuses, drops it:
//! the store is closed and reopening it needs the password, which the worker
//! keeps only for the length of the command that brought it. So before
//! opening, the worker asks the node about each account's tag, which is the
//! first request `Wallet::open` makes for each anyway. If the node does not
//! answer, or answers "account not found" for every tag, the wallet cannot
//! open, and the store stays open in the Store state instead of being
//! handed to a constructor that would refuse it and drop it. The one
//! refusal this does not foresee, every account on the ledger and every one
//! diverged, still drops the store: when the password is at hand (unlock,
//! create) the store is reopened with it, and otherwise the worker locks and
//! says why ([`LockReason::WalletRefused`]).
//!
//! # Lifecycle (docs/PLAN.md section 4.1)
//!
//! Locking drops the session: the `Keystore` (its key, its master seed, its
//! lock), the worker's copy of the master seed, any pending phrase and any
//! pending plan. It happens on [`Command::Lock`], when nothing has been done
//! for the idle period, when the app moves to the background
//! ([`WorkerHandle::background`]), when the worker shuts down, and when the
//! last handle is dropped.
//!
//! # Cancellation
//!
//! [`WorkerHandle::cancel`] asks every command sent so far to stop. The
//! handle records the newest request id it has given out, and a command
//! reads that record through the library's `recon::Cancel`, so a cancel
//! raised after a command finished cannot stop the next one: the next one
//! has a later id. It is read in the operations that can stop: a refresh
//! with the wallet open (between accounts, and inside the library's scan of
//! each) and a status read with the wallet open. The library's other walks
//! (`Wallet::open`, restore, the acknowledged advance before the gate,
//! discovery) take no `Cancel` today and run to their end; docs/DECISIONS.md
//! D23 proposes the library change. A spend is never stopped between its
//! reservation and its submission.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use mochimo_crypto::account::AccountKind as LibraryKind;
use mochimo_crypto::addr::Tag;
use mochimo_crypto::cli::create::{self, CONFIRM_POSITIONS, CreateEntropy};
use mochimo_crypto::cli::outcome::{Outcome, Shipped};
use mochimo_crypto::cli::{Code, address, discover, reconcile, restore};
use mochimo_crypto::consts::SEED_LEN;
use mochimo_crypto::keystore::{self, Disk, KeyAccess, Keystore, SALT_LEN, Unlock};
use mochimo_crypto::mesh::spend::SpendPlan;
use mochimo_crypto::mesh::{MeshClient, Transport};
use mochimo_crypto::recon::{self, Cancel, Divergence, ScanScope};
use mochimo_crypto::tx::wire::Destination;
use mochimo_crypto::wallet::Wallet;
use mochimo_crypto::{Error, Secret, mnemonic};
use zeroize::Zeroizing;

use crate::command::{Command, PlanId, RequestId};
use crate::entropy::{self, EntropyUnavailable};
use crate::event::{
    Activity, Discovered, Event, LockReason, PlanView, PlannedDestination, ReceiveView, Refusal,
    RefusalKind, Reply, SentView,
};
use crate::node::{self, Connect};
use crate::secret::{PhraseForDisplay, SecretText};
use crate::spend::{self, CheckedSpend, SpendRequest};
use crate::text;
use crate::view::{AccountId, AccountRow, AccountState, WalletView};

/// How long the wallet stays unlocked with nothing done, unless the
/// interface sets another period. Five minutes, as the dashboard rendering
/// shows ("auto-locks in 5 min"); docs/DECISIONS.md D24.
pub const DEFAULT_IDLE_LOCK: Duration = Duration::from_secs(5 * 60);

/// How the worker runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Lock after this long with nothing done.
    pub idle_lock: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            idle_lock: DEFAULT_IDLE_LOCK,
        }
    }
}

/// The worker has stopped; nothing more will be done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerStopped;

impl core::fmt::Display for WorkerStopped {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("the wallet worker has stopped")
    }
}

impl std::error::Error for WorkerStopped {}

enum Envelope {
    Command(RequestId, Command),
    Touch,
    Background,
    Shutdown,
}

/// The interface's side of the worker. Cloneable; when the last clone is
/// dropped the worker locks and stops.
#[derive(Clone)]
pub struct WorkerHandle {
    tx: Sender<Envelope>,
    /// The newest request id to stop (0 for none); see the module doc.
    cancel: Arc<AtomicU64>,
    next: Arc<AtomicU64>,
}

impl WorkerHandle {
    /// Queue a command. Its answer is the [`Event::Done`] with the id
    /// returned here.
    pub fn send(&self, command: Command) -> Result<RequestId, WorkerStopped> {
        let id = RequestId(self.next.fetch_add(1, Ordering::Relaxed));
        self.tx
            .send(Envelope::Command(id, command))
            .map_err(|_| WorkerStopped)?;
        Ok(id)
    }

    /// The person did something: restart the idle period. No answer.
    pub fn touch(&self) {
        let _ = self.tx.send(Envelope::Touch);
    }

    /// Ask every command sent so far to stop, where it can be stopped (see
    /// the module doc); one that was stopped answers with a refusal of kind
    /// [`RefusalKind::Cancelled`]. Commands sent afterwards are not
    /// affected.
    pub fn cancel(&self) {
        let newest = self.next.load(Ordering::Relaxed).saturating_sub(1);
        self.cancel.fetch_max(newest, Ordering::Relaxed);
    }

    /// The app moved to the background: stop what can be stopped, then lock
    /// (docs/PLAN.md section 4.1). Answered by [`Event::Locked`] with
    /// [`LockReason::Background`] when a store was open.
    pub fn background(&self) {
        self.cancel();
        let _ = self.tx.send(Envelope::Background);
    }

    /// Lock and stop the worker. [`Event::Stopped`] follows.
    pub fn shutdown(&self) {
        self.cancel();
        let _ = self.tx.send(Envelope::Shutdown);
    }
}

/// Start the worker. Events arrive on the returned receiver, in order.
pub fn spawn<C: Connect>(
    config: Config,
    connect: C,
) -> std::io::Result<(WorkerHandle, Receiver<Event>)> {
    let (tx, rx) = mpsc::channel();
    let (events, received) = mpsc::channel();
    let cancel = Arc::new(AtomicU64::new(0));
    let flag = Arc::clone(&cancel);
    thread::Builder::new()
        .name("tawara-wallet-core".into())
        .spawn(move || {
            // Declared first, dropped last: `Stopped` goes out after the
            // worker, and every secret it held, has been dropped -- on a
            // panic too, since the release profile unwinds.
            let _stopped = StopGuard(events.clone());
            let mut worker = Worker {
                connect,
                node: None,
                session: Session::Locked,
                create: None,
                plan: None,
                events,
                cancel: flag,
                idle: config.idle_lock,
                last_activity: Instant::now(),
                next_plan: 1,
            };
            worker.run(&rx);
        })?;
    Ok((
        WorkerHandle {
            tx,
            cancel,
            next: Arc::new(AtomicU64::new(1)),
        },
        received,
    ))
}

struct StopGuard(Sender<Event>);

impl Drop for StopGuard {
    fn drop(&mut self) {
        let _ = self.0.send(Event::Stopped {
            panicked: thread::panicking(),
        });
    }
}

/// An open store taken out of its session to be written.
struct TakenStore<T: Transport> {
    dir: PathBuf,
    store: Keystore<Disk>,
    master: Option<Secret<SEED_LEN>>,
    client: MeshClient<T>,
}

/// A store being created: everything `CreateConfirm` needs to write it.
struct PendingCreate {
    dir: PathBuf,
    password: SecretText,
    phrase: mnemonic::Phrase,
    entropy: Zeroizing<CreateEntropy>,
}

/// A spend laid out and waiting for confirmation.
struct PendingPlan {
    id: PlanId,
    from: Tag,
    plan: SpendPlan,
}

enum Session<T: Transport> {
    Locked,
    Store(StoreSession<T>),
    Wallet(WalletSession<T>),
}

struct StoreSession<T: Transport> {
    dir: PathBuf,
    store: Keystore<Disk>,
    master: Option<Secret<SEED_LEN>>,
    /// The node it was last reconciled against, when one was asked.
    client: Option<(String, MeshClient<T>)>,
    rows: Vec<AccountRow>,
    notice: Option<String>,
}

struct WalletSession<T: Transport> {
    dir: PathBuf,
    node: String,
    wallet: Wallet<Disk, T>,
    /// A copy of the store's master seed. `KeyAccess` borrows the seed for
    /// each call while `reserve_and_sign` and `settle_if_landed` borrow the
    /// wallet mutably, so the copy is what lets both borrows exist; the
    /// command line makes the same copy (`cli::decide`). It is a `Secret`
    /// and is dropped with the session.
    master: Option<Secret<SEED_LEN>>,
    rows: Vec<AccountRow>,
}

struct Worker<C: Connect> {
    connect: C,
    node: Option<String>,
    session: Session<C::Transport>,
    create: Option<PendingCreate>,
    plan: Option<PendingPlan>,
    events: Sender<Event>,
    cancel: Arc<AtomicU64>,
    idle: Duration,
    last_activity: Instant,
    next_plan: u64,
}

fn refused(kind: RefusalKind, text: impl Into<String>) -> Reply {
    Reply::Refused(Refusal {
        kind,
        text: text.into(),
    })
}

fn library(e: Error) -> Refusal {
    Refusal {
        kind: if matches!(e, Error::Cancelled) {
            RefusalKind::Cancelled
        } else {
            RefusalKind::Library
        },
        text: text::refusal(e),
    }
}

fn entropy_refusal(e: EntropyUnavailable) -> Refusal {
    Refusal {
        kind: RefusalKind::Entropy,
        text: e.to_string(),
    }
}

/// The library's promise for every refusal of `create` before the write:
/// its own helper's rule (`cli::create::nothing_was_created`, private
/// there), so a refusal arriving in another's words ends the same way.
fn nothing_was_created(text: impl core::fmt::Display) -> String {
    let text = text.to_string();
    let text = text.trim_end();
    if text.ends_with(['.', '!', '?']) {
        format!("{text} Nothing was created.")
    } else {
        format!("{text}. Nothing was created.")
    }
}

/// Whether the library refused a directory for its owner, mode or access
/// list, or for being a symbolic link. The access-list variants exist only
/// in a Windows build.
fn unsafe_directory(e: &Error) -> bool {
    #[cfg(windows)]
    if matches!(e, Error::UnsafeAcl { .. } | Error::UnsafeFileAcl { .. }) {
        return true;
    }
    matches!(
        e,
        Error::UnsafePermissions { .. } | Error::StoreDirectoryIsLink
    )
}

/// Why `Keystore::open` refused, in its words.
fn open_refusal(dir: &Path, e: Error) -> Refusal {
    let kind = match e {
        Error::Locked => RefusalKind::StoreInUse,
        Error::WrongPassword => RefusalKind::WrongPassword,
        Error::Missing => RefusalKind::NoStore,
        // No folder at all: the library reports it as the directory's
        // `stat` finding nothing, which says less than this.
        Error::Io {
            kind: std::io::ErrorKind::NotFound,
            ..
        } => {
            return Refusal {
                kind: RefusalKind::NoStore,
                text: format!(
                    "there is no keystore at {}: the folder does not exist",
                    dir.display()
                ),
            };
        }
        Error::Cancelled => RefusalKind::Cancelled,
        ref e if unsafe_directory(e) => RefusalKind::UnsafeDirectory,
        _ => RefusalKind::Library,
    };
    Refusal {
        kind,
        text: format!("cannot open the keystore at {}: {e}", dir.display()),
    }
}

/// Open the store at `dir` with a fresh nonce seed.
fn open_store(dir: &Path, password: &SecretText) -> Result<Keystore<Disk>, Refusal> {
    let nonce = entropy::nonce_seed().map_err(entropy_refusal)?;
    Keystore::open(
        dir,
        &Unlock {
            password: password.expose().as_bytes(),
            nonce_seed: *nonce,
        },
    )
    .map_err(|e| open_refusal(dir, e))
}

/// A copy of the store's master seed (see [`WalletSession::master`]).
fn master_of(store: &Keystore<Disk>) -> Result<Option<Secret<SEED_LEN>>, Refusal> {
    store
        .master()
        .map(|m| m.map(Secret::duplicate))
        .map_err(library)
}

/// The key access `tag` needs: the command line's per-account choice
/// (`cli::key_access`, private there), with its refusals in the same words.
fn key_access<'a>(
    store: &Keystore<Disk>,
    tag: &Tag,
    master: Option<&'a Secret<SEED_LEN>>,
) -> Result<KeyAccess<'a>, Error> {
    match recon::access_for(store, tag, master) {
        Ok(access) => Ok(access),
        Err(Divergence::NoMasterForDerivedAccount { .. }) => Err(Error::KeyAccessMismatch {
            kind: LibraryKind::Derived,
        }),
        Err(Divergence::CannotReconcile { cause, .. }) => Err(cause),
        Err(_) => Err(Error::ReconciliationRefused {
            what: "the store's view of this account could not choose its key access",
        }),
    }
}

/// `balance - fee`, or the refusal the command line gives when nothing is
/// left for the destination (`cli::spend_all_amount`, private there).
fn spend_all_amount(balance: u64, fee_total: u64) -> Result<u64, Error> {
    match balance.checked_sub(fee_total) {
        Some(0) | None => Err(Error::InsufficientBalance {
            balance,
            needed: fee_total.saturating_add(1),
        }),
        Some(amount) => Ok(amount),
    }
}

fn destinations(spend: &CheckedSpend, everything: u64) -> Vec<Destination> {
    spend
        .dsts
        .iter()
        .map(|d| Destination {
            tag: d.to,
            reference: d.reference,
            amount: d.amount.unwrap_or(everything),
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn reference_text(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// What the node says before the wallet is opened (module doc).
enum Precheck {
    /// At least one tag is on the ledger, or the store holds none: open.
    OnLedger,
    /// Every tag answered "account not found".
    NothingOnLedger,
    /// The node did not answer as a node.
    Unreachable(Error),
}

fn precheck<T: Transport>(store: &Keystore<Disk>, client: &MeshClient<T>) -> Precheck {
    let Ok(tags) = store.tags() else {
        // `Wallet::open` reports an unreadable store in its own words.
        return Precheck::OnLedger;
    };
    if tags.is_empty() {
        return Precheck::OnLedger;
    }
    for tag in &tags {
        match client.resolve_tag(tag) {
            Ok(_) => return Precheck::OnLedger,
            Err(Error::Mesh { code: 4, .. }) => {}
            Err(e) => return Precheck::Unreachable(e),
        }
    }
    Precheck::NothingOnLedger
}

/// The store's accounts with no wallet open: from the records, with each
/// account's status read now when a node is at hand, and the divergences
/// those reads found.
fn store_rows<T: Transport>(
    store: &Keystore<Disk>,
    client: Option<&MeshClient<T>>,
    master: Option<&Secret<SEED_LEN>>,
) -> Result<(Vec<AccountRow>, Vec<Divergence>), Refusal> {
    let held = address::accounts_in(store).map_err(library)?;
    let mut rows = Vec::with_capacity(held.len());
    let mut diverged = Vec::new();
    for h in held {
        let state = match client {
            None => AccountState::NotReconciled,
            Some(c) => match reconcile::account_status(store, c, &h.tag, master, None) {
                Ok(status) => AccountState::from_status(&status),
                Err(d) => {
                    let state = AccountState::from_divergence(&d);
                    diverged.push(d);
                    state
                }
            },
        };
        rows.push(AccountRow {
            id: AccountId::from_tag(h.tag),
            kind: h.kind.into(),
            index: h.index.get(),
            state,
            spendable: false,
        });
    }
    Ok((rows, diverged))
}

/// The store's accounts with the wallet open: the partition it opened with.
fn wallet_rows<T: Transport>(wallet: &Wallet<Disk, T>) -> Result<Vec<AccountRow>, Refusal> {
    let held = address::accounts_in(wallet.store()).map_err(library)?;
    Ok(held
        .into_iter()
        .map(|h| {
            let (state, spendable) =
                if let Some((_, status)) = wallet.accounts().iter().find(|(t, _)| *t == h.tag) {
                    let state = AccountState::from_status(status);
                    let spendable = matches!(state, AccountState::InSync { .. });
                    (state, spendable)
                } else if let Some(d) = wallet.divergence_for(&h.tag) {
                    (AccountState::from_divergence(d), false)
                } else {
                    (AccountState::NotReconciled, false)
                };
            AccountRow {
                id: AccountId::from_tag(h.tag),
                kind: h.kind.into(),
                index: h.index.get(),
                state,
                spendable,
            }
        })
        .collect())
}

/// Replace one account's row after an operation on it.
fn update_row(
    rows: &mut [AccountRow],
    tag: &Tag,
    state: AccountState,
    spendable: bool,
    index: Option<u32>,
) {
    if let Some(row) = rows.iter_mut().find(|r| r.id.tag() == *tag) {
        row.state = state;
        row.spendable = spendable;
        if let Some(i) = index {
            row.index = i;
        }
    }
}

const NO_NODE: &str = "No node is chosen, so nothing was reconciled and nothing can be sent. \
                       The store is open: its accounts and their destinations can be shown. \
                       Choose a node to reconcile them.";

impl<C: Connect> Worker<C> {
    fn emit(&self, event: Event) {
        // A dropped receiver means nobody is listening; the worker carries
        // on until its handles go.
        let _ = self.events.send(event);
    }

    fn busy(&self, id: RequestId, activity: Activity) {
        self.emit(Event::Busy { id, activity });
    }

    /// Whether request `id` has been asked to stop (module doc), for the
    /// library's `Cancel`.
    fn stop_asked(&self, id: RequestId) -> impl Fn() -> bool + 'static {
        let flag = Arc::clone(&self.cancel);
        move || flag.load(Ordering::Relaxed) >= id.0
    }

    fn holds_secrets(&self) -> bool {
        !matches!(self.session, Session::Locked) || self.create.is_some()
    }

    fn run(&mut self, rx: &Receiver<Envelope>) {
        loop {
            let envelope = if self.holds_secrets() {
                let left = self.idle.saturating_sub(self.last_activity.elapsed());
                match rx.recv_timeout(left) {
                    Ok(e) => e,
                    Err(RecvTimeoutError::Timeout) => {
                        if self.last_activity.elapsed() >= self.idle {
                            self.lock(LockReason::Idle);
                        }
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => Envelope::Shutdown,
                }
            } else {
                rx.recv().unwrap_or(Envelope::Shutdown)
            };
            match envelope {
                Envelope::Command(id, command) => {
                    self.last_activity = Instant::now();
                    let reply = self.handle(id, command);
                    self.emit(Event::Done { id, reply });
                    self.last_activity = Instant::now();
                }
                Envelope::Touch => self.last_activity = Instant::now(),
                Envelope::Background => self.lock(LockReason::Background),
                Envelope::Shutdown => {
                    self.lock(LockReason::Shutdown);
                    return;
                }
            }
        }
    }

    /// Drop the session and everything pending. `Locked` is reported when a
    /// store was open.
    fn lock(&mut self, reason: LockReason) {
        self.create = None;
        self.plan = None;
        if let Session::Locked = core::mem::replace(&mut self.session, Session::Locked) {
            return;
        }
        self.emit(Event::Locked { reason });
    }

    fn view(&self) -> Option<WalletView> {
        match &self.session {
            Session::Locked => None,
            Session::Store(s) => Some(WalletView {
                dir: s.dir.clone(),
                accounts: s.rows.clone(),
                opened: false,
                notice: s.notice.clone(),
            }),
            Session::Wallet(w) => Some(WalletView {
                dir: w.dir.clone(),
                accounts: w.rows.clone(),
                opened: true,
                notice: text::standing_notice(w.wallet.diverged()),
            }),
        }
    }

    fn handle(&mut self, id: RequestId, command: Command) -> Reply {
        match command {
            Command::SetNode { url } => self.set_node(&url),
            Command::ClearNode => {
                self.node = None;
                Reply::NodeCleared
            }
            Command::CreateBegin {
                dir,
                password,
                password_again,
            } => self.create_begin(dir, password, &password_again),
            Command::CreateConfirm { answer } => self.create_confirm(id, &answer),
            Command::CreateAbandon => {
                self.create = None;
                Reply::CreateAbandoned
            }
            Command::CreateFromPhrase {
                dir,
                password,
                password_again,
                phrase,
            } => self.create_from_phrase(id, dir, &password, &password_again, &phrase),
            Command::Unlock { dir, password } => match self.unlock(id, dir, &password) {
                Ok(view) => Reply::Unlocked(view),
                Err(r) => Reply::Refused(r),
            },
            Command::Lock => {
                self.lock(LockReason::Asked);
                Reply::Locked
            }
            Command::Refresh => self.refresh(id),
            Command::Receive { account } => self.receive(account),
            Command::PlanSend { spend } => self.plan_send(id, &spend),
            Command::ConfirmSend { plan } => self.confirm_send(id, plan),
            Command::DiscardPlan => {
                self.plan = None;
                Reply::PlanDiscarded
            }
            Command::Settle { account } => self.settle(id, account),
            Command::Resign { spend } => self.resign(id, &spend),
            Command::SubmitArtifact { artifact_hex } => self.submit_artifact(id, &artifact_hex),
            Command::Status { account, scan_to } => self.status(id, account, scan_to),
            Command::Reconcile {
                account,
                advance_to,
            } => self.reconcile(id, account, advance_to),
            Command::Restore {
                account_index,
                scan_to,
            } => self.restore(id, account_index, scan_to),
            Command::Discover { to } => self.discover(id, to),
            Command::NetworkStatus => self.network_status(id),
        }
    }

    fn set_node(&mut self, url: &str) -> Reply {
        match node::check_node_url(&self.connect, url) {
            Ok(_) => {
                let url = url.trim().to_owned();
                self.node = Some(url.clone());
                Reply::NodeSet { url }
            }
            Err(e) => refused(RefusalKind::NodeRefused, e.to_string()),
        }
    }

    /// A client for the chosen node.
    fn client(&self) -> Result<(String, MeshClient<C::Transport>), Refusal> {
        let Some(url) = self.node.clone() else {
            return Err(Refusal {
                kind: RefusalKind::NoNode,
                text: "no node is chosen; choose one to ask the chain".into(),
            });
        };
        let transport = node::check_node_url(&self.connect, &url).map_err(|e| Refusal {
            kind: RefusalKind::NodeRefused,
            text: e.to_string(),
        })?;
        Ok((url, MeshClient::new(transport)))
    }

    // ----------------------------------------------------------------- create

    /// The checks `create` makes before it shows a phrase, in its order: an
    /// occupied directory, then the password floor, then the two passwords
    /// agreeing.
    fn create_checks(dir: &Path, password: &SecretText, again: &SecretText) -> Result<(), Refusal> {
        if let Some(what) = keystore::occupied(dir) {
            return Err(Refusal {
                kind: RefusalKind::Occupied,
                text: format!(
                    "a keystore already exists at {} (its {what} is present). Nothing was shown \
                     and nothing was changed. Choose another folder for a new store, or open this \
                     one.",
                    dir.display()
                ),
            });
        }
        if let Some(why) = create::password_refusal(password.expose()) {
            return Err(Refusal {
                kind: RefusalKind::PasswordTooShort,
                text: why,
            });
        }
        if password.expose().as_bytes() != again.expose().as_bytes() {
            return Err(Refusal {
                kind: RefusalKind::PasswordsDiffer,
                text: "those two passwords are not the same. Nothing was created.".into(),
            });
        }
        Ok(())
    }

    fn create_begin(&mut self, dir: PathBuf, password: SecretText, again: &SecretText) -> Reply {
        self.create = None;
        if let Err(r) = Self::create_checks(&dir, &password, again) {
            return Reply::Refused(r);
        }
        let entropy = match entropy::create_entropy() {
            Ok(e) => e,
            Err(e) => return Reply::Refused(entropy_refusal(e)),
        };
        let phrase = match mnemonic::phrase_from_entropy(&entropy.phrase) {
            Ok(p) => p,
            Err(e) => return refused(RefusalKind::Library, nothing_was_created(e)),
        };
        let display = PhraseForDisplay::new(phrase.expose());
        self.create = Some(PendingCreate {
            dir,
            password,
            phrase,
            entropy,
        });
        Reply::CreatePhrase {
            phrase: display,
            confirm_positions: CONFIRM_POSITIONS,
        }
    }

    fn create_refusal(e: Error) -> Refusal {
        let kind = match e {
            Error::PasswordTooShort { .. } => RefusalKind::PasswordTooShort,
            Error::Exists { .. } => RefusalKind::Occupied,
            Error::Locked => RefusalKind::StoreInUse,
            ref e if unsafe_directory(e) => RefusalKind::UnsafeDirectory,
            _ => RefusalKind::Library,
        };
        Refusal {
            kind,
            text: nothing_was_created(e),
        }
    }

    fn create_confirm(&mut self, id: RequestId, answer: &SecretText) -> Reply {
        let Some(pending) = self.create.as_ref() else {
            return refused(
                RefusalKind::NothingToConfirm,
                "there is no recovery phrase waiting to be confirmed. Nothing was created.",
            );
        };
        if !create::confirmation_matches(pending.phrase.expose(), answer.expose()) {
            let p = CONFIRM_POSITIONS;
            return refused(
                RefusalKind::ConfirmationWrong,
                format!(
                    "those are not words {}, {} and {}. Nothing was created.\n\n  The phrase \
                     belongs to no store yet. Check the words against it and answer again, or \
                     start over for a fresh phrase -- and either way, write the phrase down \
                     before anything is sent to the wallet.",
                    p[0], p[1], p[2]
                ),
            );
        }
        let Some(pending) = self.create.take() else {
            return refused(RefusalKind::NothingToConfirm, "Nothing was created.");
        };
        self.busy(id, Activity::DerivingKey);
        let created = match create::create(
            &pending.dir,
            pending.phrase.expose(),
            pending.password.expose(),
            pending.entropy.salt,
            pending.entropy.nonce_seed,
        ) {
            Ok(c) => c,
            Err(e) => return Reply::Refused(Self::create_refusal(e)),
        };
        let first = AccountId::from_tag(created.tag);
        let opened = self.unlock(id, pending.dir.clone(), &pending.password);
        Reply::Created {
            dir: pending.dir,
            first,
            opened,
        }
    }

    fn create_from_phrase(
        &mut self,
        id: RequestId,
        dir: PathBuf,
        password: &SecretText,
        again: &SecretText,
        phrase: &SecretText,
    ) -> Reply {
        self.create = None;
        if let Err(r) = Self::create_checks(&dir, password, again) {
            return Reply::Refused(r);
        }
        let salt = match entropy::os_bytes::<SALT_LEN>() {
            Ok(s) => s,
            Err(e) => return Reply::Refused(entropy_refusal(e)),
        };
        let nonce = match entropy::nonce_seed() {
            Ok(n) => n,
            Err(e) => return Reply::Refused(entropy_refusal(e)),
        };
        self.busy(id, Activity::DerivingKey);
        let created = match create::create(&dir, phrase.expose(), password.expose(), *salt, *nonce)
        {
            Ok(c) => c,
            Err(e) => return Reply::Refused(Self::create_refusal(e)),
        };
        let first = AccountId::from_tag(created.tag);
        let opened = self.unlock(id, dir.clone(), password);
        Reply::Created { dir, first, opened }
    }

    // ----------------------------------------------------------------- unlock

    fn unlock(
        &mut self,
        id: RequestId,
        dir: PathBuf,
        password: &SecretText,
    ) -> Result<WalletView, Refusal> {
        self.lock(LockReason::Replaced);
        self.busy(id, Activity::DerivingKey);
        let store = open_store(&dir, password)?;
        let master = master_of(&store)?;
        self.open_session(id, dir, store, master, Some(password))
    }

    /// Make an open store a session: the wallet when it can open, the store
    /// otherwise (module doc). `password` is at hand only inside the command
    /// that brought it, and lets a store the library dropped be reopened.
    fn open_session(
        &mut self,
        id: RequestId,
        dir: PathBuf,
        store: Keystore<Disk>,
        master: Option<Secret<SEED_LEN>>,
        password: Option<&SecretText>,
    ) -> Result<WalletView, Refusal> {
        let (url, client) = match self.client() {
            Ok(c) => c,
            Err(r) => {
                let (rows, _) = store_rows::<C::Transport>(&store, None, master.as_ref())?;
                let notice = if r.kind == RefusalKind::NoNode {
                    NO_NODE.to_owned()
                } else {
                    r.text
                };
                self.session = Session::Store(StoreSession {
                    dir,
                    store,
                    master,
                    client: None,
                    rows,
                    notice: Some(notice),
                });
                return self.view().ok_or_else(Self::lost);
            }
        };
        self.busy(id, Activity::AskingNode);
        match precheck(&store, &client) {
            Precheck::Unreachable(e) => {
                let (rows, _) = store_rows::<C::Transport>(&store, None, master.as_ref())?;
                self.session = Session::Store(StoreSession {
                    dir,
                    store,
                    master,
                    client: Some((url, client)),
                    rows,
                    notice: Some(format!(
                        "The node did not answer, so nothing was reconciled and nothing can be \
                         sent. The store is open: its accounts and their destinations can be \
                         shown.\n\n{}",
                        text::refusal(e)
                    )),
                });
                self.view().ok_or_else(Self::lost)
            }
            Precheck::NothingOnLedger => {
                let (rows, diverged) = store_rows(&store, Some(&client), master.as_ref())?;
                self.session = Session::Store(StoreSession {
                    dir,
                    store,
                    master,
                    client: Some((url, client)),
                    rows,
                    notice: text::standing_notice(&diverged),
                });
                self.view().ok_or_else(Self::lost)
            }
            Precheck::OnLedger => match Wallet::open(store, client, master.as_ref()) {
                Ok(wallet) => {
                    let rows = wallet_rows(&wallet)?;
                    self.session = Session::Wallet(WalletSession {
                        dir,
                        node: url,
                        wallet,
                        master,
                        rows,
                    });
                    self.view().ok_or_else(Self::lost)
                }
                Err(refusal) => {
                    let text = text::page(&[], Outcome::StartupRefused(Box::new(refusal.clone())));
                    let Some(password) = password else {
                        self.session = Session::Locked;
                        self.emit(Event::Locked {
                            reason: LockReason::WalletRefused,
                        });
                        return Err(Refusal {
                            kind: RefusalKind::WalletRefused,
                            text,
                        });
                    };
                    self.busy(id, Activity::DerivingKey);
                    let store = open_store(&dir, password)?;
                    let master = master_of(&store)?;
                    let (mut rows, _) = store_rows::<C::Transport>(&store, None, master.as_ref())?;
                    for d in &refusal.diverged {
                        update_row(
                            &mut rows,
                            &d.tag(),
                            AccountState::from_divergence(d),
                            false,
                            None,
                        );
                    }
                    let client = self.client().ok();
                    self.session = Session::Store(StoreSession {
                        dir,
                        store,
                        master,
                        client,
                        rows,
                        notice: Some(text),
                    });
                    self.view().ok_or_else(Self::lost)
                }
            },
        }
    }

    fn lost() -> Refusal {
        Refusal {
            kind: RefusalKind::NotUnlocked,
            text: "the store is not open".into(),
        }
    }

    fn not_unlocked() -> Reply {
        refused(
            RefusalKind::NotUnlocked,
            "the wallet is locked; unlock it first",
        )
    }

    fn not_open() -> Reply {
        refused(
            RefusalKind::WalletNotOpen,
            "the wallet did not open against the node, so nothing can be sent from it; its \
             notice says why",
        )
    }

    // ---------------------------------------------------------------- refresh

    fn refresh(&mut self, id: RequestId) -> Reply {
        let node_moved = match &self.session {
            Session::Wallet(w) => self.node.as_deref() != Some(w.node.as_str()),
            _ => false,
        };
        match core::mem::replace(&mut self.session, Session::Locked) {
            Session::Locked => Self::not_unlocked(),
            Session::Wallet(w) if !node_moved => {
                self.session = Session::Wallet(w);
                self.refresh_wallet(id)
            }
            Session::Wallet(w) => {
                let (store, _old) = w.wallet.into_parts();
                match self.open_session(id, w.dir, store, w.master, None) {
                    Ok(view) => Reply::Wallet(view),
                    Err(r) => Reply::Refused(r),
                }
            }
            Session::Store(s) => match self.open_session(id, s.dir, s.store, s.master, None) {
                Ok(view) => Reply::Wallet(view),
                Err(r) => Reply::Refused(r),
            },
        }
    }

    /// Each account's status read fresh, through the open wallet's client;
    /// the partition stays the one `Wallet::open` made, so an account set
    /// aside then stays refused for spending until the wallet is reopened.
    fn refresh_wallet(&mut self, id: RequestId) -> Reply {
        self.busy(id, Activity::AskingNode);
        let asked = self.stop_asked(id);
        let Session::Wallet(w) = &mut self.session else {
            return Self::not_unlocked();
        };
        let tags: Vec<Tag> = w.rows.iter().map(|r| r.id.tag()).collect();
        let mut fresh = Vec::with_capacity(tags.len());
        for tag in &tags {
            // Each account is a round trip or more; the rows change only
            // when every one was read, so a stop leaves them as they were.
            if asked() {
                return Reply::Refused(library(Error::Cancelled));
            }
            let reconciled_at_open = w.wallet.accounts().iter().any(|(t, _)| t == tag);
            let state = match key_access(w.wallet.store(), tag, w.master.as_ref()) {
                Err(e) => AccountState::Diverged {
                    report: text::refusal(e),
                    advance_to: None,
                },
                Ok(access) => match w.wallet.status_with(
                    tag,
                    &access,
                    &ScanScope::DIAGNOSTIC,
                    &Cancel::when(&asked),
                ) {
                    Ok(status) => AccountState::from_status(&status),
                    Err(Divergence::CannotReconcile {
                        cause: Error::Cancelled,
                        ..
                    }) => return Reply::Refused(library(Error::Cancelled)),
                    Err(d) => AccountState::from_divergence(&d),
                },
            };
            let spendable = reconciled_at_open && matches!(state, AccountState::InSync { .. });
            fresh.push((*tag, state, spendable));
        }
        for (tag, state, spendable) in fresh {
            let index = w
                .wallet
                .store()
                .view(&tag)
                .ok()
                .flatten()
                .map(|v| v.wots_index.get());
            update_row(&mut w.rows, &tag, state, spendable, index);
        }
        self.view().map_or_else(Self::not_unlocked, Reply::Wallet)
    }

    // ---------------------------------------------------------------- receive

    fn receive(&self, account: AccountId) -> Reply {
        let tag = account.tag();
        let (store, master) = match &self.session {
            Session::Locked => return Self::not_unlocked(),
            Session::Store(s) => (&s.store, s.master.as_ref()),
            Session::Wallet(w) => (w.wallet.store(), w.master.as_ref()),
        };
        let found = key_access(store, &tag, master)
            .and_then(|access| address::address_of(store, &tag, &access));
        match found {
            Ok(place) => Reply::Receive(ReceiveView {
                account,
                destination: account.destination(),
                index: place.index.get(),
                ledger_address: hex(&place.address),
                text: text::page(
                    &[],
                    Outcome::Address {
                        tag,
                        address: place.address,
                        index: place.index,
                    },
                ),
            }),
            Err(e) => Reply::Refused(library(e)),
        }
    }

    // ------------------------------------------------------------------ spend

    fn diverged_refusal(d: &Divergence) -> Reply {
        refused(
            RefusalKind::Diverged,
            text::page(&[], Outcome::Diverged(Box::new(d.clone()))),
        )
    }

    fn plan_send(&mut self, id: RequestId, request: &SpendRequest) -> Reply {
        self.plan = None;
        if let Some(reply) = self.needs_wallet() {
            return reply;
        }
        let Session::Wallet(w) = &self.session else {
            return Self::not_open();
        };
        let checked = match spend::check(request) {
            Ok(c) => c,
            Err(e) => return refused(RefusalKind::Spend(e.clone()), e.to_string()),
        };
        let tag = checked.from;
        if let Some(d) = w.wallet.divergence_for(&tag) {
            return Self::diverged_refusal(d);
        }
        self.busy(id, Activity::AskingNode);
        let planned = key_access(w.wallet.store(), &tag, w.master.as_ref()).and_then(|access| {
            if checked.spends_everything() {
                // One ledger read for both the amount and the plan, as the
                // command line does it (`cli::plan_spend`), so "everything"
                // cannot fail to empty the account because the balance moved
                // between two reads.
                let addresses = w.wallet.spend_addresses(&tag, &access)?;
                let entry = w.wallet.client().resolve_tag(&tag)?;
                let amount = spend_all_amount(entry.balance, checked.fee_total)?;
                SpendPlan::new(
                    &addresses,
                    &entry,
                    destinations(&checked, amount),
                    checked.fee_total,
                    checked.blk_to_live,
                )
            } else {
                w.wallet.plan(
                    &tag,
                    &access,
                    destinations(&checked, 0),
                    checked.fee_total,
                    checked.blk_to_live,
                )
            }
        });
        let plan = match planned {
            Ok(p) => p,
            Err(e) => return Reply::Refused(library(e)),
        };
        // Every destination must render before a key can be spent on this
        // plan, as the command line requires (`cli::cmd_send`).
        let mut shown = Vec::with_capacity(checked.dsts.len());
        for d in &checked.dsts {
            let Some(destination) = AccountId::from_tag(d.to).destination() else {
                return refused(
                    RefusalKind::Library,
                    format!(
                        "cannot render the destination for the tag whose hex is {}; nothing was \
                         reserved",
                        hex(&d.to)
                    ),
                );
            };
            let amount = plan
                .dsts()
                .iter()
                .find(|p| p.tag == d.to && p.reference == d.reference)
                .map_or(0, |p| p.amount);
            shown.push(PlannedDestination {
                to: AccountId::from_tag(d.to),
                destination,
                amount,
                reference: reference_text(&d.reference),
            });
        }
        if AccountId::from_tag(tag).destination().is_none() {
            return refused(
                RefusalKind::Library,
                "cannot render this account's destination; nothing was reserved",
            );
        }
        let plan_id = PlanId(self.next_plan);
        self.next_plan += 1;
        let view = PlanView {
            plan: plan_id,
            from: AccountId::from_tag(tag),
            destinations: shown,
            send_total: plan.send_total(),
            fee_total: plan.fee_total(),
            change_total: plan.change_total(),
            balance: plan.balance(),
            blk_to_live: plan.blk_to_live(),
            empties_account: plan.change_total() == 0,
        };
        self.plan = Some(PendingPlan {
            id: plan_id,
            from: tag,
            plan,
        });
        Reply::Planned(view)
    }

    fn confirm_send(&mut self, id: RequestId, plan_id: PlanId) -> Reply {
        if let Some(reply) = self.needs_wallet() {
            self.plan = None;
            return reply;
        }
        let Some(pending) = self.plan.take() else {
            return refused(
                RefusalKind::NoSuchPlan,
                "no spend is waiting to be confirmed; plan it again",
            );
        };
        if pending.id != plan_id {
            return refused(
                RefusalKind::NoSuchPlan,
                "that is not the spend waiting to be confirmed; plan it again",
            );
        }
        self.busy(id, Activity::AskingNode);
        let Session::Wallet(w) = &mut self.session else {
            return Self::not_open();
        };
        let tag = pending.from;
        let plan = pending.plan;
        let master = w.master.as_ref();
        let signed = match key_access(w.wallet.store(), &tag, master) {
            Ok(access) => w.wallet.reserve_and_sign(&plan, access),
            Err(e) => Err(e),
        };
        let signed = match signed {
            Ok(s) => s,
            Err(e) => return Reply::Refused(library(e)),
        };
        let wire = signed.wire();
        let submitted = w.wallet.submit(&signed);
        let tx_id = submitted.as_ref().ok().map(|id| hex(&id.0));
        let shipped = Shipped {
            source: tag,
            destinations: plan.dsts().to_vec(),
            blk_to_live: plan.blk_to_live(),
            wire: wire.clone(),
            submitted: submitted.clone(),
        };
        let page = text::page(
            w.wallet.diverged(),
            Outcome::Sent {
                shipped,
                send_total: plan.send_total(),
                fee_total: plan.fee_total(),
                change_total: plan.change_total(),
                upgraded: w.wallet.store().upgraded_from(),
            },
        );
        self.refresh_one(&tag);
        let Some(view) = self.view() else {
            return Self::not_unlocked();
        };
        Reply::Sent(SentView {
            from: AccountId::from_tag(tag),
            submitted: submitted.is_ok(),
            tx_id,
            artifact_hex: hex(&wire),
            text: page,
            view,
        })
    }

    /// Read one account's status again after an operation on it.
    fn refresh_one(&mut self, tag: &Tag) {
        let Session::Wallet(w) = &mut self.session else {
            return;
        };
        let reconciled_at_open = w.wallet.accounts().iter().any(|(t, _)| t == tag);
        let state = match key_access(w.wallet.store(), tag, w.master.as_ref()) {
            Err(e) => AccountState::Diverged {
                report: text::refusal(e),
                advance_to: None,
            },
            Ok(access) => match w.wallet.status(tag, &access) {
                Ok(status) => AccountState::from_status(&status),
                Err(d) => AccountState::from_divergence(&d),
            },
        };
        let spendable = reconciled_at_open && matches!(state, AccountState::InSync { .. });
        let index = w
            .wallet
            .store()
            .view(tag)
            .ok()
            .flatten()
            .map(|v| v.wots_index.get());
        update_row(&mut w.rows, tag, state, spendable, index);
    }

    /// The refusal for a command that needs the wallet open, when it is not.
    fn needs_wallet(&self) -> Option<Reply> {
        match self.session {
            Session::Wallet(_) => None,
            Session::Locked => Some(Self::not_unlocked()),
            Session::Store(_) => Some(Self::not_open()),
        }
    }

    fn settle(&mut self, id: RequestId, account: AccountId) -> Reply {
        let tag = account.tag();
        if let Some(reply) = self.needs_wallet() {
            return reply;
        }
        self.busy(id, Activity::AskingNode);
        let Session::Wallet(w) = &mut self.session else {
            return Self::not_open();
        };
        if let Some(d) = w.wallet.divergence_for(&tag) {
            return Self::diverged_refusal(d);
        }
        let settled = match key_access(w.wallet.store(), &tag, w.master.as_ref()) {
            Ok(access) => w.wallet.settle_if_landed(&tag, &access),
            Err(e) => Err(e),
        };
        let page = match settled {
            Ok(settlement) => text::page(
                w.wallet.diverged(),
                Outcome::Settled {
                    settlement,
                    upgraded: w.wallet.store().upgraded_from(),
                },
            ),
            Err(e) => return Reply::Refused(library(e)),
        };
        self.refresh_one(&tag);
        match self.view() {
            Some(view) => Reply::Settled { text: page, view },
            None => Self::not_unlocked(),
        }
    }

    fn resign(&mut self, id: RequestId, request: &SpendRequest) -> Reply {
        if let Some(reply) = self.needs_wallet() {
            return reply;
        }
        let checked = match spend::check(request) {
            Ok(c) => c,
            Err(e) => return refused(RefusalKind::Spend(e.clone()), e.to_string()),
        };
        self.busy(id, Activity::AskingNode);
        let Session::Wallet(w) = &mut self.session else {
            return Self::not_open();
        };
        let tag = checked.from;
        if let Some(d) = w.wallet.divergence_for(&tag) {
            return Self::diverged_refusal(d);
        }
        let master = w.master.as_ref();
        let access = match key_access(w.wallet.store(), &tag, master) {
            Ok(a) => a,
            Err(e) => return Reply::Refused(library(e)),
        };
        // "Everything" resolves against the balance now; if it moved, the
        // digest differs and the library refuses it as another spend
        // (`cli::resign_destinations`).
        let dsts = if checked.spends_everything() {
            match w
                .wallet
                .client()
                .resolve_tag(&tag)
                .and_then(|entry| spend_all_amount(entry.balance, checked.fee_total))
            {
                Ok(amount) => destinations(&checked, amount),
                Err(e) => return Reply::Refused(library(e)),
            }
        } else {
            destinations(&checked, 0)
        };
        let mut listed = dsts.clone();
        listed.sort_by_key(Destination::mdst_image);
        let resigned =
            w.wallet
                .resign_pending(&tag, &access, dsts, checked.fee_total, checked.blk_to_live);
        let (outcome, wire, submitted) = match resigned {
            Ok(signed) => {
                let wire = signed.wire();
                if AccountId::from_tag(tag).destination().is_none() {
                    // The reproduction goes out to the person either way:
                    // it may be the only rendering of the only bytes that can
                    // move those funds.
                    let cause = mochimo_crypto::addr::tag_to_base58(&tag)
                        .err()
                        .unwrap_or(Error::NoSuchAccount);
                    (
                        Outcome::ReproducedButUnrenderable {
                            source: tag,
                            destinations: listed,
                            blk_to_live: checked.blk_to_live,
                            wire: wire.clone(),
                            cause,
                        },
                        Some(wire),
                        None,
                    )
                } else {
                    let submitted = w.wallet.submit(&signed);
                    (
                        Outcome::Resigned {
                            shipped: Shipped {
                                source: tag,
                                destinations: listed,
                                blk_to_live: checked.blk_to_live,
                                wire: wire.clone(),
                                submitted: submitted.clone(),
                            },
                        },
                        Some(wire),
                        Some(submitted),
                    )
                }
            }
            Err(Error::DigestMismatch) => (Outcome::NotTheReservedSpend, None, None),
            Err(Error::ReservationLanded {
                spent_index,
                settled_index,
            }) => (
                Outcome::ReservationAlreadyLanded {
                    source: tag,
                    spent_index,
                    settled_index,
                },
                None,
                None,
            ),
            Err(e) => return Reply::Refused(library(e)),
        };
        let page = text::page(w.wallet.diverged(), outcome);
        let Some(wire) = wire else {
            return refused(RefusalKind::Library, page);
        };
        self.refresh_one(&tag);
        let Some(view) = self.view() else {
            return Self::not_unlocked();
        };
        let tx_id = submitted
            .as_ref()
            .and_then(|s| s.as_ref().ok())
            .map(|id| hex(&id.0));
        Reply::Sent(SentView {
            from: AccountId::from_tag(tag),
            submitted: matches!(submitted, Some(Ok(_))),
            tx_id,
            artifact_hex: hex(&wire),
            text: page,
            view,
        })
    }

    fn submit_artifact(&self, id: RequestId, artifact_hex: &str) -> Reply {
        let (_, client) = match self.client() {
            Ok(c) => c,
            Err(r) => return Reply::Refused(r),
        };
        self.busy(id, Activity::AskingNode);
        let report = mochimo_crypto::cli::run_submit(&client, artifact_hex.trim());
        Reply::Submitted {
            accepted: report.code == Code::Ok,
            text: report.text,
        }
    }

    // ------------------------------------------------------------- pre-gate

    fn status(&mut self, id: RequestId, account: AccountId, scan_to: Option<u32>) -> Reply {
        let tag = account.tag();
        let scope = match scan_to {
            Some(m) => ScanScope::DIAGNOSTIC.with_ceiling(m.saturating_add(1)),
            None => ScanScope::DIAGNOSTIC,
        };
        let asked = self.stop_asked(id);
        let result = match &self.session {
            Session::Locked => return Self::not_unlocked(),
            Session::Store(s) => {
                let Some((_, client)) = &s.client else {
                    return refused(
                        RefusalKind::NoNode,
                        "no node was asked; choose one and refresh first",
                    );
                };
                self.busy(id, Activity::AskingNode);
                reconcile::account_status(&s.store, client, &tag, s.master.as_ref(), scan_to)
            }
            Session::Wallet(w) => {
                self.busy(id, Activity::AskingNode);
                match key_access(w.wallet.store(), &tag, w.master.as_ref()) {
                    Ok(access) => {
                        w.wallet
                            .status_with(&tag, &access, &scope, &Cancel::when(&asked))
                    }
                    Err(e) => Err(Divergence::CannotReconcile { tag, cause: e }),
                }
            }
        };
        // The command line's classification (`cli::cmd_status`): a report
        // about the account is an answer, not a refusal.
        let (state, outcome) = match result {
            Ok(status) => (
                AccountState::from_status(&status),
                Outcome::Status { tag, status },
            ),
            Err(Divergence::CannotReconcile {
                cause: Error::Cancelled,
                ..
            }) => return Reply::Refused(library(Error::Cancelled)),
            Err(Divergence::CannotReconcile {
                cause: Error::NoSuchAccount,
                ..
            }) => {
                return refused(
                    RefusalKind::Library,
                    text::page(&[], Outcome::NoSuchAccount { tag }),
                );
            }
            Err(
                d @ (Divergence::IndexMismatch { .. }
                | Divergence::ReservationUnexplained { .. }
                | Divergence::TagUnresolved { .. }),
            ) => (
                AccountState::from_divergence(&d),
                Outcome::StatusDiverged {
                    tag,
                    divergence: Box::new(d),
                },
            ),
            Err(d) => (
                AccountState::from_divergence(&d),
                Outcome::StatusRefused {
                    divergence: Box::new(d),
                },
            ),
        };
        match &mut self.session {
            Session::Store(s) => update_row(&mut s.rows, &tag, state.clone(), false, None),
            Session::Wallet(w) => {
                let spendable = w.wallet.accounts().iter().any(|(t, _)| *t == tag)
                    && matches!(state, AccountState::InSync { .. });
                update_row(&mut w.rows, &tag, state.clone(), spendable, None);
            }
            Session::Locked => {}
        }
        Reply::Status {
            account,
            state,
            text: text::page(&[], outcome),
        }
    }

    /// Take the open store out of the session for an operation that writes
    /// it, with the node it needs. The session is left Locked; the caller
    /// reopens it.
    fn take_store(&mut self) -> Result<TakenStore<C::Transport>, Refusal> {
        if matches!(self.session, Session::Locked) {
            return Err(Self::lost());
        }
        let (_, client) = self.client()?;
        self.plan = None;
        match core::mem::replace(&mut self.session, Session::Locked) {
            Session::Locked => Err(Self::lost()),
            Session::Store(s) => Ok(TakenStore {
                dir: s.dir,
                store: s.store,
                master: s.master,
                client,
            }),
            Session::Wallet(w) => {
                let (store, _old) = w.wallet.into_parts();
                Ok(TakenStore {
                    dir: w.dir,
                    store,
                    master: w.master,
                    client,
                })
            }
        }
    }

    fn reconcile(&mut self, id: RequestId, account: AccountId, advance_to: u32) -> Reply {
        let tag = account.tag();
        let TakenStore {
            dir,
            mut store,
            master,
            client,
        } = match self.take_store() {
            Ok(parts) => parts,
            Err(r) => return Reply::Refused(r),
        };
        self.busy(id, Activity::AskingNode);
        let reviewed =
            reconcile::advance_acknowledged(&mut store, &client, &tag, master.as_ref(), advance_to);
        let (advanced_to, page) = match reviewed {
            Ok(reviewed) => {
                let advanced = match &reviewed.outcome {
                    reconcile::Outcome::Advanced { index } => Some(*index),
                    _ => None,
                };
                let upgraded = store.upgraded_from();
                (
                    advanced,
                    text::page(
                        &[],
                        Outcome::Reconciled {
                            tag,
                            advance_to,
                            reviewed,
                            upgraded,
                        },
                    ),
                )
            }
            Err(e) => (None, text::refusal(e)),
        };
        let view = self.open_session(id, dir, store, master, None).ok();
        Reply::Reconciled {
            advanced_to,
            text: page,
            view,
        }
    }

    fn restore(&mut self, id: RequestId, account_index: u32, scan_to: Option<u32>) -> Reply {
        let TakenStore {
            dir,
            mut store,
            master,
            client,
        } = match self.take_store() {
            Ok(parts) => parts,
            Err(r) => return Reply::Refused(r),
        };
        self.busy(id, Activity::AskingNode);
        let outcome = match master.as_ref() {
            None => Outcome::RestoreNeedsMaster,
            Some(m) => {
                match restore::restore_account(&mut store, &client, m, account_index, scan_to) {
                    Ok(r) => Outcome::Restored {
                        account: account_index,
                        found: r.found,
                        held_at: r.held_at,
                        upgraded: store.upgraded_from(),
                    },
                    Err(failure) => Outcome::RestoreRefused {
                        account: account_index,
                        failure,
                    },
                }
            }
        };
        let page = text::page(&[], outcome);
        let view = self.open_session(id, dir, store, master, None).ok();
        Reply::Restored { text: page, view }
    }

    fn discover(&mut self, id: RequestId, to: u32) -> Reply {
        let to = to.min(crate::DISCOVER_MAX_TO);
        let fresh;
        let (store, master, client) = match &self.session {
            Session::Locked => return Self::not_unlocked(),
            Session::Store(s) => {
                let client = match &s.client {
                    Some((_, c)) => c,
                    None => match self.client() {
                        Ok((_, c)) => {
                            fresh = c;
                            &fresh
                        }
                        Err(r) => return Reply::Refused(r),
                    },
                };
                (&s.store, s.master.as_ref(), client)
            }
            Session::Wallet(w) => (w.wallet.store(), w.master.as_ref(), w.wallet.client()),
        };
        let Some(master) = master else {
            return refused(
                RefusalKind::Library,
                text::page(&[], Outcome::DiscoverNeedsMaster),
            );
        };
        self.busy(id, Activity::AskingNode);
        match discover::sweep(store, client, master, to) {
            Ok(sweep) => {
                let accounts = sweep
                    .sightings
                    .iter()
                    .map(|s| Discovered {
                        account_index: s.account,
                        id: AccountId::from_tag(s.tag),
                        ledger_balance: s.entry.map(|e| e.balance),
                        held: s.held.is_some(),
                    })
                    .collect();
                Reply::Discovered {
                    text: text::page(&[], Outcome::Discovered { sweep }),
                    accounts,
                }
            }
            Err(discover::SweepFailure::ChainUnreachable {
                account,
                searched,
                cause,
            }) => refused(
                RefusalKind::Library,
                text::page(
                    &[],
                    Outcome::SweepStopped {
                        account,
                        searched,
                        to,
                        cause,
                    },
                ),
            ),
        }
    }

    fn network_status(&self, id: RequestId) -> Reply {
        let tip = match &self.session {
            Session::Wallet(w) if self.node.as_deref() == Some(w.node.as_str()) => {
                self.busy(id, Activity::AskingNode);
                w.wallet.client().network_status()
            }
            _ => match self.client() {
                Ok((_, client)) => {
                    self.busy(id, Activity::AskingNode);
                    client.network_status()
                }
                Err(r) => return Reply::Refused(r),
            },
        };
        match tip {
            Ok(tip) => Reply::Network {
                tip_index: tip.index,
                tip_hash: hex(&tip.hash),
            },
            Err(e) => Reply::Refused(library(e)),
        }
    }
}

/// Write a signed spend's bytes to `path` as hex, the form
/// [`Command::SubmitArtifact`] and the command line's `submit` take, so they
/// are not lost (docs/PLAN.md section 4.9: the retry artifact can be lost,
/// so the app offers to save it as a file). Refuses to overwrite a file
/// that exists.
pub fn save_artifact(path: &Path, artifact_hex: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(artifact_hex.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_was_created_keeps_the_librarys_punctuation_rule() {
        assert_eq!(nothing_was_created("no."), "no. Nothing was created.");
        assert_eq!(nothing_was_created("no "), "no. Nothing was created.");
        assert_eq!(nothing_was_created("why?"), "why? Nothing was created.");
    }

    #[test]
    fn spend_all_refuses_when_nothing_is_left() {
        assert_eq!(spend_all_amount(1000, 500), Ok(500));
        assert!(matches!(
            spend_all_amount(500, 500),
            Err(Error::InsufficientBalance {
                balance: 500,
                needed: 501
            })
        ));
        assert!(matches!(
            spend_all_amount(10, 500),
            Err(Error::InsufficientBalance { .. })
        ));
    }

    #[test]
    fn reference_text_stops_at_the_first_nul() {
        let mut field = [0u8; 16];
        field[..3].copy_from_slice(b"ABC");
        assert_eq!(reference_text(&field), "ABC");
        assert_eq!(reference_text(&[0u8; 16]), "");
    }

    #[test]
    fn save_artifact_never_overwrites() {
        let path = std::env::temp_dir().join(format!("tawara-artifact-{}.hex", std::process::id()));
        let _ = std::fs::remove_file(&path);
        save_artifact(&path, "abcd").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "abcd\n");
        assert_eq!(
            save_artifact(&path, "ef").unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        let _ = std::fs::remove_file(&path);
    }
}
