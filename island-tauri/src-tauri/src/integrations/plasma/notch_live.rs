//! Live states for the collapsed Plasma notch (Dynamic-Island style): "Hermes is working",
//! "Hermes is waiting for you", a running/paused timer, and a muted microphone. Pure: callers sample the services.
//!
//! The payload only changes on real transitions, because it rides the existing notch
//! appearance push (one plasmashell script call per change, never per second). A running
//! countdown is sent as a wall-clock end time and counted down inside the widget.
use serde_json::{json, Value};

/// A countdown end time that moved by less than this is treated as unchanged, so sampling
/// jitter between the monotonic and wall clocks never causes a push.
pub const END_JITTER_MS: i64 = 1_500;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimerLive {
    /// Counting down; ends at this Unix time (ms).
    Running { ends_at_ms: i64, pomodoro: bool },
    /// Paused with this much left.
    Paused { remaining_ms: i64, pomodoro: bool },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NotchLive {
    pub working: bool,
    /// Hermes asks for approval or an answer; the notch shows it over `working`.
    pub waiting: bool,
    pub timer: Option<TimerLive>,
    pub mic_muted: bool,
}

/// Pick what the notch shows from a timer snapshot taken `age_ms` ago. Countdown wins over
/// pomodoro; the stopwatch is not shown (it has no end to count toward).
pub fn timer_live(
    countdown_status: &str,
    remaining_ms: f64,
    pomodoro_status: &str,
    phase_remaining_ms: f64,
    age_ms: u64,
    now_unix_ms: i64,
) -> Option<TimerLive> {
    let pick = |status: &str, remaining: f64, pomodoro: bool| {
        let remaining = remaining.max(0.0) as i64;
        match status {
            "running" => Some(TimerLive::Running {
                ends_at_ms: now_unix_ms + remaining - age_ms as i64,
                pomodoro,
            }),
            "paused" => Some(TimerLive::Paused {
                remaining_ms: remaining,
                pomodoro,
            }),
            _ => None,
        }
    };
    pick(countdown_status, remaining_ms, false)
        .or_else(|| pick(pomodoro_status, phase_remaining_ms, true))
}

impl NotchLive {
    /// Keep the previously sent end time when the new one is only jitter.
    pub fn stabilised(mut self, previous: Option<&NotchLive>) -> Self {
        if let (
            Some(TimerLive::Running { ends_at_ms, .. }),
            Some(NotchLive {
                timer:
                    Some(TimerLive::Running {
                        ends_at_ms: before, ..
                    }),
                ..
            }),
        ) = (self.timer.as_mut(), previous)
        {
            if (*ends_at_ms - before).abs() < END_JITTER_MS {
                *ends_at_ms = *before;
            }
        }
        self
    }

    pub fn to_json(&self) -> Value {
        let timer = match self.timer {
            None => Value::Null,
            Some(TimerLive::Running {
                ends_at_ms,
                pomodoro,
            }) => json!({"endsAt": ends_at_ms, "paused": false, "pomodoro": pomodoro}),
            Some(TimerLive::Paused {
                remaining_ms,
                pomodoro,
            }) => json!({"remainingMs": remaining_ms, "paused": true, "pomodoro": pomodoro}),
        };
        json!({"working": self.working, "waiting": self.waiting, "timer": timer, "micMuted": self.mic_muted})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_countdown_becomes_a_wall_clock_end_corrected_for_sample_age() {
        let live = timer_live("running", 90_000.0, "idle", 0.0, 4_000, 1_000_000);
        assert_eq!(
            live,
            Some(TimerLive::Running {
                ends_at_ms: 1_086_000,
                pomodoro: false
            })
        );
    }

    #[test]
    fn paused_idle_finished_and_pomodoro_fallback() {
        assert_eq!(
            timer_live("paused", 30_000.0, "running", 5.0, 9, 0),
            Some(TimerLive::Paused {
                remaining_ms: 30_000,
                pomodoro: false
            })
        );
        assert_eq!(timer_live("idle", 0.0, "idle", 0.0, 0, 0), None);
        assert_eq!(timer_live("finished", 0.0, "idle", 0.0, 0, 0), None);
        assert_eq!(
            timer_live("idle", 0.0, "running", 60_000.0, 0, 10),
            Some(TimerLive::Running {
                ends_at_ms: 60_010,
                pomodoro: true
            })
        );
    }

    #[test]
    fn jitter_is_absorbed_but_real_changes_are_not() {
        let at = |ends_at_ms| NotchLive {
            timer: Some(TimerLive::Running {
                ends_at_ms,
                pomodoro: false,
            }),
            ..Default::default()
        };
        let sent = at(100_000);
        assert_eq!(at(100_900).stabilised(Some(&sent)), sent);
        assert_eq!(at(130_000).stabilised(Some(&sent)), at(130_000));
        assert_eq!(at(100_900).stabilised(None), at(100_900));
    }

    #[test]
    fn json_shape_is_small_and_stable() {
        let idle = NotchLive::default().to_json();
        assert_eq!(
            idle,
            json!({"working": false, "waiting": false, "timer": null, "micMuted": false})
        );
        let busy = NotchLive {
            working: true,
            waiting: true,
            mic_muted: true,
            timer: Some(TimerLive::Paused {
                remaining_ms: 5,
                pomodoro: true,
            }),
        }
        .to_json();
        assert_eq!(busy["timer"]["paused"], json!(true));
        assert_eq!(busy["timer"]["remainingMs"], json!(5));
        assert_eq!(busy["waiting"], json!(true));
    }
}
