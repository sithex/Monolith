#!/usr/bin/env python3
"""9a compiling mutation belt. Tests and mutations never target the real desktop."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess

ROOT = Path(__file__).resolve().parents[1]
EVID = ROOT / 'evidence/phase9a/mutations'
TEST = 'phase9a_readiness'
CASES = [
    ('live-dir-is-scratch', 'src-tauri/src/paths.rs', '".island-data-overlay"\n    } else', '"scratch"\n    } else', TEST, 'live_overlay_and_preview_resolve_exact_shared_dirs_without_io'),
    ('test-accepts-live', 'src-tauri/src/paths.rs', 'if named_live || inside_live {', 'if named_live && inside_live && resolved.as_os_str().is_empty() {', TEST, 'test_instance_cannot_select_live_paths_or_aliases'),
    ('legacy-lock-ignored', 'src-tauri/src/singleton.rs', 'let legacy_lock = lock("instance.lock")?;', 'let legacy_lock = spec_lock.try_clone()?;', TEST, 'singleton_refuses_legacy_instance_lock_in_scratch_live_dir'),
    ('status-missing-updated', 'src-tauri/src/integrations/plasma/runtime.rs', '"updated":SystemTime::now()', '"changedAt":SystemTime::now()', 'kwin_phase9a_release', 'anchor_client_complete_status_embedded_island_and_sigterm_use_real_eis'),
    ('unknown-settings-dropped', 'src-tauri/src/services/settings.rs', 'let mut saved = current.unknown.clone();', 'let mut saved = Map::new();', TEST, 'settings_save_preserves_unknown_nested_keys_without_exposing_them_over_ipc'),
    ('release-uses-devurl', 'build.sh', '--features custom-protocol', '--no-default-features', 'python', 'test_release_build_explicitly_embeds_assets_and_never_installs_dependencies'),
    ('launcher-loses-x11', 'run.sh', 'export GDK_BACKEND=x11', 'export GDK_BACKEND=wayland', 'python', 'test_run_forces_x11_and_scrubs_electron_environment'),
    ('live-fixture-accepted', 'src-tauri/src/cli.rs', 'if self.fixture_pty {', 'if self.fixture_pty && self.test_instance {', TEST, 'live_fixture_pty_is_refused_even_with_test_environment_enabled'),
]


def hashes():
    files = [ROOT / p for p in ('build.sh', 'run.sh', 'package.json', 'package-lock.json', 'vite.config.ts', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'src-tauri/build.rs', 'src-tauri/tauri.conf.json')]
    for name in ('src-tauri/src', 'src-tauri/tests', 'ui', 'scripts', 'tests'):
        files += [p for p in (ROOT / name).rglob('*') if p.is_file() and '__pycache__' not in p.parts]
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(set(files))}


def run(argv, log):
    with log.open('w') as out:
        code = subprocess.run(argv, cwd=ROOT, stdout=out, stderr=subprocess.STDOUT, timeout=1200).returncode
    return code, log.read_text()


def check(target, name, log, compile_only=False):
    if target == 'python':
        args = ['bash', '-n', 'build.sh', 'run.sh'] if compile_only else ['python3', '-B', '-m', 'unittest', 'discover', '-s', 'tests', '-p', 'live_launch_test.py', '-k', name, '-v']
    else:
        args = ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked', '--test', target, name]
        args += ['--no-run'] if compile_only else ['--', '--exact', '--test-threads=1']
    return run(args, log)


def fresh():
    rows = json.loads((EVID / 'summary.json').read_text())
    assert len(rows) == len(CASES) >= 8 and len({r['test'] for r in rows}) == len(rows)
    assert {r['mutation'] for r in rows} == {c[0] for c in CASES}
    for row in rows:
        assert row['compiled'] and row['restored']
        assert (EVID / f"{row['mutation']}.restore.diff").read_bytes() == b''
        text = (EVID / f"{row['mutation']}.failed.log").read_text()
        assert row['marker'] in text and 'could not compile' not in text and 'error[E' not in text
    assert json.loads((EVID / 'source-hashes.json').read_text()) == hashes(), 'source hashes stale'
    print(f'PASS: {len(rows)} compiling mutations killed by distinct named non-fuzz tests; identical restores and fresh hashes')


def interrupted(signum, frame):
    raise KeyboardInterrupt(f'interrupted by {signum}')


if __name__ == '__main__':
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    options = parser.parse_args()
    if options.check:
        fresh()
    else:
        EVID.mkdir(parents=True, exist_ok=True)
        initial = hashes()
        originals = {path: (ROOT / path).read_bytes() for _, path, *_ in CASES}
        rows = []
        try:
            for label, path, old, new, target, name in CASES:
                p = ROOT / path
                text = originals[path].decode()
                assert text.count(old) == 1, (label, 'nonunique mutation anchor')
                code, output = check(target, name, EVID / f'{label}.baseline.log')
                assert code == 0 and ('1 test' in output or 'Ran 1 test' in output), (label, output[-5000:])
                try:
                    p.write_text(text.replace(old, new, 1))
                    code, output = check(target, name, EVID / f'{label}.compile.log', True)
                    assert code == 0, (label, 'noncompiling mutation', output[-5000:])
                    code, output = check(target, name, EVID / f'{label}.failed.log')
                    marker = f'{name} (live_launch_test.LiveLauncherTests.{name}) ... FAIL' if target == 'python' else f'test {name} ... FAILED'
                    assert code != 0 and marker in output and 'could not compile' not in output and 'error[E' not in output, (label, 'not a named behavioral failure', output[-5000:])
                finally:
                    p.write_bytes(originals[path])
                    os.utime(p, None)
                    assert p.read_bytes() == originals[path]
                    (EVID / f'{label}.restore.diff').write_bytes(b'')
                code, output = check(target, name, EVID / f'{label}.restored.log')
                assert code == 0, (label, 'restore failed', output[-5000:])
                row = {'mutation': label, 'test': name, 'compiled': True, 'restored': True, 'marker': marker}
                rows.append(row)
                print(json.dumps(row), flush=True)
        finally:
            for path, data in originals.items():
                p = ROOT / path
                if p.read_bytes() != data:
                    p.write_bytes(data)
                    os.utime(p, None)
        assert hashes() == initial
        (EVID / 'summary.json').write_text(json.dumps(rows, indent=2) + '\n')
        (EVID / 'source-hashes.json').write_text(json.dumps(initial, sort_keys=True, indent=2) + '\n')
        fresh()
