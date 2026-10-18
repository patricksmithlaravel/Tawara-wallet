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
| `packaging/windows.*` | the NSIS installer, per user (D36 decision 2) | not yet written |
| `packaging/linux.sh` | the AppImage and the `.deb`, built in a pinned `ubuntu:22.04` container | not yet written |

The two plist files carry no XML comments: `codesign` refuses an entitlements file with one (`AMFIUnserializeXML: syntax error`), so the reasons are written here instead.

D36 decision 1 proposes one script per target instead of one packaging tool.
Each script stands alone. The tools it uses are the platform's own, or pinned
by version and hash where D36 says so. The scripts run under
macOS's `/bin/bash` (version 3.2), and pass `bash -n` and
`shellcheck -S warning`.

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
5. Signing, when `TAWARA_MAC_SIGN_ID` is set: `codesign --options runtime
   --timestamp` with the empty entitlements file (an ad-hoc `-` takes no
   timestamp), then `codesign --verify --deep --strict`. When `TAWARA_TEAM_ID`
   is also set, the signature must name that team.
6. `Tawara-X.Y.Z.dmg` (`Tawara-X.Y.Z-dev.dmg` with `--dev`), made by
   `hdiutil create` from a folder holding the app and a link to
   `/Applications`. It is signed the same way when an identity is set.
7. Notarisation, when a Developer ID identity is set (not `-`),
   `TAWARA_NOTARY_PROFILE` is set and the build is not `--dev`:
   `notarytool submit --wait`, which must report
   `status: Accepted`; then `stapler staple` and `stapler validate`; then
   `spctl` must accept the image with `source=Notarized Developer ID`.
8. `SHA256SUMS` beside the image, checked with `shasum -a 256 -c`.

The packages go to `target/package/macos` unless `--out` says otherwise. The
script prints `signed: yes|no; notarised: yes|no` at the end. A package that
says `no` for either must not be published (`docs/RELEASE.md` section 6.2).

The script first checks that both Rust targets are installed
(`rustup target list --installed`). When one is not, it stops and prints the
`rustup target add` command to run.

## Environment

No identity, team, key, certificate or password is read from a file, and none
is written into one. Each signing or notarisation step reads its name from the
environment and is skipped, with a message, when that name is unset.

| Variable | What it is for | Release | `--dev` |
|---|---|---|---|
| `DEVELOPER_DIR` | the Xcode to build with. Unset, and `/Applications/Xcode_26.6.app` exists, the script uses that one (`docs/RELEASE.md` section 1.6) | required; its macOS SDK must be 26.x | optional; a warning if the SDK is not 26.x |
| `TAWARA_ICON` | path to the app icon as an `.icns` file. The owner supplies the artwork (`docs/RELEASE.md` section 1.2); `iconutil -c icns` makes the file from an `.iconset` folder of the standard sizes, made from a 1024-pixel PNG | required | optional; without it the build has no icon |
| `TAWARA_BUILD_NUMBER`, or `--build-number` | the bundle's `CFBundleVersion` | required | defaults to `1` |
| `TAWARA_MAC_SIGN_ID` | the Developer ID Application identity, by the name `codesign` reads, such as `Developer ID Application: Name (TEAMID)` | unset: the packages are unsigned, with a warning | signs when set |
| `TAWARA_TEAM_ID` | the team the signature must name, checked after signing | optional | optional |
| `TAWARA_NOTARY_PROFILE` | the `notarytool` keychain profile, made with `xcrun notarytool store-credentials` on this Mac | needed to notarise | not used |
| `CARGO_TARGET_DIR` | where cargo writes its output, as cargo reads it | optional | optional |

`TAWARA_MAC_SIGN_ID=-` makes an ad-hoc signature. It is not an identity, it
exercises the signing steps for a local check, and its output is not for
distribution.

CI never runs this script with secrets. `.github/workflows/ci.yml` has
`permissions: {}`, and D36 decision 4 keeps signing on the owner's machines.

## Checks

- `bash -n packaging/macos.sh` and `shellcheck -S warning packaging/macos.sh`.
- `bash packaging/macos.sh --dev`, then `lipo -info` (both architectures),
  `vtool -show-build` (`minos 11.0` for each), `plutil -lint` on the bundle's
  `Info.plist`, and a launch: `LIVENESS_SECONDS=10 bash ci/liveness.sh
  target/package/macos/Tawara.app/Contents/MacOS/tawara-desktop`. The liveness
  script starts the app with a fresh `HOME`, once on wgpu and once on
  tiny-skia. Each launch must still be running after the wait, with no panic in
  its log, and the app is stopped at the end of each launch.

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
- **User-visible wording (awaiting the owner).** This change adds none. The
  name "Tawara" is D8's display name. No copyright line is set, so
  `NSHumanReadableCopyright` is left out until the owner approves its wording.
  The Licences page in Settings (`docs/RELEASE.md` section 4) is not in this
  change.
- **Xcode 26.6.** The build machine for this change has Xcode 27 only, so a
  `--dev` check uses that Xcode and says so. A release needs Xcode 26.6 beside
  it (`docs/RELEASE.md` section 1.6).
- **Signing, notarisation and stapling** with the owner's identities. No step
  in this change runs them against any account.
- **Windows and Linux.** Their scripts are not written yet.
