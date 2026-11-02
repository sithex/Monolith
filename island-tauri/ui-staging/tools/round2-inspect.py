#!/usr/bin/env python3
"""Read-only source provenance/static audit; evidence writes only inside package."""
import hashlib
import json
import pathlib
import re
import subprocess
from datetime import datetime, timezone
ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / 'evidence/round2'
CORES = ['ics-core', 'scratchpad-core', 'downloads-core', 'mixer-core', 'brightness-core', 'notification-store', 'clipboard-history', 'mpris-core', 'lrc-lyrics', 'spectrum-bands', 'weather-core', 'power-core', 'bluetooth-core', 'module-registry']
def main():
    OUT.mkdir(parents=True, exist_ok=True)
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    paths = [ROOT.parent / 'staging' / name / 'src' / file for name in CORES for file in ['lib.rs', 'model.rs']]
    paths.append(ROOT.parent / 'ui/ipc/bindings.ts')
    records = []
    for path in paths:
        row = {'path': str(path.relative_to(ROOT.parent)), 'exists': path.is_file()}
        if path.is_file():
            row.update(sha256=hashlib.sha256(path.read_bytes()).hexdigest(), mtime=datetime.fromtimestamp(path.stat().st_mtime, timezone.utc).isoformat())
        records.append(row)
    provenance = json.dumps({'repoHead': head, 'recordedAt': datetime.now(timezone.utc).isoformat(), 'note': 'Read-only provenance. Props mirrored from bindings snapshot mtime 2026-10-31 14:24:27 -0400 and requested core lib/model public fields. Recheck generated contracts on port.', 'files': records}, indent=2) + '\n'
    if not (OUT / 'provenance.json').exists():
        (OUT / 'provenance.json').write_text(provenance)
    (OUT / 'current-provenance.json').write_text(provenance)
    checked = list((ROOT / 'src/components').glob('*.tsx')) + list((ROOT / 'src').glob('app-*.ts*'))
    forbidden = re.compile(r'@tauri-apps|\binvoke\(|\bfetch\(|navigator\.clipboard|\bsetInterval\(|#[0-9a-fA-F]{3,8}\b|\brgba?\(')
    for path in checked:
        assert not forbidden.search(path.read_text()), f'Forbidden component dependency/colour/IO: {path}'
    assert not re.search(r'#[0-9a-fA-F]{3,8}\b|\brgba?\(', (ROOT / 'src/app-styles.css').read_text())
    report = json.loads((OUT / 'mutations/summary.json').read_text())
    for row in report:
        assert row['exit'] != 0 and row['tests'] and row['restoreDiff'] == 'empty'
        assert hashlib.sha256((ROOT / row['file']).read_bytes()).hexdigest() == row['restoredSha256'], row['mutation']
        assert (OUT / 'mutations' / (row['mutation'] + '.restore.diff')).read_bytes() == b''
    print(f'PASS: source provenance recorded for {len(CORES)} cores + bindings; {len(checked)} source files pass no-IPC/IO/timer/colour-literal scan; {len(report)} mutation hashes and empty restore diffs match final files')
if __name__ == '__main__':
    main()
