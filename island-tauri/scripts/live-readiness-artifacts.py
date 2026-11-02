#!/usr/bin/env python3
"""Prepare exact launch-only changes. Never writes the original launcher/autostart."""
import difflib
import os
from pathlib import Path
import tempfile
import subprocess

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parent
EVID = ROOT / 'evidence/phase9a'
EVID.mkdir(parents=True, exist_ok=True)
source = REPO / 'island/anchor-client.py'
old = source.read_text()
assert old.count("ROOT / 'island-rust/run.sh'") == 1
new = old.replace("ROOT / 'island-rust/run.sh'", "ROOT / 'island-tauri/run.sh'")
patch = ''.join(difflib.unified_diff(old.splitlines(True), new.splitlines(True),
                                    fromfile='a/island/anchor-client.py', tofile='b/island/anchor-client.py'))
(EVID / 'anchor-client.patch').write_text(patch)
# Validate the real diff, not a description of the intended replacement.
with tempfile.TemporaryDirectory(prefix='hi-9a-patch-', dir=os.environ['TMPDIR']) as temporary:
    copy = Path(temporary) / 'anchor-client.py'
    copy.write_text(new)
    result = subprocess.run(['diff', '-u', '--label', 'a/island/anchor-client.py', '--label',
                             'b/island/anchor-client.py', str(source), str(copy)], capture_output=True, text=True)
    assert result.returncode == 1 and result.stdout == patch
    subprocess.run(['patch', '--dry-run', '-p1', '-i', str(EVID / 'anchor-client.patch')], cwd=REPO, check=True)
entry = (Path.home() / '.config/autostart/hermes-island.desktop').read_text()
old_exec = f'Exec="{REPO}/island-rust/run.sh" --overlay'
assert entry.count(old_exec) == 1
(EVID / 'hermes-island.desktop').write_text(entry.replace(old_exec, f'Exec="{REPO}/island-tauri/run.sh" --overlay'))
assert source.read_text() == old
print('PASS: diff -u launch-only anchor patch dry-runs; desktop file prepared; originals untouched')
