//! Date-time value forms (RFC 5545 §3.3.10 / §3.3.11 subset).

use crate::civil::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// How a [`DateTimeValue`] wall-clock time is anchored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DateTimeKind {
    /// `VALUE=DATE` — all-day, local date (midnight local).
    Date,
    /// Trailing `Z` — UTC.
    Utc,
    /// No timezone info — floating (local time of the caller).
    Floating,
    /// `TZID=...` — needs the caller-provided resolver.
    Zoned { tzid: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateTimeValue {
    pub naive: NaiveDateTime,
    pub kind: DateTimeKind,
}
