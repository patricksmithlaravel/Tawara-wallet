#!/usr/bin/env bash
# Desktop self-test on a CI runner: three launches of the spike, with iced's
# own renderer choice, with tiny-skia forced, and with wgpu forced (the last
# is informational: a runner with no GPU has no wgpu adapter, which is what
# tiny-skia is for). On Linux it runs under Xvfb, and xdotool types into the
# password field with real key events. The job log is the record: `SPIKE`
# lines, then each screenshot as base64 PNG between markers.
set -uo pipefail
cd "$TAWARA/spikes" || exit 1
BIN=target/release/tawara-spike
[ "${RUNNER_OS:-}" = Windows ] && BIN=$BIN.exe
OUT=${RUNNER_TEMP:-/tmp}/spike
rm -rf "$OUT"
mkdir -p "$OUT"
linux=false
[ "$(uname -s)" = Linux ] && linux=true

if $linux; then
  Xvfb :99 -screen 0 1280x1024x24 -nolisten tcp >"$OUT/xvfb.log" 2>&1 &
  export DISPLAY=:99
  sleep 2
fi

wait_line() { # wait_line FILE ERE SECONDS
  local t=0
  until grep -qE "$2" "$1" 2>/dev/null; do
    [ "$t" -ge "$3" ] && return 1
    sleep 1
    t=$((t + 1))
  done
}

shot() { # shot NAME FILE: the PNG as base64 between markers
  [ -s "$2" ] || return 0
  echo "-----BEGIN SHOT $1-----"
  # From standard input: macOS base64 takes no file operand (only -i).
  base64 <"$2" | tr -d '\n' | fold -w 76 || echo "::warning::screenshot $1: base64 failed"
  echo
  echo "-----END SHOT $1-----"
}

failed=0
for label in default tiny-skia wgpu; do
  data="$OUT/data-$label"
  mkdir -p "$data"
  chmod 700 "$data"
  log="$OUT/$label.log"
  echo "::group::self-test, renderer: $label"
  case $label in
    default) unset ICED_BACKEND ;;
    *) export ICED_BACKEND=$label ;;
  esac
  "$BIN" --self-test --storage "$data" --screenshot "$OUT/$label-app.png" 2>"$log" &
  pid=$!
  (sleep 300 && kill "$pid" 2>/dev/null) &
  watchdog=$!
  if $linux && wait_line "$log" "focus requested" 60; then
    sleep 1
    xdotool search --sync --name "Tawara spike" windowfocus --sync type --delay 50 'hunter2'
    sleep 1
    import -window root -crop 420x780+0+0 "$OUT/$label-screen.png" 2>/dev/null || true
    xdotool key Return
    # The mouse wheel over the row list, at the bottom of the window.
    xdotool mousemove 210 700 click --repeat 5 --delay 100 5
  fi
  wait "$pid"
  code=$?
  kill "$watchdog" 2>/dev/null
  grep -E '^SPIKE (PASS|FAIL|DONE|INFO|log)' "$log" | grep -vE '^SPIKE INFO pw_len=[0-6]$'
  echo "exit code $code"
  problems=()
  grep -q '^SPIKE FAIL' "$log" && problems+=("FAIL lines")
  grep -q '^SPIKE DONE' "$log" || problems+=("no SPIKE DONE")
  [ "$code" -eq 0 ] || problems+=("exit code $code")
  if $linux; then
    grep -q '^SPIKE INFO pw_len=7$' "$log" || problems+=("typing: no pw_len=7")
    grep -q '^SPIKE INFO submit pw_len=7$' "$log" || problems+=("Return: no submit pw_len=7")
    grep -q '^SPIKE INFO scroll offset y=' "$log" || problems+=("mouse wheel: no scroll offset")
  fi
  echo "::endgroup::"
  if [ ${#problems[@]} -eq 0 ]; then
    echo "CHECK PASS desktop.$label" | tee -a "$OUT/checks.txt"
  elif [ "$label" = wgpu ]; then
    echo "CHECK INFO desktop.$label: ${problems[*]} (informational)" | tee -a "$OUT/checks.txt"
  else
    echo "::error::desktop self-test ($label): ${problems[*]}"
    echo "CHECK FAIL desktop.$label: ${problems[*]}" | tee -a "$OUT/checks.txt"
    failed=1
  fi
  [ -n "${GITHUB_STEP_SUMMARY:-}" ] && grep -E '^SPIKE (PASS|FAIL|DONE)' "$log" | sed "s/^/    [$label] /" >>"$GITHUB_STEP_SUMMARY"
done

echo "::group::screenshots (base64 PNG)"
for f in "$OUT"/*.png; do shot "$(basename "$f" .png)" "$f"; done
echo "::endgroup::"
# The results again, last: a job log is read from its end.
echo "===== results ====="
for label in default tiny-skia wgpu; do
  grep -E '^SPIKE (PASS|FAIL|DONE)|^SPIKE INFO lib: create|unlock' "$OUT/$label.log" 2>/dev/null | sed "s/^/[$label] /"
done
grep -h '^CHECK ' "$OUT"/checks.txt 2>/dev/null
exit "$failed"
