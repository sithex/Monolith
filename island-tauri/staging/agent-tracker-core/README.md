# agent-tracker-core

Pure core for the Hermes Island **Agent run tracker** app (SPEC §0.5
"More tiles/features Nick picked", item 8). The Tauri actor reads the
`sessions` table of `~/.hermes/state.db` (read-only, `mode=ro`,
`query_only`) and passes rows here; this crate classifies them into
Running / Stale / Finished, builds live chips for running Hermes agent
sessions, and emits state-transition events. **It never reads anything
and never runs anything.**

Platform-free: no IO, no network, no threads, no clock (time is always an
injected parameter), no `unsafe`, never panics on any input. Dependencies:
`serde` (derive) + `serde_json` only. Wire types use camelCase via serde,
no aliases.

## Quick tour

```rust
use agent_tracker_core::{classify, Event, RunState, SessionRow, Tracker, TrackerConfig, Update};

// One row as read from the sessions table (epoch-second floats).
let row = SessionRow {
    id: "local-abc123xyz".into(),
    source: "local".into(),
    parent_session_id: None,
    model: Some("claude-sonnet".into()),
    billing_provider: Some("anthropic".into()),
    started_at: 1_750_000_000.0,
    ended_at: None,
    end_reason: None,
    last_activity_at: Some(1_750_000_450.0),
    api_call_count: Some(12),
    title: Some("Audit mixer-core".into()),
};

// classify: Running / Stale{clock_skew} / Finished{reason} / Rejected{reason}
let cfg = TrackerConfig::default();
assert_eq!(classify(&row, 1_750_000_900.0, &cfg), RunState::Running);

// Tracker keeps the previous snapshot; each update returns chips + events.
let mut tracker = Tracker::new();
let Update { chips, events, overflow } = tracker.update(vec![row], 1_750_000_900.0);
```

## Classification

- Running: no `endedAt`, and `lastActivityAt` (else `startedAt`) is within
  `staleAfterSecs` of now.
- Stale: no `endedAt`, older. A timestamp more than 300s in the future is
  also Stale (clock skew) — never Running forever.
- Finished: `endedAt` is set (reason carried through).
- Rejected: any NaN / infinite / negative timestamp; the row is dropped
  with a warning.
- Rows whose `startedAt` is older than `lookbackSecs` are ignored.

## Chips

Delegated sources (`delegatedSources`, default `oneshot`/`subagent`) and
top-level `local`/`tui`/`cli`/`desktop` sources become chips when running.
A running row whose `parentSessionId` points at a running chip folds into
that chip as a `children` count instead of a chip of its own (one level;
nested parents do not chain). Other sources (e.g. `a2a`) are not shown.
Chips are sorted newest-first (`startedAt` desc) and capped at `maxChips`;
`overflow` counts the hidden chips.

## Events

- `Started{id}` — first sight as Running (once per id).
- `Finished{id, reason, elapsedSecs}` — a previously-Running id now has
  `endedAt`.
- `Stalled{id}` — a previously-Running id went stale.
- `Disappeared{id}` — a previously-Running id is missing from this
  update's rows.
- The first update never emits Finished/Stalled (startup does not spam old
  history). No event fires twice for the same transition.

Warnings (rejected rows, clock skew, duplicate ids) accumulate on the
`Tracker` and are drained with `take_warnings()`.

## Development

```
nice -n 10 cargo generate-lockfile
nice -n 10 cargo fmt --check
nice -n 10 cargo clippy --all-targets --locked -- -D warnings
nice -n 10 cargo test --locked
```

`unwrap`/`expect` are denied in non-test code (`[lints.clippy]`); tests may
use them (`clippy.toml`). Run under `nice -n 10` because another agent runs
timing-sensitive GUI tests on the same machine.
