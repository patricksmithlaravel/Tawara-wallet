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

*Amended (owner, 2026-10-13), the reading of the last sentence: linking an
OS library is not one of the non-Rust exceptions this entry lists, so it is
allowed without one. Each entry that adds a link lists the OS libraries it
adds (D42, D45). Tawara-mobile already links one: `ndk-sys` links
`libandroid` by default, and links its other platform libraries
(`mediandk`, `nativewindow`, `jnigraphics`, `aaudio`, `sync`) only behind
features that Tawara-mobile's build does not turn on today.*

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
*Since phase 4, iced comes from the owner's fork at a pinned commit whose
sources are these releases plus the mobile change (D32, item 1).*

*Amended 2026-10-12: the eight features above are the defaults of iced
0.14.0 and of the branch commit `71f00e8`, but not of the release Tawara
now pins, `v0.14.1-mobile.2`. Its defaults add `android-native-activity`,
`mobile-logger`, `mobile-fira-sans` and `mobile-system-fonts`, which
`default-features = false` leaves off. See D32 item 9.*

### D7. The library pin: Rep-1 at `7cdc2e9`

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

**Moved again, 2026-10-05, when the owner reported Rep-1's merge.** The pin
is now `e41f8c13e605b9fd098c913784bc9eadc20543bd`, Rep-1's `main` after its
pull request #7, the merge of Rep-0's `2e69d87`: the explorer reads of
docs/LIBRARY-PROPOSALS.md, items 5 to 8 (Rep-0's pull requests #13 to #16).
The merge changed no manifest, so `Cargo.lock` again changes in the
library's own line and nowhere else. Two of the additions are fields, and
wallet-core's sample data fills them: `Outcome::RecentTransactions` gains
`from`, the offset its page starts at, and `MeshBlock` gains
`metadata`, which the sample blocks leave absent. Nothing else in Tawara
names what changed. The screens that use the new reads are later work.

**Moved again, 2026-10-05, when the owner reported Rep-1's merge.** The pin
is now `7cdc2e986319d508e4c197a4ba44220dd49a5b1d`, Rep-1's `main` after its
pull request #8, the merge of Rep-0's `4738baf`: the library's own reading
of a refused search (Rep-0's pull request #17, D30 item 4). Its tree is
that of `3ec5929`, the commit that pull request tested Tawara against. The
merge changed no manifest, so `Cargo.lock` changes in the library's own
line and nowhere else. A refused search now arrives as
`Outcome::SearchFailed`, and the worker's history read matches it where it
matched `ExplorerFailed`; the sample's refusals are written from it, so
their pages say which refusal each was.

The workspace line takes the plan's form (D1) exactly:

```toml
mochimo-crypto = { git = "https://github.com/patricksmithlaravel/mcm-rust-cli-windows",
                   rev = "7cdc2e986319d508e4c197a4ba44220dd49a5b1d",
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

*Proposed in D36, 2026-10-13, for the owner's decision: the macOS bundle
identifier `com.patricksmithlaravel.tawara` and the Linux desktop entry
`com.patricksmithlaravel.tawara.desktop`. Until the owner decides, this
entry names neither.*

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
  *Amended 2026-10-12: re-examined at the release `v0.14.1-mobile.2`.
  Both still arrive, now with iced 0.14.1, and both are still needed:
  removing either makes `cargo deny` fail. deny.toml names the path that
  reaches each at the pin (D32 item 9 (e)).*
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
  *Amended 2026-10-12: the bans also refuse `winit` as a direct dependency
  of anything but `iced_winit`, from the fork or from crates.io, so that
  the build holds the fork's one copy (D32 item 9 (e)).*
- **Sources:** crates.io, and Rep-1 by git. Nothing else.
  *Amended 2026-10-12: the iced fork is a second git source, beside Rep-1,
  pinned by full commit hash (D32, items 1 and 9).*
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
**Recommended in D27, item 1 (2026-10-05):** the screenshots are committed
and CI checks that they are current; no action is added.

### D14. The renderings are committed as images, not as the supplied bundle

**proposed, pending the owner's confirmation.** The owner supplied the
renderings as one self-unpacking HTML file. Its packaging script carries
text that the owner's standing rule on attribution keeps out of this
repository, and that rule overrides the instruction to commit the
renderings unmodified. So `design/renderings/` holds each of the eight
frames as a PNG drawn from the bundle at its own size, unretouched, and
`design/INDEX.md` records the bundle's SHA-256 so the source can be
identified. The owner keeps the source file. **D27, item 14 recommends
confirming this.**

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

*Amended 2026-10-12: four checks are added for the iced fork (D32 item 9
(e)). Every `Cargo.lock` package from the fork is at the pinned commit and
is the only package of its name, which reaches `winit` and `dpi`; the lock
holds exactly one `winit`, the fork's; no manifest of the workspace names
`winit`, under any name or in any table; and the `iced` line keeps
`default-features = false` and exactly its ten listed features. The
existing check that every crate takes iced from the workspace's lines now
also refuses a crate's `[features]` entry that switches on a feature of an
iced crate (`iced/default`, `iced_runtime/selector`), which would add to
that list in every build that turns it on; a fixture test holds that
refusal to fixed manifests. Each was seen to fail on a broken copy:
`winit` from crates.io, `dpi` at another commit, a second `dpi`, `winit`
named in `app`, in `desktop`'s dev-dependencies under another name and in
`[workspace.dependencies]`, `default-features = false` removed, a feature
removed, and `iced/default`, `iced/unconditional-rendering` and
`iced_runtime/selector` switched on from `app`'s `[features]`.*

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
with the findings of `docs/spikes/P1-REPORT.md`. The owner chose the
fork: D32, item 1.

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

  *Amended (owner, 2026-10-13): one walk takes no cancel. A settle
  reconciles through the library's `settle_if_landed`, which cannot be
  stopped, and walks only when the reservation turns out unexplained
  between two reads. Settles now run without a press, and the owner
  accepts that narrow case; D38 item 6 records it.*

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

*Amended (owner, 2026-10-13): there is now a default node,
`https://api.mochimo.org`, which the person can change or forget. D43
supersedes this entry's "there is no default node"; the rest stands.*

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

*Amended (owner, 2026-10-13): "a walk to a far key index never holds the
lock back" has one exception the owner accepts, an automatic settle whose
reservation turns out unexplained between two reads (D38 item 6).*

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
- the phrase screen's warning (S4): the library's own words as `create`
  shows them, without their last sentence, about the terminal's scrollback,
  which a window does not have;
- the note under a library page that names a command-line verb, naming the
  control that does the same (D27, item 13);
- the application's own texts on the first-run screens, the opening
  screen, the lock notes and the stopped screen (docs/SCREENS.md lists
  them);
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

## Phase 3

### D27. Phase 3: recommendations for the open items

**proposed; recommended on 2026-10-05, when the owner asked for a
recommendation on every open item and for phase 3 to proceed on them.**
Each item says what was open, what is recommended, and why. Phase 3 is
built on these recommendations; each stays open to the owner's review in
the pull request that introduces it. docs/SCREENS.md maps every screen
to them.

1. **Screenshots are committed and checked, not uploaded (answers D13).**
   `cargo run -p tawara-app --example screenshots` draws every screen
   headlessly with iced's software renderer (tiny-skia) at the renderings'
   sizes and scale 1, from fixed sample data, into `design/screenshots/`.
   CI's Linux job draws them again and fails if any differs from the
   committed file. This keeps D13's rule (no third-party action), puts
   each screen next to its rendering in every pull request's diff, and
   keeps the images for as long as the history does. An artifact-upload
   action would be the one exception to D13, and its artifacts expire.
   The cost is the repository's growth with each changed image.
2. **Branding: the application shows its own name.** The renderings carry
   the MOCHIMO wordmark and coin mark; docs/PLAN.md section 6 says the
   application is not branded as an official product of the
   cryptocurrency. So the sidebar and the first-run panel show "Tawara" as
   a wordmark in Montserrat, and the coin mark appears nowhere: not as the
   lockup, the first-run pattern, or the card watermarks. The first-run
   panel keeps its green field, headline and feature pills, and says once,
   plainly, what the application is for ("A wallet for Mochimo"). A mark of
   Tawara's own is the owner's to supply.
3. **The renderings' conflicts with docs/PLAN.md section 4, and the safe
   version of each** (section 5: section 4 wins). Each is recorded with its
   screen in docs/SCREENS.md:
   - **"Advance to #33" (05) and "Reconcile" (02)** move a key index from a
     button on a divergence (section 4.9, and the owner's terms in D19).
     Both become a link to the account-recovery screen, reached on
     purpose, which shows every account's report first, makes the person
     type the index and confirm that no other wallet uses the seed. The
     05 callout's own words ("This phrase probably signed on another
     device") are replaced by the library's report.
   - **"Recovery phrase: Show" (05)** shows the phrase again; section 4.3
     shows it once, at creation, and the store keeps the seed, not the
     phrase. Not built.
   - **"Encrypted keystore file: Export" (05)** makes a copy of the store,
     the rollback that section 4.5 calls a key-reuse hazard. Not built.
   - **"Receipt verified: Merkle proof matches block root" (04)** claims a
     check nothing in the wallet makes. Not built.
   - **"Keystore password" before "Sign & submit" (03).** The store is
     already open, and the library cannot check a password against an open
     store. The confirmation is the library's own "NOT SIGNED" page (D26)
     and an explicit "Sign & submit" after it, on its own step; no
     password field.
   - **Account names ("Primary", "Savings") and the "Default" badge
     (02-05).** The store holds no names, and the library does not say
     which derived account a tag is (its derivation number is private,
     so that nothing outside can fabricate one). Each account is named by
     its destination's first and last six characters, as the renderings
     shorten tags elsewhere ("9xQmT4…3cYdAf"), with "derived" or
     "imported" beside it; the "Default" badge is not shown. Item 11
     proposes the library change that would let a derived account be
     "Account N", as the command line's `restore --account N` names it.
   - **"1 send settling · -1,250.000001" (02).** The store keeps no amount
     for an open reservation (the library's specification says so), so the
     chip counts outstanding spends without an amount.
4. **Controls the library cannot back yet are left out, not faked.**
   "Import a legacy .mcm file" (01, D23 item 6), "Password: Change" (05),
   the mempool, a block's haiku, difficulty, nonce, root and type, the
   average solve time and the next neogenesis (06, 07), the network's name
   and "Transaction search: Indexer available" (05). Item 11 proposes the
   library changes that would back them.
5. **Controls that need a file or camera dialog are left out of phase 3.**
   "Import CSV" and "Scan QR" (03), "Export CSV" (04). A native file
   dialog is a dependency of its own (and on Linux a portal or toolkit);
   it is proposed when one is needed. The retry artifact still has to be
   saved (section 4.9): "Save artifact" writes it to the Downloads folder
   under a new name (never overwriting), shows the full path, and offers
   to copy the hex to the clipboard. The artifact is not secret: it is
   the bytes the node is sent.

   *Amended 2026-10-13, to follow the owner's decision of that day in D32
   item 10 (i): the last sentence above is withdrawn. The artifact is the
   bytes the node is sent, but while the reservation is open they are the
   only bytes that can move these funds, as `crates/wallet-core/src/event.rs`
   says and as the send page and the report tell the person
   (`crates/app/src/screens/wallet/send.rs`, `report.rs`). The owner had
   the same claim reworded in Tawara-mobile's comment on copy.*

   *Amended (owner, 2026-10-13): "Scan QR" is built on phones only, with the
   phone's camera (D42). The desktop's Send has no scanner.*
6. **Fonts.** Static TTF instances of the renderings' three families,
   bundled in `crates/app/fonts/` with each family's SIL Open Font
   License 1.1 beside them: Poppins Regular, Medium and SemiBold and IBM
   Plex Mono Regular and Medium from `google/fonts`, and Montserrat
   Medium, SemiBold, Bold, Medium Italic and Bold Italic from the family's
   own repository (Google Fonts ships Montserrat only as a variable
   font, which iced would draw at its default Thin instance). They are not
   subset: IBM Plex's licence reserves its name for unmodified files.
   Poppins has no "→" (U+2192), which the renderings draw from a system
   font; it is set in Montserrat, which has it. No text falls back to a
   system font, so the screenshots are the same on every machine, and
   item 1's check fails if any does.
7. **Icons are drawn, not loaded.** The 25 line icons are drawn from the
   path data in `design/TOKENS.md` section 6 with iced's `canvas`
   feature, which adds six crates (`lyon` and its parts). iced's `svg`
   feature would add 35, among them GIF and WebP decoders the icons do not
   need. Both pass `cargo deny`.
8. **Dark only.** The renderings are all dark (docs/PLAN.md section 5), so
   there is one theme and the 05 "Theme" control is not built.
9. **Width classes.** Phase 3 builds the expanded layout the renderings
   show (1440 px wide) and a medium one down to 1024 px, where two-column
   rows stack. The compact (phone) layout is proposed with the mobile
   shells in phase 4, as `design/INDEX.md` asks.
10. **Preferences are kept in a small file of their own.** The node's
    URL, the folder of the store last made or opened, the amount unit (MCM
    or nanoMCM, 05) and the auto-lock period are remembered in
    `preferences`, a plain text file in the application's folder beside
    `keystore/`, never inside it. Nothing secret goes there.
    Without it, the person would choose their node again at every start,
    and a store kept outside the default folder would not be offered for
    unlocking at the next one. A folder typed as a relative path is made
    absolute before it is used, so the store is remembered by where it is,
    not by where the application happened to be started from.
    The auto-lock choices are 1, 2, 5, 10 and 15 minutes, 5 by default
    (D24); there is no "never".
11. **Library changes proposed for later phases** (none blocks phase 3;
    written up in docs/LIBRARY-PROPOSALS.md):
    - the block's own metadata (haiku, difficulty, nonce, root, size) and
      its type (normal, pseudo, neogenesis), which the node sends and the
      parser drops (06, 07);
    - a read of the mempool (06);
    - an offset for a tag's history, which stops at 100 rows today (04,
      08);
    - the network's identifier and the node's sync state (05);
    - progress for a status read (D26), for a far scan on the recovery
      screen;
    - a derived account's number, read from the store's record, so an
      account can be named as `restore --account N` names it (02-05);
    - changing the store's password (05), if the owner wants it;
    - importing the older wallets' `.mcm` files (01), before release if
      moving people from them is a goal (D23 item 6).
12. **D26's three open choices.** (1) Unlocking and creating are stopped
    only by a cancel: the person is waiting on them, and the wallet
    opening inside them walks no further than the library's diagnostic
    scope. Keep. (2) A cancelled restore or advance answers with its own
    reply and the store open on its own: the screen shows the text and the
    store as it is. Keep. (3) The worker's texts: kept as written, since
    each says what happened and nothing more; docs/SCREENS.md lists every
    one with the screen that shows it, for the owner to change any.
13. **The library's pages are shown whole.** A library page (a refusal, a
    divergence report, the send and planned pages) is shown word for word
    in IBM Plex Mono, its line breaks kept, never truncated. Where it names
    a command-line verb, a note under it names the control that does the
    same (D25): `settle` is "Settle", `resign` is "Re-sign", `submit` is
    "Submit saved artifact", `reconcile ... --advance-to N` is the
    account-recovery screen. *Superseded by D28 for how a page is shown:
    it is now one click behind a summary; when shown, it is as described
    here.*
14. **D14: the renderings as images.** Recommended to confirm: the eight
    PNGs are drawn from the bundle unretouched, the bundle's hash
    identifies the source, and the owner keeps the bundle.
15. **Where the screens differ from the tokens, and why** (`design/TOKENS.md`;
    every other value is the renderings'):
    - **No letter spacing.** iced sets text solid, so the section labels'
      0.12 em and the password field's 0.08 em are not drawn.
    - **No tabular figures.** iced does not switch on a font's `tnum`
      feature, so amounts are proportional; they are right-aligned in their
      columns, as the renderings align them.
    - **A focus ring.** The frames define no focus style. A focused field
      draws its border in the accent, so a keyboard user can see where they
      are. Tab and Shift-Tab move between a form's fields: iced's fields
      leave the key to the application, which asks for the next or the
      previous one.
    - **Amounts with all nine places.** The renderings show some amounts to
      six places ("9,215.402118"); every amount here keeps its nine places
      (`12,480.537214906`), so no figure is rounded.
16. **Order of work.** Four pull requests: (a) this entry, the screen
    inventory, fonts, theme, icons, the application shell, preferences,
    the screenshot check and the first-run screens; (b) the wallet:
    dashboard, receive, adding an account, the send flow, the account
    states of section 4.9, settling, re-signing and submitting a saved
    artifact; (c) activity, settings and account recovery; (d) the
    explorer. *In (b), proposed:* the dashboard's recent activity and its
    network card (the last block's age, the latest blocks) need the
    explorer's reads, which the worker does not make yet, and turning a
    tag's history into rows is Activity's own question (W10); both come
    with (c). The sidebar's network panel shows the node, the tip and the
    latency meanwhile. *Built in (c): D29.*

    *Amended (owner, 2026-10-13): a spend now also settles without a
    press, once the chain shows it landed (D38). That W7's Settle stays as
    a fallback is recommended there.*

### D28. The library's pages: a banner, a summary, then the page

**owner; decided on 2026-10-05, on seeing PR (b)'s screens, which showed
the library's pages whole in a terminal face at the top of the window.**

The owner's words: "Slim warning banner that opens up into a summary,
with a link at the bottom of the summary that opens up the full
reproduced library output", the summary "along with common causes so
they don't freak out".

So every library page on a wallet screen is shown in three steps:

1. **A slim banner**, closed: one line saying what happened ("1 account
   paused: look at it before spending from it", "Written to the node's
   socket: not yet accepted by the network"), in the warning colour where
   something stopped or needs the person, in the accent colour where it
   is something to know.
2. **Opened, a summary** in Tawara's own words (D25's rules for the
   application's texts), with the causes that commonly lie behind it,
   so that what is usually ordinary (an account not paid yet, a store
   restored from a backup) does not read as an alarm.
3. **At the summary's foot, a link** to the library's full output, shown
   word for word as D27 item 13 describes.

What this changes: D25's "the library's pages are shown word for word"
and D27 item 13 now describe the third step, not what the screen shows
first. docs/PLAN.md section 3 still holds: the library's protective text
is reused, not rewritten, and is always one step from the summary; the
summary says what the page says and nothing it does not.

How it is kept honest:

- **The summaries are built from what the worker reports, never from
  reading the page.** wallet-core now says what a store's notice is about
  (`view::Notice` and `NoticeKind`: no node, node changed, node silent,
  cancelled, node refused, not whole, will not start) and what kind of
  divergence an account has (`view::DivergenceKind`: the chain ahead or
  behind by so many keys, unlocated, an unexplained reservation, not
  found, unreachable, no master seed, reconciling failed), from the
  library's own types.
- **The causes are the library's**, from its reports and its types'
  documentation: a chain ahead is an older copy of the store (a backup, a
  restore) or a second wallet on the same recovery phrase, and never a
  crash during a send; "account not found" is never paid, emptied to
  zero, or a lookup that failed; and so on, case by case
  (`crates/app/src/screens/wallet/report.rs`).
- **The worker's own texts** (no node chosen, node changed, reconciling
  cancelled) are already Tawara's words: their summary is the text, and
  there is no further page to open.
- **The sent page opens its summary from the start**: its three facts
  (a submission is a socket write, not acceptance; the node's checks run
  after Tawara has gone; the signed bytes can be lost, so save them) are
  the page's point and are never softened (docs/PLAN.md section 4.9).
  Every other banner starts closed.
- A refusal that reads as a line stays a line; a longer one is a banner
  whose summary is its first paragraph.

### D29. Phase 3 (c): activity, settings and account recovery

**proposed; items 1, 3, 4, 5 and 8 decided by the owner on PR (c)'s
reviews, and items 2, 6 and 7 accepted by the owner on 2026-10-06.** PR (c)
builds docs/SCREENS.md W10 to W12 and the dashboard's two
cards that D27 item 16 moved here. The choices it makes:

1. **The explorer reads go on in the background** (the owner kept this,
   on PR (c)'s first draft). The index's rows
   (`Command::Activity`, one request per account) and the newest blocks
   (`Command::Blocks`) are read when a store opens, on Refresh, and when
   Activity opens, without the busy card: the screen shows the last answer
   while the next comes, and a refusal that is not the node's answer (a
   cancel, no node) leaves it. They are commands like any other, so they
   do not keep the store open (D24), and a lock drops what the index held
   for the store. Each stops between two of its requests when a cancel, a
   lock or the idle period reaches it, since the worker that makes them
   holds the open store meanwhile: the history between accounts, and the
   newest blocks between the tip and each block. The library's walk of the
   newest blocks (`cli::cmd_blocks`) looks for no stop, so the worker makes
   that walk itself, repeated as D19 says, and has the library write the
   page from the same outcome (the owner's review of PR (c)).
2. **A row is what the transaction did to the account,** net of its
   change, by the command line's own sums: the index debits a spend's
   source gross and lists the change as a destination of its own
   (`cli::render`, `recent_transactions`; the rule is private there and
   repeated in `wallet-core::explorer`, naming it, as D19 does for the
   others). A transaction that left one of the store's accounts is in the
   rows of every one of them it paid as well, and is listed once, from the
   account it left: a transfer between two of them, or a spend that paid
   one of them among others. One that left none of them, a payment from
   elsewhere to several of them, is listed once for each account it
   reached, with what it did to that account and that account's
   references (the owner's review of PR (c)); a row is chosen by its
   transaction and its account together. The account it left is read from
   its own source operation, not from whose history held it, so a row is
   the same whichever pages have been read: a send from an account whose
   older pages are not read yet is that account's send, debit and all,
   when only its payee's page holds it (the owner's second review). A
   transfer to several of the store's accounts at once is one row whose
   amount is theirs together, so its title calls it a batch rather than
   naming one of them for the whole, and the line under it names the
   sender and every recipient (the owner's third review). The words
   (sent, received, between own accounts, mining reward) are Tawara's;
   they say nothing a row does not.
3. **References come from the index's rows** (the owner, on PR (c)'s
   first draft). The first draft said the index carries none, after the
   library's codec, which notes that `metadata` is absent on every
   `/search` operation; that note generalises from the one transaction the
   library captured live, which had no reference. At the pinned middleware
   (`mochimo-mesh` at `ddc1ee5`) the indexer records each destination's
   reference (`indexer/transactions.go:235`) and `/search` returns it as
   `metadata.memo` when it is not empty (`indexer/search.go:143`), and the
   library already reads it there (`Operation::memo`). A read of each
   transaction's block, built on the wrong premise and taken out again,
   would only repeat it. A row shows its own references: for a payment
   received, those on the destinations that reached the account; for
   anything else, those on the destinations it paid, the change left out.
   Where a row carries none, the transaction's id is shown under it, and
   no reference is inferred. Both endpoints send a reference as its whole
   sixteen-byte field, padded with NUL bytes; the padding is left off, and
   any other text the node sends is made safe to show by the command
   line's rule for external text (`cli::terminal_text`, repeated
   likewise).
4. **Dates are shown in the system's time zone** (the owner, on PR (c)'s
   first draft, which showed UTC). The library carries no calendar and
   prints the raw count, so the application reads the zone with `chrono`,
   its `clock` feature alone: chrono reads the zone itself (the TZ
   variable or /etc/localtime on Unix, the system's API on Windows, the tz
   data on Android) and never calls the C library's `localtime`, which is
   unsound beside other threads; the `time` crate refuses a local offset
   in a program with threads for that reason. It adds three crates
   (`chrono`, `iana-time-zone` and its Haiku shim), all MIT or Apache-2.0;
   the rest of what it needs was already in the graph. A transaction's
   full time names its offset ("UTC+2"). The screenshots are drawn in UTC,
   so they are the same on every machine. The age of the newest block is
   measured by a clock that moves each second, on a thread of its own, as
   the worker's events are (iced's thread-pool executor keeps no timer).
5. **Account recovery shows every report's summary first, and the
   advance waits until every full report has been opened** (the owner, on
   the same draft, which opened every account's whole library page from
   the start). D19 asks for the library's whole report for every account
   before anything else; D28 puts each library page behind a banner, then
   a summary with the common causes, then the library's own words. On
   W12 each account opens to its summary, so the page reads as D28's
   other pages do, and the advance is not offered until the person has
   opened every account's full output at least once since the reports were
   read: the evidence that one account's advance is wrong is most often in
   another's report, so it is seen before anything moves. Each account
   says whether its full report has been opened, and the advance says how
   many of them have. A report that changes (a further search answers) is
   folded back to its summary and is to be opened again; reading every
   account again starts the count over. The page reads every account
   afresh when it opens (`Command::Review`, which applies what it finds
   only once all have answered) and again after an advance.
6. **What account recovery offers.** The advance, only for an account
   whose live report names the index, once every full report has been
   opened (item 5), typed by the person and sent only once they confirm
   no other wallet uses the recovery phrase; the library
   advances only to exactly that index or refuses. For an account whose
   chain address was not among the keys searched, a wider search to a key
   index the person types (`Command::Status` with `scan_to`), which writes
   nothing. Restore stays in W3, as D19 has it.
7. **Settings takes effect at once.** The auto-lock period reaches the
   running worker (`WorkerHandle::set_idle_lock`) and counts from the
   person's last input, so a shorter period can lock straight away. About
   names the wallet library's commit (`wallet-core::LIBRARY_REV`), which
   the policy tests hold to the workspace's pin, since the library carries
   no version of its own to read. The keystore card says how a new store's
   key is derived (`keystore::Kdf::RECOMMENDED`); the library does not say
   it for an open store, so the card does not claim it.
8. **A derived account's number is shown** (the owner, on the same draft),
   beside its kind: "derived · account 3", the command line's own name
   for it (`restore --account N`). The library keeps the number in a
   record it does not show (docs/LIBRARY-PROPOSALS.md, 9a), so the worker
   finds it as the command line makes it: account N's tag is
   `derive::derive_account_tag(master, N)`, a public call, so it derives
   0, 1, 2 and on from the session's seed until every derived account is
   found, or until the discovery sweep's ceiling (1,024), and remembers
   what it found while it runs. Nothing is asked of the node or written,
   and nothing is guessed: an account not found within the ceiling shows
   "derived" alone. Proposal 9a stays, so the number can be read rather
   than searched for. Accounts are still named by their shortened
   destination (D27, item 3).


### D30. Phase 3 (c): the library's explorer reads on the wallet's pages

**proposed; accepted by the owner on 2026-10-06.** With the pin at
`e41f8c1` (D7), the library serves
docs/LIBRARY-PROPOSALS.md items 5 to 8, and the controls D27 item 4 left
out of W1, W10 and W11 for want of them are built. The explorer's own
pages (E1, E2) stay PR (d)'s. The choices:

1. **The network card shows each block's kind, the difficulty and the
   queue** (rendering 02). A block's kind is the library's
   (`MeshBlock::kind`, the C reference's own test): a neogenesis tile is
   green, a pseudo-block's amber and a normal block's raised, with the
   rendering's legend under them. A block the node sent no figures for has
   no kind and is drawn plain, never as normal; the legend is shown only
   when some block has a kind. The difficulty is the newest block's.
   "Mempool" is how many transactions wait in the node's queue
   (`Command::Mempool`, the command line's `mempool` with none read whole:
   one request). A queue the node did not answer for shows "—"; the queue
   is the node's own and not its index, so its refusal never reads as "no
   index".
2. **The node card names the network and says the middleware's sync
   state** (rendering 05). `Command::NetworkStatus` now reads
   `/network/status` whole (`MeshClient::network_status_full`): still one
   request, so the latency shown is unchanged, and it also gives when the
   tip was solved and the Mesh middleware's sync state, which the card
   shows in the middleware's own words, green when its last refresh
   finished and amber when it did not. The library is plain that this
   state is the middleware's view of its one node and not whether the node
   is current with the network, and the card says so under it; the tip's
   tile says how long ago the tip was solved, which is what shows a chain
   that has stopped moving. A reply without the tip's timestamp or the
   genesis block is refused, as Rosetta requires both. The network's name
   ("mochimo · mainnet") is `/network/list` (`Command::Networks`), asked
   for when a node is chosen and the tip is asked for, one request at a
   time, until the node has said; another node is asked again. The
   sidebar keeps the node's host.
3. **Activity reads older rows a page at a time** (W10). An account's
   history is read from the newest, 100 rows a page, and "Read older"
   reads the next page of every account the index holds more of
   (`Command::OlderActivity`, the command line's `recent-transactions
   --from`), from the offset its last page ended at
   (`AccountHistory::next`). A row the index gained at the top since the
   first page pushes the rest down, so a later page can repeat a row
   already read: it is listed once, and the offset still counts it, so
   the next page starts where the index's own ordering says. Nothing is
   read without the person asking, since a mining account can hold a row
   for every block it won. A read again (Refresh, or opening Activity)
   starts from the newest page and drops the older pages read before:
   joining them to a fresh first page would need the index's ordering not
   to have moved by more than a page between the two reads, and a gap
   between them would be a history cut short shown as a whole one.
4. **A refused search says whether the node runs an index** (the owner
   asked for the reading to be fixed in the library and here). The
   library, and wallet-core after it, read the middleware's code 1 as "no
   indexer". At the pinned middleware (`mochimo-mesh` at `ddc1ee5`) code 1
   is *Invalid request*, a body it could not decode. A deployment that runs
   no indexer never registers `/search/transactions` (`main.go`), so it
   answers that route with HTTP 404; one that runs an indexer answers code
   2, its internal error, while the indexer's database is not connected and
   when a search fails (`search_handler.go`). So Activity now has three
   refusals: a 404 is "This node keeps no transaction index", as before; a
   code 2 is "This node's transaction index did not answer", with reading
   again later and another node as what helps, since the node itself
   answered; anything else is the node not serving the read, as before.
   The blocks and the queue are the node's own, not its index, so their
   refusals say nothing about an index, though the middleware answers code
   2 for a queue it could not read as well. The library's fix is Rep-0's
   pull request #17: the three search reads get an outcome of their own
   (`Outcome::SearchFailed`), whose page says which of the two it was. The
   pin reached it at `7cdc2e9` (D7): the worker matches `SearchFailed`, and
   the library's page under each summary says which refusal it was.

### D31. Phase 3 (d): the explorer

**proposed; decided by the owner on 2026-10-06.** The fourth of D27 item 16's pull requests builds the explorer's
pages, docs/SCREENS.md E1 to E4 (renderings 06, 07, 08 and 04's detail),
on the library's reads at `7cdc2e9` (D7). The choices:

1. **Every read is walked in the worker and stops between requests.**
   None of the library's explorer calls takes a stop, and the worker that
   answers them holds the open store, so each walk is the worker's own,
   consulting the request's stop before every request, as the network
   card's walk of the newest blocks does (D30): the chain (the tip, eleven
   blocks and the block a hundred below the tip), the queue (its ids and
   five transactions), a block (the block, the block below it and the
   tip), a tag (the ledger's entry and a page of the index) and a hash (the
   index, then the node). A walk cut short answers as cancelled and shows
   nothing as a whole read. A block of the newest blocks' walk not served
   refuses the whole, as the library's walk does; a figure only one extra
   request gives (the average, a block's solve time, its confirmations) is
   left out when that request is refused.
2. **The figures the node does not send are worked out, and the page says
   how.** A block's solve time is its time less the time of the block
   below it, whose hash it names. E1's newest blocks are read one by one,
   so the chain can reorganize between two reads: a block whose parent's
   hash is not the hash of the block read below it has no solve time
   (PR #12's second review), as E2's block has none when its parent is not
   the one it names. The average solve is the tip's time less
   the time of the block a hundred below it, over a hundred, so pseudo-
   blocks count as blocks. The next neogenesis is the next block number
   whose low byte is zero, the library's own test for one
   (`MeshBlock::kind`). Confirmations are the tip less the block, plus
   one; a transaction opened from its block's page counts them from the
   tip read with the block or the explorer's, whichever is higher, so
   opening it never counts fewer (PR #12's third review). A block's fees are every fee operation in it, summed, and what it
   moved every destination of its transactions other than the reward, as
   the command line's block page sums them; its transactions are counted
   besides the reward. A block's least fee is the node's figure, not its
   fees. A time is any 64-bit number the Mesh parser takes, so a
   difference that overflows, or a block older than the one below it, is
   no solve time and no average, never a panic of the worker or a
   negative time; a number at the end of the range has no block beyond it,
   and no confirmations past it (PR #12's review).
3. **The search field reads as the command line reads the same
   arguments** (`cli::args`, private there and repeated in
   `explorer::Query`). A number is a block, and block 0 is refused, since
   the node serves 0 as its newest block. Sixty-four hex digits, with or
   without `0x`, are a hash: the node's index is searched for a
   transaction with that id first, then the node is asked for a block with
   that hash, since a block is served by its hash only from the
   deployment's archive and nothing in the digits says which they are. A
   tag in hex needs its `0x`, since the second half of a ledger address
   printed as eighty hex digits is forty hex digits too; an address goes
   through its checksum; the all-zero tag is refused, as its checksum
   cannot catch it. A hash that names nothing stays on E1 with what the
   index and the node each answered, the index's "holds no transaction
   with this id" said as not the same as there being none. E1 takes only
   the answer for the hash it is still looking for: one for an earlier
   search, asked before E1 was opened again, is dropped (PR #12's
   review).
4. **The haiku is the node's.** The middleware sends a block's haiku, and
   the library shows it for a normal block only; it does not decode it from
   the nonce itself. E2 shows it as the node sent it, made safe to show,
   for a normal block only, and says so; under it, for correctness, it
   says that Tawara does not check it against the block's nonce (the
   owner: as the node sent it, with that note).
5. **A tag is read from the ledger and the index, and the node's code 4 is
   never read as an account that does not exist** (the owner, 2026-10-06,
   after the item was explained further). The middleware answers
   *Account not found* for a tag the ledger holds no entry for, for one
   held at zero, and for a lookup that failed (the library's `recon`, fact
   3), so nothing tells them apart. E3 shows a banner, *Account not found
   or lookup failed*, which opens into a summary of why it can be: not
   paid yet, emptied or held at zero, or the lookup failed or the node
   serves another network (the owner's wording). "Current key on ledger"
   is shown only for an account in this store, as W11 says it, and its
   tile is not drawn for anyone else's: a ledger address does not reveal
   its key's index (the owner: hide the tile). The ledger address is
   shown as the tag and its key's half, said not to be somewhere to send
   funds. "Send to this account", which opens W4 with the address (Base58,
   with its checksum) as the destination, is offered only for an account
   whose address or tag the person typed into the search field: an
   account reached through a link was named by the node's answers, and a
   node could name its own account where the person expects another's
   (the owner's choice of three). "Copy" copies the address, wherever the
   account came from.
6. **The explorer is read when asked, not on a timer.** Rendering 06's
   "Live · updated 3s ago" would mean asking the node in the background
   for as long as the page is open. The chain and the queue are read when
   E1 opens and on "Read again", and the chip says how long ago they were
   read.
7. **"View all pending" is built; "All blocks" is not** (the owner). E1's
   queue card leads to a page of the node's whole queue: every transaction
   waiting, read whole up to the command line's most for one read (100,
   `mempool --count`), the rest counted, each opening whole (E4). A block
   that is not among the newest is found by searching for its number or
   hash, so there is no listing of the whole chain. 06's line on when a
   pseudo-block is made is not shown: it names a time the library does
   not.
8. **The pages link to each other.** A block's number, an account, the
   previous block and a transaction open their pages; a ledger address is
   not an account and opens nothing. "Yours" on E2 and "Your account" on E3
   say only that the account is one of the open store's.

## Phase 4

### D32. Phase 4: the mobile shells

**owner, 2026-10-06 (items 1 to 4, and item 8's UIScene half); proposed
(items 5 to 7, and item 8's Android half).** Phase 4
runs the same `tawara-app` on Android and iOS (docs/PLAN.md section 7),
on the three conditions D18 left open.

1. **iced comes from the owner's fork, pinned by commit** (D17, D18
   condition 2; the owner chose the fork, and made it on 2026-10-06 as
   `patricksmithlaravel/iced_mobile`). Its branch `tawara/0.14-mobile`
   starts at iced's own `0.14` branch, `38237dd`, whose sources are, crate
   for crate, the crates.io releases D6 records (compared file by file
   for all twelve iced crates in the lockfile), and adds one commit,
   `0b02176`: D17's patch, and `on_lifecycle`, a hook called when winit
   reports the application suspended or resumed (item 6). Every iced
   crate comes from that commit, the desktop's included, so the build
   holds one copy of each, and every screenshot draws as before;
   `deny.toml` admits that one git source beside the library's, and a
   policy test holds the iced lines to it as D15's holds the library's (a
   full commit hash and nothing else to choose it, the same commit on
   every line, every crate taking the workspace's line, and the lockfile
   resolved there). This replaces D6's "no git revision of iced is used".
   The lockfile names the crates by the branch's own versions (0.14.1, and
   0.14.2 for `iced_widget`). The fixes are offered to iced upstream;
   when a release carries them, the pin returns to crates.io.
   *Amended 2026-10-12: Tawara now pins the fork's release
   `v0.14.1-mobile.2`, `d4ab3a057db96162b087a27dd0752a3b15abfa6b`, not the
   branch `tawara/0.14-mobile`. The return to crates.io above is
   superseded: later moves go only to fork tags (D32 item 9 (a)).*
2. **Text entry is checked on real phones on phase 4's own builds**
   (D18 condition 1; the owner's choice). Before the phase's last part
   merges, the owner runs the device steps on its Android build (arm64)
   and, with an Apple developer account, on an iPhone: accented and
   non-Latin text, composition, the password field's keyboard
   (suggestions and learning), and paste from another app. If composition
   or secure entry fails, native text-input glue becomes a part of this
   phase.
3. **iOS: the store is protected with `NSFileProtectionComplete`** (plan
   section 4.6; the owner's choice). The store cannot be read while the
   phone is locked; the wallet locks when it leaves the foreground (item
   6), so it never needs the store then. The store directory also carries
   `NSURLIsExcludedFromBackupKey` (section 4.5).
4. **Android: `FLAG_SECURE` on the whole app** (plan section 4.3; the
   owner's choice, wider than the plan's "every screen that shows a
   secret"). It is set once on the window: no screenshot or recording of
   any screen, and the recent-apps thumbnail shows none of the balances,
   addresses or history, as iOS hides everything from its app switcher
   (item 6). Copy still copies an address.

   *Amended, proposed (2026-10-13, D44 item 4): "no ... recording" is what
   the flag asks of the system, not what every device gives. Android's
   guide says that in some cases the flag "does not correctly predict if
   screen recording is active", and that on Android 11 (API 30) and lower
   it helps around 70% of devices reliably, because on some devices
   keyboard taps can be recorded. No comment or text says that it stops
   every recording.*
5. **The phase in three parts.** (a) The shells: the fork's pin;
   `crates/mobile` running the application on Android (NativeActivity,
   no Java, the APK built with aapt2, zipalign and apksigner as phase 1's
   was) and on iOS (the binary as the bundle's executable); the store's
   location, protection and backup exclusion; `FLAG_SECURE`; the iOS
   privacy cover; the lock on leaving the foreground; the clipboard
   through platform calls; and CI jobs on an emulator and a simulator
   that start the application, draw its first screen, send it to the
   background and back, and check that it locked. (b) The phone layout:
   safe-area and keyboard insets, the compact layout of every screen
   (there are no phone renderings, so it is proposed with screenshots for
   the owner's approval, as D27 said), the keyboard shown again on a tap
   of the focused field, and lists that act on release. (c) Text entry,
   as item 2 finds.
6. **Leaving the foreground locks the wallet on both platforms** (plan
   section 4.1). On Android the patched shell passes `Suspended` to the
   application; on iOS, where iced hears nothing of the switch (phase 1,
   I6), the shell observes UIApplication's notifications. On either, the
   application calls `WorkerHandle::background` (D24) and draws a cover
   with no wallet content until it is back, so iOS's app-switcher
   snapshot shows nothing (section 4.3).

   **And wipes what is typed (PR #13's review, [P2]).** Locking the
   worker is not enough. A password typed on S7 and not yet submitted, or
   a recovery phrase typed on S6, lives in the interface, and was wiped
   only on `Event::Locked`; with the store already locked there is
   nothing to lock and no `Locked`, so the password survived the switch
   and Unlock opened the wallet after the return. The rule is now: every
   move to the background is reported to the interface, whether or not
   anything was open (`Event::Backgrounded`, after any `Locked`), and on
   it the interface wipes every secret field of the screen showing, in
   place: the passwords, the recovery phrase and the words typed to
   confirm one. One function lists those fields (`wipe_typed`), for
   leaving a screen and for leaving the foreground; it names every
   screen, so a new screen with a secret has to be added to it.

   **Before any further input, whatever the worker is doing (PR #13's
   second review, [P2]).** `Backgrounded` comes through the worker's
   queue, behind a command still running (a node read at start, for
   one). Waiting for it let the person return and press Unlock first: the
   password typed before leaving went into a new command, past the
   background's cancellation, and opened the wallet after the lock. So
   the shell's `left_foreground` now also counts the move, on its own
   thread, before it asks the worker to lock; and the application, before
   it looks at any message, wipes what is typed if the count has moved
   since the last one. And a secret's text is taken only from a field
   drawn after the latest such wipe (PR #13's third review): iced takes a
   whole batch of messages against the fields as last drawn before it
   draws them again, so every message from a field drawn before the move
   carries what was typed then. Each secret's text carries the generation
   of the field it came from (`Typed::drawn`, the moves the model had
   acted on when it was drawn), and any whose generation is not the
   current one is refused and wiped, however many come, until the field
   is drawn again.
   `Backgrounded` still wipes at once when the worker is free, so a typed
   secret does not wait in memory for the person to come back.

   **And is waited for on iOS (Tawara-mobile#1's review, [P1]; owner,
   2026-10-06).** Asking the worker to lock is not the lock: it locks
   after the command it is running, and a node request (up to the
   transport's timeout) or a spend between its reservation and its
   submission can hold it, while iOS may suspend the process as soon as
   the notification returns, with the store's key in memory.
   `WorkerHandle::background` therefore returned a `Locking`, which the
   worker answered once the session and everything pending were dropped,
   and `left_foreground` hands the shell a `Leaving` (since made to wait
   for the interface instead; see below). The iOS
   shell asks UIKit for background time (`beginBackgroundTask`), waits on
   it off the main thread, and ends the task when the lock is done. If
   the time runs out first, the owner chose that the process ends itself
   rather than be suspended holding the key: nothing persistent is lost,
   since the move to the background cancelled what could be cancelled and
   the library's key index only moves forward and is written before a
   spend is signed, so a spend cut off between reservation and
   submission is reconciled at the next unlock. Of the options put to the
   owner, letting iOS suspend and locking on the return was declined, and
   so, for now, was the same bound on Android.

   The lock waited for includes the interface's own wipe
   (Tawara-mobile#1's second review): a password or phrase typed, and a
   recovery phrase shown, live in the interface, which clears them on its
   next message, not when the worker is done. So the application counts
   the moves it has wiped for, and `Leaving` is done only when the worker
   has locked and the interface has wiped for that move. The clock's tick
   each second is such a message, so it comes within a second even while
   the worker is busy. A recovery phrase shown or being confirmed is
   dropped by that wipe, back to S3 with the note the worker's `Locked`
   would give; the folder chosen stays. With no background time at all,
   the shell's only thread to wait on is the main one, which the
   interface's wipe needs: the process ends there at once.

   **What "done" means is the interface's taking of the worker's answer
   to the move (Tawara-mobile#1's third review).** The wipe above can come
   before an answer still on its way: the app leaves while the worker is
   making a recovery phrase, the next tick wipes the fields, and then the
   phrase arrives and is shown, until the `Locked` after it drops it. So
   each move now has a number (`WorkerHandle::background` returns it, and
   its `Event::Backgrounded { seq }` carries it), the worker sends that
   event after the command it was running has answered and after the lock,
   and `Leaving` is done when the interface has taken it: by then it has
   taken every event before it, the phrase and the `Locked` that dropped
   it among them, in order. That also says the worker has locked, so the
   `Locking` the worker answered separately is gone. The fields' own wipe
   on the next message stays, for the race with input above. A worker that
   has stopped sends no more, and its `Stopped` comes after it dropped
   everything, so taking `Stopped` lets every move go. Everything includes
   the commands still queued when it ended, a password to unlock with or a
   recovery phrase to restore from among them (Tawara-wallet#15's review):
   the queue was held by the worker's thread itself and dropped only after
   `Stopped` had gone out, so it is now owned by the guard that sends
   `Stopped`, which drops it first, on a panic too. A dropped queue drops
   what is in it at once, though the application's handles still hold
   senders, and refuses what they send after. With no worker at
   all, the fields' wipe is what is waited for.
7. **Where the store lives.** Android: `no_backup/` (D21), with
   `android:allowBackup="false"` and `dataExtractionRules` that exclude
   every domain from cloud backup and device transfer, so the store
   relies on neither guard alone. iOS: `Library/Application Support`, as
   item 3 protects and excludes it.

   **And only there (Tawara-mobile#1's third review).** The folder field
   of S3, S6 and S7 was free, and only the default folder was in the
   protected place: a store made in a sibling such as `Library/Application
   Support/SecondWallet` would have had neither iOS's file protection nor
   the backup exclusion, on any launch. In a mobile shell the application
   now takes a store's folder only strictly inside the app-private folder
   the shell gives (`Host::private_dir`: iOS's `Library/Application
   Support/Tawara`, Android's `no_backup/`), as the file system resolves
   what exists of it, so neither `..` nor a link leads out. Anything else
   is refused on the screen, with nothing sent and nothing typed taken.
   The desktop is unchanged.
8. **What waits on winit.** An Android activity destroyed while its
   process lives cannot start again (phase 1, A18), and winit has no
   UIScene life cycle, which Apple's TN3187 says apps built with SDKs
   after iOS 26 must adopt. Both are tracked upstream (winit 0.31 is in
   beta at this writing). They are reviewed again at release readiness
   (phase 5): if UIScene is still missing then, an iOS release built with
   a newer SDK waits on it, and plan D3's fallback is put to the owner.

   *Amended (decided under the owner's delegation, 2026-10-12): the sentence
   above that says an Android activity destroyed while its process lives
   cannot start again no longer holds at the new pin, `v0.14.1-mobile.2`. The
   fork's winit patch (`android-exit-on-destroy`) lets the event loop be
   created again in the same process: the loop ends on `Destroy`, and
   dropping it allows a new one, so a new activity can run its own
   `android_main`. Tawara still ends the process after `Destroy`, since a
   same-process restart would need the rebuilding that item 9 (d) declines.*

   **UIScene: owner, 2026-10-06.** What TN3187 warned of was measured on
   phase 4's own build, on a Mac with Xcode 27. Linked with the iOS 27
   SDK, the app is stopped by UIKit at launch on the iOS 27 simulator
   (`EXC_BREAKPOINT` in `_UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption`,
   as the scene is created), after the store's directory has been made
   and protected. The same binary runs on iOS 18.3, and runs on iOS 27
   when its load command says SDK 26: the check is on the SDK the app is
   linked with, not on the phone's iOS. CI builds with Xcode 26.6, so it
   stayed green, and would have turned red the day the runner's default
   Xcode became 27; and Apple requires the newest SDK for App Store
   uploads some months after each release.

   Of four ways put to the owner (guard the build and fix later; guard
   and defer to phase 5; record only; start the scene work now), the
   owner chose to start now, with no guard. A first version did it in
   Tawara's iOS shell; the owner then asked for it in the iced fork
   instead, to build other apps on the fork later (owner,
   2026-10-06). So:

   - **The fork** (`patricksmithlaravel/iced_mobile`, branch
     `tawara/0.14-mobile`) has a second commit on top of D32 item 1's:
     `iced_winit`'s `scene` module, on iOS only. Before winit's
     `UIApplicationMain` it observes `UISceneWillConnectNotification` and
     `UIWindowDidBecomeVisibleNotification` and puts each window winit
     shows into the application's window scene with `setWindowScene:`.
     winit 0.30 makes its window with `initWithFrame:` and no scene, and
     in the scene life cycle such a window is not shown. Both orders
     occur: usually the scene connects first (winit sends `Resumed`, on
     which iced makes its window, from
     `UIApplicationDidBecomeActiveNotification`), but a run was seen
     where the window was shown first; it is then held until the scene
     connects. No Objective-C class is declared, and the crates it uses
     (`objc2` 0.5, `objc2-foundation` and `objc2-ui-kit` 0.2, `block2`
     0.5) are those winit already links. Any app on the fork gets it.
   - **The app** declares the scene, which no library can do for it:
     `Info.plist` carries a `UIApplicationSceneManifest` with one window
     scene and no delegate class. This alone is what UIKit checks at
     launch.
   - winit's life cycle and the shell's lock keep working unchanged:
     UIKit still posts `UIApplication`'s notifications in a scene-based
     app, and the cover, the lock and the return pass on iOS 27 and 18.3.
   - Tawara-mobile's `platform/ios/tawara.sh` (D33) checks that the bundle carries the manifest
     (the simulator alone would not catch its loss on an older SDK) and,
     when the cover goes on, that the key window is in a scene. The shell
     reports that from UIKit as it covers the window, well after launch,
     so the check does not race the order above.

   When winit gains its own scene support, the fork's commit is dropped
   and the manifest kept or replaced as winit's documentation says.

9. **The move to the fork's releases: owner, 2026-10-11.** The owner
   decided on 2026-10-11 that Tawara moves from the frozen fork branch
   `tawara/0.14-mobile`, at `71f00e8` (the pin since item 8), to the
   releases of the reworked fork, `patricksmithlaravel/iced_mobile`. On
   2026-10-12 the owner accepted the ten recommendations below, each marked
   'owner, 2026-10-12'. They supersede what D6, D12, item 1 and D33 say
   about iced's source and pin; those entries keep their wording until the
   move corrects them.

   **Facts the decisions rest on.** On the arm64 Mac that ran the checks, 37
   of the 55 screens differ from their committed images by 3 pixels each, at
   the current pin and at the new one alike. The 37 drawn images are
   byte-identical between the two pins, so the move changes no pixel on that
   host. CI's Linux screenshot step is the exact check. At the current pin
   it passed in all 11 CI runs on record, and run 37535340401 passed on all
   three hosts. If the Linux images differ at the move, they are regenerated
   and reviewed under D27 item 1. Windows and Linux at the new pin were not
   run locally; the move pull request's CI on three hosts is their check.
   The vendored winit's desktop sources (macOS, Windows and Linux) are
   byte-identical to crates.io winit 0.30.13. Its three differing files are
   the Android module, a `cfg` in `event_loop.rs` widened to Android, and
   the changelog. The lock changes 14 packages: the twelve iced crates keep
   their versions, `winit` and `dpi` come from the fork, and `dpi` goes back
   from 0.1.2 to the fork's 0.1.1, which Tawara's code does not name. With
   `--locked`, every `ci.yml` gate passes on the arm64 Mac at the new pin
   except the Linux-only screenshot step, which fails identically at the
   current pin.

   **(a) The pin and its moves: owner, 2026-10-12.** The pin is the fork's
   release `v0.14.1-mobile.2`, written as
   `d4ab3a057db96162b087a27dd0752a3b15abfa6b` on every iced line of both
   repositories. Later moves go only to fork tags `v*-mobile.*`, written as
   full hashes. The wallet moves first, and Tawara-mobile follows to that
   wallet commit on `main`. Each move must pass both policy suites;
   `cargo deny`, with its advisory ignores re-examined; clippy, tests and
   docs on three hosts; the Linux screenshot step; the mobile emulator and
   simulator jobs; and a hand-run desktop check. Each move gets an entry
   here. When a release contains the iOS Return fix, `586665c08`, the pin
   moves to it. The tag is the newest release, and framework code had
   settled by then. Tags do not move. The fork's stability rules keep the
   entry points Tawara uses (`set_android_app`, `on_lifecycle` and the
   two-variant `Lifecycle`) as they are.

   **(b) One winit, the fork's: owner, 2026-10-12.** Tawara-mobile takes
   `winit` only from the fork, through `iced_winit` with the
   `android-native-activity` feature. The crates.io `winit` lines in the
   root `Cargo.toml` and in `crates/mobile/Cargo.toml` are deleted.
   `AndroidApp` and `WindowManagerFlags` are imported from
   `iced_winit::winit::platform::android::activity`. The manifest change
   alone does not compile. The graph holds one winit only once the wallet
   pins the new tag, and until then the policy test fails. Reasons: this is
   the fork's documented route; a second copy would register the same
   Objective-C classes twice, and objc2 panics on the duplicate name; and
   the activity backend is chosen explicitly, not by feature unification.
   The feature sits on an Android-only dependency, so it should not reach
   iOS; the iOS check in the mobile pull request is to confirm that.

   **(c) Two pull requests in lockstep: owner, 2026-10-12.** The
   Tawara-wallet pull request moves every iced line, desktop included, and
   merges with a merge commit. The Tawara-mobile pull request follows
   straight after. It takes that wallet commit from `main` by full hash,
   with the new tag, and no other wallet change sits between the two. Mobile
   cannot take the tag before the wallet has. A gap would let phase 4 (b)
   work, which uses `safe_area()` and `clipboard::read` in `tawara-app`,
   land on a wallet `main` that Tawara-mobile cannot follow. The wallet pull
   request checks the desktop: the three-host CI, the Linux screenshot step
   and the hand-run desktop check.

   **(d) Glue kept, fork APIs adopted: owner, 2026-10-12.** Tawara-mobile
   keeps its security glue: `FLAG_SECURE` and the hand-written Android
   entry, which starts cold after the Activity is destroyed; the iOS privacy
   cover, the iOS lifecycle observers and the background-task lock, with no
   `on_lifecycle` hook on iOS; and the Android lifecycle hook and Back
   handling, unchanged. The iOS scene manifest and its checks stay, and the
   fork's reworked scene adoption arrives with the move. The clipboard write
   path is kept and gains protection: `ClipDescription.EXTRA_IS_SENSITIVE`
   on Android, and an expiring, local-only pasteboard item on iOS. The fork
   has no FLAG_SECURE, snapshot cover, background-task API or clipboard
   protection. A same-process restart would need the Activity's handles, the
   host statics and FLAG_SECURE rebuilt, all for a rare event. Adopted from
   the fork: `iced::clipboard::read`, behind a Paste button for the
   signed-bytes and address fields, and `safe_area()` for the insets of
   phase 4 (b)(1). Separately, the iOS `cover()` log is to report a cover
   only when one was added, and `UIWindowScene.keyWindow` is to be
   considered in place of the deprecated `UIApplication.keyWindow`.

   **(e) Policy rules: owner, 2026-10-12.** In the wallet's `policy.rs`
   (D15), every `Cargo.lock` package from the fork's URL must be at the
   pinned rev, which covers `winit` and `dpi`. There must be exactly one
   `winit`, from the fork. No crate manifest may name `winit`, and the iced
   lines keep `default-features = false` and their feature list. In
   Tawara-mobile's `policy.rs`, the one-copy test covers every package from
   the fork's URL. A crate name that appears twice with different sources
   fails, `winit` leaves the allowed list, and `android-activity` has the
   native-activity backend only. Both `deny.toml` files keep their git
   sources, which are already allowed. Their stale comments are fixed. Their
   advisory ignores are re-examined at the new tag, where both are still
   needed, and their iced 0.14.0 wording is updated. A ban on `winit` except
   through `iced_winit` may follow once cargo-deny 0.20.2 is seen to accept
   its syntax. Reasons: the current tests pass with a direct `winit`, with
   defaults turned on, and with features removed; a mobile graph with two
   winit passes both the policy test and `cargo deny`; and
   `multiple-versions = "deny"` is unworkable, as 39 duplicate names come
   from iced, wgpu and objc2.

   **(f) Return does not broadcast on phones: owner, 2026-10-12.** Return
   already broadcast on desktop when this was decided: W9's field in
   `crates/app/src/screens/wallet/send.rs` bound
   `on_submit(WalletMsg::Submit)` with no platform gate. The Submit button
   sends the same message, which starts `Command::SubmitArtifact` at once
   with no confirmation, and both are disabled while busy. The move adds
   Return-to-submit on phones. So the field's Return is bound only when
   building for neither Android nor iOS (`target_os`): on phones Return does
   nothing, the Submit button still broadcasts, and desktop is unchanged. The
   guard must reach the wallet's `main` before or with the move, so that
   Tawara-mobile's pin never carries a Return that broadcasts. It is the
   first `target_os` gate in `crates/app`, which until then had only
   `cfg(test)` and one test-only `cfg(unix)`. Reasons: a soft-keyboard Return
   is easy to press just after a paste, a broadcast cannot be undone, and the
   guard removes the risk the move adds without changing desktop behaviour.
   The tests run on the desktop. They draw W9 as the desktop has it, where
   Return submits, and with the binding a phone has, where Return sends
   nothing and the field still takes input. That the constant choosing the
   binding, `RETURN_BROADCASTS`, is false on Android and iOS is checked only
   by review and by the phone builds.

   **(g) Paste into secret fields: owner, 2026-10-12.** Paste into
   `secure(true)` fields is allowed. PLAN section 4 forbids copy and cut in
   those fields, not paste, and restore and password-manager use depend on
   paste. On phones paste is reachable only from a hardware keyboard, as
   there is no edit menu. Iced's paste strips every control character, so a
   phrase copied over several lines is joined into one word, as already
   happens on desktop. The restore field turns line breaks into spaces
   before validation. `on_paste` receives the value after filtering, when
   the breaks are already gone, so the change either handles the paste
   before the filter or splits the joined text back into words. Which of the
   two is possible needs a short design look first.

   **(h) Linux liveness step before the move: owner, 2026-10-12.** A small
   wallet pull request, merged at the current pin before the move, adds a
   Linux step to `ci.yml` or a new workflow, because `spikes.yml` does not
   run on the move's pull request. The step starts `tawara-desktop` under
   Xvfb, once on wgpu with `mesa-vulkan-drivers` for lavapipe, and once on
   tiny-skia. It waits a fixed time and passes if the process is still
   alive. It uses only `apt`, `xvfb-run` and shell, with no third-party
   action (D13). First, check that start-up needs no network, or that a
   missing node does not end the process. As built, a launch passes only if
   the app's own process is still running and its log holds no panic, since
   a panic on another thread, such as the worker's, does not end the
   process. Start-up was checked and needs no network: a fresh `HOME` names
   no node, because `Preferences` has no node by default
   (`crates/wallet-core/src/preferences.rs:54`), and the application sends
   `SetNode` at start-up only when a node is set
   (`crates/app/src/app.rs:898-900`). Today no automated job starts the
   desktop window, before or after the move, so a start-up panic, such as a
   shader or surface failure, would pass CI. Landing the step at the current
   pin gives a baseline, so the move shows whether the fork changes the
   result. A ready marker or self-test flag would be stronger and may follow
   later. The move's pull request keeps a hand-run desktop check on macOS,
   Windows and Linux.

   *Amended (owner, 2026-10-13): D43 makes `https://api.mochimo.org` the
   default node, so once that work lands a fresh `HOME` does name a node,
   and the premise above no longer holds. Start-up still needs no network
   if D43 item 2's recommended safeguard is taken: no node is asked at
   launch until a wallet is opened or the person presses Check now. The
   reference to `app.rs:898-900` is now lines 1087 and 1088.*

   **(i) Android: configChanges now, target 36 later: owner, 2026-10-12.**
   Two Tawara-mobile pull requests, kept apart from the move. PR A, now: add
   `mcc|mnc|grammaticalGender` to `configChanges` at
   `platform/android/AndroidManifest.xml:33`, so that the list has the 18
   names of the API 35 set in the fork's `icm` tool, and check `0x5000ffff`
   with `aapt2 dump xmltree`. Without them, a change of mobile country or
   network code, or of grammatical gender, would on Android's own rules
   recreate the Activity and end the iced app. That is a live defect whatever
   Play requires. PR B, before the first Play submission and before
   2026-11-01 if an existing listing would otherwise need the extension:
   `TARGET_API=36` in `platform/android/tawara.sh`, the compile jar pinned to
   `platforms/android-36` rather than the newest of 35 to 39, the android-36
   emulator image, and `assetsPaths` and `resourcesUnused` added to
   `configChanges`. That makes the API 36 set of 20 names, `0xd800ffff`: the
   18 names of PR A, plus `assetsPaths` (`0x80000000`) and `resourcesUnused`
   (`0x08000000`), both new at API 36 in the platform's
   `data/api-versions.xml`. aapt2 2.20 gives `0x5000ffff` for the 18 names and
   `0xd800ffff` for the 20 against the android-36 `android.jar`. `assetsPaths`
   is not a Play requirement, but the fork's reason for it was measured on API
   36 emulators. PR B is checked with `aapt2 dump badging` and `xmltree`, which
   must read `0xd800ffff`. Google Play requires target API 36 for
   new apps and updates from 2026-08-31, with an extension to 2026-11-01 on
   request; the rule and the extension are to be confirmed in Play Console
   before PR B. Target 36 keeps Tawara's Back opt-out
   (`enableOnBackInvokedCallback="false"`), which is removed at 37. Kept
   apart, a failure on the API 36 emulator is not confused with the fork
   change; the 36 path has so far run only on an API 35 emulator.

   *Amended (owner, 2026-10-13), `assetsPaths` now: the owner decided that
   `assetsPaths` leaves PR B and ships now, in a Tawara-mobile pull request
   of its own, not yet merged. That pull request adds it to
   `configChanges`, for 19 names, PR A's 18 and `assetsPaths`, which aapt2
   reads as `0xd000ffff`, and makes `tawara.sh` pin the compile jar to
   `platforms/android-36` and stop if it is missing, since aapt2 knows
   `assetsPaths` only from the API 36 jar. Until it merges, `main` holds
   the 18 names and `0x5000ffff`. The target stays at 35 until PR B. The
   reason is a start-up quit, proven on a local arm64 API 36 emulator
   with Tawara-mobile's `main` at `1cbcc1a`. Just after an install, the
   system's overlay manager adds the framework overlays to the new app's
   `ApplicationInfo`, 67 to 691 ms after the install in the local runs.
   When that lands after the Activity has resumed, Android relaunches the
   Activity for an asset-path change the manifest does not declare
   (`wm_relaunch_resume_activity` with the change `80000000`). The relaunch
   destroys the Activity, `run_in` returns, and the process ends, as items
   (d) and (j) decide for a destroyed Activity. When it lands before the
   first `onCreate`, nothing happens, which is why one tree both passed and
   failed. Turning the gestural navigation overlay on or off while Tawara
   was in front ended the process in both runs with the 18 names, and in
   neither run with `assetsPaths`, where the process and the Activity
   stayed. A person can meet the same by opening the app just after
   installing it, or by changing the navigation mode while it runs; a
   colour or wallpaper change probably does the same, which is inferred and
   not tested. That this race is behind the failures of CI's x86_64 API 35
   job is now shown by Tawara-mobile #12's CI: its first commit alone
   (8972b85) fails there with the relaunch line (run 38377412659), and its
   second commit, the fix (333dff7), passes (run 38377605869). The platform's
   data marks the change as new at API 36. `resourcesUnused`, `TARGET_API=36`
   and the API 36 emulator image stay in PR B, which would then add
   `resourcesUnused` to the 19, for the 20 names and `0xd800ffff` above.
   `resourcesUnused` was not tested. Item 10 (k) gives the order.*

   **(j) Android exit after destruction: owner, 2026-10-12.** Check first,
   then act. On an emulator, press Back from the foreground, then repeat
   once with "Don't keep activities" switched on. Read the logcat tag
   `RustStdoutStderr` for "TAWARA lifecycle: suspended; locking", for
   whatever the lock logs when it completes, and for any SIGABRT. If the
   lock's work is cut short, wait for `Leaving` for a short bound, about two
   seconds, after `run_in` returns and before the process ends. If a SIGABRT
   is reproduced, end the process with `libc::_exit` instead; that adds
   `libc` to the mobile dependencies and the policy test. Reasons: Tawara
   calls `exit(0)` as soon as `run_in` returns, without waiting for the lock
   it started; `exit` runs no destructors; android-activity's documentation
   says not to call `process::exit` from `android_main`; and the fork
   reports SIGABRT in renderer threads under `exit` but not under `_exit`,
   which has not been reproduced here. The wait comes after the Activity has
   gone, so the suspend path, where the owner chose not to wait, is
   unchanged. A test that sends the app to the background and kills it from
   Recents cannot settle this, as the lock already runs by then.

   *Added (decided under the owner's delegation, 2026-10-12; Tawara-mobile#4):
   the repeat with "Don't keep activities" on is made by writing the setting
   and rebooting the emulator. ActivityManagerService reads the setting into
   its runtime field only at boot, and no shell command calls
   `setAlwaysFinish`, whose binder transaction number differs between Android
   15 builds. The check `exit.runtime` confirms that the setting took.*

   *Amended (owner, 2026-10-13), the exit after Destroy: the check has run on
   an arm64 API 36 emulator, locally, after the move to the fork's winit, and
   its facts are these, from Tawara-mobile#4. With the fork's winit, Android's
   `Destroy` ends the event loop, and `run_in` returns; `exit.destroyed` and
   `exit.ended` pass 5 of 5. `std::process::exit` then aborted in every run:
   SIGABRT on Android's renderer threads (`hwuiTask0` and `hwuiTask1`), right
   after bionic's `FORTIFY: pthread_mutex_lock called on a destroyed mutex`.
   The fork reported this case in its commit `7a622ab64`. As item 9 (j)
   prescribes for a reproduced SIGABRT, Tawara-mobile now ends the process with
   `libc::_exit`, including the early exit on a private-folder failure.
   android-activity 0.6.1 also says not to call `process::exit` from
   `android_main` (its `src/lib.rs`, lines 227 to 231). The lock's `Leaving`
   completed before the exit in 14 of 15 runs; the one exception came when
   `Destroy` arrived 123 ms after the app went to the background. A bounded
   wait after `run_in`, which (j) named, cannot help: `Leaving` completes only
   inside `App::update`, where `Wiped::wiped` is called
   (`crates/app/src/app.rs:1164`) and `Wiped::taken` is called (lines 1666 and
   1672) through `on_event` (line 1555), which `update` calls at line 1189. The
   state they raise is `Wiped` (`crates/app/src/host.rs:103-139`). `run_in`
   hands `App::update` to `iced::application(...).run()`
   (`crates/app/src/lib.rs:59-75`), so once winit's loop has ended on
   `Destroy`, no update runs.*

   *Decided (owner, 2026-10-13): `exit.lock` in Tawara-mobile's emulator check
   becomes a reported count, not a required gate. Why this is safe: ending the
   process erases every secret held in its memory, and the library's store
   writes are single atomic commits (an interrupted write leaves the store
   fully before or fully after), so an unfinished lock leaves nothing behind.
   No `tawara-app` change.*

   *A note on the amendment above, not part of the owner's words. The library's
   claim, as checked: in `mochimo-crypto` at `7cdc2e9`, `Keystore::commit`
   (`src/keystore/mod.rs:1006`) makes four steps on Unix: write a temporary
   file, `fsync` it, `rename` it over the store, and `fsync` the directory
   (lines 1013 to 1016). `persist_settled` commits through it (line 1354). The
   proof `spend_state_is_never_observed_half_advanced`
   (`tests/invariants.rs:3532-3534`, `cfg(unix)`) kills a commit at each of
   those steps, reopens the store, and finds the spend state fully before or
   fully after. It covers a process that ends, since the store changes only at
   the `rename`, which is one syscall. It does not reach power loss, an `fsync`
   that lies, or a rename on a filesystem that does not journal it
   (`tests/invariants.rs:3489-3493`). The Windows layout has its own proof
   (`tests/invariants.rs:3831-3833`, `cfg(windows)`), which runs only on a
   Windows host and was not run for this record. Two qualifications to 'leaves
   nothing behind'. The lock writes nothing: it drops the session, the pending
   plan and any phrase (`crates/wallet-core/src/worker.rs:1205-1215`). A commit
   killed before its `rename` can leave `accounts.mks.tmp` in the store
   directory, whose header is plaintext metadata and not key material
   (`src/keystore/medium.rs:228-230`); the next commit or open removes it
   (`src/keystore/medium.rs:231`, `src/keystore/mod.rs:696`). Process exit also
   releases the process's memory without overwriting it, so 'erases' means that
   no process holds the secrets afterwards; it does not mean that they are
   zeroed.*

10. **Phase 4 (b): the phone layout and its glue. Decided under the owner's
   delegation, 2026-10-12.** On 2026-10-12 the owner asked that the questions
   phase 4 (b)'s studies raised be decided without waiting for them, and that
   the work proceed. The decisions below were made that way; each is marked so
   and gives the reasons it rests on. Two belong to earlier items and are
   recorded there: the emulator repeat of item 9 (j), and the amendment to
   item 8.

   **(a) No store folder on a phone (decided under the owner's delegation,
   2026-10-12).** A phone draws no folder field on S3, S6 or S7. The store
   always lives in the hosted default folder, `keystore/` inside
   `Host::private_dir` (item 7), the only place the shells protect:
   `no_backup/` on Android, `NSFileProtectionComplete` and the backup
   exclusion on iOS, and mode `0700` on both. A folder remembered from an
   earlier run is not offered on a phone unless it is that one. Item 7's check
   that a store's folder is strictly inside the private folder stays, as a
   second guard; its refusal no longer asks the person to choose a folder,
   since there is no field to choose one with. Where the first-run screens say
   "this computer", a phone says "this phone". The desktop is unchanged.
   Reason: a field whose only accepted values are the default folder and the
   folders under it offers a choice that protects nothing and can only be
   refused.

   **(b) The compact layout (decided under the owner's delegation,
   2026-10-12).** The layout is chosen by the window's logical width: compact
   below 600 logical pixels, and the expanded and medium layouts of D27 item 9
   from there up. The desktop window's minimum width is 1024, so a desktop
   never sees it. The narrowest width supported is 360 logical pixels: at 360
   nothing is cut off and nothing scrolls sideways. The screenshot check lays
   every phone screen out at 360 and reports anything laid out past the edge;
   since iced fits each child into the room its parent gives it, a squeezed
   control or words drawn past their own box are not seen that way, so each
   screen's tests check the rows that could be squeezed, and the images are
   looked at drawn 360 wide as well. Every screen has a compact
   form (docs/SCREENS.md, "Compact layout"): no sidebar, a bar at the top with
   the wordmark, the node's tip and Lock, the six items in a bar at the
   bottom, 16-pixel margins, tables turned into stacked rows, and controls
   that do not fit on one line wrapped onto the next; the first-run screens
   show the wordmark above the form in place of the brand panel. A phone
   starts at 360 wide until the window reports its size, so its first frame
   is already compact. While the keyboard shows, the bottom bar is not
   drawn. Reasons: every phone held upright is below 600, and 360 is the
   narrowest common Android width.

   The decision as given padded the root by the fork's safe area
   (`iced::mobile::safe_area`, the keyboard included) in the compact layout
   alone. It pads the root in every layout instead, a widening made under
   the same delegation on 2026-10-12, for the owner's approval with the
   rest: a phone held sideways is wider than 600 and still has its notch,
   and the safe area is zero on the desktop, so the desktop draws as before.
   Two more parts of the safe area's handling, decided the same way:

   - When the keyboard comes up or grows, the page is scrolled, by no more
     than it must, until the focused field stands above the keyboard with 16
     pixels of room (`screens::reveal_focused`). The root is already padded
     up to the keyboard's top, but iced's field does not scroll the page it
     sits in, so a field low on a long form (S6's recovery phrase) stayed
     under the keyboard.
   - While an iPhone's shell reports no inset at all, the root is padded by
     what the fork's template keeps until the safe area arrives, 64 above and
     48 below (the fork's `docs/icm/DESIGN.md`, F6), the bottom raised to the
     keyboard's top while it shows. An iPhone held upright always has at
     least its 20-point status bar, and an iPhone SE (3rd generation, iOS
     18.3) on the simulator was drawn with no top inset on two cold launches
     (item (f)). What a shell reports otherwise is used as it is, and
     Android, where a window not drawn edge to edge can report zero, is not
     changed.

   Not checked: widths from 600 to 1023, a phone held sideways, draw the
   medium layout, which D27 item 9 designs for 1024 and up. Drawn at 700 for
   the review of this change, W4's table squeezes its fields to their first
   characters and S1's footer link wraps onto two lines. No test covers that
   range. The owner's narrowest width, 360, is a phone held upright; a
   layout for phones held sideways is left for a later change.

   *Amended 2026-10-12, after the owner's review of the paste change: the
   screenshot check also reports a text whose words run past the window's edge,
   measured by the width its paragraph needs. iced can squeeze a text into a box
   narrower than its words, so the words can pass the edge while the box stays
   inside it (`tawara_app::overflow`). Words that end inside the window are not
   reported, even when they are wider than their box. The balance on a phone is
   set to the size its card allows (`dashboard.rs`), so that a nanoMCM amount
   stays on one line. The check is of the window's edge only, so a text wider
   than its card but inside the window passes it; the balance's figure is
   also held inside its card by a test of its own.*

   *Amended, proposed (2026-10-13): on a phone the brand panel would
   return, as a screen of its own before S1, while S1 to S8 keep the
   wordmark above the form (D37, recommended). The phone's W1 would show
   less than the desktop's, not the same content arranged anew (D39 item 2,
   recommended). The owner decided on 2026-10-13 only that the phone's W1
   keeps Recent, at three rows, and the latest-block chip (D39 item 1).*

   **(c) Phone screenshots (decided under the owner's delegation,
   2026-10-12).** Every screen drawn for the desktop, except the `narrow-*`
   images of the desktop's smallest window, is also drawn at 390 × 844 logical
   pixels at a scale of 1, with a safe area of 47 at the top and 34 at the
   bottom (an iPhone of that size), and committed as `phone-<name>.png` beside
   the desktop images; the long pages are drawn a second time scrolled to
   their end (`-end`), and two with a keyboard showing (`-keyboard`). The same
   Linux job checks them pixel for pixel (D27 item 1). The desktop images do
   not change. They are drawn on this Mac in an `ubuntu:24.04` linux/amd64
   container that runs CI's own command, and only once that container has
   drawn the 55 committed desktop images byte for byte. Had it not, the
   phone images would have been taken from what CI's check prints, as
   base64, for an image that differs or is missing
   (`TAWARA_SCREENSHOTS_PRINT`). On 2026-10-12 the container
   (`ubuntu@sha256:534baea6a22c03a63003dbc8dbe78fe34bc0d7e595d9a9dc9834884ff530eb55`,
   x86_64 with AVX2 and FMA, rustc 1.98.0) drew the 55 byte for byte, twice.
   The phone images are the proposal D27 item 9 asked for: the owner
   approves the compact layout by approving them.

   **(d) Touch (decided under the owner's delegation, 2026-10-12).** List
   rows and radio or choice items act on release, only if the pointer moved
   less than 10 logical pixels since the press and is still inside, and they
   do not capture the press, so a drag that starts on one scrolls the list
   (`tawara_app::tap`). This covers the whole-row controls (the activity rows,
   the explorer's queue rows, W1's account rows on a phone), the account
   choice and S1's cards, the segmented controls and the report banners' title
   rows. It applies on every platform: a desktop click is a press and release
   in place, so nothing visible changes, and the desktop images are the
   check. Buttons, links, checkboxes and the navigation keep iced's own
   behaviour: none of them is a row of a list, and the navigation is outside
   any scrolling. Reasons: iced's button captures the press, which stops a
   scrollable's drag (the fork's `docs/agents/limitations.md`), and acts on any
   release over it, however far the finger travelled.

   A new press takes the place of the one a row waits on, so a press whose
   release never reached it does not leave the row deaf to the next; and a
   press under a pointer that has not moved since the row was drawn is taken
   where the pointer is.

   What is left as it is: a drag that starts on a button or on a text field
   does not scroll the page, because the fork's button and text input
   capture the finger's press (`widget/src/button.rs`,
   `widget/src/text_input.rs`). On the compact forms (S3, S6, S7, W4, W8,
   W9), where fields and buttons fill much of the page, a drag starts on a
   label, a gap between fields or a card's padding, which the forms keep.
   Accepted under the owner's delegation, 2026-10-12, since changing it is
   the fork's; not yet checked on a device.

   **(e) The keyboard shown again (decided under the owner's delegation,
   2026-10-12).** A tap on the text field that already has focus shows the
   soft keyboard again. iced's shell asks the platform for the keyboard only
   when no field asked for it on the frame before, so after the person hides
   it (Android's Back) the focused field goes on asking and the shell asks for
   nothing. In `tawara-app`, a wrapper at the root of a phone's window
   watches for a touch that leaves a field focused when one already was,
   while the safe area reports no keyboard, and withholds that frame's
   request; the shell lets the keyboard go and asks for it again on the next
   frame. Nothing in the fork changes. The field keeps its focus, its caret
   and its text, and no message passes through the application. The
   desktop's window is not wrapped: it has no soft keyboard, and on a touch
   screen the withheld frame would end an input method's composition.

   **(f) The first gesture after launch (decided under the owner's
   delegation, 2026-10-12).** It is investigated on the iOS simulator, on
   the builds named below. If the cause is Tawara's, it is fixed here; if it is
   the fork's, it is recorded here and left for a change to the fork, which
   needs the owner's say-so.

   *Found 2026-10-12:* on the iOS simulator, no first gesture after a cold
   launch or after a return from the background was lost, on either build: the
   baseline at PR #6's pin (tawara-app f095432), and the foundation at 7821307,
   with logging probes that were not committed. On the foundation, taps on the
   Restore card were processed 0.13 s, 0.23 s and 14.6 s after the first frame,
   and a tap 0.04 s after the return to the foreground and one about 16 s after
   it. On the baseline, a tap a few tenths of a second after the first frame and
   one a few seconds after it were processed, and so was one after a return. A
   drag that starts on text scrolled the first screen at the first try on both
   builds, and a two-second press across two of the application's one-second
   ticks was delivered. The first tap on a password field showed the soft
   keyboard on both builds (the fork's `safe_area` line reports a keyboard of
   301 points), and the first key reached the field. The system's sheet, "Speed
   up your typing by sliding your finger", came up over the keyboard when it was
   first shown on the simulator; a key typed through the simulator's keyboard
   reached the field, and taps on the soft keys under the sheet were not tested.
   Against the hypotheses: H1 is not reproduced for the first focus; H2 does not
   run on a cold start, as the cover is set only on the move to inactive, which
   a cold launch does not make; H3 is not reproduced, since every visible change
   followed its tap (the screenshot tool takes about 1.5 s a capture, so finer
   frame timing was not measured); H4 does not apply, with no move before the
   first gesture and no secret field on the first screen; H5 is not reproduced:
   with a node that does not answer, only the footer changes, and nothing above
   it moved in the first three seconds; the window's size and the safe area
   settle within about 0.1 s of the first view (the foundation starts at 360
   wide and then reads 402), with no tap in that time; H6 can drop only touches
   that arrive before the window exists, when nothing is drawn; H7 does not
   apply. No Tawara cause was found, so `app.rs` does not change, and no fork
   change is asked for. Not settled: device timing, since the simulator's
   `active` state arrives with the first frame and a device's may not; and the
   soft keyboard under the sheet. A separate fault, not the first gesture: on
   both builds the node footer's "Choose a node" link (and "Change node") did
   not respond to six taps on its text, three on each build, on the first screen
   after a drag and on the unlock screen, while the links and buttons above it
   responded (Continue, "Open a wallet already on this computer", "Back"). With
   a node that does not answer, the link leaves the visible area, its label
   wrapping to four lines. A scratch change that put the URL below the link's
   row hid the link in that state; a scratch change that gave the footer row the
   column's width left the drawing and the taps as they were. The cause was
   not found on those builds; one idea, not confirmed, was that the scroll
   area took the press before the link's own hit test. On those two builds
   the node screen was out of reach by touch. The footer is in
   `screens/first_run.rs`, and the phone layout (item 10 (b)) moves that
   footer, so it must be checked with the same taps before it is merged.

   *Found 2026-10-12, with the phone layout:* the footer's fault is the
   desktop frame drawn at a phone's width. A headless test taps the right end
   of the footer's row on S1, S3 and S7 at 360, with the node answering and
   not answering: with the phone frame each tap reaches the link and opens the
   node screen, and with the desktop frame put back on a phone none does,
   even with the node answering. In that frame the form's pane is about half
   the window, and the link's words are drawn where no tap reaches its
   button. The phone frame gives the footer the page's width and the link
   its own room. This is shown headlessly; the same taps on the simulator
   are still to be made on the phone layout's build.

   *Found 2026-10-12, on the phone layout's build:* this branch, built into
   Tawara-mobile PR #6's shell with a scratch `Host::share` that does
   nothing, passed the 12 checks of `platform/ios/tawara.sh simulator` on an
   iPhone 17 (iOS 27.0). On an iPhone SE (3rd generation, iOS 18.3) the
   shell published a safe area with no inset at all (with `ICM_EVENTS=1`:
   insets 0, 0, 0, 0) on each cold launch, so before item (b)'s fallback
   the wordmark was drawn under the 20-point status bar; with the fallback
   it stands clear of it. Why no inset is read there is not isolated: the
   fork reads winit's safe-area frame against the window's bounds on every
   turn of the event loop, so the zero is winit's answer on that device, and
   a change there is the fork's, which needs the owner's say-so. The
   simulator's input tool was not granted, so no tap was made on this
   build: the first gesture after launch, the footer link by touch, a drag
   that starts on a row, Paste, and the keyboard shown again are still to
   be checked by hand.

   **(g) Paste buttons (decided under the owner's delegation, 2026-10-12).** In
   the compact layout a Paste button stands beside each destination field (W4,
   W8), the signed-bytes field (W9) and the recovery-phrase field (S6). It reads
   the platform's clipboard with `iced::clipboard::read`, only on that tap, and
   what it read replaces what the field held: a destination trimmed of the line
   break and spaces a copy brings at its ends; the signed bytes with every
   whitespace character removed, so that hex copied in groups or across lines
   reads as the one run it is; the phrase cleaned as (h) says. The read comes
   back as a message for the field it was asked for (`Message::Pasted`), so that
   field's own rules apply to it: a phrase's read carries the generation its
   field was drawn at, and one that returns after the application has left the
   foreground is refused and wiped (item 6). A destination's read carries the
   form it was asked for (W4 from which account, or W8 for which) and how many
   rows that form had, and is dropped if another form is shown when it comes
   back, a row was added or removed meanwhile, or the page is waiting on the
   worker; the signed bytes' read is dropped unless W9 is shown and not waiting.
   A read can come back late, for instance while iOS asks whether to allow the
   paste. The read's `Debug` prints nothing of its text. An empty or refused
   read, or one of whitespace alone, changes nothing, and nothing is submitted.
   The desktop keeps keyboard paste and shows no button (`tawara_app::paste`).
   Reasons: on a phone iced's fields have no edit menu and paste is reachable
   only from a hardware keyboard (item 9 (g)), while a destination or a saved
   transaction is pasted far more often than typed.

   *Changed (decided under the owner's delegation, 2026-10-12):* as first
   decided, a destination and the signed bytes kept iced's own paste, with the
   control characters left out. That joins hex copied across lines, but keeps
   the spaces of hex copied in groups, which are not hex, and the spaces around
   a destination. A destination has no whitespace of its own, and the signed
   bytes are hex, so each is cleaned for what it is.

   **(h) Pasted recovery phrases (decided under the owner's delegation,
   2026-10-12).** In the restore field, line breaks, tabs and runs of spaces
   become single spaces and the text is trimmed, before validation, for the
   Paste button and keyboard paste alike. iced's `text_input` drops every
   control character before any hook sees the paste (item 9 (g)), so the restore
   field alone is wrapped: the wrapper hands the field a clipboard whose read
   returns the cleaned text, and iced inserts that at the caret as it would
   anything pasted. A keyboard paste keeps one space for a run at either end
   rather than trimming it, because it may land between words already typed, and
   the space it brings is what keeps them apart; the Paste button replaces the
   field and trims. The whole phrase is cleaned and trimmed before it is sent.
   Typed text is not changed. Reason: a phrase is usually copied from a list,
   one word per line, and iced would join the words into one.

   **(i) Saving a signed transaction on a phone (decided under the owner's
   delegation, 2026-10-12).** `tawara-app` asks its `Host` to share the signed
   transaction (`Host::share`, given a file name and the text). On the desktop
   nothing changes: Save artifact writes the file to the Downloads folder, as
   D27 item 5 says. On a phone, where there is no Downloads folder, Save
   artifact opens the system's share sheet with the same content as the
   desktop's file, as text: the hex and a line break. iOS uses
   `UIActivityViewController`; Android uses `ACTION_SEND` (`text/plain`)
   through `Intent.createChooser`, with the file's name as the subject. The
   page then says, as a note and not as a refusal, that the sheet was opened
   and that Tawara cannot tell whether the bytes were saved there, so the
   spend still counts as not saved and its banner stays until the bytes are
   saved, dismissed or the wallet locks (D38 item 5, as revised on 2026-10-13).
   `Host::share` returns `Ok` when the sheet was shown and `Err` with the
   reason when it was not; a sheet that could not be shown is a refusal, as
   a file that could not be written is on the desktop. `Host::share` is a
   field every shell must fill, so a shell moved to a `tawara-app` that has
   it opens the sheet. Reasons: the share sheet is how a phone hands text to
   Files, a note or a message; a file on Android would need a `FileProvider`,
   which is Java (D4), or MediaStore's Downloads, which starts at API 29 while
   Tawara's minimum is 26.

   *Amended (owner, 2026-10-13), the Android chooser: the owner accepts the
   share sheet on Android, as decided on Tawara-mobile PR #8, with an exception
   to item 4's `FLAG_SECURE`. Android's chooser is the system's own window,
   which `FLAG_SECURE` does not cover: a screenshot taken while it was up
   showed the chooser's preview of the signed transaction's text over Tawara's
   black window. The chooser also offers a "Copy text" action
   (`com.android.intentresolver:id/copy`) whose copy Tawara cannot mark
   sensitive. Tawara sets no option that removes either. A file through a
   `FileProvider` would avoid the preview, but D4 rules it out. The exception
   holds only for the sheet's lifetime, which begins only when the person
   presses Save artifact to hand these same bytes to another app; Tawara's own
   Copy, marked sensitive as item (j) says, remains. The comment on copy in
   Tawara-mobile's `crates/mobile/src/android.rs`, from its PR #7, calls a
   signed spend "not secret". It is reworded to match
   `crates/wallet-core/src/event.rs`, which calls these bytes "the only bytes
   that can move these funds while the reservation is open".*

   *A note on the amendment above, not part of the owner's words: item (j)'s
   mark is read only from API 33, and Tawara's minimum is API 26, so on API
   26 to 32 Tawara's own Copy is unmarked in effect as well.*

   *Amended (owner, 2026-10-13), what `Ok` means: `Host::share` returning `Ok`
   says that the platform was asked to show the sheet, not that it was shown.
   A refusal the platform makes later is not reported, since the call has
   already returned. On the simulator, UIKit presents the sheet 2.4 s after the
   call on a first press and 0.6 s after it on a second. The note the page
   shows reads "Tawara asked the system to open the share sheet", and keeps
   the rest of its sense: Tawara cannot tell whether the bytes were saved
   there, so they still count as not saved and the banner stays. `Host::share`'s
   documentation says the same. This supersedes the paragraph above where it
   says that the page says the sheet was opened and that `Host::share`
   returns `Ok` when the sheet was shown.*

   **(j) Clipboard protection (decided under the owner's delegation,
   2026-10-12).** Tawara-mobile's copy path marks every copy as sensitive on
   Android (`ClipDescription` extras `android.content.extra.IS_SENSITIVE` set
   to true, which the system reads from API 33 and which is harmless below)
   and as local-only with a two-minute expiry on iOS (`UIPasteboard`
   `setItems:options:` with `UIPasteboardOptionLocalOnly` and
   `UIPasteboardOptionExpirationDate`). Two minutes is long enough to paste
   into another app and short enough that a copied destination or transaction
   does not linger. This is item 9 (d)'s protection, made exact.

   **(k) Android target API 36 after Tawara-mobile #4 and #5 (decided under
   the owner's delegation, 2026-10-12).** Item 9 (i)'s PR B is made after #4
   and #5 merge, so that the three do not edit `platform/android/tawara.sh`
   at once. It names the 20 configChanges, `0xd800ffff`.

   *Amended (owner, 2026-10-13): the start-up fix of item 9 (i)'s amendment
   takes `assetsPaths` out of PR B, into Tawara-mobile PR #12. The order is
   now (owner, 2026-10-13): #12 merges first, and then Tawara-mobile #4 (the
   exit check) merges `main`. That replaces the owner's earlier answer of the
   same day, which put the fix after #4, and the open question about the
   order. The reason is that #4 hits the same start-up quit, so its Android
   CI cannot pass until the fix is on `main`. #12's CI has also shown the bug
   on CI's own API 35 image: its first commit alone (8972b85) fails with the
   relaunch line `wm_relaunch_resume_activity ...,80000000`
   (run 38377412659), and its second commit (333dff7) passes
   (run 38377605869). PR B still comes after #4 (#5 has merged), and still
   names the 20 configChanges, `0xd800ffff`, by adding `resourcesUnused` to
   the 19.*

   **(l) CI watched on the open wallet pull request (decided under the owner's
   delegation, 2026-10-12).** Automatic fixing of CI failures is on for the
   open wallet pull request; a red run is fixed by a commit added to its
   branch, never by a force-push.

### D33. The mobile wallet in its own repository

**owner, 2026-10-06.** Until now the plan kept the Android and iOS entry
points in this workspace (`crates/mobile`, `platform/`, the `mobile` CI
workflow; D3, D32). The owner decided that the mobile wallet lives in a
repository of its own from now on, and chose the following from the
options put in detail.

1. **`patricksmithlaravel/Tawara-mobile`, public.** It holds what is
   mobile only: `crates/mobile` (the `tawara-mobile` crate and the iOS
   `tawara` binary), `platform/android`, `platform/ios` and their CI. It
   is public like this repository and the iced fork, so its CI keeps
   D13's anonymous clone and `permissions: {}` (a private repository was
   considered; its CI would have needed a read token).
2. **It depends on this repository, never copies it.** `tawara-app`, and
   through it `tawara-wallet-core` and the library, come from this
   repository by git, pinned by full commit hash, under the same rule as
   the library (docs/PLAN.md section 2, D1): no `[patch]`, `[replace]`,
   vendored copy or path override. A change the phone needs in a screen,
   in the wallet's logic or in text that protects the user is made here
   and taken there by moving `rev`. So there is one copy of every screen
   and every refusal, and the compact phone layout (phase 4 (b)) is work
   in `tawara-app`, in this repository.
3. **One iced.** Tawara-mobile takes `iced_winit` from the iced fork at
   the commit this workspace pins (D32 item 1), so the build holds one
   copy of each iced crate. A policy test there holds its pins to full
   hashes and its lockfile to one iced commit.
   *Amended 2026-10-12: the commit this workspace pins is now the fork's
   release `v0.14.1-mobile.2`; see D32 item 9 (a).*
4. **The record stays here.** Decisions, the plan and the screens remain
   in this repository's `docs/`; Tawara-mobile's README points to them.
5. **The open pull requests.** #13 (phase 4 (a)) keeps its shared part
   here: `tawara-app`'s `run_in(Host)` and `left_foreground`, the iced
   fork's pin (moved to 71f00e8, D32 item 8) and the README; its mobile
   part moves to Tawara-mobile as that repository's first pull request.
   #14 (the scene work in the shell) is closed; the scene support is in
   the iced fork, and the manifest and its checks are in Tawara-mobile.
   *Amended 2026-10-12: the fork's pin is now the release
   `v0.14.1-mobile.2`, not 71f00e8; see D32 item 9.*

Tawara-mobile pins this repository at #13's head until #13 merges; the
merge keeps that commit reachable, and the pin then moves to `main`.

## Phase 5

### D34. The threat model is written up

**Decided under the owner's delegation, 2026-10-12, for the owner to
revisit.** docs/PLAN.md section 3 names `docs/THREAT_MODEL.md` as the
full write-up of section 4, required before any release (phase 5).
It is written against this repository's `main` at `389f731`,
Tawara-mobile's `main` at `587329c`, the library at `7cdc2e9` and the iced
fork at `d4ab3a0`. The choices it makes:

- **It adds no control.** Every protection it names is in the source or in
  these records, with its place. A threat with no protection, or a
  protection with no check, is listed as a gap with who closes it: the
  owner, or work done under the owner's delegation.
- **Status says what was exercised.** "verified" means a named test or a CI
  check exercises the control, and says when that is only the emulator,
  the simulator or a desktop host; "source only" means the control was
  read and nothing exercises it; "owed" means the control or its check
  does not exist yet. No check of any kind has run on a physical phone,
  so every phone control is at most verified on the emulator or simulator.
- **Residual risks are listed for acceptance, not accepted.** Only D21's
  desktop backups and D22's bundled roots were accepted by the owner. The
  others (THREAT_MODEL section 5.4) wait on the owner, and section 6 makes
  their acceptance, or a control, a condition of release.
- **The records that disagree are listed, not corrected.** The largest:
  D19 says an app ended between signing and recording leaves the account
  behind the chain, while D28 and item 6 of D32 say the index is written
  before a spend is signed. The library's invariant I2 bears out D28 and
  D32 for a process killed at a syscall boundary, and leaves power loss
  outside its claim. Correcting D19, and D28's causes, changes text the
  person reads, which is the owner's (docs/PLAN.md section 3), so it is
  recommended there (THREAT_MODEL section 5.3, C1) rather than made here.

The file is kept current: a change that adds, removes or weakens a
control updates its row in the same pull request.

### D35. Third-party notices: cargo-about 0.9.2, generated and checked in CI

**Proposed, 2026-10-13.** PLAN section 6 asks for the notices to be generated
in CI, not written by hand. Item 6 lists what is open for the owner. The
number is fixed: D34 is the threat model's entry and D36 packaging's, each
added by its own pull request, so each keeps its number whatever order they
merge in.

1. **Tool and pin.** cargo-about 0.9.2, built from crates.io with
   `cargo install --locked --features cli cargo-about --version 0.9.2`, in the
   form cargo-deny is installed in `ci.yml` (D13). Without `--features cli`,
   cargo installs nothing and still exits 0, so `ci/notices.sh` refuses any
   other version. Of the tools examined, it is the only one that writes the
   full licence texts from `Cargo.lock` in one command. cargo-bundle-licenses
   4.2.0 leaves about a hundred texts as `NOT FOUND`, and cargo-license and
   `cargo deny list` name the licences without their texts.
2. **File.** `THIRD-PARTY-NOTICES.txt` at the repository root, in plain text,
   so that each text reads as its crate carries it. Markdown would reinterpret
   its characters, and HTML would need escaping. `ci/notices.sh` changes the
   generated text in two ways and leaves the words of the licences alone, and
   Tawara-mobile's `ci/notices.sh` does the same, so the rule is one for both
   repositories:
   - carriage returns are removed, because both repositories keep text files
     with LF (`.gitattributes`), and a CRLF copy would be rewritten on commit
     and could never match the generator again;
   - the standard MIT text's template line, "Copyright (c) <year> <copyright
     holders>", is removed with the blank line after it (item 6 (a)). Any
     other template marker stops the script, since it means a crate has
     fallen back to a standard text that `about.toml` does not account for.
3. **Contents** (`about.toml`, `THIRD-PARTY-NOTICES.hbs`). The six desktop
   targets, x86_64 and aarch64 for Windows, Linux and macOS; every feature;
   build dependencies; no development-only crates. The accepted licences are
   deny.toml's ten (D10, D12), with two exceptions, each for named crates
   only: the Mochimo agreement's `LicenseRef-` identifier for the library and
   Tawara's crates, as in deny.toml's own exceptions, and OFL-1.1 for
   tawara-app's fonts (item 6 (d)). `crates/wallet-core/tests/policy.rs` holds
   the list to deny.toml's and the exceptions to a table. The iced fork (D32
   item 9), ring 0.17.14, the bundled fonts, the Mochimo agreement and the
   copyright holders' texts (item 6 (a)) are clarified by the SHA-256 of their
   files. Each crate is listed with the authors its metadata names, and the
   file says that they are not necessarily its copyright holders.
   `ci/notices.sh` appends `LICENSE.md`, because cargo-about 0.9.2 does not
   print a `LicenseRef-` text.
4. **CI.** On Linux, after the pinned install, `bash ci/notices.sh --check`
   fails, with a diff and the regeneration command, if the committed file
   differs from what `Cargo.lock` gives, so a dependency change comes with its
   regenerated file. The other hosts build from the same lockfile and do not
   repeat the check. No action and no artifact (D13, D27 item 1); the tool is
   built cold on every run, like the rest of the job. The texts that
   `about.toml` takes from crates' repositories are fetched from
   raw.githubusercontent.com on every run, so the check needs the network, as
   the clone and the install already do; if GitHub cannot serve them, the
   check fails rather than passing on a different file. A clarification file
   that cannot be read only warns, so the byte comparison is what guards
   against it. To regenerate, run `bash ci/notices.sh`.
5. **Packaging.** Any distributed desktop build must ship the file beside the
   application. This change does not package it; packaging is phase 5 (PLAN
   section 7) and D36.
6. **Open, for the owner.**
   - (a) **Copyright holders.** The MIT, ISC and BSD licences ask for the
     copyright notice to go with the software. Where a package has no licence
     file of a name cargo-about reads, it prints the licence's standard text,
     whose copyright line is a template that names nobody. Before this was
     fixed, 58 crate versions were printed with that template line, and 62
     with a text that has no copyright line. `about.toml` now gives 42 of the
     58 their holder's text, from a file in the package (the 24 windows
     crates' `license-mit`, libm's `LICENSE.txt`) or from the crate's
     repository at the commit it was published from (lyon, gpu-alloc,
     gpu-descriptor, profiling, svg_fmt, malloc_buf, and the objc2 crates up
     to 0.5), all pinned by SHA-256. The 58th was a second, wrong entry for
     wayland-protocols-plasma, which is gone. Of the 62, zvariant_utils now
     takes its repository's text, which names its holders, and cfg_aliases no
     longer has a second text. What is left:
     - 60 crate versions publish their MIT text with no copyright line, in
       the package and in the repository alike (most of dtolnay's, serde's
       and smol-rs's crates, among others). Their text is printed as
       published.
     - 15 crate versions publish no MIT text at all: block 0.1.6, dispatch
       0.2.0, and madsmtm's objc2 0.6 and framework 0.3 crates. They get the
       standard text without its template line, under an entry that says so,
       and the policy test lists them, so that a new one is looked up before
       it joins them. Ten of them name no author in their metadata either;
       their repository is named.

     (Recommended) ship these as published, after counsel's review and before
     first distribution: the notice that MIT asks to keep is the one the
     holder published, and for these there is none, so any line written here
     would name a holder the crate does not. The alternative is to ask
     madsmtm/objc2 and SSheldon's repositories to publish licence files, and
     for the objc2 framework crates, which are also offered under Apache-2.0
     and Zlib, to choose Apache-2.0, whose text needs no holder line.
   - (b) `Iced-Icons.ttf` is compiled into every iced build
     (`graphics/src/text.rs`) and declares no licence. (Recommended) the owner
     confirms its licence with iced's upstream and the fontello source before
     first distribution.
   - (c) The Fira Sans OFL text (`graphics/fonts/OFL.txt` in the fork) is not
     in the file. Fira Sans is compiled only with the `fira-sans` features,
     which are off. If one is switched on, its text must be added by hand, and
     nothing detects that.
   - (d) OFL-1.1 is accepted for tawara-app alone, and deny.toml is unchanged.
     (Recommended) keep it so.
   - (e) The Mochimo text is appended verbatim. (Recommended) keep it so.
     Whether that suffices is one of the questions Rep-0's `FORK.md` puts to
     counsel (D10).
   - (f) The six desktop targets are (Recommended) until phase 5 fixes the
     shipped set.
   - (g) The `LicenseRef-` defect in cargo-about 0.9.2 is not reported
     upstream. Whether to report it is the owner's call.
   - (h) wayland-protocols-plasma 0.3.12, which winit's Wayland backend
     brings into the Linux builds, generates its code from KDE's protocol
     files. Its package carries them, and most declare LGPL-2.1-or-later;
     others declare MIT-CMU or BSD-3-Clause. The crate itself declares MIT,
     and so do deny.toml's view and these notices. (Recommended) the owner
     puts to counsel whether bindings generated from those files carry their
     licences, before the first Linux distribution.

### D36. Packaging: the platform's own tools for each target

**proposed, 2026-10-13, for the owner's decision.** Phase 5 (docs/PLAN.md
section 7) needs signed packages for every platform, and section 6 asks for
one packaging tool to be chosen and recorded. This entry proposes the tools;
docs/RELEASE.md is the checklist that uses them. Nothing here is decided.
The owner's delegation of 2026-10-12 (D32 item 10) covers the questions
phase 4 (b)'s studies raised, not packaging, so every choice below that is
the owner's is listed under "What the owner decides", with a
recommendation, and docs/RELEASE.md section 1.1 points there.

Three entries were written at once after D33, and each has a fixed number,
so that the order they merge in does not change any of them: D34 is the
threat model, D35 the third-party notices, and D36 this one.

**Not one tool, and why.** Section 6's "one packaging tool" is not followed,
so this is a deviation from the plan, put to the owner (decision 1).

- `cargo-packager` (section 6's example, 0.11.8, released 2025-11-27)
  describes itself as a public preview. Its documentation says nothing of
  macOS signing or notarisation, nor of how it obtains WiX and NSIS
  (source 17), so it cannot be held to D13's rule that every tool is
  installed at a pinned version.
- `icm`, the owner's release tool in the iced fork's `cli/`, at the release
  Tawara already pins (`v0.14.1-mobile.2`, `d4ab3a057`), is built for this
  app's shape (a NativeActivity `cdylib`, a hand-made iOS bundle, one fork
  iced) and has store gates for each target. It is not proposed now, for
  four reasons. It has not been built or run against Tawara, so none of its
  gates has reported on this tree. Its adoption step (`icm init
  --adopt-*`) is not implemented at the pin, so its configuration would be
  written by hand. Adopting it replaces Tawara-mobile's hand-written
  `Info.plist.in` and `AndroidManifest.xml` with generated files, and
  Tawara-mobile's checks read those files. And it does not run on Windows
  hosts at the pin, so the Windows installer would need a second tool in
  any case.
- No single tool covers the six targets with signing done the way each
  store and platform requires. So each target uses the platform vendor's
  own tools, or the one widely used tool for its format, driven by plain
  shell scripts in the repository that owns the target. Tawara-mobile's
  development scripts already drive `aapt2`, `apksigner` and `codesign`
  that way (`platform/android/tawara.sh` and `platform/ios/tawara.sh` at
  Tawara-mobile 587329c).

Where icm pins a tool, the same version and hash are used here
(`bundletool` 1.18.3, `appimagetool` 1.9.1 and its runtime), so a later
move to icm changes what drives the tools, not the tools or the packages.
That move is the first thing to revisit, once a fork tag carries icm's
adoption step and Windows support, and a dry run of `icm release` against a
scratch copy of each repository has passed; the pin would follow D32 item
9 (a).

**The tools.** Every release is built on the owner's machines, and the CI
packaging job builds the same packages unsigned (below), with the same pins.

| target | package | tools, pinned | where they come from |
|---|---|---|---|
| Windows | per-user `-setup.exe`, x86_64 | NSIS 3.13; `rc.exe` (icon and version information) and `signtool` from the Windows SDK | NSIS: the 3.13 zip (source 15), checked against a SHA-256 recorded in the script, trusted on first use (below); the SDK on the owner's Windows machine and on the `windows-2025` image |
| macOS | universal `Tawara.app` in a `.dmg` | Xcode 26.6: `lipo`, `iconutil`, `codesign`, `hdiutil`, `notarytool`, `stapler` | `/Applications/Xcode_26.6.app`, installed beside Xcode 27 on the owner's Mac, and the same path on the `macos-26` image (source 22: Xcode 26.6, build 17F113, with the iOS 26.5 SDK); chosen with `DEVELOPER_DIR` |
| Linux | AppImage, x86_64 | `appimagetool` 1.9.1 and the type 2 runtime 20251108 | `curl` from their GitHub releases (sources 20 and 21), each checked against `ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0` and `2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d`, which are icm's pins and GitHub's own digests for those files |
| Linux | `.deb`, x86_64 | `dpkg-deb --root-owner-group`, `dpkg-shlibdeps` | Ubuntu 22.04's `dpkg-dev`, installed with `apt-get` (docs/RELEASE.md section 6.3) in the `ubuntu:22.04` container the build runs in, pinned by digest |
| Android | App Bundle (`.aab`) | build-tools 36.0.0 (`aapt2 link --proto-format`), `platforms/android-36`, NDK 29.0.14206865, `bundletool` 1.18.3, `jarsigner` from a JDK 17 or later | `sdkmanager` at those versions; `bundletool` by `curl` (source 19), checked against `a099cfa1543f55593bc2ed16a70a7c67fe54b1747bb7301f37fdfd6d91028e29`, icm's pin and GitHub's digest |
| iOS | `.ipa` for App Store Connect | Xcode 26.6: `actool`, `codesign`, `plutil`, `ditto`, `vtool`; `xcrun altool` to validate and upload | as macOS |

Every Rust build in a release uses `--locked` and the release profile D11
restates; nothing in a packaging script sets `panic` or overflow checks.
The pins move only in a pull request that names the new version and hash.
The GitHub digests were read on 2026-10-13.

**Reasons for each choice.**

- **Windows: NSIS rather than WiX.** WiX 6 is distributed under version
  1.0 of the Open Source Maintenance Fee terms, and WiX 7 under version 1.1
  (source 13). Version 1.0 exempts only those who make no revenue; version
  1.1 exempts those under US$10,000 in annual revenue, and organisations
  above that must sponsor the WiX project (sources 13 and 14). Accepting
  either is the owner's to do, not something to decide on the owner's
  behalf (decision 2). WiX 5.0.2, the last release before the fee and the
  version icm targets, is not proposed either: the research found no
  statement of its support, either way. NSIS 3.13 (released 2026-09-27;
  source 15) needs no acceptance. Its source, stubs and plug-ins are under
  the zlib/libpng licence, except two compression modules: bzip2's, under
  the bzip2 licence, and LZMA's, under the Common Public License 1.0 with
  an exception for linking (source 16). The script sets `SetCompressor
  zlib`, so the installer carries only zlib/libpng code from NSIS. It
  installs per user, into `%LOCALAPPDATA%\Programs\Tawara`, with no
  administrator prompt, which suits a wallet whose store lives in the
  user's own profile (D21). An MSI can be added if the owner accepts WiX's
  terms.
- **The static C runtime on Windows.** The executable is linked with
  `-C target-feature=+crt-static`, so the installer carries no Visual C++
  runtime. The flag is given with `--config` on the release script's
  command line, not written into a `.cargo/config.toml`: the repository
  has no `.cargo` directory and needs none, and a file there would change
  every Windows build, development and CI included, to serve the release
  alone. The flag touches neither of the settings D11 protects, `panic` and
  overflow checks, which are all D11's policy test refuses in cargo
  configuration.
- **NSIS's download is trusted on first use.** NSIS publishes no checksum
  and no signature for its downloads (source 15, checked 2026-10-13), so
  nothing verifies the first download beyond its HTTPS connection to
  SourceForge. The hash recorded in the script proves that every later
  download is the same file, not that the first was the publisher's. To
  narrow that, the first download is made twice, over two different
  networks, and the two compared before the hash is recorded. The
  remaining risk is the threat model's T42 (D34): a subverted compiler
  would write the installer that the owner then signs. Building NSIS from
  its source archive at the 3.13 tag is the alternative, if the owner
  wants it (decision 8).
- **Prebuilt downloads, where D13 builds from source.** D13 builds
  cargo-deny from crates.io at a pinned version "rather than downloaded
  prebuilt", with the Rust toolchain CI already has. The four downloaded
  tools here have no such route: NSIS is C and C++ built with SCons and a
  cross-compiler for its stubs; `appimagetool` and the runtime are C,
  built statically against musl in their own CI; and `bundletool` is Java
  built with Gradle, which D4 keeps out. Building them would bring in a
  toolchain larger than the tool, which would itself need pinning. So each
  is pinned by version and SHA-256 instead, and any change in its bytes
  fails the build. For the three on GitHub the recorded hash is checked
  against more than one party: GitHub's digest, computed when the
  publisher uploaded the file, and icm's pin, made separately. This is a
  departure from D13's practice, put to the owner (decision 8).
- **macOS: Apple's tools, no third-party DMG tool.** `hdiutil` makes the
  image without driving the Finder, so it runs without a logged-in
  session; the styled window that `create-dmg` or `dmgbuild` would add is
  not needed. The app is one universal binary (`lipo` of
  `aarch64-apple-darwin` and `x86_64-apple-darwin`) with
  `LSMinimumSystemVersion` 11.0, the first release that runs on Apple
  silicon, so one minimum serves both halves (decision 5). It is signed
  with the hardened runtime and a secure timestamp, with no entitlements:
  it is not sandboxed and needs no exception. Notarisation uses
  `notarytool`, since Apple has refused `altool` notarisation since
  2023-11-01 (source 3). It is built with Xcode 26.6, as iOS is, so one
  Xcode builds both Apple targets.
- **Linux: AppImage and `.deb`, both by hand, in a pinned container.**
  `appimagetool` is run with `--appimage-extract-and-run`, so the build
  needs no FUSE, and with `--runtime-file`, so it downloads nothing at run
  time. `dpkg-deb` and `dpkg-shlibdeps` are Debian's own; `cargo-deb` would
  add a tool for what a control file and two commands do. `dpkg-shlibdeps`
  also needs a `debian/control` in its working directory (source 27), so the
  build gives it a workspace beside the tree. It names only the libraries a
  binary is linked with (source 26), and the app also opens several by name
  at run time (through `x11-dl`, `x11rb`, `xkbcommon-dl` and `wayland-sys`
  in winit's backends, and through `ash` for `libvulkan` in wgpu), so
  RELEASE.md section 6.3 names them by hand. An executable runs only on
  systems whose glibc is at least the newest version it needs, so the build
  names a floor, **glibc 2.35** (Ubuntu 22.04's, and icm's default), and
  **fails if the executable needs anything newer**; the release notes state
  the floor. The build runs in an `ubuntu:22.04` container pinned by digest,
  on the owner's machine for a release and in the same container in CI. A
  runner image is not enough: the release is not built in CI, and the newest
  images move on (Tawara-mobile's Android job already runs on
  `ubuntu-24.04`, `.github/workflows/mobile.yml` line 96, whose glibc is
  2.39), while docs/RELEASE.md section 7 tests the packages on Ubuntu 22.04.
  The image carries neither a C compiler nor `dpkg-dev`, so the build first
  installs `gcc`, `libc6-dev` and `dpkg-dev` from Ubuntu 22.04's archive,
  with `curl` and `ca-certificates` to fetch rustup-init; it needs no `-dev`
  library package, since the libraries named above are opened at run time.
  Then Rust comes from rustup-init at a pinned version and hash, then the
  toolchain `rust-toolchain.toml` names. On an Apple silicon Mac the
  `linux/amd64` container runs under emulation, which is slower and changes
  nothing about the checks. x86_64 only, for the first release (decision 6).
- **Android: `aapt2` and `bundletool`, no Gradle** (D4; Tawara-mobile's
  script already links with `aapt2`). `aapt2 link --proto-format` writes
  the resources a bundle needs; there is no DEX, as the manifest says
  `hasCode="false"`. `bundletool build-bundle` then `bundletool validate`.
  The bundle is signed with `jarsigner`, because `apksigner` cannot sign a
  bundle (source 10). It is never built with `--debug-mode`. The ABIs are
  those `tawara.sh` builds, arm64-v8a and x86_64, with 16 KB page
  alignment (`PAGE_ALIGNMENT_16K` in the bundle's configuration), which NDK
  r28 and later give by default (source 11). The target API is 36 (D32
  item 9 (i), PR B; source 6), and the compile jar is
  `platforms/android-36` exactly, not the newest one installed as the
  development script takes. Before the release script runs on the owner's
  Mac, it chooses the NDK's host directory, and runs its emulator check on
  a port and AVD of its own, which it deletes afterwards; docs/RELEASE.md
  section 1.6 lists what in `tawara.sh` stops it otherwise.
- **iOS: the bundle `tawara.sh` makes, finished for the App Store, no
  Xcode project** (Tawara-mobile's `platform/ios/README.md`). The release
  script adds what a store upload needs and the development bundle lacks:
  the App Store provisioning profile as `embedded.mobileprovision`; an
  entitlements file with the application identifier, the team,
  `get-task-allow` false, and `com.apple.developer.default-data-protection`
  set to `NSFileProtectionComplete` (D32 item 3); `PrivacyInfo.xcprivacy`;
  the icon compiled into `Assets.car` with `actool`; the `DT` keys of the
  Xcode that built it; `ITSAppUsesNonExemptEncryption`; and
  `CFBundleVersion` from the build number rather than the literal `1`. It
  is signed with the Apple Distribution identity and zipped so that the
  archive's top level is a `Payload/` directory holding `Tawara.app`, with
  no AppleDouble `._` entries, since App Store Connect has refused an upload
  over a `._` path (source 28). Xcode 26.6 is used because App Store uploads
  have needed Xcode 26 and the iOS 26 SDK since 2026-04-28 (source 3), and
  because linking with the iOS 27 SDK is what D32 item 8 found to stop the
  app at launch without a scene. The owner's Mac has only Xcode 27 at this
  writing, so the script reads the SDK the binary was linked with
  (`vtool -show-build`) and fails unless it is 26.x, before anything is
  signed. If Apple requires a newer SDK before an upload, the scene work is
  re-checked first. An Xcode project generated with XcodeGen is the
  fallback if `altool --validate-app` keeps refusing a hand-made bundle.

**What CI does, and what stays on the owner's machines (D13).** D13 is
unchanged: plain commands, no `uses:`, an anonymous clone, `permissions:
{}`, and no uploaded artifacts. A packaging job, run on pull requests that
touch the packaging scripts and by hand, builds every package unsigned
with the pinned tools (the Linux ones with `docker run` in the pinned
container), runs the static checks docs/RELEASE.md lists for each (for
example `bundletool validate`, `plutil -lint`, `dpkg-deb --info`, the glibc
floor, the linked iOS SDK), and keeps nothing. The release itself is built
from the tag on the owner's machines, where the keys are, and signed and
notarised there: signing secrets never enter CI, and the owner's Windows
certificate could not reach a hosted runner anyway, since the keys of
public code-signing certificates have had to be held in hardware since
2023-06-01 (source 18). The owner publishes the packages by hand. Signing
in CI would need secrets in the workflow and a write token to publish, both
of which D13 rules out; that is the owner's to change if it is ever wanted
(decision 4).

**Names, identifiers and placement.** The installed executable is
`tawara-desktop`, the cargo binary's own name, on every desktop system (D9:
it must not collide with the command-line wallet's `tawara`); the name
people see in menus, the Start menu and the Dock is "Tawara" (D8). D8 names
no macOS or Linux identifier, so two are proposed (decision 7): the macOS
bundle identifier `com.patricksmithlaravel.tawara`, the same as iOS's, so
that one name identifies the app everywhere; and the Linux desktop entry
`com.patricksmithlaravel.tawara.desktop`, a reverse-DNS name as the
desktop entry specification recommends. The `.deb` package is
`tawara-desktop`. The desktop scripts live in this repository under
`packaging/`; the phone scripts live in Tawara-mobile beside `tawara.sh`, as
`release.sh` in `platform/android` and `platform/ios`, and `tawara.sh`
stays the development check it is. Every package carries `LICENSE.md` and
the generated `THIRD-PARTY-NOTICES.txt` (D35); docs/RELEASE.md section 4
says where.

**Architectures.** macOS is one universal binary. Windows is x86_64 only:
Windows 11 on Arm runs x86_64 programs under its emulation, and an
aarch64 build would be a second installer to sign and check. Linux is
x86_64 only. The notices (D35) cover all six desktop targets, so either
can be added later without a change to them (decision 6).

**What the owner decides.** Each is open until the owner answers, and each
answer is recorded here with its date.

1. **Per-target tools rather than one packaging tool**, departing from
   PLAN section 6. **(Recommended)** as above, with icm revisited when it
   can adopt an existing app and runs on Windows.
2. **NSIS 3.13, or WiX 7** under the maintenance fee's version 1.1 terms
   (sponsorship if annual revenue is above US$10,000). **(Recommended)**
   NSIS.
3. **The installed executable's name**, `tawara-desktop` (D9; the threat
   model's section 6, item 8). **(Recommended)** as proposed.
4. **Signing and publishing on the owner's machines only**, with CI
   building unsigned packages and D13 unchanged. **(Recommended)** as
   proposed.
5. **macOS 11.0 as the minimum**, one universal binary.
   **(Recommended)** as proposed.
6. **The first release's architectures**: Windows and Linux x86_64 only,
   with glibc 2.35 as the Linux floor. **(Recommended)** as proposed.
7. **The identifiers**: macOS `com.patricksmithlaravel.tawara`, Linux
   `com.patricksmithlaravel.tawara.desktop`, to be added to D8.
   **(Recommended)** as proposed.
8. **Prebuilt downloads pinned by hash**, and NSIS's hash trusted on first
   use; or NSIS built from source. **(Recommended)** the downloads, with
   the two-network first download.
9. **The accounts** in docs/RELEASE.md section 1.2, among them the Apple
   enrolment as an organisation and the Play account type, each after the
   confirmation that section asks for.

**What each step needs from the owner.** docs/RELEASE.md section 1 lists
them with their lead times: the Apple Developer Program enrolled as an
organisation, its identifiers, certificates, profile and API key, and the
`notarytool` profile; a Play Console account, the app, the upload keystore
and Play App Signing; a Windows code-signing certificate on its hardware
token, with a Windows machine that holds it; the `.deb` maintainer line and
an optional GPG key; the app icon; the export compliance answer; and the
machines of its section 1.6, including Xcode 26.6 beside Xcode 27 and a
container runtime.

**Sources.** Each was read on the date given; "research" means the phase 5
packaging study of 2026-10-12, whose notes are not in the repository.

1. Apple, App Review Guidelines, 3.1.5(i): "Apps may facilitate virtual
   currency storage, provided they are offered by developers enrolled as
   an organization." https://developer.apple.com/app-store/review/guidelines/
   (2026-10-13)
2. Apple, enrolling an organisation: a legal entity, a D-U-N-S number, a
   public website on its domain, an Account Holder with authority to bind
   it. https://developer.apple.com/programs/enroll/ (2026-10-13)
3. Apple, upcoming requirements: Xcode 26 and the iOS 26 SDK for uploads
   from 2026-04-28; iOS 13 as the lowest deployment target from 2026-09-09;
   `altool` notarisation refused from 2023-11-01. No iOS 27 SDK date is
   given. https://developer.apple.com/news/upcoming-requirements/
   (2026-10-13)
4. Apple, uploading builds: Xcode, `altool`, Transporter and others; the
   roles that may upload include App Manager.
   https://developer.apple.com/help/app-store-connect/manage-builds/upload-builds
   (2026-10-13)
5. Apple Developer Program fee, US$99 a year.
   https://developer.apple.com/programs/ (research, 2026-10-12)
6. Google Play target API: API 36 for new apps and app updates from
   2026-08-31; an extension to 2026-11-01 on request, without saying
   whether a new app may have one.
   https://developer.android.com/google/play/requirements/target-sdk
   (2026-10-13)
7. Google Play testing for new personal accounts: personal accounts created
   after 2023-11-13 run a closed test with at least 12 testers opted in for
   14 days; organisation accounts are not mentioned. The test starts after
   the app's setup, the testers "must have been opted in continuously for
   the preceding 14 days" when production access is applied for, and the
   review of that application "usually takes seven days or less".
   https://support.google.com/googleplay/android-developer/answer/14151465
   (2026-10-13)
8. Google Play registration: US$25 once, identity verification.
   https://support.google.com/googleplay/android-developer/answer/6112435
   (research, 2026-10-12)
9. Play App Signing and upload key reset.
   https://developer.android.com/studio/publish/app-signing (research,
   2026-10-12)
10. bundletool: bundles are signed with `jarsigner`; `apksigner` cannot
    sign one. https://developer.android.com/studio/command-line/bundletool
    (research, 2026-10-12)
11. Android 16 KB page sizes; NDK r28 and later align by default.
    https://developer.android.com/guide/practices/page-sizes (research,
    2026-10-12)
12. Google Play cryptocurrency policy, read through a secondary summary
    only. https://support.google.com/googleplay/android-developer/answer/16329703
    (research, 2026-10-12)
13. FireGiant, Open Source Maintenance Fee: WiX 6 under OSMF 1.0, WiX 7
    under OSMF 1.1 (EULA id `wix7`); the fee began with WiX 6.
    https://docs.firegiant.com/wix/osmf/ (2026-10-13)
14. Rob Mensching, OSMF 1.1: 1.0 exempted only those without revenue; 1.1
    exempts annual revenue under US$10,000. The article does not itself map
    versions to WiX releases; source 13 does.
    https://robmensching.com/blog/posts/2026/02/04/osmf-v11/ (2026-10-13)
15. NSIS download: 3.13, released 2026-09-27; no checksums or signatures
    published. https://nsis.sourceforge.io/Download (2026-10-13)
16. NSIS licence: zlib/libpng, except the bzip2 module (bzip2 licence) and
    the LZMA module (CPL 1.0, with a linking exception).
    https://nsis.sourceforge.io/License (2026-10-13)
17. cargo-packager 0.11.8, a public preview.
    https://docs.crabnebula.dev/packager/ and
    https://crates.io/crates/cargo-packager (research, 2026-10-12)
18. SSL.com, code-signing keys held in hardware from 2023-06-01.
    https://ssl.com/article/code-signing-key-storage-requirements-will-change-on-june-1-2023/
    (research, 2026-10-12)
19. bundletool 1.18.3 and its jar's digest.
    https://github.com/google/bundletool/releases/tag/1.18.3 (2026-10-13)
20. appimagetool 1.9.1 and its x86_64 digest.
    https://github.com/AppImage/appimagetool/releases/tag/1.9.1
    (2026-10-13)
21. AppImage type 2 runtime 20251108 and its x86_64 digest.
    https://github.com/AppImage/type2-runtime/releases/tag/20251108
    (2026-10-13)
22. GitHub's `macos-26` image: Xcode 26.6 (17F113) at
    `/Applications/Xcode_26.6.app`, the image's default, with the iOS 26.5
    SDK.
    https://github.com/actions/runner-images/blob/main/images/macos/macos-26-Readme.md
    (2026-10-13)
23. Apple, Technical Note TN2206, "macOS Code Signing In Depth", section
    "Signing Disk Images": a disk image's signature is assessed with
    `spctl -a -t open --context context:primary-signature`.
    https://developer.apple.com/library/archive/technotes/tn2206/
    (2026-10-13). The notarised output, and `-t install` for installer
    packages, are from https://developer.apple.com/forums/thread/130560 and
    https://developer.apple.com/forums/thread/675354 (both checked
    2026-10-13).
24. Google Play, staged rollouts: "Staged rollouts can only be used for app
    updates, not when publishing an app for the first time."
    https://support.google.com/googleplay/android-developer/answer/6346149?hl=en
    (2026-10-13)
25. Google Play, the first production release: "If rolling out a first
    production release, clicking Start rollout to production publishes your
    app to all Google Play users in your selected countries or regions." and
    "Rollout percentage options are unavailable for a first release."
    https://support.google.com/googleplay/android-developer/answer/9859348?hl=en
    (2026-10-13)
26. Debian, dpkg-shlibdeps(1): "For each binary that dpkg-shlibdeps
    analyzes, it finds out the list of libraries that it's linked with."
    With `-O`, it prints the substitution variables, among them
    `shlibs:Depends`, to standard output; `-e` names the executable.
    https://manpages.debian.org/bookworm/dpkg-dev/dpkg-shlibdeps.1.en.html
    (2026-10-13)
27. dpkg 1.21.1, `scripts/dpkg-shlibdeps.pl` line 160, `my $control =
    Dpkg::Control::Info->new();`, reads `debian/control` from the working
    directory whether or not `-O` is given; line 161 takes its source stanza.
    In `scripts/Dpkg/Control/Info.pm`, lines 68 and 69 make `debian/control`
    the default file, lines 108 and 109 require a `Source` field in the first
    stanza, and lines 115 to 122 require `Package` and `Architecture` fields
    in each later one. `scripts/Dpkg/Interface/Storable.pm` line 93 opens the
    file and reports a failure as `cannot read %s`. Line 112 of
    `dpkg-shlibdeps.pl` matches `-e` as `^-e(.*)$`, so `-e` followed by a
    space adds an executable with an empty name; for that name no package
    root is found, and lines 359 to 365 print the warning that `binaries to
    analyze should already be installed in their package's directory`. The
    same lines are in Ubuntu 22.04's dpkg-dev 1.21.1ubuntu2.6, checked in a
    container on 2026-10-13.
    https://github.com/guillemj/dpkg/blob/1.21.1/scripts/dpkg-shlibdeps.pl#L160
    (2026-10-13)
28. Apple Developer Forums, "This bundle is invalid - Your archive contains
    paths that are not allowed: [._Symbols]": an upload refused with
    ITMS-90048 over a `._Symbols` path. No reply gives a cause; the
    workarounds remove the path from the `.ipa` or rebuild.
    https://developer.apple.com/forums/thread/776996 (2026-10-13)

## The owner's nine items, October 2026

In October 2026 the owner asked for nine changes: on phones, a lead-in
screen before a wallet is created, and a phone that always opens its own
wallet (D37); spends that settle without a press, on desktop and phone
(D38); a shorter phone main screen (D39); Send on phones as a series of
pages (D40); a QR code on Receive (D41) and QR scanning on Send (D42);
`https://api.mochimo.org` chosen in advance as the node (D43); the
recovery phrase on one screen (D44); and an unlock with the phone's own
checks (D45). On 2026-10-13 the owner answered eight of the questions they
raised: the default node, biometric unlock on iOS with the passcode, the
`qr_code` feature, scanning on phones only, the settled bytes and the
narrow lock case, Recent and the chip on the phone's W1, the order of the
warning and the phrase, and the reading of D4. Every entry below is
proposed until the owner approves it. Each marks what the owner decided
**(owner, 2026-10-13)**, what is **recommended** and waits for the owner's
approval, and what is **open**, with the question and the recommendation.
None of the nine needs a change to the library. Each entry lists the OS
libraries it links, as D4's reading of 2026-10-13 asks, and new wording
that protects the person ships only once the owner approves it (PLAN
section 3, D25). The pull requests that build these entries cite them,
and each changes docs/SCREENS.md as its entry's records say.

### D37. The phone's first run: a lead-in screen, and the phone's own wallet

**proposed, 2026-10-13; asked by the owner (items 1 and 2); recommended,
with the images for the owner's approval.** Item 1 asks for one lead-in
screen on a phone before a wallet is created. Item 2 asks that a phone
find the wallet it already holds and not prompt to create a new one.

1. **S0, a phone-only lead-in (recommended).** A new screen, "Welcome",
   shown only when the phone's store folder holds no store. It carries the
   green brand field, the wordmark, the headline and the three pills, all
   already approved in rendering 01, and one "Get started" button at the
   foot, within reach of a thumb, which leads to S1. It fits 360 × 640
   without scrolling, and the node footer stays on S1. This would amend
   D32 item 10 (b): on a phone the brand panel returns, as a screen of its
   own, while S1 to S8 keep the wordmark above the form. The desktop is
   unchanged. **Open:** one optional sentence on what comes next ("You will
   write down 24 words; have paper ready") would be new protective wording.
   (Recommended) leave it out unless the owner approves its words.
2. **The phone's own wallet (recommended).** On a phone that holds a store,
   S7 is the only first screen, and neither S1 nor S3 is offered: S7 loses
   "Create or restore a different wallet", which on a phone leads only to
   the worker's refusal of an occupied folder after a password has been
   typed. On a phone with no store, S0 leads to S1, and S1 offers no link to
   S7. Starting again with another wallet on a phone means removing the app,
   since a phone has one store folder (D32 item 10 (a)) and no button may
   delete the store (PLAN 4.9). **Open:** one line on S7 that says so is new
   wording, for the owner's approval.
3. **First, a reproduction.** Tawara-mobile at `1cbcc1a`, an ancestor of
   its `main`, already holds the change that picks the first screen again
   once the store is confined to the private folder (Tawara-wallet#19).
   Which build showed the owner the create prompt is not known, so the work
   starts with an install, a create, a killed process and a relaunch, and
   again after a reinstall on top, on an iPhone and on an Android phone with
   a build of that commit.

The owner approves the phone screens by approving their images (D32 item
10 (c)); S0 is new, and S1 and S7 are redrawn. docs/SCREENS.md gains an S0
row in the first-run table, and its S1 and S7 rows and the compact row for
S1 to S8 change with the work. OS libraries: none added.

### D38. Spends settle automatically

**proposed; the direction, and items 5 and 6, decided by the owner on
2026-10-13. Item 5 was REVISED by the owner the same day, and its item (b)
is wanted for a later PR. Item 2's additions are proposed; the rest
recommended.** Until now only W7's Settle button calls
the library's `Wallet::settle_if_landed`, from the worker's `settle()`
(`crates/wallet-core/src/worker.rs:2040`), and Refresh, Check now and
unlock read the chain without settling. The owner decided that a spend
settles automatically once the chain shows it landed, on desktop and
phone. This changes how D27 item 16's settling, built in its pull request
(b) as a step the person takes on W7, is reached; the step itself is the
library's, as before. The words the person sees are the worker's own and
the library's page, under D25 and D28, and the walk D19 and D24 speak of is
item 6's.

1. **When (recommended).** On any fresh read that classifies an account as
   `SpendLanded`: a Refresh that was not cancelled, Check now, unlock, and
   the read that follows a send. One private helper in the worker calls
   `cli::key_access` and then `settle_if_landed`, as `settle()` does, and
   reads that account again. It calls nothing else: no direct
   `persist_settled` (the library calls it inside the settle), no advance,
   no abandon, and it never writes a key index, so PLAN 4.9 holds. It is
   skipped when a cancel has been asked or no wallet is open. Accounts set
   aside at open settle once the node shows them landed, as D19 decides;
   an account the read does not classify as `SpendLanded`, a diverged one
   among them, is left as it is.
2. **A poll (recommended).** Every 60 seconds, only while the wallet is
   unlocked and in the foreground and some account is `SpendOutstanding`
   with a live reservation, driven by the existing one-second tick. It is
   not the person's input, so it does not hold the idle lock back (D24),
   and it stops on lock and on a move to the background. On a phone the
   move to the background locks the wallet, so there the settle on unlock
   covers the common case. D31 item 6 keeps the explorer off timers; this
   poll asks only about an open reservation, and is not the explorer's.
   Without it, a settle waits for the next Refresh, Check now or unlock.

   **Additions (proposed; recommended; the owner may revisit).** The rule
   above polls only an account whose reservation is live. As built in PR
   #30 (`view::Watch`; docs/SCREENS.md's W7 row states the whole rule):
   - a read that says nothing, because the node does not answer or the
     expiry cannot be read, keeps the polls going;
   - a wallet opened during such an outage polls a reservation the store
     holds, from its first read;
   - a read in doubt is "account not found", which the node also answers
     for a failed lookup, or a spend that has landed and not yet been
     settled. At most two more polls follow such a read in a row, and the
     third such read in a row ends the polls. A silent read between them
     neither counts nor restarts the count, and a poll counts when it is
     sent;
   - the polls end on a read that shows the account in sync, the
     reservation dead or unrecorded, or a divergence for the person to
     look at; they stay ended until a read shows the reservation live
     again;
   - a lock (on a phone, a move to the background), a stopped worker and
     a view that omits the account clear the watch. A poll is not the
     person's input (D24), so it does not hold the idle lock back.
3. **What the person sees (recommended).** The existing banner, "Settled:
   the account can spend again" (`report::done`), with the library's page
   one step away (D28). W6's Settle step follows the account's state.
   **Open:** where the banner sits, the W7 panel and the copy for a dead
   reservation are new wording, for the owner's approval. So are the copy for
   signed bytes kept after a settle, the dismiss control item 5 names (built in
   PR #30, with a confirming second press), and W7's copy for a settle that a
   reorganisation reverted (item 5). Its summary says spending is paused and the
   other accounts are unaffected, and its causes are a spend not landed, a store
   moved ahead and another chain (`crates/app/src/screens/wallet/report.rs:241`,
   `:266-275`). None of them names a reorg, which only the library's full report
   beneath does, and none says that the pause lasts until the spend lands or a
   saved copy is re-submitted (library `recon.rs:962-964`, `:986-1000`).
4. **The Settle button stays as a fallback (recommended).** It stays on W7
   for a landed spend whose automatic settle could not reach the node, and
   is hidden on a dead reservation, whose present copy promises a
   resolution that cannot come: a dead reservation whose chain still shows
   the spent address never settles.
5. **REVISED (owner, 2026-10-13): signed bytes are kept until saved,
   dismissed or locked.** Unsaved signed bytes are kept until the person
   saves them, dismisses them, or the wallet locks, and not until the first
   read that finds the account in sync. That first-read rule is the code's
   rule on main (`forget_settled`, `crates/app/src/app/wallet.rs`, called
   from `on_reply` in `crates/app/src/app.rs`); PR #30 replaces it with the
   rule above (owner, 2026-10-13: wording corrected). The bytes live in
   memory only and nothing is written to disk, so a restart also loses
   them. This replaces the owner's earlier answer of the same day, to
   forget them on settle. That answer rested on the wrong premise that a
   reverted settle reopens the reservation, as the corrected facts below
   show. No dismiss control exists on main (owner, 2026-10-13: wording
   corrected); PR #30 adds Dismiss, with a confirming second press, and the
   control and its copy are new wording (item 3).

   If a reorganisation of the chain then reverts the settle, the reservation
   is not reopened: `persist_settled` closes it and keeps only the settle's
   digest, index and, where recorded, figures (library at `7cdc2e9`,
   `keystore/mod.rs:1332-1363`), so the bytes are not in the store. The
   account is then diverged, not reserved. The chain holds it at the settled
   key's address, one index behind the store's, and the next read that
   reconciles it reports `IndexMismatch` with that block (library
   `recon.rs:1546-1558`). Re-sign is refused, since `resign_pending` needs a
   reconciled account and an open reservation (library `wallet.rs:738-740`).
   The library's report names two causes it cannot tell apart: a reorg that
   reverted the settle, or a settle taken on the single observation the
   settle rule permits, where the spend may yet land. For a chain behind the
   store, the report's action is to wait for the spend to land and reconcile
   again; if it never lands, the difference is permanent, and advancing
   local state backwards is never a remedy (library `recon.rs:962-964`). It
   also says that a copy the person saved outside the store can be
   re-submitted while the balance at the source has not moved and a non-zero
   block-to-live has not passed, and that this build has no command that
   turns the retained block back into those bytes (library
   `recon.rs:986-1000`).

   What is lost, in plain words. From this store, the balance at the
   reverted address moves only if the reverted spend lands after all, or if
   a copy the person saved is re-submitted while that balance has not moved
   and a non-zero block-to-live has not passed. With no saved copy, neither
   Tawara nor the library's commands can spend that balance from this store.
   Tawara offers no route to a saved copy from this account either: Submit
   saved artifact is offered only for an open reservation
   (`crates/app/src/screens/wallet/account.rs:187-191`), and the
   acknowledged advance writes nothing for a chain behind the store (library
   `cli/reconcile.rs:272-281`).

   **(b) Wanted (owner, 2026-10-13), in a later PR, not this one.** A
   diverged account one key behind, with a retained settle, should offer
   Submit saved artifact, and accept only a saved copy whose digest
   matches the retained one. It moves no key index (PLAN 4.9). Its
   constraints:
   - only for an account one key behind, with a retained settle (owner,
     2026-10-13);
   - only a saved copy whose digest matches the retained digest (owner,
     2026-10-13);
   - only the identical saved bytes may be re-submitted, and only under the
     library's conditions: the balance at the source has not moved, and a
     non-zero block-to-live has not passed (library `recon.rs:992-994`;
     proposed, for the owner's approval when that PR is made);
   - signing anything else with that key is never allowed, whether or not
     the reverted spend has expired (library `recon.rs:991-992`,
     `wallet.rs:810-812` and `README.md:339`; proposed, for the owner's
     approval when that PR is made);
   - no key index moves (owner, 2026-10-13; PLAN 4.9).

   What happens to the wallet. Refresh and Check now mark the account
   diverged and leave an open wallet open. `Wallet::open`, though, refuses a
   store in which no account is operable (library `wallet.rs:418-427`). So
   in a store whose only account diverged this way, the next open shows the
   library's WALLET WILL NOT START page and the wallet stays closed
   (`crates/wallet-core/src/worker.rs:967-999`, `:1639-1656`). With another
   account reconciled, the wallet opens and sets this one aside.

   Nothing in this store moves a key index to clear the divergence, as PLAN
   4.9's rule that no button moves a key index to make a refusal go away
   requires: the acknowledged advance names no index for a chain behind the
   store (library `recon.rs:865-873` and `:1149`, and
   `cli/reconcile.rs:273`), the recovery page offers no remedy for it
   (`crates/app/src/app/wallet.rs:213-225`), and restore writes nothing for
   an account the store already holds (library `cli/restore.rs:163-168`).
   D27 item 5's amendment holds only while the reservation is open. Tawara's
   side of this is pinned by
   `a_reverted_settle_leaves_the_account_behind_with_no_way_to_advance`
   (`crates/wallet-core/tests/worker.rs`) and
   `a_reverted_settle_is_offered_no_button_and_no_remedy`
   (`crates/app/src/screens/wallet/account.rs`).

   A store made from the same recovery phrase elsewhere is different, and
   making one is a natural step after a lost device. It creates account 0 at
   index 0 (library `cli/create.rs:167`,
   `crates/wallet-core/src/worker.rs:1557`), so it sees the chain ahead, at
   the settled key's index, and the report names that index as the advance
   target (library `recon.rs:865-873`, `:941-951`). Tawara's account
   recovery offers that advance
   (`crates/app/src/screens/wallet/account.rs:237`,
   `crates/app/src/screens/wallet/recovery.rs:137`), and an account added
   there by restore is placed at that index (library
   `cli/restore.rs:173-176`). That store could then sign with the key the
   reverted settle used, whose signature the library's report calls the only
   one that key may ever give (library `recon.rs:991-992`). That is PLAN
   4.5's key-reuse hazard. A store made from the same seed must never
   advance to that index or sign anything with that key. That holds whether
   the reverted spend lands, is re-submitted or expires, because the hazard
   is the signature itself, which already exists. After expiry the signed
   bytes may never be accepted (library `README.md:339`). Expiry does not
   undo the private material that the WOTS+ signature exposed, so a second
   signature with that key stays what the library's re-sign guard calls the
   second-signature-under-one-key catastrophe (library `wallet.rs:810-812`);
   that step is analysis, not library text. Only the identical saved bytes
   may be re-submitted, and only while the balance at the source has not
   moved and a non-zero block-to-live has not passed (library
   `recon.rs:992-994`; item (b)). Neither store can see the other, so this
   is recorded here and not enforced.

   **Why the answer changed (owner, 2026-10-13).** The earlier answer
   rested on the premise that a reverted settle reopens the reservation. It
   does not, so the owner revised it: unsaved bytes are kept until the
   person saves or dismisses them or the wallet locks, in memory only.

   Notes on the revision (analysis recorded with it, not the owner's
   words). Re-sign is refused after a revert, and no command rebuilds the
   bytes (library `recon.rs:995-996`), so a copy held outside the store is
   the only way the library names to move the balance before the spend
   lands. The library's full report, one step away in Tawara
   (`crates/app/src/screens/wallet/report.rs:318`,
   `crates/app/src/app/wallet.rs:1302-1314`), says to keep any copy of the
   artifact (library `recon.rs:999-1000`). Keeping the bytes until the
   balance at the source has moved or a non-zero block-to-live has passed
   would have been another rule. Those are the library's conditions for
   re-submitting a saved copy, which item (b) uses, not a measure of how
   long the bytes are kept.

   **As built in PR #30 (proposed, for the owner's confirmation).**
   - A read that finds the account in sync forgets only a saved spend;
     unsaved bytes stay offered after an automatic or a manual settle.
   - W6 offers Dismiss only while the bytes are not saved, on its own row
     below Save artifact and Copy hex. It asks to be confirmed, and says
     that dismissing wipes the only copy Tawara holds.
   - "The wallet locks" is read as a lock asked for, the idle lock, a move
     to the background (D32 item 6) and the worker's shutdown. Each wipes
     unsaved bytes, shown or kept, and S7 says so. A store the worker
     closed because it could not be read back, which can follow a send's
     answer at once, and one replaced by another unlock, keep them: no one
     asked for that close, and the bytes may not have been seen.
   - Saved bytes are not wiped by a lock. Their page is kept over it and
     shown again on unlock, as `keep_signed` and `bring_back_signed`
     (`crates/app/src/app/wallet.rs:713` and `:752`) do today, and they are
     forgotten once the account is in sync.
   - Whenever a kept spend is dropped, its hex and the library's page,
     which repeats the hex, are wiped. Copies outside the model (a
     clipboard, a share sheet, messages in flight) are not.
   - A worker that stops keeps unsaved bytes until Tawara closes, and the
     stopped screen offers to save them. Dated 2026-10-14, this means a worker
     that panics: a graceful stop releases the unsaved bytes, as the shutdown
     does. A stop whose reason is not yet known (a command could not be
     sent to the worker before its `Stopped` came) keeps them, through a move
     to the background and any lock but the shutdown's, until that notice says
     which, as the owner's review of PR #30 (P1) asked on the same day. The
     owner confirmed the no-notice case on 2026-10-15, below.

   One consequence follows whatever the reading. A revert after the bytes
   have gone, through a lock or a dismissal with no save, finds no copy in
   the store, and the account is then as the paragraphs above describe.
   While the reservation is still open, Re-sign (W8) can sign the same
   spend again: the library refuses any rebuilt plan whose digest differs
   from the reserved one (library `wallet.rs:813-814`).

   **Confirmed (owner, 2026-10-14):** after Tawara's worker panics, the unsaved
   signed bytes are kept until Tawara closes, including across a move to the
   background. This keeps the code as built, follows D38 item 5, and makes W6
   (docs/SCREENS.md) name this as its one exception. The reason: the stopped
   screen's save offer is the only way to keep the only bytes that can move
   those funds while the reservation is open. The bytes are never written to
   disk, and on a phone the app-switcher cover still hides the screen.

   **Note (2026-10-14, the owner's review of PR #30, P1):** W6 now names two
   exceptions, not one. Besides the panic confirmed above, a stop whose reason
   is not yet known keeps the unsaved bytes until the worker's `Stopped` says
   which, since a worker that panics drops its queue before that notice comes.
   The confirmation above covers the panic only. That a move to the background
   with no notice at all releases nothing was as built, and the owner confirmed
   it on 2026-10-15, below.

   **Confirmed (owner, 2026-10-15):** after a failed send with no stop notice
   yet (a command could not be sent to the worker before its `Stopped` came),
   the unsaved signed bytes are kept until the notice comes or Tawara closes,
   and a move to the background does not release them. This keeps the code as
   built. The app test
   `a_move_after_a_failed_send_with_no_notice_keeps_the_bytes_until_the_app_ends`
   (`crates/app/src/app.rs:5145`) pins it. When the notice comes, its reason
   decides, as the earlier rules say: a panic keeps the bytes until Tawara
   closes (confirmed 2026-10-14), and a graceful stop releases them, as the
   shutdown does.
6. **The narrow lock case is accepted, with no library change (owner,
   2026-10-13).** D19 says every walk takes a cancel, and D24 says a walk
   never holds the lock back. The library's `settle_if_landed` reconciles
   the account again through `recon::reconcile_account`, which passes
   `Cancel::NEVER` (library at `7cdc2e9`, `wallet.rs:652-660`,
   `recon.rs:1474-1481`). That reconcile walks key positions only when it
   finds the reservation unexplained (`recon.rs:1618-1624`); a
   `SpendLanded` or `SpendOutstanding` read takes no walk
   (`recon.rs:1570-1612`). The automatic settle runs only after a fresh
   `SpendLanded` read, so it walks only if the chain changes between that
   read and the settle's own, or if a node answers differently on purpose.
   Then the walk cannot be stopped, at about 1.6 ms a position (D19), and a
   lock, or on a phone the move to the background, waits for it (D32 item
   6). The Settle button has the same exposure today; the automatic settle
   makes it happen without a press. The owner accepts this case. A cancel
   on `settle_if_landed` (Rep-0) would make D24 hold strictly; it is not
   proposed unless the owner asks.
7. **Recorded risks.** The node's answers are not authenticated, and the
   library settles on one observation. A reorganisation that reverts a
   settle is caught by the next read that reconciles the account, for
   example Refresh, Check now or the next open, as `IndexMismatch`, with no
   path to acknowledge it. In a store with no other operable account, that
   open refuses the wallet (item 5). The poll adds traffic to the node, and
   with D43's default node that traffic reaches its operator and Cloudflare
   (T28, PR #21).

Records that change with the work: docs/SCREENS.md's rows for a
reservation not settled and for a spend between submission and settlement
(lines 25 and 28), which say "settle", and its W1, W6 and W7 rows; W7's
row adds that an account set aside at open may settle without a press and
rejoins on Refresh. Item 5's revision changes W6's row, which keeps a
spend's page until its account is in sync again and across a lock, and
the S7 row that says a lock keeps such a page. (b), when built, changes
W7's and W9's rows. OS libraries: none added.

### D39. The phone's main screen

**proposed; asked by the owner (item 4); item 1 decided by the owner on
2026-10-13; the rest recommended.** Item 4 asks for only high-level
details on the phone's W1: One-time keys and Network move elsewhere, and
each account is shown as a short line that opens into a larger card.

1. **Recent and the chip (owner, 2026-10-13).** Recent stays on the
   phone's W1, cut to three rows, with a "View activity" link to the full
   list. The latest-block chip stays in the top bar.
2. **What phone W1 shows (recommended).** The balance, the notice banners,
   the Accounts card and Recent. One-time keys leave W1: each account's
   next key appears in its card, and the explanation and the table stay in
   Settings. Network leaves W1 and stays in Explorer E1 and Settings, Node.
   No new screen and no seventh tab, since the bar holds six at 360 (D32
   item 10 (b)).
3. **Account rows (recommended).** One line each: the short name on the
   left, the Base58 destination's first six and last six characters as
   `short()` builds them, and the balance on the right. An account that is
   not reconciled gets a state dot and word on a second line, so a paused
   or outstanding account is never hidden. Rows act on release (D32 item
   10 (d)). **Approved (owner, 2026-10-16):** that the item's "TAG" means
   this short name, since the desktop's column is headed "Address (tag)" and
   Settings shows shortened hex tags.
4. **Rows expand in place (recommended).** A tap opens the row where it
   is, one at a time, and it closes when the page is left. The card holds
   the full destination with Copy, the next key and its kind, the state,
   and "Send from it" (only when it can spend), "Receive" and "Open
   account". The actions on a state, Settle, Re-sign, Submit and account
   recovery, stay on W7 (D19). The alternative, a tap that opens W7 at
   once, costs a page for each look.
5. **Phones, not widths (recommended).** These changes follow `phone()`,
   not the window's width, so a phone held sideways shows the same content
   and a narrow desktop window keeps today's. This is a new rule beside D27
   item 9. Every desktop image, the `narrow-*` ones included, stays byte
   for byte.

This would amend D32 item 10 (b). The owner approves the result by
approving the phone W1 images (D32 item 10 (c)), which for the first time
draw the Accounts card. docs/SCREENS.md's W1 rows (lines 59 and 131) and
E1's compact row change with the work. OS libraries: none added.

**As built (proposed; approved in part by the owner, 2026-10-16, as marked).**
- The code is the phone's branch of `view`, with `phone_accounts`,
  `account_card` and the three-row cut in `recent`, all in
  `crates/app/src/screens/wallet/dashboard.rs`; `WalletPage::expanded` in
  `crates/app/src/app.rs`; and `WalletMsg::Expand` in
  `crates/app/src/app/wallet.rs`. The desktop and a narrow window that is not a
  phone keep their layouts, and `stacked` serves that window (item 5).
- The images are the six `design/screenshots/phone-w1-wallet*.png` that change,
  and `phone-w1-wallet-account-open.png`, which is new and drawn by
  `crates/app/examples/screenshots.rs`. No desktop image changes. Approved
  (owner, 2026-10-16) under D32 item 10 (c), and that approval approves this
  decision.
- "TAG" is read as the short name: `name()` in
  `crates/app/src/screens/wallet.rs` gives the destination's first and last six
  characters, or those of its hex form where no destination is known. The row
  shows no hex tag. Approved (owner, 2026-10-16), so no longer open (item 3).
- Points the text leaves open: "View activity" sits in Recent's header,
  where "View all" sits on the desktop; and a row opens while the wallet is
  busy, since that only changes the view, with the card's buttons disabled
  then, as on W7. Two readings are approved (owner, 2026-10-16), so they are
  no longer open: a tap on the open row closes it (item 4 says only that a row
  closes when the page is left); and pending spends are listed first and capped
  at three on the phone, so a spend still settling counts among the phone's
  three Recent rows (item 1 left that open).
- The W1 rows of docs/SCREENS.md are at lines 61 (desktop) and 136 (compact).
  D39's text gave 59 and 131, which were already stale at a0fbb39, where the
  rows were at 61 and 134.

### D40. Send on phones as a series of pages

**proposed; asked by the owner (item 5); recommended.** On a phone, Send is
one long scroll from W4 to W6 today. The owner asked for a series of pages.

1. **Five pages (recommended).** To (1 of 4): the From line, with
   "Change" when more than one account can spend, one destination at a
   time with Paste, the reference, "Add another destination" and Remove.
   Amount and fee (2 of 4): the amounts, "Send everything" for a single
   destination, fee and blocks to live, live totals, and "Review spend"
   with the library's refusals beside it. Review (3 of 4): the NOT SIGNED
   banner, the destinations in full, the library's totals and "Continue to
   signing", with no Sign button. Sign and submit (4 of 4): "Sign & submit"
   alone, disabled while busy, with one line in W6's words that a submit is
   a socket write, not a verdict. Sent: W6, unchanged, with no way back
   into the flow. Signing stays a step of its own, as D27 item 3 decides.
2. **Back (recommended).** A Back at the top of each page keeps what was
   typed. Back from Review to Amount discards the plan, as Edit does today;
   Back from Sign to Review keeps it, and leaving the Sign page by any other
   way discards the worker's pending plan as leaving Review does. A tap on
   the Send tab during the flow keeps the form. Reason: today a tap on the
   Send tab builds an empty form (`crates/app/src/app/wallet.rs:929-932`),
   and pages make that tap likelier.
3. **Android's Back steps within the send pages (recommended).** On pages
   1 to 4 the system's Back steps back one page, handled in `tawara-app`
   before `Host::back`; everywhere else it keeps its meaning
   (docs/SCREENS.md, "Android's Back keeps its meaning"). The lock on a
   move to the background is kept. Reason: today Back moves the app to the
   background (`crates/app/src/app.rs:1193`), which locks it and drops a
   composed spend.
4. **Refusals where they arise (recommended).** Field checks show under
   their field when Continue is pressed, and refusals and the busy card
   show beside the button that caused them. Reason: today they are drawn at
   the top of the page (`crates/app/src/screens/wallet.rs:395-408`), while
   Review spend is pressed at its foot, so a refusal can be off-screen. The
   destination check needs `parse_destination` made public in
   `tawara-wallet-core` (`crates/wallet-core/src/spend.rs:151`), which is
   not a library change.
5. **The destination page and Paste (recommended).** The destination page
   joins `form_of` and `form()` (`crates/app/src/app/wallet.rs:691` and
   `1029`), so that a Paste read comes back to the form it was asked for,
   as D32 item 10 (g) requires; otherwise its reads are dropped.
6. **Open:** a note on S7 after a lock, that an unsigned spend was
   discarded, is new wording, for the owner's approval.
7. **Readings, for the owner's approval.** (a) A refused signing on a phone
   goes back to page 2, not to page 1: the worker has dropped the plan, and
   "Review spend" is on page 2, as the desktop's refused signing goes back to
   the form that holds it. (b) "Give this destination's address." is new
   wording, for a destination with an amount and no address. Any other
   address or reference the library refuses is shown under its field, in
   the library's words, with the destination's number. (c) The key callout
   and the three steps are on page 4, with "Sign & submit", and not on the
   review. The review keeps the library's page, the destinations and the
   figures, so the key the spend signs with is shown where the person presses
   the button that signs. The line under that button, "Submitting writes the
   signed spend to the node's socket. That is not the node's verdict: settle
   once the chain has moved.", is new wording in W6's words. (d) "Change" opens,
   under the From line on page 1, the accounts that can spend, and choosing one
   shuts the list. D40 names no way to choose an account, so the open list is a
   page state (`WalletPage::choosing`). With one account that can spend there
   is no "Change". (e) Page 1 draws one block for each destination, with its
   Paste, its reference and its Remove, as the compact W4 draws them, and not
   one destination at a time: a page with one destination on it would need a
   shown row and a way to move between rows, which D40 does not give. (f) On
   a phone a refusal and the busy card are drawn beneath the button that caused
   them; the desktop keeps them at the top of the page. (g) Page 3 has no "Edit
   the spend": the Back at the top of the page keeps what was typed and gives
   the plan up, as Edit does. (h) The labels "1 of 4" to "4 of 4", "Amounts",
   "Destinations", "Destination n", and the subtitles of pages 2 and 4 are new
   wording.

The desktop's W4 to W6, `narrow-w4-send.png` included, are unchanged. The
threat model's Locking row (PR #21) and docs/SCREENS.md's Back rule (lines
108 to 110) and its W4 to W6 rows (lines 62 to 64 and 134 to 136) change
with the work. The owner approves the pages by approving their images. OS
libraries: none added.

**As built (proposed; the readings and the images await the owner's approval).**
- The pages are drawn by `view` in `crates/app/src/screens/wallet/send.rs`,
  through `to_page`, `amount_page`, `review_page` and `sign_page`. The stages
  are `SendStage` in `crates/app/src/app/wallet.rs`: `To`, `Amount`, `Review`
  and `Sign` are the four pages, `Compose` is the desktop's W4, and `Sent` is
  W6. The transitions are the handlers for `Continue`, `Review`, `ToSigning`,
  `Sign`, `Back` and `Edit`, with `send_steps_back` and `step_send_back`; the
  return of a refused signing to page 2, in the handler for
  `Purpose::ConfirmSend`; and `WalletPage::choosing` for "Change".
- `crates/app/src/app.rs` sends the system's Back to `send_steps_back` before
  `host::back`. `crates/app/src/screens/wallet.rs` draws a phone page's busy
  card and refusal beneath the button that caused them (`frame_beside` and
  `beside`). `crates/wallet-core/src/spend.rs` makes `parse_destination` and
  `parse_reference` public, so that "Continue" checks each address and
  reference under its own field. The desktop's W4 to W6 draw as before, and
  their images are the same bytes as before.
- The images are drawn by `crates/app/examples/screenshots.rs` into
  `design/screenshots/`. New: `phone-w4-send-to`, `-to-end` and `-to-keyboard`,
  `phone-w4-send-amount` and `-amount-end`, and `phone-w5-sign` and `-sign-end`.
  Changed: `phone-w4-send-cannot-spend`, `phone-w5-review` and `-end`. Removed:
  `phone-w4-send`, `-end` and `-keyboard`, replaced by the page 1 images. The
  Sent (W6), re-sign (W8) and submit (W9) images are unchanged.
- docs/SCREENS.md's W4 to W6 rows, their compact rows and the Back rule describe
  the pages. docs/THREAT_MODEL.md's Android Locking row says that Back on pages
  2 to 4 steps back a page and does not lock the application.
- The page images await the owner's approval, which D40 asks for by approving
  the images. They are the committed Linux renders, made in a Linux container.
  Untouched screens already differ by a few pixels on macOS, so the images are
  not redrawn there.

Readings for the owner, with item 7:
- Item 7 (a) and (c) are built as written: a refused signing returns to page 2,
  where "Review spend" is, and the key callout and the three steps are on page
  4, with "Sign & submit".
- Back on page 1 is unchanged. Item 2 gives a Back at the top of each page, but
  page 1 is the first page and has none. Android's Back there keeps its meaning
  (`Host::back`), which moves the application to the background and locks it,
  as it does today. Item 3's "pages 1 to 4" is read as pages 2 to 4.

Judgment calls that D40 does not settle, for the owner to note:
- A tap on the Send tab during the flow keeps the whole page, not only the form.
- Each page of the flow starts at its top when it is entered.
- While the worker answers a command the page sent, Back does nothing, and
  Android's Back on pages 2 to 4 is used up without reaching `host::back`.
- Duplicates, amounts, the fee and the block-to-live are refused beside "Review
  spend", when it is pressed. "Continue" checks only the addresses and the
  references.
- A Paste read that comes back once the person is on page 2 is dropped, since
  page 2 has no field to paste into (`form_of`).

Not built, and open:
- Item 6, the note on S7 after a lock, is not built. It is new wording, and the
  owner approves it first.
- On a phone held sideways, 600 to 1023 wide, the pages draw without Paste
  buttons, since Paste is drawn in the compact layout only. Whether Paste
  follows the phone shell, as W1's sections do (D39 item 5), is for the owner.

Corrected line references:
- Items 2, 3 and 5 gave lines that were stale at b67e90c, the base of this
  work. The empty form for the Send tab is built in `open`
  (`crates/app/src/app/wallet.rs:1198-1201`), not at 929-932; `host::back` is at
  `crates/app/src/app.rs:1582`, not 1193; and `form_of` is at
  `crates/app/src/app/wallet.rs:773` and `form` at 1301, not 691 and 1029.
  Item 4's references were right at b67e90c: `parse_destination` is at
  `crates/wallet-core/src/spend.rs:151`, and the page's top, where the refusals
  are drawn, at `crates/app/src/screens/wallet.rs:395-408`. As built,
  `parse_destination` is at `crates/wallet-core/src/spend.rs:153` and
  `parse_reference` at 200.
- The paragraph above gave docs/SCREENS.md's Back rule at lines 108 to 110, its
  W4 to W6 rows at 62 to 64 and its compact rows at 134 to 136. At b67e90c they
  are at 112 to 114, 64 to 66 and 139 to 141.

### D41. A QR code on Receive

**proposed; the feature decided by the owner on 2026-10-13; the rest
recommended.**

1. **iced's `qr_code` feature is approved (owner, 2026-10-13).** It adds
   the fork's `qr_code` widget and the `qrcode` crate it takes (0.13.0, MIT
   OR Apache-2.0, default features off). `ICED_FEATURES` in
   `crates/wallet-core/tests/policy.rs` grows to eleven; its comment says
   that `qr_code` comes only with the owner's approval, and this is it.
2. **What is encoded (recommended).** The Base58 destination, exactly as
   Receive prints it, with no URI prefix, since none is established for
   Mochimo and the parser would need changing. Never the hex fallback,
   which has no checksum, and never the ledger address.
3. **How it is drawn (recommended).** Error correction level Q, version 3.
   The encoder chooses the mode, byte or alphanumeric, depending on the
   destination's characters. Black modules on a white card with a quiet
   zone of four modules, since the widget's palette colours come out
   inverted on Tawara's dark theme. Each module is 7 logical pixels, so the
   code is 259 pixels square on a phone at 360 wide, and the same on
   desktop; 8 would make it 296 pixels wide, more than the 294 a phone's
   card holds inside its border and padding. The size is listed for the
   owner's approval through the phone image `phone-w2-receive-end.png`, the
   one that shows the whole code. Receive has no design rendering (01 to
   05), so the owner approves its layout by the redrawn `w2-receive.png`,
   `phone-w2-receive.png` and `phone-w2-receive-end.png`.

OS libraries: none added.

### D42. QR scanning on phones

**proposed; items 1 to 3 decided by the owner on 2026-10-13, in outline;
the details recommended.** Item 6 asks that Send can scan a QR code.

1. **Phones only (owner, 2026-10-13).** The desktop has no scanning, and
   rendering 03's "Scan QR" is not drawn on the desktop's Send: a desktop
   camera is uncommon, and there the destination is usually pasted. This
   supersedes D27 item 5's exclusion of a camera dialog, for phones.
2. **iOS: Apple's scanner (owner, 2026-10-13).** A native scanner screen,
   which the shell presents as it presents the share sheet, with
   `AVCaptureMetadataOutput`, so Apple decodes the code and draws the
   preview. No decoder crate and no iced `image` feature are needed.
   (Recommended) the bindings are declared by hand on objc2, or come from a
   bindings crate whose name and version are confirmed with `cargo info`
   before it is pinned.
3. **Android: the NDK camera and `rqrr`, after a device spike (owner,
   2026-10-13).** Camera2 through the NDK and `AImageReader`, using the Y
   plane, decoded by `rqrr` with its default features off, and the preview
   drawn in iced. (Recommended) the camera permission is asked through JNI
   and then polled, since a NativeActivity is not expected to receive the
   result; that is an inference, to be checked on a device. This is
   without Java, and it is the largest single piece of these nine items.
   (Recommended) the spike puts about 50 real captures through each
   decoder, in varied light, angle and distance.
4. **The scanned text is untrusted (recommended).** It is trimmed, bounded
   in length and refused unless it is UTF-8, and `parse_destination`
   decides whether it is a destination. It fills only the row it was asked
   for, is dropped if the page changed while the camera ran, and never
   reaches a password or phrase field. The camera stops when the page is
   left and on a move to the background. A swapped sticker gives a valid
   but wrong destination, which only the review catches, as with Paste.
   The threat model gains the camera as an input.
5. **Open.** iced's `image` feature, which Android's preview needs and
   which reaches the desktop builds as well, is a new feature for the
   owner's approval before the Android work (D32 item 9). The usage text,
   `NSCameraUsageDescription` on iOS, and the Android manifest's
   `android.permission.CAMERA`, with `android.hardware.camera.any` not
   required, are for the owner's approval with the work. Tawara-mobile's
   `MOBILE_DEPENDENCIES` gains `ndk-sys` and any new crate.

docs/SCREENS.md's W4 row, which says "Scan QR" is not built, changes with
the work. OS libraries: AVFoundation on iOS; `camera2ndk` and `mediandk`,
which holds `AImageReader`, on Android, linked by Tawara-mobile. `ndk-sys`
declares their functions, but links `mediandk` only behind its `media`
feature, which Tawara-mobile's build does not turn on today, and links
nothing for `camera2ndk`.

### D43. A default node: `https://api.mochimo.org`

**proposed; the default decided by the owner on 2026-10-13; the safeguards
recommended; item 6's rules proposed, for the owner to revisit; the operator
and the threat model's acceptance open.**

1. **The default (owner, 2026-10-13).** `https://api.mochimo.org` is
   chosen in advance on desktop and phone, and the person can change it to
   any other node or forget it. The owner's reason: "many users will not be
   aware of the functionality of nodes. These users should not have to
   suffer." The owner confirmed that the endpoint works in both wallets,
   and the iPhone simulator's Explorer read live chain data through it.
   This supersedes PLAN 4.8's "There is no default node" and the same rule
   in D22, which is itself still proposed. The library's command line keeps
   its own rule ("--node has no default precisely because the operator
   chooses whom to believe", library `FORK.md:168`): a person who runs it
   chooses a node. Tawara's people may not know what a node is, and the
   owner decided that they should not be made to choose before the wallet
   works. The node's rules are otherwise unchanged: https, or http to
   loopback, and the library's bundled roots (D22).
2. **Safeguards (recommended).** The default lives in
   `tawara-wallet-core`'s `Preferences::default()`. A remembered "none"
   (`node = none`) keeps "Forget the node" forgotten across a restart; once
   the default is a node, an absent or empty `node` line means the default.
   No node is asked at launch until a wallet is opened or the person
   presses Check now, which keeps true D32 item 9 (h)'s note that start-up
   needs no network (its reference to `app.rs:898-900` is now lines 1087
   and 1088). Asking the person to confirm a suggested node was considered
   and not taken: it would still ask someone who does not know what a node
   is to act.
3. **Open: who operates `api.mochimo.org`.** No public source found names
   the operator. One check showed a certificate from Google Trust Services
   (WE1) for `CN=mochimo.org`, valid from 2026-08-25 to 2026-11-23, and a
   reply with `server: cloudflare` and a `cf-ray` header, so Cloudflare is
   in the path. (Recommended) the owner names the operator, and S2 and W11
   name the operator and Cloudflare and say what they see, in words the
   owner approves.
4. **Open: the threat model's T27 and T28 for those who keep the default
   (PR #21).** Everyone who keeps the default sends their tags, network
   address and timing to one operator and through Cloudflare (T28), with
   no second node to compare with (T27), and one endpoint carries every
   default user. (Recommended) the owner accepts both for those users, with
   Cloudflare named as a party under T28.
5. **Records that follow.** docs/SCREENS.md's S2 row; the comments that
   say there is no default (`crates/wallet-core/src/node.rs:3-5`,
   `crates/wallet-core/src/command.rs:50-51`); the S2 text
   (`crates/app/src/screens/first_run.rs:384-388`) and the W11 text
   (`crates/app/src/screens/wallet/settings.rs:487-489`), which say "There
   is no default node"; and the release privacy answers and review notes
   in PR #22's docs/RELEASE.md, which say the app talks only to the node
   the person chooses. The certificate's renewal, due by 2026-11-23, is
   tracked, and the chain checked again on each platform after it.
6. **A file that cannot be read, and a choice that cannot be saved (proposed;
   recommended; the owner may revisit).** PR #29's finding 7 asked for these
   rules to be recorded here, since items 1 to 5 cover the default and
   `node = none` only. The copy the person sees is PR #29's wording list,
   which is provisional and for the owner's approval.
   - A preferences file that cannot be read (a folder at its path, or bytes
     that are not text) gives no node, not the default. The wallet uses no
     node until the person chooses one or forgets it. A missing file still
     gives the default.
   - While the file cannot be read, an ordinary change (the unit, the
     auto-lock period, the store last opened) does not write over it.
     Choosing a node, or forgetting it, does, since that is the person's
     choice.
   - While the file cannot be read, Forget is offered on S2, beside Back,
     and on Settings' node card, beside Check now, each with a note. S2
     also offers Forget when a node is set; Settings offers it only in
     that state. A Forget that succeeds clears any address or error left
     on the page.
   - A choice or a forget that cannot be saved is said on the screen it
     was made on, and that screen does not go back as if the choice were
     saved. With no folder for the preferences, the node is used, or
     forgotten, for this run only, and the note says that the next start
     uses the default.

No library change. OS libraries: none added.

### D44. The recovery phrase on one screen

**proposed; the order decided by the owner on 2026-10-13; the details
recommended.** Item 8 asks for the whole phrase in one frame, with no
scrolling. On a 360 × 640 phone that fits only if the library's warning
moves off the frame or is shortened, and the warning is the library's text
(PLAN section 3, D25).

1. **Warning first, then the phrase (owner, 2026-10-13).** On short
   phones the library's warning comes first, on a frame of its own, whole
   and unchanged. Then all 24 words appear in a grid of three columns and
   eight rows on one frame, with the checkbox and Continue. Larger phones
   and the desktop show everything on one frame. The phrase itself is never
   split across frames.
2. **The grid (recommended).** Three columns by eight rows on every size,
   read down the columns as the desktop's already is, at 14-pixel words.
   A phone counts as short below about 720 usable pixels. The tests measure
   the fit at 360 × 640, 360 × 800, 390 × 844 and 1024 × 700, and the
   screenshot check gains a check of the bottom edge.
   **Amended (owner, 2026-10-13):** a phone counts as short below 733
   usable pixels. That is the frame's height at 360 wide, measured at
   732.4 and rounded up (`PHRASE_ONE_FRAME` in
   `crates/app/src/screens/first_run.rs`). A phone of 733 pixels or more
   draws one frame. This amends the "about 720" above, which stays as
   written.
3. **iOS (recommended).** A warning follows a screenshot taken while the
   phrase is shown (`UIApplicationUserDidTakeScreenshotNotification`, which
   arrives after the capture and cannot block it), and the grid is blanked
   while the screen is reported as recorded or mirrored, as far as iOS
   reports it. Reports from developers say the capture state can read
   inactive while a recording runs, so the blanking is a best effort and
   the copy must not promise it.
4. **Android and the desktop (recommended).** Android keeps `FLAG_SECURE`
   (D32 item 4). Android's guide to securing sensitive activities says that
   in some cases the flag "does not correctly predict if screen recording
   is active", and that on Android 11 (API 30) and lower it helps around
   70% of devices reliably, because on some devices keyboard taps can be
   recorded. So no comment or text may say that `FLAG_SECURE` stops every
   recording, or that it stops recording on every device. The desktop
   keeps no capture protection, as the threat model in PR #21 accepts.
5. **Open:** "Show the phrase", the screenshot warning and the
   placeholder shown while recording are new wording, for the owner's
   approval.

The phone S4 images are redrawn for the owner's approval (D32 item 10
(c)), and docs/SCREENS.md's rule that S4's twelve rows are two columns
(line 129) changes with the work. So does the threat model's T16 (PR #21,
`docs/THREAT_MODEL.md:141`): on iOS it gains the screenshot warning and the
best-effort blanking of item 3, and on Android the recording caveat of item
4. OS libraries: none added; the iOS observers use UIKit, which the shell
already links.

### D45. Unlock with Face ID, Touch ID or the passcode, on iOS

**proposed; items 1 to 3 decided by the owner on 2026-10-13; the details
recommended.** Item 9 asks that the keystore can be opened with the
system's credentials or biometrics. Today the store opens only with its
password, typed on S7, and on a phone every move to the background locks
it (PLAN 4.1, D32 item 6).

1. **iOS only, for now (owner, 2026-10-13).** Android keeps the Tawara
   password. No Java is added, so D4 and D32 item 10 (i)'s reading of it are
   unchanged; Android may be revisited later. **Open:** the desktop.
   (Recommended) later, not in this work: Touch ID on macOS needs a signed
   app with the data-protection keychain's entitlement (D36's packaging),
   Windows Hello needs WinRT bindings, and Linux has no standard.
2. **The device passcode is accepted (owner, 2026-10-13).** The system's
   prompt may accept Face ID, Touch ID or the device passcode, so the
   Keychain item uses the `.userPresence` access control, not biometrics
   only. Anyone who knows the phone's passcode can then open the wallet,
   and the threat model (PR #21) records it. (Recommended) the item is
   created with `kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly`. With
   `.userPresence` a change to the enrolled faces or fingers does not
   invalidate the item, as `.biometryCurrentSet` would; adding one needs
   the passcode, which this decision already trusts. This supersedes the
   design put to the owner before the answer, biometrics only with
   `.biometryCurrentSet`, in which a new enrolment invalidated the item;
   the test and the device check that expected that invalidation are
   dropped.
3. **PLAN 4.2 gains one exception (owner, 2026-10-13).** Passwords are
   never written to disk, except that on iOS, only when the person turns
   this on, the store's password is kept in the Keychain, released only
   after the system's check. The Tawara password always remains the way
   in, and its field stays on S7.
4. **How (recommended).** Off by default, and offered once after a
   successful unlock with the password. The password is kept only once it
   has opened the store, tied to a random store identifier in the private
   folder, since a Keychain item can outlive an uninstall; an item whose
   identifier does not match is deleted, not used. A failed unlock with the
   kept password deletes it, and turning the unlock off in Settings deletes
   it. The password read back moves into `Zeroizing` at once, as a typed
   one does. S7 offers the unlock when the app returns to the foreground,
   and prompts once. The `Host` additions are optional fields, `None` where
   a shell has none. That differs from `Host::share`, which D32 item 10 (i)
   makes a field every shell must fill: a shell may leave these empty, and
   the app then offers no unlock. Like `Host::share`, they are fields of a
   struct each shell builds by name, so adding them breaks both shells'
   literals, and the Tawara-mobile pull request that takes them fills both
   shells' `Host` in one commit.
5. **Risks, recorded.** The password lives in a new place, held by the
   system's secure hardware. A face or a finger can be compelled, and a
   passcode can be watched or shared. Whether a Keychain item survives an
   uninstall is to be checked on a device.
6. **Open:** the offer, its Settings text and `NSFaceIDUsageDescription`
   are new wording, for the owner's approval. A device with Face ID and an
   iPhone SE with Touch ID are needed for the checks; the simulator tests
   them only in part.

docs/SCREENS.md's S7 row changes with the work. OS libraries:
Security.framework and LocalAuthentication, on iOS, linked by
Tawara-mobile.
