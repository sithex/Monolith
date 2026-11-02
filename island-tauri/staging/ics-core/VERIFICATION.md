# VERIFICATION.md — ics-core

Pure Rust library crate at `island-tauri/staging/ics-core` (Phase 6b read-only
Calendar core). Backend-neutral; parses iCalendar (RFC 5545 subset) text and
expands an agenda over a time window. No IO/env/network/threads/clock/unsafe,
no chrono (own civil-date math), serde + serde_json only.

## Gate output (real, 2026-10-30, `nice -n 10`)

Lockfile generated (`cargo generate-lockfile`), all later cargo steps `--locked`.

    $ cargo fmt --check
    FMT_EXIT=0

    $ cargo clippy --all-targets --locked -- -D warnings
        Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
    (no warnings, no errors)

    $ cargo test --locked
    running 0 tests
    test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
    running 1 test
    test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
    running 31 tests
    test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
    running 0 tests
    test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

Total: 32 tests (31 rules + 1 fuzz), 0 failed. `unwrap`/`expect` denied in
non-test code (`[lints.clippy]`), `allow-unwrap-in-tests = true` in
clippy.toml.

## Mutation checks

Each mutation applied to the pristine tree, run with
`nice -n 10 cargo test --locked --no-fail-fast`, then reverted and
diff-checked identical to `~/.hermes/cache/scratch/ics-pristine`
(final `diff -r -x target` exit 0). All 8 killed named tests (never only the
fuzz test). Resource-guard mutations were implemented by LOWERING the guard
(candidate 10 000 → 10; instance cap 2000 → 10) so the named guard tests fail
without unbounded memory — no run required a memory/time cap.

| # | Mutation | File | Change | Failing named tests |
|---|----------|------|--------|---------------------|
| 1 | unfold keeps whitespace | src/ics.rs | `cur.push_str(rest)` → `cur.push_str(line)` | `unfolding_space_tab_crlf_lf` |
| 2 | `\N` not handled | src/ics.rs | `Some('n')|Some('N')` → `Some('n')` only | `text_escapes` |
| 3 | MONTHLY clamps 31st | src/agenda.rs | skip-short-months → `day.min(days_in_month)` | `monthly_31st_skips_short_months`, `yearly_feb29_skips_non_leap_years` |
| 4 | EXDATE filter removed | src/agenda.rs | drop `exset.contains(&cand)` continue | `exdate_comma_list_and_repeat`, `exdate_still_counts_toward_count` |
| 5 | window end inclusive | src/agenda.rs | `cs >= window_end_s` → `cs > window_end_s` | `window_overlap_edges` |
| 6 | candidate guard lowered | src/agenda.rs | `MAX_CANDIDATES_PER_RULE 10_000 → 10` | `candidate_guard_10000`, `cap_2000_instances`, `monthly_31st_skips_short_months` |
| 7 | 2000 cap lowered | src/agenda.rs | `MAX_INSTANCES 2000 → 10` | `cap_2000_instances` |
| 8 | all-day sort flipped | src/agenda.rs | `b.all_day.cmp(&a.all_day)` → reversed | `sort_order_ties` |

Note: mutation 6 also trips two neighbours because lowering the candidate guard
to 10 starves the 2000-instance fill and the MONTHLY 31st expansion under the
fixed test windows — expected knock-on, the guard test still fails.

## Decisions (least-surprising resolutions)

- Unchecked `agenda` silences the 2000-instance cap and 10 000-candidate
  guard; typed 400-day window error goes through `agenda_checked` /
  `agenda_checked_with_warnings`.
- Unknown TZID → treated as floating + warning (caller supplies `TzResolver`;
  no tz database in the crate).
- COUNT+UNTIL together → rule invalid → event non-recurring + warning.
  COUNT counts the DTSTART instance; EXDATE removes instances but they still
  count toward COUNT.
- MONTHLY repeats on DTSTART's day-of-month and skips months lacking that day
  (31st); YEARLY on Feb 29 skips non-leap years.
- DTEND absent: DURATION (`P1D`, `PT1H30M`, `P2W`; signs rejected) else
  all-day → 1 day, timed → zero length. DTEND before DTSTART → skipped +
  warning.
- Extra RRULE parts (BYMONTHDAY, BYSETPOS, numeric BYDAY) → non-recurring +
  `UnsupportedRrule` warning. WKST ignored.
- Local offset applies to all-day and floating datetimes.
- Input > 8 MiB, contains NUL, or has no VCALENDAR → `IcsError`; a broken
  VEVENT is skipped with a warning, the rest kept.
- Wire types camelCase via `#[serde(rename_all = "camelCase")]`, no aliases.

## UNVERIFIED

- Real memory/time profile of the full 10 000-candidate expansion was NOT
  measured (an earlier unbounded run OOM-killed the host via systemd-oomd at
  ~19 GB; the guard itself is behavior-verified by `candidate_guard_10000`
  and mutation 6, but not wall-clock/memory-bounded).
- Interop against .ics files actually exported by a calendar backend (KDE/
  online) not exercised — only handcrafted fixtures and the LCG fuzz input.
- Only one zoned TZID (America/New_York) and one negative local offset (−4h)
  are exercised; other zones/offsets rely on the same code path.
- Fuzz asserts sorted output, ≤ 2000 instances, window overlap, endS ≥ startS,
  and never-panic; it does not assert parse/expansion equivalence to a
  reference implementation.

## Follow-up after controller audit (2026-10-30)

Controller gap: `src/civil.rs` date math had no direct tests; mutating the
leap-year rule to `y % 4 == 0` failed no test. Added 6 named unit tests in a
`#[cfg(test)] mod tests` inside `src/civil.rs` (no public API change):

- `is_leap_knows_century_and_four_hundred_rules` — 2000=T, 1900=F, 2100=F,
  2024=T, 2026=F
- `days_from_civil_epoch_and_reference_points` — (1970,1,1)=0, (2000,3,1)=11017
- `civil_from_days_round_trips_every_997th_day` — day −800000..+800000 step 997
- `weekday_of_2026_10_30_is_friday` — weekday_number()=4 (0=Mon..6=Sun)
- `year_2100_has_no_february_29` — 2100-02-28 + 1d = 2100-03-01;
  NaiveDateTime::date(2100,2,29)=None
- `yearly_feb29_rule_starting_2096_has_no_instance_in_2100` — YEARLY Feb 29
  (DTSTART 2096-02-29) yields no instance in window 2100-01-01..2100-12-31

Gate output (real, `nice -n 10`, `--locked`):

    $ cargo fmt --check
    FMT_CHECK_GREEN  (exit 0)

    $ cargo clippy --all-targets --locked -- -D warnings
        Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s
    (no warnings, no errors)

    $ cargo test --locked
    running 6 tests
    test result: ok. 6 passed; 0 failed ...   (new civil unit tests)
    running 1 test
    test result: ok. 1 passed; 0 failed ...   (fuzz)
    running 31 tests
    test result: ok. 31 passed; 0 failed ...   (rules)
    running 0 tests
    test result: ok. 0 passed; 0 failed ...

Total: 38 tests (31 rules + 6 civil + 1 fuzz), 0 failed.

Mutation re-run (leap-year rule `(y % 4 == 0 && y % 100 != 0) || y % 400 == 0`
→ `y % 4 == 0`) with `nice -n 10 cargo test --locked --no-fail-fast` fails 3
named non-fuzz tests:

- `is_leap_knows_century_and_four_hundred_rules`
- `year_2100_has_no_february_29`
- `yearly_feb29_rule_starting_2096_has_no_instance_in_2100`

(`days_from_civil`/round-trip/weekday tests pass under this mutation because the
Hinnant algorithm encodes the century rule arithmetically, not via `is_leap` —
expected.) Mutation reverted; the final tree is byte-identical to the refreshed
snapshot `~/.hermes/cache/scratch/ics-pristine` (`diff -r -x target` exit 0),
which now includes the new tests.
