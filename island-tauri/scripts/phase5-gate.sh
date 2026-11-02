#!/usr/bin/env bash
# Phase 5: earlier gates unchanged; one GUI binary at a time, scratch-only selection owners.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=6
EVID="$ROOT/evidence/phase5"
mkdir -p "$EVID"
python3 scripts/phase3-audit.py > "$EVID/gate-before.json"
python3 scripts/phase5-sandbox.py > "$EVID/gate-sandbox-before.json"
finish() {
  local result=$?
  trap - EXIT
  local audit=0
  python3 scripts/phase3-audit.py "$EVID/gate-before.json" > "$EVID/gate-after-audit.log" || audit=$?
  python3 scripts/phase5-sandbox.py "$EVID/gate-sandbox-before.json" > "$EVID/gate-sandbox-after.log" || audit=$?
  if [[ "$result" == 0 && "$audit" == 0 ]]; then printf '%s\n' 'PASS: Phase 5 gate'; else exit "$(( result != 0 ? result : audit ))"; fi
}
trap finish EXIT
# CLEAN mode is the explicit acceptance chain. No runner override, retry or skip.
if [[ "${CLEAN:-0}" == 1 ]]; then
  nice -n 10 scripts/phase3-mutations.sh > "$EVID/gate-pre-clean-phase3-mutations.log" 2>&1
  CLEAN=1 nice -n 10 scripts/phase3-gate.sh > "$EVID/gate-clean-phase3.log" 2>&1
fi
CLEAN=0 nice -n 10 scripts/phase4-gate.sh > "$EVID/gate-phase4.log" 2>&1
nice -n 10 cargo fmt --manifest-path src-tauri/Cargo.toml --check
nice -n 10 cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
nice -n 10 cargo test --manifest-path src-tauri/Cargo.toml --locked > "$EVID/gate-all-tests.log" 2>&1
python3 -c 'from pathlib import Path; t=Path("evidence/phase5/gate-all-tests.log").read_text(); names=["clipboard_default_off_has_no_watcher_thread_or_connection","clipboard_disable_joins_watcher_and_closes_its_connection","clipboard_real_watcher_reads_text_png_and_uri_list","clipboard_pause_discards_real_offers_and_resume_captures_only_future_changes","clipboard_sensitive_hint_and_validated_deny_patterns_skip_real_offers","clipboard_copy_echo_reorders_only_with_stable_id_and_pin","clipboard_pinned_survive_clear_and_persistence_is_private_pinned_only","clipboard_x11_only_environment_reports_unavailable_without_wayland_connection","clipboard_deny_settings_are_bounded_strict_atomic_and_persisted"]; assert all(f"test {n} ... ok" in t for n in names), "missing Phase 5 acceptance test"'
npm run typecheck
node scripts/check-bindings.mjs
npm audit --audit-level=low
nice -n 10 cargo tree --manifest-path src-tauri/Cargo.toml --locked -d > "$EVID/dependency-duplicates.txt"
python3 -c 'from pathlib import Path; import re; t=Path("evidence/phase5/dependency-duplicates.txt").read_text(); assert not re.search(r"^(zbus|wayland-client|gtk|gdk|glib|cairo-rs) v",t,re.M), "duplicate protected dependency"'
nice -n 10 scripts/phase5-mutations.sh > "$EVID/gate-mutations.log" 2>&1
python3 scripts/phase5-check-mutations.py
# Prior shape, identity and four-view screenshot assertions stay unchanged.
python3 scripts/phase3-screens.py > "$EVID/gate-screens.log"
