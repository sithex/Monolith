//! Pure aggregation core for the Hermes Island **Phase 7** AI-stats
//! dashboard (SPEC §0.5 "Design direction" — Apps bullet: "AI stats as a
//! card dashboard (per-provider session/week bars with reset countdowns,
//! spending today, "now" active session, hourly trend, per-model and
//! per-project cost, activity heatmap; unknown data shown unavailable, never
//! fabricated)"). The Tauri side reads Hermes' local metadata (read-only
//! SQLite) and `hermes usage --json`, then feeds this crate already-decoded
//! rows; this crate computes the dashboard cards.
//!
//! Platform-free: no IO, no SQLite, no network, no threads, no clock, no
//! `unsafe`, no `chrono` (civil-date math is implemented here). All time
//! inputs are injected (`now_s`, `utc_offset_s`). The crate never panics on
//! any input.

#![forbid(unsafe_code)]

mod dashboard;
mod error;
pub mod time;
mod types;

pub use dashboard::build_dashboard;
pub use error::DashError;
pub use time::{
    civil_from_days, days_from_civil, hour_start_utc, local_day_index, local_hour_index,
    local_weekday_and_hour, weekday,
};
pub use types::{ActiveSession, CostLine, Dashboard, HourBucket, LimitCard, LimitWindow, UsageRow};
