#!/usr/bin/env bash
# Six independent mutation checks. This is NOT full Phase 3 legacy-view coverage.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
exec python3 "$ROOT/scripts/phase3-mutations.py"
