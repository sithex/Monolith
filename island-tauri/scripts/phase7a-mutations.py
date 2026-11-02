#!/usr/bin/env python3
"""Compiling Phase 7a behavioral mutations, exact named failures and byte-identical restores."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
sys.dont_write_bytecode = True
from phase7a_cases import CASES
ROOT = Path(__file__).resolve().parents[1]
EVID = ROOT / 'evidence/phase7a/mutations'

def hashes():
    files = [p for tree in ['src-tauri/src', 'src-tauri/tests', 'ui'] for p in (ROOT/tree).rglob('*') if p.is_file() and '__pycache__' not in p.parts]
    files += list((ROOT/'scripts').glob('phase7a*'))
    files += [ROOT/p for p in ['package.json','package-lock.json','vite.config.ts','island.html','tests/phase7a-ui.test.mjs','src-tauri/Cargo.toml','src-tauri/Cargo.lock']]
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files if p.is_file()}

def run(argv, file, cwd=ROOT):
    with file.open('w') as out:
        result = subprocess.run(argv, cwd=cwd, stdout=out, stderr=subprocess.STDOUT, timeout=600)
    return result.returncode, file.read_text()

def build_ui(log):
    code, output = run(['npm','run','build:ui'], log)
    assert code == 0, output[-4000:]
    os.utime(ROOT/'src-tauri/src/lib.rs', None)

def check(target, name, log):
    if target == 'ui':
        return run(['node','--test','--test-reporter=tap',f'--test-name-pattern=^{re.escape(name)}$','tests/phase7a-ui.test.mjs'],log)
    kind, binary = target.split(':',1)
    if kind == 'gui': build_ui(log.with_suffix('.build.log'))
    args = ['--lib'] if binary == '--lib' else ['--test',binary]
    return run(['cargo','test','--locked',*args,name,'--','--exact','--test-threads=1'],log,ROOT/'src-tauri')

def freshness():
    rows = json.loads((EVID/'summary.json').read_text())
    assert len(CASES) >= 12 and len(rows) == len(CASES)
    assert {row['mutation'] for row in rows} == {case[0] for case in CASES}
    assert len({row['failedTest'] for row in rows}) == len(rows)
    for row in rows:
        assert row['restored'] and row['compiled']
        assert not (EVID/f"{row['mutation']}.diff.log").read_bytes()
        output=(EVID/f"{row['mutation']}.log").read_text()
        assert row['failureMarker'] in output
        assert 'could not compile' not in output and 'error[E' not in output
    assert json.loads((EVID/'source-hashes.json').read_text()) == hashes(), 'mutation source hashes stale'
    print(f'PASS: {len(rows)} compiling mutations, distinct named failures, identical restores and current source hashes')

def interrupted(signum, frame):
    raise KeyboardInterrupt(f'interrupted by {signum}')
signal.signal(signal.SIGTERM, interrupted)

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--only');parser.add_argument('--check',action='store_true');args=parser.parse_args()
    if args.check: freshness();return
    selected=[c for c in CASES if not args.only or c[0] in args.only.split(',')]
    assert selected and len({c[5] for c in CASES}) == len(CASES)
    if not args.only: assert len(selected)>=12
    EVID.mkdir(parents=True,exist_ok=True)
    initial=hashes(); originals={path:(ROOT/path).read_bytes() for _,path,*_ in selected};rows=[]
    # Green baseline is required BEFORE inducing failures.
    code,output=run(['npm','run','typecheck'],EVID/'baseline-typecheck.log');assert code==0,output
    code,output=run(['npm','run','test:ui'],EVID/'baseline-ui.log');assert code==0,output
    with tempfile.TemporaryDirectory(prefix='phase7a-mutations-',dir=os.environ['TMPDIR']) as scratch:
        pristine=Path(scratch)
        for name,data in originals.items():
            p=pristine/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
        try:
            for label,name,old,new,target,expected in selected:
                path=ROOT/name;source=originals[name].decode();assert source.count(old)==1,(label,'nonunique mutation anchor')
                # Targeted baseline proves a failure is not merely an already-broken test.
                code,output=check(target,expected,EVID/f'{label}.baseline.log');assert code==0,(label,output[-4000:])
                try:
                    path.write_text(source.replace(old,new,1))
                    if target=='ui' or target.startswith('gui:'):
                        code,output=run(['npm','run','typecheck'],EVID/f'{label}.compile.log');assert code==0,(label,'not a compiling mutation',output[-3000:])
                    code,output=check(target,expected,EVID/f'{label}.log')
                    marker=f'not ok 1 - {expected}' if target=='ui' else f'test {expected} ... FAILED'
                    assert code!=0 and marker in output and 'could not compile' not in output and 'error[E' not in output,(label,'not an executed named failure',output[-4000:])
                finally:
                    path.write_bytes(originals[name]);os.utime(path,None)
                    diff=subprocess.run(['diff','-u',str(pristine/name),str(path)],capture_output=True,text=True)
                    (EVID/f'{label}.diff.log').write_text(diff.stdout+diff.stderr);assert diff.returncode==0
                code,output=check(target,expected,EVID/f'{label}.restored.log');assert code==0,(label,output[-4000:])
                row={'mutation':label,'failedTest':expected,'compiled':True,'restored':True,'failureMarker':marker};rows.append(row);print(json.dumps(row),flush=True)
        finally:
            for name,data in originals.items():
                path=ROOT/name
                if path.read_bytes()!=data: path.write_bytes(data);os.utime(path,None)
    assert initial==hashes(),'source changed during belt'
    if args.only:
        print(f'PARTIAL, not acceptance: {len(rows)} compiling mutations killed and restored');return
    (EVID/'summary.json').write_text(json.dumps(rows,indent=2)+'\n')
    (EVID/'source-hashes.json').write_text(json.dumps(initial,indent=2,sort_keys=True)+'\n')
    freshness()
if __name__=='__main__': main()
