# Packaging

The scripts that make Tawara's desktop packages, and what each one needs.
They serve the checklist in `docs/RELEASE.md`. Their choices are D36 in
`docs/DECISIONS.md`, which is **proposed, not decided**: each script builds to
D36's recommendations, and every choice the owner has to make is still open
until the owner answers it there. Awaiting the owner: those decisions, and the
user-visible wording listed under "Not decided".

## Layout

| Path | What it is | State |
|---|---|---|
| `packaging/README.md` | this file | this change |
| `packaging/macos.sh` | the macOS packages: a universal `Tawara.app`, `Tawara-X.Y.Z.dmg`, `SHA256SUMS` | this change |
| `packaging/macos/Info.plist` | the template for the bundle's `Info.plist`; the script sets its version, build number and minimum system version | this change |
| `packaging/macos/entitlements.plist` | the hardened runtime's entitlements: an empty dictionary, so no exception is granted | this change |
| `packaging/windows.sh` | the Windows installer: `tawara-desktop.exe` for x86_64 with the static C runtime and its version resource, and `Tawara-X.Y.Z-x86_64-setup.exe`, a per-user NSIS installer, with `SHA256SUMS`. It is signed when a certificate is configured (D36 decision 2) | this change |
| `packaging/windows/tawara.nsi` | the NSIS script the installer is made from | this change |
| `packaging/linux.sh` | the AppImage and the `.deb` for x86_64, built in a pinned `ubuntu:22.04` container, with `SHA256SUMS` and optional GPG signatures | this change |
| `packaging/linux/in-container.sh` | what runs inside that container: the build, the glibc floor check, the `.deb` and the AppImage | this change |
| `packaging/linux/AppRun` | the AppImage's entry point: it runs the executable beside it | this change |
| `packaging/linux/com.patricksmithlaravel.tawara.desktop` | the desktop entry, the same in both Linux packages | this change |
| `packaging/check.sh` | the static checks on the packages the three scripts make, as `docs/RELEASE.md` sections 6.1 to 6.3 name them. It reads what it is given and changes nothing | this change |
| `.github/workflows/packaging.yml` | the packaging job: each host builds its packages with `--dev` and runs `check.sh`. No third-party action, no secret, nothing uploaded (D13, D36) | this change |

The two plist files carry no XML comments: `codesign` refuses an entitlements file with one (`AMFIUnserializeXML: syntax error`), so the reasons are written here instead.

D36 decision 1 proposes one script per target instead of one packaging tool.
Each script stands alone. The tools it uses are the platform's own, or pinned
by version and hash where D36 says so. The scripts run under
macOS's `/bin/bash` (version 3.2), and pass `bash -n` and
`shellcheck -S warning`. `packaging/linux/in-container.sh` runs in the Linux
container under that image's bash, and passes the same two checks.
`packaging/windows.sh` is a plain shell script too (D36). It runs under Git Bash
on Windows, the shell that CI's Windows jobs already use. It passes the same two
checks, and its logic was also run under `/bin/bash` 3.2 on macOS, with test
doubles for the Windows tools.

## `packaging/macos.sh`

Run it from the repository root, on the Mac that holds the signing keys:

```sh
bash packaging/macos.sh --dev                      # a local check build, not for distribution
bash packaging/macos.sh --build-number 7 --out DIR # a release build (needs the variables below)
```

It makes these, in order, and stops with an error at the first step that fails:

1. A release build of `tawara-desktop` for `aarch64-apple-darwin` and
   `x86_64-apple-darwin`: `cargo build --release --locked -p tawara-desktop
   --target …` with `MACOSX_DEPLOYMENT_TARGET=11.0` (D36 decision 5;
   `docs/RELEASE.md` section 6.2).
2. A check that each build reports `minos 11.0` in `vtool -show-build`.
3. One universal binary, by `lipo -create`, which the script checks has both
   architectures.
4. `Tawara.app`, with:
   - `Contents/MacOS/tawara-desktop`, the universal binary;
   - `Contents/Info.plist` from the template: `CFBundleIdentifier`
     `com.patricksmithlaravel.tawara` (D36 decision 7's recommendation for D8),
     `CFBundleShortVersionString` from the `[workspace.package]` version in
     `Cargo.toml`, `CFBundleVersion` from the build number,
     `LSMinimumSystemVersion` 11.0, and `NSHighResolutionCapable` true;
   - `Contents/PkgInfo`;
   - `Contents/Resources/LICENSE.md` and `THIRD-PARTY-NOTICES.txt`, copied from
     the repository root (`docs/RELEASE.md` section 4; D35);
   - `Contents/Resources/AppIcon.icns`, when `TAWARA_ICON` is set.
5. Signing: `codesign --options runtime --timestamp` with the empty entitlements
   file, then `codesign --verify --deep --strict`. A release signs with the
   Developer ID identity in `TAWARA_MAC_SIGN_ID`, which it requires. A `--dev`
   build signs when the variable is set, and an ad-hoc `-` takes no timestamp.
   When `TAWARA_TEAM_ID` is also set, the signature must name that team.
6. `Tawara-X.Y.Z.dmg` (`Tawara-X.Y.Z-dev.dmg` with `--dev`), made by
   `hdiutil create` from a folder holding the app and a link to
   `/Applications`. It is signed the same way when an identity is set.
7. Notarisation, in a release: `notarytool submit --wait` with the keychain
   profile in `TAWARA_NOTARY_PROFILE`, which a release requires. It must report
   `status: Accepted`; then `stapler staple` and `stapler validate`; then
   `spctl` must accept the image with `source=Notarized Developer ID`. A `--dev`
   build does not notarise.
8. `SHA256SUMS` beside the image, checked with `shasum -a 256 -c`.

The packages go to `target/package/macos` unless `--out` says otherwise. The
script ends with `signed: yes|ad-hoc|no; notarised: yes|no`. A release refuses
to start without an identity and a notary profile, so a release ends with
`signed: yes; notarised: yes`. A `--dev` package ends with `ad-hoc` or `no`, and
`notarised: no`, and must not be published (`docs/RELEASE.md` section 6.2).

The script first checks that both Rust targets are installed
(`rustup target list --installed`). When one is not, it stops and prints the
`rustup target add` command to run.

## `packaging/linux.sh`

Run it from the repository root, on the machine that holds the keys:

```sh
bash packaging/linux.sh --dev              # a local check build, not for distribution
bash packaging/linux.sh --out DIR          # a release build (needs the variables below)
```

It needs a Docker daemon that can run `linux/amd64` images. The image is
`ubuntu:22.04` pinned by digest, the index digest
`sha256:0a9887931b0e38af2e3487ed39e29436b0535c4f3f52e57745baaf79d1898956`,
which Docker resolves to the `linux/amd64` manifest (`sha256:98510ac07bb4545e389be502a8918b385c018bfe4ceeb5ac7e0375c782268b04`).
On an Apple silicon Mac the container runs under emulation. That is slower, and
changes nothing about the checks (`docs/RELEASE.md` section 6.3).

It makes these, in order, and stops with an error at the first step that fails:

1. It checks its inputs before Docker is used: the icon, the maintainer, the
   two notices at the repository root, and the GPG key when one is named.
2. It starts the container with `--platform linux/amd64 --cpus=2 --memory=4g`
   and `CARGO_BUILD_JOBS=2`. The repository is mounted read-only at `/src`. A
   volume named `tawara-linux-N`, with `N` made from the checkout's path, is
   mounted at `/cache`, so that the next run compiles only what changed. The
   packages go to `/out`.
3. Inside the container, `packaging/linux/in-container.sh` installs the packages
   `docs/RELEASE.md` section 6.3 names, and two more that the AppImage step needs
   (`file` and `squashfs-tools`, see step 6). It checks rustup-init 1.29.1
   against its SHA-256, installs the toolchain `rust-toolchain.toml` names, and
   runs `cargo build --release --locked -p tawara-desktop --target
   x86_64-unknown-linux-gnu`.
4. The glibc floor check: `objdump -T` names the `GLIBC_` symbol versions the
   executable needs. The highest must be 2.35 or lower, or the build fails.
5. The `.deb`, from a tree holding `usr/bin/tawara-desktop`, the desktop entry
   under `usr/share/applications/`, the hicolor icons at 16, 32, 48, 64, 128,
   256 and 512 pixels, `LICENSE.md` and `THIRD-PARTY-NOTICES.txt` under
   `usr/share/doc/tawara-desktop/`, and `DEBIAN/control`. `Depends` is the value
   `dpkg-shlibdeps -O` prints for the executable, followed by the libraries the
   app opens at run time, which `docs/RELEASE.md` section 6.3 names by hand.
   `Recommends` is `libvulkan1`. The package is made with
   `dpkg-deb --root-owner-group --build`, and its `--info` and `--contents` are
   printed.
6. The AppImage, only when an icon is given: an AppDir with `AppRun`, the
   desktop entry at its top and in `usr/share/applications`, the 256-pixel icon,
   and the notices beside the executable. Then appimagetool 1.9.1 and the type 2
   runtime 20251108 are each checked against their SHA-256. appimagetool is
   itself an AppImage, and the emulated container's binfmt rule refuses to
   execute an AppImage's ELF header (`Exec format error`, seen with Docker
   Desktop on Apple silicon). So its squashfs part, which begins where the
   runtime ends, is unpacked with `unsquashfs`, and that tree's `AppRun` runs
   the same tool with `--runtime-file`, which needs no FUSE. The new AppImage
   is checked by listing its squashfs part for `AppRun` and
   `usr/bin/tawara-desktop`. On a native x86_64 host the same steps run
   unchanged.
7. On the host, `SHA256SUMS` lists the packages and is checked with
   `shasum -a 256 -c`. With `TAWARA_GPG_KEY`, `SHA256SUMS`, the `.deb` and the
   AppImage each get a detached armored signature (`.asc`), which `gpg --verify`
   checks.

The packages go to `target/package/linux` unless `--out` says otherwise:

| Release | `--dev` |
|---|---|
| `tawara-desktop_X.Y.Z_amd64.deb` | `tawara-desktop_X.Y.Z~dev_amd64.deb` |
| `Tawara-X.Y.Z-x86_64.AppImage` (with an icon) | `Tawara-X.Y.Z-dev-x86_64.AppImage` (with an icon) |
| `SHA256SUMS` | `SHA256SUMS` |

The version is the one `[workspace.package]` sets in `Cargo.toml`. The pins
are at the top of `packaging/linux.sh`: the image digest, rustup-init 1.29.1,
appimagetool 1.9.1 and runtime 20251108, each with the SHA-256 that
`docs/DECISIONS.md` D36 records. The first two were checked against the
publisher's own digests on 2026-10-18 (rustup's published `.sha256`, and
GitHub's digest for each release asset). A change to any pin is a pull request
that names the new version and hash.

The build's volume is kept between runs. Remove it with
`docker volume rm tawara-linux-N` when its cache is no longer wanted. Its
name's `N` is printed by `cksum` on the checkout's path.

The apt packages are not pinned. `apt-get` installs the archive's current
versions of the packages `docs/RELEASE.md` section 6.3 names, and of `file` and
`squashfs-tools`, which that section does not name: appimagetool needs `file`,
and unpacking its squashfs part needs `squashfs-tools`. The image digest pins
the base system only.

## `packaging/windows.sh`

Run it from the repository root, in Git Bash on Windows, on the machine that
holds the signing token:

```sh
bash packaging/windows.sh --record-nsis   # first use only: prints the NSIS archive's SHA-256
bash packaging/windows.sh --dev           # a local check build, not for distribution
bash packaging/windows.sh --out DIR       # a release build (needs the variables below)
```

It makes these, in order, and stops with an error at the first step that fails:

1. The NSIS 3.13 archive, `nsis-3.13.zip`, from SourceForge over HTTPS, when
   `target/package/tools` has no copy with the recorded hash. Until `NSIS_SHA256`,
   in the script, holds that hash (D36 decision 8; see "Not decided"), a release
   refuses to start, before its build. A `--dev` build takes this step after step
   4 and before step 5, and stops here when the hash is not recorded, so that the
   executable is built and checked first. The archive is unpacked with
   `Expand-Archive`, and `makensis -VERSION` must report 3.13.
2. The Rust target `x86_64-pc-windows-msvc`, and the Windows SDK's `rc.exe` and
   `signtool.exe`: from `TAWARA_WINSDK_BIN`, or else from the newest
   `Windows Kits\10\bin\10.*\x64` under `Program Files (x86)` that holds both.
3. The version resource: `rc.exe` compiles the icon, when `TAWARA_WIN_ICO` is
   set, and the version information, with `FileVersion` `X.Y.Z.0` and
   `ProductVersion` `X.Y.Z` (`docs/RELEASE.md` section 6.1). A `--dev` build
   without `rc.exe` has no resource.
4. The build: `cargo build --release --locked -p tawara-desktop --target
   x86_64-pc-windows-msvc`, with `target.x86_64-pc-windows-msvc.rustflags=["-C",
   "target-feature=+crt-static"]` on the command line, as the checklist gives
   it, and the resource's link flag in the same list. A release refuses when
   `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS` or
   `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS` is set, because cargo would
   then ignore that list.
5. When `TAWARA_WIN_CERT_SHA1` is set, `signtool sign /sha1 THUMBPRINT /fd sha256
   /tr ADDRESS /td sha256` on `tawara-desktop.exe`, then `signtool verify /pa /v`.
6. The installer, by `makensis -V2 -WX` on `packaging/windows/tawara.nsi`. It
   installs per user into `%LOCALAPPDATA%\Programs\Tawara`, with a Start menu
   entry named "Tawara", an uninstaller and an Installed apps entry, and it
   places `LICENSE.md` and `THIRD-PARTY-NOTICES.txt` beside the executable. When
   signing is on, NSIS signs the uninstaller as it makes it (`!uninstfinalize`).
   makensis does not fail when that command fails, so the command also writes a
   stamp file. When the stamp is missing, the script removes the installer and
   stops.
7. When signing is on, `signtool sign` and `signtool verify /pa /v` on the
   installer.
8. `SHA256SUMS` beside the installer, checked with `sha256sum -c`.

The package goes to `target/package/windows` unless `--out` says otherwise, and
the script ends with `signed: yes` or `signed: no`. A release refuses to start
without `TAWARA_WIN_CERT_SHA1`, so a release ends with `signed: yes`. A `--dev`
package ends with `signed: no` and must not be published (`docs/RELEASE.md`
section 6.1). The uninstaller's own
signature is checked after an install, as the checklist asks:
`signtool verify /pa /v "%LOCALAPPDATA%\Programs\Tawara\uninstall.exe"`. The
uninstaller removes the program's files, its Start menu entry and its Installed
apps entry, and never the store under `%LOCALAPPDATA%\Tawara` (D21). It does not
stop a running Tawara, so close the app first.

## Environment

No identity, team, key, certificate or password is read from a file, and none
is written into one. Each signing or notarisation step reads its name from the
environment and is skipped, with a message, when that name is unset.

| Variable | What it is for | Release | `--dev` |
|---|---|---|---|
| `DEVELOPER_DIR` | the Xcode to build with. Unset, and `/Applications/Xcode_26.6.app` exists, the script uses that one (`docs/RELEASE.md` section 1.6) | required; its macOS SDK must be 26.x | optional; a warning if the SDK is not 26.x |
| `TAWARA_ICON` | path to the app icon as an `.icns` file. The owner supplies the artwork (`docs/RELEASE.md` section 1.2); `iconutil -c icns` makes the file from an `.iconset` folder of the standard sizes, made from a 1024-pixel PNG | required | optional; without it the build has no icon |
| `TAWARA_BUILD_NUMBER`, or `--build-number` | the bundle's `CFBundleVersion` | required | defaults to `1` |
| `TAWARA_MAC_SIGN_ID` | the Developer ID Application identity, by the name `codesign` reads, such as `Developer ID Application: Name (TEAMID)` | required; `-` (ad-hoc) is refused | signs when set; unset: the packages are unsigned, with a warning |
| `TAWARA_TEAM_ID` | the team the signature must name, checked after signing | optional | optional |
| `TAWARA_NOTARY_PROFILE` | the `notarytool` keychain profile, made with `xcrun notarytool store-credentials` on this Mac | required | not used |
| `CARGO_TARGET_DIR` | where cargo writes its output, as cargo reads it | optional | optional |

The Linux script reads three variables. Its build never uses `CARGO_TARGET_DIR`:
the build's target directory is the volume described above.

| Variable | What it is for | Release | `--dev` |
|---|---|---|---|
| `TAWARA_ICON_PNG` | the app icon as a square PNG of at least 1024 pixels. It is the owner's artwork (`docs/RELEASE.md` section 1.2), and `sips` makes the sizes on macOS, with ImageMagick's `identify` and `convert` where `sips` is absent (the CI runner). `TAWARA_ICON` is the macOS `.icns` above, so this has its own name | required | optional; without it the `.deb` has no icons and there is no AppImage |
| `TAWARA_DEB_MAINTAINER` | the `.deb`'s `Maintainer:` field, as `Name <address>` (`docs/RELEASE.md` section 1.2). A release refuses the placeholder `Tawara Maintainer <maintainer@example.invalid>` and any `.invalid` address | required | optional; the placeholder when unset |
| `TAWARA_GPG_KEY` | the GPG key that signs `SHA256SUMS` and the packages, named by an identity in the keyring (`gpg --local-user`). Checked before the build starts | unset: unsigned | signs when set |

`TAWARA_MAC_SIGN_ID=-` makes an ad-hoc signature. It is not an identity, it
exercises the signing steps for a local check, and its output is not for
distribution. A release refuses it, and a `--dev` build prints `signed: ad-hoc`.

The Windows script reads these variables:

| Variable | What it is for | Release | `--dev` |
|---|---|---|---|
| `TAWARA_WIN_ICO` | the app icon as an `.ico` file. The owner supplies the artwork (`docs/RELEASE.md` section 1.2). It is the executable's icon and the installer's | required | optional; without it the build has no icon |
| `TAWARA_WIN_CERT_SHA1` | the SHA-1 thumbprint of the code-signing certificate, which signtool finds in the certificate store that holds the token | required | signs when set; unset: unsigned, with a warning |
| `TAWARA_WIN_TIMESTAMP_URL` | the certificate authority's RFC 3161 timestamp address. `docs/RELEASE.md` names none, so the owner supplies it. It must be a plain `http` or `https` address made of letters, digits and `._~:/?=+,@-` | required when signing | required when signing |
| `TAWARA_WIN_PUBLISHER` | the publisher name that Installed apps shows. Unset, it is `patricksmithlaravel`, D8's Windows publisher, which awaits the owner's confirmation (`docs/RELEASE.md` section 1.2). It may not hold a double quote, a dollar sign or a backquote | optional | optional |
| `TAWARA_WINSDK_BIN` | the Windows SDK's `x64` folder holding `rc.exe` and `signtool.exe`, when the newest one found is not the one to use | optional | optional |
| `CARGO_TARGET_DIR` | where cargo writes its output, read as cargo reads it: a relative path is relative to the repository root | optional | optional |

CI runs this script only with `--dev`, unsigned, on `windows-latest`, and with
no secrets: `.github/workflows/packaging.yml` has `permissions: {}`, and D36
decision 4 keeps signing on the owner's machines.

## The packaging job

`.github/workflows/packaging.yml` builds the packages on the three hosts and
checks them, as D36 describes. It runs on a push to `main`, and on a pull request,
when one of these changes: `packaging/`, `ci/liveness.sh`, `Cargo.toml`,
`Cargo.lock`, `rust-toolchain.toml`, the two notices, or the workflow. It also
runs by hand. Each host runs its script with `--dev`, then `packaging/check.sh`
on what that made:

| Host | Builds | Checks | Starts |
|---|---|---|---|
| `macos-latest` | `macos.sh --dev`: both Rust targets, the universal app, the disk image | the two slices and `minos 11.0`, `Info.plist`, the notices, the disk image and the app in it | the app, from the bundle, on wgpu and on tiny-skia |
| `ubuntu-24.04` | `linux.sh --dev` in the pinned `ubuntu:22.04` container, with a test icon | the `.deb`'s control fields and contents, the glibc floor, the AppImage's contents | the `.deb`, installed with apt, then removed; the AppImage's executable, once extracted |
| `windows-latest` | `windows.sh --dev` in Git Bash | the executable: x86-64, no Visual C++ runtime DLL, the version resource; the installer, once made | the executable is not run |

The job has no third-party action (D13), `permissions: {}`, and no secret. Nothing
is signed, notarised or uploaded. The test icon is a plain square that the job
makes, not the owner's artwork, and nothing it makes is published.

The Windows leg builds the executable and makes the installer with `windows.sh`
from the NSIS archive whose SHA-256 is recorded (D36 decision 8). The executable
check runs whether or not that step succeeds; the installer check runs after it.

## Checks

`packaging/check.sh` runs these, in the job and by hand. It reads the packages
and changes nothing; it needs no key, certificate or network.

- `bash -n packaging/macos.sh` and `shellcheck -S warning packaging/macos.sh`.
- `bash packaging/macos.sh --dev`, then `bash packaging/check.sh macos
  target/package/macos`: `lipo -archs` (both architectures), `vtool -arch … -show-build`
  (`minos 11.0` for each), `plutil` on the bundle's `Info.plist` (identifier,
  executable, package type, minimum system version and version), the notices
  against the repository's copies, and the disk image, which must verify and
  hold the same executable, attached read-only.
- A launch: `LIVENESS_SECONDS=10 bash ci/liveness.sh
  target/package/macos/Tawara.app/Contents/MacOS/tawara-desktop`. The liveness
  script starts the app with a fresh `HOME`, once on wgpu and once on
  tiny-skia. Each launch must still be running after the wait, with no panic in
  its log, and the app is stopped at the end of each launch.

### Linux checks

- `bash -n` and `shellcheck -S warning` on `packaging/linux.sh` and
  `packaging/linux/in-container.sh`, and `/bin/bash -n` (macOS's bash 3.2) on
  the first.
- `bash packaging/check.sh linux target/package/linux`: the `.deb`'s `Package`,
  `Architecture`, `Version` (with `~dev` in a development build), `Maintainer`,
  `Depends` (a `libc6` version, and the libraries section 6.3 names by hand) and
  `Recommends`, read with `dpkg-deb -f`; its contents (`dpkg-deb -c`), including
  the executable at mode 0755 and seven icons; the GLIBC symbol versions of the
  unpacked executable (`objdump -T`), at most 2.35; and the AppImage, extracted
  with `--appimage-extract`, which must hold `AppRun`, the desktop entry, and the
  same executable as the `.deb`. `SHA256SUMS` must list both and check.
- The CI job installs the `.deb` on the runner with apt, starts it under Xvfb
  with `ci/liveness.sh` for 20 seconds on each renderer, and removes it. It
  extracts the AppImage and runs the same check on the executable inside.
- The input checks stop before Docker is used: a missing or wrong icon, an
  unset, placeholder or malformed maintainer, a `TAWARA_GPG_KEY` that names no
  secret key, and an unknown argument each exit with an error and a message.
- `bash packaging/linux.sh --dev`, with a 1024-pixel PNG as `TAWARA_ICON_PNG`,
  makes the `.deb` and the AppImage. Its log holds the glibc check and the
  `dpkg-deb --info` and `--contents` output. The host then checks
  `SHA256SUMS` and prints it.
- `bash packaging/linux.sh --dev` without an icon makes the `.deb` alone, with
  no AppImage, and `SHA256SUMS` lists the `.deb` alone.
- On a fresh `ubuntu:22.04` container of the same digest, `--platform
  linux/amd64`, with `xvfb`, `xauth` and `mesa-vulkan-drivers` added, the
  `.deb` installs by its path, `ci/liveness.sh` passes on
  `/usr/bin/tawara-desktop`, and the package is removed (`docs/RELEASE.md`
  section 7). The repository is mounted read-only at `/src` for the script.
- For the AppImage: in another fresh container, its squashfs part is unpacked
  (from the offset the runtime's size gives), `AppRun` is run under `xvfb-run`
  for 20 seconds, and `ci/liveness.sh` passes on the executable inside. The
  AppImage itself cannot be executed in the emulated container, so this check
  runs the same files from their squashfs part (`docs/RELEASE.md` section 7
  says `--appimage-extract`).

### Windows checks

- `bash -n`, `shellcheck -S warning` and `shellcheck` on `packaging/windows.sh`,
  and `/bin/bash -n` (macOS's bash 3.2) on the same file.
- `bash packaging/check.sh windows-exe target/x86_64-pc-windows-msvc/release/tawara-desktop.exe`:
  the file is a Windows executable with an x86-64 machine type, names no Visual
  C++ runtime DLL (so the static runtime applied), and its version resource reads
  `FileVersion` `X.Y.Z.0`, `ProductName` Tawara, `ProductVersion` `X.Y.Z` (or
  `X.Y.Z-dev`) and `FileDescription` Tawara, read by PowerShell.
- `bash packaging/check.sh windows target/package/windows`, once the installer is
  made: one installer, listed in `SHA256SUMS`, which checks.
- `makensis` compiles `packaging/windows/tawara.nsi` with the defines the script
  passes, with `-WX` (warnings are errors), both unsigned and with signing on,
  using a stand-in `signtool`. With signing on, the uninstaller's signing command
  runs with the thumbprint and the timestamp address in it, and the stamp file
  exists only after it succeeds. On macOS, NSIS needs a UTF-8 locale
  (`LC_ALL=en_US.UTF-8`): with the C locale, the Homebrew build aborts with
  `std::bad_alloc`.
- On Windows, the first use of the NSIS archive: `bash packaging/windows.sh
  --record-nsis` on one network, and again on a second. The two hashes must agree
  before `NSIS_SHA256` is recorded.
- Then `bash packaging/windows.sh --dev` on the Windows machine, and the Windows
  entry of `docs/RELEASE.md` section 7 on a clean machine: install without an
  administrator prompt, the Start menu entry starts the app, uninstall leaves the
  store and preferences where they were, and any SmartScreen prompt names the
  certificate's subject.

## Not decided

Nothing below is settled by this change. The owner decides each one.

- **D36 as a whole** (proposed, 2026-10-13). This change builds to its
  recommendations: one script per target (decision 1); signing and publishing
  on the owner's machines only (decision 4); macOS 11.0 as the minimum, as one
  universal binary (decision 5); the macOS identifier (decision 7). The macOS
  script uses the executable name `tawara-desktop` (decision 3, also
  proposed). Decisions 2, 6, 8 and 9 concern Windows, the architectures of the Windows
  and Linux packages, prebuilt tools and accounts.
- **The build number.** `docs/RELEASE.md` section 5.2 says the desktop has no
  build number. macOS needs `CFBundleVersion`, so the script takes one. Whether
  it is the version itself or a counter is the owner's to say.
- **The app icon.** The artwork is the owner's (`docs/RELEASE.md` section 1.2;
  D27 item 2). The script only places an `.icns` it is given.
- **User-visible wording (awaiting the owner).** The Windows installer adds
  wording: its pages use NSIS's English text, its file description is "Tawara
  installer", and the executable's description is "Tawara". The name "Tawara" is
  D8's display name. No copyright line is set: the installer's `LegalCopyright`
  is present and empty, and `NSHumanReadableCopyright` is left out of the macOS
  bundle until the owner approves its wording. The Licences page in Settings
  (`docs/RELEASE.md` section 4) is not in this change.
- **Xcode 26.6.** The build machine for this change has Xcode 27 only, so a
  `--dev` check uses that Xcode and says so. A release needs Xcode 26.6 beside
  it (`docs/RELEASE.md` section 1.6).
- **Signing, notarisation and stapling** with the owner's identities. No step
  in this change runs them against any account.
- **Windows (D36 decisions 2, 6 and 8).** This change builds to their
  recommendations: NSIS rather than WiX, x86_64 only, and the pinned archive.
  The archive is `nsis-3.13.zip`, which the SourceForge folder for 3.13 lists
  (checked 2026-10-18); the NSIS download page lists only `nsis-3.13-setup.exe`
  and the source archive for 3.13. Its hash was recorded on first use on
  2026-10-19, at the owner's request, from one network, so the two-network check
  that `--record-nsis` describes was not run. The folder's address and the
  script's address gave the same bytes, and SourceForge's listing gives the same
  SHA-1 and MD5 (decision 8). Its open parts:
  - **The publisher**, which D8 records as `patricksmithlaravel` and the owner
    confirms against the certificate's subject, and **the timestamp address**,
    which the certificate authority gives. The owner also supplies the icon.
- **The Linux choices in D36, awaiting the owner.** This change builds to D36's
  recommendations for Linux: the AppImage and the `.deb` (decision 1); the
  x86_64 architecture with glibc 2.35 as the floor (decision 6); the identifier
  `com.patricksmithlaravel.tawara.desktop` (decision 7); and the pinned tools,
  checked by SHA-256 (decision 8). Decision 9's accounts are the owner's.
- **The Linux maintainer and GPG key** (`docs/RELEASE.md` section 1.2). The
  script names no maintainer and holds no key. A release refuses the
  placeholder maintainer, and signs only when `TAWARA_GPG_KEY` is set.
- **Wording the owner has not approved** (awaiting the owner): the `.deb`'s
  `Description` ("Mochimo wallet for the desktop"), the desktop entry's
  `Comment` ("Mochimo wallet"), and its `Categories` (`Office;Finance;`). The
  name "Tawara" is D8's display name.
- **The checklist's text for Linux.** `docs/RELEASE.md` section 6.3 names the
  apt packages without `file` and `squashfs-tools`, and it runs appimagetool
  with `--appimage-extract-and-run`. The script does both differently, as step
  6 above says, and needs the two packages for it. Whether the checklist's text
  changes is the owner's to decide: this change leaves the text of
  `docs/RELEASE.md` section 6.3 as it is, and adds only desktop notes to the
  two items in sections 3 and 4 that name the packages.
- **The Linux build's caps.** The container has 2 CPUs, 4 GiB and two cargo
  jobs, the caps the owner's build-CPU rule sets. `docs/RELEASE.md` section 6.3
  records a `rustc` killed for lack of memory in an 8 GiB Docker VM at four
  jobs, so the caps are the owner's to change, here, in a pull request.
