//! Check schedule: at most one check per hour when things work, exponential
//! backoff after failures (10 → 20 → 40 → 60 min cap), and never while on
//! battery unless the badge is stale (> 6 h since the last OK). Time is
//! always an injected parameter.

use crate::integrations::updates::core::model::STALE_AFTER_SECS;

/// Normal cadence between successful checks.
pub const HOURLY_SECS: u64 = 60 * 60;

/// Minutes to wait after `consecutive_failures` failures: 10, 20, 40,
/// then 60 (capped). `0` (defensive — a failure was noted without state)
/// behaves like 1.
pub fn backoff_minutes(consecutive_failures: u32) -> u64 {
    let shift = consecutive_failures.saturating_sub(1).min(4);
    (10u64 << shift).min(60)
}

/// Mutable scheduling state owned by the caller (the Tauri actor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    /// True while running on battery.
    pub on_battery: bool,
    /// Number of consecutive failed checks since the last success.
    pub consecutive_failures: u32,
}

impl Schedule {
    pub fn new(on_battery: bool) -> Self {
        Self {
            on_battery,
            consecutive_failures: 0,
        }
    }

    pub fn note_success(&mut self) {
        self.consecutive_failures = 0;
    }

    pub fn note_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }

    /// Whether a check should run now.
    ///
    /// Rules (see module docs): battery blocks unless stale; after a
    /// success wait at least `; after a failure wait the backoff delay
    /// (which is always ≤ 1 h, so it only *shortens* the wait compared
    /// with the hourly cadence).
    pub fn due(&self, now: u64, last_attempt: Option<u64>, last_ok: Option<u64>) -> bool {
        let overdue = match last_ok {
            None => true,
            Some(ok) => now.saturating_sub(ok) > STALE_AFTER_SECS,
        };
        if self.on_battery && !overdue {
            return false;
        }
        let Some(last_attempt) = last_attempt else {
            return true;
        };
        let last_failed = match last_ok {
            Some(ok) => last_attempt > ok,
            None => true,
        };
        let wait_secs = if last_failed {
            backoff_minutes(self.consecutive_failures) * 60
        } else {
            HOURLY_SECS
        };
        now.saturating_sub(last_attempt) >= wait_secs
    }
}
