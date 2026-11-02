# VERIFICATION — bluetooth-core crate

Location: `island-tauri/staging/bluetooth-core/`.
Scope: pure core behind SPEC §3.5's "Bluetooth" row and §0.5's quick
toggles. BlueZ D-Bus values (`ObjectManager.GetManagedObjects` +
`InterfacesAdded` / `InterfacesRemoved` / `PropertiesChanged`) arrive already
converted to `serde_json::Value`; nothing in this crate reads a clock or a
bus. Platform-free: no IO, no network, no threads, no clock, no `unsafe`.
Dependencies: serde (derive) + serde_json only. Never panics on any input
(pseudo-random fuzz test).

Upstream facts (from bluez/bluez master doc/org.bluez.Device.rst and
org.bluez.Battery.rst):
- Device interface `org.bluez.Device1`, object path
  `[prefix]/{hci0,hci1,...}/dev_{BDADDR}`. Properties used: Address 's'
  (readonly), Name 's' (optional; docs say always display Alias instead),
  Alias 's', Icon 's' (optional), Paired 'b', Bonded 'b', Connected 'b',
  Trusted 'b', Blocked 'b', Adapter 'o', RSSI 'n' int16 (optional),
  ServicesResolved 'b', UUIDs 'as' (optional).
- Battery interface `org.bluez.Battery1` on the same device path: Percentage
  'y' byte (optional), 0..=100.
- Adapter interface `org.bluez.Adapter1` at `[prefix]/hciN`: Powered 'b',
  Discovering 'b', Alias 's'.
- Input model: D-Bus values arrive as `serde_json::Value`; an object path is
  a JSON string; `GetManagedObjects` = JSON object `{ path: { interface: {
  prop: value } } }`.

## Gate commands — final state (all green, under `nice -n 10`)

```
$ nice -n 10 cargo generate-lockfile
    Updating crates.io index
     Locking 11 packages to highest compatible versions
exit=0

$ nice -n 10 cargo fmt --check
(clean — no output)
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking bluetooth-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/bluetooth-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s
exit=0

$ nice -n 10 cargo test --locked
   Compiling bluetooth-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/bluetooth-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.20s
     Running unittests src/lib.rs (target/debug/deps/bluetooth_core-ada429c7ccde6071)

running 4 tests
test tests::battery_parse_boundaries ... ok
test tests::display_name_fallback_direct ... ok
test tests::object_path_validation ... ok
test tests::rssi_parse_boundaries ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-8d5ada4507c881e9)

running 1 test
test never_panics_and_invariants_hold ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

     Running tests/model_tests.rs (target/debug/deps/model_tests-8ac366eed189ff1c)

running 26 tests
test adapter_props_changed_and_invalid_clear ... ok
test devices_without_valid_address_skipped_and_counted ... ok
test default_adapter_selection ... ok
test display_name_alias_blank_falls_back_to_name ... ok
test battery_percent_boundaries ... ok
test interfaces_added_non_object_or_invalid_path_counted ... ok
test interfaces_added_before_adapter_exists ... ok
test interfaces_added_skips_addressless_device_and_counts ... ok
test interfaces_removed_unknown_or_invalid_path_counted ... ok
test invalid_paths_rejected_in_load ... ok
test invalidated_connected_clears_to_false ... ok
test load_non_object_returns_error_and_leaves_model_unchanged ... ok
test load_full_blob ... ok
test load_replaces_old_state_and_resets_ignored ... ok
test ordering_connected_paired_name_path ... ok
test properties_changed_invalid_clears_field_not_stale ... ok
test properties_changed_only_present_keys_change ... ok
test removing_battery1_keeps_device ... ok
test removing_device1_removes_device_even_if_battery_remains ... ok
test rssi_boundaries ... ok
test starts_empty ... ok
test serde_camelcase_roundtrip_and_no_alias ... ok
test summary_connected_names_capped_at_three ... ok
test unknown_path_and_interface_changes_ignored_and_counted ... ok
test summary_lowest_battery_over_connected ... ok
test unknown_property_keys_are_not_counted ... ok

test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests bluetooth_core

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit=0
```

Total: 31 tests (4 unit + 1 fuzz + 26 model), all green.

## Mutation checks (all actually run on the current code)

Method: snapshot pristine tree to scratch (`rsync --exclude=target`), apply
one breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record
failing test names, restore pristine (touching restored sources so cargo
rebuilds), then `diff -r --exclude=target <pristine> .` verified empty for
every mutation. All seven restored identical, and a final full gate re-run on
the restored tree is green (above). Every mutation compiled (a non-compiling
mutation would not count).

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | Invalid `Connected` value keeps the stale value (update only on a valid bool) | `invalidated_connected_clears_to_false`, `properties_changed_invalid_clears_field_not_stale` |
| 2 | Removing `Device1` only clears the battery instead of removing the device | `removing_device1_removes_device_even_if_battery_remains` |
| 3 | Alias used even when blank (no fallback to Name/Address) | `display_name_alias_blank_falls_back_to_name`, `tests::display_name_fallback_direct` |
| 4 | Battery bounds widened to 255 (101 accepted) | `battery_percent_boundaries`, `tests::battery_parse_boundaries` |
| 5 | Ordering: paired-first becomes the primary key instead of connected-first | `ordering_connected_paired_name_path` |
| 6 | Unknown-path `Device1` change no longer increments `ignored_count` | `unknown_path_and_interface_changes_ignored_and_counted`, `load_replaces_old_state_and_resets_ignored` |
| 7 | `default_adapter` no longer falls back to the first unpowered adapter | `default_adapter_selection` |

## Decisions

- **Address validity**: a device's `Address` is "valid" when it is a JSON
  string (the documented `'s'` type). No MAC-format validation is applied;
  BlueZ always sends well-formed addresses and the spec only distinguishes
  present/absent/typed.
- **displayName**: Alias when non-blank, else Name when non-blank, else
  Address. "Blank" means empty or whitespace-only after `trim()`.
- **ignored_count() semantics**: exactly these events count — invalid path
  keys and malformed interface maps in `load_managed_objects` / `interfaces_added`;
  devices whose `Device1` lacks a valid Address at load or add time; and
  `properties_changed` calls whose path is unknown for that interface or whose
  interface is unknown. Unknown *property* keys inside a known interface do
  not count, and unknown interfaces in `load`/`interfaces_added` do not count
  (they are normal BlueZ noise). A successful `load_managed_objects` replaces
  the whole model and resets `ignored_count` (fresh world).
- **Battery**: only integers 0..=100 (`as_u64`, `<= 100`). 101+, negatives,
  floats (`50.0`), strings, bools and null → `None`.
- **RSSI**: only integers representable in `i16` (`try_from`). 40000,
  -40000, floats, strings → `None`.
- **properties_changed scope**: changes apply only when the path already holds
  a record of the matching kind (`Device1`/`Battery1` need a device record;
  `Adapter1` needs an adapter record); otherwise the change is ignored and
  counted. Only keys present in `changed` change; an invalid-typed value or an
  invalidated key clears the field to its absent value (`None`/`false`), never
  keeping stale data.
- **Removal semantics**: removing `Device1` removes the whole device record
  even if `Battery1` remains; removing only `Battery1` clears the battery to
  `None`. Removing from an unknown or invalid path is ignored and counted.
- **Announcement order**: a device need not sit under an existing-or-later
  adapter path; only path syntax is validated. `interfaces_added` may arrive
  before its adapter, and battery-only interfaces may arrive before `Device1`
  (a hidden record holds the battery until the device appears).
- **Serde wiring**: `BtDevice`, `BtAdapter` and `BtSummary` serialize with
  camelCase field names (`displayName`, `batteryPercent`, `poweredAny`,
  `connectedCount`, `connectedNames`, `lowestBatteryPercent`, ...) and accept
  no aliases; `#[serde(default)]` lets partial objects deserialize (which is
  what the no-alias test exploits — snake_case keys are ignored, fields fall
  back to defaults).
- **Adapter Alias**: a plain `String` defaulting to `""`; an absent/invalid/
  invalidated Alias clears to `""`.
- **Un-exposed properties**: `Bonded`, `ServicesResolved` and `UUIDs` are
  parsed and tracked internally (so invalid values clear per the rules) but
  are not part of the public `BtDevice` struct requested for this core.
- **`default_adapter`**: first powered adapter in path order, else the first
  adapter, else `None`.

## UNVERIFIED

- **Live D-Bus wiring** — this crate is the pure core; the wiring (`zbus`
  reads of `ObjectManager.GetManagedObjects`, `InterfacesAdded` /
  `InterfacesRemoved` / `PropertiesChanged`, conversion to
  `serde_json::Value`, and the Tauri `BtModel` consumer) lives in `src-tauri`
  and is out of scope here.
- **Real-world BlueZ payloads** — pinned by synthetic inputs; the fuzz
  generator exercises malformed values but not a captured real-bus corpus.
- **MAC-format addresses** — not validated (see Decisions); a weird but
  string-typed address is treated as valid.
- **Announcement-order edge cases** — arbitrary order is handled and fuzzed,
  but not checked against a live bus capture.
