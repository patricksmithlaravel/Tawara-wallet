#!/usr/bin/env bash
# Desktop liveness on a CI runner (docs/DECISIONS.md D32 item 9).
#
# The built desktop app is started once with wgpu and once with tiny-skia.
# Each launch passes only if the app's process is still running after a fixed
# wait and its log holds no panic. Until this check, nothing in CI started the
# desktop window, so a panic at start-up, such as a shader or surface failure,
# would have passed. The check does not look at what the window draws; the
# screenshots cover the screens' drawing, headlessly (docs/DECISIONS.md D27
# item 1). On Linux the wgpu launch uses software Vulkan (lavapipe), so it does
# not test a graphics card. The script is plain shell, as docs/DECISIONS.md D13
# asks.
#
# Usage: ci/liveness.sh TARGET_BINARY
#   LIVENESS_SECONDS  how long each launch must survive; 20 by default.

# Neither -e nor -u: a launch that fails is recorded, and the next one still
# runs. (bash 3.2, which macOS ships, also reports an empty array as unbound
# under -u, and the wrap array is empty off Linux.)
set -o pipefail

app=$1
if [ -z "$app" ] || [ ! -x "$app" ]; then
  echo "usage: ci/liveness.sh TARGET_BINARY (no executable at '$app')" >&2
  exit 2
fi
seconds=${LIVENESS_SECONDS:-20}

# Each launch is a job in a process group of its own. The group is what the
# stop below signals, so the app and the X server under it stop together.
set -m

# On Linux the app runs under xvfb-run. `-a` picks a free display number. The
# screen is named in full, 24 bits deep as the spike's Xvfb has it, rather than
# left to xvfb-run's default.
linux=false
wrap=()
if [ "$(uname -s)" = Linux ]; then
  linux=true
  wrap=(xvfb-run -a -s "-screen 0 1280x1024x24")
fi

failed=0
for renderer in wgpu tiny-skia; do
  work=$(mktemp -d) || exit 2
  home=$work/home
  mkdir -p "$home"
  log=$work/app.log
  status=$work/status

  # ICED_BACKEND names the renderer. Left unset, iced tries wgpu and falls back
  # to tiny-skia without a word (renderer/src/fallback.rs), so a wgpu launch
  # could pass on tiny-skia. Named, a wgpu that cannot start ends the app
  # instead, because the tiny-skia compositor refuses any name but its own
  # (tiny_skia/src/window/compositor.rs). Both are iced_mobile at 7325ccd, the
  # release v0.14.1-mobile.4 that docs/DECISIONS.md D32 item 9 (a) pins, and
  # are unchanged from v0.14.1-mobile.2 (d4ab3a0) and from 71f00e8, the pins
  # before it.
  env_set=(HOME="$home" XDG_DATA_HOME="$home/data" XDG_CONFIG_HOME="$home/config"
    XDG_CACHE_HOME="$home/cache" XDG_STATE_HOME="$home/state" ICED_BACKEND="$renderer")
  # On Linux the wgpu launch takes Vulkan only, so on a runner with no graphics
  # card its adapter is lavapipe and not an OpenGL device. WGPU_BACKEND is read
  # by wgpu itself.
  if $linux && [ "$renderer" = wgpu ]; then
    env_set+=(WGPU_BACKEND=vulkan)
  fi

  echo "::group::desktop, renderer $renderer"
  ( env "${env_set[@]}" "${wrap[@]}" "$app" </dev/null >"$log" 2>&1; echo $? >"$status" ) &
  pid=$!
  # Not waited for, and not reported by bash as a terminated job when the stop
  # below ends it: the status file says how the launch ended.
  disown "$pid"
  sleep "$seconds"

  # The verdict is read before the stop, because the stop makes the app exit,
  # and that writes the status file too.
  exited=
  if [ -s "$status" ]; then exited=$(cat "$status"); fi
  # $pid is the subshell around env, xvfb-run and the app, so its being alive
  # does not show that the app started. The app's own process is looked for
  # in the launch's group by its exact name, which is what the kernel records
  # (at most 15 characters on Linux); the wrappers' command lines also hold the
  # app's path, so a match on the full command line would find them instead.
  running=false
  if pgrep -g "$pid" -x "$(basename "$app")" >/dev/null; then running=true; fi

  # TERM goes to the whole group. KILL is for whatever ignores TERM, so that the
  # step cannot hang on a launch.
  kill -TERM -- "-$pid" 2>/dev/null
  sleep 2
  kill -KILL -- "-$pid" 2>/dev/null

  echo "the log:"
  cat "$log"
  echo "::endgroup::"

  if [ -n "$exited" ]; then
    echo "::error::desktop, renderer $renderer: the app exited before $seconds s, with status $exited"
    failed=1
  elif ! $running; then
    echo "::error::desktop, renderer $renderer: after $seconds s the launch is running but the app is not"
    failed=1
  # A panic on another thread, such as the wallet's worker, does not end the
  # process, so the log is read as well as the process.
  elif grep -q panicked "$log"; then
    echo "::error::desktop, renderer $renderer: the log holds a panic"
    failed=1
  else
    echo "desktop, renderer $renderer: still running after $seconds s, no panic in the log"
  fi
  rm -rf "$work"
done
exit "$failed"
