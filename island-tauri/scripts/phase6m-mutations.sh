#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=6
exec nice -n 10 python3 "$ROOT/scripts/phase6m-mutations.py" "$@"
