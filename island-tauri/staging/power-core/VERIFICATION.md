# VERIFICATION — power-core crate

Location: `island-tauri/staging/power-core/`.
Scope: pure core behind SPEC §3.5's "Battery/power" and "Battery alerts" rows.
UPower D-Bus values (`DisplayDevice` `PropertiesChanged` + root `OnBattery`)
arrive already converted to `serde_json::Value`; nothing in this crate reads a
clock. Platform-free: no IO, no network, no threads, no clock, no `unsafe`.
Dependencies: serde (derive) + serde_json only. Never panics on any input
(pseudo-random fuzz test). "Unavailable values are `null`, never fabricated
zeros (existing stats rule)".

UPower DisplayDevice facts (from upower.freedesktop.org/docs/Device.html):
Percentage 'd' (0..100), State 'u' 0 Unknown / 1 Charging / 2 Discharging / 3
Empty / 4 Fully charged / 5 Pending charge / 6 Pending discharge, TimeToEmpty /
TimeToFull 'x' seconds with 0 = unknown, IsPresent 'b', root `OnBattery`
'b'.

## Gate commands — final state (all green)

```
$ nice -n 10 cargo generate-lockfile
    Updating crates.io index
     Locking 11 packages to highest compatible versions
exit=0

$ nice -n 10 cargo fmt --check
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking power-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/power-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s
exit=0

$ nice -n 10 cargo test --locked
   Compiling power-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/power-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.19s
     Running unittests src/lib.rs (target/debug/deps/power_core-272b19223659041d)

running 2 tests
test tests::percentage_from_f64_rejects_nonfinite_and_out_of_range ... ok
test tests::value_as_nonneg_integer_edge_cases ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/alerts.rs (target/debug/deps/alerts-7703d02a0f7ae7a5)

running 19 tests
test charging_rearms_low_and_critical ... ok
test config_validation_errors ... ok
test critical_fires_after_low_independently ... ok
test critical_fires_at_exactly_threshold ... ok
test critical_rearms_only_at_threshold_plus_hysteresis ... ok
test critical_takes_priority_over_low_in_same_update ... ok
test default_config_is_low20_critical10_full100_hysteresis2 ... ok
test disappear_reappear_does_not_refire_without_rearm ... ok
test discharging_rules_include_unknown_with_onbattery_true ... ok
test full_fires_at_exactly_full_while_charging ... ok
test full_fires_on_fully_charged_even_below_100 ... ok
test full_rearms_only_at_full_minus_hysteresis ... ok
test full_rearms_when_discharging_starts ... ok
test jump_30_to_8_emits_critical_only_and_low_not_until_rearm ... ok
test low_fires_once_at_threshold_while_discharging ... ok
test nothing_fires_when_absent_or_percentage_none ... ok
test oscillation_within_hysteresis_fires_once ... ok
test pending_discharge_counts_as_discharging ... ok
test percentage_none_leaves_armed_state_untouched ... ok

test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/fuzz.rs (target/debug/deps/fuzz-0dbf973786df8b9a)

running 1 test
test never_panics_and_alerts_require_intervening_rearm ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s

     Running tests/model.rs (target/debug/deps/model-eba469f01e3f5d8f)

running 13 tests
test applies_full_display_device_map ... ok
test fractional_percentage_accepted ... ok
test invalid_state_value_clears_to_unknown_after_valid ... ok
test invalid_value_clears_field_not_stale ... ok
test on_battery_true_false_and_invalid ... ok
test partial_changed_keeps_unlisted_fields ... ok
test percentage_boundaries ... ok
test percentage_rejects_non_numbers ... ok
test snapshot_wires_camelcase_and_no_alias ... ok
test starts_absent ... ok
test time_to_empty_zero_negative_and_noninteger_are_none ... ok
test time_to_full_zero_negative_and_noninteger_are_none ... ok
test times_accept_integers_and_integral_floats ... ok

test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/parse_state.rs (target/debug/deps/parse_state-a647fbf23594860c)

running 2 tests
test maps_all_six_states ... ok
test rejects_out_of_range_and_other_types ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests power_core

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit=0
```

Total: 37 tests (2 unit + 19 alerts + 1 fuzz + 13 model + 2 parse_state), all green.

## Mutation checks (all actually run on the current code)

Method: snapshot pristine tree to scratch (`--exclude=target`), apply one
breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record failing
test names, restore pristine (touching restored sources so cargo rebuilds),
then `diff -r --exclude=target <pristine> .` verified empty for every
mutation. All seven restored identical, and a final full gate re-run on the
restored tree is green (above). Every mutation compiled (a non-compiling
mutation would not count; mutation 7 was first written as a non-compiling arm
type mismatch and re-written as the compiling behavior change below).

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | Hysteresis ignored on re-arm (`critical + hysteresis` → `critical`, `low + hysteresis` → `low`, `full - hysteresis` → `full` in the re-arm rules) | `oscillation_within_hysteresis_fires_once`, `critical_rearms_only_at_threshold_plus_hysteresis`, `full_rearms_only_at_full_minus_hysteresis` |
| 2 | Critical no longer consumes the pending Low (`self.low_armed = false;` removed from the Critical-fire branch) | `jump_30_to_8_emits_critical_only_and_low_not_until_rearm`, `critical_takes_priority_over_low_in_same_update`, `critical_fires_at_exactly_threshold`, `critical_rearms_only_at_threshold_plus_hysteresis` |
| 3 | State 4 mapped to Charging instead of FullyCharged | `maps_all_six_states`, `full_fires_on_fully_charged_even_below_100` |
| 4 | TimeToEmpty/TimeToFull 0 kept as `Some(0)` instead of `None` | `time_to_empty_zero_negative_and_noninteger_are_none`, `time_to_full_zero_negative_and_noninteger_are_none` |
| 5 | Invalid `PropertiesChanged` value keeps the stale value (update only on parse success) | `invalid_value_clears_field_not_stale`, `invalid_state_value_clears_to_unknown_after_valid` |
| 6 | Alerts fire while `present == false` (present guard removed) | `nothing_fires_when_absent_or_percentage_none`, `never_panics_and_alerts_require_intervening_rearm` (fuzz invariant) |
| 7 | `OnBattery` invalid value keeps the stale flag (update only on bool) | `on_battery_true_false_and_invalid` |

## Decisions

- **Percentage**: only finite values inside the inclusive 0..=100 range are
  kept (`parse_percentage`). `-1`, `101`, strings, bools and null → `None`.
  NaN/inf cannot be represented by `serde_json::Number` (`Number::from_f64`
  rejects non-finite values), so the `is_finite()` guard is defensive; it is
  unit-tested directly through `percentage_from_f64` (NaN, ±inf).
- **Times** (`TimeToEmpty`/`TimeToFull`): JSON integers and integral floats are
  accepted; 0 (unknown), negative, non-integer and non-numeric values → `None`.
  Nothing is fabricated: an absent/unknown time is `null`, never a zero.
- **State**: `parse_state` accepts integer JSON numbers 0..=6 only
  (`as_u64`); 7, negatives, floats (even `6.0`), strings, bools and arrays →
  `None`.
- **PropertiesChanged semantics**: only keys present in the map change; a key
  with an invalid value is cleared to its absent value (`None`, `Unknown`, or
  `false` for `IsPresent`) rather than keeping the stale value; missing keys
  are untouched. `OnBattery` is a root-object property applied via
  `apply_on_battery` (a non-boolean clears it to `None`); it is ignored inside
  the DisplayDevice map.
- **Alert fire rules**: Low/Critical fire while discharging (state `Discharging`
  / `PendingDischarge`, or `Unknown` with `OnBattery == true`) at `≤ low` /
  `≤ critical`; Full fires on `FullyCharged` (regardless of percentage) or
  `Charging`/`PendingCharge` at `≥ full`. At most one alert per update,
  priority Critical > Low > Full. A Critical fire consumes the pending Low, so
  a 30% → 8% jump emits Critical only and Low does not fire afterwards until
  re-armed.
- **Re-arm rules** (gated on the fire condition being currently false): Low/
  Critical re-arm when percentage reaches threshold + hysteresis or charging
  starts; Full re-arms when percentage drops to full − hysteresis or charging
  starts. The gating is what makes a persistent `FullyCharged` state fire Full
  exactly once (pinned by `full_fires_on_fully_charged_even_below_100`); the
  Low/Critical re-arm preconditions already imply the fire condition is false,
  so gating is redundant there.
- **Absence**: nothing fires while `present == false` or `percentage` is
  `None`, and neither re-arms anything; a device disappearing then reappearing
  never re-fires an alert whose condition never re-armed (pinned by
  `disappear_reappear_does_not_refire_without_rearm`).
- **Config validation**: threshold range checks (0..=100, which also rejects
  NaN/inf) run before the `critical < low < full` ordering check; hysteresis is
  validated separately against 0..=10. Typed `AlertError` variants distinguish
  invalid thresholds from invalid hysteresis.
- **Serde wiring**: `PowerSnapshot` (and `ChargeState`/`AlertKind`) serialize
  with camelCase field names (`onBattery`, `timeToEmptyS`, `timeToFullS`);
  no `#[serde(alias)]` is used, so snake_case keys do not populate fields
  (pinned by `snapshot_wires_camelcase_and_no_alias`). Missing keys on
  deserialize fall back to the absent defaults via `#[serde(default)]`.
- **No clock**: this crate never reads a clock; `TimeToEmpty`/`TimeToFull`
  are carried as plain seconds data in the snapshot.

## UNVERIFIED

- **Live D-Bus / UPower wiring** — this crate is the pure core; the wiring
  (`zbus` reads, `PropertiesChanged`/`OnBattery` conversion to
  `serde_json::Value`, `SystemAlert` production into NotificationStore) lives
  in `src-tauri` and is out of scope here.
- **Real-world device quirks** — pinned by synthetic inputs; the variety of
  real UPower payloads is exercised via fuzz but not against a captured corpus.
- **NaN/inf percentages** — unrepresentable through `serde_json::Value`
  (`Number::from_f64` rejects non-finite values), so the Value-level parser can
  never observe them; the f64-level guard is unit-tested directly, and the
  fuzz generator cannot synthesize them either.

## Follow-up after controller audit

Controller audit found one test gap: removing `|| is_discharging` from the Full
re-arm condition in `AlertEngine::update` (leaving only `pct <= full -
hysteresis`) survived every pre-existing test. The existing discharging re-arm
test (`full_rearms_when_discharging_starts`) used 80% — far below
`full - hysteresis` (98) — so the percentage clause alone re-armed it; the gap
is discharging *inside* the hysteresis band (e.g. 99%).

### Changes (tests only; library behavior untouched)

Two named tests appended to `tests/alerts.rs`:

- `full_rearms_when_discharging_starts_inside_hysteresis_band`: FullyCharged at
  100% → Full fires; Discharging at 99% (inside the band, > 98) → nothing, but
  discharging re-arms; Charging at 99% → nothing; FullyCharged at 100% → Full
  fires again.
- `full_does_not_refire_after_charging_dip_within_hysteresis_band`: Charging at
  100% → Full fires; Charging dip to 99% (inside the band, no discharging) →
  no re-arm; returning to FullyCharged at 100% → does NOT fire again.

Test count: 37 → 39.

### Gate — final restored state (all green, under `nice -n 10`)

```
$ nice -n 10 cargo fmt --check
(clean)

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking power-core v0.1.0 (.../power-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s

$ nice -n 10 cargo test --locked
running 2 tests  -> ok
running 21 tests -> ok (alerts: 21, incl. the two new tests)
running 1 test  -> ok (fuzz)
running 13 tests -> ok (model)
running 2 tests  -> ok (parse_state)
running 0 tests  -> ok (doc-tests)
```

Total: 39 tests, all green.

### Mutation check (actually run on the current code)

- Snapshot taken AFTER adding the two tests (pristine final state) to scratch
  (`--exclude=target --exclude=Cargo.lock`).
- Breakage applied: Full re-arm condition `(pct <= full - hysteresis ||
  is_discharging)` → `(pct <= full - hysteresis)` (compiling behavior change).
- `nice -n 10 cargo test --locked --no-fail-fast` result:

```
test full_does_not_refire_after_charging_dip_within_hysteresis_band ... ok
test full_rearms_when_discharging_starts_inside_hysteresis_band ... FAILED
failures:
    full_rearms_when_discharging_starts_inside_hysteresis_band
     (panicked at tests/alerts.rs:367 — the final "Full fires again" assert)
test result: FAILED. 20 passed; 1 failed
```

  Exactly the new named test fails; every other test (incl. the companion
  percentage-band test and the fuzz invariant) still passes. Reverted and
  diff-checked identical: `diff -r --exclude=target --exclude=Cargo.lock
  <snapshot> .` empty, restored sources touched before the final gate re-run
  (green above).
