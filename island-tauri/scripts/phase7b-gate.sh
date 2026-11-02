#!/usr/bin/env bash
# Phase7b incremental acceptance; the accepted 7a gate stays unchanged.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
: "${TMPDIR:?private Hermes scratch is required}"
TIER="${TIER:-quick}"
case "$TIER" in quick|full) ;; *) printf '%s\n' 'TIER must be quick or full' >&2; exit 2 ;; esac
EVID="$ROOT/evidence/phase7b"
mkdir -p "$EVID"
COMPLETED=0
python3 scripts/phase3-audit.py > "$EVID/gate-before.json"
python3 scripts/phase7a-sandbox.py > "$EVID/gate-sandbox-before.json"
python3 scripts/phase7a-audit.py > "$EVID/gate-frozen-before.json"
finish() {
  local result=$?
  trap - EXIT
  local audit=0
  python3 scripts/phase3-audit.py "$EVID/gate-before.json" > "$EVID/gate-after-audit.log" || audit=$?
  python3 scripts/phase7a-sandbox.py "$EVID/gate-sandbox-before.json" > "$EVID/gate-sandbox-after.log" || audit=$?
  python3 scripts/phase7a-audit.py "$EVID/gate-frozen-before.json" > "$EVID/gate-frozen-after.log" || audit=$?
  if [[ "$result" == 0 && "$audit" == 0 && "$COMPLETED" == 1 ]]; then
    printf '%s\n' "PASS: Phase7b gate (TIER=$TIER)"
  else
    printf '%s\n' "FAIL: Phase7b gate (step=$result audit=$audit completed=$COMPLETED)"
    exit "$(( result != 0 ? result : audit != 0 ? audit : 1 ))"
  fi
}
trap finish EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
python3 -c 'import os; assert not any(k.startswith("CARGO_TARGET_") and k.endswith("_RUNNER") for k in os.environ), "runner override refused"; assert "CARGO_BUILD_JOBS" not in os.environ, "build jobs cap refused"'
# QUICK includes its necessary build/fmt/strict-Clippy/whole Rust suite (every GUI
# binary, including new 7b scenarios), bindings, UI tests, npm audit, legacy parity,
# 7a visuals AND all 16 preserved 7a mutations. FULL first chains its unchanged FULL.
TIER="$TIER" scripts/phase7a-gate.sh > "$EVID/gate-phase7a.log" 2>&1
python3 -c 'from pathlib import Path; t=Path("evidence/phase7a/gate-all-tests.log").read_text(); names=("launcher_real_drag_done_persists_across_restart","launcher_real_cancel_does_not_persist","launcher_real_disable_badge_hides_unlocked_module","launcher_home_and_settings_are_not_movable_or_disableable","timer_completed_activity_is_dismissable_by_real_input","frozen_prototype_keeps_real_terminal_and_motion_working","native_pill_side_union_and_launcher_app_pacing_use_real_input","media_capture_stops_on_real_navigation_and_collapse","launcher_apps_layout_and_draft_use_real_input_in_private_instance","authoritative_completion_kind_survives_phase_transition_and_dismisses_both_activities"); assert all(n+" ... ok" in t for n in names), "required new 7b scenarios did not run"; print("PASS: every named Phase7b native scenario ran")'
scripts/phase7b-mutations.sh > "$EVID/gate-mutations.log" 2>&1
python3 -B scripts/phase7b-mutations.py --check
python3 scripts/phase7b-visuals.py > "$EVID/gate-visuals.log"
python3 scripts/phase7b-audit.py "$EVID/resume-before.json" > "$EVID/gate-controller-boundary.log"
python3 -c 'import json; from pathlib import Path; p=json.loads(Path("evidence/phase7b/frame-pacing.json").read_text()); assert all(p[k]["count"]>3 for k in ("launcher","app")); m=json.loads(Path("evidence/phase7b/media-capture-counts.json").read_text()); assert m["starts"]==2 and m["reaped"] is True; print("PASS: real frame samples and private capture counters recorded")'
COMPLETED=1
