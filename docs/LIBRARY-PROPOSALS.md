# Library changes proposed for Rep-0

The changes to `mochimo-crypto` that Tawara's `wallet-core` would use,
approved by the owner on 2026-10-04 (docs/DECISIONS.md D23). Rep-2 changes
nothing in the library: each change is made in Rep-0
(`patricksmithlaravel/mcm-rust-cli-wallet`), merged down into Rep-1, and
picked up here by moving the pinned `rev` (D1, D7).

None of them changes what the command-line wallet does. Each is an
addition beside what exists, so the command line keeps calling what it
calls today. Line numbers are those of Rep-1 at `121daf3`, the revision
pinned when this was written.

Order of work: 1, 2, 4, then 3, which can be taken one function at a time.

## Status: all four landed and are in use

Rep-0 made the four changes (its pull requests #7, #9, #10 and #11, merged
at `cef8ed2`). Rep-1 merged them down (`8c2f39a`), and the pin moved there
(docs/DECISIONS.md D7). wallet-core uses them and has dropped what each
replaced (D26):

| item | what the library now has | what wallet-core dropped |
|---|---|---|
| 1 | `_with` and `_with_progress` forms of `Wallet::open`, `restore_account`, `advance_acknowledged` and `sweep`; a cancel is `Unfinished::Cancelled`, and a cancelled call writes nothing | the `MAX_SCAN_TO` bound of 100,000 (the command line's `u32::MAX - 1` stands instead), and the note in D19 that these walks run to their end; the worker now sends `Event::Progress` |
| 2 | `Wallet::open_or_return` and its cancellable forms, handing the store and the client back on a refusal and on a cancel | the checks before `Wallet::open`, the reopen with the password, and the `WalletRefused` closure |
| 4 | `Outcome::Planned` and `Outcome::planned(&plan)`, rendered with the emptying warning when the change is zero | `PlanView::empties_account` as the only signal; `PlanView::text` carries the library's page |
| 3 | `key_access`, `spend_all_amount`, `plan_spend`, `reconcile::scope_to`, `status_outcome`, `resign_outcome`, `create::nothing_was_created` and `args::plaintext_off_loopback`, all public | each copy |

What is still repeated, and small: a spend's destinations with "everything"
resolved (the library's `spend_destinations`), re-signing's resolution of
"everything" (`resign_destinations`), and the parser's checks of a spend's
arguments, all private in the library. A status read can be cancelled but
reports no progress, because the library's walk for it takes no counter.
Either is a small addition if the screens need it.

## 1. Cancellation and progress for the long operations

**What exists.** Four operations a person waits on take no `recon::Cancel`
and report no progress:

| operation | where | what it waits on |
|---|---|---|
| `Wallet::open(store, client, master)` | `wallet.rs:187` | every account reconciled; a divergence larger than the window walks up to `RECOVERY_CEILING` positions, about 15.8 s each in a release build |
| `cli::restore::restore_account(store, client, master, account_index, scan_to)` | `cli/restore.rs:63` | the walk to `scan_to` |
| `cli::reconcile::advance_acknowledged(store, client, tag, master, advance_to)` | `cli/reconcile.rs:127` | every account reconciled, the named one walked to `advance_to` |
| `cli::discover::sweep(store, client, master, to)` | `cli/discover.rs:109` | one node request per account, up to 1,024 |

The work underneath already takes a `Cancel`: `restore_account` calls
`recon::restore_account_index_with` (`cli/restore.rs:75`) and
`advance_acknowledged` calls `recon::reconcile_account_with` and
`recon::advance_after_operator_review` (`cli/reconcile.rs:159`, `:211`),
each with `Cancel::NEVER`.

**Why Tawara needs it.** The application runs one worker thread, so
locking (after the idle period, on a move to the background, on Lock)
waits for the running operation. Today wallet-core cannot stop any of
these four, so it refuses a scan or an advance past index 100,000
(`MAX_SCAN_TO`, D19) to keep that wait to a few minutes, and it cannot
show how far a long operation has got (docs/PLAN.md section 3 asks for
both cancellation and progress).

**Proposed.**

- A `_with` variant of each taking `cancel: &recon::Cancel<'_>`, passed
  down to the calls that already take one, and checked between accounts
  in `Wallet::open` and `sweep`. The existing functions stay, as wrappers
  passing `Cancel::NEVER`.
- **A cancelled call writes nothing.** Restore and the advance already
  decide before they write (restore builds the account and commits once,
  `cli/restore.rs`; the advance commits only in
  `advance_after_operator_review`), so the check belongs before that
  write, and a cancel after it is too late to honour. A cancelled call
  returns `Error::Cancelled` (or the operation's own failure type with
  that cause), never a report that reads as a finding about the account,
  as `StoppedBy::Cancelled` already makes the walk's own record say.
- Progress, separately and smaller: a callback,
  `progress: &mut dyn FnMut(Progress)`, with
  `Progress { account: u32, accounts: u32, position: u32, ceiling: u32 }`
  or similar, called between accounts and every few hundred positions of
  a walk. It can follow the cancellation change.
- Tests the library could carry: each operation cancelled partway leaves
  the store's snapshot byte-for-byte as it was; a cancel raised before
  the call stops it before the first node request.

**What wallet-core drops.** The `MAX_SCAN_TO` refusal for the operations
that become cancellable (a lock then cancels them instead of waiting),
and the note in D19 that these four run to their end.

## 2. `Wallet::open` that hands the store back when it refuses

**What exists.** `Wallet::open(store, client, master) -> Result<Wallet,
StartupRefusal>` (`wallet.rs:187`) takes the store by value and drops it
when every account diverges, closing it.

**Why Tawara needs it.** The application keeps the password only for the
command that brought it, so a dropped store cannot be reopened without
asking the person again. To avoid that, wallet-core first asks the node
about each tag and then makes the comparison `Wallet::open` makes for each
account, and only opens the wallet when at least one account reconciles
(D19). That reconciles every account twice on every unlock, and it still
leaves a window: when the node's answer changes between the comparison and
`Wallet::open`, the store is lost and wallet-core has to report it closed
(`LockReason::WalletRefused`).

**Proposed.** Beside `open`:

```rust
pub struct Refused<M: Medium, T: Transport> {
    pub refusal: StartupRefusal,
    pub store: Keystore<M>,
    pub client: MeshClient<T>,
}

impl<M: Medium, T: Transport> Wallet<M, T> {
    pub fn open_or_return(
        store: Keystore<M>,
        client: MeshClient<T>,
        master: Option<&Secret<SEED_LEN>>,
    ) -> Result<Wallet<M, T>, Refused<M, T>>;
}
```

`open` stays as it is (it can call `open_or_return` and drop the parts).
With item 1, `open_or_return` takes a `Cancel` as well.

**What wallet-core drops.** The checks before `Wallet::open`, the reopen
with the password after a refusal, and the `WalletRefused` closure.

## 4. The emptying warning before the spend is signed

**What exists.** When a spend's change is zero, the page the command line
prints after `send` includes "THIS EMPTIES THE ACCOUNT", from the private
`emptying_text` (`cli/mod.rs:852`). It is rendered only in the page for a
spend already signed and submitted (`cli/render.rs`, the `Sent` outcome).

**Why Tawara needs it.** The application shows a spend before it is
signed and asks for confirmation. That confirmation screen is where the
warning matters most, and without the library's words it would need a
paraphrase (D25 requires the owner's approval for any).

**Proposed**, in order of preference:

- An outcome for a planned spend, rendered by `cli::render::render`: the
  same destination lines (checksummed, in wire order), fee, change and
  block-to-live the `Sent` page shows, and the emptying paragraph when the
  change is zero, without the parts that only exist after signing (the
  artifact, the submission). The confirmation screen then shows the
  library's own pre-signing page.
- At least, `emptying_text` made public, for example as
  `cli::render::emptying_warning(source: &Tag) -> Result<String>`.

**What wallet-core drops.** The `PlanView::empties_account` flag as the
only signal, and any paraphrase of the warning.

## 3. The command line's decisions, made public

**What exists.** wallet-core makes the same decisions the command line
makes, but several live in private functions, so wallet-core repeats them
and names the function it follows:

| decision | where | wallet-core's copy |
|---|---|---|
| which key access an account needs, with the command line's refusals | `cli/mod.rs:194`, `key_access` | `worker.rs`, `key_access` |
| the amount for "everything": balance less fee, refused at zero | `cli/mod.rs:904`, `spend_all_amount` | `worker.rs`, `spend_all_amount` |
| laying out "everything" from one ledger read | `cli/mod.rs:878`, `plan_spend` | `worker.rs`, `plan_send` |
| the scan scope for `--scan-to` / `--advance-to` | `cli/reconcile.rs:65`, `scope_to` | `worker.rs`, `scan_scope` |
| which divergences `status` reports and which it refuses | `cli/mod.rs:755`, `cmd_status` | `worker.rs`, `status` |
| which outcome a re-sign produces | `cli/mod.rs:1225`, `cmd_resign` | `worker.rs`, `resign` |
| "Nothing was created." on every refusal before the write | `cli/create.rs:692`, `nothing_was_created` | `worker.rs`, `nothing_was_created` |
| plaintext http to a host that is not loopback | `cli/args.rs:1092`, `plaintext_off_loopback` | `node.rs`, `plaintext_off_loopback` |

**Why.** A copy can drift: a change to one of these in the library would
reach the command line and not the application, and nothing would say so.

**Proposed.** The pure decisions made `pub` with their documentation as
it is (`key_access`, `spend_all_amount`, `scope_to`, `nothing_was_created`,
`plaintext_off_loopback`). For the two classifications, a function from
the library call's result to the `Outcome`, for example
`cli::status_outcome(tag, Result<AccountStatus, Divergence>) -> Outcome`
and `cli::resign_outcome(source, Result<SignedTransaction, Error>, ...) ->
Outcome`, so the command line and the application render from one
decision. `plan_spend` made public as it is. `cli::decide` already exists
but runs a whole command, opening the wallet each time, which a
long-running application cannot use step by step.

**What wallet-core drops.** Each copy, as its public counterpart lands.

## Not pursued now

- **Platform trust roots** (D22, D23 item 5). The bundled `webpki-roots`
  stay, as the command line has them.
- **Importing the older wallets' `.mcm` files** (D23 item 6). Open: it is
  needed before release if moving people from the older wallets is a goal
  of the release.

# Proposed in phase 3

The screens of phase 3 (docs/DECISIONS.md D27, item 11) found more the
library could serve. None blocks phase 3: each control it would back is
left out until it lands (D27, item 4). Line numbers are those of Rep-1 at
`8c2f39a`, the revision pinned when this was written. Suggested order: 5,
then 6, 7 and 8, then 9; 10 and 11 wait on the owner.

## Status: 5 to 8 landed, and are not yet used

Rep-0 made items 5 to 8 (its pull requests #13 to #16, merged at
`2e69d87`). Rep-1 merged them down (its pull request #7, `e41f8c1`), and
the pin moved there (docs/DECISIONS.md D7). No screen uses them yet; the
controls each would back are still left out (D27, item 4).

| item | what the library now has |
|---|---|
| 5 | `MeshBlock::metadata`, an `Option<BlockMetadata>` read only when all eight of the node's keys are there, and `MeshBlock::kind()`: normal, pseudo or neogenesis, and `None` for a block that is not neogenesis when the node sent no metadata |
| 6 | `MeshClient::mempool()`, the waiting ids in the queue's order, and `MeshClient::mempool_transaction(id)`, one of them read whole; a `mempool [--count N]` verb |
| 7 | `MeshClient::search_by_account_from(tag, limit, offset)` and `cli::cmd_recent_transactions_from`; `recent-transactions --from M` |
| 8 | `MeshClient::networks()` over `/network/list`, and `MeshClient::network_status_full()`: the tip, its solve time, the genesis block and the middleware's own sync state |

Item 8's sync state is the middleware's view of its one node: whether its
last refresh of the node's tip finished, every five seconds by default. It
does not say whether the node is current with the network; the tip's solve
time is what shows a tip that has stopped moving.

## 5. The block's own metadata and its type

**What exists.** `MeshClient::block_by_index` and `block_by_hash`
(`mesh/mod.rs:232`, `:240`) return `codec::MeshBlock { block, parent,
timestamp_ms, transactions }` (`mesh/codec.rs:466`). The node's `/block`
reply also carries `block.metadata = {block_size, difficulty, fee, haiku,
nonce, root, stime, tx_count}`, and `parse_block` drops it on purpose
(`codec.rs:588-590`: "is not read").

**Why.** The explorer renderings (06, 07) show each block's difficulty,
haiku, nonce, Merkle root and type (normal, pseudo, neogenesis), and the
dashboard (02) the types of the last six blocks. Parsing the node's JSON
in the application would put a second Mesh parser outside the library.

**Proposed.** `MeshBlock` gains `metadata: Option<BlockMetadata>` with the
fields the node sends, each checked as the codec checks the rest, and a
`kind()` that names normal, pseudo and neogenesis by the protocol's own
rules, so the classification lives beside the parser.

## 6. A read of the mempool

**What exists.** Nothing: no request builder, parser or client method.
`/network/options` reports `"mempool_coins": false`, unparsed.

**Why.** The explorer overview (06) lists the pending transactions with
their destinations, fee and amount, and counts them.

**Proposed.** `MeshClient::mempool()` over `/mempool` (the transaction
ids) and `/mempool/transaction` (one), with the same response caps as
`/block`, and a renderer page for a read-only `mempool` verb.

## 7. An offset for a tag's history

**What exists.** `search_by_account(tag, limit)` (`mesh/mod.rs:263`)
sends no offset, so a tag's history stops at its newest 100 rows;
`SearchPage::next_offset` is read and cannot be used.

**Why.** The activity screen (04) and the tag page (08) list a tag's
history; an account with more than 100 transactions loses the rest
silently.

**Proposed.** `search_by_account_from(tag, limit, offset)`, and
`recent-transactions --from N`.

## 8. The network's name and the node's sync state

**What exists.** `network_status` (`mesh/mod.rs:209`) keeps the tip and
drops the reply's timestamp and `sync_status`; `/network/list` has a
parser (`codec.rs:260`) and no client method.

**Why.** The node card (05) shows the network ("mochimo · mainnet") and
whether the node is connected and current. A node that is still syncing
answers with an old tip, and every reconciliation against it is then
against the past.

**Proposed.** `ChainTip` stays as it is; `network_status_full()` returns
it with the timestamp and the sync state, and `networks()` the list.

## 9. Progress for a status read

**What exists.** A status read takes a `Cancel`
(`recon::reconcile_account_with`) and no progress counter; the four
long operations of item 1 report progress.

**Why.** The account-recovery screen runs a status read to a key index
the person types, which can be far, and can show only a spinner while it
walks.

**Proposed.** `reconcile::account_status_with_progress`, counting as
`recon::watched` does.

## 9a. A derived account's number

**What exists.** `cli::address::accounts_in` (`cli/address.rs:69`) returns
`Held { tag, kind, index }`; the derivation number is in the store's record
(`account::KeyMaterial::Derived { account_index, .. }`), which is
`pub(crate)` on purpose, so that code outside cannot fabricate one.

**Why.** The command line names a derived account by its number (`address
--account N`, `restore --account N`); the application can only name it by
its tag, which a person does not recognise.

**Proposed.** Reading it, not constructing it: `Held` gains `account:
Option<u32>`, `Some(n)` for a derived account, filled from the record.

**Meanwhile** (docs/DECISIONS.md D29, item 8): the application finds the
number by deriving account tags from the seed with the public
`derive::derive_account_tag` until each derived account matches, up to
the discovery ceiling. This change would let it read the number instead.

## 10. Changing the store's password

**What exists.** No call re-encrypts the store under a new password.

**Why.** The settings rendering (05) has "Password: Change". Only if the
owner wants it.

## 11. Importing the older wallets' `.mcm` files

D23, item 6: open. The first-run rendering (01) offers it; it is needed
before release if moving people from the older wallets is a goal.
