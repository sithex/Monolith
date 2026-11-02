#!/usr/bin/env bash
# Phase 1 gate (SPEC §8): typecheck, UI build, fmt, clippy -D warnings, cargo test (incl. the Xvfb
# integration tests), dependency-tree checks, and proof that the live island was left alone.
# Usage: island-tauri/scripts/phase1-gate.sh   (run from anywhere; set CLEAN=1 for a from-scratch build)
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
LIVE_EXE="$(cd "$ROOT/.." && pwd)/island-rust/target/release/hermes-island"
export PATH="$HOME/.cargo/bin:$PATH"
cd "$ROOT"

live_pids() { # PIDs whose *executable* is the live island-rust binary (name alone is ambiguous)
  local p
  for p in $(pgrep -x hermes-island || true); do
    [[ "$(readlink "/proc/$p/exe" 2>/dev/null || true)" == "$LIVE_EXE" ]] && echo "$p"
  done
}
live_stamp() { for p in $(live_pids); do echo "$p start=$(ps -o lstart= -p "$p")"; done; }

echo "== live island before: $(live_stamp | tr '\n' ';')"
BEFORE="$(live_stamp)"

if [[ "${CLEAN:-0}" == 1 ]]; then
  echo "== clean build"; rm -rf dist evidence/phase1 src-tauri/target
fi
rm -rf evidence/phase1

echo "== npm run typecheck";  npm run typecheck
echo "== npm run build:ui";   npm run build:ui
cd src-tauri
echo "== cargo fmt --check";  cargo fmt --check
echo "== cargo clippy --all-targets --locked -- -D warnings"
cargo clippy --all-targets --locked -- -D warnings
echo "== cargo test --locked"
cargo test --locked
echo "== cargo tree -i gtk";    cargo tree --locked -i gtk | head -6
echo "== cargo tree -i specta"; cargo tree --locked -i specta | head -8
echo "== cargo tree -d (duplicates)"; cargo tree --locked -d | grep -E '^(gtk|gdk|glib|cairo-rs|specta|tauri|wry|tao) ' || echo "(no duplicate gtk/gdk/glib/cairo-rs/specta/tauri/wry/tao)"

echo "== live island after: $(live_stamp | tr '\n' ';')"
AFTER="$(live_stamp)"
# Controller amendment (Oct 31, Phase 6b2 audit): the live island is only a sandbox-leak detector
# (SPEC §8 Phase 3/6m notes); when it is not running before AND after, the unchanged check still holds.
if [[ "$BEFORE" != "$AFTER" ]]; then
  echo "FAIL: live island changed (before='$BEFORE' after='$AFTER')"; exit 1
fi
if [[ -z "$BEFORE" ]]; then echo "NOTE: live island not running before or after (unchanged)"; fi
echo "PASS: Phase 1 gate (live island PID/start unchanged)"
