#!/usr/bin/env python3
"""Phase 5 compiling mutation evidence; every edit is restored and diff-checked.

--carryover is an explicitly partial run closing the Phase 4 decoder test gaps.
It must never be mistaken for the full Phase 5 acceptance belt.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT / 'src-tauri'
CASES = [
    ('signal-owner-guard-removed', 'src/integrations/notifications/linux_monitor.rs',
     '            && sender == owner\n', '',
     'notification_monitor', 'monitor_signals_require_current_owner_for_closed_and_action'),
    ('notify-destination-guard-removed', 'src/integrations/notifications/linux_monitor.rs',
     '(destination == NAME || destination == owner)', 'true',
     'notification_monitor', 'monitor_notify_destination_must_be_well_known_or_current_unique_owner'),
    ('clipboard-default-on', 'src/contract/settings.rs', 'clipboard_history: false,', 'clipboard_history: true,',
     'kwin_phase5_clipboard', 'clipboard_default_off_has_no_watcher_thread_or_connection'),
    ('disable-keeps-watcher-enabled', 'src/integrations/clipboard/mod.rs',
     'let enabled = settings.clipboard_history;', 'let enabled = settings.clipboard_history || self.shared.enabled.load(Ordering::Acquire);',
     'kwin_phase5_clipboard', 'clipboard_disable_joins_watcher_and_closes_its_connection'),
    ('password-hint-ignored', 'src/integrations/clipboard/linux_wayland.rs', 'if rx.data == b"secret" {', 'if rx.data == b"secret" && false {',
     'kwin_phase5_clipboard', 'clipboard_sensitive_hint_and_validated_deny_patterns_skip_real_offers'),
    ('self-echo-guard-removed', 'src/integrations/clipboard/history/store.rs', 'if self.take_echo(hash, now_ms) {', 'if self.take_echo(hash, now_ms) && false {',
     'clipboard_core_history', 'echo_consumed_once'),
    ('pause-ignored', 'src/integrations/clipboard/mod.rs', '            && !self.paused.load(Ordering::Acquire)\n', '',
     'kwin_phase5_clipboard', 'clipboard_pause_discards_real_offers_and_resume_captures_only_future_changes'),
    ('dedupe-disabled', 'src/integrations/clipboard/history/store.rs',
     'if let Some(pos) = self.entries.iter().position(|e| e.hash == hash) {\n            let id',
     'if let Some(pos) = self.entries.iter().position(|e| e.hash == hash && false) {\n            let id',
     'clipboard_core_history', 'dedupe_promotes_keeps_id_and_pin'),
    ('pinned-dropped-on-clear', 'src/integrations/clipboard/history/store.rs', 'self.entries.retain(|e| e.pinned);', 'self.entries.retain(|_| false);',
     'clipboard_core_history', 'clear_keeps_pinned'),
    ('queue-cap-lowered', 'src/integrations/clipboard/mod.rs', 'if inner.pending.len() == 128 {', 'if inner.pending.len() == 2 {',
     '--lib', 'integrations::clipboard::tests::clipboard_added_queue_is_128_drops_oldest_and_disabled_drops_pending'),
]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--carryover', action='store_true')
    args = parser.parse_args()
    cases = CASES[:2] if args.carryover else CASES
    assert args.carryover or len(cases) >= 9
    evidence = ROOT / ('evidence/phase5/carryover-mutations' if args.carryover else 'evidence/phase5/mutations')
    evidence.mkdir(parents=True, exist_ok=True)
    watched = [p for tree in ['src-tauri/src', 'src-tauri/tests', 'ui']
               for p in (ROOT / tree).rglob('*')
               if p.is_file() and '__pycache__' not in p.parts]
    hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in watched}
    watched.extend([CRATE / 'Cargo.toml', CRATE / 'Cargo.lock', ROOT / 'scripts/phase5-mutations.py'])
    hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in watched}
    sources = {name: (CRATE / name).read_bytes() for _, name, *_ in cases}
    rows = []
    env = dict(os.environ, CARGO_BUILD_JOBS='6')
    with tempfile.TemporaryDirectory(prefix='phase5-mutations-', dir=os.environ['TMPDIR']) as directory:
        pristine = Path(directory)
        for name, data in sources.items():
            target = pristine / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        try:
            for label, name, old, new, target, expected in cases:
                path = CRATE / name
                text = sources[name].decode()
                assert text.count(old) == 1, (label, 'nonunique mutation anchor')
                try:
                    path.write_text(text.replace(old, new, 1))
                    log = evidence / f'{label}.log'
                    with log.open('w') as output:
                        result = subprocess.run(
                            ['nice', '-n', '10', 'cargo', 'test', '--locked', *(['--lib'] if target == '--lib' else ['--test', target]), expected, '--', '--exact'],
                            cwd=CRATE, env=env, stdout=output, stderr=subprocess.STDOUT, timeout=180)
                    output = log.read_text()
                    assert result.returncode != 0 and f'test {expected} ... FAILED' in output and 'could not compile' not in output, (label, output[-3000:])
                finally:
                    path.write_bytes(sources[name])
                    os.utime(path, None)
                    diff = subprocess.run(['diff', '-u', str(pristine / name), str(path)], capture_output=True, text=True)
                    (evidence / f'{label}.diff.log').write_text(diff.stdout + diff.stderr)
                    assert diff.returncode == 0, (label, 'restore differs')
                row = {'mutation': label, 'failedTests': [expected], 'restored': True}
                rows.append(row)
                print(json.dumps(row), flush=True)
        finally:
            for name, data in sources.items():
                (CRATE / name).write_bytes(data)
                os.utime(CRATE / name, None)
    with (evidence / 'restored-tests.log').open('w') as output:
        for _, _, _, _, target, expected in cases:
            result = subprocess.run(
                ['nice', '-n', '10', 'cargo', 'test', '--locked', *(['--lib'] if target == '--lib' else ['--test', target]), expected, '--', '--exact'],
                cwd=CRATE, env=env, stdout=output, stderr=subprocess.STDOUT, timeout=180)
            assert result.returncode == 0, ('restored test failed', expected)
    assert all(hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == digest for name, digest in hashes.items()), 'source changed during mutations'
    (evidence / 'source-hashes.json').write_text(json.dumps(hashes, indent=2, sort_keys=True) + '\n')
    (evidence / 'summary.json').write_text(json.dumps(rows, indent=2) + '\n')
    print(f'PASS: {len(rows)} compiling mutations killed; distinct named failures; every restoration diff-identical; restored named tests passed' + (' (carryover ONLY)' if args.carryover else ''))


if __name__ == '__main__':
    main()
