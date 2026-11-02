#!/usr/bin/env python3
"""Generate differential fixtures using only the Rust core's public API."""
import json
import pathlib
import subprocess
import sys
ROOT = pathlib.Path(__file__).resolve().parents[1]
result = subprocess.run(['nice', '-n', '10', 'cargo', 'run', '--locked', '--quiet', '--manifest-path', str(ROOT / 'contract-probe/Cargo.toml')], cwd=ROOT, capture_output=True, text=True, check=True)
traces = json.loads(result.stdout)
path = ROOT / 'tests/arrangement-fixtures.json'
if '--check' in sys.argv:
    assert json.loads(path.read_text()) == traces, 'Rust fixture drift'
else:
    path.write_text(result.stdout)
print(f'Rust conformance: {len(traces)} traces, {sum(len(t["steps"]) for t in traces)} transitions; {"checked" if "--check" in sys.argv else "generated"} {path.relative_to(ROOT)}')
