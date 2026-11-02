#!/usr/bin/env python3
"""Run deliberate, one-at-a-time defects; require named assertion failures.
All writes are confined to this package. A finally block restores exact bytes.
"""
import difflib
import hashlib
import json
import pathlib
import subprocess
import sys
ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / 'evidence/round2/mutations'
MUTANTS = [
    ('move-clamp', 'src/arrangement.ts', 'visible.length - 1', 'visible.length', 'last visible past end with trailing hidden is a no-op'),
    ('id-64-rejected', 'src/arrangement.ts', '.length > 64', '.length >= 64', 'accepts exactly 64-byte module id'),
    ('id-65-accepted', 'src/arrangement.ts', '.length > 64', '.length > 65', 'rejects exactly 65-byte module id'),
    ('locked-disable', 'src/arrangement.ts', "if (item.locked) return { ...next, error: 'Locked' };", "if (false) return { ...next, error: 'Locked' };", 'locked never disabled'),
    ('cancel-keeps-edits', 'src/arrangement.ts', 'items: copy(state.snapshot), snapshot: null', 'items: copy(state.items), snapshot: null', 'cancel restores the exact snapshot'),
    ('reenable-first', 'src/arrangement.ts', 'items.splice(last + 1, 0, { ...item, enabled: true });', 'items.splice(0, 0, { ...item, enabled: true });', 're-enabled goes last visible'),
    ('incomplete-permutation', 'src/arrangement.ts', 'order: state.items.map(i => i.id),', 'order: state.items.filter(i => i.enabled).map(i => i.id),', 'commit returns full-permutation'),
    ('ignore-reduced-motion', 'src/components/AppGrid.tsx', 'editing && !reduced', 'editing', 'reduced motion shows badges without wiggle'),
    ('hide-pending-state', 'src/components/ControlsApp.tsx', "const pending = service.available && service.value.pending !== null;", 'const pending = false;', 'pending toggle displays optimistic value distinctly'),
    ('overflow-off-by-one', 'src/components/AiDashboard.tsx', '+{update.overflow}</button>', '+{update.overflow + 1}</button>', 'chips overflow is supplied core count'),
    ('ignore-escape', 'src/components/AppGrid.tsx', "if (e.key === 'Escape')", "if (e.key === 'NeverEscape')", 'Escape cancels edit changes'),
    ('concealed-clipboard', 'src/components/ClipboardView.tsx', 'item.concealed === true', 'item.concealed === false', 'concealed mock is never rendered or actionable'),
    ('unknown-volume-zero', 'src/components/VolumeMixer.tsx', 'volume === null ? <p', 'false ? <p', 'volume None is unknown never zero'),
    ('part-openable', 'src/components/DownloadsApp.tsx', "item.state === 'ready' && !partial", "item.state === 'ready'", 'part files cannot open even if malformed ready state arrives'),
    ('hidden-notification', 'src/components/NotificationsView.tsx', 'item.hidden ? <p', 'false ? <p', 'hidden notification never renders body image or actions'),
    ('settings-lock-ignored', 'src/components/SettingsModules.tsx', "const locked = module.locked || module.id === 'terminal' || module.id === 'settings';", 'const locked = false;', 'locked home and Settings cannot be disabled'),
    ('equalizer-reduced-ignored', 'src/components/NowPlaying.tsx', 'reduced ? 0.35 :', 'false ? 0.35 :', 'reduced equalizer stays static across changing spectrum props'),
    ('calendar-secret-leak', 'src/components/CalendarApp.tsx', '<div className="calendar-body">', '<span>{(model as CalendarModel & { subscriptionUrl?: string }).subscriptionUrl}</span><div className="calendar-body">', 'calendar never renders secret URL even on malformed extra prop'),
    ('unmapped-weather-throws', 'src/components/QuickInfo.tsx', 'conditions[weather.code] ?? `Unknown condition · WMO ${weather.code}`', 'conditions[weather.code].toUpperCase()', 'unmapped WMO code uses generic icon and unknown text'),
    ('saving-shown-saved', 'src/components/ScratchpadApp.tsx', "model.save.status === 'saving' ?", 'false ?', 'save indicator is props-driven saving'),
    ('passive-action-enabled', 'src/components/NotificationsView.tsx', 'disabled={!item.canInvokeActions}', 'disabled={false}', 'passive actions disabled and empty clear disabled'),
    ('broken-artwork-checksum', 'src/app-mocks.ts', 'AAAAASUVORK5CYII=', 'AAAAASUVORK5CYIA=', 'artwork fixture PNG chunks have valid checksums and real dimensions'),
]
def main():
    OUT.mkdir(parents=True, exist_ok=True)
    baseline = subprocess.run(['nice', '-n', '10', 'npm', 'test'], cwd=ROOT, capture_output=True, text=True, timeout=120)
    (OUT / 'baseline.log').write_text(baseline.stdout + baseline.stderr)
    assert baseline.returncode == 0, 'Baseline must pass before mutations'
    rows = []
    selected = [m for m in MUTANTS if len(sys.argv) == 1 or m[0] in sys.argv[1:]]
    assert selected, 'No mutations selected'
    for name, file, old, new, test in selected:
        path = ROOT / file
        original = path.read_bytes()
        text = original.decode()
        assert old in text, f'mutation target missing: {name}'
        # The reduced-motion mutant deliberately changes animation AND the observable path.
        mutated = text.replace(old, new) if name == 'ignore-reduced-motion' else text.replace(old, new, 1)
        report = OUT / f'{name}.json'
        try:
            path.write_text(mutated)
            result = subprocess.run(['nice', '-n', '10', 'npm', 'test', '--', '-t', test, '--reporter=json', '--outputFile', str(report)], cwd=ROOT, capture_output=True, text=True, timeout=120)
            (OUT / f'{name}.log').write_text(result.stdout + result.stderr)
            assert report.exists(), f'No structured report: {name}'
            data = json.loads(report.read_text())
            failures = [a['fullName'] for suite in data['testResults'] for a in suite['assertionResults'] if a['status'] == 'failed']
            assert result.returncode != 0 and failures and any(test in t for t in failures), f'Mutant survived or failed without intended assertion: {name}'
        finally:
            path.write_bytes(original)
            restored = path.read_bytes()
            diff = ''.join(difflib.unified_diff(original.decode().splitlines(True), restored.decode().splitlines(True)))
            (OUT / f'{name}.restore.diff').write_text(diff)
            assert not diff and restored == original, f'Restore drift: {name}'
        row = {'mutation': name, 'file': file, 'tests': failures, 'exit': result.returncode, 'restoredSha256': hashlib.sha256(original).hexdigest(), 'restoreDiff': 'empty'}
        rows.append(row)
        print(f'KILLED {name}: {"; ".join(failures)} · restore diff empty', flush=True)
    (OUT / ('summary.json' if len(sys.argv) == 1 else 'selected-summary.json')).write_text(json.dumps(rows, indent=2) + '\n')
    print(f'{len(rows)}/{len(selected)} mutants killed by named tests; every restore byte-identical')
if __name__ == '__main__':
    main()
