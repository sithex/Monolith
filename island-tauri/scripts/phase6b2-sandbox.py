#!/usr/bin/env python3
"""Augment accepted leak audits with all Phase 6b2-owned scratch roots."""
from pathlib import Path
import json,os,subprocess,sys
ROOT=Path(__file__).resolve().parents[1]
base=json.loads(subprocess.run([sys.executable,str(ROOT/'scripts/phase6b1-sandbox.py')],capture_output=True,text=True,check=True).stdout)
base['phase6b2Scratch']=sorted(str(p) for p in Path(os.environ['TMPDIR']).iterdir() if p.name.startswith(('hi-6b2-','phase6b2-mutations-')))
if len(sys.argv)==1:print(json.dumps(base,indent=2,sort_keys=True))
else:
    assert base==json.loads(Path(sys.argv[1]).read_text()),'Phase 6b2 sandbox process/socket/scratch/real-home marker changed'
    print('PASS: prior sandbox audits and Phase 6b2 scratch roots unchanged')
