# Release checklist

The steps from a green `main` to Tawara in people's hands, in the order they
are done. Section 2 is the exception: it lists the threat model's items and
names for each the gate it closes at, section 3's or section 8's.
docs/PLAN.md section 7 makes this checklist a condition of phase 5; D36 in
docs/DECISIONS.md proposes the tools it uses. It covers both repositories:
this one, which builds the desktop packages, and
[Tawara-mobile](https://github.com/patricksmithlaravel/Tawara-mobile), which
builds the phone packages from `tawara-app` pinned here (D33). The threat
model is D34 (docs/THREAT_MODEL.md); the third-party notices are D35.

Each step says who does it. **Owner** steps are decisions, accounts, keys and
device checks that only the owner can supply (D32 item 2). **Delegated**
steps are engineering work done through pull requests the owner reviews. A
step that does not apply to a release says why in that release's record
(section 11).

The store, licence and tool facts here are cited to the numbered sources in
D36's source list ("D36, source N"), each with its address and the date it
was checked. Stores change their rules every year, so each is confirmed in
App Store Connect or Play Console before the step that depends on it.

Section 1 is long-running and starts first: certificates, store accounts and
counsel's answers take weeks (PLAN section 6, "Lead time").

## 1. Owner prerequisites

### 1.1 The proposals to decide

D36 is a proposal, not yet a decision. Its owner-decision list (D36, "What
the owner decides") holds every choice in it that is the owner's to make,
each with a recommendation:

- [ ] **The tools**, which depart from PLAN section 6's "one packaging tool"
  (owner).
- [ ] **NSIS rather than WiX** for the Windows installer, or WiX 7 under
  its fee terms (owner).
- [ ] **The installed executable's name**, `tawara-desktop` (owner; D9,
  and the threat model's section 6, item 8, which the owner closes).
- [ ] **Signing and publishing on the owner's machines only**, with CI
  building unsigned packages (owner).
- [ ] **What the first release covers**: macOS 11.0 and later as one
  universal binary; Windows x86_64 only; Linux x86_64 only, with glibc 2.35
  as the floor (owner).
- [ ] **The identifiers D8 does not yet name**: the macOS bundle identifier
  and the Linux desktop entry's name (owner).
- [ ] **Prebuilt tool downloads**, pinned by hash, where D13 builds
  cargo-deny from source; and trust on first use for NSIS (owner).

### 1.2 Accounts, certificates and keys

Nothing can be signed or submitted without these. None of them is ever put
in the repository or in CI (D13, D36).

- [ ] **Apple Developer Program, enrolled as an organisation** (owner).
  App Review guideline 3.1.5(i) reads "Apps may facilitate virtual currency
  storage, provided they are offered by developers enrolled as an
  organization" (D36, source 1). Enrolment as an organisation needs a legal
  entity and its D-U-N-S number, which Dun & Bradstreet issues, a public
  website on the organisation's domain, and an Account Holder with authority
  to bind it (D36, source 2). The fee is US$99 a year (D36, source 5).
  Confirm with App Review that Tawara falls under that guideline before
  enrolling.
- [ ] **Apple identifiers and certificates** (owner), once enrolled:
  - the App ID `com.patricksmithlaravel.tawara` (D8) with the Data
    Protection capability set to Complete Protection, which the app's
    entitlement asks for (D32 item 3);
  - an Apple Distribution certificate and an App Store provisioning profile
    for that App ID (iOS);
  - a Developer ID Application certificate (macOS);
  - the App Store Connect app record for the bundle identifier;
  - an App Store Connect Team API key with the App Manager role, used by
    `altool` for uploads and by `notarytool` for notarisation (D36, source
    4);
  - a `notarytool` keychain profile made from that key on the Mac that
    notarises (`xcrun notarytool store-credentials`). Keychain profiles do
    not move between machines.
- [ ] **Google Play Console** (owner): the account (US$25 once, with
  identity verification; D36, source 8); the app, created for
  `com.patricksmithlaravel.tawara`; and an upload keystore, generated with
  `keytool`, kept with an offline backup. Personal accounts created after
  2023-11-13 must run a closed test with at least 12 testers, opted in for
  14 days, before they may publish to production (D36, source 7). That page
  says nothing about organisation accounts, so whether an organisation
  account avoids the test is **confirmed in Play Console before the owner
  chooses the account type**.
- [ ] **Play App Signing** (owner): enrolled at the first upload, so that
  Google holds the app signing key and the owner holds only the upload key.
  A lost upload key can then be reset in Play Console instead of ending the
  listing (D36, source 9).
- [ ] **Windows code-signing certificate** (owner): an OV or EV
  certificate from a public certification authority. Its key is on the
  authority's hardware token (required since 2023-06-01; D36, source 18),
  so signing happens on a Windows machine that holds the token, with the
  Windows SDK's `signtool` and the authority's RFC 3161 timestamp address.
  The name Windows shows to people is the certificate's subject, not D8's
  publisher string; the owner confirms that the two agree, or which one the
  installer names.
- [ ] **Linux** (owner): the maintainer name and address for the `.deb`'s
  control file, and a GPG key if the packages and the checksum file are to
  carry detached signatures. **(Recommended)** sign the checksum file.
- [ ] **The app icon** (owner): Tawara's own mark, not the coin mark (D27
  item 2), as a square PNG of at least 1024 pixels. iOS needs it opaque at
  1024; Play's listing needs 512; the desktop icons are made from it.
- [ ] **Store listing material** (owner): the descriptions; a privacy
  policy address (App Store Connect requires one for every app); the App
  Store's privacy answers and Play's Data safety form; Play's content
  rating and target audience; Play's feature graphic (1024 by 500);
  screenshots of at least two phone screens per store. The committed
  screenshots (D27 item 1) are a starting point. The privacy answers rest
  on what the code does, which was checked at this repository's 389f731 and
  Tawara-mobile's 587329c: the only network client is the library's `ureq`
  transport, opened in `crates/wallet-core/src/node.rs` to the node address
  the person enters; no other address is compiled in outside tests and
  examples; and Tawara-mobile's crates open no connection of their own. So
  the app sends only the requests the person makes to the node they choose,
  and has no analytics or crash reporting of its own. **The check is
  repeated at the release tags** (section 5.4) before the answers are given.
- [ ] **Store rules for a wallet** (owner): Play's financial features
  declaration, and its policy on cryptocurrency software wallets, which a
  secondary summary reports to exempt non-custodial wallets from its
  licensing rules (D36, source 12); confirm both in Play Console. Tawara
  holds no one's keys but the person's own, on their own device.
- [ ] **Export compliance** (owner, with counsel): Apple asks whether the
  app uses encryption beyond the exempt kinds, and the answer is written
  into the bundle as `ITSAppUsesNonExemptEncryption`. Tawara signs with
  the library's keys, encrypts its store under a password, and talks to the
  node over HTTPS, or plain HTTP to a loopback address (D22).
  **(Recommended)** put the question to counsel together with section 1.3's
  two.

### 1.3 The two licence questions in FORK.md

PLAN section 6 and D10 require both to be answered by counsel before first
distribution. They are in `FORK.md`'s Licence section, in Rep-0
(`patricksmithlaravel/mcm-rust-cli-wallet`, PLAN section 1); this repository
points to it and does not restate the licence's terms. The summary below is
an engineering reading, not legal advice.

- [ ] **Question 1** (owner, counsel): whether `mochimo-crypto` is Original
  Software or Covered Software under the agreement's section 1. The answer
  decides which part of section 3.2 governs distributing an executable built
  on it.
- [ ] **Question 2** (owner, counsel): whether distributing the executable
  under the agreement itself, rather than sublicensing it, leaves the
  grant-back in section 3.2(a)(ii)(1) untriggered, as its text reads.
- [ ] Both answers recorded in docs/DECISIONS.md, with their date, before
  any package is published (delegated, once the owner has them).
- [ ] FORK.md reads the source-availability obligation as owed whichever
  answer counsel gives; section 9 checks it for every package.
- [ ] The application is still not branded as an official product of the
  cryptocurrency (PLAN section 6; D27 item 2): no coin mark in the app, the
  icon, the listings or the screenshots (owner, at review).

### 1.4 Moving people from the older wallets

- [ ] **Decide whether the release must import the older wallets' `.mcm`
  stores** (owner; D23 item 6). The library has no such import, so neither
  has Tawara. If it is a goal, the change is proposed for Rep-0 in
  docs/LIBRARY-PROPOSALS.md, made there, merged down into Rep-1, and taken
  here by moving the library's `rev` (D1), all before the release.
  **(Recommended)** decide before the first release, and say in the
  listings either way whether people can bring an existing wallet.

### 1.5 Android target API 36

- [ ] **Tawara-mobile targets API 36** (delegated; D32 item 9 (i), PR B,
  made after Tawara-mobile #4 and #5 under item 10 (k)). Google Play has
  required target API 36 for new apps and app updates since 2026-08-31,
  and offers an extension to 2026-11-01 on request (D36, source 6). The
  page does not say whether a new app can have the extension; this
  checklist assumes it cannot, which is an inference, so the first upload
  targets 36. PR B sets `TARGET_API=36`, pins the compile jar to
  `platforms/android-36`, uses the android-36 emulator image, and adds
  `assetsPaths` and `resourcesUnused` to `configChanges`, which `aapt2 dump
  xmltree` must then read as `0xd800ffff`. Confirm the rule in Play Console
  first (owner). Tawara-mobile PR #19 makes these changes
  (`platform/android/tawara.sh`, `platform/android/AndroidManifest.xml`); the
  rule stays the owner's to confirm.

### 1.6 The machines that build and sign

Every package is built on the owner's machines (section 6). Each needs the
following before section 6 starts.

- [ ] **Xcode 26.6 beside Xcode 27** (owner) on the Mac that builds the
  macOS and iOS packages, installed as `/Applications/Xcode_26.6.app`, the
  path the `macos-26` runner image uses (D36, source 22). The owner's Mac
  has only Xcode 27.0 (27A266a), with the iOS 27.0 SDK, and D32 item 8
  records that an app linked with the iOS 27 SDK is stopped at launch
  without a scene. The release commands select 26.6 with `DEVELOPER_DIR`,
  so the Mac's default Xcode is left alone.
- [ ] **A container runtime** (owner) for the Linux build: Docker or Podman,
  able to run `linux/amd64` images (section 6.3).
- [ ] **A Windows machine** (owner) with the code-signing token, the Windows
  SDK (`signtool`, `rc.exe`), Rust at the version `rust-toolchain.toml`
  names, and NSIS 3.13 from the pinned download (section 6.1).
- [ ] **The Android SDK** (owner) with build-tools 36.0.0,
  `platforms/android-36`, NDK 29.0.14206865 and a JDK 17 or later.
- [ ] **Tawara-mobile's scripts made to run on the owner's Mac** (delegated,
  before section 6.4; each is a change to Tawara-mobile at origin/main
  587329c):
  - the NDK's toolchain path chosen from the host.
    `platform/android/tawara.sh` line 48 fixes `prebuilt/linux-x86_64`, and
    the NDK on the owner's Mac has only `prebuilt/darwin-x86_64`;
  - the emulator check on a port and an AVD name of its own. The script's
    `SERIAL` is `emulator-5554` (line 27), the port the owner's own emulator
    uses, and line 183 runs `avdmanager create avd --force -n tawara`, which
    would overwrite any AVD of that name. The release script uses another
    port and a name made for the run, never `--force`, and deletes only the
    AVD it created;
  - the `apt-get` step skipped off Linux, and an arm64-v8a system image on
    an Apple silicon Mac, where the x86_64 image does not run.

  Tawara-mobile PR #19 makes the NDK host change, in `ndk_host`. It does not
  make the other three: the emulator's port and AVD name, the `apt-get` step,
  and the arm64 image.

## 2. The threat model's must-close items

PLAN sections 3 and 7 make docs/THREAT_MODEL.md (D34) a condition of any
release. Its section 6 lists what must be closed, or accepted by the owner in
writing in docs/DECISIONS.md, before any release. Items that need a built
package or a device close at section 8's gate; the rest close at section 3's
gate, before the tag. Each item below names its gate. A failure found at
section 3 costs no new tag; one found at section 8 costs a new version and
new packages (section 8, "When a check fails and is fixed"). In summary,
with the threat model's own identifiers:

- [ ] **Every device check, V1 to V18** (its section 5.2), run on the
  builds to be released and recorded: section 8 here (owner; section 8's
  gate).
- [ ] **G6**, the Android exit after `Destroy`: answered by D32 item 9 (j)'s
  2026-10-13 amendment. Its emulator check is repeated at the release's pin
  (delegated, then owner; section 3's gate). Its run on the phone is V4
  (section 8's gate).
- [ ] **G17**, a working save path on phones: Tawara-mobile #8 (the share
  sheet) merged, with its pin on this repository's `main` (delegated;
  section 3's gate).
- [ ] **C1**, the records on what an interrupted send can leave (D19
  against D28 and D32 item 6), made to agree, in words the owner approves
  (owner; section 3's gate).
- [ ] **G4 and C7**, the typed-secret fields' plain `String`s against PLAN
  4.2's "at once": fixed, or the deviation recorded (owner; section 3's
  gate).
- [ ] **G1, G2, G10, G12 and G24**, the owner's decisions on the
  two-wallets warning, the desktop's phrase on screen, accessibility,
  clearing a pasted phrase, and the application's wording (owner; section
  3's gate). G10's decision says what is required, and its check is V7
  (section 8's gate). G24's wording is approved on the screens the release
  commit draws, as `design/screenshots/` shows them; CI checks those images
  pixel for pixel against what the screens draw (D27 item 1).
- [ ] **G27**, iOS while the app is active: a screenshot, recording or
  mirroring of S4 is not stopped. The owner decides it after V18 has its
  recorded result (owner; section 8's gate).
- [ ] **Its section 5.4**: each residual risk accepted, or a control added
  (owner; section 3's gate). The phone part of its item 10, T16 and T31 on
  phones, waits for V1 and V7 (section 8's gate).
- [ ] **Phase 5's own items** (its item 8): the notices, the counsel
  questions, the executable's name, this checklist and target API 36
  (section 3's gate); signed packages for every platform, and UIScene
  re-checked with the Xcode that builds the upload (D32 item 8), so that
  the signed build launches on the newest iOS and on iOS 16 (section 8's
  gate). iOS 16 is the lowest version the build declares
  (`IPHONEOS_DEPLOYMENT_TARGET=16.0`, section 6.5, as Tawara-mobile's
  `platform/ios/tawara.sh` sets it). No decision records it as the floor,
  so the owner confirms it before the tag.

The threat model also lists items that may follow the first release if the
owner agrees; they do not block it. If its list and this summary differ, the
threat model's wins and this summary is corrected.

## 3. The commits to release

- [ ] **The threat model on `main`** (delegated, owner's review):
  docs/THREAT_MODEL.md merged (D34), and each item of its section 6 that
  needs no built package or a device closed, as section 2 here records. The
  rest close at section 8's gate, before anything is submitted or published.
- [ ] **Fork pin settled** (delegated): no iced fork move is in flight.
  A move during a release is its own pull request with D32 item 9 (a)'s
  checks, never part of the release.
- [ ] **This repository's `main` is green** on all three hosts, the Linux
  screenshot step and the liveness step included (delegated).
- [ ] **Tawara-mobile pins the commit being released** (delegated): its
  `tawara-app` `rev` is this repository's release commit, by full hash
  (D33), with every Tawara-mobile check green on it. The phones and the
  desktop then ship the same screens and the same refusals.
- [ ] **Notices merged** in both repositories (D35; section 4).
- [ ] **Packaging scripts merged** (delegated; D36): `packaging/` here,
  `platform/android/release.sh` and `platform/ios/release.sh` in
  Tawara-mobile with section 1.6's changes, and the packaging job green on
  each. **Desktop: built in PR #43, not yet merged.** The scripts are
  `packaging/macos.sh`, `packaging/linux.sh` (with
  `packaging/linux/in-container.sh`) and `packaging/windows.sh` (with
  `packaging/windows/tawara.nsi`), checked by `packaging/check.sh` in the
  packaging job (`.github/workflows/packaging.yml`). The job's Windows leg
  makes the installer from the NSIS archive whose SHA-256 is recorded (D36
  decision 8). Still open: the two phone scripts, in Tawara-mobile.

## 4. Third-party notices

PLAN section 6 asks for the notices to be generated in CI, not by hand. D35
records how; it is merged in both repositories before the first release.

- [ ] **Generated, and checked in CI** (delegated): `THIRD-PARTY-NOTICES.txt`
  at each repository's root is written by `ci/notices.sh` with cargo-about
  0.9.2 from `about.toml` and the lockfile, followed by `LICENSE.md`; CI
  regenerates it and fails on one differing byte. It covers every desktop
  target, build dependencies included, and the three bundled font families
  under the SIL Open Font License (D27 item 6).
- [ ] **In every package** (delegated; the packaging scripts place it, with
  `LICENSE.md`):
  - Windows: beside `tawara-desktop.exe` in the install folder
    (`packaging/windows/tawara.nsi`; PR #43, not yet merged);
  - macOS: `Tawara.app/Contents/Resources/` (`packaging/macos.sh`; PR #43);
  - `.deb`: `/usr/share/doc/tawara-desktop/`; AppImage: in the AppDir
    beside the executable (`packaging/linux/in-container.sh`; PR #43);
  - Android: the bundle's `assets/` (open; Tawara-mobile);
  - iOS: the root of `Tawara.app` (open; Tawara-mobile).
- [ ] **Shown to people** (owner): a file inside a phone app cannot be
  opened by the person using it. **(Recommended)** a Licences page in
  Settings that draws the same file, with wording the owner approves.
  **Built** (D35, as built; docs/SCREENS.md, W11a): About's "Show the
  licences" opens it, and it draws the licence and the notices. On the
  desktop the notices are the wallet's own file. A phone's shell gives its
  own, the file its package carries, as `Host::notices`, which is required,
  so a shell cannot leave the page with the wallet's file. Tawara-mobile
  passes its file when its pin moves to the commit that carries the page.
  Until then no phone has the page, so the phones' part of this item is
  open. Its wording awaits the owner's approval, so the box stays unticked
  until the owner gives it.

That the file is current at the tag is section 5.3.

## 5. Version numbers, notices at the tag, and tags

Done in this order, so that the notices name exactly what the tagged commit
builds.

### 5.1 The version

- [ ] **Choose the version** (owner): `X.Y.Z`, set in each repository's
  `[workspace.package] version` (both are `0.1.0` at this writing). The two
  must be equal for a release made from one wallet commit; Tawara-mobile's
  release script compares its own version with the `tawara-app` version in
  its `Cargo.lock` and fails if they differ (delegated). It becomes the
  desktop's displayed version, Android's `versionName` and iOS's
  `CFBundleShortVersionString`.

### 5.2 The build number

- [ ] **Raise the build number** (delegated): one integer in
  Tawara-mobile, raised by one for every upload to either store and never
  reused, because both stores refuse a build number they have seen. It
  becomes Android's `versionCode` and iOS's `CFBundleVersion`, replacing the
  development bundle's literal `1`. A rebuilt upload of the same version
  takes a new build number. The desktop has no build number.
  Tawara-mobile PR #19 keeps the counter in `platform/build-number`, at 0 until
  the first upload, raised with `platform/build-number.sh bump N`. Both release
  scripts refuse any other number. Where the counter lives is the owner's
  choice.

### 5.3 Notices current

- [ ] **Regenerated after the last lockfile change** (delegated), the
  version change of 5.1 included, with `bash ci/notices.sh` in each
  repository, committed, and the notices check green on that commit. The
  tag goes on that commit, or on a later one that leaves the lockfile as it
  is.

### 5.4 Tags

- [ ] **Tag** (owner), after the commit passes sections 3 and 5.3 and
  before any package is built, so that every package is built from a tag:
  - this repository: an annotated tag `vX.Y.Z` on the release commit;
  - Tawara-mobile: an annotated tag `vX.Y.Z+N` (N the build number) on the
    commit each upload is built from.
  Tags are never moved or deleted, as the fork's are not (D32 item 9 (a));
  a mistake is fixed by a new version or build number. The tag message
  names the library's `rev`, the fork's `rev`, and for Tawara-mobile the
  wallet commit it pins.
- [ ] **The network check of section 1.2 repeated at the tags** (delegated),
  before the store privacy answers are given.

## 6. Build and sign, per target

Each package is built from the tag on the owner's machines (section 1.6),
where the keys are, with the tools and versions D36 pins; nothing is signed
in CI (D13, D36). Every Rust build uses `--locked` and the release profile
D11 restates. The scripts named are D36's; the commands below are what they
run.

### 6.1 Windows (`tawara-desktop`, NSIS installer)

- [ ] Build: `cargo build --release --locked -p tawara-desktop --target
  x86_64-pc-windows-msvc`, with the static C runtime given on the command
  line (`--config 'target.x86_64-pc-windows-msvc.rustflags=["-C",
  "target-feature=+crt-static"]'`), so the installer needs no Visual C++
  runtime (D36). x86_64 only, for the first release (D36).
- [ ] The executable carries the icon and version information:
  `FileVersion` `X.Y.Z.0` (the fourth field is 0, since the desktop has no
  build number) and `ProductVersion` `X.Y.Z`.
- [ ] Sign the executable before it goes into the installer: `signtool sign
  /fd sha256 /tr <the CA's timestamp address> /td sha256` with the token
  present (owner).
- [ ] Build the installer with NSIS 3.13, its download checked against the
  SHA-256 recorded in the script (D36), with `SetCompressor zlib`: per user,
  into `%LOCALAPPDATA%\Programs\Tawara`, a Start menu entry named "Tawara",
  an uninstaller, and an entry in Installed apps. The uninstaller removes
  the program's own files only and **never the wallet's store or
  preferences**: the store holds the key index the library only moves
  forward, and PLAN 4.9 lets nothing in Tawara delete it. The uninstaller is
  signed through NSIS's `!uninstfinalize`, then the installer itself
  (owner).
- [ ] `signtool verify /pa /v` on the executable, the uninstaller and the
  installer.

### 6.2 macOS (`Tawara.app` in a `.dmg`)

- [ ] With `DEVELOPER_DIR=/Applications/Xcode_26.6.app/Contents/Developer`,
  build both halves with `cargo build --release --locked -p tawara-desktop`
  for `aarch64-apple-darwin` and `x86_64-apple-darwin`, with
  `MACOSX_DEPLOYMENT_TARGET=11.0`, and join them with `lipo -create`.
- [ ] Assemble `Tawara.app`: `Contents/MacOS/tawara-desktop`, an
  `Info.plist` (bundle identifier `com.patricksmithlaravel.tawara`, as D36
  proposes for D8; `CFBundleShortVersionString` `X.Y.Z`;
  `LSMinimumSystemVersion` 11.0), an `AppIcon.icns` made with `iconutil`,
  and the notices (section 4).
- [ ] Sign the app: `codesign --force --options runtime --timestamp --sign
  "Developer ID Application: <name> (<team>)"`, with no entitlements
  (owner).
- [ ] Make the image with `hdiutil create` from a folder holding the app and
  a link to `/Applications`; sign the `.dmg` the same way (owner).
- [ ] Notarise: `xcrun notarytool submit Tawara-X.Y.Z.dmg --keychain-profile
  <profile> --wait`; on a refusal, `xcrun notarytool log <id>` says why
  (owner).
- [ ] Staple the ticket to the image, the outermost container: `xcrun
  stapler staple` then `xcrun stapler validate` (owner).

### 6.3 Linux (AppImage and `.deb`)

- [ ] Build in an `ubuntu:22.04` container pinned by its digest, which the
  script records, on the owner's machine: `docker run --platform linux/amd64
  ubuntu:22.04@sha256:<digest>`. The image has no C compiler, no `curl` and
  no `dpkg-dev`, so first run `apt-get update`, then `apt-get install -y
  --no-install-recommends ca-certificates curl gcc libc6-dev dpkg-dev`
  (D36). `curl` and `ca-certificates` fetch rustup-init. `gcc` and
  `libc6-dev` give `cc`, which `cargo` links the executable with and
  `ring`'s build compiles its C with. `dpkg-dev` holds `dpkg-shlibdeps`, for
  the `.deb` below. No `-dev` library package is needed, because the app
  opens its display and Vulkan libraries at run time (D36). Then rustup-init
  at a pinned version checked against its recorded SHA-256, then the
  toolchain `rust-toolchain.toml` names, then `cargo build --release
  --locked -p tawara-desktop`. If `rustc` is killed with signal 9, the
  container ran out of memory: that happened in an 8 GiB Docker VM with 14
  processors, where `CARGO_BUILD_JOBS=4` then built it; the job count
  changes only how many crates compile at once. On an Apple silicon Mac the
  container runs under emulation, which is slower and changes nothing about
  the checks. x86_64 only, for the first release (D36).
- [ ] **The glibc floor is 2.35**, Ubuntu 22.04's and icm's default (D36).
  The script reads the highest `GLIBC_` symbol version the executable needs
  and **fails if it is above 2.35**; the release notes state the floor.
- [ ] AppImage: an AppDir with `AppRun`, the desktop entry
  `com.patricksmithlaravel.tawara.desktop` (`Name=Tawara`,
  `Exec=tawara-desktop`), the icon at 256 pixels, and the notices; then
  `appimagetool --appimage-extract-and-run --runtime-file <the pinned
  runtime>` with appimagetool 1.9.1, each checked against its SHA-256
  before it runs. The AppImage bundles no libraries, so it needs from the
  system those the `.deb` below names by hand; the release notes list them
  (section 9).
- [ ] `.deb`: a tree with `usr/bin/tawara-desktop`, the same desktop entry
  under `usr/share/applications/`, hicolor icons from 16 to 512 pixels, the
  notices under `usr/share/doc/tawara-desktop/`, and `DEBIAN/control`
  (package `tawara-desktop`, the owner's maintainer line). `Depends` has two
  parts. The first is the value that `dpkg-shlibdeps -O` prints for the
  variable `shlibs:Depends`, without the variable's name, which `Depends`
  cannot hold; it names only the libraries the executable is linked with
  (D36, source 26). `dpkg-shlibdeps` reads `debian/control` from its working
  directory whether or not `-O` is given (`scripts/dpkg-shlibdeps.pl` line
  160, dpkg 1.21.1; D36, source 27), so run it in a workspace beside the
  tree, never in the tree, where a `debian/` directory would be packaged. In
  an empty workspace directory, where `<tree>` is the tree's path:

  ```sh
  mkdir -p debian
  printf 'Source: tawara-desktop\n\nPackage: tawara-desktop\nArchitecture: amd64\n' > debian/control
  dpkg-shlibdeps -O <tree>/usr/bin/tawara-desktop
  ```

  The workspace's `debian/control` is a `Source` stanza, a blank line, then
  one `Package` stanza, and nothing else; `dpkg-shlibdeps` rejects a
  `Package` stanza without `Architecture`. Give the path bare. With a space
  after `-e`, as in `-e <path>`, `dpkg-shlibdeps` also analyses an
  executable with an empty name and prints a spurious warning (D36, source
  27); the attached form `-e<path>` does not, but the bare path needs no
  `-e` at all. The second part of `Depends` names by hand the libraries the
  app opens at run time (D36): for an X11 session, `libx11-6`,
  `libx11-xcb1`, `libxcursor1`, `libxi6` and `libxkbcommon-x11-0`; for a
  Wayland session, `libwayland-client0`; and for both, `libxkbcommon0`,
  which the keyboard needs. All of them are `Depends`, since one package
  serves either session. `Recommends` is `libvulkan1`: without it the app
  draws with its software renderer. The X11 names are those that
  `.github/workflows/ci.yml` installs before its start-up check (lines 141
  and 142), less `libxrandr2`, which winit does not open; `libx11-6` and
  `libxkbcommon0` are added because the app opens `libX11` and
  `libxkbcommon` itself. Then `dpkg-deb --root-owner-group --build`, in the
  same container. The package has no maintainer script that touches anyone's
  home folder.
- [ ] Optional detached GPG signatures (owner, section 1.2).

### 6.4 Android (App Bundle)

In Tawara-mobile, after sections 1.5 and 1.6.

`platform/android/release.sh` (Tawara-mobile PR #19) makes the first four
bullets below and the manifest checks of the fifth. Two things differ from the
bullets. Its universal APK, made with `TAWARA_ANDROID_APK=1`, is signed with the
upload key, where the fifth bullet names a throwaway key. The emulator checks on
that APK are not in the script. Both are for the owner to decide.

- [ ] Build `libtawara_mobile.so` for `aarch64-linux-android` and
  `x86_64-linux-android` with NDK 29.0.14206865, as `tawara.sh build`
  does, with its checks: `ANativeActivity_onCreate` exported and 16 KB
  `LOAD` alignment.
- [ ] Link the manifest and resources with build-tools 36.0.0's `aapt2 link
  --proto-format` against `platforms/android-36/android.jar`, with
  `--min-sdk-version 26 --target-sdk-version 36`, `--version-code` the build
  number and `--version-name` the version, and **without** `--debug-mode`.
- [ ] Lay out the base module (manifest, resources, `lib/<abi>/`, the
  notices under `assets/`) and run `bundletool build-bundle` with
  bundletool 1.18.3, its configuration asking for 16 KB alignment
  (`PAGE_ALIGNMENT_16K`); then `bundletool validate`.
- [ ] Sign with the upload key: `jarsigner -keystore <upload keystore>
  Tawara-X.Y.Z+N.aab <alias>` (owner). `apksigner` cannot sign a bundle
  (D36, source 10).
- [ ] Check what was built: `bundletool dump manifest` shows `targetSdk` 36,
  the build number, no `debuggable`, `allowBackup` false, the backup rules,
  and `configChanges` `0xd800ffff`; then `bundletool build-apks --mode
  universal` with a throwaway key, and the emulator checks on that APK, on
  the port and AVD the run creates (section 1.6), which it deletes
  afterwards.

### 6.5 iOS (`.ipa`)

In Tawara-mobile, after the owner items of section 1.2 and Xcode 26.6
(section 1.6).

`platform/ios/release.sh` (Tawara-mobile PR #19) makes the bullets below up to
the archive checks, and stops before signing unless the signing variables are
set. Validation and upload stay with the owner. Its privacy manifest declares
file timestamps and the system boot time, which the release binary's imports
read on 2026-10-18 show, and no disk-space reason, since the binary imports no
disk-space call. The reason codes are for the owner to confirm against Apple's
list and the 26.x build.

- [ ] Every command below runs with
  `DEVELOPER_DIR=/Applications/Xcode_26.6.app/Contents/Developer`.
- [ ] Build the device binary: `cargo build --locked --release --bin tawara
  --target aarch64-apple-ios` with `IPHONEOS_DEPLOYMENT_TARGET=16.0`, as
  `tawara.sh device` does, with its platform check.
- [ ] **Check the SDK the binary is linked with** before anything is
  signed: `xcrun vtool -show-build` on the binary must report an `sdk` of
  26.x. A 27 there means the wrong Xcode built it, and the app would stop at
  launch (D32 item 8); the script fails.
- [ ] Assemble `Tawara.app` from `Info.plist.in` as `tawara.sh` does, and
  add what the store needs: `CFBundleVersion` the build number;
  `ITSAppUsesNonExemptEncryption` from section 1.2's answer; the icon
  compiled into `Assets.car` with `xcrun actool`, and `CFBundleIcons`;
  `PrivacyInfo.xcprivacy`, with a reason for each required-reason API the
  binary uses (likely file timestamps, disk space, and the system boot time
  that the monotonic clock behind Rust's `Instant` reads; confirmed against
  the binary's imports); the `DT` keys of the Xcode in use; the App
  Store profile as `embedded.mobileprovision`; and the notices. Check with
  `plutil -lint` and the scene-manifest check `tawara.sh` makes.
- [ ] Sign the bundle with the entitlements file (application identifier,
  team, `get-task-allow` false,
  `com.apple.developer.default-data-protection` `NSFileProtectionComplete`):
  `codesign --force --timestamp --sign "Apple Distribution: <name>
  (<team>)" --entitlements <file> Tawara.app` (owner), naming the bundle as
  `tawara.sh` does. Read them back with
  `codesign -d --entitlements :- Tawara.app` and compare.
- [ ] Put the signed app in `Payload/Tawara.app`, then zip the `Payload`
  directory, not the app, as `Tawara-X.Y.Z+N.ipa` (D36). From the directory
  that contains `Payload`, run `ditto -c -k --noextattr --norsrc --noqtn
  --keepParent Payload Tawara-X.Y.Z+N.ipa`. Zipping `Payload/Tawara.app`
  instead puts `Tawara.app/` at the root of the archive, with no
  `Payload/` directory. Without `--norsrc` and `--noextattr`, ditto writes
  an AppleDouble `._` entry for each file or folder that carries an
  extended attribute, and App Store Connect has refused an upload over a
  `._` path (D36, source 28).
- [ ] Check the archive. Both must succeed, also under `set -o pipefail`,
  so neither uses `grep -q`, which can close the pipe before `unzip` is
  done:
  `unzip -l Tawara-X.Y.Z+N.ipa | grep " Payload/Tawara.app/" >/dev/null`
  and
  `! unzip -l Tawara-X.Y.Z+N.ipa | grep -E "(^| |/)\._" >/dev/null`.
- [ ] Validate `Tawara-X.Y.Z+N.ipa`: `xcrun altool --validate-app -f
  Tawara-X.Y.Z+N.ipa -t ios` with the API key (owner).

## 7. Checks on the signed packages

Before anything is submitted or published, on clean machines that have
never had Tawara on them (owner, with delegated help):

- [ ] **Windows:** install without an administrator prompt; the Start menu
  entry starts the app; uninstall leaves the store and preferences where
  they were; SmartScreen's prompt, if any, names the certificate's
  subject. A new certificate has no reputation yet, so a prompt on the
  first releases is expected.
- [ ] **macOS:** `spctl -a -vvv -t open --context context:primary-signature`
  on the image, which must print `accepted` and
  `source=Notarized Developer ID` (D36, source 23); `-t install` assesses
  installer packages, not images. `codesign --verify --deep --strict` on the
  app; a copy downloaded through a browser opens with no Gatekeeper warning,
  on Apple silicon and on Intel.
- [ ] **Linux:** the `.deb` installs, starts and removes on Ubuntu 22.04
  and 24.04; the AppImage starts on both; `dpkg-deb --info` and
  `--contents` show what section 6.3 says. In a fresh `ubuntu:22.04`
  container with only `xvfb`, `xauth` and `mesa-vulkan-drivers` added before
  the package (the last is the software Vulkan driver that the wgpu launch
  needs), `ci/liveness.sh` passes on the installed binary. Libraries that
  those packages pull in cannot show a gap, so the check covers the list only
  for libraries the container lacks. The AppImage gets the same check: in
  another fresh container with the same three packages and the `.deb`'s
  hand list installed by name, extract it with `--appimage-extract`, and
  `ci/liveness.sh` passes on the executable inside it.
- [ ] **Every desktop package:** the version shown is `X.Y.Z`; a wallet
  made by the previous release, if there is one, opens; the notices are
  where section 4 says.
- [ ] **Checksums:** a `SHA256SUMS` file listing every desktop package,
  signed if section 1.2 provided a key.

## 8. Device checks on the release builds

The gate between the signed packages and publishing. No check has yet run
on a physical phone, and these must run on the builds to be released, so
they come here, after the packages exist and before anything is submitted
for review or published.

- [ ] **Upload the phone builds for testing** (owner): the `.ipa` to App
  Store Connect with `xcrun altool --upload-app` and the API key, or with
  Transporter, then to TestFlight; the first `.aab` by hand in Play Console
  to the internal testing track, which enrols Play App Signing (section
  1.2). These deliver exactly the builds of section 6.
- [ ] **The Play closed test's timing** (owner), if the account needs one
  (section 1.2). The test can start only after the app's setup in Play
  Console. Production access is applied for when 12 testers have been
  opted in for the preceding 14 days, and Google's review of that
  application usually takes seven days or less (D36, source 7). With the
  closed test in section 9, the first Play release comes at least 14 days,
  plus that review, after this section's gate. To shorten the wait, the
  owner may start the closed test from the build uploaded above while the
  device checks run, so that its testers hold a build whose checks have
  not yet passed. Play Console is first asked whether a closed-track
  release is reviewed. If it is, that release is a submission for review,
  which this section's gate forbids until the checks pass, and starting
  early then needs the owner's exception in writing in docs/DECISIONS.md.
- [ ] **Run the device checks** (owner) on an arm64 Android phone and an
  iPhone (D32 item 2), from those testing builds. The list is the threat
  model's section 5.2 (D34), V1 to V18; if it changes before the release,
  its list is the one run:
  - V1 text entry and keyboard learning; V2 `FLAG_SECURE` from the first
    frame; V3 the Android share chooser; V4 the Android exit after `Destroy`
    on the phone (its emulator run at the release's pin is G6, before the
    tag; section 2); V5 iOS data protection, on the signed build; V6 the iOS
    app switcher's snapshot; V7 TalkBack and VoiceOver; V8 the library on a
    phone's file system; V9 the first gesture after launch, and touch; V10
    the background lock;
  - V11 backup and transfer leave the store out; V12 the app starts on
    arm64; V13 a kill mid-send, and split screen; V14 the copy mark on
    Android; V15 hardware-keyboard copy and cut; V16 the first screen on
    an iPhone SE; V17 drawing, speed and memory on a phone; V18 screen
    recording and mirroring while S4, the recovery phrase, is shown, on an
    iPhone (G27).
- [ ] **Record each result** in docs/DECISIONS.md, with the build number it
  ran on (delegated, from the owner's notes).
- [ ] **What blocks.** A failure of V1, V2, V5 or V6, or of the footer link
  or Paste in V9, blocks the phone release it concerns until it is fixed
  (the threat model's section 6, item 1). Any other failure is fixed, or
  accepted by the owner in writing in docs/DECISIONS.md, before that
  release goes on.
- [ ] **The threat model's section 6, for the items that need a build or a
  device** (delegated, then owner): each is closed, or accepted in writing in
  docs/DECISIONS.md, before anything is submitted for review or published
  (section 9).
- [ ] **When a check fails and is fixed** (delegated, then owner): the fix
  is a new commit, through a pull request, and the release goes back to
  section 3 for it. A fix in this repository takes a new version, since
  `vX.Y.Z` is never moved (section 5.4), and new desktop packages too; a
  fix in Tawara-mobile alone takes a new build number and the tag
  `vX.Y.Z+N` for it. Sections 5 to 8 then run again for every package the
  new commit changes, and every device check runs again on the new builds,
  not only the one that failed.

## 9. Store submissions and publishing

- [ ] **Source available for every package** (delegated, owner's review):
  FORK.md reads the licence's source-availability obligation as owed
  whichever answer counsel gives (section 1.3). Before publishing, check
  that each source is public, and link all four from the GitHub release,
  the release notes and each store listing:
  - this repository at `vX.Y.Z`;
  - Tawara-mobile at `vX.Y.Z+N`;
  - the library, Rep-1 (`patricksmithlaravel/mcm-rust-cli-windows`), at
    the `rev` the tag pins;
  - the iced fork (`patricksmithlaravel/iced_mobile`) at the tag it pins.
- [ ] **App Store** (owner): the build already in App Store Connect from
  section 8; answer export compliance; complete the privacy answers; write
  review notes that say what the app is (a wallet for Mochimo that holds
  the person's keys on the device and talks only to the node they choose)
  and how to reach a screen without funds; submit for review, with manual
  release, so that the release happens when the other platforms are ready.
- [ ] **Google Play** (owner): from the internal testing track of section 8,
  the app's setup in Play Console, among it the Data safety form, content
  rating, target audience and financial features declaration, which the
  closed test needs first; the closed test if the account needs one (section
  1.2), unless the owner started it in section 8; then production. Play
  offers no staged rollout for an app's first publication, so the first
  release reaches all Google Play users in the countries and regions chosen
  for production at once (D36, sources 24 and 25). The device checks of
  section 8 are the gate before it. **(Recommended)** for each later update,
  a staged rollout, starting at a small share, so that a fault reaches few
  people (D36, source 24).
- [ ] **Desktop** (owner): a GitHub release on the tag `vX.Y.Z`, made from
  the owner's machine, with the signed installer, the `.dmg`, the
  AppImage, the `.deb`, `SHA256SUMS` and its signature, and release notes
  that state the version, the glibc floor (2.35), the libraries the
  AppImage needs from the system (section 6.3), the minimum macOS and
  Windows versions, the source links above, and that the program is not an
  official product of the cryptocurrency. CI uploads nothing (D13).
- [ ] **Release notes** (delegated draft, owner approval): what changed, the
  library's and the fork's `rev`, and anything a person must do; the same
  substance in each store's "what's new".

## 10. After release

- [ ] **What people get is what was built** (delegated): download each
  desktop package from the GitHub release and compare it with
  `SHA256SUMS`; install each phone app from its production listing and
  check the version and build number.
- [ ] **Records** (delegated): an entry in docs/DECISIONS.md for the
  release (section 11).
- [ ] **Reports** (owner): Play Console's Android vitals and App Store
  Connect's crash reports, which come only from people who share
  diagnostics with Apple or Google; the app sends nothing of its own
  (section 1.2's check).
- [ ] **Dates that recur** (owner, with a reminder for each):
  - Play raises the required target API from time to time; the current
    rule took effect on 2026-08-31 (D36, source 6). Read Play's target API
    page before each release and after each new Android version;
  - Apple raises the SDK required for uploads; the current rule, Xcode 26
    and the iOS 26 SDK, took effect on 2026-04-28 (D36, source 3). Read
    Apple's upcoming requirements page before each release. A rule that
    requires the iOS 27 SDK needs the scene work of D32 item 8 first, and
    the UIScene re-check of section 2 runs again;
  - the Apple Distribution certificate, the Developer ID certificate, the
    provisioning profile, the API key and the Windows certificate each
    expire, and a renewal well before is cheaper than a lapsed signature;
  - the upload keystore's backup is checked to open.
- [ ] **Next version** (delegated): the next release's work starts from
  this checklist again, and any step it found wrong is corrected here in
  the same pull request as the fix.

## 11. The release record

Each release adds to docs/DECISIONS.md, under the phase it ships in:

- the version, the build number, and the tags in both repositories;
- the commits: this repository's, Tawara-mobile's, the library's `rev` and
  the iced fork's `rev`;
- the version of every tool D36 pins, as used, and the Xcode build;
- the SHA-256 of every published package;
- the device check results of section 8, or where they are recorded;
- each step of this checklist that did not apply, and why.
