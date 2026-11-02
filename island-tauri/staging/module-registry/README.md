# module-registry

Pure Rust library for SPEC §0.5(2): versioned, reorderable, per-module settings.
Only serde/serde_json dependencies. No filesystem, clipboard, sockets, IPC,
service activation, or application changes. All work is confined to this crate.

## Public API

- `Registry::defaults()` / `Default`: seven modules, current version 1.
- `Registry::new(version, modules)`: checked construction; subsets are legal.
- `modules()`, `get(&ModuleId)`, `version()`, `unknown_ids()`: read-only access.
- `reorder(&[ModuleId])`: exact permutation of every present ID (unknowns too).
  Valid calls rewrite vector order and assign contiguous `u32` order values.
- `set_enabled(&id, bool)`, `set_config(&id, ModuleConfig)`: validated, atomic.
- `normalized()`: new registry sorted by old order, then ID, numbered from zero.
- `export_json()`: validated, normalized, deterministic pretty JSON.
- `Registry::from_json(&str)`: bounded load returning `(Registry, ImportReport)`.
- `registry.import_json(&str)`: bounded atomic replacement, returning the report.
- `upgrade(serde_json::Value)`: checked in-memory version upgrade hook.
- `migrate_from_flat(&serde_json::Value)`: never-failing legacy migration,
  returning `(Registry, MigrationReport)` without modifying the source value.

Import/export accept at most 256 KiB of UTF-8 bytes (inclusive). Imports reject
unsupported versions, malformed JSON, invalid slugs, duplicate IDs, extra
contract fields, positional arrays, mismatched config types, and invalid values.
Direct serde loading also validates v1, but is not the bounded/upgrade API.
Call `from_json` or `import_json` for untrusted textual files. Direct `upgrade`
accepts an already-parsed in-memory value and does not impose a text size cap.

## Wire model and defaults

Registry: `{ "version": 1, "modules": [...] }`.
Entry: `{ "id": "music", "enabled": false, "order": 6, "config": {...} }`.
Known configs carry a camelCase `type` discriminator matching the module ID.

| ID | Enabled | Default config (in addition to type) |
|---|---|---|
| terminal | true | fontSize: 13.0 (8..=24) |
| monitor | true | empty |
| ai | true | empty |
| settings | true | empty |
| notifications | true | systemNotificationMirror: false, toastTtlMs: 6000.0 (1500..=20000), redactBodies: false, dnd: false |
| clipboard | false | historySize: 100.0 (10..=500), persist: false |
| music | false | lyrics: true, equalizer: true, equalizerBands: 16 (integer 8..=32) |

Initial order is the table order. Flat-compatible numeric configs stay `f64`,
including fractional history sizes. New module operations reject invalid/out-of-
range/non-finite values instead of clamping. IDs obey `[a-z][a-z0-9-]{0,31}`;
unknown valid slugs are preserved exactly. Invalid IDs are rejected, not repaired.

Unknown modules retain their stored enabled bit and opaque config during v1
round trips, but `ModuleEntry::is_active()` is always false for them. Consumers
MUST use that method rather than treating `enabled` as service authorization.
`set_enabled(unknown, true)` returns a typed error; setting false is allowed.
`ImportReport.unknown_ids` and `Registry::unknown_ids()` expose the unknowns.
Opaque JSON uses the local `JsonValue` type, whose numbers are `f64`, not
BigInt-sized integers. Numeric spelling/precision and whitespace are not a
byte-preserving JSON archive; opaque keys/strings/structure and ID bytes survive.
Opaque keys are data and may contain underscores; contract fields may not.

## Flat migration and additive integration

| Legacy key | Destination |
|---|---|
| hardwareMonitor | monitor.enabled |
| terminalFontSize | terminal.config.fontSize |
| notificationsEnabled | notifications.enabled |
| systemNotificationMirror | notifications.config.systemNotificationMirror |
| toastTtlMs | notifications.config.toastTtlMs |
| redactBodies | notifications.config.redactBodies |
| dnd | notifications.config.dnd |
| clipboardHistory | clipboard.enabled |
| clipboardHistorySize | clipboard.config.historySize |
| clipboardPersist | clipboard.config.persist |

All other keys (appearance, panel, battery, weather, arbitrary future keys, etc.)
are left alone and cloned into `MigrationReport.untouched`. Successfully mapped
keys are listed in `mapped_keys`. Wrong types and invalid numeric ranges retain
the corresponding module default and appear in `invalid_keys` AND `untouched`
with the original value; siblings still migrate independently. Non-object input
returns defaults with `input_problem`. Migration does not interpret nested
registries or create/enable unknown modules from arbitrary flat keys.

The controller should add a registry field to the application's existing
Settings, retaining every existing flat key exactly as-is. Do not replace the
flat Settings with this crate's Registry. Registry-only export does not export
unmapped flat settings; full-settings export remains the controller's job.
Prefer a valid existing registry over re-running flat migration. No such wiring
is performed by this staged crate.

## Version hook

Synthetic v0 uses the same entry/config shape but predates music. Upgrading
validates the entire old document, changes the version to 1, disables every
unknown module, and appends default disabled music if absent. Existing music is
not duplicated. No other absent module is invented. Imported output is normalized.
Future versions are errors, never silently downgraded. The hook is a pattern,
not evidence that a real historical v0 file exists.

## Verification

Run from this directory (all cargo processes are nice'd):

    python3 scripts/verify.py gates
    python3 scripts/verify.py mutations
    python3 scripts/verify.py gates

The gate helper runs lockfile generation, formatting, format check, strict
clippy and tests. The mutation helper saves the complete non-target tree under
`target/verification/pristine`, runs each compiling behavior breakage separately,
requires named failing tests, restores and touches source, then compares the
entire non-target tree using `diff -r` after EACH mutation. Detailed execution
logs live in ignored `target/verification/`; durable results are pasted into
VERIFICATION.md. It never operates on another crate.
