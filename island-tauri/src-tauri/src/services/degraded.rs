//! Optional subsystems that failed to start.
//!
//! Startup used to abort on the first optional subsystem that failed (a stale socket, an
//! HTTP client that could not be built), leaving no island at all. Now only what is needed
//! to show the window may abort; anything else is recorded here, logged, and reported to the
//! page as `available: false` with the reason code `start-failed`.
use crate::contract::system::ServiceStatus;
use std::{collections::BTreeMap, fmt::Display, sync::Mutex};

/// Reason code for a subsystem that failed to start (wording in ui/island/service-reasons.ts).
pub const START_FAILED: &str = "start-failed";

#[derive(Default)]
pub struct Degraded(Mutex<BTreeMap<&'static str, String>>);

impl Degraded {
    pub fn record(&self, name: &'static str, error: &dyn Display) {
        tracing::error!(subsystem = name, %error, "Optional subsystem failed to start; the island runs without it");
        let mut failed = match self.0.lock() {
            Ok(failed) => failed,
            Err(poisoned) => poisoned.into_inner(),
        };
        failed.insert(name, error.to_string());
    }
    pub fn failed(&self) -> Vec<&'static str> {
        match self.0.lock() {
            Ok(failed) => failed.keys().copied().collect(),
            Err(poisoned) => poisoned.into_inner().keys().copied().collect(),
        }
    }
}

/// `Some(value)` on success; on failure records `name` and returns `None`.
pub fn optional<T, E: Display>(
    name: &'static str,
    result: Result<T, E>,
    degraded: &Degraded,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            degraded.record(name, &error);
            None
        }
    }
}

/// The status a missing service reports in the bootstrap snapshot.
pub fn start_failed(service: &str) -> ServiceStatus {
    ServiceStatus {
        rev: 0,
        service: service.into(),
        available: false,
        detail: START_FAILED.into(),
        connects: 0,
    }
}
