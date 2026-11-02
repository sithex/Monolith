#!/usr/bin/env python3
"""Phase 6a compiling mutation evidence; every edit is restored and diff-checked.

Each row applies ONE deliberate behaviour change (it must compile), runs exactly one
named test, requires that test to FAIL by assertion (never a compile error), restores
the pristine bytes and diff-checks them. No size/count/time cap is ever removed: caps
are lowered. `--only LABEL[,LABEL]` runs a subset (diagnostic, never acceptance);
`--list` prints the rows. Phase 5's belt (scripts/phase5-mutations.py) is untouched.
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
EVIDENCE = ROOT / 'evidence/phase6a/mutations'
WATCHED_TREES = ['src-tauri/src', 'src-tauri/tests', 'ui']
WATCHED_FILES = ['src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'scripts/phase6a-mutations.py']

# (label, file under src-tauri, old text, new text, test target or '--lib', exact test name)
CASES = [
    # --- carried-over Phase 5 clipboard gaps ---
    ('clipboard-concealed-arm-renamed', 'src/integrations/clipboard/linux_wayland.rs',
     '"org.nspasteboard.concealedtype"\n                    |', '"org.nspasteboard.concealedtypeX"\n                    |',
     'kwin_phase5_clipboard', 'clipboard_concealed_and_transient_offers_are_skipped_and_counted_as_sensitive'),
    ('clipboard-read-cap-times-eight', 'src/integrations/clipboard/linux_wayland.rs',
     'if rx.data.len() + n > cap {', 'if rx.data.len() + n > cap * 8 {',
     'kwin_phase5_clipboard', 'clipboard_read_caps_drop_oversize_offers_and_a_following_offer_is_still_captured'),
    ('clipboard-persistence-mode-check-disabled', 'src/integrations/clipboard/persistence.rs',
     'metadata.mode() & 0o077 != 0', 'metadata.mode() & 0o000 != 0',
     '--lib', 'integrations::clipboard::tests::clipboard_persistence_load_rejects_group_or_other_readable_file'),
    # --- power ---
    ('power-name-lost-not-reported', 'src/integrations/power/upower.rs',
     '                    service.status.unavailable("name-lost");', '                    service.status.available("upower");',
     'power_service', 'power_name_lost_reports_unavailable_then_restart_recovers_without_restarting_the_service'),
    ('power-wrong-sender-accepted', 'src/integrations/power/upower.rs',
     'Some(owner) {\n        return;\n    }\n    let path', 'Some(owner) && false {\n        return;\n    }\n    let path',
     'power_service', 'power_ignores_signals_from_senders_that_are_not_the_upower_owner'),
    ('power-wrong-path-accepted', 'src/integrations/power/upower.rs',
     '(IFACE_DEVICE, DISPLAY_PATH) =>', '(IFACE_DEVICE, _) =>',
     'power_service', 'power_ignores_the_owners_signals_for_other_object_paths'),
    ('power-core-percentage-upper-bound', 'src/integrations/power/model.rs',
     '(0.0..=100.0).contains(&f)', '(0.0..=1000.0).contains(&f)',
     'power_core_model', 'percentage_boundaries'),
    # --- bluetooth ---
    ('bluetooth-name-lost-not-reported', 'src/integrations/bluetooth/bluez.rs',
     '                    service.status.unavailable("name-lost");', '                    service.status.available("bluez");',
     'bluetooth_service', 'bluetooth_name_lost_reports_unavailable_then_restart_recovers_without_restarting_the_service'),
    ('bluetooth-wrong-sender-accepted', 'src/integrations/bluetooth/bluez.rs',
     'Some(owner) {\n        return;\n    }\n    let path', 'Some(owner) && false {\n        return;\n    }\n    let path',
     'bluetooth_service', 'bluetooth_ignores_signals_from_senders_that_are_not_the_bluez_owner'),
    ('bluetooth-foreign-path-accepted', 'src/integrations/bluetooth/bluez.rs',
     'path == "/org/bluez" || path.starts_with("/org/bluez/")', 'path.starts_with("/")',
     'bluetooth_service', 'bluetooth_ignores_objects_outside_the_org_bluez_tree'),
    ('bluetooth-non-root-object-manager-accepted', 'src/integrations/bluetooth/bluez.rs',
     '            // Root-object guard: the ObjectManager lives at `/`.\n            if path != "/" {',
     '            // Root-object guard: the ObjectManager lives at `/`.\n            if path != "/" && false {',
     'bluetooth_service', 'bluetooth_object_manager_signals_are_accepted_only_from_the_root_object'),
    ('bluetooth-core-battery-upper-bound', 'src/integrations/bluetooth/model.rs',
     '(n <= 100).then_some(n as u8)', '(n <= 101).then_some(n as u8)',
     'bluetooth_core_model_tests', 'battery_percent_boundaries'),
    # --- weather ---
    ('weather-fetches-while-disabled', 'src/integrations/weather/mod.rs',
     '(true, Some(place)) => (shared.generation', '(_, Some(place)) => (shared.generation',
     'weather_service', 'weather_without_a_location_makes_no_request_whatever_else_happens'),
    ('weather-unit-ignored', 'src/integrations/weather/mod.rs',
     'TempUnit::Fahrenheit => celsius * 9.0 / 5.0 + 32.0,', 'TempUnit::Fahrenheit => celsius,',
     'weather_service', 'weather_fetches_once_when_a_location_is_set_and_unit_changes_need_no_request'),
    ('weather-core-temperature-upper-bound', 'src/integrations/weather/policy.rs',
     '(-100.0..=70.0).contains(&temp_c)', '(-100.0..=71.0).contains(&temp_c)',
     'weather_core', 'forecast_temperature_range_is_inclusive_and_rejects_outliers'),
]


def tree_hashes():
    paths = [p for tree in WATCHED_TREES for p in (ROOT / tree).rglob('*')
             if p.is_file() and '__pycache__' not in p.parts]
    paths.extend(ROOT / name for name in WATCHED_FILES)
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}


def cargo_test(target, expected, output):
    scope = ['--lib'] if target == '--lib' else ['--test', target]
    return subprocess.run(
        ['nice', '-n', '10', 'cargo', 'test', '--locked', *scope, expected, '--', '--exact'],
        cwd=CRATE, env=dict(os.environ, CARGO_BUILD_JOBS='6'),
        stdout=output, stderr=subprocess.STDOUT, timeout=420)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--only')
    parser.add_argument('--list', action='store_true')
    args = parser.parse_args()
    if args.list:
        for row in CASES:
            print(row[0], '->', row[5])
        return
    partial = bool(args.only)
    cases = [c for c in CASES if c[0] in args.only.split(',')] if partial else CASES
    assert cases and (partial or len(cases) >= 10), 'Phase 6a belt needs >= 10 mutations'
    assert len({c[5] for c in CASES}) == len(CASES) or partial, 'each row needs its own named test'
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    hashes = tree_hashes()
    sources = {name: (CRATE / name).read_bytes() for _, name, *_ in cases}
    rows = []
    with tempfile.TemporaryDirectory(prefix='phase6a-mutations-', dir=os.environ['TMPDIR']) as directory:
        pristine = Path(directory)
        for name, data in sources.items():
            target = pristine / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        try:
            for label, name, old, new, target, expected in cases:
                path = CRATE / name
                text = sources[name].decode()
                assert text.count(old) == 1, (label, 'nonunique or missing mutation anchor')
                try:
                    path.write_text(text.replace(old, new, 1))
                    log = EVIDENCE / f'{label}.log'
                    with log.open('w') as output:
                        result = cargo_test(target, expected, output)
                    output = log.read_text()
                    short = expected.rsplit('::', 1)[-1]
                    assert (result.returncode != 0 and f'test {expected} ... FAILED' in output
                            and 'could not compile' not in output and 'error[E' not in output), (label, output[-3000:])
                finally:
                    path.write_bytes(sources[name])
                    os.utime(path, None)
                    diff = subprocess.run(['diff', '-u', str(pristine / name), str(path)], capture_output=True, text=True)
                    (EVIDENCE / f'{label}.diff.log').write_text(diff.stdout + diff.stderr)
                    assert diff.returncode == 0, (label, 'restore differs')
                row = {'mutation': label, 'failedTests': [expected], 'restored': True}
                rows.append(row)
                print(json.dumps(row), flush=True)
        finally:
            for name, data in sources.items():
                (CRATE / name).write_bytes(data)
                os.utime(CRATE / name, None)
    with (EVIDENCE / ('restored-tests-partial.log' if partial else 'restored-tests.log')).open('w') as output:
        for _, _, _, _, target, expected in cases:
            result = cargo_test(target, expected, output)
            assert result.returncode == 0, ('restored test failed', expected)
    assert tree_hashes() == hashes, 'source changed during mutations'
    if partial:
        print(f'PARTIAL (not acceptance): {len(rows)} mutations killed and restored')
        return
    (EVIDENCE / 'source-hashes.json').write_text(json.dumps(hashes, indent=2, sort_keys=True) + '\n')
    (EVIDENCE / 'summary.json').write_text(json.dumps(rows, indent=2) + '\n')
    print(f'PASS: {len(rows)} compiling mutations killed; distinct named failures; every restoration diff-identical; restored named tests passed')


if __name__ == '__main__':
    main()
