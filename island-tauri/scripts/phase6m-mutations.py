#!/usr/bin/env python3
"""Phase 6m compiling mutation evidence; every edit is restored and diff-checked.

Each row applies ONE deliberate behaviour change (it must compile), runs exactly one
named test, requires that test to FAIL by assertion (never a compile error), restores
the pristine bytes and diff-checks them. No size/count/time cap is ever removed: caps
are lowered. `--only LABEL[,LABEL]` runs a subset (diagnostic, never acceptance);
`--list` prints the rows. Phase 5's belt (scripts/phase5-mutations.py) is untouched.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT / 'src-tauri'
EVIDENCE = ROOT / 'evidence/phase6m/mutations'
WATCHED_TREES = ['src-tauri/src', 'src-tauri/tests', 'ui']
WATCHED_FILES = ['src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'scripts/phase6m-mutations.py', 'scripts/phase6m_cases.py']

sys.dont_write_bytecode = True
from phase6m_cases import CASES  # noqa: E402


def tree_hashes():
    paths = [p for tree in WATCHED_TREES for p in (ROOT / tree).rglob('*')
             if p.is_file() and '__pycache__' not in p.parts]
    paths.extend(ROOT / name for name in WATCHED_FILES)
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}


def cargo_test(target, expected, output):
    scope = ['--lib'] if target == '--lib' else ['--test', target]
    return subprocess.run(
        ['nice', '-n', '10', 'cargo', 'test', '--locked', *scope, expected, '--', '--exact'],
        cwd=CRATE, env=dict(os.environ, CARGO_BUILD_JOBS='6'),
        stdout=output, stderr=subprocess.STDOUT, timeout=420)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--only')
    parser.add_argument('--list', action='store_true')
    args = parser.parse_args()
    if args.list:
        for row in CASES:
            print(row[0], '->', row[5])
        return
    partial = bool(args.only)
    cases = [c for c in CASES if c[0] in args.only.split(',')] if partial else CASES
    assert cases and (partial or len(cases) >= 12), 'Phase 6m belt needs >= 12 mutations'
    assert len({c[5] for c in CASES}) == len(CASES) or partial, 'each row needs its own named test'
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    hashes = tree_hashes()
    sources = {name: (CRATE / name).read_bytes() for _, name, *_ in cases}
    rows = []
    with tempfile.TemporaryDirectory(prefix='phase6m-mutations-', dir=os.environ['TMPDIR']) as directory:
        pristine = Path(directory)
        for name, data in sources.items():
            target = pristine / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        try:
            for label, name, old, new, target, expected in cases:
                path = CRATE / name
                text = sources[name].decode()
                olds, news = (old, new) if isinstance(old, list) else ([old], [new])
                assert all(text.count(o) == 1 for o in olds), (label, "nonunique or missing mutation anchor")
                mutated = text
                for o, n in zip(olds, news):
                    mutated = mutated.replace(o, n, 1)
                try:
                    path.write_text(mutated)
                    log = EVIDENCE / f'{label}.log'
                    with log.open('w') as output:
                        result = cargo_test(target, expected, output)
                    output = log.read_text()
                    short = expected.rsplit('::', 1)[-1]
                    assert (result.returncode != 0 and f'test {expected} ... FAILED' in output
                            and 'could not compile' not in output and 'error[E' not in output), (label, output[-3000:])
                finally:
                    path.write_bytes(sources[name])
                    os.utime(path, None)
                    diff = subprocess.run(['diff', '-u', str(pristine / name), str(path)], capture_output=True, text=True)
                    (EVIDENCE / f'{label}.diff.log').write_text(diff.stdout + diff.stderr)
                    assert diff.returncode == 0, (label, 'restore differs')
                row = {'mutation': label, 'failedTests': [expected], 'restored': True}
                rows.append(row)
                print(json.dumps(row), flush=True)
        finally:
            for name, data in sources.items():
                (CRATE / name).write_bytes(data)
                os.utime(CRATE / name, None)
    with (EVIDENCE / ('restored-tests-partial.log' if partial else 'restored-tests.log')).open('w') as output:
        for _, _, _, _, target, expected in cases:
            result = cargo_test(target, expected, output)
            assert result.returncode == 0, ('restored test failed', expected)
    assert tree_hashes() == hashes, 'source changed during mutations'
    if partial:
        print(f'PARTIAL (not acceptance): {len(rows)} mutations killed and restored')
        return
    (EVIDENCE / 'source-hashes.json').write_text(json.dumps(hashes, indent=2, sort_keys=True) + '\n')
    (EVIDENCE / 'summary.json').write_text(json.dumps(rows, indent=2) + '\n')
    print(f'PASS: {len(rows)} compiling mutations killed; distinct named failures; every restoration diff-identical; restored named tests passed')


if __name__ == '__main__':
    main()
