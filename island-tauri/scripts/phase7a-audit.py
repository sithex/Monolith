#!/usr/bin/env python3
"""Freeze coexistence pages, accepted gates/mutations, SPEC and staging sources."""
from pathlib import Path
import hashlib
import json
import os
import sys
ROOT = Path(__file__).resolve().parents[1]
EXCLUDE = {'target','node_modules','dist','evidence','__pycache__','.git'}
FIXED = ['SPEC.md','index.html','legacy.html','spike.html','ui/main.tsx',
         'ui/legacy-main.tsx','ui/spike-main.tsx','ui/ipc/legacy-shim.ts',
         'ui/ipc/legacy-mode.ts','src-tauri/src/contract/events.rs']
paths = [ROOT / name for name in FIXED]
paths += [p for p in (ROOT/'scripts').iterdir() if p.is_file() and p.name.startswith('phase') and not p.name.startswith('phase7a')]
for relative in ['staging','ui-staging','ui/legacy']:
    for directory, dirs, files in os.walk(ROOT/relative):
        dirs[:] = [name for name in dirs if name not in EXCLUDE]
        paths.extend(Path(directory)/name for name in files)
current = {str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(paths)}
if len(sys.argv) == 1:
    print(json.dumps(current,indent=2,sort_keys=True))
else:
    before=json.loads(Path(sys.argv[1]).read_text())
    changed=sorted(key for key in before.keys()|current.keys() if before.get(key)!=current.get(key))
    assert not changed, f'Frozen coexistence/SPEC/staging/accepted scripts changed: {changed}'
    print(f'PASS: {len(current)} frozen coexistence, SPEC, staging and earlier script files unchanged')
