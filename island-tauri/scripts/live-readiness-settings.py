#!/usr/bin/env python3
"""One authorized read of settings; all parsing/saves run only against a scratch copy."""
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
source = ROOT.parent / '.island-data-overlay/settings.json'
with tempfile.TemporaryDirectory(prefix='hi-9a-settings-', dir=os.environ['TMPDIR']) as temporary:
    copy = Path(temporary) / 'seed.json'
    data = source.read_bytes()
    copy.write_bytes(data)
    copy.chmod(0o400)
    assert hashlib.sha256(copy.read_bytes()).digest() == hashlib.sha256(data).digest()
    result = subprocess.run(['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked',
                             '--test', 'phase9a_readiness', 'copied_user_settings_roundtrip_through_both_actual_parsers',
                             '--', '--exact', '--nocapture'], cwd=ROOT,
                            env={**os.environ, 'HI_9A_SETTINGS_SEED': str(copy)},
                            capture_output=True, text=True)
    print(result.stdout, end='')
    print(result.stderr, end='')
    assert result.returncode == 0
    assert 'PASS: private read-only user settings copy loaded' in result.stdout, 'copy test did not execute'
    assert copy.read_bytes() == data, 'read-only seed changed'
    if '--gui' in sys.argv:
        saved = Path(temporary) / 'tauri-saved.json'
        gui = subprocess.run(['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked',
                              '--test', 'kwin_phase9a_release', '--', '--test-threads=1', '--nocapture'],
                             cwd=ROOT, env={**os.environ, 'HI_9A_RELEASE_SMOKE': '1',
                                           'HI_9A_SETTINGS_SEED': str(copy), 'HI_9A_SAVED_COPY': str(saved)},
                             capture_output=True, text=True)
        print(gui.stdout, end=''); print(gui.stderr, end='')
        assert gui.returncode == 0 and 'PASS: release test instance loaded exact user settings COPY' in gui.stdout
        assert saved.is_file() and saved.stat().st_mode & 0o777 == 0o400
        # Run the ACTUAL rollback parser against the file saved by the real release
        # app/IPC, not just against a unit-service reconstruction of it.
        result = subprocess.run(['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked',
                                 '--test', 'phase9a_readiness', 'copied_user_settings_roundtrip_through_both_actual_parsers',
                                 '--', '--exact', '--nocapture'], cwd=ROOT,
                                env={**os.environ, 'HI_9A_SETTINGS_SEED': str(saved)},
                                capture_output=True, text=True)
        print(result.stdout, end=''); print(result.stderr, end='')
        assert result.returncode == 0 and 'PASS: private read-only user settings copy loaded' in result.stdout
    assert source.read_bytes() == data, 'real settings changed'
    print('PASS: original settings hash unchanged; scratch seed read-only, private and removed')
