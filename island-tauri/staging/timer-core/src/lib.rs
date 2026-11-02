//! Pure core behind SPEC §0.5 "Design direction" (Apps bullet) and §8
//! "Phase 6b" — the Timer app (Timer / Pomodoro / Stopwatch tabs) for the
//! Hermes Island widget. A [`TimerCore`] owns three independent sub-states
//! (countdown, pomodoro, stopwatch); switching tabs never pauses or resets
//! another. All times are injected `u64` milliseconds on a monotonic clock;
//! nothing in this crate reads a clock.
//!
//! Platform-free: no IO, no network, no threads, no clock, no `unsafe`,
//! and never panics on any input. Time going backwards (a `now` smaller
//! than the last seen one) is clamped to the last seen `now`. Wire types
//! (snapshots, events, phases, status kinds) use camelCase through serde
//! and accept no aliases.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Minimum countdown duration: 1 s.
pub const MIN_COUNTDOWN_MS: u64 = 1_000;
/// Maximum countdown duration: 24 h.
pub const MAX_COUNTDOWN_MS: u64 = 86_400_000;
/// Minimum pomodoro phase duration: 1 min.
pub const MIN_PHASE_MS: u64 = 60_000;
/// Maximum pomodoro phase duration: 4 h.
pub const MAX_PHASE_MS: u64 = 14_400_000;
/// Minimum pomodoro rounds before a long break.
pub const MIN_ROUNDS: u64 = 1;
/// Maximum pomodoro rounds before a long break.
pub const MAX_ROUNDS: u64 = 12;
/// Maximum number of stored stopwatch laps (oldest dropped beyond this).
pub const MAX_LAPS: usize = 99;
/// Maximum number of pomodoro phase transitions one `tick` call returns.
pub const MAX_EVENTS_PER_TICK: usize = 100;

/// Typed error. Operations that are invalid for the current state return
/// [`TimerError::InvalidTransition`] and leave the state unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    /// `set_countdown` duration is outside 1 s ..= 24 h.
    CountdownDurationOutOfRange,
    /// `start` was called before any countdown duration was set.
    CountdownNotSet,
    /// The operation is not valid in the current state; state unchanged.
    InvalidTransition,
    /// A pomodoro phase duration is outside 1 min ..= 4 h.
    PomodoroDurationOutOfRange,
    /// `rounds_before_long` is outside 1..=12.
    PomodoroRoundsOutOfRange,
}

/// A pomodoro phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Focus,
    ShortBreak,
    LongBreak,
}

/// Validated pomodoro configuration. Construct via [`PomodoroConfig::new`],
/// which rejects out-of-range values with a typed [`TimerError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PomodoroConfig {
    focus_ms: u64,
    short_break_ms: u64,
    long_break_ms: u64,
    rounds_before_long: u64,
    auto_start_next: bool,
}

impl PomodoroConfig {
    /// Validate and construct a configuration. Each phase duration must be
    /// inside 1 min ..= 4 h and `rounds_before_long` inside 1..=12.
    pub fn new(
        focus_ms: u64,
        short_break_ms: u64,
        long_break_ms: u64,
        rounds_before_long: u64,
        auto_start_next: bool,
    ) -> Result<Self, TimerError> {
        if !(MIN_PHASE_MS..=MAX_PHASE_MS).contains(&focus_ms)
            || !(MIN_PHASE_MS..=MAX_PHASE_MS).contains(&short_break_ms)
            || !(MIN_PHASE_MS..=MAX_PHASE_MS).contains(&long_break_ms)
        {
            return Err(TimerError::PomodoroDurationOutOfRange);
        }
        if !(MIN_ROUNDS..=MAX_ROUNDS).contains(&rounds_before_long) {
            return Err(TimerError::PomodoroRoundsOutOfRange);
        }
        Ok(Self {
            focus_ms,
            short_break_ms,
            long_break_ms,
            rounds_before_long,
            auto_start_next,
        })
    }

    /// Focus phase duration (ms).
    pub fn focus_ms(&self) -> u64 {
        self.focus_ms
    }

    /// Short break duration (ms).
    pub fn short_break_ms(&self) -> u64 {
        self.short_break_ms
    }

    /// Long break duration (ms).
    pub fn long_break_ms(&self) -> u64 {
        self.long_break_ms
    }

    /// Number of completed focus phases before a long break.
    pub fn rounds_before_long(&self) -> u64 {
        self.rounds_before_long
    }

    /// Whether the next phase auto-starts when a phase ends.
    pub fn auto_start_next(&self) -> bool {
        self.auto_start_next
    }
}

/// Status of the countdown sub-state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CountdownStatusKind {
    #[default]
    Idle,
    Running,
    Paused,
    Finished,
}

/// Status of the pomodoro sub-state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PomodoroStatusKind {
    #[default]
    Idle,
    Running,
    Paused,
}

/// Status of the stopwatch sub-state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopwatchStatusKind {
    #[default]
    Idle,
    Running,
    Paused,
}

/// One stored stopwatch lap. `number` counts monotonically across laps
/// (the oldest lap is dropped beyond [`MAX_LAPS`], but numbering keeps
/// counting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Lap {
    number: u64,
    elapsed_ms: u64,
}

impl Lap {
    fn new(number: u64, elapsed_ms: u64) -> Self {
        Self { number, elapsed_ms }
    }

    /// Monotonic lap number (1-based, never reused after a drop).
    pub fn number(&self) -> u64 {
        self.number
    }

    /// Elapsed stopwatch time at the moment the lap was taken (ms).
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }
}

/// Snapshot of the countdown sub-state at a given `now`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CountdownSnapshot {
    status: CountdownStatusKind,
    remaining_ms: u64,
}

impl CountdownSnapshot {
    /// Current status.
    pub fn status(&self) -> CountdownStatusKind {
        self.status
    }

    /// Remaining ms: `deadline − now` (saturating, never negative) when
    /// running; the configured duration when idle; the leftover when paused;
    /// 0 when finished.
    pub fn remaining_ms(&self) -> u64 {
        self.remaining_ms
    }
}

/// Snapshot of the pomodoro sub-state at a given `now`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PomodoroSnapshot {
    status: PomodoroStatusKind,
    phase: Phase,
    remaining_ms: u64,
    round: u32,
}

impl PomodoroSnapshot {
    /// Current status.
    pub fn status(&self) -> PomodoroStatusKind {
        self.status
    }

    /// Current phase (Focus when idle).
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Remaining ms in the current phase at `now` (saturating, never
    /// negative).
    pub fn remaining_ms(&self) -> u64 {
        self.remaining_ms
    }

    /// Completed focus phases in the current cycle (0 right after a long
    /// break or when idle; reaches `rounds_before_long − 1` before the next
    /// long break).
    pub fn round(&self) -> u32 {
        self.round
    }
}

/// Snapshot of the stopwatch sub-state at a given `now`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StopwatchSnapshot {
    status: StopwatchStatusKind,
    elapsed_ms: u64,
    laps: Vec<Lap>,
}

impl StopwatchSnapshot {
    /// Current status.
    pub fn status(&self) -> StopwatchStatusKind {
        self.status
    }

    /// Elapsed ms at `now` (accumulated + current segment; never decreases
    /// except on reset).
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }

    /// Stored laps, oldest first (at most [`MAX_LAPS`]).
    pub fn laps(&self) -> &[Lap] {
        &self.laps
    }
}

/// Immutable snapshot of all three sub-states at a given `now`.
/// A pure read: it never fires events and never advances the core's clock.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TimerSnapshot {
    countdown: CountdownSnapshot,
    pomodoro: PomodoroSnapshot,
    stopwatch: StopwatchSnapshot,
}

impl TimerSnapshot {
    /// Countdown sub-snapshot.
    pub fn countdown(&self) -> &CountdownSnapshot {
        &self.countdown
    }

    /// Pomodoro sub-snapshot.
    pub fn pomodoro(&self) -> &PomodoroSnapshot {
        &self.pomodoro
    }

    /// Stopwatch sub-snapshot.
    pub fn stopwatch(&self) -> &StopwatchSnapshot {
        &self.stopwatch
    }
}

/// An event produced by [`TimerCore::tick`] when a deadline expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all_fields = "camelCase")]
pub enum TimerEvent {
    /// The countdown reached zero.
    CountdownFinished,
    /// A pomodoro phase ended and the next one began (or was armed).
    PomodoroPhaseEnded {
        /// The phase that ended.
        ended: Phase,
        /// The phase that came next.
        next: Phase,
        /// Total completed focus phases so far.
        completed_focus: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CountdownState {
    Idle,
    Running { deadline: u64 },
    Paused { remaining: u64 },
    Finished,
}

#[derive(Debug, Clone)]
struct Countdown {
    state: CountdownState,
    duration_ms: u64,
}

impl Countdown {
    fn new() -> Self {
        Self {
            state: CountdownState::Idle,
            duration_ms: 0,
        }
    }

    fn set(&mut self, duration_ms: u64) -> Result<(), TimerError> {
        if self.state != CountdownState::Idle {
            return Err(TimerError::InvalidTransition);
        }
        if !(MIN_COUNTDOWN_MS..=MAX_COUNTDOWN_MS).contains(&duration_ms) {
            return Err(TimerError::CountdownDurationOutOfRange);
        }
        self.duration_ms = duration_ms;
        Ok(())
    }

    fn start(&mut self, now: u64) -> Result<(), TimerError> {
        match self.state {
            CountdownState::Idle => {
                if self.duration_ms == 0 {
                    return Err(TimerError::CountdownNotSet);
                }
                self.state = CountdownState::Running {
                    deadline: now.saturating_add(self.duration_ms),
                };
                Ok(())
            }
            CountdownState::Finished => {
                self.state = CountdownState::Running {
                    deadline: now.saturating_add(self.duration_ms),
                };
                Ok(())
            }
            _ => Err(TimerError::InvalidTransition),
        }
    }

    fn pause(&mut self, now: u64) -> Result<(), TimerError> {
        match self.state {
            CountdownState::Running { deadline } => {
                self.state = CountdownState::Paused {
                    remaining: deadline.saturating_sub(now),
                };
                Ok(())
            }
            _ => Err(TimerError::InvalidTransition),
        }
    }

    fn resume(&mut self, now: u64) -> Result<(), TimerError> {
        match self.state {
            CountdownState::Paused { remaining } => {
                self.state = CountdownState::Running {
                    deadline: now.saturating_add(remaining),
                };
                Ok(())
            }
            _ => Err(TimerError::InvalidTransition),
        }
    }

    fn reset(&mut self) {
        self.state = CountdownState::Idle;
    }

    fn add_time(&mut self, now: u64, delta_ms: u64) -> Result<(), TimerError> {
        match self.state {
            CountdownState::Running { deadline } => {
                let remaining = deadline.saturating_sub(now);
                let new_remaining = remaining.saturating_add(delta_ms).min(MAX_COUNTDOWN_MS);
                self.state = CountdownState::Running {
                    deadline: now.saturating_add(new_remaining),
                };
                Ok(())
            }
            CountdownState::Paused { remaining } => {
                self.state = CountdownState::Paused {
                    remaining: remaining.saturating_add(delta_ms).min(MAX_COUNTDOWN_MS),
                };
                Ok(())
            }
            _ => Err(TimerError::InvalidTransition),
        }
    }

    fn status(&self) -> CountdownStatusKind {
        match self.state {
            CountdownState::Idle => CountdownStatusKind::Idle,
            CountdownState::Running { .. } => CountdownStatusKind::Running,
            CountdownState::Paused { .. } => CountdownStatusKind::Paused,
            CountdownState::Finished => CountdownStatusKind::Finished,
        }
    }

    fn remaining_at(&self, now: u64) -> u64 {
        match self.state {
            CountdownState::Idle => self.duration_ms,
            CountdownState::Running { deadline } => deadline.saturating_sub(now),
            CountdownState::Paused { remaining } => remaining,
            CountdownState::Finished => 0,
        }
    }

    fn deadline(&self) -> Option<u64> {
        match self.state {
            CountdownState::Running { deadline } => Some(deadline),
            _ => None,
        }
    }

    fn tick(&mut self, now: u64) -> Option<TimerEvent> {
        match self.state {
            CountdownState::Running { deadline } if deadline <= now => {
                self.state = CountdownState::Finished;
                Some(TimerEvent::CountdownFinished)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PomoState {
    Idle,
    Running { phase: Phase, deadline: u64 },
    Paused { phase: Phase, remaining: u64 },
}

#[derive(Debug, Clone)]
struct Pomodoro {
    cfg: PomodoroConfig,
    state: PomoState,
    completed_focus_count: u64,
}

impl Pomodoro {
    fn new(cfg: PomodoroConfig) -> Self {
        Self {
            cfg,
            state: PomoState::Idle,
            completed_focus_count: 0,
        }
    }

    fn phase_duration(&self, phase: Phase) -> u64 {
        match phase {
            Phase::Focus => self.cfg.focus_ms,
            Phase::ShortBreak => self.cfg.short_break_ms,
            Phase::LongBreak => self.cfg.long_break_ms,
        }
    }

    /// Advance past `ended` (which ended at `at`). A natural focus
    /// completion (`counted == true`) counts toward the round counter; a
    /// skipped one does not. With `auto_start_next` the next phase runs from
    /// `at` (the previous phase's deadline — a late tick does not drift);
    /// otherwise it is armed paused at full length.
    fn advance(&mut self, ended: Phase, counted: bool, at: u64) {
        if ended == Phase::Focus && counted {
            self.completed_focus_count += 1;
        }
        let next = if ended == Phase::Focus {
            if counted && self.completed_focus_count % self.cfg.rounds_before_long == 0 {
                Phase::LongBreak
            } else {
                Phase::ShortBreak
            }
        } else {
            Phase::Focus
        };
        let dur = self.phase_duration(next);
        if self.cfg.auto_start_next {
            self.state = PomoState::Running {
                phase: next,
                deadline: at.saturating_add(dur),
            };
        } else {
            self.state = PomoState::Paused {
                phase: next,
                remaining: dur,
            };
        }
    }

    fn start(&mut self, now: u64) -> Result<(), TimerError> {
        match self.state {
            PomoState::Idle => {
                self.state = PomoState::Running {
                    phase: Phase::Focus,
                    deadline: now.saturating_add(self.cfg.focus_ms),
                };
                Ok(())
            }
            PomoState::Paused { phase, remaining } => {
                self.state = PomoState::Running {
                    phase,
                    deadline: now.saturating_add(remaining),
                };
                Ok(())
            }
            _ => Err(TimerError::InvalidTransition),
        }
    }

    fn pause(&mut self, now: u64) -> Result<(), TimerError> {
        match self.state {
            PomoState::Running { phase, deadline } => {
                self.state = PomoState::Paused {
                    phase,
                    remaining: deadline.saturating_sub(now),
                };
                Ok(())
            }
            _ => Err(TimerError::InvalidTransition),
        }
    }

    fn resume(&mut self, now: u64) -> Result<(), TimerError> {
        match self.state {
            PomoState::Paused { phase, remaining } => {
                self.state = PomoState::Running {
                    phase,
                    deadline: now.saturating_add(remaining),
                };
                Ok(())
            }
            _ => Err(TimerError::InvalidTransition),
        }
    }

    fn reset(&mut self) {
        self.state = PomoState::Idle;
        self.completed_focus_count = 0;
    }

    fn skip(&mut self, now: u64) -> Result<(), TimerError> {
        match self.state {
            PomoState::Running { phase, .. } | PomoState::Paused { phase, .. } => {
                self.advance(phase, false, now);
                Ok(())
            }
            PomoState::Idle => Err(TimerError::InvalidTransition),
        }
    }

    fn status(&self) -> PomodoroStatusKind {
        match self.state {
            PomoState::Idle => PomodoroStatusKind::Idle,
            PomoState::Running { .. } => PomodoroStatusKind::Running,
            PomoState::Paused { .. } => PomodoroStatusKind::Paused,
        }
    }

    fn phase(&self) -> Phase {
        match self.state {
            PomoState::Idle => Phase::Focus,
            PomoState::Running { phase, .. } => phase,
            PomoState::Paused { phase, .. } => phase,
        }
    }

    fn remaining_at(&self, now: u64) -> u64 {
        match self.state {
            PomoState::Idle => self.cfg.focus_ms,
            PomoState::Running { deadline, .. } => deadline.saturating_sub(now),
            PomoState::Paused { remaining, .. } => remaining,
        }
    }

    fn deadline(&self) -> Option<u64> {
        match self.state {
            PomoState::Running { deadline, .. } => Some(deadline),
            _ => None,
        }
    }

    fn round(&self) -> u32 {
        (self.completed_focus_count % self.cfg.rounds_before_long) as u32
    }

    fn tick(&mut self, now: u64) -> Vec<TimerEvent> {
        let mut events = Vec::new();
        loop {
            match self.state {
                PomoState::Running { phase, deadline } if deadline <= now => {
                    if events.len() >= MAX_EVENTS_PER_TICK {
                        break;
                    }
                    let ended = phase;
                    self.advance(ended, true, deadline);
                    events.push(TimerEvent::PomodoroPhaseEnded {
                        ended,
                        next: self.phase(),
                        completed_focus: self.completed_focus_count.min(u32::MAX as u64) as u32,
                    });
                }
                _ => break,
            }
        }
        events
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwState {
    Idle,
    Running { started: u64 },
    Paused,
}

#[derive(Debug, Clone)]
struct Stopwatch {
    state: SwState,
    accumulated_ms: u64,
    laps: VecDeque<Lap>,
    lap_counter: u64,
}

impl Stopwatch {
    fn new() -> Self {
        Self {
            state: SwState::Idle,
            accumulated_ms: 0,
            laps: VecDeque::new(),
            lap_counter: 0,
        }
    }

    fn start(&mut self, now: u64) -> Result<(), TimerError> {
        match self.state {
            SwState::Idle | SwState::Paused => {
                self.state = SwState::Running { started: now };
                Ok(())
            }
            SwState::Running { .. } => Err(TimerError::InvalidTransition),
        }
    }

    fn pause(&mut self, now: u64) -> Result<(), TimerError> {
        match self.state {
            SwState::Running { started } => {
                self.accumulated_ms = self
                    .accumulated_ms
                    .saturating_add(now.saturating_sub(started));
                self.state = SwState::Paused;
                Ok(())
            }
            _ => Err(TimerError::InvalidTransition),
        }
    }

    fn resume(&mut self, now: u64) -> Result<(), TimerError> {
        match self.state {
            SwState::Paused => {
                self.state = SwState::Running { started: now };
                Ok(())
            }
            _ => Err(TimerError::InvalidTransition),
        }
    }

    fn reset(&mut self) {
        self.state = SwState::Idle;
        self.accumulated_ms = 0;
        self.laps.clear();
        self.lap_counter = 0;
    }

    fn lap(&mut self, now: u64) -> Result<(), TimerError> {
        match self.state {
            SwState::Running { .. } => {
                let elapsed = self.elapsed_at(now);
                self.lap_counter += 1;
                self.laps.push_back(Lap::new(self.lap_counter, elapsed));
                if self.laps.len() > MAX_LAPS {
                    self.laps.pop_front();
                }
                Ok(())
            }
            _ => Err(TimerError::InvalidTransition),
        }
    }

    fn elapsed_at(&self, now: u64) -> u64 {
        match self.state {
            SwState::Running { started } => self
                .accumulated_ms
                .saturating_add(now.saturating_sub(started)),
            SwState::Paused => self.accumulated_ms,
            SwState::Idle => 0,
        }
    }

    fn status(&self) -> StopwatchStatusKind {
        match self.state {
            SwState::Idle => StopwatchStatusKind::Idle,
            SwState::Running { .. } => StopwatchStatusKind::Running,
            SwState::Paused => StopwatchStatusKind::Paused,
        }
    }

    fn laps(&self) -> Vec<Lap> {
        self.laps.iter().copied().collect()
    }
}

/// The pure Timer core: three independent sub-states (countdown, pomodoro,
/// stopwatch) driven by injected monotonic `u64` millisecond timestamps.
#[derive(Debug, Clone)]
pub struct TimerCore {
    countdown: Countdown,
    pomodoro: Pomodoro,
    stopwatch: Stopwatch,
    last_seen_now: u64,
}

impl TimerCore {
    /// A fresh core with the given (already validated) pomodoro config.
    pub fn new(cfg: PomodoroConfig) -> Self {
        Self {
            countdown: Countdown::new(),
            pomodoro: Pomodoro::new(cfg),
            stopwatch: Stopwatch::new(),
            last_seen_now: 0,
        }
    }

    /// Total focus phases completed across the pomodoro's lifetime (skips do
    /// not count).
    pub fn completed_focus_count(&self) -> u64 {
        self.pomodoro.completed_focus_count
    }

    /// Clamp `now` to the last seen `now` and record it (mutating ops).
    fn clamp_now(&mut self, now: u64) -> u64 {
        let now = now.max(self.last_seen_now);
        self.last_seen_now = now;
        now
    }

    /// Clamp `now` to the last seen `now` without recording it (pure reads).
    fn clamp_ref(&self, now: u64) -> u64 {
        now.max(self.last_seen_now)
    }

    // ---- countdown ----

    /// Set the countdown duration (1 s ..= 24 h). Only allowed while Idle.
    pub fn set_countdown(&mut self, duration_ms: u64) -> Result<(), TimerError> {
        self.countdown.set(duration_ms)
    }

    /// Start the countdown (from Idle or Finished) at `now`.
    pub fn start(&mut self, now: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.countdown.start(now)
    }

    /// Pause a running countdown at `now`, saving the remaining time.
    pub fn pause(&mut self, now: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.countdown.pause(now)
    }

    /// Resume a paused countdown at `now`.
    pub fn resume(&mut self, now: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.countdown.resume(now)
    }

    /// Stop the countdown and return to Idle (keeping the configured
    /// duration so it can be started again).
    pub fn reset(&mut self) {
        self.countdown.reset();
    }

    /// Add `delta_ms` to the countdown (Running or Paused only), capping the
    /// total remaining at 24 h.
    pub fn add_time(&mut self, now: u64, delta_ms: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.countdown.add_time(now, delta_ms)
    }

    // ---- pomodoro ----

    /// Start the pomodoro: from Idle begins a Focus phase; from Paused
    /// resumes the paused phase. `now`.
    pub fn pomo_start(&mut self, now: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.pomodoro.start(now)
    }

    /// Pause the current pomodoro phase at `now`.
    pub fn pomo_pause(&mut self, now: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.pomodoro.pause(now)
    }

    /// Resume a paused pomodoro phase at `now`.
    pub fn pomo_resume(&mut self, now: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.pomodoro.resume(now)
    }

    /// Reset the pomodoro to Idle (completed focus count cleared).
    pub fn pomo_reset(&mut self) {
        self.pomodoro.reset();
    }

    /// End the current phase immediately without counting a Focus as
    /// completed. The next phase auto-starts from `now` (if configured) or
    /// is armed paused at full length.
    pub fn pomo_skip(&mut self, now: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.pomodoro.skip(now)
    }

    // ---- stopwatch ----

    /// Start (or restart after pause) the stopwatch at `now`.
    pub fn sw_start(&mut self, now: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.stopwatch.start(now)
    }

    /// Pause the stopwatch at `now`, accumulating the elapsed segment.
    pub fn sw_pause(&mut self, now: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.stopwatch.pause(now)
    }

    /// Resume the stopwatch at `now`.
    pub fn sw_resume(&mut self, now: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.stopwatch.resume(now)
    }

    /// Reset the stopwatch to Idle, clearing elapsed time and all laps.
    pub fn sw_reset(&mut self) {
        self.stopwatch.reset();
    }

    /// Record a lap at `now` (Running only). Beyond [`MAX_LAPS`] the oldest
    /// lap is dropped; lap numbering keeps counting.
    pub fn sw_lap(&mut self, now: u64) -> Result<(), TimerError> {
        let now = self.clamp_now(now);
        self.stopwatch.lap(now)
    }

    // ---- time processing ----

    /// Process every deadline ≤ `now` in chronological order and return the
    /// events that fired. A countdown finish fires [`TimerEvent::CountdownFinished`];
    /// each elapsed pomodoro phase fires a
    /// [`TimerEvent::PomodoroPhaseEnded`] (capped at
    /// [`MAX_EVENTS_PER_TICK`] per call — leftovers spill into the next
    /// `tick`). Calling `tick` twice with the same `now` emits nothing the
    /// second time once all due deadlines have been drained.
    pub fn tick(&mut self, now: u64) -> Vec<TimerEvent> {
        let now = self.clamp_now(now);
        let mut events = Vec::new();

        let cd_deadline = self.countdown.deadline();
        let po_deadline = self.pomodoro.deadline();
        let cd_due = match cd_deadline {
            Some(d) => d <= now,
            None => false,
        };
        let po_due = match po_deadline {
            Some(d) => d <= now,
            None => false,
        };

        match (cd_due, po_due) {
            (true, true) => {
                // Chronological order: the earlier deadline's events first.
                let cd_first = match (cd_deadline, po_deadline) {
                    (Some(c), Some(p)) => c <= p,
                    _ => false,
                };
                if cd_first {
                    if let Some(e) = self.countdown.tick(now) {
                        events.push(e);
                    }
                    events.extend(self.pomodoro.tick(now));
                } else {
                    events.extend(self.pomodoro.tick(now));
                    if let Some(e) = self.countdown.tick(now) {
                        events.push(e);
                    }
                }
            }
            (true, false) => {
                if let Some(e) = self.countdown.tick(now) {
                    events.push(e);
                }
            }
            (false, true) => {
                events.extend(self.pomodoro.tick(now));
            }
            (false, false) => {}
        }

        events
    }

    /// Earliest pending deadline of the countdown / pomodoro (the stopwatch
    /// has none); `None` when nothing is running.
    pub fn next_deadline(&self) -> Option<u64> {
        match (self.countdown.deadline(), self.pomodoro.deadline()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// Immutable snapshot of all three sub-states at `now`. Pure read: never
    /// fires events and never advances the core's clock.
    pub fn snapshot(&self, now: u64) -> TimerSnapshot {
        let now = self.clamp_ref(now);
        TimerSnapshot {
            countdown: CountdownSnapshot {
                status: self.countdown.status(),
                remaining_ms: self.countdown.remaining_at(now),
            },
            pomodoro: PomodoroSnapshot {
                status: self.pomodoro.status(),
                phase: self.pomodoro.phase(),
                remaining_ms: self.pomodoro.remaining_at(now),
                round: self.pomodoro.round(),
            },
            stopwatch: StopwatchSnapshot {
                status: self.stopwatch.status(),
                elapsed_ms: self.stopwatch.elapsed_at(now),
                laps: self.stopwatch.laps(),
            },
        }
    }
}
