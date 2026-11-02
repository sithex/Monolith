#!/usr/bin/env python3
"""Cargo test runner: preserve the actor's raw failure label before old gate grep filters.
No retries, argument changes, environment changes, exit-code changes or assertion changes.
"""
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
command = sys.argv[1:]
assert command, 'Cargo must supply the original test binary and arguments'
binary = Path(command[0]).resolve()
assert binary.is_relative_to(root / 'src-tauri/target'), binary
if not binary.name.startswith('kwin_phase3_actor-'):
    raise SystemExit(subprocess.call(command))
log = root / 'evidence/phase4/continuation/actor-raw-latest.log'
log.parent.mkdir(parents=True, exist_ok=True)
with log.open('wb') as output:
    child = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert child.stdout is not None
    for line in iter(child.stdout.readline, b''):
        output.write(line)
        output.flush()
        sys.stdout.buffer.write(line)
        sys.stdout.buffer.flush()
    status = child.wait()
raise SystemExit(status)
