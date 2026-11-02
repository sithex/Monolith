#!/usr/bin/env python3
"""Phase 6m leak snapshot: Phase 6a's checks plus MPRIS mocks, fake tools, private PipeWire /
WirePlumber stacks and their scratch directories. Metadata only; never opens a bus or socket."""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SCRATCH = Path(os.environ['TMPDIR'])
PW_TOOLS = {'pipewire', 'wireplumber', 'pw-play', 'pw-cat', 'pw-record', 'pw-cli', 'pw-dump', 'pipewire-pulse'}
PREFIXES = ('hi-pw-', 'hi-eqfake-', 'hi-lyrics-', 'phase6m-mutations-', 'hi-bus-')


def snapshot():
    base = json.loads(subprocess.run([sys.executable, str(ROOT / 'scripts/phase6a-sandbox.py')],
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
        if 'mpris_mock.py' in joined or 'fake_pw.py' in joined:
            extra.append((proc.name, args))
            continue
        if os.path.basename(args[0]) in PW_TOOLS or 'wireplumber' in args[0]:
            # Only instances running against a scratch runtime dir are ours (the user's own
            # PipeWire is not touched and not compared).
            try:
                environ = (proc / 'environ').read_bytes().split(b'\0')
            except OSError:
                continue
            if any(e.startswith(b'XDG_RUNTIME_DIR=') and str(SCRATCH).encode() in e for e in environ):
                extra.append((proc.name, args))
    base['processes'] = sorted(base['processes'] + extra)
    base['scratch'] = sorted(set(base['scratch']) | {
        str(p) for p in SCRATCH.iterdir() if p.name.startswith(PREFIXES)})
    return base


current = snapshot()
if len(sys.argv) == 1:
    print(json.dumps(current, indent=2, sort_keys=True))
else:
    before = json.loads(Path(sys.argv[1]).read_text())
    assert before == current, f'Phase 6m sandbox leak/marker change: before={before!r}, after={current!r}'
    print('PASS: owned GUI/source/bus/mock/PipeWire processes, scratch roots, Wayland socket names and real-home markers unchanged')
