# Phase 1 report: iced on desktop and mobile, and the library on a device

docs/PLAN.md section 7, phase 1. The spike is `spikes/tawara-spike`: one iced
program, the same code on every platform, built and run by
`.github/workflows/spikes.yml`. `spikes/README.md` lists every check by its
ID; this report cites those IDs and the CI runs they come from. Phase 1 used
CI emulators and simulators only, as the owner chose; nothing here was run on
a physical device.

## The answer in brief

iced 0.14 runs Tawara's kind of interface on Windows, macOS, Linux, an
Android emulator and an iOS simulator, from one code base, once its shell
(`iced_winit`) carries a 138-line patch. Rendering, touch, scrolling, the
soft keyboard appearing and going with focus, typing through key events,
Return, Back, rotation, density changes, going to the background and coming
back, a cold start, the clipboard (through platform calls), the worker
thread with progress and cancellation, and every library check all work on
both mobile platforms. The library behaves on both: stores, commits, the
lock, the owner, mode and link checks, one unlock at `Kdf::RECOMMENDED`, and
TLS to `api.mochimo.org`.

What does not work, or could not be shown, is concentrated in text input
and in the system's edges:

- the soft keyboard is an ordinary one for the password field on both
  platforms (no secure entry), and input-method composition is absent from
  winit 0.30 on both, so non-Latin text and some accented input are at risk;
- neither platform's insets reach the app: content sits under the iPhone's
  Dynamic Island and Android's gesture bar, and the keyboard covers the
  lower part of the window;
- an Android activity destroyed while its process lives cannot be
  recreated (winit allows one event loop per process);
- the arm64 Android library could only be tried under the emulator's ARM
  translation, where it crashes in android-activity's start-up before any
  Tawara code runs.

My recommendation is **go, with conditions** (last section).

## Approaches compared (Android)

The plan named three. iced 0.14's own shell does not run on Android or iOS
as published, so all three need a modified iced:

1. `iced_winit` 0.14.1 does not compile for either platform: it imports
   winit's `modifier_supplement`, which winit provides only on desktop
   platforms.
2. On Android it builds winit's event loop itself, with no way to hand it
   the `AndroidApp` that winit requires.
3. On Android it opens its window at start-up, before the system has given
   the app a native window; creating the surface then panics. After a
   return from the background it does not rebuild the surface.
4. On iOS it sizes the window 1024×768 points.

| Approach | Finding | Chosen |
|---|---|---|
| iced's own shell with `android-activity`, NativeActivity | Built and run (this report). A 138-line patch to `iced_winit` (`spikes/patches/`, docs/DECISIONS.md D17) fixes all four; iced's tasks, subscriptions and executor are unchanged, so the worker, progress and cancellation are the desktop code. No Java, no Gradle: the APK is built with aapt2, zip, zipalign and apksigner. | **yes** |
| iced's own shell with GameActivity | Read, not built: needs Gradle and AndroidX, and winit 0.30.13 drops GameActivity's text events, so it would gain nothing today. | no |
| The community example with Java interop | Read, not built: it writes its own shell, which discards iced's tasks, and still needs a fork of iced, Java and Gradle. | no |
| `iced_mobile` | Read, not built: it has no licence and does not compile against iced 0.14. | no |

## What worked

Runs referred to below: run 6 (`7ee145d`, the first full Android checklist),
run 8 (`d25a266`, iOS simulator green, desktop green on all three systems)
and run 9 (`c436fc6`, every job green apart from the three expected Android
failures, A10, A18 and A22). Earlier runs found faults in the spike's own
checks (timings too tight for a busy host, a script's quoting), each fixed
and recorded in its commit.

**Desktop** (Linux under Xvfb, macOS 26, and the windows-latest runner; runs
1 to 9). The
self-test passes with iced's own renderer choice, with tiny-skia forced and
with wgpu forced, on all three: the password field and its move into a
`Zeroizing` buffer, the worker with progress, cancellation and completion,
the interface responsive throughout (largest frame gap 3 to 47 ms while the
worker ran, apart from one 926 ms stall in run 1's informational
forced-wgpu launch on macOS, not seen since), iced's clipboard, and on Linux
and macOS every library check.
On Linux, xdotool typed `hunter2` and Return with real key events and turned
the mouse wheel over the list. On macOS and Windows nothing typed (CI cannot
drive their keyboards), so the field was filled from inside the app.

**Android** (API 35 google_apis x86_64 emulator, 1080×2400 at 420 dpi,
wgpu on Vulkan over SwiftShader by default):

| Checklist item (owner's list) | Result | Check |
|---|---|---|
| Rendering | First frame and screenshots correct with wgpu and with tiny-skia | A3, A20 |
| Touch | Taps reach the rows | A11 |
| Scrolling | A swipe scrolls the list (offsets of 85 to 269 logical pixels) | A12, runs 8 and 9 |
| Soft keyboard on focus, off on blur, back on refocus | Works (`mInputShown` true, false, true) | A5, A9 |
| Typing and Return | Key events type `hunter2`; Enter submits | A6, A7 |
| Paste | The platform clipboard works through JNI (`ClipboardManager`) | A8 |
| Back | Delivered to the app, which moves its task to the background; same process on return | A14 |
| Rotation | The window resizes to 914×411; same process | A15 |
| Suspend and resume, surface loss and recreation | Home during a task, then back: same process, drawing again after a 5.4 s gap, no crash | A13 |
| Display density | `wm density 160` and back: the scale factor follows; same process | A16 |
| Process death | Killed, then a cold start in 0.9 to 1.6 s | A17 |
| Store files | Store directory 0700, files 0600, owned by the app's user; `files/` is 0771, so the spike keeps stores in `no_backup/` | A19 |
| targetSdk 36 | Runs; Back still delivered | A21 |
| Worker and interface | Progress, cancellation and completion work; the thread stops within one step of a cancel. With the default renderer (wgpu on SwiftShader, a CPU implementation of Vulkan) the interface ran up to about 0.5 s behind the busy worker (largest frame gap 265 to 352 ms); with tiny-skia, 73 to 97 ms. A phone's GPU should do better; device numbers are manual | A8, A20 |

**iOS** (iPhone 17 simulator, the newest runtime on the macOS 26 runner, wgpu
on Metal by default; runs 8 and 9 green):

| Checklist item | Result | Check |
|---|---|---|
| Rendering | Correct with Metal and with tiny-skia; the window fills the screen (402×874 points) once the patch leaves the size to the screen | I1, I2, I7 |
| Soft keyboard on focus and off on blur | Shown on focus (screenshot: the system keyboard, at first its one-time "slide to type" sheet), gone when the field goes, back when it returns | I4, screenshots |
| Typing and Return | Text sent through winit's own `insertText:` arrives; Return submits | I3 |
| Paste | `UIPasteboard` write and read; the value read from outside with `simctl pbpaste` | I5 |
| Rotation | The window resizes to 874×402 on request (run 1); the screenshot 2 s later still showed portrait, so the visual rotation is not confirmed | I9 |
| Suspend and resume | Settings in front, then back: same process, drawing again after a 1.6 s gap, no Metal errors in the system log | I6 |
| Crashes | None reported | I10 |

**The library on a device.** Every check passes on the Android emulator and
the iOS simulator, in both renderers: a store created at the cheap test
cost with three commits (no signing), modes 0700 and 0600, a second open
refused with `Error::Locked` while the first holds the lock, a second create
refused, reopen and a commit through the reopened handle, a group-writable
directory refused with `UnsafePermissions` and left untouched, a symlinked
store directory refused with `StoreDirectoryIsLink`, an absent directory
reported, and `network_status` over TLS from `https://api.mochimo.org` (160
to 870 ms on the mobile runners). The probe of Android's `files/` directory
is refused (mode 0771), which is why Tawara's Android store will live in
`no_backup/` or a 0700 subdirectory.

## What did not, and where

| What | Where | Effect | Check |
|---|---|---|---|
| The shell does not compile or start on mobile | iced (`iced_winit` 0.14.1) | Patched for the spike (D17); production needs a route (below) | build |
| No secure keyboard for the password field | winit 0.30.13: `set_ime_purpose` does nothing on Android and is ignored on iOS | The keyboard may suggest and learn what is typed into the password field | screenshots; device check |
| No input-method composition | winit 0.30.13 (NativeActivity has no composition events; iOS has no marked text) | Non-Latin scripts, and accents typed through composition, likely do not arrive | code reading; device check |
| Keyboard not shown again after the user dismisses it | iced (`text_input` asks for it only when focus changes) | A tap on the still-focused field does nothing | A10 (expected) |
| Activity destroyed while the process lives | winit 0.30.13: one event loop per process | Relaunch after such a destroy shows nothing; the spike's `configChanges`, `singleTask` and Back-to-background avoid the common causes | A18 (expected) |
| A swipe that starts on a button does not scroll | iced (`scrollable` lets its content take the touch first; a button captures the press) | Lists of tappable rows must react on release, and must ignore a release after a drag | A12 |
| No safe-area or keyboard insets | winit 0.30.13 on both platforms | Content under the Dynamic Island and the gesture bar; the keyboard covers the lower window | screenshots |
| iced's clipboard does nothing | window_clipboard 0.5.1 (stub backends on Android and iOS) | Platform calls instead (done in the spike) | A8, I1 |
| No system fonts | fontdb finds none on Android or iOS | Fonts are bundled (the spike: Fira Sans; Tawara: its own) | rendering |
| The arm64 library under ARM translation | android-activity 0.6.1 start-up with `jni` 0.22.4, on the x86_64 image's translator | Crash before any Tawara code; the x86_64 build of the same code passes everything. A real arm64 device decides | A22 (expected) |
| No UIScene support | winit 0.30.13 | Apple's technical note TN3187 says apps built with the SDK after iOS 26 must adopt the scene life cycle; unless winit adds it, that SDK cannot be used (from the note; not testable today) | none |

Emulator and simulator timings vary with the host: on a busy simulator host
the worker's first steps and the unlock took several times longer (the
unlock 1.6 to 3.7 s against 0.2 s on a quiet one). The checks wait for slow
hosts rather than fail on them, and the report quotes both.

## Platform glue each mobile platform needed

**Android** (Rust only, no Java):

- `android_main` handing the `AndroidApp` to the patched shell
  (`iced_winit::set_android_app`), and ending the process when the event
  loop returns;
- JNI calls for the clipboard (`ClipboardManager`) and for Back
  (`Activity.moveTaskToBack`);
- the app-private directory: `no_backup/` beside `files/` (which the
  library refuses at mode 0771);
- the manifest (NativeActivity, `hasCode=false`, uncompressed 16 KB-aligned
  native libraries, `INTERNET`, no backup) and an APK built with aapt2, zip,
  zipalign and apksigner;
- still needed for production, not in the spike: insets (JNI
  `WindowInsets`), showing the keyboard again on a tap, and backup exclusion
  rules.

**iOS**:

- the binary as the bundle's executable (winit calls `UIApplicationMain`),
  with a hand-written `Info.plist`, signed ad hoc for the simulator;
- `UIPasteboard` for the clipboard;
- the app-private directory under `Library/Application Support`;
- test hooks the spike needed because the simulator cannot be driven:
  typing through `insertText:`, safe-area insets, rotation requests;
- still needed for production, not in the spike: safe-area insets applied
  to the layout, secure keyboard traits, a privacy cover for the app
  switcher, the scene life cycle, and a signed device build.

## Unlock measurements

One unlock at `Kdf::RECOMMENDED` (Argon2id, 64 MiB, 3 passes) in a fresh
store, timed on a worker thread while another thread sampled the resident
size every millisecond. "Added" is the sampled peak over the resident size
just before.

| Platform | Run, renderer | Wall time | Resident size before → peak (added) |
|---|---|---|---|
| Android emulator, x86_64, 4 vCPU (KVM) | run 6, wgpu | 432.7 ms | 158,448 → 223,984 KiB (+65,536) |
| Android emulator | run 6, tiny-skia | 191.2 ms | 146,248 → 221,896 KiB (+75,648) |
| Android emulator | run 8, tiny-skia | 211.8 ms | 146,372 → 222,020 KiB (+75,648) |
| Android emulator | run 9, wgpu | 539.6 ms | 159,880 → 225,288 KiB (+65,408) |
| Android emulator | run 9, tiny-skia | 199.2 ms | 146,888 → 222,536 KiB (+75,648) |
| iOS simulator, macOS 26 runner | run 1, Metal | 216.4 ms | +65,536 KiB |
| iOS simulator | run 1, tiny-skia | 210.5 ms | +65,536 KiB |
| iOS simulator | run 8, Metal | 236.4 ms | 216,928 → 282,560 KiB (+65,632) |
| iOS simulator | run 6, tiny-skia | 237.4 ms | 228,016 → 305,936 KiB (+77,920) |
| iOS simulator, busy host | runs 2, 6, 8 | 1,588.5 to 3,742.8 ms | +65,536 to +87,264 KiB |
| Linux runner | run 8 | 103.0 to 104.3 ms | 19,632 → 85,172 KiB (+65,540) |
| macOS runner | run 8 | 110.0 to 140.7 ms | no rise: the allocator reused the 64 MiB freed by the store's creation just before |
| Windows runner | | not measured (the spike's library checks are Unix-only) | |

On every platform an unlock adds the 64 MiB the parameters ask for and
little else. The tiny-skia runs show about 10 MiB more, most likely the
software renderer drawing during the unlock. An unlock takes 0.2 to 0.55 s
on the emulator and simulator. Real phones will differ; the device steps in
`spikes/README.md` print the same lines.

## Recommendation

**Go with iced for mobile, with three conditions before phase 4 starts:**

1. **Text entry on real phones.** Run the device checks in
   `spikes/README.md` on one Android phone and one iPhone: accented and
   non-Latin text, composition, the password field's keyboard (suggestions
   and learning), paste from another app, and the arm64 Android build. If
   composition or secure entry fails, the remedy is platform text-input glue
   (on Android, GameActivity with a winit change that forwards its text
   events, or a native text field over the password input), which is phase
   4 work and should be planned as such.
2. **A route for the `iced_winit` change.** The spike patches the crates.io
   release. Production needs one of: a fork of iced at a git revision
   (allowed by the plan, D2, with `allow-git` in deny.toml), the change sent
   upstream, or a small shell crate of Tawara's own. I recommend sending the
   compile fix and the `AndroidApp` hand-off upstream and carrying a fork at
   a pinned revision until they land.
3. **Accept or mitigate the known limits.** Insets and the keyboard re-show
   need per-platform glue in phase 4; activity recreation (A18) and the
   missing UIScene support need winit changes, so they should be tracked
   upstream with a date by which the mobile shells are reconsidered (plan
   D3's fallback) if they have not moved.

If the owner prefers not to carry these risks, the plan's alternative is
native mobile shells over `wallet-core`, which this phase did not evaluate.
