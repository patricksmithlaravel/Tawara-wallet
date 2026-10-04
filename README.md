# Tawara

A graphical wallet for Mochimo v3, for Windows, macOS, Linux, Android and
iOS, written in Rust with the [iced](https://iced.rs) toolkit.

Tawara is built on `mochimo-crypto`, the wallet library of the command-line
wallet (`patricksmithlaravel/mcm-rust-cli-windows`), which it uses as a
dependency pinned to an exact commit. Every safety property of the wallet
lives in that library: the encrypted keystore whose one-time key index only
ever moves forward, reconciliation against the chain, and the refusals that
protect a user from signing twice with one key. This repository holds only
the application around it, and never modifies the library.

Tawara is not an official product of the Mochimo cryptocurrency.

## Status

**Phase 2: `wallet-core`.** There is no interface yet. Phase 0 (the
repository, its rules and CI) and phase 1 (the feasibility spikes, in
`spikes/`, with the owner's decision to go ahead with iced on mobile) are
done. `crates/wallet-core` holds the worker that owns the store and the
wallet, its commands and events, locking, secret input, entropy and the
store's location, tested end to end against a scripted node. The plan, its
phases and what each is done when are in [docs/PLAN.md](docs/PLAN.md); the
decisions made along the way, and the questions open for the owner, are in
[docs/DECISIONS.md](docs/DECISIONS.md).

## Layout

```
crates/
  wallet-core/   everything that touches the library; no interface dependency
  app/           the iced application: state, messages, screens, theme
  desktop/       the Windows, macOS and Linux entry point
  mobile/        the Android and iOS entry points
platform/
  android/       manifest, backup rules and shims (phase 4)
  ios/           Xcode project and Info.plist (phase 4)
design/
  renderings/    the owner's reference renderings
  INDEX.md       what each rendering shows, and its size
docs/
  PLAN.md               the approved plan
  DECISIONS.md          every decision and deviation, with its reason
  LIBRARY-PROPOSALS.md  changes to the library proposed for Rep-0
```

## Building

The compiler is pinned in `rust-toolchain.toml`; `rustup` installs it on
first use. TLS uses `ring`, which compiles C, so a C compiler is needed
(the platform's usual one: MSVC on Windows, Xcode's on macOS, `cc` on
Linux).

```
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
cargo deny check            # needs cargo-deny 0.20.2 and the network
```

CI runs all of these on Windows, macOS and Linux
(`.github/workflows/ci.yml`).

## Licence

Tawara is distributed under the wallet's licence, the Mochimo Cryptocurrency
Engine License Agreement, version 1.0 ([LICENSE.md](LICENSE.md)), the same
text as the library's. Third-party dependencies are held to permissive
licences by [deny.toml](deny.toml).
