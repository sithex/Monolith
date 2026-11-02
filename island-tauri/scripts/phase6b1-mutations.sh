#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
export CARGO_BUILD_JOBS=6
exec nice -n 10 python3 scripts/phase6b1-mutations.py "$@"
