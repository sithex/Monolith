# VERIFICATION — ai-dashboard-core crate

Location: `island-tauri/staging/ai-dashboard-core/`.
Scope: pure aggregation core for the Hermes Island Phase 7 AI-stats dashboard
(SPEC §0.5 "Design direction", Apps bullet: "AI stats as a card dashboard
(per-provider session/week bars with reset countdowns, spending today, "now"
active session, hourly trend, per-model and per-project cost, activity
heatmap; unknown data shown unavailable, never fabricated)"). The Tauri side
reads Hermes' local metadata (read-only SQLite) and `hermes usage --json`
and feeds this crate already-decoded rows; this crate computes the cards.
Platform-free: no IO, no SQLite, no network, no threads, no clock, no
`unsafe`, no `chrono` (civil-date math is implemented here, Hinnant's
`days_from_civil`). Dependencies: serde (derive) + serde_json only.
Wire types use camelCase via `#[serde(rename_all = "camelCase")]`, no
aliases.

## Gate commands — final state (all green)

```
$ nice -n 10 cargo generate-lockfile
    Updating crates.io index
     Locking 11 packages to highest compatible versions
exit=0

$ nice -n 10 cargo fmt --check
(clean)
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking ai-dashboard-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/ai-dashboard-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.43s
exit=0

$ nice -n 10 cargo test --locked
running 0 tests        (unittests src/lib.rs)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 38 tests      (tests/dashboard_tests.rs)
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 1 test        (tests/fuzz_tests.rs)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 7 tests       (tests/time_tests.rs)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 0 tests       (doc-tests)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total: 46 tests (38 dashboard + 1 fuzz + 7 time), all green under
`nice -n 10`.

## Mutation checks (all actually run on the current code)

Method: snapshot the pristine tree to scratch (`rsync -a --exclude=target`),
apply one breakage, run `nice -n 10 cargo test --locked --no-fail-fast`,
record failing test names, restore pristine (touching restored sources so
cargo rebuilds), then `diff -r --exclude=target <pristine> .` verified empty
for every mutation. All six restored identical, and the final full gate
re-run on the restored tree is green (above). Every mutation compiled (a
non-compiling mutation would not count). Each mutation fails *named* tests,
not only the fuzz test.

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | `spend_today` ignores `utc_offset_s` (always uses offset 0 for "today") | `spend_today_respects_local_day_with_negative_offset` |
| 2 | `spend_today` returns `Some(0.0)` even when today's rows all have unknown cost | `spend_today_none_when_all_today_costs_unknown`, `unknown_data_never_fabricated_as_zero` |
| 3 | Active-session 6 h stale check removed (any open row is "active") | `active_beyond_6h_is_stale`, `active_stale_only_row_is_none` |
| 4 | Hourly emits 25 buckets instead of 24 | `hourly_always_24_buckets_current_last`, `hourly_sums_tokens_and_distinct_sessions`, `hourly_tokens_none_when_all_unknown_but_sessions_counted`, `hourly_token_sums_saturate`, `hourly_model_switching_attributed_per_row`, `future_rows_ignored_and_dropped`, `empty_provider_and_model_dropped`, `invalid_cost_counted_but_kept` (plus fuzz) |
| 5 | >20 cost lines truncated without merging the remainder into "Other" | `by_model_21_keys_merge_into_other` |
| 6 | Heatmap weekday shifted by one (`(wd+1)%7`) | `heatmap_weekday_for_known_date`, `heatmap_counts_distinct_session_start` |

## Decisions

- **Active boundary inclusive.** A row with `endedAtS == None` and `startedAtS >= now − 6h` is active; exactly 6 h old is still active, 6 h + 1 s is stale. Ties on `startedAtS` are broken by the lexicographically greater `sessionId`.
- **SoonToday `Some(0.0)` semantics.** `Some(0.0)` is returned only for a *real* zero: no rows at all in the local day, or today's rows whose known costs genuinely sum to 0.0. If today's rows exist but none has a cost (all `None`/invalid), `None` is returned; if some have cost and some don't, the known costs are summed and `spendTodayPartial` is set.
- **CostLine with mixed known/unknown cost.** `costUsd` sums the known (finite, non-negative) costs; `None` only when no row in the group has a known cost. There is no partial flag on `CostLine` (the partial flag exists only on `spendToday`).
- **"Other" line placement.** The merged remainder is appended after the sorted top 20 lines (it is *not* re-inserted into the sort), so a table can have at most 21 lines. A real key that literally equals `"Other"` would collide with the merged line; accepted.
- **Heatmap counts distinct (sessionId, startedAtS) pairs.** Rows of the same session sharing an identical start count once (e.g. model-switch row duplication); the same session starting at a different time counts separately. This keeps the heatmap total ≤ number of distinct (session, start) rows in 28 days.
- **"Negative token / cost-free invalid values".** Input/output tokens are `u64` and cannot be negative, so this clause is interpreted as *invalid cost* (NaN/±inf/negative) — such rows are counted in `rowsIgnored` but kept (their cost becomes unknown). `None` cost is normal, not ignored. Future rows (> now + 60 s) and empty-provider/model rows are counted and dropped.
- **Local time.** "Today", hour buckets and the heatmap use local time = UTC + injected offset; the offset is validated ±14 h inclusive before any computation. All civil-date arithmetic uses the proleptic Gregorian calendar via Hinnant's `days_from_civil` / `civil_from_days` (weekday 0 = Monday, 1970-01-01 = Thursday).
- **Saturation.** Token sums saturate at `u64::MAX`; f64 sums only ever add finite non-negative values and saturate at `f64::MAX`.
- **Boundary windows.** byModel/byProject lines include rows with `startedAtS >= now − 30d`; heatmap includes rows with `startedAtS >= now − 28d` (inclusive).

## UNVERIFIED

- **Tauri actor wiring** — how the real `hermes usage --json` output and the SQLite read map onto `UsageRow`/`LimitWindow` (field naming, which rows per session) is outside this crate's scope.
- **Live clock / offset changes** — a single injected `(now_s, utc_offset_s)` snapshot; DST transitions mid-day or a clock jump within one build are not modeled.
- **Very large inputs** — no streaming or cap beyond what the caller feeds in; memory scales with the input size.
- **Fuzz guarantees** — the fuzz test uses a fixed-seed hand-rolled LCG (2000 iterations); it is a never-panic + invariant smoke test, not exhaustive property testing.
- **Reset countdown text for negative/unknown resets** — only `Some(>= 0)` values are formatted; `None` → `None`.
