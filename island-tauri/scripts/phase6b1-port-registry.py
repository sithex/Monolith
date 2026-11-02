#!/usr/bin/env python3
"""Port the accepted registry into the contract. Staging stays byte-identical.
Additive new app configs + specta metadata are applied only to the new port.
"""
from pathlib import Path
root=Path(__file__).resolve().parents[1]
text=(root/'staging/module-registry/src/lib.rs').read_text()
text=text.replace('pub const KNOWN_IDS: [&str; 7]', 'pub const KNOWN_IDS: [&str; 11]')
text=text.replace('    "music",\n];','    "music",\n    "timer",\n    "scratchpad",\n    "downloads",\n    "calendar",\n];')
text=text.replace('pub enum ModuleConfig {','pub enum ModuleConfig {\n    Timer {},\n    Scratchpad {},\n    Downloads {},\n    Calendar {},')
text=text.replace('        match self {\n            Self::Terminal', '        match self {\n            Self::Timer {} => Some("timer"),\n            Self::Scratchpad {} => Some("scratchpad"),\n            Self::Downloads {} => Some("downloads"),\n            Self::Calendar {} => Some("calendar"),\n            Self::Terminal',1)
text=text.replace('                equalizer_bands: 16,\n            },\n        ];','                equalizer_bands: 16,\n            },\n            ModuleConfig::Timer {},\n            ModuleConfig::Scratchpad {},\n            ModuleConfig::Downloads {},\n            ModuleConfig::Calendar {},\n        ];')
# Keep the accepted version-1 subset/import semantics. Old files with no registry migrate
# from flat keys into new defaults. Explicit imported subsets stay explicit subsets.
text=text.replace('JsonValue','ModuleJsonValue')
for shape in ['Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize',
              'Clone, Debug, PartialEq, Serialize, Deserialize',
              'Clone, Debug, PartialEq, Serialize']:
    text=text.replace('#[derive('+shape+')]', '#[derive('+shape+', specta::Type)]')
target=root/'src-tauri/src/contract/modules.rs'
with target.open('x') as out:out.write(text)
print(target)
