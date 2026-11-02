# ai-dashboard-core

Pure aggregation core for the Hermes Island **Phase 7** AI-stats dashboard
(SPEC §0.5 "Design direction", Apps bullet). The Tauri side reads Hermes'
local metadata (read-only SQLite) and `hermes usage --json`, decodes rows,
and hands them to this crate; this crate computes the dashboard cards.

Pure library: no IO, no SQLite, no network, no threads, no clock, no
`chrono`, no `unsafe`, never panics on any input. All time inputs are
injected (`now_s` seconds, `utc_offset_s` in seconds, validated ±14 h).

## API

```rust
pub fn build_dashboard(
    rows: &[UsageRow],
    limits: &[LimitWindow],
    now_s: i64,
    utc_offset_s: i32,
) -> Result<Dashboard, DashError>
```

- `UsageRow` — one session may have several rows (model switching); rows are
  attributed per row. All `Option` fields mean *unknown*, never 0.
- `LimitWindow` — provider limit: `provider`, `label`, `usedPercent`
  (clamped 0..=100, NaN/±inf → None), `resetsAtS` (→ `resetsInS` +
  `resetText`: "now" / "<1m" / "Nm" / "Hh Mm" / "Dd Hh").
- `Dashboard` (serde camelCase):
  - `limits` — sorted by provider then label.
  - `spendTodayUsd` / `spendTodayPartial` — true only when today's rows exist
    but some lack cost; `None` when today's rows exist but none has cost;
    `Some(0.0)` only for a real zero (no rows today, or known costs sum to 0).
  - `active` — most recently started row with no end, within the last 6 h
    (stale beyond 6 h), ties broken by sessionId.
  - `hourly` — exactly 24 local-hour buckets ending with the hour containing
    now; tokens saturate, distinct sessions counted once.
  - `byModel` / `byProject` — last 30 days, sorted cost desc (None last),
    tokens desc, key; cap 20 lines, remainder merged into "Other".
  - `heatmap` — `[[u32; 24]; 7]`, Monday-first, distinct (session, start)
    pairs over the last 28 days.
  - `rowsIgnored` — future rows, empty provider/model (dropped), invalid cost
    (kept, cost → unknown).

## Development

```sh
nice -n 10 cargo generate-lockfile   # once
nice -n 10 cargo fmt --check
nice -n 10 cargo clippy --all-targets --locked -- -D warnings
nice -n 10 cargo test --locked
```

`unwrap`/`expect` are denied in non-test code (`[lints.clippy]`); tests use
`allow-unwrap-in-tests` but free helper fns in integration tests use
`match`/`panic!`.

## Layout

- `src/lib.rs` — crate root, re-exports.
- `src/dashboard.rs` — the aggregation core (`build_dashboard` + helpers).
- `src/time.rs` — civil-date/local-time math (Hinnant's days_from_civil).
- `src/types.rs` — wire types (`UsageRow`, `LimitWindow`, `Dashboard`, ...).
- `src/error.rs` — `DashError::{InvalidUtcOffset}`.
- `tests/dashboard_tests.rs`, `tests/time_tests.rs`, `tests/fuzz_tests.rs`.
- `VERIFICATION.md` — gate output, mutation table, decisions, unverified.
