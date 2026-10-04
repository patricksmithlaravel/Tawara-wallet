# Phase 1 spikes

Throwaway code for docs/PLAN.md section 7, phase 1: can iced carry Tawara on
Windows, macOS, Linux, Android and iOS, and does the wallet library behave on
a phone? Nothing here ships, and nothing in `crates/` depends on it. The
findings are in `docs/spikes/P1-REPORT.md`; the owner's go/no-go on iced for
mobile follows that report.

## What is here

| Path | What it is |
|---|---|
| `Cargo.toml` | A workspace of its own, separate from the application's. It carries the `[patch]` below and restates the release profile. |
| `tawara-spike/` | One iced program for every platform. Library `tawara_spike` (Android loads it as `libtawara_spike.so`), binary `tawara-spike` (desktop, and the iOS app's executable). |
| `patches/iced_winit-0.14.1-mobile.patch` | The change iced's shell needs on Android and iOS (below). |
| `tools/vendor-iced-winit.sh` | Unpacks crates.io `iced_winit` 0.14.1 into `vendor/` (ignored by git), checks it against the application's `Cargo.lock` checksum, and applies the patch. Run it before any cargo command here. |
| `platform/android/AndroidManifest.xml`, `platform/ios/Info.plist.in` | The two platform files. No Gradle, no Java, no Xcode project. |
| `ci/` | The scripts `.github/workflows/spikes.yml` runs: `desktop.sh`, `android.sh`, `ios.sh`, and `shot.py` (Android screenshots, standard library only). |

## Why iced_winit is patched

iced 0.14 cannot run on Android or iOS as published:

1. `iced_winit` 0.14.1 does not compile for either. It imports winit's
   `modifier_supplement`, which winit provides only on desktop platforms.
2. On Android, winit needs the `AndroidApp` that `android_main` receives, and
   `iced_winit::run` builds the event loop itself with no way to pass it in.
3. On Android, iced opens its window at start-up, before the system has given
   the app a native window; creating the surface then fails, and iced panics.
   When the app goes to the background, iced keeps its surfaces on the
   native window the system is destroying, which winit forbids, and on
   return it does not build new ones.
4. On iOS, iced gives winit a 1024×768 window size, which winit uses as the
   window's frame in points.

The patch (+166/−19 lines) fixes the two compile errors, adds
`iced_winit::set_android_app`, holds iced's start-up actions until the first
`Resumed`, drops every window's surface on `Suspended` before winit's
callback returns (winit requires it), builds new ones on the next
`Resumed`, skips drawing while suspended, and leaves the window size to the
screen on iOS. Most of it is behind `cfg(target_os = ...)`; the rest acts
only on `Suspended` and `Resumed`, which iced receives only on Android. It
is applied to the crates.io tarball, so the build has one copy of every
other iced crate. Choosing how production carries this (a fork at a git
revision, an upstream change, or a shell crate of Tawara's own) is a
decision for after the go/no-go; docs/DECISIONS.md records it.

`spikes/` is outside the application's policy checks and outside `cargo
deny`: the `[patch]` here would fail both, by design. The spike uses public
test vectors only (the store phrase is the BIP39 vector "abandon ... art",
which must never be funded), and the Android APK is debug-signed and
debuggable.

## Running it

On a desktop, from this directory:

```
bash tools/vendor-iced-winit.sh
cargo run --release --bin tawara-spike -- --self-test --storage /tmp/spike-data --screenshot /tmp/spike.png
```

`--offline` skips the network check. `ICED_BACKEND=tiny-skia` forces the
software renderer, `ICED_BACKEND=wgpu` the GPU one. Without `--self-test`
the window stays open for trying the fields and buttons by hand.

The self-test prints one line per fact on stderr: `SPIKE PASS <name>:
<detail>`, `SPIKE FAIL <name>: <detail>`, `SPIKE INFO <fact>`, and at the end
`SPIKE DONE pass=N fail=M`. It never prints a password, only lengths.

## The self-test

The timeline starts 1.5 seconds after the window opens, not when the program
starts: on iOS the window opens only once UIKit has finished launching.

1. Focus the password field and take an in-app screenshot (`render.*`). CI
   then has 10 seconds to type and press Return (`pw_len=7`, `submit pw_len=7`);
   on iOS the app types through winit's own `insertText:`. Submitting moves
   the value into a `Zeroizing` buffer and clears the field
   (`password.moved_to_zeroizing`).
2. Start a slow task on a worker thread (40 steps of 100 ms of busy work),
   check it reports progress, cancel it, check it stopped (`task.*`).
3. Run a 20-step task to the end, and check the interface kept drawing while
   it ran: no gap between frames of 500 ms or more (`ui.responsive`).
4. Clipboard: iced's own on the desktop (`clipboard.iced_roundtrip`); on
   mobile, where iced's is a stub, the platform's through JNI or
   `UIPasteboard` (`clipboard.platform_roundtrip`).
5. Remove the field, then bring it back and focus it, so CI can watch the
   keyboard hide and return.
6. The library, on a worker thread (`lib.*`, unix only): a store at the
   cheap test cost with three commits, its modes (0700 and 0600), the lock
   (`Locked` for a second open), reopen and a commit through the reopened
   handle, refusal of a group-writable directory and of a symlink, one
   unlock at `Kdf::RECOMMENDED` with its time and memory, and
   `network_status` from `https://api.mochimo.org` over TLS.
7. iOS only: ask UIKit to rotate to landscape and back.
8. `SPIKE DONE`. The desktop exits (code 1 if anything failed); mobile
   prints `READY lifecycle` for CI's lifecycle checks, starts a 60-second
   task, and from then on keeps drawing and logs `frames N` about once a
   second (on iOS, where iced reports no move to the background, this
   heartbeat is how CI sees the app draw again after a return).

## What CI checks

Each script prints `CHECK PASS|FAIL|INFO|SKIP <id> <name>: <detail>` and a
table in the run's summary.

### Desktop (`ci/desktop.sh`, Linux under Xvfb, macOS, Windows)

Three launches: iced's own renderer choice, tiny-skia forced, wgpu forced
(informational). Each must end with `SPIKE DONE`, no `SPIKE FAIL` and exit
code 0. On Linux, xdotool types `hunter2` and Return with real key events
and turns the mouse wheel over the row list, and the screen is captured as
well as the in-app screenshot.

### Android (`ci/android.sh`, API 35 google_apis x86_64 emulator)

| ID | Check |
|---|---|
| A1 | The targetSdk 34 APK installs |
| A2 | It launches, and the app starts |
| A3 | First frame: the in-app screenshot is not blank; a screen capture |
| A4 | Which renderer was chosen (informational) |
| A5 | The soft keyboard shows when the field is focused |
| A6 | `adb shell input text hunter2` arrives (key events, not the input method) |
| A7 | Enter submits |
| A8 | The app's own checks: task, cancel, responsiveness, clipboard through JNI, library, TLS |
| A9 | The keyboard hides when the field goes, and returns when it is focused again |
| A10 | After the user dismisses the keyboard, tapping the focused field shows it again. **Expected to fail**: iced asks for the keyboard only when focus changes |
| A11 | A tap reaches a row of the touch area |
| A12 | A swipe scrolls the row list. The rows react on release: iced's scrollable lets its content see a touch first, and a button captures the press, so a swipe that starts on a button does not scroll |
| A13 | Home during a running task, then back: same process, drawing again, no crash |
| A14 | Back reaches the app, which moves to the background; back again, same process |
| A15 | Rotation: same process, a resize |
| A16 | A density change: same process, a rescale or resize |
| A17 | Process killed, then a cold start whose first frame passes the app's screenshot check |
| A18 | The activity destroyed while the process lives, then relaunched, with a first frame that passes the screenshot check. **Expected to fail**: winit allows one event loop per process |
| A19 | The store's directory is 0700 and its files 0600, owned by the app's user |
| A20 | The whole self-test with tiny-skia forced |
| A21 | The targetSdk 36 APK: layout under edge-to-edge, and Back (informational) |
| A22 | The arm64 library, where the image translates arm64 code. **Expected to fail**: under the x86_64 image's ARM translation, android-activity's start-up JNI (`jni` 0.22.4's frame check) panics before the app's code runs, while the x86_64 build of the same code passes everything; a real arm64 device decides |
| A23 | No crash anywhere outside A18 and A22 |

### iOS (`ci/ios.sh`)

| ID | Check |
|---|---|
| D1 | The device build links for iOS (`LC_BUILD_VERSION` platform 2). Not installed: that needs signing |
| I1 | The self-test in the simulator ends with no failure |
| I2 | The window fills the screen (the layout line), and the renderer |
| I3 | Typing and Return through winit's `insertText:` |
| I4 | The keyboard on screen while the field is focused (screenshot) |
| I5 | The app's pasteboard write, read from outside with `simctl pbpaste` |
| I6 | Settings in front, then back: both launches succeed, the same process returns, and it draws a frame (its `frames N` heartbeat) within 10 s. How many heartbeats came while Settings was in front (informational) |
| I7 | The whole self-test with tiny-skia forced |
| I8 | The safe-area insets (informational) |
| I9 | Rotation on request (informational) |
| I10 | No crash report |

## What CI cannot measure

These need a real device, by hand:

- scrolling on iOS (simctl has no swipe; XCUITest could drive one);
- typing on the on-screen keyboard: accented and non-Latin characters,
  composition (pinyin), glide typing, autocorrect, and whether the keyboard
  learns what is typed into the password field;
- paste from another app, and the system's paste prompt;
- the app-switcher snapshot;
- real-GPU drawing, frame pacing, unlock time and memory on a phone;
- low-memory kills and split screen.

**Android device.** Turn on developer options and USB debugging, then:

```
adb install -r spike-t34.apk
adb shell setprop debug.tawara.spike 1
adb shell am start -n com.patricksmithlaravel.tawara.spike/android.app.NativeActivity
adb logcat -s RustStdoutStderr
```

Then tap the field, type ASCII and non-ASCII text, press Enter, dismiss the
keyboard and tap the field again, copy and paste to and from another app,
press Home during a task and return, rotate, lock and unlock, and press Back.
CI keeps no APK (a `run:` step cannot upload artifacts), so build one with
the Android SDK and NDK installed: `ci/android.sh build`, then
`ci/android.sh apk`.

**iOS device.** Create a provisioning profile for
`com.patricksmithlaravel.tawara.spike` (letting Xcode manage signing for an
empty app with that bundle ID once is the simplest way). Build with
`--target aarch64-apple-ios`, make `TawaraSpike.app` as `ci/ios.sh` does
with `@PLATFORM@=iPhoneOS`, copy the profile in as `embedded.mobileprovision`,
sign it with `codesign --force --sign "Apple Development: ..."`, then:

```
xcrun devicectl device install app --device <id> TawaraSpike.app
xcrun devicectl device process launch --console --device <id> \
  --environment-variables '{"SPIKE_SELFTEST":"1"}' com.patricksmithlaravel.tawara.spike
```
