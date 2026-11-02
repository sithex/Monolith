# timer-core

Pure Rust core behind SPEC §0.5 "Design direction" (Apps bullet) and §8
"Phase 6b" — the Timer app for the Hermes Island widget: Timer / Pomodoro /
Stopwatch tabs. The Tauri actor owns one `TimerCore`, feeds it injected
monotonic ms, schedules a wakeup at `next_deadline()`, and turns returned
events into an activity + notification.

No IO, no network, no threads, no clock, no `unsafe`, never panics on any
input. All times are injected `u64` ms on a monotonic clock; time going
backwards is clamped to the last seen `now`.

## API

- `TimerCore::new(cfg)` — three independent sub-states (countdown, pomodoro,
  stopwatch); switching tabs never pauses/resets another.
- Countdown: `set_countdown` (1 s ..= 24 h, Idle only), `start/pause/resume/
  reset/add_time` (`add_time` caps the total remaining at 24 h); states
  Idle/Running/Paused/Finished; remaining = deadline − now, saturating, never
  negative.
- Pomodoro: `PomodoroConfig` validated (each phase 1 min ..= 4 h, rounds
  1..=12); `pomo_start/pause/resume/reset/skip`; Focus→ShortBreak→…→LongBreak
  after every `rounds_before_long`-th completed Focus; skips never count;
  auto-start chains from the previous phase's deadline; `completed_focus_count()`
  exposed.
- Stopwatch: `sw_start/pause/resume/reset/lap` (lap Running-only, at most 99
  stored, oldest dropped, numbering keeps counting); elapsed never decreases
  except on reset.
- `tick(now)` returns events (`CountdownFinished`, `PomodoroPhaseEnded`) in
  chronological order; expiry is inclusive; at most 100 pomodoro events per
  call; a second call with the same `now` emits nothing once all due deadlines
  are drained.
- `next_deadline()` — earliest pending deadline of countdown/pomodoro, or
  `None` when nothing is running.
- `snapshot(now)` — pure read (never fires events, never advances the core's
  clock); serde camelCase; no aliases accepted.

Semantics are documented on the items in `src/lib.rs`; the gate output and the
mutation record live in `VERIFICATION.md`.
