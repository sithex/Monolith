//! Row classification: turn a [`SessionRow`] into a [`RunState`] at a
//! given `now`. Total: NaN/infinite/negative timestamps are rejected with a
//! reason string; future timestamps more than 300s ahead are Stale (clock
//! skew), never Running forever.

use crate::integrations::agents::core::model::{RunState, SessionRow, TrackerConfig};

/// A timestamp more than this far in the future is treated as clock skew
/// (Stale), never Running.
pub const CLOCK_SKEW_SECS: f64 = 300.0;

/// Whether `t` is a usable epoch-second timestamp: finite and non-negative.
pub fn valid_time(t: f64) -> bool {
    t.is_finite() && t >= 0.0
}

fn bad_time(t: f64) -> &'static str {
    if t.is_nan() {
        "NaN"
    } else if t.is_infinite() {
        if t.is_sign_positive() {
            "+inf"
        } else {
            "-inf"
        }
    } else if t < 0.0 {
        "negative"
    } else {
        "invalid"
    }
}

/// Classify one row. Validation happens first: any NaN/inf/negative
/// timestamp rejects the whole row (even if `endedAt` is set).
pub fn classify(row: &SessionRow, now_secs: f64, cfg: &TrackerConfig) -> RunState {
    if !valid_time(row.started_at) {
        return RunState::Rejected {
            reason: format!("startedAt {}", bad_time(row.started_at)),
        };
    }
    if let Some(t) = row.last_activity_at {
        if !valid_time(t) {
            return RunState::Rejected {
                reason: format!("lastActivityAt {}", bad_time(t)),
            };
        }
    }
    if let Some(t) = row.ended_at {
        if !valid_time(t) {
            return RunState::Rejected {
                reason: format!("endedAt {}", bad_time(t)),
            };
        }
    }

    if let Some(ended) = row.ended_at {
        // endedAt set -> Finished, regardless of startedAt/activity times.
        let _ = ended;
        return RunState::Finished {
            reason: row.end_reason.clone(),
        };
    }

    // No endedAt: staleness is judged from the most recent activity
    // (lastActivityAt, else startedAt).
    let activity = row.last_activity_at.unwrap_or(row.started_at);
    let ahead = now_secs + CLOCK_SKEW_SECS;
    let skewed = row.started_at > ahead || activity > ahead;
    let active = activity >= now_secs - cfg.stale_after_secs() as f64;

    if skewed {
        RunState::Stale { clock_skew: true }
    } else if active {
        RunState::Running
    } else {
        RunState::Stale { clock_skew: false }
    }
}
