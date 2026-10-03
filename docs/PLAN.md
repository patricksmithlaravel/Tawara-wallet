# Rep-2: the graphical wallet — plan

Status: approved by the owner on 2026-10-03. This file is the reference for
every decision below; a change to any of them is recorded in
`docs/DECISIONS.md` with its reason and the owner's approval.

## 1. Decisions

### D1. Rep-2 depends on the wallet library; it does not fork it

Rep-2 is a new repository that contains only the graphical application. It
uses `mochimo-crypto` from Rep-1
(`patricksmithlaravel/mcm-rust-cli-windows`) as a library, pinned to an
exact commit or tag:

```toml
[workspace.dependencies]
mochimo-crypto = { git = "https://github.com/patricksmithlaravel/mcm-rust-cli-windows",
                   rev = "<full commit hash>", default-features = false,
                   features = ["native", "mesh-https"] }
```

This replaces `FORK.md`'s "Fork point 2", which planned for Rep-2 to copy
the code from Rep-0 and later merge in Rep-1's Windows changes. That plan's
only stated reason was scheduling. It lapsed once Rep-1's Windows work
landed, passing on Linux, macOS and Windows.

Consequences, all deliberate:

- **No merge-downs.** Upgrading the library is a change to `rev`.
- **Rep-2 never modifies the library.** No `[patch]` section, no vendored
  copy, no fork of the crate. A change the application needs from the
  library is made in Rep-0 (`patricksmithlaravel/mcm-rust-cli-wallet`),
  merged down into Rep-1, and then picked up here by bumping `rev`.
- **Two of `FORK.md`'s Phase 2 items are met by construction.** R2-1 (GUI
  code kept out of the library's `crates/`) and R2-3 (GUI dependencies kept
  out of the library's `deny.toml`) both hold because the library's tests,
  scans and `deny.toml` never see this repository. Rep-2 has its own
  `deny.toml`, toolchain pin and CI.

### D2. The toolkit is iced

This replaces `FORK.md` R2-2's choice of egui. The reasons:

- iced's model (state, messages, `update`, `view`, subscriptions) fits an
  application whose library calls all run on one background thread and
  reach the interface as messages.
- It has a pure-Rust software renderer (`tiny-skia`), used when no GPU is
  available.
- 0.14 added input-method support, headless mode and end-to-end testing,
  and system theme reactions.

Pin the latest released iced at the start of the work and record the
version. Use a git revision of iced only if mobile requires it, and record
why.

### D3. The targets are desktop and mobile

| platform | support from iced | consequence |
|---|---|---|
| Windows, macOS, Linux | official | the primary targets |
| Android | community only: `android-activity`, NativeActivity or GameActivity, Java interop for the soft keyboard and the clipboard | **feasibility spike before any mobile screen is built** |
| iOS | community and experimental | same |

The iced project does not officially support mobile. All wallet logic
therefore lives in a crate with no UI dependency (§3), so the mobile shells
can be replaced without touching it if iced on mobile falls short. Whether
to do that is the owner's decision, made after the spike (§7, phase 1).

### D4. Rust-native, with the exceptions named

- **Everything is Rust except `ring`**, which compiles C and assembly for
  TLS (`mesh-https`). The command-line wallet ships it already, and the
  library's `lib.rs` explains why it is acceptable.
- **No webview, no JavaScript runtime, no embedded browser.**
- **Mobile needs platform glue that is not Rust**, kept to the minimum and
  listed in `docs/DECISIONS.md`. On Android that is `AndroidManifest.xml`,
  possibly Gradle, and small Java or Kotlin shims (soft keyboard,
  clipboard, `FLAG_SECURE`, backup rules). On iOS it is `Info.plist` and an
  Xcode project.
- **Linking against OS libraries is not an exception.** Graphics APIs,
  windowing and system frameworks are dynamically linked by any native
  application.

## 2. What was measured before this plan (2026-10-03, Rep-1 at `02239c1`)

- **`mochimo-crypto` compiles for mobile.** `cargo check -p mochimo-crypto
  --lib` passes, with no warnings, for `aarch64-linux-android`,
  `x86_64-linux-android`, `aarch64-apple-ios` and `aarch64-apple-ios-sim`.
  That holds with default features and with `--features mesh-http`.
- **TLS needs a C compiler for each mobile target.** With `mesh-https`,
  `ring`'s build script needs one: the Android NDK's clang, and Xcode for
  iOS. This was not tested further.
- **The library has never run on Android or iOS.** Its tests run on Linux,
  macOS and Windows only. Android and iOS both build its Unix arm: the
  directory handle through `rustix`, `flock`, `fsync`, and the owner and
  mode checks. That has to be exercised on a device.

## 3. Architecture

```
<rep-2>/
  Cargo.toml              workspace
  rust-toolchain.toml
  deny.toml               this repository's own policy
  crates/
    wallet-core/          no UI dependency (see below)
    app/                  iced application: state, messages, screens, theme; a lib crate
    desktop/              bin: Windows, macOS, Linux entry point
    mobile/               cdylib/staticlib: Android and iOS entry points
  platform/
    android/              manifest, backup rules, Gradle if needed, minimal Java/Kotlin shims
    ios/                  Xcode project, Info.plist
  design/
    renderings/           the owner's reference images, unmodified
    INDEX.md              each rendering: file, screen it shows, platform, size
  docs/
    PLAN.md               this file
    DECISIONS.md          every decision and every deviation, with its reason
    THREAT_MODEL.md       the restated threat model (§4); required before any release
    SCREENS.md            screen inventory, mapped to renderings and to the safety states
```

**`wallet-core`** owns everything that touches the library:

- **A worker thread** that owns the `Keystore` and the `Wallet`. Nothing
  calls the library on the UI thread. The transport is synchronous,
  `Wallet::open` reconciles over the network, and the password-hashing step
  (`Kdf::RECOMMENDED`) takes 64 MiB of memory over three passes.
- **Messages in both directions.** Commands go in and events come out.
  Events carry view models: plain data that holds no secret.
- **Cancellation and progress** through the library's `recon::Cancel`.
- **Store-location policy (§4.6), entropy (§4.4), and secret input and its
  zeroizing (§4.2).**
- **Lifecycle.** An idle timer, and suspend on mobile, drop the `Keystore`.
  That drops the secret and releases the store's lock in one move.

**`app`** holds the iced state, `update`, `view` and subscriptions. It never
calls `mochimo-crypto` directly: it talks to `wallet-core` only.

**Library text that protects the user is reused, not rewritten.** That
covers refusals, divergence reports, reservation states and the three
residues in §4.9. Use the library's own wording: `Error`'s `Display`, and
the decision layer (`cli::decide` returns an `Outcome` with no wording
attached) together with the renderer's text. Any paraphrase needs the
owner's approval.

## 4. The threat model, restated (written up in full in `docs/THREAT_MODEL.md`)

The command-line wallet's memory argument is that a command is a process
that exits, so it retains nothing between calls. A graphical wallet runs
for hours, and on mobile it is suspended and resumed. The following hold:

1. **Holding the seed.** An idle timeout locks the wallet by dropping the
   `Keystore`. So does every move to the background on mobile, and an
   explicit "Lock" action.
2. **Secret input.** Passwords and the recovery phrase pass through UI text
   fields. Those fields use password mode: masked, no copy or cut, and not
   exposed as a value to accessibility services. Their contents move into
   `Zeroizing` buffers at once and the field is cleared. They are never
   logged and never written to disk.
3. **Showing the phrase.** It is shown once, at creation, behind a
   confirmation, and is never copied to the clipboard.
   - Android: `FLAG_SECURE` on every screen that shows a secret.
   - iOS: content is hidden when the app becomes inactive, so the app
     switcher's snapshot shows nothing.
4. **Entropy.** It comes from the operating system's generator, as the
   command-line binary draws it (`/dev/urandom`, `BCryptGenRandom`). The
   library takes entropy as a parameter. Record the mechanism chosen.
5. **Backups and sync are a key-reuse hazard.** Restoring an older copy of
   the store rolls back the one-time key index and any open reservation.
   That is the path to signing twice with one key, which the library's
   README warns against. So the store directory must be excluded from
   every backup and sync:
   - Android: Auto Backup and device transfer
     (`android:allowBackup="false"` plus `dataExtractionRules` that exclude
     it);
   - iOS: iCloud and device backup (`NSURLIsExcludedFromBackupKey` on the
     directory);
   - desktop: the default location is outside cloud-sync folders, and the
     app warns if the user chooses one.
6. **Store location defaults:**
   - Windows: under `%LOCALAPPDATA%` (not Roaming);
   - macOS: `~/Library/Application Support/<app>/`;
   - Linux: `$XDG_DATA_HOME/<app>/`;
   - Android: the app's private files directory;
   - iOS: Application Support in the app container, with a file-protection
     class the owner chooses.

   The library's own checks still apply: owner and mode on Unix (Android
   and iOS included), access lists on Windows, and refusal of a linked
   store directory.
7. **One writer.** The store's lock is held while the wallet is unlocked.
   `Error::Locked` is explained in a sentence: another window, or the
   command-line wallet, has this store open.
8. **The node.**
   - TLS is required, as the command-line wallet requires it: plain HTTP
     is refused except to loopback.
   - There is no default node. The user chooses whom to trust, as with
     `--node`.
   - Trust roots, per platform, are recorded in `DECISIONS.md`. The
     options are the bundled `webpki-roots` (what the command-line wallet
     uses) and the platform's own roots (`FORK.md` expects Rep-2 to weigh
     them).
9. **Failing closed in front of a person (`FORK.md` R2-7, R2-8).**
   - These states each get a designed screen, never an OK dialog:
     - an account diverged from the chain;
     - reserved and not settled;
     - a reservation that can no longer be accepted;
     - an account emptied to zero;
     - a spend between submission and settlement.
   - **No button anywhere resets, deletes or restores the store, or moves
     a key index, to make a refusal go away.**
   - Three facts are never softened:
     - a submit is a socket write, not a verdict;
     - the node's transaction validation has never run offline;
     - the signed transaction (the retry artifact) can be lost.

     So that it is not lost, the app offers to save it as a file.

## 5. Visual design from the owner's renderings

- **Source.** The renderings in `design/renderings/` are the visual
  reference. Do not edit them; index them in `design/INDEX.md`.
- **Theme.** Derive the design tokens from them (colour, type scale,
  spacing, corner radii, elevation, icon style) into one theme module in
  `app`. Build a dark variant only if the renderings show one.
- **Layout.** One code base adapts to three width classes: compact
  (phone), medium, and expanded (desktop). It respects safe areas on
  mobile.
- **Fonts.** Bundle the fonts the renderings use, and record each font's
  licence (usually OFL-1.1) for the notices.
- **Checking.** Generate screenshots of every screen headlessly, at the
  renderings' sizes, and compare them side by side with the renderings.
  Keep the screenshots as CI artifacts.
- **Conflicts.** Where a rendering conflicts with §4, §4 wins and the
  owner is asked. Examples: a reset button, a refusal hidden or softened, a
  phrase on screen next to a copy button.

## 6. Packaging, licences, release (`FORK.md` R2-9, R2-10, Licence)

- **Desktop:**
  - Windows: an installer signed with Authenticode;
  - macOS: a `.app` in a `.dmg`, signed and notarised;
  - Linux: an AppImage and a `.deb`.

  Choose one packaging tool (for example `cargo-packager`) and record it.
- **Mobile:** an Android App Bundle with an upload key, and an iOS archive.
  This needs an Apple Developer account and a Play Console account.
- **Lead time.** Start on the signing certificates, notarisation and store
  accounts at the beginning; they take time.
- **Notices.** Generate the third-party notices in CI (for example with
  `cargo-about`), not by hand.
- **Licences.**
  - Rep-2's `deny.toml` admits permissive licences only.
  - GPL is excluded (see `FORK.md`'s Licence section).
  - Every entry in `ignore` carries a written reason.
  - The executable is distributed under the wallet's licence.
  - The two questions `FORK.md` raises for counsel are answered before
    first distribution.
  - The app is not branded as an official product of the cryptocurrency.

## 7. Phases

| phase | contents | done when |
|---|---|---|
| 0 | repository, rules, CI skeleton | workspace builds and is clean under clippy on 3 desktop OSes in CI; `cargo deny` green; library pinned |
| 1 | feasibility spikes (gate) | written report to the owner: iced on Android and iOS (text input with soft keyboard, clipboard, suspend/resume, rendering), and the library on device (store create/open/commit in app-private storage, a TLS request to a node). **The owner decides go/no-go for iced on mobile.** |
| 2 | `wallet-core` | worker, commands and events, lifecycle, secret handling and entropy, store policy; unit-tested against a fake `mesh::Transport` |
| 3 | desktop screens from the renderings | every screen in `SCREENS.md`, including the §4.9 states; headless screenshots generated |
| 4 | mobile shells | same `app` crate on Android and iOS; backup exclusion, `FLAG_SECURE`, safe areas, lifecycle locking |
| 5 | release readiness | `THREAT_MODEL.md`, notices, signed packages for every platform, release checklist |

## 8. Upstream follow-up

`FORK.md` in Rep-0 still says Rep-2 forks from Rep-0 (Fork point 2) and that
the toolkit is egui (R2-2). D1 and D2 supersede both. The owner updates
`FORK.md` in Rep-0, and it flows down into Rep-1.
