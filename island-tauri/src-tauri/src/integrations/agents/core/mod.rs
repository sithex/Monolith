//! Pure core behind SPEC §0.5 "More tiles/features Nick picked" item (8) —
//! the Agent run tracker for the Hermes Island widget. The Tauri actor
//! reads the `sessions` table of `~/.hermes/state.db` (read-only) and
//! passes rows here; this crate classifies them (Running / Stale /
//! Finished), builds live chips and emits transition events. It never
//! reads anything, never runs processes, and never panics on any input.
//!
//! Time is always an injected parameter (no clock). Platform-free: no IO,
//! no network, no threads, no `unsafe`. Wire types use `camelCase` through
//! serde and accept no aliases.

#![forbid(unsafe_code)]

mod classify;
mod error;
mod model;
mod tracker;

pub use classify::{classify, valid_time, CLOCK_SKEW_SECS};
pub use error::ConfigError;
pub use model::{
    Chip, Event, RunState, SessionRow, TrackerConfig, Update, WarningKind, DEFAULT_LOOKBACK_SECS,
    DEFAULT_MAX_CHIPS, DEFAULT_STALE_AFTER_SECS, MAX_MAX_CHIPS, MAX_STALE_AFTER_SECS,
    MIN_MAX_CHIPS, MIN_STALE_AFTER_SECS,
};
pub use tracker::{label_for, short_id, Tracker};
