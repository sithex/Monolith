#!/usr/bin/env bash
# Run the private-compositor GUI suites (nested `kwin_wayland --virtual` + Xwayland, or Xvfb)
# one binary at a time, at low priority, and verify nothing they started survives.
#
#   scripts/run-gui-tests.sh [LOG_DIR] [suite ...]
#
# Default suites: every tests/kwin_*.rs, tests/notification_monitor.rs, tests/xvfb_shape.rs.
# Needs: built UI (`npm run build:ui` in island-tauri/), kwin_wayland, Xwayland, Xvfb, xfwm4,
# dbus-daemon, python3-gobject. The fixtures clear the inherited environment, so the live
# desktop's display, bus and HOME are never used. Extra test-binary args: GUI_TEST_ARGS="...".
set -u
here=$(cd "$(dirname "$0")/.." && pwd)
crate="$here/island-tauri/src-tauri"
logs=${1:-"$here/gui-test-logs"}
shift || true
mkdir -p "$logs"
logs=$(cd "$logs" && pwd) # absolute: the script changes directory below
cd "$crate" || exit 2
if [ $# -eq 0 ]; then
    set -- $(for f in tests/kwin_*.rs tests/notification_monitor.rs tests/xvfb_shape.rs; do
        n=${f##*/}; echo "${n%.rs}"; done)
fi
low=(systemd-run --user --scope -q -p MemoryMax=6G -p MemoryHigh=4G -p CPUWeight=20 -p IOWeight=20
    env CARGO_BUILD_JOBS=4 nice -n 15)
own_procs() { # private compositors/servers started by these tests (never the live session's)
    pgrep -af 'kwin_wayland --virtual|Xvfb :|hi-gui-|hermes-island --test-instance|xfwm4' \
        | grep -v -e pgrep -e run-gui-tests || true
}
status=0
for suite in "$@"; do
    echo "=== $suite $(date +%T)"
    "${low[@]}" cargo test --locked --test "$suite" -- --include-ignored \
        --skip clipboard_source_helper --test-threads=1 ${GUI_TEST_ARGS:-} \
        >"$logs/$suite.log" 2>&1
    rc=$?
    grep -E '^test |^test result:' "$logs/$suite.log" | grep -v '^test result: ok. 0 passed'
    [ $rc -eq 0 ] || status=1
    sleep 2
    left=$(own_procs)
    if [ -n "$left" ]; then
        echo "!!! leftover processes after $suite:"; echo "$left"; status=1
    fi
done
exit $status
