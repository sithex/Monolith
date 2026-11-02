#!/usr/bin/env python3
"""Fail-closed mutation evidence. Only owns explicitly snapshotted Phase 2 files.
No staging, legacy-source, live app, desktop or real HOME operations.
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
EVID = ROOT / 'evidence/phase2-mutations'
EVID.mkdir(parents=True, exist_ok=True)
CARGO = shutil.which('cargo')
assert CARGO, 'cargo must be installed'
FILES = ['ui/ipc/xterm-ack-core.ts', 'ui/ipc/legacy-mode.ts',
         'src-tauri/src/services/terminal/mod.rs', 'src-tauri/src/contract/settings.rs',
         'src-tauri/src/contract/system.rs', 'ui/ipc/bindings.ts']
original = {name: (ROOT / name).read_bytes() for name in FILES}

def restore():
    for name, content in original.items():
        (ROOT / name).write_bytes(content)

def mutate(name, old, new):
    text = (ROOT / name).read_text()
    assert text.count(old) == 1, (name, old, text.count(old))
    (ROOT / name).write_text(text.replace(old, new))

def run(args, *, cargo=False):
    command = ['nice', '-n', '10', *args]
    result = subprocess.run(command, cwd=ROOT / 'src-tauri' if cargo else ROOT,
                            text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    return result.returncode, result.stdout

cases = [
    ('ack-on-delivery', 'ui/ipc/xterm-ack-core.ts', 'record(packet: Packet) {',
     'record(packet: Packet) { parsed(packet.seq);',
     ['node', '--test', 'scripts/xterm-ack-adapter.test.mjs'], False,
     'no credit before the xterm parse callback'),
    ('reader-never-pauses', 'src-tauri/src/services/terminal/mod.rs',
     '                    flow.wait_for_credit();',
     '                    // MUTATION: PTY reads never pause for credit.',
     [CARGO, 'test', '--locked', '--test', 'kwin_gui', 'kwin_gui_overlay', '--', '--nocapture'], True,
     'kwin_gui_overlay'),
    ('shim-mode-wrong', 'ui/ipc/legacy-mode.ts', 'preview: "hover"', 'preview: "still"',
     ['node', '--test', 'scripts/xterm-ack-adapter.test.mjs'], False,
     'shim translates every mode'),
    ('accept-invalid-setting', 'src-tauri/src/contract/settings.rs',
     'if value.is_empty() || is_hex_color(value) {',
     'if value.is_empty() || is_hex_color(value) || value == "red" {',
     [CARGO, 'test', '--locked', '--lib', 'patch_validation_is_atomic_and_clamps', '--', '--nocapture'], True,
     'patch_validation_is_atomic_and_clamps'),
    ('plain-f64-to-option', 'src-tauri/src/contract/system.rs',
     'pub updated_at: f64,', 'pub updated_at: Option<f64>,',
     [CARGO, 'test', '--locked', '--lib', 'generated_types_are_clean', '--', '--nocapture'], True,
     'generated_types_are_clean'),
]
# Snapshot only our source areas, not concurrent staging/build/evidence. Each mutation is
# restored and diffed against pristine bytes even when a test crashes or unexpectedly passes.
scratch = Path(tempfile.mkdtemp(prefix='hi-phase2-mutations-', dir=os.environ['TMPDIR']))
try:
    for name, content in original.items():
        dest = scratch / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(content)
    for tag, file, old, new, command, cargo, expected in cases:
        restore()
        mutate(file, old, new)
        if tag == 'plain-f64-to-option':
            code, text = run([CARGO, 'test', '--locked', '--lib', 'export_bindings', '--', '--ignored'], cargo=True)
            assert code == 0, text
        code, text = run(command, cargo=cargo)
        (EVID / f'{tag}.log').write_text(text)
        assert code != 0, f'{tag}: broken product unexpectedly passed\n{text}'
        assert expected in text, f'{tag}: expected named failure absent\n{text}'
        # Compiler errors, infrastructure errors and a generic nonzero status do not count.
        assert not re.search(r'error\[E\d+\]|could not compile', text), text
        if cargo:
            failures = re.findall(r'^test (.+) \.\.\. FAILED$', text, re.M)
            assert any(expected in name for name in failures), text
        else:
            assert re.search(r'(?:✖|not ok).*' + re.escape(expected), text), text
            failures = [line for line in text.splitlines() if line.startswith(('✖', 'not ok'))]
        print(f'{tag}: expected failure (exit {code}): {failures}', flush=True)
        restore()
        for name in FILES:
            subprocess.run(['diff', '-u', str(scratch / name), str(ROOT / name)], check=True)
        print(f'{tag}: restored; diff identical', flush=True)
    print('PASS: all five deliberate breakages failed named tests; final source/bindings diff identical', flush=True)
finally:
    restore()
    shutil.rmtree(scratch)
