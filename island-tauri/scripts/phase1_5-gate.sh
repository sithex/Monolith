#!/usr/bin/env bash
# Phase 1.5 gate (SPEC §8): everything in the Phase 1 gate (which includes the Xvfb tests), plus the
# nested-KWin tests, `npm audit` clean, regenerated + checked bindings, dependency decisions, and
# proof that nothing touched the live island, the real session, or the real home.
# Usage: island-tauri/scripts/phase1_5-gate.sh   (CLEAN=1 for a from-scratch cargo build)
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="$(cd "$ROOT/.." && pwd)"
LIVE_EXE="$REPO/island-rust/target/release/hermes-island"
export PATH="$HOME/.cargo/bin:$PATH"
cd "$ROOT"
EVID="$ROOT/evidence/phase1_5"
mkdir -p "$EVID"

live_pids() { local p; for p in $(pgrep -x hermes-island || true); do
  [[ "$(readlink "/proc/$p/exe" 2>/dev/null || true)" == "$LIVE_EXE" ]] && echo "$p"; done; }
live_stamp() { for p in $(live_pids); do echo "$p start=$(ps -o lstart= -p "$p")"; done; }
# things a nested compositor / test stack could leak into the real session or machine
audit() {
  echo "x11-sockets: $(ls /tmp/.X11-unix | tr '\n' ' ')"
  echo "runtime-dir hits: $(ls "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}" 2>/dev/null | grep -E 'hi-kw|hermes-island|kwin-xwayland-eis' | tr '\n' ' ')"
  echo "scratch-dirs: $(ls -d "${TMPDIR:-/tmp}"/hi-kwin-* "${TMPDIR:-/tmp}"/hi-xvfb-* 2>/dev/null | tr '\n' ' ')"
  echo "real-home: $(ls -d "$HOME/.local/share/local.hermes.island" "$HOME/.config/xfce4" 2>/dev/null | tr '\n' ' ')"
  echo "leaked-procs: $(pgrep -af 'kwin_wayland --virtual|hi-kw-|eis_pointer.py|Xvfb -screen' | grep -v pgrep | tr '\n' ';')"
}

BEFORE_LIVE="$(live_stamp)"; BEFORE_AUDIT="$(audit)"
ROOT_PKG_SHA="$(sha256sum "$REPO/package.json" | cut -d' ' -f1)"
echo "== live island before: $(echo "$BEFORE_LIVE" | tr '\n' ';')"
echo "== audit before"; echo "$BEFORE_AUDIT"
# Controller amendment (Oct 31, Phase 6b2 audit): live island is only a leak detector; not running before is fine (after must still equal before).
if [[ -z "$BEFORE_LIVE" ]]; then echo "NOTE: live island not running before (after must stay not running)"; fi

if [[ "${CLEAN:-0}" == 1 ]]; then echo "== clean build"; rm -rf dist src-tauri/target; fi
rm -f "$EVID"/*.png

echo "== npm audit (must be clean, including dev dependencies)"
npm audit 2>&1 | tee "$EVID/npm-audit-after.txt"
npm audit --omit=dev >/dev/null
echo "== vite / react plugin"; npm ls vite @vitejs/plugin-react 2>&1 | head -8
VITE_MAJOR="$(node -p "require('./node_modules/vite/package.json').version.split('.')[0]")"
[[ "$VITE_MAJOR" -ge 6 ]] || { echo "FAIL: vite major $VITE_MAJOR < 6"; exit 1; }

echo "== npm run typecheck"; npm run typecheck
echo "== npm run build:ui"; npm run build:ui

cd src-tauri
echo "== bindings: regenerate, must equal the checked-in file, then checked"
BEFORE_SHA="$(sha256sum ../ui/ipc/bindings.ts | cut -d' ' -f1)"
cargo test --locked --lib export_bindings -- --ignored 2>&1 | grep -E '^test |test result'
AFTER_SHA="$(sha256sum ../ui/ipc/bindings.ts | cut -d' ' -f1)"
echo "bindings sha256 before=$BEFORE_SHA after=$AFTER_SHA"
[[ "$BEFORE_SHA" == "$AFTER_SHA" ]] || { echo "FAIL: checked-in bindings were stale (now regenerated; re-run)"; exit 1; }
# Field-aware nullability is enforced by `generated_types_are_clean` (run below); a blanket
# `number | null` grep became wrong once Phase 2 added SPEC-required Option fields (revised after Phase 2 review).
if grep -nE '_Serialize|_Deserialize|bigint' ../ui/ipc/bindings.ts; then echo "FAIL: split/bigint types in bindings"; exit 1; fi
grep -n 'scale: number,' ../ui/ipc/bindings.ts
cargo test --locked --lib -- bindings_are_current generated_types_are_clean command_lists_match 2>&1 | grep -E '^test |test result'
(cd .. && npm run typecheck >/dev/null && echo "typecheck against regenerated bindings: ok")

echo "== cargo fmt --check";  cargo fmt --check
echo "== cargo clippy --all-targets --locked -- -D warnings"; cargo clippy --all-targets --locked -- -D warnings
echo "== cargo test --locked (lib + xvfb_shape + kwin_shape)"
cargo test --locked 2>&1 | grep -E '^test |test result|Running|panicked|FAILED' | tee "$EVID/cargo-test.txt"
! grep -q FAILED "$EVID/cargo-test.txt"

echo "== dependency decisions"
echo "-- cargo tree -i gtk";    cargo tree --locked -i gtk | head -4
echo "-- cargo tree -i specta"; cargo tree --locked -i specta | head -3
echo "-- duplicates:"; cargo tree --locked -d | grep -E '^(gtk|gdk|glib|cairo-rs|specta|tauri|wry|tao|zbus) ' || echo "(no duplicate gtk/gdk/glib/cairo-rs/specta/tauri/wry/tao/zbus)"
if cargo tree --locked -i keyring >/dev/null 2>&1; then echo "FAIL: keyring is in the dependency tree"; exit 1; else echo "keyring: absent"; fi
grep -rn "keyring" ../SPEC.md | grep -v -i -E "removed|no longer|dropped|decision|without|not " || true

cd "$ROOT"
echo "== root package.json untouched"
[[ "$(sha256sum "$REPO/package.json" | cut -d' ' -f1)" == "$ROOT_PKG_SHA" ]] && echo "ok (sha256 $ROOT_PKG_SHA)"

AFTER_LIVE="$(live_stamp)"; AFTER_AUDIT="$(audit)"
echo "== live island after: $(echo "$AFTER_LIVE" | tr '\n' ';')"
echo "== audit after"; echo "$AFTER_AUDIT"
[[ "$BEFORE_LIVE" == "$AFTER_LIVE" ]] || { echo "FAIL: live island changed"; exit 1; }
[[ "$BEFORE_AUDIT" == "$AFTER_AUDIT" ]] || { echo "FAIL: audit changed:"; diff <(echo "$BEFORE_AUDIT") <(echo "$AFTER_AUDIT") || true; exit 1; }
echo "PASS: Phase 1.5 gate (Phase 1 + nested KWin + npm audit + bindings; live island PID/start unchanged)"
