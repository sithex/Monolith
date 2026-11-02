#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "$ROOT"
: "${TMPDIR:?private Hermes scratch is required}"
exec python3 -B scripts/phase9a-mutations.py "$@"
