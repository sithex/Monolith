# VERIFICATION — lrclib-core crate

Location: `island-tauri/staging/lrclib-core/`.
Scope: pure core behind SPEC §8 Phase 6 "Music extras" lyrics lookup from LRCLIB
(keyless; HTTP itself is done later by the integrating phase). Builds the
`GET /api/get` query, classifies HTTP responses, and derives cache keys.
Platform-free: no IO, no network, no threads, no clock, no `unsafe`.
Dependencies: serde (derive) + serde_json + xxhash-rust (xxh3) only. Never
panics on any input (pseudo-random fuzz test).

## Gate commands — final state (all green)

```
$ nice -n 10 cargo generate-lockfile
    Updating crates.io index
     Locking 12 packages to highest compatible versions
exit=0

$ nice -n 10 cargo fmt --check
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.04s
exit=0

$ nice -n 10 cargo test --locked
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.03s
     Running unittests src/lib.rs (target/debug/deps/lrclib_core-abd1a3273ef572b1)

running 5 tests
test tests::duration_seconds_rounds_nearest_500_up ... ok
test tests::form_encode_keeps_unreserved_and_space_becomes_plus ... ok
test tests::json_content_type_media_type_param_insensitive ... ok
test tests::form_encode_percent_upper_hex_per_byte ... ok
test tests::normalize_trims_lowercases_collapses_whitespace ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/cache_key.rs (target/debug/deps/cache_key-81d25fcef4a56015)

running 7 tests
test blank_title_or_artist_is_none ... ok
test blank_album_eq_absent_album ... ok
test deterministic_lowercase_hex ... ok
test different_fields_produce_different_keys ... ok
test normalization_trims_lowercases_collapses_whitespace ... ok
test omitted_duration_eq_unknown_duration ... ok
test separator_makes_component_boundaries_distinct ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/fuzz.rs (target/debug/deps/fuzz-7440d8e23172108b)

running 1 test
test never_panics_on_pseudorandom_input ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.74s

     Running tests/get_path.rs (target/debug/deps/get_path-91b0018c811fc6aa)

running 14 tests
test album_included_when_nonblank_and_trimmed ... ok
test album_is_encoded_too ... ok
test blank_album_omitted ... ok
test blank_title_or_artist_is_none ... ok
test duration_omitted_when_unknown_or_out_of_range ... ok
test duration_rounds_to_nearest_second_500_up ... ok
test forms_urlencoded_punctuation_and_space ... ok
test minimal_title_and_artist ... ok
test multibyte_utf8_encoded_per_byte_upper_hex ... ok
test param_order_is_track_artist_album_duration ... ok
test trims_title_and_artist ... ok
test fields_capped_at_512_chars ... ok
test cap_boundary_keeps_full_multibyte_scalar ... ok
test unreserved_characters_kept ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/parse_response.rs (target/debug/deps/parse_response-f1d1a25eb2a057c1)

running 15 tests
test any_other_status_is_failed_with_status ... ok
test both_lyrics_null_or_blank_is_not_found ... ok
test instrumental_beats_any_lyrics ... ok
test invalid_json_or_invalid_record_200_is_failed ... ok
test non_json_200_is_failed ... ok
test status_404_is_not_found_whatever_the_body ... ok
test plain_only_and_trimmed ... ok
test status_429_retry_after_delta_seconds ... ok
test json_content_type_allows_params_case_insensitive ... ok
test status_503_retry_after ... ok
test synced_wins_with_inline_plain ... ok
test synced_with_blank_or_null_plain ... ok
test unknown_extra_fields_ignored ... ok
test oversize_body_fails_before_status_dispatch ... ok
test exactly_one_mib_is_parsed_but_not_more ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests lrclib_core

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit=0
```

Total: 42 tests (5 unit + 7 cache_key + 1 fuzz + 14 get_path + 15 parse_response), all green.

## Mutation checks (all actually run on the current code)

Method: snapshot pristine tree to scratch (`--exclude=target`), apply one
breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record failing
test names, restore pristine, then `diff -r --exclude=target <pristine> .`
verified empty for every mutation. All eight mutations compiled (a
non-compiling mutation would not count) and restored identical, and a final
full gate re-run on the restored tree is green (above).

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | Nearest-second rounding replaced by floor (`length_ms? / 1000`; 500 ms now rounds down) | `duration_rounds_to_nearest_second_500_up`, `duration_omitted_when_unknown_or_out_of_range`, `omitted_duration_eq_unknown_duration`, `param_order_is_track_artist_album_duration`, `duration_seconds_rounds_nearest_500_up` |
| 2 | Duration range narrowed to 1..3600 (3600 s now excluded) | `duration_rounds_to_nearest_second_500_up`, `omitted_duration_eq_unknown_duration`, `duration_seconds_rounds_nearest_500_up` |
| 3 | Space percent-encoded as `%20` instead of `+` | `forms_urlencoded_punctuation_and_space`, `album_is_encoded_too`, `multibyte_utf8_encoded_per_byte_upper_hex`, `form_encode_keeps_unreserved_and_space_becomes_plus` |
| 4 | Synced lyrics preferred over instrumental (instrumental check moved after the synced branch) | `instrumental_beats_any_lyrics` |
| 5 | Body-size cap removed (1 MiB guard no longer precedes parsing/dispatch) | `oversize_body_fails_before_status_dispatch`, `exactly_one_mib_is_parsed_but_not_more` |
| 6 | `Retry-After` not clamped to 1..=3600 | `status_429_retry_after_delta_seconds` |
| 7 | Cache-key component length prefixes dropped (naive concatenation) | `separator_makes_component_boundaries_distinct` |

## Decisions

- **URL form**: `get_path` emits `/api/get?track_name=<t>&artist_name=<a>[&album_name=<al>][&duration=N]`
  with fields trimmed, capped at 512 chars (`chars().take(512)`, char-boundary
  safe), then `application/x-www-form-urlencoded`-encoded: unreserved
  `A-Za-z0-9-._~` kept, space → `+`, everything else `%XX` upper hex per
  UTF-8 byte. Blank title/artist → `None`; blank album omitted.
- **Duration rounding**: `length_ms` → nearest second, 500 ms rounds up
  (`(ms + 500) / 1000`, saturating for overflow safety); included only when
  the rounded value is in 1..=3600, otherwise omitted. So 3600.0 s is kept and
  3600.5 s is omitted.
- **Record shape**: a valid record deserializes only the fields the crate
  reads: `instrumental` (`bool`, defaults to `false` when absent or null via
  `Option<bool>` + `serde(default)`), and `plainLyrics`/`syncedLyrics`
  (`string|null`, optional). The rest of the upstream record (`id`,
  `trackName`, `artistName`, `albumName`, `duration`, …) is ignored (no
  `deny_unknown_fields`), so live data with a null/missing album or a
  missing/integer duration still classifies. On 200, non-JSON Content-Type,
  invalid JSON, a non-object JSON body, or a wrong type for a deserialized
  field (e.g. `syncedLyrics: 5`) → `Failed { 200 }`. A JSON object with no
  lyrics fields → `NotFound`.
- **200 classification order**: instrumental (if true) → synced non-blank
  (`Synced { lrc, plain }`, `plain` only when non-blank) → plain non-blank
  (`Plain`) → `NotFound`. Returned lyric strings are trimmed of outer
  whitespace; internal content is preserved.
- **Content-Type check**: media type is the part before the first `;`,
  trimmed, compared ASCII case-insensitively to `application/json`.
- **Body cap**: `body.len() > 1 MiB` → `Failed { status }` — checked before
  any parsing or status dispatch (even 404/429/503). Exactly 1 MiB is parsed.
- **Retry-After**: delta-seconds only; trimmed `u64` parse, default 60 when
  missing/unparseable (including negative, overflow, dates), clamped to
  1..=3600.
- **Cache key**: xxh3-64 lowercase hex (`{:016x}`) of the tuple
  `(title, artist, album-or-"", duration-seconds-or-"")` where each component
  is normalized (trim + Unicode lowercase + collapse internal whitespace runs
  to one space) and written length-prefixed so component boundaries are
  unambiguous (pinned by `separator_makes_component_boundaries_distinct`,
  incl. the ("a b","c") vs ("a","b c") pair). Duration uses the exact string
  `get_path` would produce (empty when omitted). Blank title/artist → `None`.
- **Never panic**: JSON via `serde_json::from_slice` (error-tolerant), checked
  arithmetic, no non-char-boundary slicing; fuzz pinned by
  `never_panics_on_pseudorandom_input` (3000 random status/Content-Type/body
  triples incl. oversize and exact-1-MiB bodies, plus 3000 random
  `TrackQuery`s) with a hand-rolled LCG and no new dependencies.

## UNVERIFIED

- **Live LRCLIB HTTP** — this crate is the pure core; the transport, the
  `User-Agent` header (`USER_AGENT`), and the opt-in network policy live in
  the integrating phase, which will add a local HTTP fixture plus one real
  `#[ignore]` call per SPEC §8 Phase 6.
- **Cache persistence in the data dir** — `cache_key` provides the key; the
  on-disk cache itself is out of scope here.
- **RFC 7231 date-form `Retry-After`** — only delta-seconds are handled, per
  the SPEC bullet; date-form headers are treated as unparseable → default 60.
- **Real-world LRCLIB records beyond the documented shape** — exercised via
  the fuzz corpus but not against a live record sample.

## Follow-up after controller audit

Controller findings: (1) missing `#![forbid(unsafe_code)]`; (2) over-strict
record parsing — `LyricsRecord` required `trackName`/`artistName`/`albumName`/
`duration` as non-null even though the crate never reads them, so a real record
with `albumName: null`, a missing album, or an integer/absent duration turned
available lyrics into `Failed{200}` ("one unexpected field makes live data
fail").

Changes:
- `src/lib.rs`: added `#![forbid(unsafe_code)]` at the crate root.
  `LyricsRecord` now deserializes only what is used — `instrumental`
  (`Option<bool>` + `serde(default)`, defaults to `false` when absent or null)
  and `plainLyrics`/`syncedLyrics` (optional, null-tolerant). The `id`,
  `trackName`, `artistName`, `albumName`, `duration` and other upstream fields
  are no longer deserialized (unknown fields stay ignored). `classify_record`
  reads `instrumental.unwrap_or_default()`.
- `tests/parse_response.rs`: added `album_null_with_synced_is_synced`,
  `record_missing_metadata_with_plain_is_plain`,
  `missing_instrumental_with_synced_is_synced`,
  `wrong_type_synced_lyrics_is_failed_200`, `json_array_body_is_failed_200`,
  and `empty_object_is_not_found`. The former `{}`/`{"trackName":...}` failed-
  set entries now assert `NotFound` (they are valid empty records).

Gate commands — final state (all green)

```
$ nice -n 10 cargo fmt --check
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking lrclib-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/lrclib-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
exit=0

$ nice -n 10 cargo test --locked
    Running unittests src/lib.rs (target/debug/deps/lrclib_core-abd1a3273ef572b1)
running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
    Running tests/cache_key.rs (target/debug/deps/cache_key-81d25fcef4a56015)
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
    Running tests/fuzz.rs (target/debug/deps/fuzz-7440d8e23172108b)
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.70s
    Running tests/get_path.rs (target/debug/deps/get_path-91b0018c811fc6aa)
running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
    Running tests/parse_response.rs (target/debug/deps/parse_response-f1d1a25eb2a057c1)
running 21 tests
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
   Doc-tests lrclib_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit=0
```

Total: 48 tests (5 unit + 7 cache_key + 1 fuzz + 14 get_path + 21 parse_response),
all green.

Follow-up mutation checks (both actually run on the current code; same method
as above — snapshot pristine, apply one breakage, run
`nice -n 10 cargo test --locked --no-fail-fast`, record failing tests, restore,
`diff -r --exclude=target <pristine> .` verified empty for both; final full
gate re-run on the restored tree is green above).

| # | Breakage applied | Failing tests observed |
|---|---|---|
| A | `album_name: String` re-added as a required field (old over-strict shape) | `album_null_with_synced_is_synced`, `record_missing_metadata_with_plain_is_plain`, `missing_instrumental_with_synced_is_synced`, `empty_object_is_not_found` |
| B | `instrumental` made a required `bool` (no default; read directly in `classify_record`) | `record_missing_metadata_with_plain_is_plain`, `missing_instrumental_with_synced_is_synced`, `empty_object_is_not_found` |

Decisions updated: see the **Record shape** bullet above (fields deserialized,
null tolerance; wrong-type/non-object body still `Failed{200}`). All other
decisions unchanged.
