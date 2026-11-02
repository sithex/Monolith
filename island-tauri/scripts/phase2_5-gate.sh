#!/usr/bin/env bash
# Phase 2.5 gate (SPEC §8): the panel-overlap spike. Runs
#   1. the Phase 2 gate unchanged (fmt, clippy -D warnings, ALL cargo tests incl. the spike test,
#      typecheck/build, npm audit, bindings, legacy byte-identity, dependency decisions, leak audit),
#      unless SKIP_PHASE2=1 (CLEAN=1 is passed through for a from-scratch build);
#   2. the spike test on its own with its output kept as evidence, and the spike unit tests;
#   3. spike-specific checks (page built, evidence produced, no edits outside island-tauri/),
#   4. its own before/after audit: live island PID/start/exe, X sockets, scratch dirs, processes,
#      real-home markers, autostart entry. The nested KWin only ever uses a private bus and
#      scratch HOME/XDG_*; the real display, session bus and Plasma panel are never touched.
# Usage: island-tauri/scripts/phase2_5-gate.sh   (CLEAN=1, SKIP_PHASE2=1 optional)
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="$(cd "$ROOT/.." && pwd)"
LIVE_EXE="$REPO/island-rust/target/release/hermes-island"
export PATH="$HOME/.cargo/bin:$PATH"
cd "$ROOT"
EVID="$ROOT/evidence/phase2_5"
mkdir -p "$EVID"

live_pids() { local p; for p in $(pgrep -x hermes-island || true); do
  [[ "$(readlink "/proc/$p/exe" 2>/dev/null || true)" == "$LIVE_EXE" ]] && echo "$p"; done; }
live_stamp() { for p in $(live_pids); do echo "$p start=$(ps -o lstart= -p "$p")"; done; }
tauri_procs() { local p; for p in $(pgrep -x hermes-island || true); do
  [[ "$(readlink "/proc/$p/exe" 2>/dev/null || true)" == "$ROOT/src-tauri/target/"* ]] && echo "$p"; done; }
audit() {
  echo "x11-sockets: $(ls /tmp/.X11-unix | tr '\n' ' ')"
  echo "runtime-dir hits: $(ls "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}" 2>/dev/null | grep -E 'hi-kw|hi-gui|hermes-island|kwin-xwayland-eis' | tr '\n' ' ')"
  echo "scratch-dirs: $(ls -d "${TMPDIR:-/tmp}"/hi-kwin-* "${TMPDIR:-/tmp}"/hi-xvfb-* "${TMPDIR:-/tmp}"/hi-gui-* 2>/dev/null | tr '\n' ' ')"
  echo "real-home: $(ls -d "$HOME/.local/share/local.hermes.island" "$HOME/.config/xfce4" 2>/dev/null | tr '\n' ' ')"
  echo "autostart: $(sha256sum "$HOME/.config/autostart/hermes-island.desktop" 2>/dev/null | cut -d' ' -f1)"
  echo "leaked-procs: $(pgrep -af 'kwin_wayland --virtual|hi-kw-|hi-gui-|eis_pointer.py|eis_input.py|kwin_burst.py|Xvfb -screen' | grep -v pgrep | tr '\n' ';')"
  echo "tauri-island-procs: $(tauri_procs | tr '\n' ' ')"
}
# every tracked/untracked path OUTSIDE island-tauri/ (the task forbids edits there)
outside_state() { (cd "$REPO" && git status --porcelain | grep -v ' island-tauri/' | sort | sha256sum | cut -d' ' -f1); }
legacy_hash() { (cd "$REPO" && find island island-rust/src island-rust/Cargo.toml island-native/src src \
    -type f -not -path '*/node_modules/*' -not -path '*/dist/*' -not -path '*/__pycache__/*' -not -path '*/evidence/*' \
    -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1; sha256sum package.json | cut -d' ' -f1); }

BEFORE_LIVE="$(live_stamp)"; BEFORE_AUDIT="$(audit)"; BEFORE_OUT="$(outside_state)"; BEFORE_LEGACY="$(legacy_hash)"
echo "== live island before: $(echo "$BEFORE_LIVE" | tr '\n' ';')"
echo "== audit before"; echo "$BEFORE_AUDIT"
# Controller amendment (Oct 31, Phase 6b2 audit): live island is only a leak detector; not running before is fine (after must still equal before).
if [[ -z "$BEFORE_LIVE" ]]; then echo "NOTE: live island not running before (after must stay not running)"; fi
[[ -z "$(tauri_procs)" ]] || { echo "FAIL: a Tauri island process is already running"; exit 1; }

if [[ "${SKIP_PHASE2:-0}" != 1 ]]; then
  echo "== Phase 2 gate (unchanged script; includes the spike test via cargo test)"
  "$ROOT/scripts/phase2-gate.sh" 2>&1 | tee "$EVID/phase2-gate.log" | tail -n 25
  grep -q '^PASS: Phase 2 gate' "$EVID/phase2-gate.log" || { echo "FAIL: Phase 2 gate did not pass"; exit 1; }
fi

echo "== spike page is part of the build"
test -f dist/spike.html || { echo "FAIL: dist/spike.html missing (npm run build:ui)"; exit 1; }
grep -q 'spike' vite.config.ts

cd src-tauri
echo "== spike unit tests: --spike-panel parsing and the test-only gate"
cargo test --locked --lib spike 2>&1 | tee "$EVID/lib-spike-tests.txt" | grep -E '^test |test result'
grep -q 'cli::tests::spike_panel_is_test_only_overlay_only_and_defaults_off ... ok' "$EVID/lib-spike-tests.txt"
grep -q 'window::spike::tests::bad_specs_are_rejected_not_ignored ... ok' "$EVID/lib-spike-tests.txt"

echo "== spike scenario on a private nested KWin (real EIS input, compositor captures)"
rm -f "$EVID"/*.png "$EVID"/summary.json
cargo test --locked --test kwin_spike_panel -- --nocapture 2>&1 | tee "$EVID/cargo-test.txt" \
  | grep -E '^test |test result|panicked|PHASE2_5_SUMMARY' | cut -c1-240
grep -q 'test kwin_spike_panel ... ok' "$EVID/cargo-test.txt" || { echo "FAIL: kwin_spike_panel did not pass"; exit 1; }
grep -q 'spike_analysis::tests::percentile_stats_and_sharpness ... ok' "$EVID/cargo-test.txt"
grep -q 'test result: ok. 6 passed' "$EVID/cargo-test.txt" || { echo "FAIL: expected 6 passing tests in kwin_spike_panel"; exit 1; }
for f in summary.json morph-open-contact-sheet.png morph-close-contact-sheet.png blur-on.png blur-reference-island-closed.png \
         stack-notification-start.png stack-toolbar-above-after-dock-click.png; do
  test -s "$EVID/$f" || { echo "FAIL: evidence file $f missing"; exit 1; }
done
echo "evidence: $(ls "$EVID" | wc -l) files in $EVID"

cd "$ROOT"
AFTER_LIVE="$(live_stamp)"; AFTER_AUDIT="$(audit)"; AFTER_OUT="$(outside_state)"; AFTER_LEGACY="$(legacy_hash)"
echo "== live island after: $(echo "$AFTER_LIVE" | tr '\n' ';')"
echo "== audit after"; echo "$AFTER_AUDIT"
[[ "$BEFORE_LIVE" == "$AFTER_LIVE" ]] || { echo "FAIL: live island changed"; exit 1; }
[[ "$BEFORE_AUDIT" == "$AFTER_AUDIT" ]] || { echo "FAIL: audit changed:"; diff <(echo "$BEFORE_AUDIT") <(echo "$AFTER_AUDIT") || true; exit 1; }
[[ "$BEFORE_OUT" == "$AFTER_OUT" ]] || { echo "FAIL: files outside island-tauri/ changed"; exit 1; }
[[ "$BEFORE_LEGACY" == "$AFTER_LEGACY" ]] || { echo "FAIL: legacy trees or root package.json changed"; exit 1; }
# renderer.jsx alone carries the pinned Phase 3 ownership amendment (SPEC §8 Phase 3): the live
# diff must equal ui/legacy/renderer.ownership.patch byte-for-byte.
# (`diff` exits 1 when the files differ, which is expected here; only `cmp` decides.)
if ! (diff -u --label island/renderer.jsx --label island-tauri/ui/legacy/renderer.jsx "$REPO/island/renderer.jsx" ui/legacy/renderer.jsx || true) \
    | cmp - ui/legacy/renderer.ownership.patch; then echo "FAIL: renderer.jsx drifted from island/renderer.jsx + renderer.ownership.patch"; exit 1; fi
echo "renderer.jsx == island/renderer.jsx + renderer.ownership.patch"
for f in monitor.jsx ai-stats.jsx state.cjs palette.cjs click-sound.cjs style.css; do
  cmp "$REPO/island/$f" "ui/legacy/$f"
done
echo "ui/legacy byte-identical to island/; legacy trees and root package.json unchanged"
echo "PASS: Phase 2.5 gate (spike scenario green; live island PID/start unchanged; no leaks)"
