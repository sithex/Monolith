//! One framed line in, typed reply + store deltas out. No buffering or transport.
use super::types::{nonempty, time, ttl};
use super::*;
use serde::{Deserialize, Serialize};

pub const MAX_LINE_BYTES: usize = 16 * 1024;
pub const MESSAGES_PER_SECOND: u32 = 20;
pub const MAX_CONNECTIONS: u32 = 8;
pub const IDLE_TIMEOUT_MS: f64 = 10_000.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

/// Strict operation-specific inputs. No alias names, permissive Value fallback,
/// or unknown cross-operation fields. Validation also runs on directly built values.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(
    tag = "op",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Request {
    Notify {
        v: u32,
        title: String,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        level: Level,
        #[serde(default)]
        ttl_ms: Option<f64>,
        #[serde(default)]
        source: Option<String>,
        #[serde(default)]
        dedupe_key: Option<String>,
    },
    Progress {
        v: u32,
        dedupe_key: String,
        title: String,
        progress: f64,
    },
    Clear {
        v: u32,
        dedupe_key: String,
    },
}
impl Request {
    pub fn validate(&self) -> Result<(), Error> {
        let v = match self {
            Self::Notify { v, .. } | Self::Progress { v, .. } | Self::Clear { v, .. } => *v,
        };
        if v != 1 {
            return Err(Error::invalid("unsupported protocol version"));
        }
        match self {
            Self::Notify {
                title,
                body,
                ttl_ms,
                source,
                dedupe_key,
                ..
            } => {
                nonempty(title, MAX_TITLE_BYTES)?;
                if let Some(body) = body {
                    super::types::bounded(body, MAX_BODY_BYTES)?;
                }
                if let Some(value) = ttl_ms {
                    ttl(*value)?;
                }
                if let Some(value) = source {
                    nonempty(value, MAX_NAME_BYTES)?;
                }
                if let Some(value) = dedupe_key {
                    nonempty(value, MAX_NAME_BYTES)?;
                }
            }
            Self::Progress {
                title,
                dedupe_key,
                progress,
                ..
            } => {
                nonempty(title, MAX_TITLE_BYTES)?;
                nonempty(dedupe_key, MAX_NAME_BYTES)?;
                if !progress.is_finite() || !(0.0..=1.0).contains(progress) {
                    return Err(Error::invalid("progress out of range"));
                }
            }
            Self::Clear { dedupe_key, .. } => nonempty(dedupe_key, MAX_NAME_BYTES)?,
        }
        Ok(())
    }
    pub fn apply(&self, store: &mut NotificationStore, now_ms: f64) -> Result<Changes, Error> {
        self.validate()?;
        time(now_ms)?;
        match self {
            Self::Notify {
                title,
                body,
                level,
                ttl_ms,
                source,
                dedupe_key,
                ..
            } => {
                let mut input = NotificationInput::hermes(title);
                input.body = body.clone();
                input.app_name = source.clone().unwrap_or_else(|| "Hermes".into());
                input.dedupe_key = dedupe_key.clone();
                input.ttl_ms = *ttl_ms;
                input.urgency = match level {
                    Level::Error => Urgency::Critical,
                    Level::Info | Level::Success | Level::Warning => Urgency::Normal,
                };
                store.insert(input, now_ms)
            }
            Self::Progress {
                title,
                dedupe_key,
                progress,
                ..
            } => {
                let mut input = NotificationInput::hermes(title);
                input.dedupe_key = Some(dedupe_key.clone());
                input.progress = Some(*progress);
                // In-flight progress remains until replaced/cleared; completion uses normal TTL.
                input.ttl_ms = if *progress < 1.0 { Some(0.0) } else { None };
                store.insert(input, now_ms)
            }
            Self::Clear { dedupe_key, .. } => Ok(store.clear_key(dedupe_key)),
        }
    }
}

/// Inclusive payload-byte cap, excluding ONE final LF (and optional preceding CR).
/// A missing final newline is accepted for callers that already removed framing.
pub fn parse_line(line: &[u8]) -> Result<Request, Error> {
    let payload = if let Some(without_lf) = line.strip_suffix(b"\n") {
        without_lf.strip_suffix(b"\r").unwrap_or(without_lf)
    } else {
        line
    };
    if payload.len() > MAX_LINE_BYTES {
        return Err(Error::invalid("line exceeds 16 KiB"));
    }
    if payload.contains(&b'\n') || payload.contains(&b'\r') {
        return Err(Error::invalid("expected one JSON line"));
    }
    // Do not expose serde's source excerpts or request contents in error messages.
    let request: Request = serde_json::from_slice(payload)
        .map_err(|_| Error::invalid("invalid request shape or JSON"))?;
    request.validate()?;
    Ok(request)
}

#[derive(Clone, Debug)]
pub struct RateLimiter {
    tokens: f64,
    last_ms: f64,
}
impl RateLimiter {
    pub fn new(now_ms: f64) -> Result<Self, Error> {
        time(now_ms)?;
        Ok(Self {
            tokens: f64::from(MESSAGES_PER_SECOND),
            last_ms: now_ms,
        })
    }
    /// Bucket capacity 20, refill 20/s. Rewinds mint nothing; nonfinite is an error.
    pub fn check(&mut self, now_ms: f64) -> Result<bool, Error> {
        time(now_ms)?;
        if now_ms > self.last_ms {
            self.tokens = (self.tokens
                + (now_ms - self.last_ms) * f64::from(MESSAGES_PER_SECOND) / 1000.0)
                .min(f64::from(MESSAGES_PER_SECOND));
            self.last_ms = now_ms;
        }
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Error>,
}
impl Reply {
    fn success(id: Option<String>) -> Self {
        Self {
            ok: true,
            id,
            error: None,
        }
    }
    fn failure(error: Error) -> Self {
        Self {
            ok: false,
            id: None,
            error: Some(error),
        }
    }
    pub fn is_ok(&self) -> bool {
        self.ok
    }
    pub fn error(&self) -> Option<&Error> {
        self.error.as_ref()
    }
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
    pub fn to_line(&self) -> Result<String, serde_json::Error> {
        let mut line = serde_json::to_string(self)?;
        line.push('\n');
        Ok(line)
    }
}
#[derive(Clone, Debug)]
pub struct IngestOutcome {
    pub reply: Reply,
    pub changes: Changes,
}
#[derive(Clone, Debug)]
pub struct IngestSession {
    limiter: RateLimiter,
}
impl IngestSession {
    pub fn new(now_ms: f64) -> Result<Self, Error> {
        Ok(Self {
            limiter: RateLimiter::new(now_ms)?,
        })
    }
    /// All attempted frames count, including malformed and oversized requests.
    pub fn handle(
        &mut self,
        store: &mut NotificationStore,
        line: &[u8],
        now_ms: f64,
    ) -> IngestOutcome {
        let result = (|| {
            if !self.limiter.check(now_ms)? {
                return Err(Error {
                    code: ErrorCode::RateLimited,
                    message: "rate limit exceeded".into(),
                });
            }
            let request = parse_line(line)?;
            let changes = request.apply(store, now_ms)?;
            if !matches!(request, Request::Clear { .. }) && changes.id.is_none() {
                return Err(Error {
                    code: ErrorCode::Filtered,
                    message: "notification filtered".into(),
                });
            }
            Ok(changes)
        })();
        match result {
            Ok(changes) => IngestOutcome {
                reply: Reply::success(changes.id.clone()),
                changes,
            },
            Err(error) => IngestOutcome {
                reply: Reply::failure(error),
                changes: Changes::default(),
            },
        }
    }
}
