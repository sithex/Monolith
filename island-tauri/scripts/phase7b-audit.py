#!/usr/bin/env python3
"""Keep earlier audits unchanged; verify the one controller-approved test amendment."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
ROOT=Path(__file__).resolve().parents[1]
def main():
    before=json.loads(Path(sys.argv[1]).read_text())
    key='island-tauri/src-tauri/tests/kwin_phase7a_island.rs'
    path=ROOT/'src-tauri/tests/kwin_phase7a_island.rs'
    source=path.read_text()
    source=re.sub(r'^ *// Phase7b diagnostic begin\n.*?^ *// Phase7b diagnostic end\n','',source,flags=re.S|re.M)
    source,count=re.subn(r'            // Controller-approved pill exclusion begin:.*?            // Controller-approved pill exclusion end\.\n','',source,count=1,flags=re.S)
    assert count==1,'approved native pill exclusion not found'
    original,count=re.subn(r'            // Controller-approved Phase 7b amendment:.*?            let current: Vec<RoundedRect> = .*?\.unwrap\(\);\n','            let current = rect(s);\n',source,count=1,flags=re.S)
    assert count==1,'approved settle amendment not found'
    original=original.replace('region_rows(&current, mode, 1., true)','region_rows(&[current], mode, 1., true)',1)
    original,count=re.subn(r'    settled\(s, conn, window, IslandMode::Large\);\n    // Same production launcher and physical EIS path as the user\. No hidden tabs\..*?    assert_eq!\(actor_state\(s\)\["view"\], "launcher"\);\n','',original,count=1,flags=re.S)
    assert count==1,'approved real-launcher amendment not found'
    assert hashlib.sha256(original.encode()).hexdigest()==before['earlierTests'][key], '7a GUI changed outside the approved geometry/navigation amendments'
    before['earlierTests'][key]=hashlib.sha256(path.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory(prefix='phase7b-audit-',dir=os.environ['TMPDIR']) as temporary:
        adjusted=Path(temporary)/'before.json';adjusted.write_text(json.dumps(before))
        subprocess.run([sys.executable,str(ROOT/'scripts/phase3-audit.py'),str(adjusted)],check=True)
    frozen=json.loads((ROOT/'evidence/phase7b/resume-frozen-before.json').read_text())
    current=json.loads(subprocess.run([sys.executable,str(ROOT/'scripts/phase7a-audit.py')],capture_output=True,text=True,check=True).stdout)
    assert all(current.get(k)==v for k,v in frozen.items() if not k.startswith('scripts/phase7b-')),'frozen coexistence/SPEC/staging/earlier scripts changed'
    assert all(k.startswith('scripts/phase7b-') for k in current.keys()-frozen.keys()),'unexpected new earlier script'
    print('PASS: only the approved 7a GUI geometry/navigation amendment; other earlier tests/assertions and frozen sources unchanged')
if __name__=='__main__':main()
