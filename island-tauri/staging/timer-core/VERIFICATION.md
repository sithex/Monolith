# VERIFICATION — timer-core crate

Location: `island-tauri/staging/timer-core/`.
Scope: pure core behind SPEC §0.5 "Design direction" (Apps bullet) and §8
"Phase 6b" — the Timer app (Timer / Pomodoro / Stopwatch tabs). The Tauri
actor owns one `TimerCore`, feeds it injected monotonic ms, schedules a wakeup
at `next_deadline()`, and turns returned events into an activity +
notification. Platform-free: no IO, no network, no threads, no clock, no
`unsafe`, never panics on any input.
Dependencies: serde (derive) + serde_json only. All times are injected `u64`
ms on a monotonic clock; backwards time is clamped to the last seen `now`.

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
    Checking timer-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/timer-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.21s
exit=0

$ nice -n 10 cargo test --locked
   Compiling timer-core v0.1.0 (.../timer-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.16s
     Running unittests src/lib.rs (target/debug/deps/timer_core-ac712b48070ab8f9)

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/countdown.rs
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz.rs
running 1 test
test fuzz_invariants ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/pomodoro.rs
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/snapshot.rs
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/stopwatch.rs
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests timer_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total: 39 tests (12 countdown + 1 fuzz + 10 pomodoro + 8 snapshot + 8
stopwatch), all green under `nice -n 10`.

## Mutation checks (all actually run on the current code)

Method: snapshot pristine tree to scratch (`--exclude=target`), apply one
breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record failing
test names, restore pristine (touching restored sources so cargo rebuilds),
then `diff -r --exclude=target <pristine> .` verified empty for every
mutation. All seven restored identical, and a final full gate re-run on the
restored tree is green (above). Every mutation compiled (a non-compiling
mutation would not count).

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | Inclusive expiry made exclusive: `deadline <= now` → `deadline < now` in both the countdown tick and the pomodoro tick | countdown: `start_runs_and_finishes_inclusively_at_deadline`, `add_time_only_while_running_or_paused`, `backwards_clock_is_clamped_to_last_seen_now`, `pause_resume_preserve_remaining_exactly_across_cycles`, `reset_returns_to_idle_keeps_duration_and_can_restart`; pomodoro: `phases_cycle_with_long_break_after_4th_completed_focus`, `several_phases_elapse_in_one_late_tick`, `hundred_event_cap_spills_to_next_tick`, `auto_start_next_false_arms_paused_at_full_length`, `skip_ends_phase_without_counting_focus`; snapshot: `next_deadline_reflects_earliest_pending_deadline` |
| 2 | Pause no longer saves remaining (`remaining: 0` in both countdown and pomodoro pause) | countdown: `pause_resume_preserve_remaining_exactly_across_cycles`, `add_time_extends_running_and_paused_capped_at_24h`, `backwards_clock_is_clamped_to_last_seen_now`; pomodoro: `pause_resume_preserve_phase_remaining`; snapshot: `next_deadline_reflects_earliest_pending_deadline`, `snapshot_wires_camelcase_keys_exactly`, `snapshot_serde_roundtrip_preserves_value` |
| 3 | Long-break counter off by one: `completed_focus_count % rounds == 0` → `== 1` | pomodoro: `phases_cycle_with_long_break_after_4th_completed_focus`, `auto_start_next_false_arms_paused_at_full_length`, `auto_start_next_chains_from_deadline_not_now`, `several_phases_elapse_in_one_late_tick`; snapshot: `snapshot_does_not_advance_the_cores_clock` |
| 4 | Skip counts as a completed focus (`advance(phase, true, now)` in skip) | pomodoro: `skip_ends_phase_without_counting_focus`, `reset_clears_pomodoro_progress` |
| 5 | Auto-start chains from `now` instead of the previous phase's deadline (`advance(ended, true, now)` in the pomodoro tick) | pomodoro: `auto_start_next_chains_from_deadline_not_now`, `several_phases_elapse_in_one_late_tick`, `hundred_event_cap_spills_to_next_tick`; snapshot: `snapshot_does_not_advance_the_cores_clock` |
| 6 | Backwards clock not clamped (`clamp_now` drops the `max`) | countdown: `backwards_clock_is_clamped_to_last_seen_now`; stopwatch: `backwards_clock_clamps_stopwatch`; fuzz: `fuzz_invariants` |
| 7 | Lap cap drops the newest lap instead of the oldest (`pop_front` → `pop_back`) | stopwatch: `hundredth_lap_drops_lap_1_but_numbering_keeps_counting` |

## Decisions

- **Three independent sub-states, one clock.** `last_seen_now` is global
  across countdown/pomodoro/stopwatch. Any op on any sub-state advances it; a
  later op with a smaller `now` clamps to it (per spec: "now < last seen now
  is clamped to last seen now"). `snapshot` is a pure read: it clamps its own
  computation against `last_seen_now` but never advances the core's clock
  (pinned by `snapshot_does_not_advance_the_cores_clock`).
- **Events only from `tick`.** Automatic phase completion (deadline reached)
  is the only event source; user-initiated transitions (`skip`, pause/resume,
  start, reset, lap) never emit events — completed skips raise no notification.
- **100-event per-call cap.** When more than 100 pomodoro phases are due in
  one tick (auto-start chain after sleep), the first tick returns exactly 100
  events and the leftovers spill into subsequent ticks with the same `now`.
  Therefore "a second tick at the same now emits nothing" holds whenever one
  tick fully drains all due deadlines (the normal case); it does not hold
  while the cap is exhausted (pinned by `hundred_event_cap_spills_to_next_tick`).
- **Auto-start chaining from the previous deadline.** With `auto_start_next`,
  the next phase starts at the phase's deadline (not at `now`), so a late tick
  does not drift (pinned by `auto_start_next_chains_from_deadline_not_now`).
  A skip ends the current phase at the injected `now`; the next phase then
  starts from that `now`.
- **Long break timing.** A LongBreak fires exactly when a *natural* focus
  completion brings `completed_focus_count % rounds_before_long == 0`; skips
  never increment the counter and never trigger a long break. The snapshot's
  `round` = `completed_focus_count % rounds_before_long` (0 right after a long
  break or when idle).
- **Serde wiring.** Snapshots use camelCase field names (`remainingMs`,
  `elapsedMs`, `laps`, …); the `TimerEvent` struct-variant field is
  `completedFocus`; `Phase` values are camelCase (`focus`, `shortBreak`,
  `longBreak`); event tag variants keep the spec's spelling
  (`CountdownFinished`, `PomodoroPhaseEnded`). No `#[serde(alias)]` is used,
  so snake_case keys do not populate fields (pinned by
  `snake_case_keys_are_ignored_no_aliases`); missing keys fall back to
  defaults.
- **Restart semantics.** Countdown `start` is valid from Idle *and* Finished
  (immediate restart with the configured duration); `reset` keeps the
  duration so a re-start works. `pomo_start`/`sw_start` from Paused resume
  rather than restart (documented; starting while Running is an invalid
  transition).
- **Stopwatch laps.** Lap numbers are a global per-session counter that keeps
  counting past the 99-lap drop; `sw_reset` clears both laps and numbering.
- **Idle remaining.** Idle countdown shows the configured duration (0 if
  none set); idle pomodoro shows the focus duration.
- **add_time capping.** The *remaining* total is capped at 24 h; the deadline
  becomes `now + capped remaining` (so after a cap the deadline can exceed 24 h
  by the current offset).
- **No clock / never panics.** All arithmetic is saturating; fuzz covers
  `u64::MAX` nows, backwards time and random invalid ops.

## UNVERIFIED

- **Tauri actor wiring** — activities/notifications from events, wakeup
  scheduling at `next_deadline()`, and the frontend minute-picker are outside
  this crate's scope.
- **Live monotonic source behaviour** — e.g. system suspend on a real host;
  synthetic inputs only (the auto-chain + cap rules model a sleeping host).
- **Granularity of the picker** — the spec explicitly leaves "1 s ..= 24 h,
  any ms" granularity to the UI; no sub-second rounding is enforced here.
