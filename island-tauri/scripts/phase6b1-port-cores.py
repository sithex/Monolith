#!/usr/bin/env python3
"""One-shot audited source port; refuses to overwrite any existing destination.
Only imports/module paths change. Staging is read-only.
"""
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
CORES = {'timer-core': 'timer', 'scratchpad-core': 'scratchpad',
         'downloads-core': 'downloads', 'ics-core': 'calendar'}
for crate, app in CORES.items():
    source = ROOT / 'staging' / crate
    dest = ROOT / 'src-tauri/src/integrations' / app / 'core'
    dest.mkdir(parents=True, exist_ok=True)
    module = f'crate::integrations::{app}::core::'
    for path in sorted((source / 'src').glob('*.rs')):
        text = path.read_text().replace('crate::', module)
        target = dest / ('mod.rs' if path.name == 'lib.rs' else path.name)
        with target.open('x') as output:
            output.write(text)
    for path in sorted((source / 'tests').glob('*.rs')):
        text = path.read_text().replace(crate.replace('-', '_') + '::',
               f'hermes_island_lib::integrations::{app}::core::')
        target = ROOT / 'src-tauri/tests' / f'{app}_core_{path.name}'
        with target.open('x') as output:
            output.write(text)
    print(f'Ported {crate} source and tests to {app}/core (imports only)')
