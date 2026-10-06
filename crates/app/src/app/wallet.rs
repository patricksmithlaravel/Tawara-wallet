//! The wallet's pages (docs/SCREENS.md W1 to W12, and the explorer's, E1 to
//! E4): what each holds, the messages that change it, and what the worker's
//! answers do to it.
//!
//! Nothing here is secret. Destinations, amounts, the library's pages and a
//! signed spend's bytes are what the node is sent or what the store shows;
//! the store's key never leaves the worker.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use iced::Task;
use tawara_wallet_core::explorer::{
    BlockAt, BlockDetail, ExplorerRefusal, Found, PendingView, QUEUE_ROWS, Query, TagView,
    TransactionView,
};
use tawara_wallet_core::preferences::{AmountUnit, IDLE_LOCK_MINUTES};
use tawara_wallet_core::spend::{Amount, DestinationInput, SpendRequest};
use tawara_wallet_core::view::{AccountId, AccountState, DivergenceKind, WalletView};
use tawara_wallet_core::{AccountReport, amount};

use crate::history::{Filter, RowId};
use tawara_wallet_core::{
    Command, Discovered, MAX_DESTINATIONS, PlanView, ReceiveView, Reply, SentView,
};

use super::{App, Message, Model, Purpose, Screen, WalletPage};

/// Which wallet page is shown.
#[derive(Clone, Debug, Default)]
pub enum Page {
    /// W1.
    #[default]
    Dashboard,
    /// W2.
    Receive(ReceivePage),
    /// W3.
    AddAccount(AddAccountPage),
    /// W4 to W6.
    Send(SendPage),
    /// W7.
    Account(AccountPage),
    /// W8.
    Resign(ResignPage),
    /// W9.
    Submit(SubmitPage),
    /// W10.
    Activity(ActivityPage),
    /// W11.
    Settings(SettingsPage),
    /// W12.
    Recovery(RecoveryPage),
    /// E1.
    Explorer(ExplorerPage),
    /// E2.
    Block(BlockPage),
    /// E3.
    Tag(TagPage),
    /// E4.
    Transaction(TransactionPage),
    /// E1's "View all pending": the node's whole queue.
    Queue(QueuePage),
}

/// The node's whole queue, read whole up to the command line's most.
#[derive(Clone, Debug, Default)]
pub struct QueuePage {
    /// The node's answer, once it has come.
    pub read: Option<Result<PendingView, ExplorerRefusal>>,
}

/// E1: the chain, the node's queue, and the search field.
#[derive(Clone, Debug, Default)]
pub struct ExplorerPage {
    /// What is typed in the search field.
    pub search: String,
    /// Why what was typed is none of the things the explorer finds.
    pub invalid: Option<String>,
    /// The hash being looked for. Only its answer is taken: one for an
    /// earlier search, asked before the explorer was opened again, is not.
    pub finding: Option<[u8; 32]>,
    /// What the last hash looked for named, when it was nothing.
    pub missed: Option<Box<Missed>>,
}

/// A hash that named nothing: the index's answer for a transaction with
/// that id, and the node's for a block with that hash.
#[derive(Clone, Debug)]
pub struct Missed {
    /// The index answered, and holds no such transaction.
    pub searched: bool,
    pub transaction: ExplorerRefusal,
    pub block: ExplorerRefusal,
}

/// How many of a block's transactions its page lists at a time.
pub const BLOCK_ROWS: usize = 10;

/// E2: one block.
#[derive(Clone, Debug)]
pub struct BlockPage {
    pub at: BlockAt,
    /// The node's answer, once it has come.
    pub read: Option<Result<Box<BlockDetail>, ExplorerRefusal>>,
    /// The first of its transactions the page lists, [`BLOCK_ROWS`] at a
    /// time.
    pub rows_from: usize,
}

/// E3: one account, by its tag.
#[derive(Clone, Debug)]
pub struct TagPage {
    pub account: AccountId,
    /// The node's answer, once it has come.
    pub read: Option<Box<TagView>>,
    /// An older page of its history is on its way.
    pub reading_older: bool,
    /// The person typed this account's address or tag into the explorer's
    /// search field. Only then is "Send to this account" offered: an
    /// account reached through a link came from the node's answers, and a
    /// node could name its own account where the person expects another's
    /// (docs/DECISIONS.md D31, item 5).
    pub typed: bool,
}

/// E4: one transaction, whole.
#[derive(Clone, Debug)]
pub struct TransactionPage {
    pub transaction: TransactionView,
    /// As the node's index lists it (its source at its gross amount, the
    /// change a destination of its own), rather than as a block or the
    /// queue lists it (its source at what left it net of the change, which
    /// is not listed).
    pub from_index: bool,
    /// The library's page for it, when it was looked up in the index; a
    /// transaction opened from its block's page or a tag's history is on
    /// that page's.
    pub text: Option<String>,
    /// The chain's tip read with the page it was opened from, when that page
    /// read one (a block's, E2). Its confirmations are counted from this or
    /// the explorer's tip, whichever is higher, so opening a transaction
    /// never counts fewer than its block's page did.
    pub tip: Option<u64>,
}

/// W10: every account's transactions from the node's index.
#[derive(Clone, Debug, Default)]
pub struct ActivityPage {
    pub filter: Filter,
    /// What is typed in the search field.
    pub search: String,
    /// The row shown beside the list; the newest when none is chosen.
    pub selected: Option<RowId>,
}

/// W11.
#[derive(Clone, Debug, Default)]
pub struct SettingsPage {
    /// The node's address as typed, before it is saved.
    pub node: String,
}

/// W12, account recovery: every account's report first, then the
/// acknowledged advance on the owner's terms (docs/DECISIONS.md D19, D29).
#[derive(Clone, Debug, Default)]
pub struct RecoveryPage {
    /// Every account's report, once the review answers. Each opens to its
    /// summary; the advance waits until every one's full output has been
    /// opened (`read`).
    pub reports: Option<Vec<AccountReport>>,
    /// The accounts whose full report has been opened since it was read.
    pub read: BTreeSet<AccountId>,
    /// The account to act on: advance it, or search further for it.
    pub target: Option<AccountId>,
    /// The key index typed for it. Nothing fills it in: the person types the
    /// index the report names, as the command line makes them.
    pub index: String,
    /// The person confirmed no other wallet uses this recovery phrase.
    pub confirmed: bool,
    /// Whether the last advance is done, and the library's page for it.
    pub result: Option<(bool, String)>,
}

impl RecoveryPage {
    /// How many accounts' full reports are still to be opened before the
    /// advance is offered; `None` before the reports have come.
    #[must_use]
    pub fn unread(&self) -> Option<usize> {
        let reports = self.reports.as_ref()?;
        Some(
            reports
                .iter()
                .filter(|a| !self.read.contains(&a.account))
                .count(),
        )
    }
}

/// What can be done for an account on the recovery page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Remedy {
    /// The acknowledged advance to the index the report names.
    Advance(u32),
    /// Its chain address was not among the keys searched: search further.
    SearchFurther,
}

/// What the recovery page offers for `state`, if anything: an advance only
/// where the library's report names the index, a further search only where
/// it found nothing in the keys searched.
#[must_use]
pub fn remedy(state: &AccountState) -> Option<Remedy> {
    match state {
        AccountState::Diverged {
            advance_to: Some(to),
            ..
        } => Some(Remedy::Advance(*to)),
        AccountState::Diverged {
            kind: DivergenceKind::Unlocated,
            ..
        } => Some(Remedy::SearchFurther),
        _ => None,
    }
}

/// W2: one account's destination.
#[derive(Clone, Debug, Default)]
pub struct ReceivePage {
    pub account: Option<AccountId>,
    /// What the worker said about it, once it has.
    pub view: Option<ReceiveView>,
}

/// W3: a discovery sweep, and adding what it found.
#[derive(Clone, Debug)]
pub struct AddAccountPage {
    /// How far to search, as typed: derived accounts `0..=to`.
    pub to: String,
    /// The library's page for the last sweep, and its rows.
    pub found: Option<(String, Vec<Discovered>)>,
    /// Whether the last account asked for was added, and the library's
    /// page for it.
    pub added: Option<(bool, String)>,
}

impl Default for AddAccountPage {
    fn default() -> AddAccountPage {
        AddAccountPage {
            to: tawara_wallet_core::DISCOVER_DEFAULT_TO.to_string(),
            found: None,
            added: None,
        }
    }
}

/// One destination as typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DestinationRow {
    pub to: String,
    /// In MCM.
    pub amount: String,
    pub reference: String,
}

/// A spend as typed: W4's form, and W8's, which must give the reserved
/// spend's figures again exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendForm {
    pub rows: Vec<DestinationRow>,
    /// Send the whole balance less the fee to the one destination.
    pub everything: bool,
    /// The fee in nanoMCM, in total; empty for the node's floor.
    pub fee: String,
    /// The block after which the spend may not be included; empty for
    /// none.
    pub blk_to_live: String,
}

impl Default for SpendForm {
    fn default() -> SpendForm {
        SpendForm {
            rows: vec![DestinationRow::default()],
            everything: false,
            fee: String::new(),
            blk_to_live: String::new(),
        }
    }
}

/// W4 to W6: composing a spend, reviewing the library's page for it, and
/// what happened once it was signed.
#[derive(Clone, Debug, Default)]
pub struct SendPage {
    pub from: Option<AccountId>,
    pub form: SpendForm,
    pub stage: SendStage,
}

/// Where a spend is.
#[derive(Clone, Debug, Default)]
pub enum SendStage {
    /// W4.
    #[default]
    Compose,
    /// W5: laid out and not signed; the worker holds the plan.
    Review(Box<PlanView>),
    /// W6: signed, and written to the node's socket or not.
    Sent(Box<SentPage>),
}

/// W6.
#[derive(Clone, Debug)]
pub struct SentPage {
    pub sent: SentView,
    /// Re-signed (W8) rather than signed for the first time.
    pub resigned: bool,
    /// Where "Save artifact" wrote the bytes, or why it could not.
    pub saved: Option<Result<PathBuf, String>>,
}

/// W7: one account, its state, and what that state allows.
#[derive(Clone, Debug)]
pub struct AccountPage {
    pub account: AccountId,
    /// What was last done here, and the library's page for it.
    pub report: Option<(Done, String)>,
}

/// What was done on an account's page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Done {
    Checked,
    Settled,
}

/// A report on a wallet page (docs/DECISIONS.md D28): each is shown as a
/// slim banner, opens into a summary in Tawara's words, and from there into
/// the wallet library's full output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReportKey {
    /// The store's notice (W1).
    Notice,
    /// A diverged account's report (W7).
    Diverged,
    /// What a check or a settlement found (W7).
    Done,
    /// The spend laid out and not signed (W5).
    Plan,
    /// The spend signed (W6).
    Sent,
    /// An account's destination (W2).
    Receive,
    /// A discovery sweep (W3).
    Found,
    /// An account added (W3).
    Added,
    /// Saved bytes submitted (W9).
    Submitted,
    /// The refusal the page's last command met.
    Refused,
    /// The node's index for an account (W10).
    History,
    /// What the index or the newest blocks could not be read for (W1, W10).
    Explorer,
    /// The n-th account's report on the recovery page (W12).
    Review(u16),
    /// What the last advance did (W12).
    Advanced,
    /// The library's page for what an explorer page shows (E1 to E4).
    Read,
    /// The node's answer for a block, when a hash named nothing (E1).
    Missed,
    /// An account the node did not resolve (E3).
    Ledger,
}

/// How far a report is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Level {
    /// The banner alone.
    #[default]
    Closed,
    /// The summary, with its common causes.
    Summary,
    /// The summary and the library's full output.
    Full,
}

/// W8: the reserved spend from one account, typed again.
#[derive(Clone, Debug)]
pub struct ResignPage {
    pub account: AccountId,
    pub form: SpendForm,
}

/// W9: signed bytes saved earlier, submitted again.
#[derive(Clone, Debug, Default)]
pub struct SubmitPage {
    pub hex: String,
    /// Whether the library counts it as written, and its page.
    pub result: Option<(bool, String)>,
}

/// Where a wallet message goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum To {
    Dashboard,
    /// W2, for this account or the first.
    Receive(Option<AccountId>),
    AddAccount,
    /// W4, from this account or the first that can spend.
    Send(Option<AccountId>),
    Account(AccountId),
    Resign(AccountId),
    Submit,
    Activity,
    Settings,
    /// W12, with this account to act on.
    Recovery(Option<AccountId>),
    /// E1.
    Explorer,
    /// E2.
    Block(BlockAt),
    /// E3.
    Tag(AccountId),
    /// E1's "View all pending".
    Queue,
}

/// What the wallet's pages react to.
#[derive(Clone, Debug)]
pub enum WalletMsg {
    Open(To),
    /// W2: show this account's destination.
    ReceiveFor(AccountId),
    /// Put this text on the clipboard: a destination, or the hex of a
    /// signed spend. Neither is secret.
    Copy(String),
    DiscoverTo(String),
    Discover,
    /// W3: add derived account N, as the sweep found it.
    Add(u32),
    From(AccountId),
    DestinationTo(usize, String),
    Amount(usize, String),
    Reference(usize, String),
    AddDestination,
    RemoveDestination(usize),
    Everything(bool),
    Fee(String),
    BlockToLive(String),
    /// W4 to W5: lay the spend out.
    Review,
    /// W5 back to W4: forget the plan, keep what was typed.
    Edit,
    /// W5: reserve the key, sign and submit.
    Sign,
    SaveArtifact,
    /// Show again the kept spend whose bytes are these (see
    /// [`Model::signed`]).
    ShowSigned(String),
    /// Save the bytes of the kept spend whose bytes are these: the stopped
    /// screen's, with no page to show them on.
    SaveSigned(String),
    Settle(AccountId),
    /// W7: reconcile the account now and report.
    Check(AccountId),
    /// W8: re-sign and submit.
    Resign,
    ArtifactHex(String),
    Submit,
    /// Open or close a report on the page.
    Report(ReportKey, Level),
    /// W10.
    Filter(Filter),
    Search(String),
    Select(RowId),
    /// W10: read the node's index again.
    ReadActivity,
    /// W10: read the next older page of every account the index holds more
    /// of.
    ReadOlderActivity,
    /// W11.
    Unit(AmountUnit),
    /// W11: the auto-lock period, in minutes.
    AutoLock(u64),
    NodeTyped(String),
    SaveNode,
    /// W11: ask the node for its tip now.
    CheckNode,
    /// W12: reconcile every account again.
    ReviewAll,
    Target(AccountId),
    RecoveryIndex(String),
    Confirm(bool),
    Advance,
    SearchFurther,
    /// E1: what is typed in the search field.
    ExplorerTyped(String),
    /// E1: find what is typed.
    ExplorerFind,
    /// E1: read the chain and the queue again.
    ReadChain,
    /// E2: list the block's transactions from this one on.
    BlockRows(usize),
    /// E4: the block page's `n`-th transaction, whole.
    OpenSpend(usize),
    /// E4: the tag page's `n`-th row, whole.
    OpenTagRow(usize),
    /// E4: the `n`-th of the queue's transactions read whole.
    OpenPending(usize),
    /// E3: read the next older page of the tag's transactions.
    ReadOlderTag,
    /// W4 from E3: the send form, with this account as its destination.
    SendTo(AccountId),
}

impl From<WalletMsg> for Message {
    fn from(m: WalletMsg) -> Message {
        Message::Wallet(m)
    }
}

impl Page {
    /// Which sidebar item it belongs to: 0 Wallet, 1 Send, 2 Receive.
    #[must_use]
    pub fn nav(&self) -> usize {
        match self {
            Page::Send(_) | Page::Resign(_) | Page::Submit(_) => 1,
            Page::Receive(_) => 2,
            Page::Activity(_) => 3,
            Page::Explorer(_)
            | Page::Block(_)
            | Page::Tag(_)
            | Page::Transaction(_)
            | Page::Queue(_) => 4,
            Page::Settings(_) | Page::Recovery(_) => 5,
            Page::Dashboard | Page::AddAccount(_) | Page::Account(_) => 0,
        }
    }
}

/// The spend `form` asks for from `from`, or why it cannot be one yet: what
/// the application can tell before the worker checks the rest by the
/// command line's rules.
pub fn spend_request(from: AccountId, form: &SpendForm) -> Result<SpendRequest, String> {
    let rows: Vec<&DestinationRow> = form
        .rows
        .iter()
        .filter(|r| **r != DestinationRow::default())
        .collect();
    if rows.is_empty() {
        return Err("Give at least one destination.".to_owned());
    }
    let everything = form.everything && rows.len() == 1;
    let mut destinations = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let amount = if everything {
            Amount::Everything
        } else {
            match amount::parse_mcm(&row.amount) {
                Ok(nano) => Amount::Nano(nano),
                Err(e) => return Err(format!("Destination {}: {e}.", i + 1)),
            }
        };
        destinations.push(DestinationInput {
            to: row.to.trim().to_owned(),
            amount,
            reference: row.reference.trim().to_owned(),
        });
    }
    let fee = match form.fee.trim() {
        "" => None,
        typed => Some(amount::parse_nano(typed).map_err(|e| format!("The fee: {e}."))?),
    };
    let blk_to_live = match form.blk_to_live.trim() {
        "" => 0,
        typed => typed.parse::<u64>().map_err(|_| {
            "The block-to-live is a block number, in digits, or empty for none.".to_owned()
        })?,
    };
    Ok(SpendRequest {
        from,
        destinations,
        fee,
        blk_to_live,
    })
}

/// Why a spend is not laid out from the account chosen: it cannot spend
/// now (`AccountRow::spendable`).
const CANNOT_SPEND: &str = "This account cannot spend now. An account spends once the wallet \
                                has reconciled it on opening and it is in sync; one set aside \
                                when the wallet opened spends again after Refresh. Choose another \
                                account, or Refresh the wallet first.";

/// The accounts a spend may be planned from now.
fn spendable(wallet: Option<&WalletView>) -> impl Iterator<Item = AccountId> + '_ {
    wallet
        .into_iter()
        .flat_map(|w| w.accounts.iter())
        .filter(|a| a.spendable)
        .map(|a| a.id)
}

/// Whether two folders are one store's, however they were written.
fn same_store(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// A signed spend whose page is not shown, kept for the rest of the run
/// while its reservation may be open (see [`Model::signed`]).
#[derive(Clone, Debug)]
pub struct Signed {
    pub sent: SentPage,
    /// What was typed for it, which re-signing it starts from (W8).
    pub form: SpendForm,
    /// Which of its page's reports were open.
    pub open: BTreeMap<ReportKey, Level>,
    /// A lock closed its page while it was shown: the page comes back as
    /// it was left once its store is unlocked.
    pub resume: bool,
    /// Its hex is what the stopped screen last copied.
    pub copied: bool,
}

impl Signed {
    /// The store it was signed from.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.sent.sent.view.dir
    }

    /// Whether its bytes are in memory only: not saved to a file.
    #[must_use]
    pub fn unsaved(&self) -> bool {
        !matches!(self.sent.saved, Some(Ok(_)))
    }

    fn into_page(self) -> WalletPage {
        WalletPage {
            page: Page::Send(SendPage {
                from: Some(self.sent.sent.from),
                form: self.form,
                stage: SendStage::Sent(Box::new(self.sent)),
            }),
            open: self.open,
            ..WalletPage::default()
        }
    }
}

impl Model {
    /// Before the screen changes: keep the signed spend's page it shows, if
    /// it shows one (see [`Model::signed`]), with `resume` when a lock
    /// closes it. Whether it did.
    pub(super) fn keep_signed(&mut self, resume: bool) -> bool {
        let Screen::Wallet(shown) = &mut self.screen else {
            return false;
        };
        let (form, mut sent, open) = match core::mem::take(shown) {
            WalletPage {
                page:
                    Page::Send(SendPage {
                        form,
                        stage: SendStage::Sent(sent),
                        ..
                    }),
                open,
                ..
            } => (form, *sent, open),
            other => {
                *shown = other;
                return false;
            }
        };
        // What the store showed goes; its folder says which store the spend
        // is from.
        sent.sent.view.accounts = Vec::new();
        sent.sent.view.notice = None;
        // A re-signed spend is the same bytes again: one spend, kept once.
        self.signed
            .retain(|s| s.sent.sent.artifact_hex != sent.sent.artifact_hex);
        self.signed.push(Signed {
            sent,
            form,
            open,
            resume,
            copied: false,
        });
        true
    }

    /// Once a store is unlocked: the signed spend's page a lock closed for
    /// it, shown again as it was left.
    pub(super) fn bring_back_signed(&mut self) {
        let Some(view) = &self.wallet else {
            return;
        };
        let Some(at) = self
            .signed
            .iter()
            .position(|s| s.resume && same_store(s.dir(), &view.dir))
        else {
            return;
        };
        let mut kept = self.signed.remove(at);
        kept.sent.sent.view = view.clone();
        self.screen = Screen::Wallet(kept.into_page());
    }

    /// Forget the kept spends whose account the open store now shows in
    /// sync: the reservation is settled, and the bytes can move nothing.
    pub(super) fn forget_settled(&mut self) {
        let Some(view) = &self.wallet else {
            return;
        };
        self.signed.retain(|s| {
            !same_store(s.dir(), &view.dir)
                || !view.accounts.iter().any(|a| {
                    a.id == s.sent.sent.from && matches!(a.state, AccountState::InSync { .. })
                })
        });
    }

    /// The kept spends from the open store whose bytes are not saved.
    pub fn unsaved_signed(&self) -> impl Iterator<Item = &Signed> {
        let dir = self.wallet.as_ref().map(|w| w.dir.as_path());
        self.signed
            .iter()
            .filter(move |s| s.unsaved() && dir.is_some_and(|d| same_store(s.dir(), d)))
    }

    /// What was typed for the newest kept spend from `account` in the open
    /// store.
    fn kept_form(&self, account: AccountId) -> Option<SpendForm> {
        let dir = &self.wallet.as_ref()?.dir;
        self.signed
            .iter()
            .rev()
            .find(|s| s.sent.sent.from == account && same_store(s.dir(), dir))
            .map(|s| s.form.clone())
    }
}

/// Write a signed spend's bytes to the Downloads folder under a name of
/// their own, or say why they were not.
fn save(downloads: Option<&Path>, sent: &SentView) -> Result<PathBuf, String> {
    match downloads {
        Some(dir) => {
            tawara_wallet_core::save_artifact_in(dir, &artifact_stem(sent), &sent.artifact_hex)
                .map_err(|e| format!("The bytes could not be saved in {}: {e}.", dir.display()))
        }
        None => Err(
            "This system names no Downloads folder, so the bytes were not saved. \
                     Copy the hex instead and keep it somewhere safe."
                .to_owned(),
        ),
    }
}

/// A name for a saved artifact: the spend's id when the node echoed one,
/// so the file can be matched to the page that shows it.
fn artifact_stem(sent: &SentView) -> String {
    match &sent.tx_id {
        Some(id) => format!("tawara-spend-{}", &id[..id.len().min(16)]),
        None => "tawara-spend-unsubmitted".to_owned(),
    }
}

impl App {
    /// The wallet page shown, when the wallet is.
    fn wallet_page(&mut self) -> Option<&mut WalletPage> {
        match &mut self.model.screen {
            Screen::Wallet(p) => Some(p),
            _ => None,
        }
    }

    /// Leave the page shown: a plan the worker holds for the review page is
    /// forgotten, so nothing waits to be signed that no screen shows; a
    /// signed spend's page is kept (see [`Model::signed`]).
    fn leave_page(&mut self) {
        let reviewing = matches!(
            self.wallet_page().map(|p| &p.page),
            Some(Page::Send(SendPage {
                stage: SendStage::Review(_),
                ..
            }))
        );
        if reviewing {
            self.send(Command::DiscardPlan, Purpose::DiscardPlan);
        }
        self.model.keep_signed(false);
    }

    /// Show again the kept spend whose bytes are `hex`, as it was left.
    fn show_signed(&mut self, hex: &str) {
        let Some(view) = self.model.wallet.clone() else {
            return;
        };
        self.leave_page();
        if let Some(at) = self
            .model
            .signed
            .iter()
            .position(|s| s.sent.sent.artifact_hex == hex && same_store(s.dir(), &view.dir))
        {
            let mut kept = self.model.signed.remove(at);
            kept.sent.sent.view = view;
            self.model.screen = Screen::Wallet(kept.into_page());
        } else {
            self.model.screen = Screen::Wallet(WalletPage::default());
        }
    }

    /// Show `page`, and ask the worker for what it needs first.
    fn open(&mut self, to: To) {
        self.leave_page();
        let wallet = self.model.wallet.as_ref();
        let first = wallet.and_then(|w| w.accounts.first()).map(|a| a.id);
        let mut receive = None;
        let page = match to {
            To::Dashboard => Page::Dashboard,
            To::Receive(account) => {
                receive = account.or(first);
                Page::Receive(ReceivePage {
                    account: receive,
                    view: None,
                })
            }
            To::AddAccount => Page::AddAccount(AddAccountPage::default()),
            // An account asked for stays the one chosen, even one that
            // cannot spend now: the page says so, and never puts another in
            // its place. With none asked for, the first that can spend.
            To::Send(from) => Page::Send(SendPage {
                from: from.or_else(|| spendable(wallet).next()),
                ..SendPage::default()
            }),
            To::Account(account) => Page::Account(AccountPage {
                account,
                report: None,
            }),
            // The same spend exactly: what was typed for it, when this run
            // signed it.
            To::Resign(account) => Page::Resign(ResignPage {
                account,
                form: self.model.kept_form(account).unwrap_or_default(),
            }),
            To::Submit => Page::Submit(SubmitPage::default()),
            To::Activity => Page::Activity(ActivityPage::default()),
            To::Settings => Page::Settings(SettingsPage {
                node: self.model.node.url.clone().unwrap_or_default(),
            }),
            To::Recovery(target) => Page::Recovery(RecoveryPage {
                target,
                ..RecoveryPage::default()
            }),
            To::Explorer => Page::Explorer(ExplorerPage::default()),
            To::Block(at) => Page::Block(BlockPage {
                at,
                read: None,
                rows_from: 0,
            }),
            To::Tag(account) => Page::Tag(TagPage {
                account,
                read: None,
                reading_older: false,
                typed: false,
            }),
            To::Queue => Page::Queue(QueuePage::default()),
        };
        self.model.screen = Screen::Wallet(WalletPage {
            page,
            ..WalletPage::default()
        });
        if let Some(account) = receive {
            self.send(Command::Receive { account }, Purpose::Receive);
        }
        match to {
            // Read afresh when the page opens, behind anything already
            // asked.
            To::Activity => self.read_activity(),
            // Every account's report before anything else (D19).
            To::Recovery(_) => {
                self.send(Command::Review, Purpose::Review);
            }
            To::Explorer => self.read_chain(),
            To::Block(at) => {
                self.send(Command::Block(at), Purpose::Block(at));
            }
            To::Tag(account) => {
                self.send(Command::Tag(account), Purpose::Tag(account));
            }
            To::Queue => {
                self.send(Command::Pending { count: QUEUE_ROWS }, Purpose::Queue);
            }
            _ => {}
        }
    }

    /// Show `page`, read already: a transaction from the page that held it,
    /// or what a hash named.
    fn show(&mut self, page: Page) {
        self.leave_page();
        self.model.screen = Screen::Wallet(WalletPage {
            page,
            ..WalletPage::default()
        });
    }

    /// The recovery page shown.
    fn recovery(&mut self) -> Option<&mut RecoveryPage> {
        match &mut self.wallet_page()?.page {
            Page::Recovery(r) => Some(r),
            _ => None,
        }
    }

    /// The report the recovery page holds for `account`, as the last
    /// review or search left it.
    fn reviewed(&self, account: AccountId) -> Option<&AccountReport> {
        match &self.model.screen {
            Screen::Wallet(WalletPage {
                page: Page::Recovery(r),
                ..
            }) => r.reports.as_ref()?.iter().find(|a| a.account == account),
            _ => None,
        }
    }

    /// The send or re-sign form shown.
    fn form(&mut self) -> Option<&mut SpendForm> {
        match &mut self.wallet_page()?.page {
            Page::Send(SendPage {
                form,
                stage: SendStage::Compose,
                ..
            })
            | Page::Resign(ResignPage { form, .. }) => Some(form),
            _ => None,
        }
    }

    /// Show `error` on the page, or clear it.
    fn page_error(&mut self, error: Option<String>) {
        if let Some(p) = self.wallet_page() {
            p.error = error;
        }
    }

    pub(super) fn on_wallet(&mut self, msg: WalletMsg) -> Task<Message> {
        // While a command the page waits on runs, the page's controls are
        // shown disabled; a message sent before they were is let go.
        let busy = self.model.busy.is_some();
        match msg {
            WalletMsg::Open(to) => {
                if !busy && self.model.wallet.is_some() {
                    self.open(to);
                }
            }
            WalletMsg::ReceiveFor(account) => {
                if let Some(WalletPage {
                    page: Page::Receive(r),
                    ..
                }) = self.wallet_page()
                {
                    r.account = Some(account);
                    r.view = None;
                    self.send(Command::Receive { account }, Purpose::Receive);
                }
            }
            WalletMsg::Copy(text) => {
                match self.wallet_page() {
                    Some(p) => p.copied = Some(text.clone()),
                    // The stopped screen: the kept spend with these bytes.
                    None => {
                        for s in &mut self.model.signed {
                            s.copied = s.sent.sent.artifact_hex == text;
                        }
                    }
                }
                // A mobile shell's clipboard, where iced's reaches nothing.
                if crate::host::copy(&text) {
                    return Task::none();
                }
                return iced::clipboard::write(text);
            }
            WalletMsg::DiscoverTo(to) => {
                if let Some(WalletPage {
                    page: Page::AddAccount(a),
                    ..
                }) = self.wallet_page()
                {
                    a.to = to;
                }
            }
            WalletMsg::Discover => {
                let Some(WalletPage {
                    page: Page::AddAccount(a),
                    ..
                }) = self.wallet_page()
                else {
                    return Task::none();
                };
                match a.to.trim().parse::<u32>() {
                    Ok(to) => {
                        a.found = None;
                        a.added = None;
                        self.page_error(None);
                        self.send(Command::Discover { to }, Purpose::Discover);
                    }
                    Err(_) => self.page_error(Some(format!(
                        "Search up to which account: a number from 1 to {}.",
                        tawara_wallet_core::DISCOVER_MAX_TO
                    ))),
                }
            }
            WalletMsg::Add(account_index) => {
                self.page_error(None);
                self.send(
                    Command::Restore {
                        account_index,
                        scan_to: None,
                    },
                    Purpose::Restore,
                );
            }
            WalletMsg::From(account) => {
                if let Some(WalletPage {
                    page: Page::Send(s),
                    ..
                }) = self.wallet_page()
                {
                    s.from = Some(account);
                }
            }
            WalletMsg::DestinationTo(i, text) => {
                if let Some(row) = self.form().and_then(|f| f.rows.get_mut(i)) {
                    row.to = text;
                }
            }
            WalletMsg::Amount(i, text) => {
                if let Some(row) = self.form().and_then(|f| f.rows.get_mut(i)) {
                    row.amount = text;
                }
            }
            WalletMsg::Reference(i, text) => {
                if let Some(row) = self.form().and_then(|f| f.rows.get_mut(i)) {
                    row.reference = text.to_uppercase();
                }
            }
            WalletMsg::AddDestination => {
                if let Some(f) = self.form()
                    && f.rows.len() < usize::from(MAX_DESTINATIONS)
                {
                    f.rows.push(DestinationRow::default());
                    f.everything = false;
                }
            }
            WalletMsg::RemoveDestination(i) => {
                if let Some(f) = self.form()
                    && f.rows.len() > 1
                    && i < f.rows.len()
                {
                    f.rows.remove(i);
                }
            }
            WalletMsg::Everything(on) => {
                if let Some(f) = self.form() {
                    f.everything = on && f.rows.len() == 1;
                }
            }
            WalletMsg::Fee(text) => {
                if let Some(f) = self.form() {
                    f.fee = text;
                }
            }
            WalletMsg::BlockToLive(text) => {
                if let Some(f) = self.form() {
                    f.blk_to_live = text;
                }
            }
            WalletMsg::Review => {
                let can: Vec<AccountId> = spendable(self.model.wallet.as_ref()).collect();
                let Some(WalletPage {
                    page:
                        Page::Send(SendPage {
                            from: Some(from),
                            form,
                            stage: SendStage::Compose,
                        }),
                    ..
                }) = self.wallet_page()
                else {
                    return Task::none();
                };
                let from = *from;
                let request = spend_request(from, form);
                if !can.contains(&from) {
                    self.page_error(Some(CANNOT_SPEND.to_owned()));
                    return Task::none();
                }
                match request {
                    Ok(spend) => {
                        self.page_error(None);
                        self.send(Command::PlanSend { spend }, Purpose::PlanSend);
                    }
                    Err(e) => self.page_error(Some(e)),
                }
            }
            WalletMsg::Edit => {
                self.leave_page();
                if let Some(WalletPage {
                    page: Page::Send(s),
                    ..
                }) = self.wallet_page()
                {
                    s.stage = SendStage::Compose;
                }
            }
            WalletMsg::Sign => {
                if let Some(WalletPage {
                    page:
                        Page::Send(SendPage {
                            stage: SendStage::Review(plan),
                            ..
                        }),
                    ..
                }) = self.wallet_page()
                {
                    let plan = plan.plan;
                    self.page_error(None);
                    self.send(Command::ConfirmSend { plan }, Purpose::ConfirmSend);
                }
            }
            WalletMsg::SaveArtifact => {
                let downloads = self.model.downloads.clone();
                if let Some(WalletPage {
                    page:
                        Page::Send(SendPage {
                            stage: SendStage::Sent(sent),
                            ..
                        }),
                    ..
                }) = self.wallet_page()
                {
                    sent.saved = Some(save(downloads.as_deref(), &sent.sent));
                }
            }
            WalletMsg::ShowSigned(hex) => {
                if !busy {
                    self.show_signed(&hex);
                }
            }
            WalletMsg::SaveSigned(hex) => {
                let downloads = self.model.downloads.clone();
                if let Some(s) = self
                    .model
                    .signed
                    .iter_mut()
                    .find(|s| s.sent.sent.artifact_hex == hex)
                {
                    s.sent.saved = Some(save(downloads.as_deref(), &s.sent.sent));
                }
            }
            WalletMsg::Settle(account) => {
                self.page_error(None);
                self.send(Command::Settle { account }, Purpose::Settle);
            }
            WalletMsg::Check(account) => {
                self.page_error(None);
                self.send(
                    Command::Status {
                        account,
                        scan_to: None,
                    },
                    Purpose::Status,
                );
            }
            WalletMsg::Resign => {
                let Some(WalletPage {
                    page: Page::Resign(r),
                    ..
                }) = self.wallet_page()
                else {
                    return Task::none();
                };
                match spend_request(r.account, &r.form) {
                    Ok(spend) => {
                        self.page_error(None);
                        self.send(Command::Resign { spend }, Purpose::Resign);
                    }
                    Err(e) => self.page_error(Some(e)),
                }
            }
            WalletMsg::Report(key, level) => {
                if let Some(p) = self.wallet_page() {
                    p.open.insert(key, level);
                    // On the recovery page, opening an account's full report
                    // counts towards the advance (D29).
                    if let (Page::Recovery(r), ReportKey::Review(n), Level::Full) =
                        (&mut p.page, key, level)
                        && let Some(a) = r.reports.as_ref().and_then(|all| all.get(usize::from(n)))
                    {
                        r.read.insert(a.account);
                    }
                }
            }
            WalletMsg::ArtifactHex(hex) => {
                if let Some(WalletPage {
                    page: Page::Submit(s),
                    ..
                }) = self.wallet_page()
                {
                    s.hex = hex;
                }
            }
            WalletMsg::Submit => {
                let Some(WalletPage {
                    page: Page::Submit(s),
                    ..
                }) = self.wallet_page()
                else {
                    return Task::none();
                };
                let artifact_hex = s.hex.trim().to_owned();
                if artifact_hex.is_empty() {
                    self.page_error(Some("Paste the signed bytes, as hex.".to_owned()));
                } else {
                    s.result = None;
                    self.page_error(None);
                    self.send(
                        Command::SubmitArtifact { artifact_hex },
                        Purpose::SubmitArtifact,
                    );
                }
            }
            WalletMsg::Filter(filter) => {
                if let Some(WalletPage {
                    page: Page::Activity(a),
                    ..
                }) = self.wallet_page()
                {
                    a.filter = filter;
                }
            }
            WalletMsg::Search(text) => {
                if let Some(WalletPage {
                    page: Page::Activity(a),
                    ..
                }) = self.wallet_page()
                {
                    a.search = text;
                }
            }
            WalletMsg::Select(id) => {
                if let Some(WalletPage {
                    page: Page::Activity(a),
                    ..
                }) = self.wallet_page()
                {
                    a.selected = Some(id);
                }
            }
            WalletMsg::ReadActivity => self.read_activity(),
            WalletMsg::ExplorerTyped(text) => {
                if let Some(WalletPage {
                    page: Page::Explorer(e),
                    ..
                }) = self.wallet_page()
                {
                    e.search = text;
                    e.invalid = None;
                }
            }
            WalletMsg::ExplorerFind => {
                let Some(WalletPage {
                    page: Page::Explorer(e),
                    ..
                }) = self.wallet_page()
                else {
                    return Task::none();
                };
                if e.finding.is_some() {
                    return Task::none();
                }
                match Query::parse(&e.search) {
                    Err(why) => e.invalid = Some(why),
                    Ok(Query::Block(n)) => self.open(To::Block(BlockAt::Number(n))),
                    Ok(Query::Tag(account)) => {
                        self.open(To::Tag(account));
                        if let Some(WalletPage {
                            page: Page::Tag(t), ..
                        }) = self.wallet_page()
                        {
                            t.typed = true;
                        }
                    }
                    Ok(Query::Hash(hash)) => {
                        e.missed = None;
                        if self
                            .send(Command::Find(hash), Purpose::Find(hash))
                            .is_some()
                            && let Some(WalletPage {
                                page: Page::Explorer(e),
                                ..
                            }) = self.wallet_page()
                        {
                            e.finding = Some(hash);
                        }
                    }
                }
            }
            WalletMsg::ReadChain => self.read_chain(),
            WalletMsg::BlockRows(from) => {
                if let Some(WalletPage {
                    page: Page::Block(b),
                    ..
                }) = self.wallet_page()
                {
                    b.rows_from = from;
                }
            }
            WalletMsg::OpenSpend(n) => {
                let spend = match self.wallet_page().map(|p| &p.page) {
                    Some(Page::Block(BlockPage {
                        read: Some(Ok(block)),
                        ..
                    })) => block.spends.get(n).cloned().map(|t| (t, block.tip)),
                    _ => None,
                };
                if let Some((transaction, tip)) = spend {
                    self.show(Page::Transaction(TransactionPage {
                        transaction,
                        from_index: false,
                        text: None,
                        tip,
                    }));
                }
            }
            WalletMsg::OpenPending(n) => {
                // From the queue's own page, or from the overview's card.
                let read = match self.wallet_page().map(|p| &p.page) {
                    Some(Page::Queue(q)) => q.read.as_ref(),
                    _ => self.model.pending.last.as_ref(),
                };
                let row = match read {
                    Some(Ok(p)) => p.rows.get(n).and_then(|r| r.transaction.clone()),
                    _ => None,
                };
                if let Some(transaction) = row {
                    self.show(Page::Transaction(TransactionPage {
                        transaction,
                        from_index: false,
                        text: None,
                        tip: None,
                    }));
                }
            }
            WalletMsg::OpenTagRow(n) => {
                let row = match self.wallet_page().map(|p| &p.page) {
                    Some(Page::Tag(TagPage {
                        read: Some(view), ..
                    })) => view
                        .history
                        .as_ref()
                        .ok()
                        .and_then(|h| h.transactions.get(n).cloned()),
                    _ => None,
                };
                if let Some(transaction) = row {
                    self.show(Page::Transaction(TransactionPage {
                        transaction,
                        from_index: true,
                        text: None,
                        tip: None,
                    }));
                }
            }
            WalletMsg::ReadOlderTag => {
                let older = match self.wallet_page().map(|p| &p.page) {
                    Some(Page::Tag(TagPage {
                        account,
                        read: Some(view),
                        reading_older: false,
                        ..
                    })) => view
                        .history
                        .as_ref()
                        .ok()
                        .filter(|h| h.more())
                        .map(|h| (*account, h.next)),
                    _ => None,
                };
                if let Some((account, from)) = older
                    && self
                        .send(
                            Command::TagHistory { account, from },
                            Purpose::TagHistory(account),
                        )
                        .is_some()
                    && let Some(WalletPage {
                        page: Page::Tag(t), ..
                    }) = self.wallet_page()
                {
                    t.reading_older = true;
                }
            }
            WalletMsg::SendTo(account) => {
                // Only from the page of an account the person typed.
                let typed = matches!(
                    self.wallet_page().map(|p| &p.page),
                    Some(Page::Tag(t)) if t.typed && t.account == account
                );
                if busy || self.model.wallet.is_none() || !typed {
                    return Task::none();
                }
                self.open(To::Send(None));
                if let Some(WalletPage {
                    page: Page::Send(s),
                    ..
                }) = self.wallet_page()
                    && let Some(to) = account.destination()
                {
                    s.form.rows = vec![DestinationRow {
                        to,
                        ..DestinationRow::default()
                    }];
                }
            }
            WalletMsg::ReadOlderActivity => self.read_older_activity(),
            WalletMsg::Unit(unit) => {
                self.model.prefs.unit = unit;
                self.save_prefs();
            }
            WalletMsg::AutoLock(minutes) => {
                if IDLE_LOCK_MINUTES.contains(&minutes) {
                    let period = std::time::Duration::from_secs(minutes * 60);
                    self.model.prefs.idle_lock = period;
                    if let Some(w) = &self.worker {
                        w.handle.set_idle_lock(period);
                    }
                    self.save_prefs();
                }
            }
            WalletMsg::NodeTyped(url) => {
                if let Some(WalletPage {
                    page: Page::Settings(s),
                    ..
                }) = self.wallet_page()
                {
                    s.node = url;
                }
            }
            WalletMsg::SaveNode => {
                let Some(WalletPage {
                    page: Page::Settings(s),
                    ..
                }) = self.wallet_page()
                else {
                    return Task::none();
                };
                let url = s.node.trim().to_owned();
                if url.is_empty() {
                    self.page_error(Some(
                        "Type the node's address, starting https://.".to_owned(),
                    ));
                } else {
                    self.page_error(None);
                    self.send(Command::SetNode { url }, Purpose::SetNode { form: true });
                }
            }
            WalletMsg::CheckNode => self.ask_tip(),
            WalletMsg::ReviewAll => {
                if let Some(r) = self.recovery() {
                    r.result = None;
                }
                self.page_error(None);
                self.send(Command::Review, Purpose::Review);
            }
            WalletMsg::Target(account) => {
                if let Some(r) = self.recovery() {
                    r.target = Some(account);
                    r.index.clear();
                    r.confirmed = false;
                }
            }
            WalletMsg::RecoveryIndex(text) => {
                if let Some(r) = self.recovery() {
                    r.index = text;
                }
            }
            WalletMsg::Confirm(on) => {
                if let Some(r) = self.recovery() {
                    r.confirmed = on;
                }
            }
            WalletMsg::Advance => {
                let Some(r) = self.recovery() else {
                    return Task::none();
                };
                let unread = r.unread();
                let (target, typed, confirmed) = (r.target, r.index.trim().to_owned(), r.confirmed);
                let Some(account) = target else {
                    return Task::none();
                };
                // Offered only where the live report names an index; the
                // library advances only to exactly that one.
                let named = self
                    .reviewed(account)
                    .and_then(|a| remedy(&a.state))
                    .is_some_and(|m| matches!(m, Remedy::Advance(_)));
                let checked = if !named {
                    Err("The report names no index to advance this account to.")
                } else if unread != Some(0) {
                    Err(
                        "Open every account's full report first: the evidence that an advance \
                         is wrong is most often in another account's report.",
                    )
                } else if !confirmed {
                    Err("Confirm first that no other wallet uses this recovery phrase.")
                } else {
                    typed
                        .parse::<u32>()
                        .map_err(|_| "Type the key index the report names, in digits.")
                };
                match checked {
                    Ok(advance_to) => {
                        self.page_error(None);
                        self.send(
                            Command::Reconcile {
                                account,
                                advance_to,
                            },
                            Purpose::Reconcile,
                        );
                    }
                    Err(e) => self.page_error(Some(e.to_owned())),
                }
            }
            WalletMsg::SearchFurther => {
                let Some(r) = self.recovery() else {
                    return Task::none();
                };
                let (target, typed) = (r.target, r.index.trim().to_owned());
                let Some(account) = target else {
                    return Task::none();
                };
                match typed.parse::<u32>() {
                    Ok(scan_to) if scan_to <= tawara_wallet_core::MAX_KEY_INDEX => {
                        self.page_error(None);
                        self.send(
                            Command::Status {
                                account,
                                scan_to: Some(scan_to),
                            },
                            Purpose::Status,
                        );
                    }
                    _ => self.page_error(Some(
                        "Type how far to search, as a key index in digits.".to_owned(),
                    )),
                }
            }
        }
        Task::none()
    }

    /// The worker's answer to a command a wallet page sent. Refusals are
    /// shown on the page by [`App::refused`], apart from the ones that move
    /// the page.
    pub(super) fn on_wallet_reply(&mut self, purpose: Purpose, reply: Reply) {
        match (purpose, reply) {
            // An explorer page's answer goes to it while it shows what was
            // asked for; another page has nothing to do with it.
            (Purpose::Block(at), Reply::Block(read)) => {
                if let Some(WalletPage {
                    page: Page::Block(b),
                    ..
                }) = self.wallet_page()
                    && b.at == at
                {
                    b.read = Some(read);
                }
            }
            (Purpose::Tag(account), Reply::Tag(view)) => {
                if let Some(WalletPage {
                    page: Page::Tag(t), ..
                }) = self.wallet_page()
                    && t.account == account
                {
                    t.read = Some(view);
                }
            }
            // A page is added below the rows already read; a refusal leaves
            // them as they were, as Activity's does.
            (Purpose::TagHistory(account), reply) => {
                if let Some(WalletPage {
                    page: Page::Tag(t), ..
                }) = self.wallet_page()
                    && t.account == account
                {
                    t.reading_older = false;
                    if let Reply::TagHistory(Ok(older)) = reply
                        && let Some(view) = &mut t.read
                        && let Ok(history) = &mut view.history
                    {
                        history.extend(older);
                    }
                }
            }
            (Purpose::Find(hash), Reply::Found(found)) => {
                let Some(WalletPage {
                    page: Page::Explorer(e),
                    ..
                }) = self.wallet_page()
                else {
                    return;
                };
                if e.finding != Some(hash) {
                    return;
                }
                e.finding = None;
                match *found {
                    Found::Transaction { transaction, text } => {
                        self.show(Page::Transaction(TransactionPage {
                            transaction,
                            from_index: true,
                            text: Some(text),
                            tip: None,
                        }));
                    }
                    Found::Block(block) => self.show(Page::Block(BlockPage {
                        at: BlockAt::Number(block.summary.index),
                        read: Some(Ok(block)),
                        rows_from: 0,
                    })),
                    Found::Neither {
                        searched,
                        transaction,
                        block,
                    } => {
                        e.missed = Some(Box::new(Missed {
                            searched,
                            transaction,
                            block,
                        }));
                    }
                }
            }
            (Purpose::Queue, reply) => {
                if let Some(WalletPage {
                    page: Page::Queue(q),
                    error,
                    ..
                }) = self.wallet_page()
                {
                    match reply {
                        Reply::Pending(read) => q.read = Some(read),
                        Reply::Refused(r) => *error = Some(r.text),
                        _ => {}
                    }
                }
            }
            (Purpose::Find(hash), Reply::Refused(r)) => {
                if let Some(WalletPage {
                    page: Page::Explorer(e),
                    error,
                    ..
                }) = self.wallet_page()
                    && e.finding == Some(hash)
                {
                    e.finding = None;
                    *error = Some(r.text);
                }
            }
            (Purpose::Block(at), Reply::Refused(r)) => {
                if let Some(WalletPage {
                    page: Page::Block(b),
                    error,
                    ..
                }) = self.wallet_page()
                    && b.at == at
                {
                    *error = Some(r.text);
                }
            }
            (Purpose::Tag(account), Reply::Refused(r)) => {
                if let Some(WalletPage {
                    page: Page::Tag(t),
                    error,
                    ..
                }) = self.wallet_page()
                    && t.account == account
                {
                    *error = Some(r.text);
                }
            }
            (Purpose::Receive, Reply::Receive(view)) => {
                if let Some(WalletPage {
                    page: Page::Receive(r),
                    ..
                }) = self.wallet_page()
                    && r.account == Some(view.account)
                {
                    r.view = Some(view);
                }
            }
            (Purpose::Discover, Reply::Discovered { text, accounts }) => {
                if let Some(WalletPage {
                    page: Page::AddAccount(a),
                    ..
                }) = self.wallet_page()
                {
                    a.found = Some((text, accounts));
                }
            }
            (Purpose::Restore, Reply::Restored { ok, text, opened }) => {
                match opened {
                    Ok(view) => self.model.wallet = Some(view),
                    // The store could not be read back and is closed; the
                    // lock that follows says so.
                    Err(r) => self.page_error(Some(r.text)),
                }
                if let Some(WalletPage {
                    page: Page::AddAccount(a),
                    ..
                }) = self.wallet_page()
                {
                    // What the sweep showed is out of date now.
                    a.found = None;
                    a.added = Some((ok, text));
                }
            }
            (Purpose::PlanSend, Reply::Planned(plan)) => {
                if let Some(WalletPage {
                    page: Page::Send(s),
                    ..
                }) = self.wallet_page()
                {
                    s.stage = SendStage::Review(Box::new(plan));
                } else {
                    // The page was left while it was being laid out.
                    self.send(Command::DiscardPlan, Purpose::DiscardPlan);
                }
            }
            (Purpose::ConfirmSend | Purpose::Resign, Reply::Sent(sent)) => {
                // Whatever is shown, the sent page is: it carries the only
                // copy of the signed bytes (docs/PLAN.md section 4.9).
                self.model.wallet = Some(sent.view.clone());
                let from = Some(sent.from);
                let form = match self.wallet_page().map(|p| &p.page) {
                    Some(Page::Send(s)) => s.form.clone(),
                    Some(Page::Resign(r)) => r.form.clone(),
                    _ => SpendForm::default(),
                };
                self.model.screen = Screen::Wallet(WalletPage {
                    page: Page::Send(SendPage {
                        from,
                        form,
                        stage: SendStage::Sent(Box::new(SentPage {
                            sent,
                            resigned: matches!(purpose, Purpose::Resign),
                            saved: None,
                        })),
                    }),
                    // The three facts are the page's point (docs/PLAN.md
                    // section 4.9): their summary starts open.
                    open: [(ReportKey::Sent, Level::Summary)].into(),
                    ..WalletPage::default()
                });
            }
            // Signing was refused; the worker dropped the plan and used no
            // key. Back to the form, with why.
            (Purpose::ConfirmSend, Reply::Refused(r)) => {
                if let Some(WalletPage {
                    page: Page::Send(s),
                    error,
                    ..
                }) = self.wallet_page()
                {
                    s.stage = SendStage::Compose;
                    *error = Some(r.text);
                }
            }
            (Purpose::Settle, Reply::Settled { text, view }) => {
                self.model.wallet = Some(view);
                if let Some(WalletPage {
                    page: Page::Account(a),
                    ..
                }) = self.wallet_page()
                {
                    a.report = Some((Done::Settled, text));
                }
            }
            (
                Purpose::Status,
                Reply::Status {
                    account,
                    state,
                    spendable,
                    text,
                },
            ) => {
                // The worker's word on spending, not one worked out here: a
                // check that failed and then one that reconciled leave an
                // account the wallet held spendable again, and one set aside
                // when it opened stays aside.
                if let Some(w) = &mut self.model.wallet
                    && let Some(row) = w.accounts.iter_mut().find(|a| a.id == account)
                {
                    row.spendable = spendable;
                    row.state = state.clone();
                }
                let Some(p) = self.wallet_page() else {
                    return;
                };
                match &mut p.page {
                    Page::Account(a) if a.account == account => {
                        a.report = Some((Done::Checked, text));
                    }
                    // A further search on the recovery page: the account's
                    // report, as it stands now. A report that changed is to
                    // be read again, so it folds back to its summary.
                    Page::Recovery(r) => {
                        r.read.remove(&account);
                        if let Some((n, entry)) = r
                            .reports
                            .iter_mut()
                            .flatten()
                            .enumerate()
                            .find(|(_, a)| a.account == account)
                        {
                            *entry = AccountReport {
                                account,
                                state,
                                spendable,
                                text,
                            };
                            p.open.insert(
                                ReportKey::Review(u16::try_from(n).unwrap_or(u16::MAX)),
                                Level::Summary,
                            );
                        }
                    }
                    _ => {}
                }
            }
            (Purpose::Review, Reply::Reviewed(reports)) => {
                // The worker applied what it found; the view follows.
                if let Some(w) = &mut self.model.wallet {
                    for found in &reports {
                        if let Some(row) = w.accounts.iter_mut().find(|a| a.id == found.account) {
                            row.state = found.state.clone();
                            row.spendable = found.spendable;
                        }
                    }
                }
                if let Some(p) = self.wallet_page()
                    && let Page::Recovery(r) = &mut p.page
                {
                    // Each opens to its summary; the advance waits until
                    // every full report has been opened again (D19, D29).
                    for n in 0..reports.len() {
                        p.open.insert(
                            ReportKey::Review(u16::try_from(n).unwrap_or(u16::MAX)),
                            Level::Summary,
                        );
                    }
                    r.read.clear();
                    r.reports = Some(reports);
                }
            }
            (
                Purpose::Reconcile,
                Reply::Reconciled {
                    ok, text, opened, ..
                },
            ) => {
                let reopened = match opened {
                    Ok(view) => {
                        self.model.wallet = Some(view);
                        true
                    }
                    // The store could not be read back and is closed; the
                    // lock that follows says so.
                    Err(r) => {
                        self.page_error(Some(r.text));
                        false
                    }
                };
                if let Some(p) = self.wallet_page()
                    && let Page::Recovery(r) = &mut p.page
                {
                    r.result = Some((ok, text));
                    r.index.clear();
                    r.confirmed = false;
                    p.open.insert(ReportKey::Advanced, Level::Summary);
                }
                // The reports above are from before it: read them again.
                if reopened {
                    self.send(Command::Review, Purpose::Review);
                }
            }
            (Purpose::SubmitArtifact, Reply::Submitted { accepted, text }) => {
                if let Some(WalletPage {
                    page: Page::Submit(s),
                    ..
                }) = self.wallet_page()
                {
                    s.result = Some((accepted, text));
                }
            }
            (_, Reply::Refused(r)) => self.refused(r),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> AccountId {
        AccountId::from_tag([7; 20])
    }

    fn row(to: &str, amount: &str, reference: &str) -> DestinationRow {
        DestinationRow {
            to: to.to_owned(),
            amount: amount.to_owned(),
            reference: reference.to_owned(),
        }
    }

    #[test]
    fn a_spend_is_read_exactly_from_what_was_typed() {
        let form = SpendForm {
            rows: vec![
                row(" dest-a ", "420.5", "PAYROLL-11"),
                DestinationRow::default(),
                row("dest-b", "0.000000001", ""),
            ],
            everything: false,
            fee: "1000".to_owned(),
            blk_to_live: "871200".to_owned(),
        };
        let spend = spend_request(account(), &form).unwrap();
        assert_eq!(spend.from, account());
        assert_eq!(spend.fee, Some(1000));
        assert_eq!(spend.blk_to_live, 871_200);
        assert_eq!(
            spend.destinations,
            vec![
                DestinationInput {
                    to: "dest-a".to_owned(),
                    amount: Amount::Nano(420_500_000_000),
                    reference: "PAYROLL-11".to_owned(),
                },
                DestinationInput {
                    to: "dest-b".to_owned(),
                    amount: Amount::Nano(1),
                    reference: String::new(),
                },
            ],
            "an empty row is left out, and nothing is rounded"
        );
    }

    #[test]
    fn everything_is_for_one_destination_and_defaults_are_the_floor() {
        let mut form = SpendForm {
            rows: vec![row("dest", "", "")],
            everything: true,
            ..SpendForm::default()
        };
        let spend = spend_request(account(), &form).unwrap();
        assert_eq!(spend.destinations[0].amount, Amount::Everything);
        assert_eq!(spend.fee, None);
        assert_eq!(spend.blk_to_live, 0);

        // With two, "everything" does not apply, and the amount is needed.
        form.rows.push(row("other", "1", ""));
        assert!(spend_request(account(), &form).is_err());
    }

    #[test]
    fn what_cannot_be_a_spend_is_said_before_the_worker_is_asked() {
        let empty = SpendForm::default();
        assert!(spend_request(account(), &empty).is_err());
        let bad_amount = SpendForm {
            rows: vec![row("dest", "1.0000000001", "")],
            ..SpendForm::default()
        };
        let e = spend_request(account(), &bad_amount).unwrap_err();
        assert!(e.starts_with("Destination 1:"), "{e}");
        let bad_fee = SpendForm {
            rows: vec![row("dest", "1", "")],
            fee: "0.5".to_owned(),
            ..SpendForm::default()
        };
        assert!(
            spend_request(account(), &bad_fee)
                .unwrap_err()
                .starts_with("The fee")
        );
        let bad_btl = SpendForm {
            rows: vec![row("dest", "1", "")],
            blk_to_live: "soon".to_owned(),
            ..SpendForm::default()
        };
        assert!(spend_request(account(), &bad_btl).is_err());
    }
}
