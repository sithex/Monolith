#!/usr/bin/env bash
# Phase 6a: earlier gates unchanged (phase5 chains 4 → 3 …); private bus/HTTP mocks only.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=6
EVID="$ROOT/evidence/phase6a"
mkdir -p "$EVID"
python3 scripts/phase3-audit.py > "$EVID/gate-before.json"
python3 scripts/phase6a-sandbox.py > "$EVID/gate-sandbox-before.json"
finish() {
  local result=$?
  trap - EXIT
  local audit=0
  python3 scripts/phase3-audit.py "$EVID/gate-before.json" > "$EVID/gate-after-audit.log" || audit=$?
  python3 scripts/phase6a-sandbox.py "$EVID/gate-sandbox-before.json" > "$EVID/gate-sandbox-after.log" || audit=$?
  if [[ "$result" == 0 && "$audit" == 0 ]]; then printf '%s\n' 'PASS: Phase 6a gate'; else exit "$(( result != 0 ? result : audit ))"; fi
}
trap finish EXIT
# The whole earlier chain (phases 1→5) runs unchanged; CLEAN is passed straight down.
CLEAN="${CLEAN:-0}" nice -n 10 scripts/phase5-gate.sh > "$EVID/gate-phase5.log" 2>&1
nice -n 10 cargo fmt --manifest-path src-tauri/Cargo.toml --check
nice -n 10 cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
nice -n 10 cargo test --manifest-path src-tauri/Cargo.toml --locked -- --test-threads=1 > "$EVID/gate-all-tests.log" 2>&1
python3 - <<'PY'
from pathlib import Path
t = Path("evidence/phase6a/gate-all-tests.log").read_text()
names = [
 "clipboard_concealed_and_transient_offers_are_skipped_and_counted_as_sensitive",
 "clipboard_read_caps_drop_oversize_offers_and_a_following_offer_is_still_captured",
 "clipboard_persistence_load_rejects_group_or_other_readable_file",
 "power_initial_snapshot_property_change_event_and_alerts",
 "power_name_lost_reports_unavailable_then_restart_recovers_without_restarting_the_service",
 "power_ignores_signals_from_senders_that_are_not_the_upower_owner",
 "power_ignores_the_owners_signals_for_other_object_paths",
 "power_malformed_values_never_become_fabricated_numbers",
 "bluetooth_initial_snapshot_property_changes_added_and_removed_reach_the_topic",
 "bluetooth_name_lost_reports_unavailable_then_restart_recovers_without_restarting_the_service",
 "bluetooth_ignores_signals_from_senders_that_are_not_the_bluez_owner",
 "bluetooth_ignores_objects_outside_the_org_bluez_tree",
 "bluetooth_object_manager_signals_are_accepted_only_from_the_root_object",
 "bluetooth_malformed_values_never_become_fabricated_values",
 "weather_without_a_location_makes_no_request_whatever_else_happens",
 "weather_fetches_once_when_a_location_is_set_and_unit_changes_need_no_request",
 "weather_failures_keep_the_last_good_value_stale_and_never_fabricate",
 "weather_geocode_is_user_initiated_bounded_and_independent_of_enablement",
 "weather_location_change_discards_the_old_place_and_a_superseded_response",
 "services_report_capabilities_snapshots_events_alerts_and_recover_in_the_real_app",
 "a_test_instance_without_an_explicit_system_bus_never_connects_to_any_bus",
]
import re
missing = [n for n in names if not re.search(rf"^test (?:\S+::)?{n} \.\.\. ok$", t, re.M)]
assert not missing, f"missing Phase 6a acceptance tests: {missing}"
PY
npm run typecheck
node scripts/check-bindings.mjs
npm audit --audit-level=low
nice -n 10 cargo tree --manifest-path src-tauri/Cargo.toml --locked -d > "$EVID/dependency-duplicates.txt"
python3 - <<'PY'
from pathlib import Path
import re
t = Path("evidence/phase6a/dependency-duplicates.txt").read_text()
assert not re.search(r"^(zbus|wayland-client|gtk|gdk|glib|cairo-rs|reqwest|hyper|rustls) v", t, re.M), "duplicate protected dependency"
PY
nice -n 10 scripts/phase6a-mutations.sh > "$EVID/gate-mutations.log" 2>&1
python3 scripts/phase6a-check-mutations.py
# Prior shape, identity and four-view screenshot assertions stay unchanged.
python3 scripts/phase3-screens.py > "$EVID/gate-screens.log"
