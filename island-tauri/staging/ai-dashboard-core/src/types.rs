use serde::{Deserialize, Serialize};

/// One usage row decoded by the Tauri side (one session may have several
/// rows, e.g. on model switching; rows are attributed per row). All `Option`
/// fields mean "unknown", never 0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    pub session_id: String,
    /// Unix seconds (UTC).
    pub started_at_s: i64,
    pub ended_at_s: Option<i64>,
    pub provider: String,
    pub model: String,
    pub project: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
}

/// A provider limit window (session / weekly / free-text label).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitWindow {
    pub provider: String,
    pub label: String,
    pub used_percent: Option<f64>,
    pub resets_at_s: Option<i64>,
}

/// Computed dashboard (all cards).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub limits: Vec<LimitCard>,
    pub spend_today_usd: Option<f64>,
    pub spend_today_partial: bool,
    pub active: Option<ActiveSession>,
    pub hourly: Vec<HourBucket>,
    pub by_model: Vec<CostLine>,
    pub by_project: Vec<CostLine>,
    pub heatmap: [[u32; 24]; 7],
    pub rows_ignored: u32,
}

/// Per-provider limit card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitCard {
    pub provider: String,
    pub label: String,
    /// Clamped to 0..=100; NaN/±inf → None.
    pub used_percent: Option<f64>,
    /// resetsAt − now; None if unknown; if already past → Some(0).
    pub resets_in_s: Option<i64>,
    /// "now" / "<1m" / "Nm" / "Hh Mm" / "Dd Hh".
    pub reset_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSession {
    pub session_id: String,
    pub provider: String,
    pub model: String,
    pub started_at_s: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HourBucket {
    /// Unix seconds at which this local hour starts.
    pub start_s: i64,
    /// Sum of known input+output tokens (saturating); None if every row in
    /// the bucket lacks both.
    pub tokens: Option<u64>,
    /// Distinct session ids started in the bucket.
    pub sessions: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostLine {
    pub key: String,
    pub tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub rows: u32,
}
