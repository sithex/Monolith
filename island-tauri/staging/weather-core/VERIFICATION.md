# VERIFICATION — weather-core crate

Location: `island-tauri/staging/weather-core/`.
Scope: pure Open-Meteo URLs, bounded JSON parsing, weather conditions/icons,
failure staleness and UTC quarter-hour policy for Phase 6. Only serde (derive)
and serde_json; unsafe forbidden; no IO, network, clock reads or threads in the
library. All intentional project writes and generated evidence are inside this
crate. The sibling notification-store was read only for conventions. SPEC was
read only with the requested matching-line search. No skills/memory edits,
commit/push, GUI interactions or process kills.

## Official documentation verification

Consulted before implementation:

- https://open-meteo.com/en/docs
- https://open-meteo.com/en/docs#api_documentation
- https://open-meteo.com/en/docs#json_return_object
- https://open-meteo.com/en/docs#hourly_parameter_definition
- https://open-meteo.com/en/docs#weather_variable_documentation
- https://open-meteo.com/en/docs#errors
- https://open-meteo.com/en/docs/geocoding-api
- https://open-meteo.com/en/docs/geocoding-api#json_return_object
- https://open-meteo.com/en/docs/geocoding-api#errors

Confirmed from extracted official documentation:

- Forecast `/v1/forecast`: `current` parameter selects variables;
  `temperature_unit=celsius` is supported (also the default); `timezone=auto`
  resolves the location's timezone.
- Response `current` is an object of chosen numeric weather variables, plus
  `time` and `interval`. Requested variable names are `temperature_2m`,
  `weather_code`, `is_day`; `is_day` is dimensionless 1=daylight, 0=night.
  Celsius is spelled `°C` in the temperature-unit documentation/example.
- Top-level `utc_offset_seconds` is an integer offset fixed at request time;
  `timezone` is a string timezone identifier. These are metadata, not a UTC
  fetch timestamp. `updatedAt` uses the caller's Unix seconds unchanged.
- Geocoding `/v1/search`: `results[]` objects use integer `id`, string `name`,
  numeric `latitude`/`longitude`, string `country`, snake_case `country_code`,
  string `admin1` and `timezone`. Empty optional fields may be omitted. The
  documented Berlin example explicitly shows all requested field names.
- Both endpoints document an HTTP 400 JSON object with `error: true` and
  string `reason`. This crate parses the body only, not HTTP status codes.
- The WMO table lists code **97 (Heavy thunderstorm)**, and the docs note most
  models report thunderstorms as 95 and 97 (96/99 only from hail models). 97 is
  adopted per controller decision. See Decisions below.

Not explicitly confirmable from the extracted static docs:

- `current_units` is NOT spelled out in the static JSON-return example/table
  (that example shows `hourly_units`). The requested `current_units` object
  with `temperature_2m`, `weather_code`, `is_day` is corroborated by an indexed
  official API response, not independently verified in a live response here:
  https://api.open-meteo.com/v1/forecast?latitude=46.2044&longitude=6.1432&current=temperature_2m,weather_code,is_day
- A second docs extraction with the selected current variables did not expose
  the dynamic response JSON. A direct urllib HTML fetch returned HTTP 403;
  browser fallback could not connect to the local Camofox service. No service
  was started or unrelated environment changed to work around that blocker.

All fixtures are **hand-written from the documentation**, not live captures;
`tests/fixtures/README.md` labels provenance and invented values explicitly.
No test performs a live request and no ignored real-call test exists.

## Gate commands — final state

Lockfile generation preceded every compilation/test command:

```text
$ nice -n 10 cargo generate-lockfile
Updating crates.io index
     Locking 11 packages to highest compatible versions
exit=0
```

Real output from `nice -n 10 python3 scripts/verify.py gates`, after all thirteen
mutation restorations. 38 tests passed; none failed or ignored. Two tests were
renamed to express the contract change: `forecast_codes_are_documented_unsigned_
integers_not_floats_or_unknowns` became `forecast_codes_are_in_range_unsigned_
integers_or_rejected`, and `wmo_unknown_codes_are_none_exhaustively_including_
upstream_contract_discrepancy_97` became `wmo_unknown_codes_are_none_exhaustively`.
This is a SPEC-driven contract change — undocumented in-range codes now parse and
are stored, which is intentional, not a weakening of the range/type guards (100,
−1 and non-integer codes are still rejected by the same test).

```text
$ nice -n 10 cargo fmt
exit=0

$ nice -n 10 cargo fmt --check
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking weather-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/weather-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.27s
exit=0

$ nice -n 10 cargo test --locked
   Compiling weather-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/weather-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.35s
     Running unittests src/lib.rs (target/debug/deps/weather_core-199e0f90a1df2071)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/weather.rs (target/debug/deps/weather-04c06ee4dbb35993)

running 38 tests
test both_parsers_reject_bad_json_utf8_trailing_data_and_nonobject_roots ... ok
test errors_have_stable_display_and_implement_standard_error ... ok
test api_reasons_truncate_at_two_hundred_unicode_characters_not_bytes ... ok
test failure_keeps_entire_last_good_snapshot_stale_without_mutating_input ... ok
test failure_without_last_good_is_unavailable_with_nulls_never_fabricated_zeros ... ok
test both_parsers_return_api_reason_before_success_shape_checks ... ok
test forecast_ignores_unknown_fields_without_requiring_metadata ... ok
test forecast_nonfinite_syntax_and_overflow_are_errors_not_measurements ... ok
test forecast_fixture_sets_all_fields_and_uses_fetch_unix_time_not_local_time ... ok
test geocode_count_is_one_through_ten_inclusive ... ok
test geocode_missing_empty_or_null_results_are_empty_but_wrong_container_is_invalid ... ok
test geocode_fixture_maps_snake_case_upstream_to_optional_contract_fields ... ok
test forecast_url_and_parser_reject_bad_place_coordinates_including_nonfinite ... ok
test geocode_optional_nulls_are_preserved_as_none_and_extra_fields_ignored ... ok
test geocode_drops_invalid_coordinates_and_blank_names_but_keeps_boundaries ... ok
test geocode_percent_encoding_roundtrips_reserved_characters_and_utf8 ... ok
test geocode_upstream_camel_case_is_not_an_alias_for_country_code ... ok
test geocode_trims_and_counts_unicode_characters_not_bytes ... ok
test forecast_requires_celsius_and_rejects_fahrenheit_or_any_other_unit ... ok
test forecast_missing_current_and_nested_fields_have_typed_errors ... ok
test malformed_error_reason_is_missing_or_invalid_without_fabricated_text ... ok
test forecast_daylight_is_only_integer_zero_or_one ... ok
test language_is_encoded_literally_and_cannot_inject_parameters_or_fragment ... ok
test next_refresh_exact_boundary_is_strictly_next_quarter_hour ... ok
test next_refresh_saturates_when_boundary_is_not_representable_without_overflow ... ok
test forecast_temperature_range_is_inclusive_and_rejects_outliers ... ok
test geocode_skips_malformed_entries_and_does_not_invent_required_values ... ok
test geocode_retains_first_ten_valid_results_in_original_order ... ok
test urls_append_endpoint_once_below_origin_or_prefix_without_trailing_slash_dependency ... ok
test state_and_place_wire_shapes_are_camel_case_with_explicit_nulls_and_no_aliases ... ok
test urls_reject_invalid_bases_instead_of_query_or_fragment_injection ... ok
test next_refresh_second_before_boundary_and_every_remainder_are_wall_clock_aligned ... ok
test forecast_rejects_wrong_current_containers_and_types ... ok
test forecast_codes_are_in_range_unsigned_integers_or_rejected ... ok
test wmo_mapping_pins_every_requested_code_condition_and_stable_icon_pair ... ok
test parsers_check_body_cap_before_json_including_exact_boundary ... ok
test wmo_unknown_codes_are_none_exhaustively ... ok
test never_panic_lcg_random_byte_bodies_for_both_parsers ... ok

test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s

   Doc-tests weather_core

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

Toolchain (queried under nice -n 10):

```text
rustc 1.99.1 (58cf8f9c3 2026-10-13) (Arch Linux rust 1:1.99.1-1)
cargo 1.99.1 (5f94df478 2026-08-27) (Arch Linux rust 1:1.99.1-1)
```

An initial run found an incorrect test assumption that MAX-100 must overflow
(the last representable quarter-hour can still lie ahead); only that test was
corrected. The subsequent baseline passed all 38 tests and strict Clippy.

## Mutation checks (actual execution)

All thirteen mutations compiled and failed their pinned runtime tests with exit
101. Every full-tree restoration diff was empty with exit 0. Table lists one
actual failing test per mutation; complete failure lists/traces are in
`target/verification/mutations.json` and `M*.log` / `M*.diff.log`.

| ID | Deliberate breakage | Pinned actual failing test | Test exit | Restored diff |
|---|---|---|---|---|
| M1 | Remove body-size cap | `parsers_check_body_cap_before_json_including_exact_boundary` | 101 | empty, exit 0 |
| M2 | Accept Fahrenheit as Celsius | `forecast_requires_celsius_and_rejects_fahrenheit_or_any_other_unit` | 101 | empty, exit 0 |
| M3 | Return now at exact boundary | `next_refresh_exact_boundary_is_strictly_next_quarter_hour` | 101 | empty, exit 0 |
| M4 | Leave ampersand unescaped | `geocode_percent_encoding_roundtrips_reserved_characters_and_utf8` | 101 | empty, exit 0 |
| M5 | Drop last-good fields on failure | `failure_keeps_entire_last_good_snapshot_stale_without_mutating_input` | 101 | empty, exit 0 |
| M6 | Accept temperature above 70 C | `forecast_temperature_range_is_inclusive_and_rejects_outliers` | 101 | empty, exit 0 |
| M7 | Accept is_day=2 | `forecast_daylight_is_only_integer_zero_or_one` | 101 | empty, exit 0 |
| M8 | Accept latitude beyond north pole | `geocode_drops_invalid_coordinates_and_blank_names_but_keeps_boundaries` | 101 | empty, exit 0 |
| M9 | Keep eleven geocode results | `geocode_retains_first_ten_valid_results_in_original_order` | 101 | empty, exit 0 |
| M10 | Map thunderstorm to fog | `wmo_mapping_pins_every_requested_code_condition_and_stable_icon_pair` | 101 | empty, exit 0 |
| M11 | Keep 201 reason characters | `api_reasons_truncate_at_two_hundred_unicode_characters_not_bytes` | 101 | empty, exit 0 |
| M12 | Remove code 97 heavy-thunderstorm mapping | `wmo_mapping_pins_every_requested_code_condition_and_stable_icon_pair` | 101 | empty, exit 0 |
| M13 | Reject undocumented in-range codes again | `forecast_codes_are_in_range_unsigned_integers_or_rejected` | 101 | empty, exit 0 |

Actual helper output:

```text
13 compiling mutations killed; every restoration diff empty.
```

After final gates, a separate full-tree comparison returned exit 0 and empty
output; programmatic parsing of real logs confirmed the counts:

```text
$ diff -r --exclude=target target/verification/pristine .
(empty output; exit=0)
passing_test_counts= [0, 38, 0] total= 38
mutations= 13 all_runtime_killed= True all_identical_restorations= True
source_files= 13
```

Usage checks ran at the start, baseline gate and final evidence inspection.
Last observed codex Session was 70%, below the 95% stop threshold, at
2026-10-30 02:33:28 EDT (before the 03:23 reset deadline).

Reproducer: `nice -n 10 python3 scripts/verify.py mutations`.
The helper snapshots the whole crate except target/, applies one compiling
behavior change, runs `nice -n 10 cargo test --locked --no-fail-fast`, restores
source bytes in finally, refreshes source mtime, and requires an empty recursive
full-tree diff. Compilation failure does not count. Logs and pristine copy
stay under `target/verification/`; the final gate run used restored source.
Only this report is intentionally updated after full-tree snapshot comparisons.

## Decisions

1. Public Rust fields are idiomatic snake_case; serde rename_all="camelCase"
   yields exactly countryCode/tempC/isDay/updatedAt. No serde aliases. Separate
   response parsing recognizes upstream country_code and no camelCase alias.
2. SPEC matching line 753 lists `isDay` without a concrete Rust type (unlike
   its explicitly optional temperature/code/place fields); it does not declare
   a nonoptional bool. Use the task's `Option<bool>`, with None on no data and
   Some(false/true) on successful forecast. updatedAt is Option<u64> UTC seconds.
3. Always request and validate Celsius. SPEC's row also permits Fahrenheit in
   the URL, but the explicit task overrides this: UI weatherUnit conversion is
   outside the core. The place's timezone/name are not interpolated into URLs.
4. Base is an HTTP(S) origin or optional path prefix, not a full endpoint. Strip
   trailing slashes, then append /v1/forecast or /v1/search. Reject empty
   authority, credentials, query/fragment, whitespace/control and backslash.
   This is a minimal injection guard, not a complete URL/hostname validator;
   the transport adapter must parse the URL before using it. No URL dependency.
5. Query length counts trimmed Unicode scalar values, not UTF-8 bytes/graphemes.
   Encode UTF-8 bytes with uppercase RFC 3986 percent escapes, leaving only
   ALPHA / DIGIT / - . _ ~ unescaped. Language is encoded literally; no invented
   locale normalization or length policy. Query 2..100 and count 1..10 enforced.
6. Body cap is inclusive 256 KiB and checked before JSON. API error=true takes
   precedence over success fields; reason is truncated to 200 Unicode scalar
   values. Missing/null reason is MissingField; non-string reason is Invalid.
   JSON syntax/UTF-8/numeric overflow returns Json; root non-object is Invalid.
7. Missing/null required forecast fields return MissingField; wrong types/ranges
   return Invalid. All three current observations and Celsius units are required
   for available=true; no partial or fabricated measurements. JSON nonfinite
   syntax/overflow fails decoding; finite range is checked separately. Unknown
   extras/metadata are ignored; local response time is not interpreted as UTC.
8. Per controller decision (verified against open-meteo.com/docs "WMO Weather
   interpretation codes"), code 97 (heavy thunderstorm) is adopted as
   `Condition::HeavyThunderstorm`. It shares the stable "thunderstorm" icon key
   with plain thunderstorm: severity remains a distinct variant while the icon
   is shared, the same precedent as rain/drizzle severities. Parse tolerance is
   now SPEC-driven: any integer weather code in 0..=99 is stored as-is in
   `WeatherState.code`, even when it has no documented condition;
   `wmo_condition` returns None for those and the UI shows a generic icon. This
   is an intentional contract change, not a weakening — codes outside 0..=99,
   negative values, and non-integer values still fail with `Invalid("weather_
   code")`, pinned by `forecast_codes_are_in_range_unsigned_integers_or_rejected`
   and mutation M13. Condition has one variant per requested code; severities
   may share icon keys. Icon keys are stable renderer-neutral identifiers, not
   shipped icon assets.
9. Geocoding skips malformed individual records, including invalid/missing ID,
   required numeric/string fields or wrong-type optional strings. Blank names
   (including whitespace-only) and invalid/nonfinite coords are dropped. Valid
   display names are preserved literally. Missing/null/empty results is empty;
   non-array results is Invalid. Null/missing optional strings become None.
   Filter invalid records before taking the first ten valid entries, in order.
10. on_failure trusts its last_good precondition, clones all fields unchanged
    except stale=true; repeated failures are idempotent. No previous success:
    available=false, stale=false and every Option=None. There is no invented
    age-based timeout: the adapter owns scheduling/on-resume and failure state.
11. Refresh is the next strictly future UTC multiple of 900, using saturating
    arithmetic. If that boundary is beyond u64::MAX, return MAX instead of
    wrapping/panicking. MAX itself has no representable strictly greater u64;
    this unavoidable exception is documented and tested. No timezone offsets.
12. Cargo manifest/lints and clippy.toml follow notification-store (edition 2021,
    rust-version 1.85, deny unwrap/expect outside tests, allow-unwrap-in-tests).
    Own Cargo.lock and target/, .gitignore /target/. Build/test commands always
    run under nice -n 10. The verification helper is external developer tooling,
    not library IO. No npm/GUI/other-crate gates are run in this isolated task.

## UNVERIFIED

- Explicit current_units field spelling in static official docs, as detailed
  above; fixture shapes are hand-written, not proof of a real network response.
- Live Open-Meteo request, status/timeout handling, streaming body cap before
  allocation, TLS, transport URL parsing, resume events and real scheduling.
  Phase 6 owns local HTTP-fixture integration and its ignored real-call test.
- Integration into SPEC/Tauri/specta/TypeScript, Settings weather_location and
  weather_unit ownership, geocoding UI, icon assets and Fahrenheit presentation.
- Renderer handling of the generic fallback icon for undocumented in-range
  codes, and whether upstream ever emits codes in 0..=99 beyond the current
  documented set (this crate stores them regardless; icon/asset rendering is
  outside its scope).
- Rust 1.85 execution (manifest follows sibling); gates use the installed
  toolchain. Deterministic LCG testing is not coverage-guided fuzzing or proof
  against allocation exhaustion/panic under arbitrary resource starvation.
