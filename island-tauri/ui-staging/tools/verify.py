#!/usr/bin/env python3
"""Package-only verification, capturing real combined stdout/stderr."""
import hashlib
import json
import pathlib
import subprocess
import sys
ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / ('evidence/round2' if '--round2' in sys.argv else 'evidence')
COMMANDS = [
    ('contracts', ['python3', 'tools/generate-contracts.py', '--check']),
    ('typecheck', ['npm', 'run', 'typecheck']),
    ('unit-tests', ['npm', 'test']),
    ('audit', ['npm', 'audit', '--audit-level=low']),
    ('build', ['npm', 'run', 'build']),
]
def main():
    OUT.mkdir(parents=True, exist_ok=True)
    results = []
    for name, command in COMMANDS:
        result = subprocess.run(['nice', '-n', '10', *command], cwd=ROOT, capture_output=True, text=True, timeout=180)
        text = result.stdout + result.stderr
        (OUT / f'{name}.log').write_text(text)
        print(f'$ nice -n 10 {" ".join(command)}\n{text}\nexit={result.returncode}', flush=True)
        results.append({'command': command, 'exit': result.returncode, 'log': str((OUT / f'{name}.log').relative_to(ROOT))})
        if result.returncode:
            (OUT / 'checks.json').write_text(json.dumps(results, indent=2) + '\n')
            raise SystemExit(result.returncode)
    for font in (ROOT / 'src/assets/fonts').glob('*.woff'):
        source = ROOT.parent / 'ui/src/assets/fonts' / font.name
        assert font.read_bytes() == source.read_bytes(), f'font copy drift: {font.name}'
    hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for part in ['src', 'tests', 'tools'] for p in (ROOT / part).rglob('*') if p.is_file() and '__pycache__' not in str(p)}
    (OUT / 'source-hashes.json').write_text(json.dumps(hashes, indent=2) + '\n')
    (OUT / 'checks.json').write_text(json.dumps(results, indent=2) + '\n')
    print('PASS: strict typecheck, unit tests, low-level audit, build, Rust fixture freshness and byte-identical bundled fonts')
if __name__ == '__main__':
    main()
