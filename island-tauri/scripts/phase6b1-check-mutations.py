#!/usr/bin/env python3
"""Fail closed on stale/incomplete Phase 6b1 mutation evidence."""
import hashlib
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
EVIDENCE=ROOT/'evidence/phase6b1/mutations'
rows=json.loads((EVIDENCE/'summary.json').read_text())
assert len(rows)>=12
assert all(row['restored'] and row['failedTests'] for row in rows)
assert len({n for row in rows for n in row['failedTests']})==len(rows)
expected=json.loads((EVIDENCE/'source-hashes.json').read_text())
current={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest()
         for tree in ['src-tauri/src','src-tauri/tests','ui'] for p in (ROOT/tree).rglob('*')
         if p.is_file() and '__pycache__' not in p.parts}
for name in ['src-tauri/Cargo.toml','src-tauri/Cargo.lock','scripts/phase6b1-mutations.py','scripts/phase6b1_cases.py']:
    current[name]=hashlib.sha256((ROOT/name).read_bytes()).hexdigest()
assert expected==current,'Phase 6b1 mutation source set/content changed'
for row in rows:
    log=(EVIDENCE/(row['mutation']+'.log')).read_text()
    assert all('test '+n+' ... FAILED' in log for n in row['failedTests'])
    assert 'could not compile' not in log and 'error[E' not in log
    assert not (EVIDENCE/(row['mutation']+'.diff.log')).read_text()
print(f'PASS: {len(rows)} distinct compiling mutation failures, identical restores, source hashes current')
