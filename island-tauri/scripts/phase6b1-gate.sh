#!/usr/bin/env bash
# SPEC §8 tiered gates. QUICK is incremental; FULL first runs the unchanged earlier chain.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=6
: "${TMPDIR:?TMPDIR must be the Hermes scratch directory}"
TIER="${TIER:-quick}"
case "$TIER" in quick|full) ;; *) printf '%s\n' 'TIER must be quick or full' >&2; exit 2 ;; esac
# Controller amendment (Phase 6b1 audit): the agent's usage guard was removed from the gate; usage guards belong to the agent's procedure, never to a gate's verdict.
EVID="$ROOT/evidence/phase6b1"
COMPLETED=0
mkdir -p "$EVID"
python3 scripts/phase3-audit.py > "$EVID/gate-before.json"
python3 scripts/phase6b1-sandbox.py > "$EVID/gate-sandbox-before.json"
finish() {
  local result=$?
  trap - EXIT
  local audit=0
  python3 scripts/phase3-audit.py "$EVID/gate-before.json" > "$EVID/gate-after-audit.log" || audit=$?
  python3 scripts/phase6b1-sandbox.py "$EVID/gate-sandbox-before.json" > "$EVID/gate-sandbox-after.log" || audit=$?
  if [[ "$result" == 0 && "$audit" == 0 && "$COMPLETED" == 1 ]]; then
    printf '%s\n' "PASS: Phase 6b1 gate (TIER=$TIER)"
  else
    printf '%s\n' "FAIL: Phase 6b1 gate (step=$result audit=$audit)"
    exit "$(( result != 0 ? result : audit != 0 ? audit : 1 ))"
  fi
}
trap finish EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
if [[ "$TIER" == full ]]; then
  TIER=full nice -n 10 scripts/phase6m-gate.sh > "$EVID/gate-phase6m.log" 2>&1
fi
nice -n 10 cargo fmt --manifest-path src-tauri/Cargo.toml --check
nice -n 10 cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
nice -n 10 cargo test --manifest-path src-tauri/Cargo.toml --locked --lib export_bindings -- --ignored > "$EVID/gate-export-bindings.log" 2>&1
# All earlier GUI binaries are included; the exporter is the only ignored earlier gate item.
nice -n 10 cargo test --manifest-path src-tauri/Cargo.toml --locked -- --test-threads=1 > "$EVID/gate-all-tests.log" 2>&1
python3 scripts/phase6b1-check-tests.py "$EVID/gate-all-tests.log"
npm run typecheck
node scripts/check-bindings.mjs
npm audit --audit-level=low
nice -n 10 cargo tree --manifest-path src-tauri/Cargo.toml --locked -d > "$EVID/dependency-duplicates.txt"
python3 -c 'from pathlib import Path; import re; t=Path("evidence/phase6b1/dependency-duplicates.txt").read_text(); assert not re.search(r"^(zbus|wayland-client|gtk|gdk|glib|cairo-rs|reqwest|hyper|rustls) v",t,re.M),"duplicate protected dependency"'
nice -n 10 scripts/phase6b1-mutations.sh > "$EVID/gate-mutations.log" 2>&1
python3 scripts/phase6b1-check-mutations.py
python3 scripts/phase3-screens.py > "$EVID/gate-screens.log"
COMPLETED=1
