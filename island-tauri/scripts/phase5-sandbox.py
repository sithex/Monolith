#!/usr/bin/env python3
"""Phase 5 leak snapshot. Reads metadata only, never opens clipboard/session sockets."""
import json
import os
from pathlib import Path
import sys

SCRATCH = Path(os.environ['TMPDIR'])

def snapshot():
    processes = []
    for proc in Path('/proc').iterdir():
        if not proc.name.isdigit():
            continue
        try:
            args = (proc / 'cmdline').read_bytes().split(b'\0')
            if not args:
                continue
            executable = args[0].decode(errors='replace')
            fixture = b'clipboard_source_helper' in args
            gui = executable.endswith(('kwin_wayland', 'Xwayland', 'Xvfb')) and any(b'hi-' in arg or b'--virtual' in arg for arg in args)
            if fixture or gui:
                processes.append((proc.name, [a.decode(errors='replace') for a in args]))
        except OSError:
            continue
    runtime = Path(f'/run/user/{os.getuid()}')
    wayland = sorted(p.name for p in runtime.glob('wayland-*'))
    home = Path.home()
    markers = {}
    for name in ['.config/autostart/hermes-island.desktop', '.config/xfce4', '.local/share/klipper']:
        path = home / name
        try:
            stat = path.stat()
            markers[name] = [stat.st_mode, stat.st_size, stat.st_mtime_ns]
        except FileNotFoundError:
            markers[name] = None
    return {'processes': sorted(processes), 'scratch': sorted(str(p) for p in SCRATCH.iterdir() if p.name.startswith(('hi-', 'phase5-mutations-'))), 'waylandSockets': wayland, 'homeMarkers': markers}

current = snapshot()
if len(sys.argv) == 1:
    print(json.dumps(current, indent=2, sort_keys=True))
else:
    before = json.loads(Path(sys.argv[1]).read_text())
    assert before == current, f'Phase 5 sandbox leak/marker change: before={before!r}, after={current!r}'
    print('PASS: owned GUI/source processes, scratch roots, Wayland socket names and real-home markers unchanged')
