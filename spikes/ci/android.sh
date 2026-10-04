#!/usr/bin/env bash
# Android spike on a GitHub-hosted ubuntu-24.04 runner, plain commands only:
#
#   ci/android.sh build      # libtawara_spike.so for arm64-v8a and x86_64
#   ci/android.sh apk        # two APKs from one .so: targetSdk 34 and 36
#   ci/android.sh emulator   # KVM, SDK packages, AVD, boot, checklist, evidence
#
# The app prints `SPIKE ...` lines on stderr, which android-activity sends to
# logcat under the tag RustStdoutStderr. This script prints `CHECK PASS|FAIL|
# INFO|SKIP <id> <name>: <detail>` lines of its own, a table in the step
# summary, and screenshots as base64 PNG thumbnails between markers (run
# steps get no token to upload artifacts). The checklist IDs are those of
# spikes/README.md.
set -Eeuo pipefail

PKG=com.patricksmithlaravel.tawara.spike
ACTIVITY=android.app.NativeActivity
LIB=tawara_spike
MIN_API=26
TARGETS=(aarch64-linux-android x86_64-linux-android)
IMG="system-images;android-35;google_apis;x86_64"
NDK_PIN=29.0.14206865
SERIAL=emulator-5554
TAG=RustStdoutStderr
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
SPIKES=$(cd "$HERE/.." && pwd)
OUT=${RUNNER_TEMP:-/tmp}/android
SDK=${ANDROID_HOME:-${ANDROID_SDK_ROOT:?ANDROID_HOME is not set}}
export ANDROID_AVD_HOME=$HOME/.android/avd
export PATH="$SDK/platform-tools:$SDK/emulator:$SDK/cmdline-tools/latest/bin:$PATH"
LOG=$OUT/logcat.txt
mkdir -p "$OUT"/{shots,raw,apk}

group() { echo "::group::$*"; }
endgroup() { echo "::endgroup::"; }
die() { echo "::error::$*"; exit 1; }

ndk_paths() {
  NDK=$(ls -d "$SDK"/ndk/29.* 2>/dev/null | sort -V | tail -1 || true)
  if [ -z "$NDK" ]; then
    yes 2>/dev/null | sdkmanager --install "ndk;$NDK_PIN" >/dev/null
    NDK=$SDK/ndk/$NDK_PIN
  fi
  TC=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin
  [ -x "$TC/clang" ] || die "no NDK clang at $TC"
}

abi_of() {
  case $1 in
    aarch64-linux-android) echo arm64-v8a ;;
    x86_64-linux-android) echo x86_64 ;;
  esac
}

# ---------------------------------------------------------------------------
build() {
  ndk_paths
  echo "NDK $NDK"
  cd "$SPIKES"
  for t in "${TARGETS[@]}"; do
    local u=${t//-/_}
    local U=${u^^}
    export "CARGO_TARGET_${U}_LINKER=$TC/${t}${MIN_API}-clang"
    export "CC_${u}=$TC/${t}${MIN_API}-clang" "CXX_${u}=$TC/${t}${MIN_API}-clang++"
    export "AR_${u}=$TC/llvm-ar" "RANLIB_${u}=$TC/llvm-ranlib"
    group "cargo rustc --crate-type cdylib --target $t"
    cargo rustc --locked --release -p tawara-spike --lib --crate-type cdylib --target "$t"
    endgroup
    local so=target/$t/release/lib$LIB.so
    group "inspect $so"
    ls -la "$so"
    "$TC/llvm-readelf" -d "$so" | grep NEEDED || true
    "$TC/llvm-readelf" --dyn-syms -W "$so" | grep -E ' ANativeActivity_onCreate$' \
      || die "$so does not export ANativeActivity_onCreate"
    echo "LOAD alignments: $("$TC/llvm-readelf" -lW "$so" | awk '$1=="LOAD"{print $NF}' | sort -u | xargs)"
    endgroup
  done
  if cargo tree --locked --target x86_64-linux-android -i aws-lc-sys >/dev/null 2>&1; then
    echo "::warning::aws-lc-sys is in the Android graph; the library's TLS is meant to stay on ring"
  fi
}

# ---------------------------------------------------------------------------
apk() {
  ndk_paths
  cd "$SPIKES"
  local bt jar
  bt=$(ls -d "$SDK"/build-tools/* | sort -V | tail -1)
  jar=$(ls -d "$SDK"/platforms/android-3[5-9] 2>/dev/null | sort -V | tail -1)/android.jar
  [ -f "$jar" ] || die "no android-35 or later platform in $SDK/platforms"
  echo "build-tools $bt; platform $jar"
  local w=$OUT/apk
  rm -rf "$w"
  mkdir -p "$w/root/lib"
  for t in "${TARGETS[@]}"; do
    local abi
    abi=$(abi_of "$t")
    mkdir -p "$w/root/lib/$abi"
    "$TC/llvm-strip" --strip-debug -o "$w/root/lib/$abi/lib$LIB.so" "target/$t/release/lib$LIB.so"
  done
  keytool -genkeypair -noprompt -keystore "$w/debug.keystore" -storepass android -keypass android \
    -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 \
    -dname "CN=Android Debug,O=Android,C=US" >/dev/null 2>&1
  for sdk in 34 36; do
    group "APK, targetSdk $sdk"
    "$bt/aapt2" link -o "$w/u$sdk.apk" -I "$jar" --manifest platform/android/AndroidManifest.xml \
      --min-sdk-version "$MIN_API" --target-sdk-version "$sdk" \
      --version-code 1 --version-name 0.1.0 --debug-mode
    # Stored, not deflated, and 16 KB-aligned: the manifest says
    # extractNativeLibs="false", so the system maps the libraries in place.
    (cd "$w/root" && zip -q -0 -r -D -X "../u$sdk.apk" lib)
    "$bt/zipalign" -f -P 16 4 "$w/u$sdk.apk" "$w/a$sdk.apk"
    "$bt/apksigner" sign --ks "$w/debug.keystore" --ks-pass pass:android --key-pass pass:android \
      --ks-key-alias androiddebugkey --v4-signing-enabled false --out "$OUT/spike-t$sdk.apk" "$w/a$sdk.apk"
    "$bt/apksigner" verify --print-certs "$OUT/spike-t$sdk.apk" | head -3
    "$bt/zipalign" -c -P 16 4 "$OUT/spike-t$sdk.apk" && echo "zipalign -c -P 16: ok"
    "$bt/aapt2" dump badging "$OUT/spike-t$sdk.apk" | grep -E "^package:|targetSdk|native-code|debuggable|uses-permission"
    ls -la "$OUT/spike-t$sdk.apk"
    endgroup
  done
}

# ---------------------------------------------------------------------------
a() { adb -s "$SERIAL" "$@"; }
ash() { adb -s "$SERIAL" shell "$@" | tr -d '\r'; }

set_ini() {
  local f=$1 k=$2 v=$3
  if grep -q "^$k=" "$f"; then sed -i "s|^$k=.*|$k=$v|" "$f"; else echo "$k=$v" >>"$f"; fi
}

emulator_up() {
  group "KVM"
  echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' \
    | sudo tee /etc/udev/rules.d/99-kvm4all.rules >/dev/null
  sudo udevadm control --reload-rules
  sudo udevadm trigger --name-match=kvm
  ls -l /dev/kvm || die "/dev/kvm missing"
  endgroup

  group "SDK packages"
  (yes 2>/dev/null || true) | sdkmanager --licenses >/dev/null 2>&1 || true
  sdkmanager --install platform-tools emulator "$IMG" >"$OUT/sdkmanager.txt" 2>&1 \
    || { tail -30 "$OUT/sdkmanager.txt"; die "sdkmanager failed"; }
  # The emulator links against desktop libraries the runner image lacks
  # (libpulse first); -no-window still loads them.
  sudo apt-get update -q >/dev/null
  sudo apt-get install -y -q --no-install-recommends libpulse0 libnss3 libxcomposite1 \
    libxcursor1 libxdamage1 libxi6 libxtst6 libxkbfile1 libgl1 libegl1 libasound2t64 \
    libbsd0 libxkbcommon-x11-0 libx11-xcb1 >/dev/null
  # The emulator's launcher puts its own libraries (lib64 and below) on the
  # path; with those, anything ldd still cannot find is the system's to
  # supply. A warning only: `emulator -version` below is the real test.
  local libpath missing
  libpath=$(find "$SDK/emulator/lib64" -name '*.so*' -printf '%h\n' | sort -u | paste -sd:)
  missing=$(LD_LIBRARY_PATH=$libpath ldd "$SDK/emulator/qemu/linux-x86_64/qemu-system-x86_64" | grep 'not found' || true)
  [ -z "$missing" ] || echo "::warning::the emulator may lack: $(echo "$missing" | xargs)"
  emulator -version | head -1
  endgroup

  group "AVD and boot"
  echo no | avdmanager create avd --force -n spike -k "$IMG" >/dev/null
  local cfg=$ANDROID_AVD_HOME/spike.avd/config.ini
  set_ini "$cfg" hw.keyboard no # the soft keyboard must be allowed to show
  set_ini "$cfg" hw.ramSize 4096
  set_ini "$cfg" hw.cpu.ncore 4
  set_ini "$cfg" disk.dataPartition.size 6G
  set_ini "$cfg" hw.lcd.width 1080
  set_ini "$cfg" hw.lcd.height 2400
  set_ini "$cfg" hw.lcd.density 420
  nohup emulator -avd spike -port "${SERIAL#emulator-}" -no-window -no-audio -no-boot-anim \
    -no-snapshot -gpu swiftshader_indirect -accel on -camera-back none -camera-front none \
    >"$OUT/emulator.txt" 2>&1 &
  echo $! >"$OUT/emulator.pid"
  adb start-server >/dev/null
  timeout 300 adb -s "$SERIAL" wait-for-device || { tail -50 "$OUT/emulator.txt"; die "no adb device"; }
  local t=0
  until [ "$(ash getprop sys.boot_completed 2>/dev/null)" = 1 ]; do
    kill -0 "$(cat "$OUT/emulator.pid")" 2>/dev/null || { tail -80 "$OUT/emulator.txt"; die "the emulator exited"; }
    [ "$t" -ge 600 ] && { tail -80 "$OUT/emulator.txt"; die "no boot after 600 s"; }
    sleep 5
    t=$((t + 5))
  done
  until a shell pm path android >/dev/null 2>&1; do sleep 2; done
  echo "booted after about ${t} s"
  endgroup

  group "device"
  ash settings put global window_animation_scale 0
  ash settings put global transition_animation_scale 0
  ash settings put global animator_duration_scale 0
  ash settings put global stay_on_while_plugged_in 7
  ash settings put system screen_off_timeout 2147483647
  ash settings put secure show_ime_with_hard_keyboard 1
  ash input keyevent KEYCODE_WAKEUP
  ash wm dismiss-keyguard || true
  for p in ro.build.fingerprint ro.build.version.sdk ro.product.cpu.abilist ro.dalvik.vm.native.bridge; do
    echo "$p=$(ash getprop $p)"
  done
  ash getconf PAGE_SIZE || true
  ash wm size
  ash wm density
  ash ime list -s || true
  a logcat -c
  a logcat -v threadtime >"$LOG" 2>&1 &
  echo $! >"$OUT/logcat.pid"
  endgroup
}

emulator_down() {
  a emu kill >/dev/null 2>&1 || true
  [ -f "$OUT/logcat.pid" ] && kill "$(cat "$OUT/logcat.pid")" 2>/dev/null || true
}

# --------------------------------------------------------------- helpers
EXPECTED_FAIL=" A10 A18 " # known limitations (spikes/README.md); recorded, not fatal

record() { # record ID NAME RESULT DETAIL
  printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "${4:-}" >>"$OUT/results.tsv"
  echo "CHECK $3 $1 $2: ${4:-}"
}
mark() { wc -l <"$LOG"; }
since() { tail -n +"$(($1 + 1))" "$LOG"; }
spike_since() { since "$1" | grep -E "$TAG: SPIKE " | sed -E "s/.*$TAG: //" || true; }
wait_log() { # wait_log MARK ERE SECONDS: prints the first matching app line
  local t=0 line
  while :; do
    line=$(spike_since "$1" | grep -E -m1 "$2" || true)
    [ -n "$line" ] && { echo "$line"; return 0; }
    [ "$t" -ge "$3" ] && return 1
    sleep 1
    t=$((t + 1))
  done
}
shot() { # shot NAME: prints the stats line
  a exec-out screencap >"$OUT/raw/$1.raw"
  python3 "$HERE/shot.py" stats "$OUT/raw/$1.raw"
}
ime_shown() { { ash dumpsys input_method | grep -m1 -o 'mInputShown=[a-z]*' | cut -d= -f2; } || true; }
wait_ime() { # wait_ime true|false SECONDS
  local t=0
  until [ "$(ime_shown)" = "$1" ]; do
    [ "$t" -ge "$2" ] && return 1
    sleep 1
    t=$((t + 1))
  done
}
resumed() { { ash dumpsys activity activities | grep -m1 -E 'topResumedActivity=|ResumedActivity:'; } || true; }
app_pid() { ash pidof "$PKG" || true; }
cur_size() { { ash wm size | grep -oE '[0-9]+x[0-9]+' | tail -1; } || true; }
rotation() { { ash dumpsys window displays | grep -m1 -oE 'mRotation=[0-9]|rotation=[0-9]' | grep -oE '[0-9]$'; } || true; }
launch() { a shell am start -W -n "$PKG/$ACTIVITY" | tr -d '\r'; }
crashes_since() { since "$1" | grep -E 'FATAL EXCEPTION|Fatal signal|SPIKE FAIL panic|panicked at|RecreationAttempt' | head -5 || true; }
tap_frac() {
  local wh
  wh=$(cur_size)
  a shell input tap "$(awk -v f="$1" -v w="${wh%x*}" 'BEGIN{printf "%d", f*w}')" \
    "$(awk -v f="$2" -v h="${wh#*x}" 'BEGIN{printf "%d", f*h}')"
}
# The top of the app's window on the screen, in pixels (0 if not found).
win_top() {
  { ash dumpsys window windows | awk -v p="$PKG" '
      index($0, p) { f = 1 }
      f && match($0, /[fF]rame=\[[0-9]+,[0-9]+\]/) {
        s = substr($0, RSTART, RLENGTH); gsub(/[^0-9,]/, "", s); split(s, x, ","); print x[2]; exit }'; } || true
}
# Records every PASS/FAIL line the app printed since MARK, prefixed.
app_checks() { # app_checks MARK ID PREFIX
  local line name result detail
  while IFS= read -r line; do
    result=$(awk '{print $2}' <<<"$line")
    name=$(sed -E 's/^SPIKE (PASS|FAIL) ([^:]+):.*/\2/' <<<"$line")
    detail=$(sed -E 's/^SPIKE (PASS|FAIL) [^:]+: ?//' <<<"$line")
    record "$2" "$3$name" "$result" "${detail:0:300}"
  done < <(spike_since "$1" | grep -E '^SPIKE (PASS|FAIL) ' || true)
}

# --------------------------------------------------------------- checklist
checklist() {
  : >"$OUT/results.tsv"
  local m l s pid pid2 top scale fy

  group "A1, A2 install and launch"
  if a install -r "$OUT/spike-t34.apk" >"$OUT/install.txt" 2>&1 && grep -q Success "$OUT/install.txt"; then
    record A1 install PASS "$(ash pm path "$PKG")"
  else
    record A1 install FAIL "$(tail -3 "$OUT/install.txt" | xargs)"
    return 1
  fi
  ash setprop debug.tawara.spike 1
  ash setprop debug.tawara.spike.backend "''" || true
  m=$(mark)
  M1=$m
  launch | tee "$OUT/am-start.txt"
  if grep -q 'Status: ok' "$OUT/am-start.txt" && wait_log "$m" '^SPIKE INFO start ' 30 >/dev/null; then
    record A2 launch PASS "$(grep -E 'LaunchState|TotalTime' "$OUT/am-start.txt" | xargs)"
  else
    record A2 launch FAIL "$(crashes_since "$m" | xargs) $(since "$m" | grep -E 'Unable to (find|load)|dlopen' | head -2 | xargs)"
  fi
  endgroup

  # Typing first: the app gives CI a 10 s window after it focuses the field.
  group "A6, A7 typing and Return (key events)"
  local m2
  if wait_log "$m" 'focus requested' 45 >/dev/null; then
    m2=$(mark)
    ash input text hunter2
    if wait_log "$m2" '^SPIKE INFO pw_len=7$' 6 >/dev/null; then record A6 typing-key-events PASS "pw_len=7"; else
      record A6 typing-key-events FAIL "$(spike_since "$m2" | grep -E 'pw_len' | tail -1)"
    fi
    ime_after_typing=$(ime_shown)
    m2=$(mark)
    ash input keyevent KEYCODE_ENTER
    if wait_log "$m2" '^SPIKE INFO submit pw_len=7$' 3 >/dev/null; then record A7 return-submits PASS "submit pw_len=7"; else
      record A7 return-submits FAIL "$(spike_since "$m2" | grep -E 'submit|pw_len' | tail -2 | xargs)"
    fi
  else
    record A6 typing-key-events FAIL "no 'focus requested' line; $(crashes_since "$m" | xargs)"
  fi
  endgroup

  group "A3-A5 first frame, renderer, soft keyboard"
  if l=$(wait_log "$m" '^SPIKE (PASS|FAIL) render.screenshot_not_blank' 30); then
    record A3 first-frame "$(awk '{print $2}' <<<"$l")" "${l#*: }"
  else
    record A3 first-frame FAIL "no in-app screenshot; $(crashes_since "$m" | xargs)"
  fi
  record A3 screen INFO "w h fmt sha colours nonbg-permille: $(shot 01-focused)"
  l=$(spike_since "$m" | grep -E 'iced_wgpu.*(Selected|Available adapters)' | head -2 | xargs || true)
  record A4 renderer INFO "${l:-no iced_wgpu adapter line (tiny-skia?)}; $(spike_since "$m" | grep -m1 'ICED_BACKEND=' || true)"
  if [ "${ime_after_typing:-}" = true ] || wait_ime true 3; then
    record A5 keyboard-on-focus PASS "mInputShown=true"
  else
    record A5 keyboard-on-focus FAIL "$(ash dumpsys input_method | grep -E 'mInputShown|mServedView|mCurrentFocusedWindow' | head -3 | xargs)"
  fi
  shot 02-keyboard >/dev/null
  endgroup

  group "A9 keyboard on blur and refocus"
  if wait_log "$m" 'field hidden' 40 >/dev/null; then
    if wait_ime false 3; then record A9 keyboard-hides-on-blur PASS ""; else record A9 keyboard-hides-on-blur FAIL "mInputShown=$(ime_shown)"; fi
    if wait_log "$m" 'field shown' 6 >/dev/null && wait_ime true 3; then
      record A9 keyboard-on-refocus PASS ""
    else
      record A9 keyboard-on-refocus FAIL "mInputShown=$(ime_shown)"
    fi
  else
    record A9 keyboard-hides-on-blur FAIL "no 'field hidden' line"
  fi
  endgroup

  group "A8, A12 the app's own checks (task, clipboard, library, TLS)"
  if wait_log "$m" '^SPIKE DONE ' 120 >/dev/null; then
    app_checks "$m" A8 "app."
    record A8 iced-clipboard INFO "$(spike_since "$m" | grep -m1 'iced clipboard' || true)"
  else
    app_checks "$m" A8 "app."
    record A8 self-test FAIL "no SPIKE DONE within 120 s; $(crashes_since "$m" | xargs)"
  fi
  endgroup

  group "A10 re-show the keyboard after the user dismisses it (expected FAIL)"
  if wait_log "$m" 'READY lifecycle' 10 >/dev/null; then
    wait_ime true 3 || true
    ash input keyevent KEYCODE_BACK # with the keyboard up, Back goes to the keyboard
    wait_ime false 3 || true
    top=$(win_top)
    l=$(spike_since "$m" | grep 'layout logical' | tail -1)
    scale=$(sed -nE 's/.* scale ([0-9.]+):.*/\1/p' <<<"$l")
    fy=$(sed -nE 's/.*field centre \([0-9.]+,([0-9.]+)\).*/\1/p' <<<"$l")
    local wh x y
    wh=$(cur_size)
    x=$((${wh%x*} / 2))
    y=$(awk -v t="${top:-0}" -v s="${scale:-2.625}" -v f="${fy:-100}" 'BEGIN{printf "%d", t + s*f}')
    a shell input tap "$x" "$y"
    if wait_ime true 3; then record A10 keyboard-reshown PASS "tap at $x,$y (window top ${top:-?})"; else
      record A10 keyboard-reshown FAIL "tap at $x,$y (window top ${top:-?}); mInputShown=$(ime_shown)"
    fi
    shot 03-after-field-tap >/dev/null
  else
    record A10 keyboard-reshown FAIL "no READY lifecycle line"
  fi
  endgroup

  group "A11 touch"
  local m3
  m3=$(mark)
  wait_ime false 1 || ash input keyevent KEYCODE_BACK
  sleep 1
  tap_frac 0.5 0.85
  if wait_log "$m3" 'touch target n=' 5 >/dev/null; then record A11 touch-target PASS "$(spike_since "$m3" | grep -c 'touch finger') finger press(es)"; else
    record A11 touch-target FAIL "$(spike_since "$m3" | grep -E 'touch' | head -2 | xargs)"
  fi
  endgroup

  group "A12 scrolling"
  m3=$(mark)
  local wh
  wh=$(cur_size)
  # A slow swipe up inside the row list (the lower part of the window).
  a shell input swipe "$((${wh%x*} / 2))" "$((${wh#*x} * 9 / 10))" "$((${wh%x*} / 2))" "$((${wh#*x} * 6 / 10))" 600
  sleep 1
  if l=$(wait_log "$m3" 'scroll offset y=' 4); then record A12 scroll PASS "$l"; else
    record A12 scroll FAIL "no scroll offset line; $(spike_since "$m3" | grep -E 'touch' | head -2 | xargs)"
  fi
  shot 03b-scrolled >/dev/null
  endgroup

  group "A13 background and return during a task"
  pid=$(app_pid)
  m3=$(mark)
  ash input keyevent KEYCODE_HOME
  sleep 3
  record A13 home INFO "$(resumed)"
  shot 04-home >/dev/null
  launch >"$OUT/am-start-2.txt"
  sleep 3
  pid2=$(app_pid)
  s=$(shot 05-returned)
  l=$(crashes_since "$m3")
  if [ -n "$pid" ] && [ "$pid" = "$pid2" ] && [ -z "$l" ] && wait_log "$m3" 'frame after gap' 5 >/dev/null; then
    record A13 resume PASS "same pid $pid; $(spike_since "$m3" | grep -m1 'frame after gap'); screen: $s"
  else
    record A13 resume FAIL "pid $pid -> $pid2; $(spike_since "$m3" | grep -m1 'frame after gap' || echo 'no frame after gap'); $l; screen: $s"
  fi
  endgroup

  group "A14 Back"
  pid=$(app_pid)
  m3=$(mark)
  wait_ime false 1 || { ash input keyevent KEYCODE_BACK; sleep 1; }
  ash input keyevent KEYCODE_BACK
  sleep 2
  if wait_log "$m3" 'back pressed' 3 >/dev/null; then
    record A14 back-delivered PASS "now: $(resumed)"
  else
    record A14 back-delivered FAIL "now: $(resumed)"
  fi
  launch >/dev/null
  sleep 2
  pid2=$(app_pid)
  [ -n "$pid" ] && [ "$pid" = "$pid2" ] && record A14 back-same-process PASS "pid $pid" \
    || record A14 back-same-process FAIL "pid $pid -> $pid2; $(crashes_since "$m3" | xargs)"
  endgroup

  group "A15 rotation"
  pid=$(app_pid)
  m3=$(mark)
  ash settings put system accelerometer_rotation 0
  ash settings put system user_rotation 1
  sleep 4
  s=$(shot 06-landscape)
  pid2=$(app_pid)
  l=$(spike_since "$m3" | grep -m1 'window event Resized' || true)
  if [ "$(rotation)" = 1 ] && [ "$pid" = "$pid2" ] && [ -n "$l" ]; then
    record A15 rotation PASS "$l; screen: $s"
  else
    record A15 rotation FAIL "rotation=$(rotation) pid $pid -> $pid2; ${l:-no Resized}; $(crashes_since "$m3" | xargs)"
  fi
  ash settings put system user_rotation 0
  sleep 3
  endgroup

  group "A16 density change"
  pid=$(app_pid)
  m3=$(mark)
  ash wm density 160
  sleep 4
  shot 07-density160 >/dev/null
  pid2=$(app_pid)
  l=$(spike_since "$m3" | grep -m1 -E 'window event (Rescaled|Resized)' || true)
  if [ "$pid" = "$pid2" ] && [ -n "$l" ]; then record A16 density PASS "$l"; else
    record A16 density FAIL "pid $pid -> $pid2; ${l:-no Rescaled or Resized}; $(crashes_since "$m3" | xargs)"
  fi
  ash wm density reset
  sleep 3
  endgroup

  group "A19 store files on the device"
  local uid modes
  uid=$(ash run-as "$PKG" id -u 2>/dev/null || true)
  modes=$(ash run-as "$PKG" sh -c 'stat -c "%a %u %n" no_backup/tawara-spike/run-*/store-a no_backup/tawara-spike/run-*/store-a/* 2>&1' || true)
  echo "$modes"
  if [ -n "$uid" ] && grep -qE "^700 $uid .*store-a$" <<<"$modes" && grep -qE "^600 $uid .*accounts.mks$" <<<"$modes" \
    && ! grep -vE "^(700|600) $uid " <<<"$modes" | grep -q .; then
    record A19 store-modes PASS "uid $uid; $(wc -l <<<"$modes") entries 0700/0600"
  else
    record A19 store-modes FAIL "uid ${uid:-?}; $(head -4 <<<"$modes" | xargs)"
  fi
  record A19 private-dirs INFO "$(ash run-as "$PKG" sh -c 'stat -c "%a %n" . files no_backup' 2>&1 | xargs)"
  endgroup

  group "A17 process death and cold start"
  ash input keyevent KEYCODE_HOME
  sleep 1
  ash am force-stop "$PKG"
  sleep 1
  m3=$(mark)
  launch | tee "$OUT/am-start-cold.txt"
  if grep -q 'LaunchState: COLD' "$OUT/am-start-cold.txt" && wait_log "$m3" '^SPIKE (PASS|FAIL) render.screenshot_not_blank' 60 >/dev/null; then
    record A17 cold-start PASS "$(grep -E 'TotalTime' "$OUT/am-start-cold.txt" | xargs)"
  else
    record A17 cold-start FAIL "$(grep -E 'LaunchState' "$OUT/am-start-cold.txt" | xargs); $(crashes_since "$m3" | xargs)"
  fi
  endgroup

  group "A18 activity destroyed, process kept (expected FAIL)"
  ash settings put global always_finish_activities 1
  ash input keyevent KEYCODE_HOME
  sleep 3
  A18_START=$(mark)
  launch >/dev/null
  if wait_log "$A18_START" '^SPIKE (PASS|FAIL) render.screenshot_not_blank' 30 >/dev/null; then
    record A18 activity-recreated PASS "$(spike_since "$A18_START" | grep -m1 'start os=' || true)"
  else
    record A18 activity-recreated FAIL "$(crashes_since "$A18_START" | xargs)"
  fi
  A18_END=$(mark)
  ash settings put global always_finish_activities 0
  ash am force-stop "$PKG"
  endgroup

  group "A20 tiny-skia forced"
  ash setprop debug.tawara.spike.backend tiny-skia
  m3=$(mark)
  launch >/dev/null
  if wait_log "$m3" '^SPIKE DONE ' 150 >/dev/null; then
    app_checks "$m3" A20 "tiny-skia."
    shot 08-tiny-skia >/dev/null
  else
    app_checks "$m3" A20 "tiny-skia."
    record A20 tiny-skia FAIL "no SPIKE DONE; $(crashes_since "$m3" | xargs)"
  fi
  ash setprop debug.tawara.spike.backend "''" || true
  ash am force-stop "$PKG"
  endgroup

  group "A21 targetSdk 36: edge-to-edge and Back"
  if a install -r "$OUT/spike-t36.apk" >"$OUT/install36.txt" 2>&1 && grep -q Success "$OUT/install36.txt"; then
    m3=$(mark)
    launch >/dev/null
    wait_log "$m3" '^SPIKE (PASS|FAIL) render.screenshot_not_blank' 60 >/dev/null || true
    sleep 1
    shot 09-target36 >/dev/null
    record A21 layout INFO "$(spike_since "$m3" | grep 'layout logical' | tail -1); window top $(win_top)"
    wait_ime false 1 || { ash input keyevent KEYCODE_BACK; sleep 1; }
    local m4
    m4=$(mark)
    ash input keyevent KEYCODE_BACK
    sleep 2
    record A21 back INFO "$(spike_since "$m4" | grep -m1 'back pressed' || echo 'no back pressed line'); now: $(resumed)"
    ash am force-stop "$PKG"
  else
    record A21 install FAIL "$(tail -3 "$OUT/install36.txt" | xargs)"
  fi
  endgroup

  group "A22 the arm64 library"
  if ash getprop ro.product.cpu.abilist | grep -q arm64-v8a; then
    a uninstall "$PKG" >/dev/null 2>&1 || true
    if a install --abi arm64-v8a "$OUT/spike-t34.apk" >"$OUT/install-arm.txt" 2>&1 && grep -q Success "$OUT/install-arm.txt"; then
      m3=$(mark)
      launch >/dev/null
      if wait_log "$m3" '^SPIKE DONE ' 240 >/dev/null; then
        record A22 arm64-start INFO "$(spike_since "$m3" | grep -m1 'start os=' || true)"
        app_checks "$m3" A22 "arm64."
      else
        record A22 arm64 FAIL "no SPIKE DONE within 240 s; $(spike_since "$m3" | grep -m1 'start os=' || true) $(crashes_since "$m3" | xargs)"
      fi
      ash am force-stop "$PKG"
    else
      record A22 arm64-install FAIL "$(tail -3 "$OUT/install-arm.txt" | xargs)"
    fi
  else
    record A22 arm64 SKIP "the image runs no arm64 code: $(ash getprop ro.product.cpu.abilist)"
  fi
  endgroup

  group "A23 crashes"
  a logcat -d -b crash >"$OUT/logcat-crash.txt" 2>&1 || true
  local before after
  before=$(head -n "${A18_START:-0}" "$LOG" | tail -n +"$((M1 + 1))" | grep -E 'FATAL EXCEPTION|Fatal signal|SPIKE FAIL panic|panicked at' | head -5 || true)
  after=$(since "${A18_END:-0}" | grep -E 'FATAL EXCEPTION|Fatal signal|SPIKE FAIL panic|panicked at' | head -5 || true)
  if [ -z "$before$after" ]; then record A23 crashes PASS "none outside A18"; else record A23 crashes FAIL "$(echo "$before $after" | xargs)"; fi
  endgroup
}

evidence() {
  # Screenshots first and the results table last: a job log is read from
  # its end.
  for raw in "$OUT"/raw/*.raw; do
    [ -f "$raw" ] || continue
    local n
    n=$(basename "$raw" .raw)
    python3 "$HERE/shot.py" thumb "$raw" "$OUT/shots/$n.png" 4
    echo "-----BEGIN SHOT $n-----"
    base64 -w 76 "$OUT/shots/$n.png"
    echo "-----END SHOT $n-----"
  done
  group "logcat around the app (errors and the activity manager)"
  grep -E "AndroidRuntime|DEBUG|libc|ActivityTaskManager.*$PKG|InputMethodManagerService|android_activity|winit|wgpu|$TAG: (thread|note|stack)" "$LOG" | tail -200 || true
  endgroup
  group "app log (SPIKE lines, all launches)"
  grep -E "$TAG: SPIKE " "$LOG" | sed -E "s/.*$TAG: //" | grep -vE '^SPIKE INFO pw_len=[0-6]$' || true
  endgroup
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    {
      echo "### Android emulator checklist"
      echo "| id | check | result | detail |"
      echo "|---|---|---|---|"
      awk -F'\t' '{gsub(/\|/,"/",$4); printf "| %s | %s | %s | %s |\n", $1, $2, $3, substr($4,1,200)}' "$OUT/results.tsv"
    } >>"$GITHUB_STEP_SUMMARY"
  fi
  echo "===== results ====="
  awk -F'\t' '{printf "CHECK %s %s %s: %s\n", $3, $1, $2, $4}' "$OUT/results.tsv"
  local fatal
  fatal=$(awk -F'\t' -v e="$EXPECTED_FAIL" '$3=="FAIL" && index(e, " " $1 " ")==0' "$OUT/results.tsv")
  if [ -n "$fatal" ]; then
    echo "::error::Android checklist failures outside the expected ones:"
    echo "$fatal"
    return 1
  fi
}

case ${1:-} in
  build) build ;;
  apk) apk ;;
  emulator)
    trap emulator_down EXIT
    emulator_up
    status=0
    checklist || status=$?
    evidence || status=1
    exit "$status"
    ;;
  *)
    echo "usage: $0 build|apk|emulator" >&2
    exit 2
    ;;
esac
