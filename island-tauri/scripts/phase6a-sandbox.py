#!/usr/bin/env python3
"""Phase 6a leak snapshot: Phase 5's checks plus the private bus daemons, python mocks and
their scratch directories. Metadata only; never opens a bus or any socket."""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SCRATCH = Path(os.environ['TMPDIR'])


def snapshot():
    base = json.loads(subprocess.run([sys.executable, str(ROOT / 'scripts/phase5-sandbox.py')],
                                     capture_output=True, text=True, check=True).stdout)
    extra = []
    for proc in Path('/proc').iterdir():
        if not proc.name.isdigit():
            continue
        try:
            args = [a.decode(errors='replace') for a in (proc / 'cmdline').read_bytes().split(b'\0')]
        except OSError:
            continue
        joined = ' '.join(args)
        if 'system_mock.py' in joined or ('dbus-daemon' in args[0] and '/hi-bus-' in joined):
            extra.append((proc.name, args))
    base['processes'] = sorted(base['processes'] + extra)
    base['scratch'] = sorted(set(base['scratch']) | {
        str(p) for p in SCRATCH.iterdir() if p.name.startswith(('hi-bus-', 'phase6a-mutations-'))})
    return base


current = snapshot()
if len(sys.argv) == 1:
    print(json.dumps(current, indent=2, sort_keys=True))
else:
    before = json.loads(Path(sys.argv[1]).read_text())
    assert before == current, f'Phase 6a sandbox leak/marker change: before={before!r}, after={current!r}'
    print('PASS: owned GUI/source/bus/mock processes, scratch roots, Wayland socket names and real-home markers unchanged')
