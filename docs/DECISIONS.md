# Decisions

Every decision this repository rests on, and every deviation from the plan
or the renderings, with its reason. docs/PLAN.md is the reference; a change
to one of its decisions is recorded here with the owner's approval.

Each entry says who decided it: **plan** (docs/PLAN.md, approved by the
owner on 2026-10-03), **owner** (the owner's answer to a question, with its
date), or **proposed** (made while building, open to the owner's review in
the pull request that introduces it).

## From the plan

### D1. Rep-2 depends on the wallet library; it does not fork it

**plan.** `mochimo-crypto` comes from Rep-1
(`patricksmithlaravel/mcm-rust-cli-windows`) as a git dependency pinned by
full commit hash. No `[patch]`, no vendored copy, no fork. A change the
application needs from the library is proposed to the owner, made in Rep-0,
merged down into Rep-1, and picked up here by moving `rev`. See D7 for the
pin and `crates/wallet-core/tests/policy.rs` for the check that holds it.

### D2. The toolkit is iced

**plan.** iced replaces `FORK.md` R2-2's egui. See D6 for the version and
features.

### D3. The targets are desktop and mobile

**plan.** Windows, macOS and Linux are the primary targets. Android and iOS
are supported by iced's community only, so a feasibility spike (phase 1)
comes before any mobile screen, and all wallet logic lives in
`crates/wallet-core`, which has no interface dependency.

### D4. Rust-native, with the exceptions named

**plan.** Everything is Rust except `ring` (C and assembly for TLS, under
`mesh-https`). No webview, no JavaScript runtime, no embedded browser.
Mobile platform glue that is not Rust is kept to the minimum and listed here
when phase 4 adds it. Linking OS libraries is not an exception.

## Phase 0

### D5. The compiler: Rust 1.98.0

**proposed.** `rust-toolchain.toml` pins `channel = "1.98.0"` with `clippy`
and `rustfmt`.

- It is the compiler Rep-1 pins, so the library is built here by exactly
  the compiler its own board verified it on.
- It is newer than both floors that bind this workspace: `mochimo-crypto`'s
  `rust-version` of 1.89 (for `std::fs::File::try_lock`) and iced 0.14.0's
  `rust-version` of 1.88.
- The latest stable on 2026-10-03 was 1.99.0. Taking it would build the
  library on a compiler its board has not run; the pin moves with Rep-1's,
  in the same change that moves `rev`.
- The workspace declares no `rust-version` of its own. It ships as built
  applications, not as a library others compile, and a declared minimum
  that no CI job compiles on would be an unchecked claim.

### D6. iced 0.14.0, its default features written out

**proposed, following plan D2** ("Pin the latest released iced at the
start of the work"). iced's latest release on 2026-10-03 was 0.14.0
(crates.io). `=0.14.0` fixes the `iced` crate itself. Its subcrates
(`iced_core`, `iced_widget`, `iced_winit`, `iced_renderer`, `iced_wgpu`,
`iced_tiny_skia` and the rest) are required by it at `^0.14.0`, so the
committed `Cargo.lock` is what holds them, and CI builds with `--locked`. At
this commit they resolve to `iced_widget` 0.14.2, `iced_winit` 0.14.1 and
`iced_tiny_skia` 0.14.1, the rest at 0.14.0. A `cargo update` can move them
within 0.14.x; that is a deliberate change, reviewed with `cargo deny check`
like any other. `default-features = false`, with the features listed in the root
`Cargo.toml`: `wgpu`, `tiny-skia`, `crisp`, `web-colors`, `thread-pool`,
`linux-theme-detection`, `x11`, `wayland`. These are iced 0.14.0's own
defaults, written out so that what ships is a list in this repository.
Features the renderings may need later (`svg`, and `qr_code` only with the
owner's approval) are added by the change that needs them, with
`cargo deny check` run against the result. No git revision of iced is used.

### D7. The library pin: Rep-1 at `8c2f39a`

**owner, 2026-10-03.** The owner's instructions for phase 0 named Rep-1's
`02239c1`, or a later commit the owner named; by the time the work began, Rep-1's `main` was at
`121daf3773fbc7869ee56c2c795e21115044dfe7`, which adds the name Tawara, the
binary `tawara`, and the updated `FORK.md` (Rep-2 as a dependent, iced in
place of egui). The library's API did not change between the two. The owner
chose `121daf3`.

**Moved, 2026-10-04, when the owner reported Rep-1's merge.** The pin is
now `8c2f39a2cdc2710ac7363b1d64026b6875a16537`, Rep-1's `main` after its
merge of Rep-0's `cef8ed2`: the four library changes of D23, items 1 to 4
(Rep-0's pull requests #7, #9, #10 and #11). Every library source file
those changes touched is byte for byte the same in Rep-1 as in Rep-0; the
files that differ between the two are Rep-1's own (its Windows store
permissions, the binary's name), and none of them changed between
`121daf3` and `8c2f39a`. The library's API grew and nothing was taken
away, and its `Cargo.toml` did not change, so `Cargo.lock` changes in the
library's own line and nowhere else. What wallet-core does with the new
API is D26. The phase 1 spikes keep `121daf3`: they are the record of that
phase, and nothing in them uses what changed.

The workspace line takes the plan's form (D1) exactly:

```toml
mochimo-crypto = { git = "https://github.com/patricksmithlaravel/mcm-rust-cli-windows",
                   rev = "8c2f39a2cdc2710ac7363b1d64026b6875a16537",
                   default-features = false, features = ["native", "mesh-https"] }
```

Phase 0 first added `version = "2.0.0"` to it, on the belief that
cargo-deny would otherwise deny the line as a wildcard. That was wrong:
`allow-wildcard-paths` admits a version-less git dependency of an
unpublished crate. The key was removed, so the line is the plan's again.

### D8. Names and identifiers

**owner, 2026-10-03.**

| | |
|---|---|
| repository | `patricksmithlaravel/Tawara-wallet` |
| display name | Tawara |
| Android `applicationId` | `com.patricksmithlaravel.tawara` |
| iOS bundle identifier | `com.patricksmithlaravel.tawara` |
| Windows publisher | `patricksmithlaravel` |

### D9. Package names

**proposed.** The directories are the plan's (`crates/wallet-core`,
`crates/app`, `crates/desktop`, `crates/mobile`); the packages are
`tawara-wallet-core`, `tawara-app`, `tawara-desktop` and `tawara-mobile`, so
that no generic name (`app`, `mobile`) reaches a library file name or a
package listing. Android loads the mobile crate as `libtawara_mobile.so`.
The desktop executable's installed name is decided with the packaging
(phase 5); it must not collide with the command-line wallet's `tawara`.

### D10. The licence, and how cargo-deny reads it

**plan section 6** ("The executable is distributed under the wallet's
licence"). `LICENSE.md` is Rep-1's file copied byte for byte (SHA-256
`f958e87e09453252d1eedecd3da93c0d89140ab0420635ac56817442f39dbd17`): the
Mochimo Cryptocurrency Engine License Agreement, version 1.0. Every crate
declares it with `license-file` and `publish = false`.

**proposed, for the check.** The agreement has no SPDX identifier, so
cargo-deny cannot read it. Rep-1 answers that for its own crate with
`[licenses.private] ignore = true`. Here that would pass the library
because it is unpublished rather than because its licence was read, and
would pass any other unpublished crate that entered the graph. So
`deny.toml` names the licence `LicenseRef-Mochimo-Engine-1.0`, clarifies
the library and this repository's four crates to it, pins each to the text
by hash, and allows the identifier for those five crates only. If the
library's licence text changes when `rev` moves, `cargo deny check` fails
until someone reads the new text. Measured: with a wrong hash, the check
exits 4.

The two questions `FORK.md` puts to counsel are still open, and are
answered before first distribution (plan section 6).

### D11. The release profile is restated here

**proposed.** Cargo reads `[profile.*]` only from the root of the
workspace being built, so Rep-1's `[profile.release]` does not reach a
build of this repository. Its two settings are security properties of the
library and are restated in the root `Cargo.toml`:

- `panic = "unwind"`: the library scrubs key material in `Drop`, which
  `panic = "abort"` skips;
- `overflow-checks = true`: a release build otherwise wraps a bare `+` on a
  balance silently.

`crates/wallet-core/tests/policy.rs` fails if either changes, and if
anything else in the repository undoes them: any profile, per-package
override (`[profile.*.package.*]`) or `build-override` in the root manifest
that turns overflow checks off or sets `panic = "abort"`, the same in a
profile in any `.cargo/config(.toml)` in the repository, or rustflags there
that do either. It cannot see cargo configuration outside the repository
(`$CARGO_HOME/config.toml`) or the environment (`RUSTFLAGS`,
`CARGO_PROFILE_*`); CI sets neither.

### D12. What deny.toml admits beyond Rep-1's

**proposed, against plan section 6** ("permissive licences only", "every
entry in `ignore` carries a written reason").

- **Licences:** `BSD-2-Clause` (`arrayref`, under `tiny-skia`), `BSL-1.0`
  (`clipboard-win`, `error-code`), `CC0-1.0` (`hexf-parse`, under `naga`),
  `Zlib` (`foldhash`, under wgpu; `slotmap`, under the text stack and
  wgpu-hal's GL backend). These are the four `FORK.md` R2-2 measured iced
  as needing, and its Licence section reads all four as permissive. No GPL
  or LGPL identifier is allowed; the two crates that offer one (`self_cell`,
  `r-efi`) do so in an OR and are admitted through their permissive arm.
  Dev-dependencies are licence-checked too (`include-dev = true`), and an
  exception no crate matches is an error (`unused-license-exception =
  "deny"`).
- **Advisories ignored, both maintenance notices that arrive with iced
  0.14.0:** `RUSTSEC-2024-0436` (`paste`, a compile-time proc macro under
  `metal`, contributing no code to the executable) and
  `RUSTSEC-2026-0192` (`ttf-parser`, under iced's text stack and, on
  Linux, winit's Wayland decorations; it parses only bundled and locally
  installed fonts, never node data). `FORK.md` R2-2 measured iced
  as tripping `unmaintained` twice; these are the two. Both are
  re-examined when the iced pin moves.
- **Kept from Rep-1:** `unmaintained = "all"`, `yanked = "deny"`, the
  `digest` 0.11 ban, and the denied features of `ureq`, `argon2` and
  `serde_json`. Feature unification means a crate in the interface's graph
  could switch one of those on in the library's dependencies, so the bans
  matter more here, not less.
- **Added to Rep-1's bans:** `ureq`'s `native-tls-no-default`, which in
  ureq 3.4.2 is the same C TLS stack as `native-tls` under another name;
  the `native-tls` and `openssl-sys` crates themselves; and the library's
  `raw-backend` feature, refused in the resolved graph whatever route would
  turn it on (a crate's `[features]`, a renamed dependency, another crate
  depending on the library).
- **Sources:** crates.io, and Rep-1 by git. Nothing else.
- **Wildcards:** a registry dependency with a `*` requirement is denied.
  `allow-wildcard-paths` admits, for the four unpublished crates, every
  dependency that does not come from a registry: the path dependencies
  between them, and the library's git line, whose commit `rev` fixes.

### D13. CI without actions

**proposed, following the owner's instructions for phase 0 ("Prefer plain
commands against the runner's own tools (git, rustup) over third-party
actions, as Rep-1's workflow does") and Rep-1's board workflow.**
`.github/workflows/ci.yml` runs on Windows, macOS and Linux (`-latest`
images): build, clippy with `-D warnings`, tests, docs with
`RUSTDOCFLAGS=-D warnings`, `cargo fmt --check`, and `cargo deny check`.
Every step is a plain command against the runner's own git, rustup and C
compiler; no `uses:` line. The commit is fetched by its SHA, so a re-run
after the base branch moves still checks out the commit it ran for.
cargo-deny is built from crates.io at 0.20.2 with `--locked` rather than
downloaded prebuilt. Every command that resolves the dependency graph uses
`--locked` (`cargo fmt` reads no lockfile). After a failed build the other
checks still run; after a failed clone, line-ending refusal or toolchain
install, nothing runs. The token has no permissions; the clone is
anonymous because the repository is public.

The costs: a cold build on every run, and **no uploaded artifacts**. The
runner gives the token an artifact upload needs only to actions, never to
`run:` steps, so a run's only output is its log. That conflicts with
docs/PLAN.md section 5, "Keep the screenshots as CI artifacts", which phase
3 needs. Before phase 3 the owner decides between an artifact-upload action
pinned by commit hash, as the one exception to this rule, and keeping the
screenshots some other way (for example committed to a branch).

### D14. The renderings are committed as images, not as the supplied bundle

**proposed, pending the owner's confirmation.** The owner supplied the
renderings as one self-unpacking HTML file. Its packaging script carries
text that the owner's standing rule on attribution keeps out of this
repository, and that rule overrides the instruction to commit the
renderings unmodified. So `design/renderings/` holds each of the eight
frames as a PNG drawn from the bundle at its own size, unretouched, and
`design/INDEX.md` records the bundle's SHA-256 so the source can be
identified. The owner keeps the source file.

### D15. Policy checks that run with the tests

**proposed.** `crates/wallet-core/tests/policy.rs` holds the standing rules
that a machine can check, with no network:

- the library line in the plan's form (D7): Rep-1, a 40-hex `rev` and
  nothing else to choose the commit, `default-features = false`, exactly
  `native` and `mesh-https`. The library is found by the package a
  dependency resolves to, not by its key, so a rename does not hide it.
  Every crate takes it as `mochimo-crypto.workspace = true` and nothing
  more, and no crate's `[features]` forwards any of its features. No
  `[patch]` or `[replace]` in any manifest, no `patch`, `source` or `paths`
  in any `.cargo/config(.toml)` in the repository, no `vendor/`, and
  `Cargo.lock` resolved at that commit. (deny.toml separately refuses
  `raw-backend` in the resolved graph.)
- the release profile of D11;
- `Kdf::CHEAP_FOR_TESTS` nowhere outside test code (the owner's
  instructions for phase 0: "Add a check that it never appears outside test
  code"). The scan is conservative by construction: it reads every Rust file
  under each crate except its top-level `tests/` and `benches/`, plus every
  file the module walk reaches wherever it lives, and exempts a file only
  when it is reachable solely through test-only declarations (a
  `#[cfg(test)] mod`, a `cfg_attr(test, path = ..)`, a module nested in a
  test-only one, an `include!` in test-only code). The walk starts at the
  root of every non-test target (library, binaries, build script, examples)
  and follows `mod`, every `#[path]` and every `cfg_attr` path, and literal
  `include!`s, as rustc may in some configuration. Declarations in an
  `include!`d file resolve beside that file, as rustc 1.98.0 resolves them
  (it makes the included file's directory the module directory), and a file
  reached both as a module and by `include!` is followed in both contexts,
  since rustc resolves its declarations in both. Every
  file a non-test declaration can select must exist: a missing one is
  refused, since the check cannot tell a configuration rustc never builds
  from a path it resolved differently from rustc. So is a module file
  reached through a symbolic link (the file, or a directory on its path
  below the crate): rustc resolves the modules such a file declares from
  the link's directory, while the walk compares files by their canonical
  paths. What it does not follow
  in non-test code it refuses as unreadable rather than passing: a
  non-literal `include!`, a conditional path on an inline module, an
  out-of-line `mod` inside a block (rustc loads one there by `#[path]`), a
  macro whose tokens declare a module or use `include!`, `include` imported
  under another name (`use std::include as load;`), and a macro given a
  string that may name a `.rs` file (a macro from a dependency could expand
  to `include!`). A string is judged by its value as rustc reads it, not its
  spelling: escapes are decoded (`"shared.r\x73"` is `shared.rs`), a
  `concat!` or `stringify!` is evaluated, a `concat!` with a part the check
  cannot evaluate (an `env!`) counts as naming one, and the suffix is
  compared without case (macOS and Windows open `shared.rs` for
  `shared.RS`). A bare `env!` is not refused: its value is set outside the
  source, and refusing it would refuse every version string. That keeps the
  walk's production set complete, which is what makes the exemption sound: a
  file a non-test build compiles is never exempt, even when a test also
  reaches it. The crate's own code cannot reach `include!` by a route the
  walk does not see; a macro defined in a dependency can build any path it
  likes, so there the check refuses only what a hand-off looks like, and the
  dependencies themselves are reviewed through `Cargo.lock` and deny.toml.
  In test code, a `mod` inside a block is followed, so a fixture it loads is
  exempt. In every scanned file, code under
  `#[cfg(test)]`, `#[cfg(all(test, ..))]`, `#[test]` or a file's own
  `#![cfg(test)]`, on items, statements, match arms, fields and variants,
  is removed before looking, and identifiers are compared raw or not, macro
  bodies included. `spikes/` is outside the scan: it is phase 1's throwaway
  code and is never built into a shipped artifact;
- `wallet-core` has no interface dependency anywhere in its dependency
  closure as `Cargo.lock` resolved it; the interface crates never depend on
  the library (by any name) or name `mochimo_crypto` in any source; and no
  file the scan reads in `wallet-core` re-exports the library wholesale.

Each check was seen to fail on a deliberately broken copy of the tree: a
`[patch]` section, `raw-backend` on the workspace line, a crate's
`[features]` forwarding `mochimo-crypto/raw-backend`, a renamed library
dependency in `app`, a `paths` override in `.cargo/config.toml`,
`overflow-checks = false` at the top level and in
`[profile.release.package."*"]`, rustflags turning overflow checks off, the
cheap KDF named in `app` (also as a raw identifier) and in a production
module of `wallet-core` two files down, `mochimo_crypto` named in `app`,
`pub use mochimo_crypto` at the top of `wallet-core`, inside a nested
module and in an `include!`d file, the cheap KDF in an `include!`d file, in
a file selected by `#[cfg_attr(unix, path = ..)]` beside a clean fallback,
in an unreferenced file, in a file outside the crate or under its `tests/`
loaded by a `mod` inside a function body (by `#[path]` and by `cfg_attr`),
behind a `macro_rules!` that declares a `mod` or uses `include!`, in a file
a test module also reaches, in a file a test fixture loads that production
code also loads through `include!` imported as `load` and given
`"shared.r\x73"`, and in the crate's `tests/shared.rs` loaded by
`#[path = "../../tests/shared.rs"]` in `src/nested/inc.rs`, itself
`include!`d from `src/lib.rs`, under `tests/` loaded by a `#[path]` in an
inline module of a file that is both a module and `include!`d, and in
`src/alias/shared.rs`, loaded by `src/alias/common.rs`, a link to
`src/actual/common.rs`, beside a test fixture that also loads it; and iced
in `wallet-core`'s dependencies.
Each passes on the tree as committed; an out-of-line `#[cfg(test)] mod
tests;` naming the cheap KDF passes, as it should, and so does a fixture
loaded by a `mod` inside a `#[test]` function. Four more tests hold
the detectors to fixed cases, the walk to a crate laid out on disk where the
answer is known, and its refusals to what it cannot read, so a detector that
sees nothing cannot pass.

### D16. Edition 2024

**proposed.** This workspace's crates use Rust edition 2024 and resolver
3, as iced 0.14.0 does. The library keeps its own edition (2021); editions
are per crate and do not interact.

### D17. Phase 1: the spike patches iced_winit, in `spikes/` only

**proposed.** docs/PLAN.md D2 allows a git revision of iced "only if mobile
requires it, and record why". Mobile requires a change to iced's shell,
`iced_winit` 0.14.1:

- it does not compile for Android or iOS: it imports winit's
  `modifier_supplement`, which winit provides only on desktop platforms;
- on Android it has no way to receive the `AndroidApp` that winit needs,
  since it builds the event loop itself;
- on Android it opens its window before the system has given the app one,
  and creating the surface then panics; when the app goes to the background
  it keeps its surfaces on the native window being destroyed, which winit
  forbids, and on return it does not rebuild them;
- on iOS it sizes the window 1024×768 points.

For phase 1 the change is a patch, `spikes/patches/iced_winit-0.14.1-mobile.patch`
(+166/−19; most of it behind `cfg(target_os = ...)`, and the rest acts only
on the `Suspended` and `Resumed` events, which iced receives only on
Android), applied to the crates.io tarball by
`spikes/tools/vendor-iced-winit.sh` after checking it against the checksum
in `Cargo.lock`, and used through `[patch.crates-io]` in
`spikes/Cargo.toml`. Patching the tarball rather than taking iced's git
repository keeps one copy of every other iced crate in the build.

The patch is confined to `spikes/`, which has its own workspace and is
outside the policy checks of D15 and outside `cargo deny`; the
application's manifests stay free of `[patch]`, as D15 requires. How
production carries the change (a fork of iced at a git revision, which
needs `allow-git` in deny.toml; an upstream change; or a shell crate of
Tawara's own) is for the owner to choose after the phase 1 go/no-go,
with the findings of `docs/spikes/P1-REPORT.md`.

### D18. Phase 1 gate: iced goes ahead on mobile

**owner, 2026-10-04.** After docs/spikes/P1-REPORT.md, the owner decided
"Go for iced mobile": iced stays the toolkit for Android and iOS as well as
the desktop (plan D2 and D3), and the mobile shells are not replaced.

The report recommends three conditions before phase 4, which stand as open
items for then:

1. text entry checked on one real Android phone and one iPhone (composition,
   accented and non-Latin text, the password field's keyboard, paste from
   another app, the arm64 Android build), with the device steps in
   `spikes/README.md`;
2. a production route for the `iced_winit` change of D17 (a fork at a pinned
   git revision, the change sent upstream, or a shell crate of Tawara's
   own), chosen by the owner;
3. insets and showing the keyboard again planned as phase 4 glue, and the
   winit limits (activity recreation, UIScene) tracked upstream.

## Phase 2

### D19. `wallet-core`: one worker, three sessions, the library's own words

**proposed; its open questions decided by the owner on 2026-10-04
(below).** `crates/wallet-core` is built as docs/PLAN.md section 3
describes it, with these choices inside that design:

- **One worker thread** (`worker::spawn`) owns the `Keystore` and the
  `Wallet`. The interface sends `Command`s through a `WorkerHandle` and
  receives `Event`s: one `Done` per command, `Busy` before anything slow
  (the key derivation, the node), `Locked` whenever an open store closes,
  and `Stopped` last. Events carry view models (`view`) with no secret in
  them; the one exception is the new store's recovery phrase, shown once,
  in a `PhraseForDisplay` that zeroizes on drop and prints nothing in
  `Debug`.
- **Three sessions.** *Locked*; *Store*, where the `Keystore` is open and
  the wallet is not (no node chosen, the node silent, no account on the
  ledger yet, every account diverged, or a cancel stopped the wallet
  opening); and *Wallet*, after `Wallet::open` reconciled. Spending needs *Wallet*. The store's
  accounts and destinations can be shown in *Store*, and the library's
  pre-gate operations (status, the acknowledged advance, restore,
  discovery) run there, as they do at the command line.
- **A refused wallet keeps its store.** The worker keeps the password
  only for the length of the command that brought it, so it opens the
  wallet with `Wallet::open_or_return_with_progress` (D23, item 2; D26):
  the reconciliation `Wallet::open` makes, but a refusal or a cancel hands
  the store and the client back, still open and still locked, and the
  store stays open in *Store*. The notice is the library's own "WALLET
  WILL NOT START" page, except for a node that did not answer (the
  worker's notice) and a store none of whose accounts the node holds (the
  library's "THIS STORE IS NOT WHOLE"), told apart from the refusal itself
  with no second request. Until phase 2's follow-up (D26) the worker asked
  the node and compared each account itself before opening, because
  `Wallet::open` dropped the store when it refused.
- **The library's words.** The worker makes each decision through the
  library's `Wallet` and `Keystore` methods, builds the `cli::outcome::
  Outcome` the command line would, and has `cli::render::render` write the
  text. Divergence reports, the startup refusal, the send page with its
  three facts (§4.9), settlement, re-signing, reconciliation, restore and
  discovery therefore read exactly as they do at the command line. The
  standing "THIS STORE IS NOT WHOLE" notice is taken from the renderer
  without the rule the command line draws under it. The command line's own
  decisions are called, not repeated (D23, item 3; D26): `key_access`,
  `spend_all_amount`, `plan_spend`, `reconcile::scope_to`,
  `status_outcome`, `resign_outcome`, `create`'s `nothing_was_created` and
  the plaintext test `plaintext_off_loopback`. Two small ones are still
  private there and repeated here, each naming the function it follows:
  a spend's destinations with "everything" resolved
  (`spend_destinations`), and re-signing's resolution of "everything"
  (`resign_destinations`), along with the parser's checks of a spend's
  arguments (`spend.rs`).
- **A copy of the master seed** is held beside the `Wallet` for the
  session, as the command line holds one (`cli::decide`), because
  `KeyAccess` borrows the seed while `reserve_and_sign` borrows the wallet
  mutably. It is a `Secret` and is dropped with the session.
- **Cancellation** (`WorkerHandle::cancel`) stops every command sent so
  far and none sent later: the handle records the newest request id and
  each command compares its own. A command the cancel reaches before it
  starts does nothing, unless it only drops something (Lock, discarding a
  plan or a phrase, forgetting the node); so a spend queued behind a slow
  command does not sign after a cancel or a move to the background. A
  running command reads it through `recon::Cancel`: opening the wallet, a
  refresh, a status read, a restore, an acknowledged advance and a
  discovery sweep (D23, item 1; D26). Nothing it read is applied, and a
  restore or an advance it stops has written nothing. A spend is never
  stopped between its reservation and its submission.
- **Accounts set aside at open.** `Wallet::open` sets aside an account
  it cannot explain, and the library refuses to spend from it by that
  observation. Settling is not refused on it: the library's
  `settle_if_landed` reconciles the account afresh, so an account set
  aside because the node could not see its outstanding spend settles once
  the node shows it landed. A refresh that finds a set-aside account
  reconciling opens the wallet again, so the account can spend.
- **Walks can be stopped, so they are not bounded below the command
  line's.** The worker runs one command at a time, and the library walks
  every key position up to an index the person names, about 1.6 ms each in
  a release build. Every such walk now takes a cancel (D26), and the idle
  period stops it as a cancel does (D24), so a lock never waits on one:
  `scan_to` (status, restore) and `advance_to` (reconcile) take any index
  the command line takes, up to `MAX_KEY_INDEX` (`u32::MAX - 1`). A
  discovery sweep takes 1 to 1,024, as the command line's does. Until
  phase 2's follow-up the restore and the advance could not be stopped,
  and the owner approved a bound of 100,000 (2026-10-04) to be lifted once
  they could.

**Owner, 2026-10-04: the interface offers the acknowledged advance and
restore, on these terms.** The worker offers the library's acknowledged
advance (`Command::Reconcile`) and restore (`Command::Restore`) because
they are the remedies the library's own reports name, and they move a key
index only on the library's terms (the advance only to the index the live
report names). The advance is needed in a graphical wallet: on mobile the
system can end the app between signing a spend and recording it, which
leaves the account behind the chain. It is also the one remedy the
library warns can destroy keys, when a second wallet is live on the seed.
So, holding to docs/PLAN.md section 4.9 (no button moves a key index "to
make a refusal go away"):

- **The advance has its own account-recovery screen**, reached on
  purpose, never offered as a button on a refusal or a divergence report.
- That screen shows the library's whole report for **every** account in
  the store before anything else: the library's own advance page says the
  evidence that one account's advance is wrong is most often in another
  account's report.
- The person **types the index** the report names, as the command line
  makes them type `--advance-to N`; nothing fills it in.
- The person **confirms that no other wallet uses this seed** before the
  advance is sent.
- **Restore**, which adds a derived account at the position the chain
  already shows and moves no index the store holds, is offered in the
  ordinary flow for adding an account.

### D20. Entropy and wallet-core's two other dependencies

**proposed.** docs/PLAN.md section 4.4 asks for the mechanism to be
recorded. Entropy comes from `getrandom` 0.2 (`getrandom::getrandom`),
which reads the operating system's generator: `BCryptGenRandom` with the
system-preferred generator on Windows (the command-line wallet's call),
the `getrandom` system call with `/dev/urandom` as its fallback on Linux
and Android, `getentropy` on macOS, and `CCRandomGenerateBytes` on iOS.
Creating a store takes three separate draws (the phrase's entropy, the
key-derivation salt, the nonce seed); opening one takes one (the nonce
seed). A failed draw refuses the command with nothing created or opened.

`getrandom` 0.2.17 and `zeroize` 1.9.0 are wallet-core's only direct
dependencies besides the library, and both were already in every build
through it (`ring` uses `getrandom` 0.2; the library uses `zeroize`), so
naming them adds no crate. `serde_json` is a dev-dependency, for the fake
node's tests, and is also already in the graph.

Secret input (§4.2) is a `SecretText`, built with `SecretText::take`,
which moves the text field's buffer into a `Zeroizing<String>` and leaves
the field empty, so the secret is never copied on the way in.

The workspace also optimises the two key-derivation crates (`argon2`,
`blake2`) in the dev profile. Every store the tests make pays Argon2id at
the library's recommended cost, about three seconds each unoptimised and
about one optimised. The release profile is untouched.

### D21. Store location, sync folders, and desktop backups

**proposed; decided by the owner on 2026-10-04 (below).**
`location::default_store_dir` gives docs/PLAN.md section
4.6's defaults, with `keystore` as the store's own folder:
`%LOCALAPPDATA%\Tawara\keystore` on Windows,
`~/Library/Application Support/Tawara/keystore` on macOS, and
`$XDG_DATA_HOME/tawara/keystore` on Linux, falling back to
`~/.local/share` when `$XDG_DATA_HOME` is unset or relative, as the XDG
specification says. Finding the default creates nothing. When a store is
created, the worker makes the application folder (`Tawara` or `tawara`,
and any folder above it that is missing; mode `0700` on Unix), because
the library makes only the store's own folder, in a parent that exists;
the library then makes that folder, mode `0700` or with a private access
list. Neither is made until the store is about to be written: after the
confirmation words, for a new phrase. On Android
and iOS the shell passes its private directory to
`location::store_dir_in`.

**Android: `no_backup/`, not `files/`.** Section 4.6 says "the app's
private files directory". The shell will pass `no_backup/`, which Android
leaves out of Auto Backup by definition, as a second guard beside the
manifest rules of section 4.5 (phase 4). This deviation from the plan's
wording was approved by the owner on 2026-10-04; the manifest rules
(`android:allowBackup="false"` and `dataExtractionRules` that exclude the
store) are still made in phase 4, so the store does not depend on either
guard alone.

`location::sync_warning` says when a chosen folder is inside one a cloud
service syncs (section 4.5's desktop rule): iCloud Drive and its Desktop
and Documents sync, the macOS `CloudStorage` providers, OneDrive (with the
variables Windows sets for it), Dropbox, Google Drive, Box, pCloud,
Nextcloud, ownCloud, MEGA, Synology Drive and Seafile, and a roaming
Windows profile. It checks the path as given and the path with its links
resolved. It warns; it does not refuse, since the plan asks for a warning.

**Owner, 2026-10-04: desktop backups may archive the store.** Section
4.5 says the store is excluded from every backup; on the desktop the
owner decided otherwise. Time Machine on macOS, and Windows' own backups
and snapshots (File History, and the volume shadow copies behind Previous
Versions and System Restore), may keep copies of the store. Tawara sets
no backup exclusion on the desktop. The sync-folder warning above is
unchanged, and on Android and iOS the store is still excluded (section
4.5, phase 4).

What this keeps and what it accepts:

- A copy of the store is encrypted under its password (Argon2id at the
  library's recommended cost), as the store on disk is.
- Restoring an older copy rolls back the account's one-time key index and
  any open reservation. The library reconciles every account against the
  chain before anything is signed: an account the chain has moved past
  the restored index is set aside as diverged (the store behind the
  chain), and nothing signs from it until the acknowledged advance (D19).
- The exposure that remains is a copy taken before a spend was reserved,
  restored while that spend has not landed (or after it failed to land):
  the chain still holds the account at the key the copy expects, so it
  reconciles as in sync, and a new spend would sign with the key the lost
  reservation already signed with. The owner accepts this.

### D22. The node: https, or http to loopback, and the library's roots

**proposed.** As docs/PLAN.md section 4.8 says, there is no default node
and plain `http://` is refused except to `127.0.0.0/8`, `::1` and
`localhost`. The command line refuses the same URLs, with the same test,
but has a flag to override it (`--allow-plaintext-node`); this wallet has
no override. The refusal is the command line's text except in the three
places where it names a command-line flag: it opens with the URL rather
than `--node <URL>`, it says "an acknowledged advance" for
"`reconcile --advance-to`", and its `ACTION` line reads "use an https
node. http to 127.0.0.0/8, ::1 or localhost is accepted." instead of
offering `--allow-plaintext-node`. The owner approved those changes on
2026-10-04 (D25).

Changing or clearing the node detaches an open store from the node it
was opened against: a wallet open against it becomes the store alone,
with no node, and a pending plan is dropped, so nothing goes on asking
that node or sending to it. The next command that asks a node asks the
one chosen then, and a refresh reconciles the store against it.

Trust roots (section 4.8 asks for them to be recorded): the library's
transport with `mesh-https`, which is rustls with `ring` and the bundled
`webpki-roots` (Mozilla's store, compiled in), on every platform. That is
what the command-line wallet uses. The platform's own roots would need a
feature in the library (its `Cargo.toml` already discusses
`rustls-native-certs`). **Owner, 2026-10-04: the bundled roots stay**; the
platform's roots are not pursued now (D23, item 5). They would add
platform-specific verification and trust any root installed on the
machine, including a corporate inspection root; they are worth revisiting
only if people need private nodes with their own certificates.

### D23. Library changes this phase would use

**proposed; decided by the owner on 2026-10-04 (below); items 1 to 4
landed in the library and are in use (D7, D26).** Rep-2 does not
modify the library. These are the changes wallet-core would use, to be
made in Rep-0, merged down into Rep-1, and picked up here by moving `rev`.
None blocks phase 2.

**Owner, 2026-10-04.** Items 1 to 4 are approved, to be worked in the
order 1, 2, 4, then 3 (which can be taken a function at a time). Item 5
is not pursued now (D22). Item 6 is open: it is needed before release
(phase 5) if moving people from the older wallets is a goal of the
release, and not otherwise. Each approved change is written up for Rep-0
in `docs/LIBRARY-PROPOSALS.md`: what is asked, why, the shape proposed,
and what wallet-core drops once it lands.

1. **`Cancel` and progress for the long walks.** `Wallet::open`,
   `restore::restore_account`, `reconcile::advance_acknowledged` and
   `discover::sweep` take no `recon::Cancel` and report no progress, so
   the worker cannot stop them or show how far they are (section 3 asks
   for both), and has to bound the indices the person names (D19). A
   variant of each taking `&Cancel` and a progress callback. (A status
   read needs nothing: the worker builds it from the public
   `recon::access_for` and `recon::reconcile_account_with`, which takes a
   `Cancel`, as `reconcile::account_status` does with `Cancel::NEVER`.)
2. **`Wallet::open` that returns the store when it refuses**, so a
   refusal never closes the store. The worker's checks before opening
   (D19) would then be unnecessary, and so would the comparison they make
   twice for every account on every open.
3. **The command line's private decisions, made public**: `key_access`,
   `spend_all_amount`, `plan_spend`, `reconcile::scope_to`, the
   classification in `cmd_status`, the outcome choice in `cmd_resign`, and
   `create`'s `nothing_was_created`. The worker repeats each today (D19).
4. **The emptying warning before signing.** The send page includes "THIS
   EMPTIES THE ACCOUNT" when the change is zero, but only after the spend
   is signed. The confirmation screen needs the same words before it;
   `emptying_text` (private) or an outcome for a planned spend would give
   them. Until then wallet-core reports `PlanView::empties_account` and
   the interface needs a wording the owner approves.
5. **Platform trust roots** as a feature beside `mesh-https` (D22). Not
   now (owner, 2026-10-04).
6. **Importing a legacy store.** The library has no import of the older
   wallets' `.mcm` files, so neither has wallet-core. Open: before
   release if moving people from the older wallets is a goal.

### D24. The wallet locks after five idle minutes

**proposed.** `DEFAULT_IDLE_LOCK` is five minutes, the period the
dashboard rendering shows ("auto-locks in 5 min"). The interface can set
another (`Config::idle_lock`) and calls `WorkerHandle::touch` when the
person does something, which counts at once even while a command runs.
The period is measured from the person's last input: that call, or a
command carrying something they typed (unlock, create, the confirmation
words). Any other command is not input, so an interface that polls the
chain tip or refreshes on a timer cannot keep the store open, and the
period is checked before each queued command is taken, so polls queued
behind a slow node cannot either. A command still running when the period
passes is stopped as a cancel stops it, and the store locks as soon as it
has (D26): a walk to a far key index never holds the lock back. The stop
holds for the rest of that command even if the person returns meanwhile,
since the walk it ended reports a search cut short. Unlocking
and creating are the exception: they carry the person's input and count
as activity when they finish, so only a cancel stops them, and the wallet
opening inside them walks no further than the library's diagnostic
scope. Locking drops the `Keystore` (its key, master seed
and lock), the worker's copy of the seed, any pending recovery phrase and
any pending plan. It also happens on `Command::Lock`, when the app moves
to the background (`WorkerHandle::background`, which first cancels every
command sent so far), when another store is unlocked in its place, on
shutdown, and when the last handle is dropped (both of which also cancel
every command sent so far, so nothing still queued, a spend among it,
runs on the way out). Each reports `Locked` with
its reason, and so does an idle or background lock that drops a recovery
phrase waiting for its confirmation.

### D25. Wording the worker writes itself

**proposed; approved by the owner on 2026-10-04 (below).** docs/PLAN.md
section 3 requires the owner's approval for any
paraphrase of the library's protective text. These are the worker's own
texts and the one changed library text, listed so they can be approved or
changed in one place:

- the plaintext-node refusal, in the three places where the command
  line's text names its flags (D22);
- the standing notice without the command line's rule under it (D19);
- the *Store* session's notices: no node chosen, the node changed, the
  node not answering, and the wallet opening cancelled (`worker.rs`,
  `NO_NODE`, `NODE_CHANGED`, `NODE_SILENT` and `OPEN_CANCELLED`);
- what a cancelled restore or advance answers in place of the library's
  page, which has none for a cancel (`RESTORE_CANCELLED`,
  `ADVANCE_CANCELLED`);
- the create refusals before a phrase is shown (a store already in the
  folder, the passwords differing) and at confirmation (the wrong words,
  nothing to confirm), each ending with the library's "Nothing was
  created.";
- "there is no keystore at ...: the folder does not exist", for a folder
  that is not there (the library reports it as `keystore stat directory:
  NotFound`);
- the short refusals for a command in the wrong session (locked, wallet
  not open, no node, no such plan);
- the refusal of a scan or an advance past `MAX_KEY_INDEX` (D19), whose
  range is the command line's and whose reason is the one its parser's
  documentation gives (`cli::args`, `index_flag`), and of a discovery bound
  outside 1 to 1,024, whose two reasons are the command line's; reworded
  because its text names its flags and verbs (`--scan-to`, `--advance-to`,
  `--to`, `address`, `balance`).

The rendered pages also name command-line verbs (`settle`,
`reconcile ... --advance-to N`, `submit`).

**Owner, 2026-10-04.** The plaintext-node refusal's three changes stand.
The worker's other texts above stand as placeholders, and their final
wording is settled in phase 3 with the screens that show them. The
library's rendered pages are shown word for word, not paraphrased; where
a page names a command-line verb, the screen adds a short note naming the
control that does the same (for example, that `settle` is the Settle
button).

### D26. Phase 2 follow-up: wallet-core on the library's new calls

**proposed.** With the pin at `8c2f39a` (D7), wallet-core uses D23's four
changes and drops the workaround each replaced
(docs/LIBRARY-PROPOSALS.md says what each was):

- **Opening the wallet** (item 2) is
  `Wallet::open_or_return_with_progress`. The checks before opening, the
  reopen with the password after a refusal, and `LockReason::WalletRefused`
  and `RefusalKind::WalletRefused` are gone: a refusal leaves the store
  open with its notice (D19). The node is asked about each account once
  per open, where the checks asked about it two or three times.
- **Cancellation and progress** (item 1). Opening, restore
  (`restore_account_with_progress`), the acknowledged advance
  (`advance_acknowledged_with_progress`) and discovery
  (`sweep_with_progress`) take the worker's cancel. That cancel also says
  stop once the idle period has passed (D24), except while unlocking or
  creating. Once a request has been told to stop, it stays stopped for
  the rest of it, including the wallet opening after a restore or an
  advance. Without that, a touch arriving between the stop and the next
  question would let a walk cut short be applied as a divergence. The
  library's counts reach the interface as `Event::Progress`,
  in wallet-core's own `Progress`, which names no library type. A status
  read reports no progress: the library's walk for it takes a cancel and
  no counter. Adding one is a small library change, if the screens want it.
- **What a stopped command answers.**
  - A refresh with the wallet open, a status read and a sweep answer
    `Refused` with kind `Cancelled`, and nothing they read is applied. A
    sweep stopped short reports nothing found, as the library's
    `Unfinished::Cancelled` requires.
  - Unlock, create and a refresh of the store alone answer with the store
    open on its own, not reconciled, and a notice saying so.
  - A restore or an advance answers with its own reply (`Restored`,
    `Reconciled`): `ok` false, no index, a text saying nothing was
    written, and the store back open on its own. It took the store out of
    its session to write it. The cancel that stopped it also stops the
    wallet opening again, before that opening asks the node anything.
- **The bound on key indices** is the command line's: `MAX_KEY_INDEX`,
  which is `u32::MAX - 1`, in place of `MAX_SCAN_TO` (D19).
- **The planned page** (item 4). `PlanView::text` is the library's page
  for a spend laid out and not signed (`Outcome::planned`), including
  "THIS EMPTIES THE ACCOUNT" when the change is zero. `empties_account`
  stays, as a flag a screen can style by. The warning needs no paraphrase
  (D25).
- **The command line's decisions** (item 3) are called, not repeated
  (D19). A checked spend is now the library's own `cli::args::Spend`,
  which `plan_spend` takes.
- **The worker's new texts** (`NODE_SILENT` is the old silent-node notice,
  moved into a constant; `OPEN_CANCELLED`, `RESTORE_CANCELLED` and
  `ADVANCE_CANCELLED` are new) are listed in D25. They are placeholders on
  D25's terms, for the owner to confirm with the screens in phase 3.

The worker's tests cover each of these:
- a cancel stopping an unlock, a restore, an advance and a sweep;
- the idle period stopping a walk to `MAX_KEY_INDEX` and locking;
- the progress reports of opening, restore, advance and sweep;
- a refused reopen keeping its store;
- the planned page with and without the emptying warning.
