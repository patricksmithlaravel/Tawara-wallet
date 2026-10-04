# Library changes proposed for Rep-0

The changes to `mochimo-crypto` that Tawara's `wallet-core` would use,
approved by the owner on 2026-10-04 (docs/DECISIONS.md D23). Rep-2 changes
nothing in the library: each change is made in Rep-0
(`patricksmithlaravel/mcm-rust-cli-wallet`), merged down into Rep-1, and
picked up here by moving the pinned `rev` (D1, D7).

None of them changes what the command-line wallet does. Each is an
addition beside what exists, so the command line keeps calling what it
calls today. Line numbers are those of Rep-1 at `121daf3`, the pinned
revision.

Order of work: 1, 2, 4, then 3, which can be taken one function at a time.

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
