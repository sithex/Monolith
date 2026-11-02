#!/usr/bin/env bash
# Phase 6m gate (SPEC §8 Phase 6 "Tiered gates"). TIER=quick (default) or TIER=full.
#   quick: incremental build; fmt, clippy, the WHOLE cargo test suite (every earlier GUI test
#          binary is in it), bindings/typecheck/npm audit, dependency duplicates, the Phase 6m
#          mutations, the four-view screenshot compare and the before/after sandbox audits.
#   full:  the same, preceded by the unchanged earlier chain `CLEAN=1 scripts/phase6a-gate.sh`
#          (→ 5 → 4 → 3 → 1/1.5/2/2.5 with every earlier mutation script).
# Mocks only: private dbus-daemon, local HTTP fixtures, a private PipeWire instance.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=6
export TMPDIR="${TMPDIR:?TMPDIR must point at the scratch directory}"
TIER="${TIER:-quick}"
case "$TIER" in quick|full) ;; *) echo "TIER must be quick or full" >&2; exit 2 ;; esac
EVID="$ROOT/evidence/phase6m"
mkdir -p "$EVID"
python3 scripts/phase3-audit.py > "$EVID/gate-before.json"
python3 scripts/phase6m-sandbox.py > "$EVID/gate-sandbox-before.json"
finish() {
  local result=$?
  trap - EXIT
  local audit=0
  python3 scripts/phase3-audit.py "$EVID/gate-before.json" > "$EVID/gate-after-audit.log" || audit=$?
  python3 scripts/phase6m-sandbox.py "$EVID/gate-sandbox-before.json" > "$EVID/gate-sandbox-after.log" || audit=$?
  if [[ "$result" == 0 && "$audit" == 0 ]]; then printf '%s\n' "PASS: Phase 6m gate (TIER=$TIER)"; else exit "$(( result != 0 ? result : audit ))"; fi
}
trap finish EXIT
if [[ "$TIER" == full ]]; then
  # The whole earlier chain runs unchanged; CLEAN defaults to 1 for the full tier.
  CLEAN="${CLEAN:-1}" nice -n 10 scripts/phase6a-gate.sh > "$EVID/gate-phase6a.log" 2>&1
fi
nice -n 10 cargo fmt --manifest-path src-tauri/Cargo.toml --check
nice -n 10 cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
# The only #[ignore]d test an earlier gate runs explicitly is the bindings exporter.
nice -n 10 cargo test --manifest-path src-tauri/Cargo.toml --locked --lib export_bindings -- --ignored > "$EVID/gate-export-bindings.log" 2>&1
nice -n 10 cargo test --manifest-path src-tauri/Cargo.toml --locked -- --test-threads=1 > "$EVID/gate-all-tests.log" 2>&1
python3 scripts/phase6m-check-tests.py "$EVID/gate-all-tests.log"
npm run typecheck
node scripts/check-bindings.mjs
npm audit --audit-level=low
nice -n 10 cargo tree --manifest-path src-tauri/Cargo.toml --locked -d > "$EVID/dependency-duplicates.txt"
python3 - <<'PY'
from pathlib import Path
import re
t = Path("evidence/phase6m/dependency-duplicates.txt").read_text()
assert not re.search(r"^(zbus|wayland-client|gtk|gdk|glib|cairo-rs|reqwest|hyper|rustls) v", t, re.M), "duplicate protected dependency"
PY
nice -n 10 scripts/phase6m-mutations.sh > "$EVID/gate-mutations.log" 2>&1
python3 scripts/phase6m-check-mutations.py
# Prior shape, identity and four-view screenshot assertions stay unchanged.
python3 scripts/phase3-screens.py > "$EVID/gate-screens.log"
