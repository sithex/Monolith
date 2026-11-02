#!/usr/bin/env bash
# Fail-closed Phase 4 gate. Incomplete monitor coverage MUST NOT produce PASS.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
EVID="$ROOT/evidence/phase4"
mkdir -p "$EVID"
python3 scripts/phase3-audit.py > "$EVID/gate-before.json"
finish() { python3 scripts/phase3-audit.py "$EVID/gate-before.json" > "$EVID/gate-after-audit.log"; }
trap finish EXIT
# Existing gates run unchanged, including their required fresh mutation evidence.
nice -n 10 scripts/phase4-mutations.sh > "$EVID/gate-mutations.log" 2>&1
nice -n 10 scripts/phase3-mutations.sh > "$EVID/gate-phase3-mutations.log" 2>&1
nice -n 10 scripts/phase3-gate.sh > "$EVID/gate-phase3.log" 2>&1
nice -n 10 cargo fmt --manifest-path src-tauri/Cargo.toml --check
nice -n 10 cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
for target in notification_actor notification_core_store notification_core_dbus notification_core_ingest notification_core_edges kwin_phase4_notifications kwin_phase4_tray; do
  nice -n 10 cargo test --manifest-path src-tauri/Cargo.toml --locked --test "$target"
done
node --test scripts/notifications.test.mjs
node scripts/check-bindings.mjs
npm audit --audit-level=low
python3 -c 'from pathlib import Path; import hashlib,json; h=json.loads(Path("evidence/phase4/mutations/source-hashes.json").read_text()); bad=[p for p,v in h.items() if not Path(p).is_file() or hashlib.sha256(Path(p).read_bytes()).hexdigest()!=v]; assert not bad, f"Mutation evidence no longer matches source: {bad}"'
nice -n 10 cargo tree --manifest-path src-tauri/Cargo.toml --locked -d > "$EVID/dependency-duplicates.txt"
python3 -c 'from pathlib import Path; import re; text=Path("evidence/phase4/dependency-duplicates.txt").read_text(); assert not re.search(r"^zbus v",text,re.M), "duplicate zbus"'
# These are mandatory acceptance tests, not optional skips. Do not remove to obtain green.
if [[ ! -s src-tauri/src/integrations/notifications/linux_monitor.rs || ! -s src-tauri/tests/notification_monitor.rs ]]; then
  printf '%s\n' 'FAIL: Phase 4 incomplete: passive Linux notification monitor and its private-bus acceptance tests are not implemented.'
  exit 1
fi
nice -n 10 cargo test --manifest-path src-tauri/Cargo.toml --locked --test notification_monitor > "$EVID/gate-monitor.log" 2>&1
python3 -c 'from pathlib import Path; text=Path("evidence/phase4/gate-monitor.log").read_text(); required=["passive_notify_monitor_correlates_notify_return_closed_and_actions", "owner_replacement_rearms_monitor_and_bounds_pending", "loop_guard_and_policy_are_enforced_on_private_bus", "monitor_denial_preserves_hermes_ingest_capability", "dismiss_only_requests_remote_close_for_live_same_server_id"]; missing=[name for name in required if f"test {name} ... ok" not in text]; assert not missing, f"Missing required monitor tests: {missing}"'
printf '%s\n' 'PASS: Phase 4 gate'
