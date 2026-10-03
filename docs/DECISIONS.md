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

### D7. The library pin: Rep-1 at `121daf3`

**owner, 2026-10-03.** The owner's instructions for phase 0 named Rep-1's
`02239c1`, or a later commit the owner named; by the time the work began, Rep-1's `main` was at
`121daf3773fbc7869ee56c2c795e21115044dfe7`, which adds the name Tawara, the
binary `tawara`, and the updated `FORK.md` (Rep-2 as a dependent, iced in
place of egui). The library's API did not change between the two. The owner
chose `121daf3`.

The workspace line takes the plan's form (D1) exactly:

```toml
mochimo-crypto = { git = "https://github.com/patricksmithlaravel/mcm-rust-cli-windows",
                   rev = "121daf3773fbc7869ee56c2c795e21115044dfe7",
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
  code"). The check walks the module tree of every non-test target of every
  crate (library, binaries, build script, examples) from its root, following
  `mod` declarations and `#[path]` as rustc does. It removes what only test
  builds compile (`#[cfg(test)]`, `#[cfg(all(test, ..))]`, `#[test]`, a
  file's own `#![cfg(test)]`, on items, statements, match arms, fields and
  variants), so an out-of-line `#[cfg(test)] mod tests;` is not followed. It
  looks for the identifier, raw or not, in what is left, macro bodies
  included. Integration tests and benchmarks are not in that tree.
  `spikes/` is outside the walk: it is phase 1's throwaway code and is never
  built into a shipped artifact;
- `wallet-core` has no interface dependency anywhere in its dependency
  closure as `Cargo.lock` resolved it; the interface crates never depend on
  the library (by any name) or name `mochimo_crypto` in any source; and no
  module `wallet-core` compiles re-exports the library wholesale.

Each check was seen to fail on a deliberately broken copy of the tree: a
`[patch]` section, `raw-backend` on the workspace line, a crate's
`[features]` forwarding `mochimo-crypto/raw-backend`, a renamed library
dependency in `app`, a `paths` override in `.cargo/config.toml`,
`overflow-checks = false` at the top level and in
`[profile.release.package."*"]`, rustflags turning overflow checks off, the
cheap KDF named in `app` (also as a raw identifier) and in a production
module of `wallet-core` two files down, `mochimo_crypto` named in `app`,
`pub use mochimo_crypto` at the top of `wallet-core` and inside a nested
module, and iced in `wallet-core`'s dependencies. Each passes on the tree as
committed, and an out-of-line `#[cfg(test)] mod tests;` naming the cheap KDF
passes, as it should. Three more tests hold the detectors to fixed cases
and to a small crate laid out on disk, so a detector that sees nothing
cannot pass.

### D16. Edition 2024

**proposed.** This workspace's crates use Rust edition 2024 and resolver
3, as iced 0.14.0 does. The library keeps its own edition (2021); editions
are per crate and do not interact.
