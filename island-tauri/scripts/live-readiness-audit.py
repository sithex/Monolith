#!/usr/bin/env python3
"""Read-only 9a boundary snapshot. Never connects to a live socket or reads secrets."""
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parent
SKIP = {'target', 'node_modules', 'dist', 'evidence', '__pycache__', '.git'}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_tree(base):
    result = {}
    for directory, dirs, files in os.walk(base):
        dirs[:] = [d for d in dirs if d not in SKIP]
        for name in files:
            p = Path(directory) / name
            if p.is_file():
                result[str(p.relative_to(REPO))] = digest(p)
    return result


def snapshot():
    protected = {}
    for name in ('island', 'island-rust', 'island-native', 'src',
                 'island-tauri/staging', 'island-tauri/ui-staging', 'island-tauri/ui/legacy'):
        protected.update(source_tree(REPO / name))
    for name in ('package.json', '.gitignore', 'island-tauri/SPEC.md',
                 'island-tauri/index.html', 'island-tauri/legacy.html',
                 'island-tauri/ui/legacy-main.tsx', 'island-tauri/ui/ipc/legacy-shim.ts',
                 'island-tauri/ui/ipc/legacy-mode.ts', 'island-tauri/src-tauri/src/contract/events.rs'):
        protected[name] = digest(REPO / name)
    earlier = {str(p.relative_to(ROOT)): digest(p) for p in (ROOT / 'scripts').iterdir()
               if p.is_file() and p.name.startswith('phase') and not p.name.startswith('phase9a')}
    data = {}
    for directory in (REPO / '.island-data', REPO / '.island-data-overlay'):
        if not directory.exists():
            continue
        for p in sorted(directory.rglob('*')):
            m = p.lstat()
            row: dict[str, object] = {'mode': stat.S_IMODE(m.st_mode), 'type': stat.S_IFMT(m.st_mode)}
            if stat.S_ISREG(m.st_mode):
                # Hash only; never emit settings/history/secret contents.
                row['sha256'] = digest(p)
                if p.name == 'status.json' and p.parent == directory:
                    value = json.loads(p.read_bytes())
                    value.pop('updated', None)
                    row['stableSha256'] = hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()
            elif stat.S_ISLNK(m.st_mode):
                row['link'] = os.readlink(p)
            data[str(p.relative_to(REPO))] = row
    auto = Path.home() / '.config/autostart/hermes-island.desktop'
    live = []
    for p in Path('/proc').iterdir():
        if not p.name.isdigit():
            continue
        try:
            exe = os.readlink(p / 'exe')
            if exe not in [str(REPO / n / 'target/release/hermes-island') for n in ('island-rust', 'island-tauri/src-tauri')]:
                continue
            live.append([p.name, exe, (p / 'stat').read_text().rsplit(')', 1)[1].split()[19]])
        except OSError:
            continue
    return {'protected': protected, 'earlier': earlier, 'data': data,
            'autostart': digest(auto) if auto.exists() else None, 'live': sorted(live),
            'xSockets': sorted(p.name for p in Path('/tmp/.X11-unix').iterdir()),
            'outsideGit': [l for l in subprocess.check_output(['git', 'status', '--porcelain'], cwd=REPO, text=True).splitlines() if ' island-tauri/' not in l]}


if __name__ == '__main__':
    current = snapshot()
    if len(sys.argv) == 1:
        print(json.dumps(current, indent=2, sort_keys=True))
    else:
        before = json.loads(Path(sys.argv[1]).read_text())
        # Keep raw heartbeat hashes in both evidence snapshots, but compare all
        # other status fields byte-for-value: a running app updates `updated`.
        for snap in (before, current):
            for key, row in snap['data'].items():
                if key in ('.island-data/status.json', '.island-data-overlay/status.json'):
                    row.pop('sha256', None)
        changes = [k for k in before if before[k] != current[k]]
        assert not changes, f'9a boundary changed: {changes}'
        print('PASS: protected sources, earlier scripts, real persistent data hashes, stable status hash (heartbeat excluded), autostart, live processes, X sockets and outside git unchanged')
