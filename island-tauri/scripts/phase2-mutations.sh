#!/usr/bin/env bash
# Deliberate breakages against the real Phase 2 code; restoration is unconditional.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "$ROOT/scripts/phase2_mutations.py"
