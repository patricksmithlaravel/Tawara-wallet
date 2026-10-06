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

**Phase 4: the mobile shells.** The desktop application is complete for
Windows, macOS and Linux: every screen in [docs/SCREENS.md](docs/SCREENS.md)
is built, and CI builds and tests it on all three. It is not yet packaged:
installers and signed packages are phase 5. Phase 4 runs the same
application on Android and iOS. The plan, its phases and what each is done
when are in [docs/PLAN.md](docs/PLAN.md); the decisions made along the way
are in [docs/DECISIONS.md](docs/DECISIONS.md).

## Layout

```
crates/
  wallet-core/   everything that touches the library; no interface dependency
  app/           the iced application: state, messages, screens, theme
  desktop/       the Windows, macOS and Linux entry point
  mobile/        the Android and iOS entry points
platform/
  android/       manifest, backup rules, build and check script
  ios/           Info.plist, build and check script
design/
  renderings/    the owner's reference renderings
  INDEX.md       what each rendering shows, and its size
docs/
  PLAN.md               the approved plan
  DECISIONS.md          every decision and deviation, with its reason
  LIBRARY-PROPOSALS.md  changes to the library proposed for Rep-0
```

## Compiling the desktop application

There is no installer yet (phase 5), so the desktop application is built
from source. The same steps work on Windows, macOS and Linux.

1. **Install Rust** with [rustup](https://rustup.rs). You do not need to
   pick a version: the compiler this repository uses is pinned in
   `rust-toolchain.toml`, and `rustup` installs it the first time you run
   `cargo` here.
2. **Install a C compiler.** TLS uses `ring`, which compiles C:
   - Windows: the Visual Studio Build Tools, with the "Desktop development
     with C++" workload (MSVC);
   - macOS: Xcode's command-line tools, `xcode-select --install`;
   - Linux: the system's compiler, for example `sudo apt install
     build-essential` on Debian or Ubuntu.
3. **Get the source:**

   ```
   git clone https://github.com/patricksmithlaravel/Tawara-wallet.git
   cd Tawara-wallet
   ```

4. **Build it:**

   ```
   cargo build --release --locked -p tawara-desktop
   ```

   The first build downloads the dependencies, the wallet library among
   them (from GitHub, at the commit `Cargo.toml` pins), so it needs the
   network, and it takes several minutes. `--locked` builds exactly the
   versions in `Cargo.lock`.
5. **Run it:** the program is `target/release/tawara-desktop`
   (`target\release\tawara-desktop.exe` on Windows). You can copy it
   anywhere; it needs nothing beside it. `cargo run --release -p
   tawara-desktop` builds and runs in one step.

Build with `--release`: in a debug build, unlocking a store (Argon2id at
64 MiB, three passes) takes many seconds.

On Linux the application needs an X11 or Wayland desktop session, with
`libxkbcommon` (present on most desktops). It draws with the GPU through
wgpu and falls back to software rendering when there is none; to force
software rendering, for example in a virtual machine, run it with
`ICED_BACKEND=tiny-skia`.

The store goes, by default, under `%LOCALAPPDATA%\Tawara` on Windows,
`~/Library/Application Support/Tawara` on macOS and
`$XDG_DATA_HOME/tawara` (or `~/.local/share/tawara`) on Linux
(docs/DECISIONS.md D21). The application asks for a node the first time it
starts; there is no default node.

## Developing

The checks CI runs (`.github/workflows/ci.yml`), on Windows, macOS and
Linux:

```
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
cargo deny check            # needs cargo-deny 0.20.2 and the network
cargo run -p tawara-app --example screenshots -- --check
```

The last one draws every screen and compares it with the images committed
in `design/screenshots/` (run it without `--check` to draw them again). The
Android and iOS builds and their checks are in `platform/android/` and
`platform/ios/`, and run in `.github/workflows/mobile.yml`.

## Licence

Tawara is distributed under the wallet's licence, the Mochimo Cryptocurrency
Engine License Agreement, version 1.0 ([LICENSE.md](LICENSE.md)), the same
text as the library's. Third-party dependencies are held to permissive
licences by [deny.toml](deny.toml).
