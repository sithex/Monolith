#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="$ROOT/island-tauri/src-tauri/target/release/hermes-island"
if [[ ! -x "$BINARY" ]]; then
  printf 'Tauri application is not built. Run: %s/island-tauri/build.sh\n' "$ROOT" >&2
  exit 1
fi
# WebKitGTK falls back to a 60 Hz timer when it cannot match the monitor to a DRM CRTC
# (physical-size mismatch); this shim fixes the match so animations run at the real refresh.
VBLANK_FIX="$ROOT/island-tauri/native/vblank-fix/libvblank_fix.so"
if [[ -f "$VBLANK_FIX" ]]; then export LD_PRELOAD="$VBLANK_FIX${LD_PRELOAD:+:$LD_PRELOAD}"; fi
unset NODE_OPTIONS ELECTRON_RUN_AS_NODE
cd -- "$ROOT"
# Harness stays the CLI default. Only overlay launches without an explicit page select island.
overlay=0
ui=0
for arg in "$@"; do
  [[ "$arg" != --overlay ]] || overlay=1
  case "$arg" in --ui|--ui=*) ui=1 ;; esac
done
if [[ "$overlay" == 1 && "$ui" == 0 ]]; then
  set -- "$@" --ui island
fi
# Display server (Settings → General → Display server, Linux only). The overlay island
# defaults to native Wayland in a Wayland session; X11 means GDK on XWayland. Every other
# launch (preview/harness) stays on X11 as before. HERMES_ISLAND_GDK_BACKEND overrides.
backend=x11
if [[ "$overlay" == 1 && -n "${WAYLAND_DISPLAY:-}" ]]; then
  backend=$(/usr/bin/python3 -c 'import json,sys
try: v=json.load(open(sys.argv[1])).get("displayBackend")
except Exception: v=None
print(v if v in ("wayland","x11") else "wayland")' "$ROOT/.island-data-overlay/settings.json" 2>/dev/null || echo wayland)
fi
export GDK_BACKEND="${HERMES_ISLAND_GDK_BACKEND:-$backend}"
# Detached overlay launches (autostart, panel, tray restart) keep their log in the data dir
# (tray → View log); the previous run's log is kept as monolith.log.1.
if [[ "$overlay" == 1 && ! -t 2 && -d "$ROOT/.island-data-overlay" ]]; then
  log="$ROOT/.island-data-overlay/monolith.log"
  [[ -f "$log" ]] && mv -f -- "$log" "$log.1"
  exec 2>>"$log"
fi

# --- Automatic rollback (overlay launches only) ------------------------------------------
# build.sh replaces target/release/hermes-island; a broken build must not leave Nick without
# an island. An overlay launch is therefore supervised: if the new binary dies early with a
# failure status, or never publishes a status.json carrying its own pid, the saved
# `hermes-island.last-good` runs instead (once: never a loop). A binary that has stayed
# healthy for GOOD_AFTER seconds becomes the new last-good. Harness/test launches (no
# --overlay, --test-instance, --data-dir) and launches while an island is already alive keep
# the plain `exec`. HERMES_ISLAND_NO_SUPERVISOR=1 disables this; the *_SECS variables exist
# for the shell tests (tests/run_rollback_test.sh).
DATA="$ROOT/.island-data-overlay"
supervise=0
if [[ "$overlay" == 1 && -d "$DATA" && -z "${HERMES_ISLAND_NO_SUPERVISOR:-}" ]]; then
  supervise=1
  for arg in "$@"; do
    case "$arg" in --test-instance | --data-dir | --data-dir=*) supervise=0 ;; esac
  done
fi
status_pid() { # pid recorded in status.json; empty when unreadable
  [[ -r "$DATA/status.json" ]] || return 0
  { grep -o '"pid": *[0-9]*' -- "$DATA/status.json" | head -n1 | tr -dc '0-9'; } 2>/dev/null || true
}
status_age() { # seconds since status.json was written; huge when missing
  local m
  m=$(stat -c %Y -- "$DATA/status.json" 2>/dev/null) || { echo 999999; return 0; }
  echo $(($(date +%s) - m))
}
if [[ "$supervise" == 1 ]]; then
  # An island that is alive owns the data dir: a second launch just loses the single-instance
  # race, so there is nothing to roll back.
  other=$(status_pid)
  if [[ -n "$other" && -d "/proc/$other" && "$(status_age)" -lt 30 ]]; then supervise=0; fi
fi
[[ "$supervise" == 1 ]] || exec "$BINARY" "$@"

EARLY="${HERMES_ISLAND_EARLY_EXIT_SECS:-10}"
GRACE="${HERMES_ISLAND_STATUS_GRACE_SECS:-20}"
GOOD_AFTER="${HERMES_ISLAND_GOOD_AFTER_SECS:-30}"
LASTGOOD="$BINARY.last-good"
ARGS=("$@")
child=0
stopping=0
log() { printf '[run.sh %s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
forward() { stopping=1; if [[ "$child" -gt 0 ]]; then kill -TERM "$child" 2>/dev/null || true; fi; }
trap forward TERM INT HUP

mark_last_good() {
  # Only the file that was launched may become last-good: build.sh can replace $BINARY while
  # this older island is still up (the device/inode/size/mtime signature then differs).
  [[ "$(stat -c '%d:%i:%s:%Y' -- "$BINARY" 2>/dev/null)" == "$1" ]] || return 0
  cmp -s -- "$BINARY" "$LASTGOOD" 2>/dev/null && return 0
  if cp -- "$BINARY" "$LASTGOOD.new.$$" && chmod 755 -- "$LASTGOOD.new.$$" \
    && mv -f -- "$LASTGOOD.new.$$" "$LASTGOOD"; then
    log "healthy for ${GOOD_AFTER}s: saved $(basename -- "$LASTGOOD")"
  else
    rm -f -- "$LASTGOOD.new.$$"
    log "could not save $(basename -- "$LASTGOOD")"
  fi
}

# watch <binary> <primary|fallback>: runs it to completion; sets code, why, ran (seconds).
watch() {
  local bin="$1" role="$2" start healthy=0 settled=0 reaped=0 rc
  why=""
  launched_sig=$(stat -c '%d:%i:%s:%Y' -- "$bin" 2>/dev/null || true)
  "$bin" "${ARGS[@]}" &
  child=$!
  start=$SECONDS
  while kill -0 "$child" 2>/dev/null; do
    if [[ "$role" == fallback || "$settled" == 1 ]]; then
      rc=0
      wait "$child" 2>/dev/null || rc=$? # a trapped signal also ends wait: re-check below
      if ! kill -0 "$child" 2>/dev/null; then code=$rc; reaped=1; break; fi
      continue
    fi
    sleep 0.2
    ran=$((SECONDS - start))
    if [[ "$healthy" == 0 && "$(status_pid)" == "$child" ]]; then healthy=1; fi
    if [[ "$healthy" == 1 && "$ran" -ge "$GOOD_AFTER" && "$(status_age)" -le 10 ]]; then
      mark_last_good "$launched_sig"
      settled=1
    elif [[ "$healthy" == 0 && "$ran" -ge "$GRACE" && "$stopping" == 0 ]]; then
      why="no status.json carrying pid $child after ${GRACE}s"
      kill -TERM "$child" 2>/dev/null || true
      for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25; do
        kill -0 "$child" 2>/dev/null || break
        sleep 0.2
      done
      kill -KILL "$child" 2>/dev/null || true
    fi
  done
  if [[ "$reaped" == 0 ]]; then
    code=0
    wait "$child" 2>/dev/null || code=$?
  fi
  ran=$((SECONDS - start))
  child=0
}

code=0 ran=0 why=""
watch "$BINARY" primary
failed=0
if [[ -n "$why" ]]; then
  failed=1
elif [[ "$stopping" == 0 && "$ran" -lt "$EARLY" ]]; then
  case "$code" in 0 | 129 | 130 | 143) ;; *) failed=1; why="exited with status $code after ${ran}s" ;; esac
fi
if [[ "$failed" == 1 ]]; then
  if [[ -x "$LASTGOOD" ]] && ! cmp -s -- "$BINARY" "$LASTGOOD"; then
    log "new binary failed ($why); rolling back to $(basename -- "$LASTGOOD")"
    watch "$LASTGOOD" fallback
    log "rolled-back binary exited with status $code"
  else
    log "new binary failed ($why); no different last-good binary to roll back to"
  fi
fi
exit "$code"
