#!/usr/bin/env python3
"""One bounded foreground wait/check, never starts a display or browser itself."""
import argparse
from datetime import datetime, timezone
import importlib.util
import json
import pathlib
import time
ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / 'evidence/round2/gui-isolation-checks.json'
def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--delay', type=int, default=0)
    args = parser.parse_args()
    assert 0 <= args.delay <= 540
    if args.delay:
        print(f'Foreground wait {args.delay}s before next GUI exclusion check; no GUI started', flush=True)
        time.sleep(args.delay)
    spec = importlib.util.spec_from_file_location('capture', ROOT / 'tools/screenshots.py')
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    active = module.active_gates()
    rows = json.loads(OUT.read_text()) if OUT.exists() else []
    rows.append({'at': datetime.now(timezone.utc).isoformat(), 'pgrepPattern': module.PATTERN, 'stdout': active, 'ready': not active})
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(rows, indent=2) + '\n')
    print('GUI exclusion pgrep stdout:\n' + (active or '(empty — safe to invoke screenshot runner, which rechecks again)'))
if __name__ == '__main__':
    main()
