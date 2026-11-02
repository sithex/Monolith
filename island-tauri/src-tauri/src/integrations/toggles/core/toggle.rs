//! Generic optimistic tile state machine shared by all toggles.
//!
//! A request stores an optimistic `pending` value; the system's
//! confirmation (`confirm`) reconciles it with reality, `fail` aborts a
//! specific request, and `tick` ages pendings out back to `confirmed`
//! after [`PENDING_TIMEOUT_MS`].

/// Milliseconds a pending optimistic update is kept before `tick` drops
/// it (display falls back to `confirmed`).
pub const PENDING_TIMEOUT_MS: u64 = 3000;

/// An in-flight optimistic update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending<T> {
    pub value: T,
    pub sent_at_ms: u64,
    pub request_id: u64,
}

/// Optimistic toggle state: a confirmed value plus (optionally) a newer
/// pending request. `display()` shows the pending value while one exists,
/// otherwise the confirmed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToggleState<T> {
    confirmed: Option<T>,
    pending: Option<Pending<T>>,
    next_id: u64,
}

impl<T> ToggleState<T> {
    /// No confirmed value, no pending request.
    pub fn new() -> Self {
        Self {
            confirmed: None,
            pending: None,
            next_id: 0,
        }
    }

    /// Start with a known confirmed value (e.g. from a fresh read).
    pub fn with_confirmed(value: T) -> Self {
        Self {
            confirmed: Some(value),
            pending: None,
            next_id: 0,
        }
    }

    /// The last confirmed value, if any.
    pub fn confirmed(&self) -> Option<&T> {
        self.confirmed.as_ref()
    }

    /// The current pending request, if any.
    pub fn pending(&self) -> Option<&Pending<T>> {
        self.pending.as_ref()
    }

    /// Send an optimistic request, replacing any previous pending. Returns
    /// the new request id.
    pub fn request(&mut self, value: T, now_ms: u64) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.pending = Some(Pending {
            value,
            sent_at_ms: now_ms,
            request_id: id,
        });
        id
    }

    /// Reconcile an observation of the real state. An observation equal to
    /// the pending value clears the pending request; any other observation
    /// updates `confirmed` and keeps the pending request until it times out.
    pub fn confirm(&mut self, observed: T, _now_ms: u64)
    where
        T: PartialEq,
    {
        let matches = match &self.pending {
            Some(p) => p.value == observed,
            None => false,
        };
        if matches {
            self.pending = None;
        }
        self.confirmed = Some(observed);
    }

    /// Report a request as failed. Clears the pending request only if its
    /// id matches; a stale failure for an older id never clears a newer
    /// pending request.
    pub fn fail(&mut self, request_id: u64, _now_ms: u64) {
        let matches = match &self.pending {
            Some(p) => p.request_id == request_id,
            None => false,
        };
        if matches {
            self.pending = None;
        }
    }

    /// Age the pending request; drop it once it is [`PENDING_TIMEOUT_MS`]
    /// old (elapsed >= timeout). A `now_ms` before `sent_at_ms` is treated
    /// as 0 elapsed (never dropped).
    pub fn tick(&mut self, now_ms: u64) {
        let expired = match &self.pending {
            Some(p) => now_ms.saturating_sub(p.sent_at_ms) >= PENDING_TIMEOUT_MS,
            None => false,
        };
        if expired {
            self.pending = None;
        }
    }

    /// The value to show: the pending value while one exists, else the
    /// confirmed value.
    pub fn display(&self) -> Option<T>
    where
        T: Clone,
    {
        match &self.pending {
            Some(p) => Some(p.value.clone()),
            None => self.confirmed.clone(),
        }
    }
}

impl<T> Default for ToggleState<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_sets_pending_and_returns_new_id() {
        let mut s = ToggleState::new();
        let id = s.request(true, 1000);
        assert_eq!(id, 1);
        let p = s.pending().unwrap();
        assert!(p.value);
        assert_eq!(p.sent_at_ms, 1000);
        assert_eq!(p.request_id, 1);
        assert_eq!(s.display(), Some(true));
    }

    #[test]
    fn confirm_equal_clears_pending_and_updates_confirmed() {
        let mut s = ToggleState::new();
        s.request(true, 1000);
        s.confirm(true, 1500);
        assert!(s.pending().is_none());
        assert_eq!(s.confirmed(), Some(&true));
        assert_eq!(s.display(), Some(true));
    }

    #[test]
    fn confirm_different_updates_confirmed_keeps_pending() {
        let mut s = ToggleState::new();
        s.request(true, 1000);
        s.confirm(false, 1500);
        assert_eq!(s.confirmed(), Some(&false));
        assert!(s.pending().is_some());
        assert_eq!(s.display(), Some(true));
    }

    #[test]
    fn confirm_without_pending_updates_confirmed() {
        let mut s = ToggleState::new();
        s.confirm(true, 1000);
        assert_eq!(s.confirmed(), Some(&true));
        assert_eq!(s.display(), Some(true));
    }

    #[test]
    fn fail_matching_id_clears_pending() {
        let mut s = ToggleState::new();
        let id = s.request(true, 1000);
        s.fail(id, 1500);
        assert!(s.pending().is_none());
    }

    #[test]
    fn fail_stale_id_ignored() {
        let mut s = ToggleState::new();
        let id1 = s.request(true, 1000);
        let id2 = s.request(false, 1100);
        assert!(id2 > id1);
        s.fail(id1, 1200);
        assert!(s.pending().is_some());
        assert_eq!(s.pending().unwrap().request_id, id2);
        s.fail(id2, 1300);
        assert!(s.pending().is_none());
    }

    #[test]
    fn fail_when_no_pending_is_noop() {
        let mut s: ToggleState<bool> = ToggleState::new();
        s.fail(42, 1000);
        assert!(s.pending().is_none());
    }

    #[test]
    fn tick_keeps_before_boundary_drops_at_and_after() {
        let mut s = ToggleState::new();
        s.request(true, 1000);
        s.tick(1000 + PENDING_TIMEOUT_MS - 1); // 2999 ms elapsed: kept
        assert!(s.pending().is_some());
        s.tick(1000 + PENDING_TIMEOUT_MS); // 3000 ms elapsed: dropped
        assert!(s.pending().is_none());
    }

    #[test]
    fn tick_never_drops_without_elapsed_time() {
        let mut s = ToggleState::new();
        s.request(true, 5000);
        s.tick(4000); // now before sent -> saturating 0 elapsed
        assert!(s.pending().is_some());
    }

    #[test]
    fn new_request_replaces_pending_with_new_id() {
        let mut s = ToggleState::new();
        let id1 = s.request(true, 1000);
        let id2 = s.request(false, 1100);
        assert!(id2 > id1);
        assert!(!s.pending().unwrap().value);
        assert_eq!(s.pending().unwrap().request_id, id2);
        s.fail(id1, 1200); // stale
        assert!(s.pending().is_some());
    }

    #[test]
    fn display_prefers_pending_then_confirmed() {
        let mut s = ToggleState::with_confirmed(true);
        assert_eq!(s.display(), Some(true));
        s.request(false, 1000);
        assert_eq!(s.display(), Some(false));
        s.fail(1, 1500);
        assert_eq!(s.display(), Some(true));
    }
}
