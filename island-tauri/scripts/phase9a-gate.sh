#!/usr/bin/env bash
# Incremental 9a acceptance. The final unchanged 7b QUICK is a separate controller gate.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "$ROOT"
: "${TMPDIR:?private Hermes scratch is required}"
EVID="$ROOT/evidence/phase9a"
mkdir -p "$EVID"
COMPLETED=0
python3 -B scripts/live-readiness-audit.py > "$EVID/gate-before.json"
python3 scripts/phase7a-sandbox.py > "$EVID/gate-sandbox-before.json"
python3 scripts/phase7a-audit.py > "$EVID/gate-frozen-before.json"
finish() {
  local result=$?
  trap - EXIT
  local audit=0
  python3 -B scripts/live-readiness-audit.py "$EVID/gate-before.json" > "$EVID/gate-after-audit.log" || audit=$?
  python3 scripts/phase7a-sandbox.py "$EVID/gate-sandbox-before.json" > "$EVID/gate-sandbox-after.log" || audit=$?
  python3 scripts/phase7a-audit.py "$EVID/gate-frozen-before.json" > "$EVID/gate-frozen-after.log" || audit=$?
  if [[ "$result" == 0 && "$audit" == 0 && "$COMPLETED" == 1 ]]; then
    printf '%s\n' 'PASS: Phase9a gate'
  else
    printf '%s\n' "FAIL: Phase9a gate (step=$result audit=$audit completed=$COMPLETED)"
    exit "$((result != 0 ? result : audit != 0 ? audit : 1))"
  fi
}
trap finish EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
python3 -c 'import os; assert not any(k.startswith("CARGO_TARGET_") and k.endswith("_RUNNER") for k in os.environ), "runner override refused"; assert "CARGO_BUILD_JOBS" not in os.environ, "jobs cap refused"'
npm run build:ui > "$EVID/gate-ui-build.log" 2>&1
touch src-tauri/build.rs
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib export_bindings -- --ignored > "$EVID/gate-bindings.log" 2>&1
cargo test --manifest-path src-tauri/Cargo.toml --locked -- --test-threads=1 > "$EVID/gate-all-tests.log" 2>&1
python3 scripts/phase6b2-check-tests.py "$EVID/gate-all-tests.log"
python3 -B scripts/live-readiness-settings.py > "$EVID/gate-settings.log" 2>&1
python3 -B -m unittest discover -s tests -p live_launch_test.py -v
npm run typecheck
node scripts/check-bindings.mjs
npm run test:ui
npm audit --audit-level=low
cargo tree --manifest-path src-tauri/Cargo.toml --locked -d > "$EVID/dependency-duplicates.txt"
python3 -c 'from pathlib import Path; import re; assert not re.search(r"^(zbus|wayland-client|gtk|gdk|glib|cairo-rs|reqwest|hyper|rustls) v",Path("evidence/phase9a/dependency-duplicates.txt").read_text(),re.M)'
./build.sh > "$EVID/gate-release-build.log" 2>&1
env HI_9A_RELEASE_SMOKE=1 cargo test --manifest-path src-tauri/Cargo.toml --locked --test kwin_phase9a_release -- --test-threads=1 --nocapture > "$EVID/gate-release-smoke.log" 2>&1
python3 -B scripts/live-readiness-settings.py --gui > "$EVID/gate-release-settings-copy.log" 2>&1
python3 -B scripts/live-readiness-artifacts.py
scripts/phase9a-mutations.sh > "$EVID/gate-mutations.log" 2>&1
python3 -B scripts/phase9a-mutations.py --check
python3 scripts/phase3-screens.py > "$EVID/gate-legacy-screens.log"
COMPLETED=1
