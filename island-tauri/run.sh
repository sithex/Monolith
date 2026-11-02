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
exec "$BINARY" "$@"
