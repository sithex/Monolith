#!/usr/bin/env python3
"""Earlier leak audits unchanged, augmented with Phase7a-owned scratch roots."""
from pathlib import Path
import json
import os
import subprocess
import sys
ROOT = Path(__file__).resolve().parents[1]
base = json.loads(subprocess.run([sys.executable, str(ROOT/'scripts/phase6b2-sandbox.py')], capture_output=True, text=True, check=True).stdout)
base['phase7aScratch'] = sorted(str(p) for p in Path(os.environ['TMPDIR']).iterdir() if p.name.startswith(('hi-7a-', 'hi-gui-phase7a-', 'phase7a-mutations-')))
if len(sys.argv) == 1:
    print(json.dumps(base, indent=2, sort_keys=True))
else:
    assert base == json.loads(Path(sys.argv[1]).read_text()), 'Phase7a sandbox process/socket/scratch/real-home marker changed'
    print('PASS: prior sandbox audits and Phase7a scratch roots unchanged')
