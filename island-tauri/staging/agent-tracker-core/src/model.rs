//! Wire types, config, and the derived run/chip/event model for the agent
//! run tracker. Wire types are camelCase via serde and accept no aliases.

use crate::error::ConfigError;
use serde::{Deserialize, Serialize};

/// Default staleness window: 15 minutes without activity = stale.
pub const DEFAULT_STALE_AFTER_SECS: u64 = 900;

/// Minimum accepted `staleAfterSecs`.
pub const MIN_STALE_AFTER_SECS: u64 = 60;

/// Maximum accepted `staleAfterSecs`.
pub const MAX_STALE_AFTER_SECS: u64 = 86400;

/// Default history window: rows started more than 24h ago are ignored.
pub const DEFAULT_LOOKBACK_SECS: u64 = 86400;

/// Default maximum number of chips shown.
pub const DEFAULT_MAX_CHIPS: u32 = 6;

/// Minimum accepted `maxChips`.
pub const MIN_MAX_CHIPS: u32 = 1;

/// Maximum accepted `maxChips`.
pub const MAX_MAX_CHIPS: u32 = 32;

/// A row of the `sessions` table read (read-only) from
/// `~/.hermes/state.db`. Times are epoch seconds as floats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: String,
    pub source: String,
    pub parent_session_id: Option<String>,
    pub model: Option<String>,
    pub billing_provider: Option<String>,
    pub started_at: f64,
    pub ended_at: Option<f64>,
    pub end_reason: Option<String>,
    pub last_activity_at: Option<f64>,
    pub api_call_count: Option<u64>,
    pub title: Option<String>,
}

/// One live chip for the island UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chip {
    pub id: String,
    /// Title, else model, else the short id (first 8 chars after a
    /// `local-` prefix).
    pub label: String,
    pub model: Option<String>,
    /// `billingProvider`.
    pub provider: Option<String>,
    /// Seconds since `startedAt` (may be slightly negative near the clock
    /// skew boundary).
    pub elapsed_secs: f64,
    pub api_call_count: Option<u64>,
    /// True for sources listed in `delegatedSources`.
    pub is_delegated: bool,
    /// Number of running subagents folded into this chip.
    pub children: u32,
}

/// A state-transition event from a [`Tracker`](crate::Tracker) update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    /// First time the session was seen Running.
    Started { id: String },
    /// A previously-Running session now has `endedAt`.
    Finished {
        id: String,
        reason: Option<String>,
        elapsed_secs: f64,
    },
    /// A previously-Running session went past the stale window.
    Stalled { id: String },
    /// A previously-Running session is missing from this update's rows.
    Disappeared { id: String },
}

/// The result of one [`Tracker::update`](crate::Tracker::update).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub chips: Vec<Chip>,
    pub events: Vec<Event>,
    /// Number of running chips beyond `maxChips` (not shown).
    pub overflow: u32,
}

/// The classification of one row at a point in time.
#[derive(Debug, Clone, PartialEq)]
pub enum RunState {
    /// No `endedAt` and activity within the stale window.
    Running,
    /// No `endedAt` and older than the stale window. `clock_skew` is true
    /// when the row was rejected only because a timestamp is more than 300s
    /// in the future (never Running forever).
    Stale { clock_skew: bool },
    /// `endedAt` is set.
    Finished { reason: Option<String> },
    /// A timestamp was NaN / infinite / negative; the row is unusable.
    Rejected { reason: String },
}

/// A non-fatal diagnostic counted by the tracker.
#[derive(Debug, Clone, PartialEq)]
pub enum WarningKind {
    /// A row had an invalid (NaN/inf/negative) timestamp and was dropped.
    InvalidTime { id: String, reason: String },
    /// A row was classified Stale only because of a future timestamp
    /// (clock skew) more than 300s ahead.
    ClockSkew { id: String },
    /// Two rows shared an id; the one with the latest activity was kept.
    DuplicateId { id: String },
}

/// Tracker configuration. Construct via [`TrackerConfig::try_new`] or
/// [`TrackerConfig::default`]; fields are private so the validated ranges
/// (staleAfterSecs 60..=86400, maxChips 1..=32, positive lookbackSecs,
/// non-empty delegated sources) cannot be violated by construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerConfig {
    stale_after_secs: u64,
    lookback_secs: u64,
    max_chips: u32,
    delegated_sources: Vec<String>,
}

impl TrackerConfig {
    /// Validate and build a config. Returns a typed [`ConfigError`] on any
    /// out-of-range or empty-slot value; never clamps.
    pub fn try_new(
        stale_after_secs: u64,
        lookback_secs: u64,
        max_chips: u32,
        delegated_sources: Vec<String>,
    ) -> Result<Self, ConfigError> {
        if !(MIN_STALE_AFTER_SECS..=MAX_STALE_AFTER_SECS).contains(&stale_after_secs) {
            return Err(ConfigError::StaleAfterSecsOutOfRange);
        }
        if lookback_secs == 0 {
            return Err(ConfigError::LookbackSecsZero);
        }
        if !(MIN_MAX_CHIPS..=MAX_MAX_CHIPS).contains(&max_chips) {
            return Err(ConfigError::MaxChipsOutOfRange);
        }
        if delegated_sources.iter().any(|s| s.is_empty()) {
            return Err(ConfigError::EmptyDelegatedSource);
        }
        Ok(Self {
            stale_after_secs,
            lookback_secs,
            max_chips,
            delegated_sources,
        })
    }

    /// The staleness window in seconds.
    pub fn stale_after_secs(&self) -> u64 {
        self.stale_after_secs
    }

    /// The history window in seconds: rows started earlier than
    /// `now - lookbackSecs` are ignored.
    pub fn lookback_secs(&self) -> u64 {
        self.lookback_secs
    }

    /// The maximum number of chips shown.
    pub fn max_chips(&self) -> u32 {
        self.max_chips
    }

    /// The sources treated as delegated runs.
    pub fn delegated_sources(&self) -> &[String] {
        &self.delegated_sources
    }

    /// Whether `source` is a delegated source.
    pub fn is_delegated_source(&self, source: &str) -> bool {
        self.delegated_sources.iter().any(|s| s == source)
    }
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            stale_after_secs: DEFAULT_STALE_AFTER_SECS,
            lookback_secs: DEFAULT_LOOKBACK_SECS,
            max_chips: DEFAULT_MAX_CHIPS,
            delegated_sources: vec!["oneshot".to_string(), "subagent".to_string()],
        }
    }
}
