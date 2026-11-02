#!/usr/bin/env python3
"""Exercise the real gate's preamble/exit trap under SIGTERM, without starting a build.
Foreground owned subprocess only; the exact heavyweight-command suffix is not executed.
"""
from pathlib import Path
import os,subprocess,tempfile
root=Path(__file__).resolve().parents[1]
text=(root/'scripts/phase6b1-gate.sh').read_text()
prefix=text.split('if [[ "$TIER" == full ]]; then',1)[0]
prefix=prefix.replace('ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"',f'ROOT="{root}"')
prefix=prefix.replace('EVID="$ROOT/evidence/phase6b1"','EVID="$ROOT/evidence/phase6b1/interruption-probe"')
with tempfile.TemporaryDirectory(prefix='phase6b1-interruption-',dir=os.environ['TMPDIR']) as scratch:
    path=Path(scratch)/'gate-exit-probe.sh';path.write_text(prefix+'\nkill -TERM $$\n')
    done=subprocess.run(['bash',str(path)],cwd=root,capture_output=True,text=True,timeout=60)
    output=done.stdout+done.stderr
    (root/'evidence/phase6b1/interruption-probe.log').write_text(output)
    assert done.returncode==143,(done.returncode,output)
    assert 'PASS: Phase 6b1 gate' not in output,output
    assert 'FAIL: Phase 6b1 gate' in output,output
print('PASS: SIGTERM exits 143 and the real gate trap never claims completion')
