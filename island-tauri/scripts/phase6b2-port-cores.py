#!/usr/bin/env python3
"""One-shot audited source port of the six Controls cores; refuses to overwrite any destination.
Only imports/module paths change (lib.rs -> mod.rs). Staging is read-only. Behaviour deviations
(tailscale `Online` unknown, saturating brightness counters, ...) are separate, named edits."""
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
CORES = {'mixer-core': 'mixer', 'brightness-core': 'brightness', 'quick-toggles-core': 'toggles',
         'system-tiles-core': 'tiles', 'agent-tracker-core': 'agents', 'updates-core': 'updates'}
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
    mod = dest.parent / 'mod.rs'
    if not mod.exists():
        mod.write_text(f'//! Phase 6b2 {app} (ported core; service adapter added separately).\npub mod core;\n')
    print(f'Ported {crate} -> {app}/core')
