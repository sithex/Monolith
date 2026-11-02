#!/usr/bin/env bash
# Phase7a incremental QUICK; FULL chains the unchanged accepted 6b2 FULL first.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
: "${TMPDIR:?TMPDIR must be Hermes scratch}"
TIER="${TIER:-quick}"
case "$TIER" in quick|full) ;; *) printf '%s\n' 'TIER must be quick or full' >&2; exit 2 ;; esac
EVID="$ROOT/evidence/phase7a"
COMPLETED=0
mkdir -p "$EVID"
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
    printf '%s\n' "PASS: Phase7a gate (TIER=$TIER)"
  else
    printf '%s\n' "FAIL: Phase7a gate (step=$result audit=$audit completed=$COMPLETED)"
    exit "$(( result != 0 ? result : audit != 0 ? audit : 1 ))"
  fi
}
trap finish EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
python3 -c 'import os; assert not any(k.startswith("CARGO_TARGET_") and k.endswith("_RUNNER") for k in os.environ), "runner override refused"; assert "CARGO_BUILD_JOBS" not in os.environ, "build jobs cap refused"'
if [[ "$TIER" == full ]]; then
  TIER=full scripts/phase6b2-gate.sh > "$EVID/gate-phase6b2.log" 2>&1
fi
npm run build:ui > "$EVID/gate-build.log" 2>&1
python3 -c 'from pathlib import Path; assert all(Path("dist",p).is_file() for p in ("index.html","legacy.html","spike.html","island.html"))'
# Cargo must re-embed the new assets, not reuse its old dist fingerprint.
touch src-tauri/src/lib.rs
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib export_bindings -- --ignored > "$EVID/gate-export-bindings.log" 2>&1
# Earlier GUI binaries remain included unchanged. Earlier explicitly-run ignore = exporter.
cargo test --manifest-path src-tauri/Cargo.toml --locked -- --test-threads=1 > "$EVID/gate-all-tests.log" 2>&1
python3 scripts/phase6b2-check-tests.py "$EVID/gate-all-tests.log"
python3 -c 'from pathlib import Path; t=Path("evidence/phase7a/gate-all-tests.log").read_text(); names=("agents_missing_database_is_unavailable_and_reader_never_creates_it","appearance_is_atomic_private_persistent_and_visual_revisions_never_regress","island_page_real_input_geometry_retained_draft_appearance_and_screenshots"); assert all(f"test {n} ... ok" in t for n in names); print("PASS: three named Phase7a Rust/real-GUI scenarios ran")'
npm run typecheck
node scripts/check-bindings.mjs
npm run test:ui
npm audit --audit-level=low
cargo tree --manifest-path src-tauri/Cargo.toml --locked -d > "$EVID/dependency-duplicates.txt"
python3 -c 'from pathlib import Path; import re; t=Path("evidence/phase7a/dependency-duplicates.txt").read_text(); assert not re.search(r"^(zbus|wayland-client|gtk|gdk|glib|cairo-rs|reqwest|hyper|rustls) v",t,re.M),"duplicate protected dependency"'
scripts/phase7a-mutations.sh > "$EVID/gate-mutations.log" 2>&1
python3 -B scripts/phase7a-mutations.py --check
python3 scripts/phase3-screens.py > "$EVID/gate-screens.log"
python3 scripts/phase7a-visuals.py > "$EVID/gate-visuals.log"
COMPLETED=1
