//! Shared availability report for the OS-integration services (SPEC §3.5, §4.5 `ServiceStatus`).
//!
//! A service is `available: false` with a reason whenever its daemon/endpoint is not
//! reachable, and values it cannot know are `null` rather than zero. The report is a
//! latest-value topic: the revision only moves when something observable changed.

use crate::{bus::Latest, contract::system::ServiceStatus};
use std::sync::{Arc, Mutex};

pub struct Reporter {
    inner: Mutex<ServiceStatus>,
    pub topic: Latest<ServiceStatus>,
}

impl Reporter {
    pub fn new(service: &str) -> Arc<Self> {
        let initial = ServiceStatus {
            rev: 0,
            service: service.to_owned(),
            available: false,
            detail: "starting".into(),
            connects: 0,
        };
        let topic = Latest::new();
        topic.publish(initial.clone());
        Arc::new(Self {
            inner: Mutex::new(initial),
            topic,
        })
    }

    pub fn get(&self) -> ServiceStatus {
        match self.inner.lock() {
            Ok(status) => status.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn change(&self, edit: impl FnOnce(&mut ServiceStatus)) {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let before = guard.clone();
        edit(&mut guard);
        if *guard != before {
            guard.rev = guard.rev.wrapping_add(1);
            self.topic.publish(guard.clone());
        }
    }

    /// The service is down or inactive, with a user-presentable reason.
    pub fn unavailable(&self, detail: &str) {
        self.change(|s| {
            s.available = false;
            s.detail = detail.to_owned();
        });
    }

    /// The service reached its daemon/endpoint and has a current value.
    pub fn available(&self, detail: &str) {
        self.change(|s| {
            s.available = true;
            s.detail = detail.to_owned();
        });
    }

    /// A (re)connection to the daemon succeeded.
    pub fn connected(&self) {
        self.change(|s| s.connects = s.connects.wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_moves_only_on_observable_change() {
        let report = Reporter::new("power");
        assert_eq!(report.get().rev, 0);
        report.unavailable("starting");
        assert_eq!(report.get().rev, 0, "same value is not a change");
        report.available("ok");
        report.available("ok");
        let status = report.get();
        assert_eq!((status.rev, status.available), (1, true));
        report.unavailable("name-lost");
        report.connected();
        let status = report.get();
        assert_eq!(
            (status.rev, status.connects, status.detail.as_str()),
            (3, 1, "name-lost")
        );
    }
}
