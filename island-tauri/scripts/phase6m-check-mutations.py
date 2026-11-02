#!/usr/bin/env python3
"""Phase 6m: source-hash freshness, >= 12 distinct named failures, exact restores."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / 'evidence/phase6m/mutations'
rows = json.loads((EVIDENCE / 'summary.json').read_text())
assert len(rows) >= 12, 'incomplete Phase 6m belt'
assert all(row['restored'] and row['failedTests'] for row in rows), 'a mutation did not fail or restore'
assert len({name for row in rows for name in row['failedTests']}) == len(rows), 'mutations need distinct named tests'
expected = json.loads((EVIDENCE / 'source-hashes.json').read_text())
current = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
           for tree in ['src-tauri/src', 'src-tauri/tests', 'ui'] for p in (ROOT / tree).rglob('*')
           if p.is_file() and '__pycache__' not in p.parts}
for name in ['src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'scripts/phase6m-mutations.py', 'scripts/phase6m_cases.py']:
    current[name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
assert expected == current, 'Phase 6m mutation evidence stale: source set or content differs'
print(f'PASS: {len(rows)} distinct compiling mutation failures, identical restores, source hashes current')
