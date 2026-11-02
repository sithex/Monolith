# VERIFICATION — lrc-lyrics crate

Location: `island-tauri/staging/lrc-lyrics/`.
Scope: pure LRC synced-lyrics parser for the Hermes Island music widget
(SPEC §8 Phase 6 "Music extras", `syncedLyrics` field of LRCLIB). Platform-free:
no IO, no network, no `unsafe`, no external dependencies. Never panics on any
input (pseudo-random fuzz test).

## Gate commands — final state (all green)

```
$ nice -n 10 cargo generate-lockfile
exit=0

$ nice -n 10 cargo fmt --check
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking lrc-lyrics v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/lrc-lyrics)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
exit=0

$ nice -n 10 cargo test --locked
   Compiling lrc-lyrics v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/lrc-lyrics)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.16s
     Running unittests src/lib.rs (target/debug/deps/lrc_lyrics-d1fce4067474e8b1)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/parse.rs (target/debug/deps/parse-ceba17c0c25d5173)

running 25 tests
test active_index_boundaries ... ok
test active_index_offset_can_push_first_line_before_zero ... ok
test active_index_offset_negative_shows_later ... ok
test active_index_offset_positive_shows_earlier ... ok
test bad_length_value_leaves_none ... ok
test colon_fraction ... ok
test crlf_and_utf8_bom_handled ... ok
test empty_text_timed_lines_kept ... ok
test enhanced_word_timing_stripped ... ok
test fraction_scaling ... ok
test malformed_timestamp_tag_is_ignored ... ok
test metadata_parsed ... ok
test minutes_may_exceed_59_and_have_many_digits ... ok
test multi_timestamp_line_expands ... ok
test no_timed_lines_errors ... ok
test non_timestamp_angle_spans_kept ... ok
test parses_basic_mm_ss ... ok
test stable_sort_preserves_equal_time_input_order ... ok
test unknown_tags_ignored ... ok
test untimed_non_metadata_lines_ignored ... ok
test whitespace_trimmed ... ok
test window_clamps_and_behaves_before_first_line ... ok
test no_panic_on_pseudorandom_input ... ok
test too_large_errors ... ok
test exactly_one_mib_accepted ... ok

test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s

   Doc-tests lrc_lyrics

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit=0
```

## Mutation checks (all actually run on the current code)

Method: snapshot pristine tree to scratch (`--exclude=target`), apply one
breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record failing
test names, restore pristine, then `diff -r --exclude=target <pristine> .`
verified empty for every mutation. All five restored identical, and a final
full gate re-run on the restored tree is green (above).

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | Fraction denominator ignored (`checked_div(denom)` → `checked_div(100)`: `.5` reads as 5000 ms) in `parse_fraction_ms` | `fraction_scaling`, `colon_fraction` |
| 2 | Offset sign flipped in `active_index` (`+ self.offset_ms` → `- self.offset_ms`) | `active_index_offset_positive_shows_earlier`, `active_index_offset_negative_shows_later`, `active_index_offset_can_push_first_line_before_zero` |
| 3 | Multi-timestamp expansion truncated to the first timestamp (`for ms in stamps` → `take(1)`) | `multi_timestamp_line_expands` |
| 4 | `active_index` boundary at exactly a line's start off by one (`<=` → `<`) | `active_index_boundaries`, `window_clamps_and_behaves_before_first_line`, `active_index_offset_positive_shows_earlier`, `active_index_offset_negative_shows_later` |
| 5 | Empty-text timed lines dropped (skipped before push) | `empty_text_timed_lines_kept` |

Note: a first M1 attempt replaced the tail expression so the function no longer
compiled (no tests ran) — that is not a valid mutation; M1 was re-run with a
compiling mis-scaling, which is what the table records.

## Decisions

- **Timestamps are `mm:ss[:fraction]`, not `hh:mm:ss`.** The first numeric
  group is always minutes (may exceed 59, any digit count), the second is
  seconds, and an optional third group is the fraction. `:` is accepted as the
  fraction separator alongside `.`, so `[01:02:50]` = 1 min + 2.5 s = 62,500
  ms (spec's own example). There is no separate hours field; a
  `[hh:mm:ss]`-style file would be read under this model (documented
  limitation, see UNVERIFIED).
- **Fractions are decimal seconds** computed as
  `digits × 1000 / 10^len`: `.5` = 500 ms, `.05` = 50 ms, `.005` = 5 ms,
  `:50` = 500 ms. Fractions longer than 9 digits are treated as a malformed
  tag (ignored) — beyond-ms precision is meaningless and the cap keeps all
  arithmetic overflow-free.
- **Positive offset = lyrics shown earlier** (user-specified convention):
  a line is active when `start_ms <= position_ms + offset_ms`. Equivalently
  the effective start is `start_ms - offset_ms`, so `[offset:+500]` slides
  every line 500 ms earlier. Negative offsets slide later. A large positive
  offset can push the first line's effective start before 0, making it active
  at position 0.
- **`active_index` reports the last line whose offset-adjusted start has been
  reached** (`partition_point`), so when several lines share the same start
  time the last one in input order is active. Stable sorting (`sort_by_key`)
  preserves input order for equal start times; the multi-timestamp expansion
  order is preserved.
- **`window` semantics:** a contiguous slice around the active line with up to
  `before` preceding and `after` following lines, clamped to the edges. When
  the position is before the first line (nothing active), the first `after`
  lines are returned so a widget can preview upcoming lyrics.
- **Empty-text timed lines are kept** (instrumental gaps); untimed
  non-metadata lines are ignored; unknown or malformed `[xx:...]` tags are
  ignored (including `[re:]`, `[by:]`, `[ve:]`, `[10]`, `[10:20:30:40]`,
  `[:10]`).
- **Metadata is last-wins** for repeated `[ti:]`/`[ar:]`/`[al:]`/
  `[length:]` tags; tag prefixes are ASCII case-insensitive. `[length:]`
  reuses the timestamp parser, so `[length:03:24]` = 204,000 ms; an
  unparseable value leaves `length_ms = None`.
- **`[offset:]` is last-wins**; its value must be `[+/-]digits` (an optional
  sign is accepted). A magnitude that overflows `i64` is ignored.
- **Size limit checked before any parsing** on the input's byte length: more
  than 1 MiB (1,048,576 bytes) → `ParseError::TooLarge`; exactly 1 MiB is
  accepted.
- **Never panic:** all integer parsing uses validated digit strings + `.ok()`
  and checked arithmetic; all slicing happens at verified char boundaries
  (`get(..)` guarded prefix slicing, char-length advancement); a UTF-8 BOM
  and CRLF endings are stripped. Pinned by `no_panic_on_pseudorandom_input`
  (500 deterministic LCG-generated byte strings, no new deps).
- **Enhanced word timing:** `<mm:ss.xx>` spans are stripped from `text` only
  when their inner content parses as a timestamp; non-timestamp spans like
  `<3>` or `<b>` are kept verbatim.

## UNVERIFIED

- **Real LRCLIB fetch / data-dir cache / MPRIS-position integration** —
  Phase 6 wiring lives in `src-tauri`; this crate is the pure parser only.
- **Real-world corpus diversity** — behavior is pinned by synthetic/fixture
  inputs; no large corpus of real LRC files was exercised.
- **Hours-style timestamps** (`[hh:mm:ss]`) — intentionally unsupported (see
  Decisions); if real files turn out to use them, the timestamp model should
  be revisited with a new spec decision.
- **Widget rendering** — the `window` before-first-line behavior and the
  equal-start "last wins" active choice are our design decisions; they may be
  tuned when the actual music widget lands.
- **Length formats other than `mm:ss[:fraction]`** — e.g. a plain seconds
  value in `[length:]` is parsed as `None`.
