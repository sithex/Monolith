# VERIFICATION — agent-tracker-core crate

Location: `island-tauri/staging/agent-tracker-core/`.
Scope: pure core behind SPEC §0.5 "More tiles/features Nick picked" item
(8) — the Agent run tracker ("running = no `ended_at` and
`last_activity_at` within a stale window; delegated runs = `source`
`oneshot`/`subagent`; a finish raises an activity + notification"). The
Tauri actor reads the `sessions` table of `~/.hermes/state.db` (read-only,
`mode=ro`, `query_only`) and hands rows here; this crate classifies them
(`classify` → Running / Stale / Finished / Rejected), builds live chips
(`Tracker::update` → `Update { chips, events, overflow }`) and counts
warnings. It never reads anything and never runs anything. Platform-free:
no IO, no network, no threads, no clock (time is always an injected
parameter), no `unsafe`, never panics on any input. Dependencies: serde
(derive) + serde_json only. Wire types use camelCase via serde, no
`#[serde(alias)]`. Upstream facts (table columns, observed `source`
values a2a/cli/desktop/local/oneshot/subagent/tui, observed end reasons
cli_close/new_session/startup_orphan_reap/tui_shutdown/tui_close/
agent_close/ws_orphan_reap/conduit_voice, 29 endedAt-None orphans on
nick-pc) are as verified read-only by the controller.

## Gate commands — final state (all green, under `nice -n 10`)

```
$ nice -n 10 cargo generate-lockfile
    Updating crates.io index
     Locking 11 packages to highest compatible versions
exit=0

$ nice -n 10 cargo fmt --check
(clean)
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking agent-tracker-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/agent-tracker-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.58s
exit=0

$ nice -n 10 cargo test --locked
     Running unittests src/lib.rs (target/debug/deps/agent_tracker_core-7840ccf6e1e09313)
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/agent_tracker_tests.rs (target/debug/deps/agent_tracker_tests-b23b90e49089a938)
running 45 tests
test result: ok. 45 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-c3fcba89f9df233f)
running 1 test
test fuzz_tracker_never_panics_and_keeps_invariants ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests agent_tracker_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total: 46 tests (45 integration + 1 fuzz), all green.

## Mutation checks (all actually run on the current code)

Method: snapshot the pristine tree to scratch (`--exclude=target`), apply
one breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record
failing (named, non-fuzz) test names, restore pristine (`rsync -a
--delete` + `touch` the restored source so cargo recompiles fresh), then
`diff -r --exclude=target <pristine> .` verified empty for every mutation
(printed `RESTORED_IDENTICAL` each time), and the final full gate re-run
on the restored tree is green (above). Every mutation compiled (a
non-compiling mutation would not count) and every one fails *named*
non-fuzz tests. No size/count cap was removed (the overflow mutation only
changes the count arithmetic).

| # | Breakage applied | Failing named tests observed |
|---|---|---|
| 1 | `endedAt` None treated as Running regardless of staleness (`active` check dropped in `classify`) | `classify_stale_after_boundary`, `update_stall_transition`, `update_mixed_scenario`, `update_first_update_emits_no_history_events` |
| 2 | First update emits Finished/Stalled for history (transition loop seeded with all present ids when `prev_running` is empty) | `update_first_update_emits_no_history_events`, `update_mixed_scenario` |
| 3 | Subagent shown as its own chip while its parent runs (deep parity check replaced by `even = true`; children never counted) | `update_subagent_folds_into_parent`, `update_nested_folding_only_one_level`, `update_children_multiple`, `update_mixed_scenario` |
| 4 | Overflow count off by one (`saturating_sub(max + 1)` instead of `saturating_sub(max)`) | `update_chips_cap_and_overflow` |
| 5 | Clock-skew guard removed (future rows classify Running) | `classify_future_skew_boundary`, `classify_future_activity_skew`, `update_skew_warns_and_not_running` |
| 6 | Duplicate id keeps the older row (`better_than` returns `false`) | `update_duplicate_keeps_latest_activity`, `update_duplicate_tie_keeps_later_started` |
| 7 | Lookback effectively disabled (`startedAt < cutoff - 1e9`) | `update_ignores_rows_started_before_lookback` |

## Fuzz

`tests/fuzz_tests.rs`: hand-rolled MMIX LCG (fixed seed, no external
deps), 2500 iterations. Each iteration classifies every random row
directly and drives a fresh `Tracker` through one update plus four follow-up
updates (transition coverage). Random input includes NaN/±inf/negative
timestamps, far-future and near-future (skew-boundary) times, arbitrary
epochs, self/ghost/missing parents, unknown/empty sources, multibyte and
empty ids, garbage `now` (`f64::from_bits`), and random-but-valid configs.
Invariants asserted: no panic; chips never exceed `maxChips`; chips unique
and sorted newest-first when `now` is finite; no duplicate (kind, id)
event in one update; a fresh tracker's first update never emits
Finished/Stalled.

## Decisions (ambiguous API points resolved the least-surprising way)

- **`classify` returns `RunState` including a `Rejected{reason}`
  variant.** The spec says "row rejected with a warning"; rejection must
  be representable in the declared `-> RunState` signature. `Stale`
  carries `clock_skew: bool` so the skew warning is observable. Warnings
  themselves accumulate on the `Tracker` (`warnings()` / `take_warnings()`)
  rather than inside `Update` (whose shape is fixed).
- **`Update` gains an `overflow` field** (`u32`): the spec's "capped at
  maxChips with an `overflow` count" needs a home; it rides on the update
  result alongside `chips`/`events`.
- **Chip sources.** Delegated sources (`delegatedSources`, default
  `oneshot`/`subagent`) and top-level `local`/`tui`/`cli`/`desktop` become
  chips when running. `a2a` is **not** a chip source (it is absent from
  the spec's list), so an `a2a` running row is never shown; its subagent
  children still appear as their own delegated chips. Recorded as a
  deliberate reading; if `a2a` should be shown, add it to
  `TOP_LEVEL_SOURCES` (pinned by `update_a2a_source_is_not_a_chip`).
- **Folding is one-level and parity-based.** A running row folds into its
  parent chip iff the parent is itself a running chip. Chain
  gp←p←s yields chips gp and s (p folded into gp), regardless of input
  order — the parent-links graph is resolved by depth parity, with cycles
  (pathological) treated deterministically as "not chips". A row whose
  `parentSessionId` is its own id is never folded. Pinned by
  `update_nested_folding_only_one_level` and `update_self_parent_not_folded`.
- **Folded subagents still emit `Started`** on first sight (they are
  running sessions); folding only affects chips, not events.
- **Clock skew boundary:** timestamps ≤ 300s ahead are Running; > 300s
  ahead are Stale with a `ClockSkew` warning (never Running forever).
  `startedAt` and `lastActivityAt` are both checked.
- **Validation precedes state:** any NaN/inf/negative timestamp rejects
  the whole row, even if `endedAt` is set (`Finished` never wins over an
  invalid time).
- **Lookback filtering is inclusive at the boundary** (`startedAt < now -
  lookbackSecs` is ignored; equality is kept). Filtering happens after
  time validation and before dedupe, so an out-of-window duplicate does
  not warn. Pinned by `update_ignores_rows_started_before_lookback`.
- **Duplicate ids:** keep the row with the latest `lastActivityAt`;
  ties break toward the latest `startedAt`; full ties keep the first-seen
  row. One `DuplicateId` warning per duplicate id per update.
- **`elapsedSecs` is `now - startedAt`** (raw; may be slightly negative up
  to −300s at the skew boundary; Finished elapsed is `endedAt - startedAt`).
- **Label fallback:** title → model → short id; empty titles/models fall
  through. Short id strips a leading `local-` prefix then takes 8 chars
  (char-boundary safe).
- **Event ordering is deterministic by id** (BTree iterate): Started events
  in id order, then transitions (Stalled/Finished/Disappeared) in id order.
- **serde sequence form quirk (known, pinned):** serde's `Deserialize`
  accepts the positional-array form of a struct. `serde_sequence_form_parses_row`
  pins that an 11-element array parses as a `SessionRow` and that a
  wrong-length array errors; `serde_no_aliases_snake_case_keys_rejected`
  pins that snake_case keys are not aliases. The Tauri side should always
  send objects.

## UNVERIFIED

- **Tauri actor wiring** — reading `~/.hermes/state.db`, translating rows
  into `SessionRow`, and presenting chips/events are outside this crate's
  scope (it never opens the DB or runs any command). The observed
  `source`/end-reason vocabularies come from the controller's read-only
  verification and are used only as test fixtures.
- **Timezone/epoch semantics** — times are opaque epoch-second floats;
  the crate never interprets calendar time.
- **Deeply nested/non-finite edge behavior of `total_cmp`** — sorting uses
  `f64::total_cmp`; rejected rows are filtered before chips are built, so
  NaN never reaches the sort, but `total_cmp`'s total order is relied upon.
- **A `sessions` table with more columns** — extra columns are ignored by
  the actor; this crate only reads the documented fields.
- **Concurrency / shared state** — the crate is single-threaded with no
  locks; the actor serializes access.
