#!/usr/bin/env python3
"""Fail closed unless the recorded mutation evidence is complete and newer than every source."""
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
summary = ROOT / "evidence/phase3/mutations/summary.json"
rows = json.loads(summary.read_text())
assert len(rows) >= 13, f"only {len(rows)} mutations recorded"
assert all(r["restored"] and r["failedTests"] for r in rows), "a mutation did not fail a named test"
newest = max(
    p.stat().st_mtime
    for pattern in ("src-tauri/src/**/*.rs", "src-tauri/tests/**/*.rs", "ui/legacy/renderer.jsx", "ui/ipc/*.ts")
    for p in ROOT.glob(pattern)
    if "bindings.ts" not in p.name
)
if summary.stat().st_mtime < newest:
    sys.exit("FAIL: mutation summary is older than the sources; re-run scripts/phase3-mutations.sh")
print(f"mutation summary: {len(rows)} mutations, each restored with named failing tests, newer than all sources")
