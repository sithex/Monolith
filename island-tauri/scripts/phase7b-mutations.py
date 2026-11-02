#!/usr/bin/env python3
"""Phase7b mixed production-wiring belt, identical restores and hash freshness."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
EVID = ROOT / 'evidence/phase7b/mutations'
spec = importlib.util.spec_from_file_location('phase7b_cases', ROOT / 'scripts/phase7b-mutations.cases.py')
assert spec is not None and spec.loader is not None, 'mutation cases cannot be loaded'
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
CASES = module.CASES

def hashes():
    files = [p for tree in ['ui', 'tests', 'scripts', 'src-tauri/src', 'src-tauri/tests',
                           'src-tauri/capabilities', 'src-tauri/permissions']
             for p in (ROOT / tree).rglob('*') if p.is_file() and '__pycache__' not in p.parts]
    files += [ROOT/p for p in ['package.json', 'package-lock.json', 'src-tauri/Cargo.toml',
                              'src-tauri/Cargo.lock', 'src-tauri/tauri.conf.json',
                              'src-tauri/build.rs', 'vite.config.ts', 'tsconfig.json',
                              'index.html', 'legacy.html', 'spike.html', 'island.html']]
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(files)}

def run(args, path):
    with path.open('w') as log:
        result = subprocess.run(args, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=600)
    return result.returncode, path.read_text()

def check(runner, name, path):
    kind, target = runner.split(':', 1)
    if kind == 'ui':
        return run(['node', '--test', '--test-reporter=tap', f'--test-name-pattern=^{re.escape(name)}$', target], path)
    return run(['cargo','test','--manifest-path','src-tauri/Cargo.toml','--locked','--test',target,name,'--','--exact','--test-threads=1','--nocapture'],path)

def compile_case(runner, path):
    kind,target=runner.split(':',1)
    commands=[['npm','run','typecheck']]
    if kind=='gui':
        commands.append(['npm','run','build:ui'])
    if kind!='ui':
        commands.append(['cargo','test','--manifest-path','src-tauri/Cargo.toml','--locked','--test',target,'--no-run'])
    with path.open('w') as log:
        for args in commands:
            if args[0]=='cargo' and kind=='gui': (ROOT/'src-tauri/build.rs').touch()
            log.write('COMMAND '+repr(args)+'\n');log.flush()
            result=subprocess.run(args,cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,timeout=600)
            if result.returncode: return result.returncode,path.read_text()
    return 0,path.read_text()

def passed(runner,name,code,text):
    return code==0 and (bool(re.search(r'^ok \d+ - '+re.escape(name)+r'$',text,re.M)) if runner.startswith('ui:') else 'test result: ok. 1 passed; 0 failed;' in text and name in text)

def failed(runner,name,code,text):
    if runner.startswith('ui:'): return code!=0 and bool(re.search(r'^not ok \d+ - '+re.escape(name)+r'$',text,re.M))
    return code!=0 and 'test result: FAILED. 0 passed; 1 failed;' in text and bool(re.search(r'^    '+re.escape(name)+r'$',text,re.M))

def freshness(partial):
    assert partial or len(CASES) >= 14, 'Phase7b belt incomplete: at least 14 compiling rows and 4 real-GUI rows required'
    assert partial or sum(c[4].startswith('gui:') for c in CASES)>=4,'at least four real GUI rows required'
    rows = json.loads((EVID/'summary.json').read_text())
    assert len(rows) == len(CASES) and len({r['failedTest'] for r in rows}) == len(rows)
    assert {r['mutation'] for r in rows} == {c[0] for c in CASES}
    for row in rows:
        assert row['compiled'] and row['restored']
        assert not (EVID/f"{row['mutation']}.restore.diff").read_bytes()
        assert failed(row['runner'],row['failedTest'],1,(EVID/f"{row['mutation']}.log").read_text())
    assert json.loads((EVID/'source-hashes.json').read_text()) == hashes(), 'Phase7b mutation source hashes stale'
    print(f'{"PARTIAL checkpoint" if partial else "PASS"}: {len(rows)} compiling mutations killed ({sum(r["runner"].startswith("gui:") for r in rows)} real-GUI); distinct named failures, identical restores, fresh hashes')

def interrupted(signum, frame):
    raise KeyboardInterrupt(f'interrupted by signal {signum}')
signal.signal(signal.SIGTERM, interrupted)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--checkpoint', action='store_true', help='explicit partial evidence, never final acceptance')
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    assert args.checkpoint or len(CASES) >= 14, 'Phase7b belt incomplete; --checkpoint records partial evidence only'
    if args.check:
        freshness(args.checkpoint)
        return
    EVID.mkdir(parents=True, exist_ok=True)
    initial = hashes()
    originals = {path: (ROOT/path).read_bytes() for _, path, *_ in CASES}
    rows = []
    try:
        code, output = run(['npm', 'run', 'typecheck'], EVID/'baseline-typecheck.log')
        assert code == 0, output
        for label, path, old, new, runner, name in CASES:
            source = originals[path].decode()
            assert source.count(old) == 1, (label, 'mutation anchor must be unique')
            code, output = compile_case(runner,EVID/f'{label}.baseline-compile.log')
            assert code==0,(label,'baseline compile failed',output)
            code, output = check(runner,name, EVID/f'{label}.baseline.log')
            assert passed(runner,name,code,output), (label, output)
            target = ROOT/path
            pristine = EVID/f'{label}.pristine'
            pristine.write_bytes(originals[path])
            try:
                target.write_text(source.replace(old, new, 1))
                code, output = compile_case(runner, EVID/f'{label}.compile.log')
                assert code == 0, (label, 'not a compiling mutation', output)
                code, output = check(runner,name, EVID/f'{label}.log')
                assert failed(runner,name,code,output), (label, 'not a named behavioral failure', output)
            finally:
                target.write_bytes(originals[path])
                os.utime(target, None)
                assert target.read_bytes() == originals[path], 'restore differs'
                diff = subprocess.run(['diff', '-u', str(pristine), str(target)], capture_output=True, text=True, check=False)
                (EVID/f'{label}.restore.diff').write_text(diff.stdout + diff.stderr)
                assert diff.returncode == 0, (label, 'restore diff is not empty')
            code, output = compile_case(runner,EVID/f'{label}.restored-compile.log')
            assert code==0,(label,'restored compile failed',output)
            code, output = check(runner,name, EVID/f'{label}.restored.log')
            assert passed(runner,name,code,output), (label, output)
            rows.append({'mutation': label, 'failedTest': name, 'runner':runner, 'compiled': True, 'restored': True})
            print(f'KILLED {label}: {name}; restore identical', flush=True)
    finally:
        for path, data in originals.items():
            if (ROOT/path).read_bytes() != data:
                (ROOT/path).write_bytes(data)
                os.utime(ROOT/path, None)
    assert hashes() == initial, 'source changed during belt'
    (EVID/'summary.json').write_text(json.dumps(rows, indent=2)+'\n')
    (EVID/'source-hashes.json').write_text(json.dumps(initial, indent=2, sort_keys=True)+'\n')
    freshness(args.checkpoint)
if __name__ == '__main__':
    main()
