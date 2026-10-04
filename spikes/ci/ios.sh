#!/usr/bin/env bash
# iOS spike on a GitHub-hosted macos-26 runner, plain commands only:
#
#   ci/ios.sh device      # build and link for aarch64-apple-ios (installing
#                         # on a device needs signing; see spikes/README.md)
#   ci/ios.sh simulator   # build, bundle by hand, sign ad hoc, boot a
#                         # simulator, run the checklist, print the evidence
#
# The app's `SPIKE ...` lines come from its stderr, which `simctl launch
# --stderr` writes to a file. This script prints `CHECK ...` lines, a table
# in the step summary, and screenshots as base64 JPEGs between markers. The
# checklist IDs are those of spikes/README.md.
set -Eeuo pipefail

BIN=tawara-spike
BUNDLE=com.patricksmithlaravel.tawara.spike
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
SPIKES=$(cd "$HERE/.." && pwd)
OUT=${RUNNER_TEMP:-/tmp}/ios
export IPHONEOS_DEPLOYMENT_TARGET=${IPHONEOS_DEPLOYMENT_TARGET:-16.0}
mkdir -p "$OUT/shots"
: >"$OUT/results.tsv"

group() { echo "::group::$*"; }
endgroup() { echo "::endgroup::"; }
die() { echo "::error::$*"; exit 1; }
record() { # record ID NAME RESULT DETAIL
  printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "${4:-}" >>"$OUT/results.tsv"
  echo "CHECK $3 $1 $2: ${4:-}"
}
platform_of() { otool -l "$1" | awk '/LC_BUILD_VERSION/{f=1} f&&/platform/{print $2; exit}'; }

device() {
  group "toolchain"
  xcodebuild -version
  xcrun --sdk iphoneos --show-sdk-version
  endgroup
  cd "$SPIKES"
  group "cargo build --target aarch64-apple-ios"
  cargo build --locked --release --target aarch64-apple-ios --bin "$BIN"
  endgroup
  local exe=target/aarch64-apple-ios/release/$BIN
  file "$exe"
  otool -l "$exe" | grep -A4 LC_BUILD_VERSION
  # LC_BUILD_VERSION platform 2 is iOS (a device).
  if [ "$(platform_of "$exe")" = 2 ]; then record D1 device-build PASS "$(file -b "$exe")"; else
    record D1 device-build FAIL "platform $(platform_of "$exe")"
  fi
  summary
  ! grep -q $'\tFAIL\t' "$OUT/results.tsv"
}

# --------------------------------------------------------------- simulator
wait_line() { # wait_line FILE ERE SECONDS: prints the first match
  local t=0 line
  while :; do
    line=$(grep -E -m1 "$2" "$1" 2>/dev/null || true)
    [ -n "$line" ] && { echo "$line"; return 0; }
    [ "$t" -ge "$3" ] && return 1
    sleep 1
    t=$((t + 1))
  done
}
wait_after() { # wait_after FILE LINE ERE SECONDS: the first match after line LINE
  local t=0 line
  while :; do
    line=$(tail -n +"$(($2 + 1))" "$1" 2>/dev/null | grep -E -m1 "$3" || true)
    [ -n "$line" ] && { echo "$line"; return 0; }
    [ "$t" -ge "$4" ] && return 1
    sleep 1
    t=$((t + 1))
  done
}
shot() { xcrun simctl io "$UDID" screenshot --type=png "$OUT/shots/$1.png" >/dev/null 2>&1 || true; }
app_checks() { # app_checks FILE ID PREFIX
  local line name result detail
  while IFS= read -r line; do
    result=$(awk '{print $2}' <<<"$line")
    name=$(sed -E 's/^SPIKE (PASS|FAIL) ([^:]+):.*/\2/' <<<"$line")
    detail=$(sed -E 's/^SPIKE (PASS|FAIL) [^:]+: ?//' <<<"$line")
    record "$2" "$3$name" "$result" "${detail:0:300}"
  done < <(grep -E '^SPIKE (PASS|FAIL) ' "$1" || true)
}
launch() { # launch OUT_PREFIX [extra simctl launch arguments]: prints the pid
  local prefix=$1
  shift
  SIMCTL_CHILD_SPIKE_SELFTEST=1 SIMCTL_CHILD_RUST_BACKTRACE=1 xcrun simctl launch "$@" \
    --stdout="$OUT/$prefix.out" --stderr="$OUT/$prefix.err" "$UDID" "$BUNDLE" | awk '{print $NF}'
}

simulator() {
  group "toolchain"
  xcodebuild -version
  xcrun --sdk iphonesimulator --show-sdk-version
  endgroup
  cd "$SPIKES"
  group "cargo build --target aarch64-apple-ios-sim"
  cargo build --locked --release --target aarch64-apple-ios-sim --bin "$BIN"
  endgroup
  local exe=target/aarch64-apple-ios-sim/release/$BIN
  # LC_BUILD_VERSION platform 7 is the iOS simulator.
  [ "$(platform_of "$exe")" = 7 ] || die "$exe: platform $(platform_of "$exe"), expected 7"

  group "bundle and sign"
  APP=$OUT/TawaraSpike.app
  rm -rf "$APP"
  mkdir -p "$APP"
  cp "$exe" "$APP/$BIN"
  sed -e 's/@PLATFORM@/iPhoneSimulator/' -e "s/@MINOS@/$IPHONEOS_DEPLOYMENT_TARGET/" \
    platform/ios/Info.plist.in >"$APP/Info.plist"
  plutil -lint "$APP/Info.plist"
  codesign --force --sign - --timestamp=none "$APP"
  codesign --verify --verbose=2 "$APP"
  endgroup

  group "simulator"
  local runtime devtype
  read -r runtime devtype < <(python3 - <<'PY'
import json, subprocess
rts = json.loads(subprocess.check_output(["xcrun", "simctl", "list", "-j", "runtimes", "available"]))["runtimes"]
ios = [r for r in rts if r.get("platform") == "iOS"]
rt = max(ios, key=lambda r: tuple(int(x) for x in r["version"].split(".")))
phones = [d for d in rt.get("supportedDeviceTypes", []) if d.get("productFamily") == "iPhone" or d["name"].startswith("iPhone")]
named = [d for d in phones if d["name"] == "iPhone 17"]
print(rt["identifier"], (named or phones)[-1]["identifier"])
PY
  )
  echo "runtime $runtime; device type $devtype"
  UDID=$(xcrun simctl create tawara-spike "$devtype" "$runtime")
  echo "udid $UDID"
  # The software keyboard: Simulator's preference, written before boot.
  defaults write com.apple.iphonesimulator ConnectHardwareKeyboard -bool false
  local p=$HOME/Library/Preferences/com.apple.iphonesimulator.plist
  /usr/libexec/PlistBuddy -c "Add :DevicePreferences dict" "$p" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Add :DevicePreferences:$UDID dict" "$p" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Add :DevicePreferences:$UDID:ConnectHardwareKeyboard bool false" "$p" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Set :DevicePreferences:$UDID:ConnectHardwareKeyboard false" "$p"
  xcrun simctl boot "$UDID"
  xcrun simctl bootstatus "$UDID" -b
  xcrun simctl install "$UDID" "$APP"
  endgroup

  xcrun simctl spawn "$UDID" log stream --style compact --level debug \
    --predicate "process == \"$BIN\"" >"$OUT/os.log" 2>&1 &
  LOGPID=$!
  trap 'kill $LOGPID 2>/dev/null || true; xcrun simctl shutdown "$UDID" >/dev/null 2>&1 || true' EXIT
  checklist || true
  evidence
}

checklist() {
  local err=$OUT/app.err pid pid2 l
  group "I1-I5, I8, I9 the self-test"
  pid=$(launch app --terminate-running-process)
  echo "launched pid $pid"
  if wait_line "$err" 'focus requested' 60 >/dev/null; then
    sleep 1
    shot 01-focused
  fi
  if wait_line "$err" 'ios insertText hook ran' 15 >/dev/null; then
    sleep 1
    shot 02-typed
  fi
  if wait_line "$err" 'iced clipboard roundtrip' 60 >/dev/null; then
    # simctl reaches the simulator's pasteboard through a host service that
    # can fail on a busy host: an error from the tool is tried again (up to
    # three attempts, each counted in the result), a wrong value is not.
    local try rc
    for try in 1 2 3; do
      l=$(xcrun simctl pbpaste "$UDID" 2>&1) && rc=0 || rc=$?
      [ "$rc" -eq 0 ] && break
      sleep 2
    done
    if [ "$rc" -eq 0 ] && [ "$l" = tawara-spike-clip ]; then
      record I5 pasteboard-from-outside PASS "simctl pbpaste: $l (attempt $try)"
    else
      record I5 pasteboard-from-outside FAIL "simctl pbpaste, attempt $try, status $rc: $(tr -s '\n' ' ' <<<"$l" | cut -c1-300)"
    fi
  else
    record I5 pasteboard-from-outside FAIL "no clipboard step"
  fi
  if wait_line "$err" 'orientation request landscape=true' 90 >/dev/null; then
    sleep 2
    shot 03-landscape
    l=$(grep -E 'window event Resized' "$err" | tail -1 || true)
    record I9 rotation INFO "${l:-no Resized after the orientation request}"
  fi
  if wait_line "$err" '^SPIKE DONE ' 180 >/dev/null; then
    l=$(grep -m1 '^SPIKE DONE' "$err")
    case $l in
      *" fail=0") record I1 self-test PASS "$l" ;;
      *) record I1 self-test FAIL "$l" ;;
    esac
  else
    record I1 self-test FAIL "no SPIKE DONE within 180 s"
  fi
  app_checks "$err" I1 "app."
  record I2 window INFO "$(grep 'layout logical' "$err" | head -1)"
  grep -q '^SPIKE INFO pw_len=7$' "$err" && grep -q '^SPIKE INFO submit pw_len=7$' "$err" \
    && record I3 insertText-typing PASS "pw_len=7, submit pw_len=7" \
    || record I3 insertText-typing FAIL "$(grep -E 'pw_len|insertText' "$err" | tail -3 | xargs)"
  record I4 keyboard INFO "see screenshot 01-focused"
  record I8 safe-area INFO "$(grep -m1 'safeAreaInsets' "$err" || echo 'no safeAreaInsets line')"
  record I2 renderer INFO "$(grep -E 'iced_wgpu.*(Selected|Available adapters)' "$err" | head -2 | xargs)"
  endgroup

  # The checklist runs with errexit off (`checklist || true`), so each
  # command's status is checked here. iced reports no move to the background
  # on iOS; the app's heartbeat (`frames N`, about once a second in the
  # lifecycle phase) shows whether it draws after the return.
  group "I6 background and return"
  local out front back beat gap
  if ! wait_line "$err" 'READY lifecycle' 10 >/dev/null; then
    record I6 return-same-process FAIL "no READY lifecycle line"
  elif ! out=$(xcrun simctl launch "$UDID" com.apple.Preferences 2>&1); then
    record I6 return-same-process FAIL "Settings did not launch: $(tr -s '\n' ' ' <<<"$out" | cut -c1-160)"
  else
    front=$(wc -l <"$err")
    sleep 4
    shot 04-settings-in-front
    back=$(wc -l <"$err")
    if ! out=$(xcrun simctl launch "$UDID" "$BUNDLE" 2>&1); then
      record I6 return-same-process FAIL "the app did not relaunch: $(tr -s '\n' ' ' <<<"$out" | cut -c1-160)"
    else
      pid2=$(awk 'END {print $NF}' <<<"$out")
      beat=$(wait_after "$err" "$(wc -l <"$err")" '^SPIKE INFO frames [0-9]+$' 10 || true)
      sleep 2
      shot 05-returned
      gap=$(tail -n +"$((back + 1))" "$err" | grep -m1 'frame after gap' || true)
      if [ "$pid" != "$pid2" ]; then
        record I6 return-same-process FAIL "pid $pid -> $pid2"
      elif [ -z "$beat" ]; then
        record I6 return-same-process FAIL "pid $pid; no frame within 10 s of the return"
      else
        record I6 return-same-process PASS "pid $pid; drawing after the return (${beat#SPIKE INFO }); ${gap:-no gap over 1 s}"
      fi
    fi
    record I6 frames-behind-settings INFO "$(awk -v a="$front" -v b="$back" 'NR > a && NR <= b && /^SPIKE INFO frames [0-9]+$/ {n++} END {print n + 0}' "$err") heartbeats while Settings was in front"
    record I6 background-gpu INFO "$(grep -cE 'Insufficient Permission|BackgroundExecutionNotPermitted|IOGPU|MTLCommandBuffer.*error' "$OUT/os.log" || true) Metal/background error lines in os.log"
  fi
  endgroup

  group "I7 tiny-skia forced"
  SIMCTL_CHILD_ICED_BACKEND=tiny-skia launch app-tiny-skia --terminate-running-process >/dev/null
  if wait_line "$OUT/app-tiny-skia.err" '^SPIKE DONE ' 180 >/dev/null; then
    shot 06-tiny-skia
  else
    record I7 tiny-skia FAIL "no SPIKE DONE within 180 s"
  fi
  app_checks "$OUT/app-tiny-skia.err" I7 "tiny-skia."
  xcrun simctl terminate "$UDID" "$BUNDLE" >/dev/null 2>&1 || true
  endgroup

  group "I10 crash reports"
  sleep 2
  local reports=()
  for f in ~/Library/Logs/DiagnosticReports/*"$BIN"*; do [ -e "$f" ] && reports+=("$f"); done
  if [ ${#reports[@]} -eq 0 ]; then record I10 crash-reports PASS "none"; else
    record I10 crash-reports FAIL "${reports[*]##*/}"
    for f in "${reports[@]}"; do head -80 "$f"; done
  fi
  endgroup
}

summary() {
  group "results"
  column -t -s $'\t' "$OUT/results.tsv" 2>/dev/null || cat "$OUT/results.tsv"
  endgroup
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    {
      echo "### iOS checklist"
      echo "| id | check | result | detail |"
      echo "|---|---|---|---|"
      awk -F'\t' '{gsub(/\|/,"/",$4); printf "| %s | %s | %s | %s |\n", $1, $2, $3, substr($4,1,200)}' "$OUT/results.tsv"
    } >>"$GITHUB_STEP_SUMMARY"
  fi
}

evidence() {
  # Screenshots first and the results last: a job log is read from its end.
  for f in "$OUT"/shots/*.png; do
    [ -f "$f" ] || continue
    local j=${f%.png}.jpg
    sips -Z 700 -s format jpeg "$f" --out "$j" >/dev/null
    echo "-----BEGIN SHOT $(basename "$j" .jpg)-----"
    base64 -i "$j" | fold -w 76
    echo "-----END SHOT $(basename "$j" .jpg)-----"
  done
  group "os log (errors and faults)"
  grep -iE 'error|fault|crash|terminat' "$OUT/os.log" | tail -150 || true
  endgroup
  for f in app app-tiny-skia; do
    group "app stderr ($f)"
    grep -vE '^SPIKE INFO pw_len=[0-6]$' "$OUT/$f.err" 2>/dev/null || true
    endgroup
  done
  summary
  echo "===== results ====="
  awk -F'\t' '{printf "CHECK %s %s %s: %s\n", $3, $1, $2, $4}' "$OUT/results.tsv"
  if grep -q $'\tFAIL\t' "$OUT/results.tsv"; then
    echo "::error::iOS checklist has failures"
    return 1
  fi
}

case ${1:-} in
  device) device ;;
  simulator) simulator ;;
  *)
    echo "usage: $0 device|simulator" >&2
    exit 2
    ;;
esac
