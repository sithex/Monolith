use serde::{Deserialize, Serialize};
use std::fmt;

pub const STORE_CAPACITY: usize = 200;
pub const MAX_NAME_BYTES: usize = 256;
pub const MAX_TITLE_BYTES: usize = 1024;
pub const MAX_BODY_BYTES: usize = 8192;
pub const MAX_ACTIONS: usize = 32;
pub const MAX_ICON_BYTES: usize = 64 * 1024;
pub const DEFAULT_TTL_MS: f64 = 6000.0;
pub const MAX_TTL_MS: f64 = 86_400_000.0;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    Invalid,
    RateLimited,
    Filtered,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
}

impl Error {
    pub fn invalid(message: &str) -> Self {
        Self {
            code: ErrorCode::Invalid,
            message: message.into(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}

pub(crate) fn bounded(value: &str, max: usize) -> Result<(), Error> {
    if value.len() > max {
        return Err(Error::invalid("string exceeds byte limit"));
    }
    Ok(())
}
pub(crate) fn nonempty(value: &str, max: usize) -> Result<(), Error> {
    bounded(value, max)?;
    if value.trim().is_empty() {
        return Err(Error::invalid("required string is empty"));
    }
    Ok(())
}
pub(crate) fn time(now_ms: f64) -> Result<(), Error> {
    if !now_ms.is_finite() || now_ms < 0.0 {
        return Err(Error::invalid("nowMs must be finite and nonnegative"));
    }
    Ok(())
}
pub(crate) fn ttl(ttl_ms: f64) -> Result<(), Error> {
    if !ttl_ms.is_finite() || !(0.0..=MAX_TTL_MS).contains(&ttl_ms) {
        return Err(Error::invalid("ttlMs out of range"));
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NotificationSource {
    System,
    Hermes,
    Alert,
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationAction {
    pub key: String,
    pub label: String,
}

/// Output payload matching SPEC §5.2, with the queue topic revision added.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IslandNotification {
    pub rev: u32,
    pub id: String,
    pub source: NotificationSource,
    pub app_name: String,
    pub title: String,
    pub body: Option<String>,
    pub icon_png: Option<String>,
    pub urgency: Urgency,
    pub received_at: f64,
    pub actions: Vec<NotificationAction>,
    pub can_invoke_actions: bool,
    pub progress: Option<f64>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct NotificationReceived(pub IslandNotification);
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CloseReason {
    Expired,
    Dismissed,
    Cleared,
    Evicted,
    Filtered,
    Replaced,
    ServerClosed,
}
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationClosed {
    pub rev: u32,
    pub id: String,
    pub reason: CloseReason,
}

/// Convenience envelope; callers emit the contained payload on its named topic.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "camelCase")]
pub enum Delta {
    NotificationReceived(NotificationReceived),
    NotificationClosed(NotificationClosed),
}

/// Rust-only candidate, NOT a deserialization boundary. Every store insert validates it.
#[derive(Clone, Debug, PartialEq)]
pub struct NotificationInput {
    pub source: NotificationSource,
    pub app_name: String,
    pub title: String,
    pub body: Option<String>,
    pub icon_png: Option<String>,
    pub urgency: Urgency,
    pub actions: Vec<NotificationAction>,
    pub progress: Option<f64>,
    pub dedupe_key: Option<String>,
    pub server_id: Option<u32>,
    pub replaces_id: u32,
    pub transient: bool,
    /// None: default policy, Some(0): resident, Some(n): explicit lifetime.
    pub ttl_ms: Option<f64>,
}
impl NotificationInput {
    pub fn hermes(title: impl Into<String>) -> Self {
        Self {
            source: NotificationSource::Hermes,
            app_name: "Hermes".into(),
            title: title.into(),
            body: None,
            icon_png: None,
            urgency: Urgency::Normal,
            actions: vec![],
            progress: None,
            dedupe_key: None,
            server_id: None,
            replaces_id: 0,
            transient: false,
            ttl_ms: None,
        }
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        nonempty(&self.app_name, MAX_NAME_BYTES)?;
        nonempty(&self.title, MAX_TITLE_BYTES)?;
        if let Some(body) = &self.body {
            bounded(body, MAX_BODY_BYTES)?;
        }
        if let Some(icon) = &self.icon_png {
            bounded(icon, MAX_ICON_BYTES)?;
            if !icon.starts_with("data:image/png;base64,") {
                return Err(Error::invalid("icon must be a PNG data URL"));
            }
        }
        if let Some(key) = &self.dedupe_key {
            nonempty(key, MAX_NAME_BYTES)?;
            if self.source != NotificationSource::Hermes {
                return Err(Error::invalid("dedupeKey is Hermes-only"));
            }
        }
        if self.actions.len() > MAX_ACTIONS {
            return Err(Error::invalid("too many actions"));
        }
        for action in &self.actions {
            nonempty(&action.key, MAX_NAME_BYTES)?;
            bounded(&action.label, MAX_NAME_BYTES)?;
        }
        if let Some(progress) = self.progress {
            if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
                return Err(Error::invalid("progress out of range"));
            }
        }
        if let Some(value) = self.ttl_ms {
            ttl(value)?;
        }
        if self.server_id == Some(0)
            || (self.source != NotificationSource::System
                && (self.server_id.is_some() || self.replaces_id != 0))
        {
            return Err(Error::invalid("invalid server identity"));
        }
        Ok(())
    }
}

/// Every mutation returns its observable changes, even when empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changes {
    pub events: Vec<Delta>,
    pub id: Option<String>,
    pub toast: bool,
    /// Best-effort CloseNotification requests; no transport work happens here.
    pub close_server_ids: Vec<u32>,
}
