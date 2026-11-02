# VERIFICATION — module-registry crate

Location: `island-tauri/staging/module-registry/`.
Scope: pure versioned module settings, atomic imports/edits, and additive legacy
flat-settings migration. Only serde + serde_json; unsafe forbidden; no IO in the
library. Neighbouring crates and live settings are not changed.

## Gate commands — final state (all green)

Real output from `python3 scripts/verify.py gates`, run AFTER all eight mutation
restorations. Every cargo command, including the first lockfile generation and
every mutation test run, was launched under `nice -n 10`. First lock generation
preceded all compilation/testing; no sibling cargo process was touched.

```text
$ nice -n 10 cargo generate-lockfile
    Updating crates.io index
     Locking 11 packages to highest compatible versions
exit=0

$ nice -n 10 cargo fmt
exit=0

$ nice -n 10 cargo fmt --check
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking module-registry v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/module-registry)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.21s
exit=0

$ nice -n 10 cargo test --locked
   Compiling module-registry v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/module-registry)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.42s
     Running unittests src/lib.rs (target/debug/deps/module_registry-1db0e3cc447c6b73)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/registry.rs (target/debug/deps/registry-da58d3f00742cb24)

running 25 tests
test duplicate_ids_and_config_mismatch_rejected_by_constructor ... ok
test duplicate_envelope_fields_are_rejected ... ok
test import_success_replaces_instead_of_merging ... ok
test migration_non_objects_never_fail ... ok
test export_import_roundtrip_is_deterministic_and_reports_unknowns ... ok
test migration_unexpected_types_and_invalid_ranges_keep_defaults_and_original_values ... ok
test json_shape_snapshot_all_modules_camel_case ... ok
test normalization_orders_by_previous_order_then_id_and_is_idempotent ... ok
test music_bands_range_and_config_mismatch_are_atomic ... ok
test reorder_rejects_every_non_permutation_atomically ... ok
test reorder_includes_unknown_ids_and_sets_contiguous_orders ... ok
test upgrade_never_enables_unknown_modules ... ok
test validated_slug_boundaries_without_normalization ... ok
test version_zero_upgrade_adds_disabled_music_preserves_values_and_reports_version ... ok
test import_atomic_on_syntax_semantic_version_and_size_errors ... ok
test unknown_modules_preserved_verbatim_and_inert ... ok
test realistic_flat_fixture_migrates_all_corresponding_keys_without_loss ... ok
test set_enabled_known_and_missing_errors_are_atomic ... ok
test numeric_ranges_reject_nonfinite_and_preserve_fractional_legacy_values ... ok
test upgrade_rejects_future_versions_and_invalid_v0_before_editing ... ok
test unknown_payload_can_reuse_known_tag_without_interpretation ... ok
test unknown_nonfinite_payload_rejected_recursively ... ok
test strict_input_rejects_unknown_fields_aliases_arrays_and_wrong_numeric_types ... ok
test import_size_cap_is_inclusive_in_bytes ... ok
test export_size_cap_rejects_unimportable_document ... ok

test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests module_registry

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

Initial development note: the first test run had 24 passing tests and one
failure because an opaque `JsonValue::Number(f64)` fixture compared `99.0`
against serde_json's integer `99`. The fixture now explicitly uses `99.0` to
pin the chosen f64 wire domain. This is not a claim of lexical or arbitrary-
precision JSON preservation (see Decisions and UNVERIFIED).

## Mutation checks (all actually run)

Method: `python3 scripts/verify.py mutations` snapshots the entire crate except
`target/` and Python caches to `target/verification/pristine/`. The snapshot is
inside this crate to honor the strict write boundary. Each mutation changes
only `src/lib.rs`, runs `nice -n 10 cargo test --locked --no-fail-fast`, and must
fail its pinned named runtime test. A compile failure is NOT counted. Source
is restored by rewriting the pristine bytes and touching its mtime in a
`finally` block; each restoration is checked with:

```text
diff -r --exclude=target --exclude=__pycache__ target/verification/pristine .
exit=0
(empty output)
```

All eight compiled, produced exit 101 with the following failing test names,
and restored with empty diff / exit 0. No surviving or compile-only mutation is
counted. The final gate rerun above passed on the restored source. Only this
verification document was intentionally updated after the final full-tree diff.

| ID | Deliberate breakage | Actual failing tests | Restored diff |
|---|---|---|---|
| M1 | Clear existing modules before parsing an import | `import_atomic_on_syntax_semantic_version_and_size_errors` | empty, exit 0 |
| M2 | Bypass reorder's permutation guard | `reorder_includes_unknown_ids_and_sets_contiguous_orders`; `reorder_rejects_every_non_permutation_atomically` | empty, exit 0 |
| M3 | Drop unknown modules from current-version loads | `reorder_includes_unknown_ids_and_sets_contiguous_orders`; `unknown_modules_preserved_verbatim_and_inert`; `unknown_nonfinite_payload_rejected_recursively`; `unknown_payload_can_reuse_known_tag_without_interpretation`; `version_zero_upgrade_adds_disabled_music_preserves_values_and_reports_version` | empty, exit 0 |
| M4 | Change v0 migration to enable unknown modules | `upgrade_never_enables_unknown_modules` | empty, exit 0 |
| M5 | Remove enum `rename_all_fields = "camelCase"` | `json_shape_snapshot_all_modules_camel_case`; `upgrade_never_enables_unknown_modules`; `version_zero_upgrade_adds_disabled_music_preserves_values_and_reports_version`; `strict_input_rejects_unknown_fields_aliases_arrays_and_wrong_numeric_types` | empty, exit 0 |
| M6 | Accept non-finite numbers in opaque configs | `unknown_nonfinite_payload_rejected_recursively` | empty, exit 0 |
| M7 | Treat an unknown module's stored enabled bit as active | `unknown_modules_preserved_verbatim_and_inert` | empty, exit 0 |
| M8 | Stop mapping legacy clipboard persistence | `realistic_flat_fixture_migrates_all_corresponding_keys_without_loss` | empty, exit 0 |

Raw per-mutation test traces and machine-readable results are in ignored
`target/verification/M1.log` through `M8.log` and `mutations.json`. The helper
is retained in `scripts/verify.py` so the checks are reproducible. It only
deletes its own prior crate-local snapshot, never another process or crate.

## Decisions

These are design decisions for the controller to fold into SPEC; SPEC itself
was not edited.

1. **Additive integration, no flat-key churn.** The controller should add a
   `moduleRegistry` Settings field; retain every current flat field/name/type.
   This staged crate exports the registry only, not a replacement full Settings
   document. Whole-settings export/import and precedence belong to integration;
   prefer an already-valid registry over rerunning the flat migration. No IPC,
   filesystem writes or service startup live in this library.
2. **Version/schema.** `CURRENT_VERSION = 1`; registry fields are `version: u32`
   and `modules: Vec<ModuleEntry>`. Entries have `id`, `enabled: bool`,
   `order: u32`, `config`. Registry fields are private, read through accessors;
   `Registry::new` and serde loading validate the whole value. Entry fields are
   public for constructing candidates, never for mutating entries in a registry.
3. **IDs.** All IDs, including unknown imported IDs, must satisfy
   `[a-z][a-z0-9-]{0,31}`. Valid unknown slugs retain exact bytes; invalid IDs
   are rejected rather than normalized, lowercased, aliased or silently dropped.
   `ModuleId::new` and its deserializer enforce the same rule.
4. **Unknown modules remain inert.** v1 imports preserve their stored enabled
   bit and opaque config, report them via `ImportReport.unknown_ids`, and expose
   `Registry::unknown_ids()`. Consumers MUST use `ModuleEntry::is_active()`;
   it requires a known ID as well as enabled=true. Explicitly enabling an
   unknown ID is a typed `UnknownModule` error; disabling it is allowed. Neither
   flat migration nor v0 upgrade activates unknown modules. There is no registry
   insert/delete API beyond checked construction and whole-registry import.
5. **Wire types.** Contract structs are camelCase; known data-carrying configs
   use `type` tags and BOTH `rename_all` and `rename_all_fields`. Inputs deny
   unknown fields; custom object-only envelopes reject serde's positional array
   struct form. No aliases, asymmetric renames or BigInt-sized integer fields
   are used. `MAX_JSON_BYTES` and collection indices are internal usize values,
   never serialized fields. Migration/import reports are Rust-only metadata,
   not derived wire contracts.
6. **Opaque extension domain.** `ModuleConfig::Unknown(JsonValue)` serializes
   without a wrapper/tag. Local `JsonValue` has null/bool/f64/string/array/object
   forms; it avoids exposing serde_json::Value's i64/u64 shape in an eventual
   IPC type. Unknown config tags/keys/strings/structure survive, even when the
   tag happens to name a known module. Opaque keys are data, so underscores in
   them are legal. All opaque numeric leaves must be finite. Numbers follow
   f64 semantics, NOT arbitrary precision or exact numeric lexical spelling.
   Unknown ID bytes, not entire JSON source bytes, are preserved verbatim.
7. **Known config agreement.** A config's `type` must match its known ID;
   Unknown config is invalid for a known ID, and a typed known config is invalid
   for an unknown ID at checked construction/set_config. The untagged unknown
   fallback cannot smuggle malformed known configs through registry validation.
8. **Defaults/order.** terminal, monitor, ai, settings, notifications, clipboard,
   music in that order, numbered 0..6. First five enabled, clipboard and music
   disabled. Terminal fontSize=13.0; notifications systemNotificationMirror=false,
   toastTtlMs=6000.0, redactBodies=false, dnd=false; clipboard historySize=100.0,
   persist=false; music lyrics=true, equalizer=true, equalizerBands=16. Monitor,
   ai and settings currently have empty typed configs. There are no mandatory
   always-enabled modules or service-availability assumptions in this model.
9. **Numeric domains.** Preserve legacy f64 domains and fractional values:
   fontSize 8..=24, toastTtlMs 1500..=20000, historySize 10..=500. Music
   equalizerBands is a u32 integer 8..=32. Reject invalid/out-of-range/non-finite
   values on imports/config edits, never silently clamp. Invalid flat values
   fall back to the corresponding default and are reported with original data.
10. **Present-set semantics.** Subsets (even empty registries) are legal;
    imports replace rather than merge or append absent known modules. Reorder
    requires exactly one occurrence of EVERY present ID, unknowns included,
    rejects any omission/duplicate/foreign ID with `InvalidPermutation`, and
    leaves state unchanged on error. Successful reorder replaces vector order
    and assigns contiguous u32 orders. Missing setters return `MissingModule`.
11. **Normalization.** `normalized()` returns a new value, sorted by old order
    then ID, numbered 0..n-1, preserving config/enabled. Equal old orders are
    resolved deterministically by ID; no arithmetic on old u32 order values,
    so u32::MAX orders are valid. Export/import normalize; direct constructor
    and serde loading validate but retain supplied order values.
12. **Atomic bounded JSON.** `Registry::from_json` builds a candidate and report;
    `import_json(&mut self, ...)` replaces self only after the entire candidate
    passes. Cap is 256 KiB inclusive in UTF-8 BYTES, checked before parsing.
    Unsupported versions are errors. Export uses validated normalized pretty
    JSON, and also rejects output exceeding the cap. Direct serde loading and
    `upgrade(Value)` are in-memory APIs, not substitutes for bounded file input.
    Duplicate registry/entry envelope fields are rejected. Config contents are
    decoded via Value, so duplicate config/object keys have serde_json's usual
    last-value semantics (not a strict duplicate-key archival format).
13. **Legacy mapping (real keys read-only).** hardwareMonitor→monitor.enabled;
    terminalFontSize→terminal.fontSize; notificationsEnabled→notifications.enabled;
    systemNotificationMirror/toastTtlMs/redactBodies/dnd→notifications config;
    clipboardHistory→clipboard.enabled; clipboardHistorySize/clipboardPersist→
    clipboard config historySize/persist. No guessed music/AI/terminal enable keys
    are consumed. All other keys, including nested/future registries, appearance,
    panel, battery and weather settings, are untouched. The realistic fixture
    covers all current flat Settings fields plus one future key.
14. **Migration report/losslessness.** Every successful mapping is listed in
    `mapped_keys`; all other original key/value pairs are cloned to `untouched`.
    Invalid recognized values are also listed in `invalid_keys` and do not
    prevent valid siblings from migrating. Borrowed input is never modified;
    the controller retains the complete original flat settings. Non-objects
    yield defaults plus `input_problem`. Unknown flat keys are not interpreted
    as module definitions or enable commands. No nested registry is activated.
15. **Version-0 pattern.** Synthetic v0 shares the entry schema but predates
    music. Validate first, set version=1, force all unknown stored enabled bits
    false, append disabled default music only when absent, preserve other
    configs/enabled bits, and normalize the append path. Existing music is not
    duplicated; no other missing module is invented. Import reports
    `upgraded_from=Some(0)` and normalizes every upgraded result. Future versions
    are rejected, never silently downgraded. This is a tested evolution hook,
    not an assertion that actual historical v0 files existed.
16. **Conventions/scope.** Copied sibling lrc-lyrics edition 2021, rust-version
    1.85, MIT metadata, `[lints.clippy]` unwrap_used/expect_used deny, clippy.toml
    test allowance, `/target` gitignore and this verification layout. Only serde
    (derive) + serde_json direct deps; unsafe forbidden. Reproducibility helper
    IO is test tooling, not library IO. All writes, snapshots and logs stay under
    module-registry; no other crate, SPEC, skill or memory was edited; no git
    commit/push or process kill. Usage checks ran at start, during implementation
    and after verification; last observed codex Session was 71%, below the 95%
    stop threshold.

## UNVERIFIED

- Actual additive Settings/IPC/specta bindings and persistence integration.
  No generated TypeScript claim is made; that requires the controller's
  specta integration (this crate intentionally cannot depend on specta).
- UI controls, service activation, music rendering, and real user settings.
- A real historical v0 schema; the fixture is deliberately synthetic.
- Arbitrary-precision numeric/byte-for-byte opaque JSON archival (wire uses f64,
  pretty export normalizes order/whitespace, duplicate config keys collapse).
- Workload/corpus fuzzing and allocation-exhaustion behavior; tests are named,
  deterministic fixtures and boundary cases, not an unbounded stress campaign.
- Rust 1.85 toolchain execution: manifest follows the sibling's declared MSRV,
  while the gates above used the installed toolchain, not a pinned 1.85 runner.
