use super::*;
use std::collections::{BTreeSet, VecDeque};

/// Rust-only settings; host validates/deserializes its own contract.
#[derive(Clone, Debug, PartialEq)]
pub struct StorePolicy {
    pub denied_apps: BTreeSet<String>,
    pub redact_bodies: bool,
    pub dnd: bool,
    pub default_ttl_ms: f64,
}
impl Default for StorePolicy {
    fn default() -> Self {
        Self {
            denied_apps: BTreeSet::new(),
            redact_bodies: false,
            dnd: false,
            default_ttl_ms: DEFAULT_TTL_MS,
        }
    }
}
impl StorePolicy {
    fn validate(&self) -> Result<(), Error> {
        super::types::ttl(self.default_ttl_ms)?;
        for app in &self.denied_apps {
            super::types::nonempty(app, MAX_NAME_BYTES)?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
struct Entry {
    notification: IslandNotification,
    dedupe_key: Option<String>,
    server_id: Option<u32>,
    expires_at: Option<f64>,
    transient: bool,
}

#[derive(Clone, Debug, Default)]
pub struct NotificationStore {
    entries: VecDeque<Entry>,
    policy: StorePolicy,
    next_id: u32,
    rev: u32,
}
impl NotificationStore {
    pub fn new(policy: StorePolicy) -> Result<Self, Error> {
        policy.validate()?;
        Ok(Self {
            policy,
            ..Self::default()
        })
    }
    pub fn override_urgency(&mut self, id: &str, urgency: Urgency) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.notification.id == id) {
            entry.notification.urgency = urgency;
        }
    }
    pub fn retain_history(&mut self) {
        for entry in &mut self.entries {
            entry.expires_at = None;
        }
    }
    pub fn deactivate_server(&mut self, server_id: u32) -> Changes {
        let rev = self.revision();
        let mut changes = Changes::default();
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.server_id == Some(server_id))
        {
            entry.notification.rev = rev;
            entry.notification.actions.clear();
            entry.notification.can_invoke_actions = false;
            entry.expires_at = None;
            changes
                .events
                .push(Delta::NotificationReceived(NotificationReceived(
                    entry.notification.clone(),
                )));
        }
        changes
    }
    pub fn policy(&self) -> &StorePolicy {
        &self.policy
    }
    /// Host loss-repair boundary; includes revisions of entries already closed.
    pub fn current_revision(&self) -> u32 {
        self.rev
    }
    pub fn is_current(&self, id: &str, rev: u32) -> bool {
        self.entries
            .iter()
            .any(|e| e.notification.id == id && e.notification.rev == rev)
    }
    /// Host scheduling query; expiry policy and mutation remain the audited pure core.
    pub fn next_expiry(&self) -> Option<f64> {
        self.entries
            .iter()
            .filter_map(|entry| entry.expires_at)
            .min_by(f64::total_cmp)
    }
    pub fn list(&self) -> Vec<IslandNotification> {
        self.entries
            .iter()
            .map(|e| e.notification.clone())
            .collect()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn is_transient(&self, id: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.notification.id == id && e.transient)
    }
    fn revision(&mut self) -> u32 {
        self.rev = self.rev.wrapping_add(1);
        self.rev
    }
    fn closed(&mut self, id: String, reason: CloseReason) -> Delta {
        Delta::NotificationClosed(NotificationClosed {
            rev: self.revision(),
            id,
            reason,
        })
    }
    fn fresh_id(&mut self, source: NotificationSource) -> String {
        let prefix = match source {
            NotificationSource::Hermes => "h",
            NotificationSource::System => "s",
            NotificationSource::Alert => "a",
        };
        loop {
            self.next_id = self.next_id.wrapping_add(1);
            let id = format!("{prefix}-{}", self.next_id);
            if !self.entries.iter().any(|e| e.notification.id == id) {
                return id;
            }
        }
    }
    /// Atomic on validation error. Expiry is explicit, never an implicit side effect.
    pub fn insert(&mut self, input: NotificationInput, now_ms: f64) -> Result<Changes, Error> {
        input.validate()?;
        super::types::time(now_ms)?;
        if input.app_name == "Hermes Island" || self.policy.denied_apps.contains(&input.app_name) {
            return Ok(Changes::default());
        }
        let lifetime = input.ttl_ms.unwrap_or_else(|| {
            if input.urgency == Urgency::Critical && !input.transient {
                0.0
            } else {
                self.policy.default_ttl_ms
            }
        });
        let expires_at = if lifetime == 0.0 {
            None
        } else {
            let deadline = now_ms + lifetime;
            if !deadline.is_finite() || deadline <= now_ms {
                return Err(Error::invalid("expiry cannot be represented"));
            }
            Some(deadline)
        };
        let mut previous = Vec::new();
        let mut kept = VecDeque::new();
        while let Some(entry) = self.entries.pop_front() {
            let e = &entry;
            let matches = (input.source == NotificationSource::Hermes
                && input.dedupe_key.is_some()
                && e.notification.source == NotificationSource::Hermes
                && e.dedupe_key == input.dedupe_key)
                || (input.server_id.is_some()
                    && e.server_id.is_some()
                    && (e.server_id == input.server_id
                        || (input.replaces_id != 0 && e.server_id == Some(input.replaces_id))));
            if matches {
                previous.push(entry);
            } else {
                kept.push_back(entry);
            }
        }
        self.entries = kept;
        let id = if let Some(server_id) = input.server_id {
            format!("d-{server_id}")
        } else if let Some(entry) = previous.first() {
            entry.notification.id.clone()
        } else {
            self.fresh_id(input.source)
        };
        let mut changes = Changes {
            id: Some(id.clone()),
            toast: !self.policy.dnd && input.urgency != Urgency::Low,
            ..Changes::default()
        };
        for entry in previous {
            if entry.notification.id != id {
                changes
                    .events
                    .push(self.closed(entry.notification.id, CloseReason::Replaced));
            }
        }
        if self.entries.len() == STORE_CAPACITY {
            if let Some(oldest) = self.entries.pop_back() {
                changes
                    .events
                    .push(self.closed(oldest.notification.id, CloseReason::Evicted));
            }
        }
        let notification = IslandNotification {
            rev: self.revision(),
            id,
            source: input.source,
            app_name: input.app_name,
            title: input.title,
            body: if self.policy.redact_bodies && input.body.is_some() {
                Some("Hidden".into())
            } else {
                input.body
            },
            icon_png: input.icon_png,
            urgency: input.urgency,
            received_at: now_ms,
            actions: input.actions,
            can_invoke_actions: false,
            progress: input.progress,
        };
        changes
            .events
            .push(Delta::NotificationReceived(NotificationReceived(
                notification.clone(),
            )));
        self.entries.push_front(Entry {
            notification,
            dedupe_key: input.dedupe_key,
            server_id: input.server_id,
            expires_at,
            transient: input.transient,
        });
        Ok(changes)
    }
    pub fn expire(&mut self, now_ms: f64) -> Result<Changes, Error> {
        super::types::time(now_ms)?;
        Ok(self.remove_matching(
            |e| e.expires_at.is_some_and(|deadline| now_ms >= deadline),
            CloseReason::Expired,
            false,
        ))
    }
    fn remove_matching(
        &mut self,
        predicate: impl Fn(&Entry) -> bool,
        reason: CloseReason,
        request_close: bool,
    ) -> Changes {
        let mut changes = Changes::default();
        let mut kept = VecDeque::new();
        while let Some(entry) = self.entries.pop_front() {
            if predicate(&entry) {
                if request_close {
                    if let Some(id) = entry.server_id {
                        changes.close_server_ids.push(id);
                    }
                }
                changes
                    .events
                    .push(self.closed(entry.notification.id.clone(), reason));
                changes.id = Some(entry.notification.id);
            } else {
                kept.push_back(entry);
            }
        }
        self.entries = kept;
        changes
    }
    pub fn dismiss(&mut self, id: &str) -> Changes {
        self.remove_matching(|e| e.notification.id == id, CloseReason::Dismissed, true)
    }
    pub fn clear(&mut self) -> Changes {
        self.remove_matching(|_| true, CloseReason::Cleared, true)
    }
    pub fn clear_key(&mut self, key: &str) -> Changes {
        self.remove_matching(
            |e| {
                e.notification.source == NotificationSource::Hermes
                    && e.dedupe_key.as_deref() == Some(key)
            },
            CloseReason::Cleared,
            false,
        )
    }
    pub fn server_closed(&mut self, server_id: u32) -> Changes {
        self.remove_matching(
            |e| e.server_id == Some(server_id),
            CloseReason::ServerClosed,
            false,
        )
    }
    /// Pair with NotifyCorrelator::reset when the notification server owner changes.
    pub fn reset_server(&mut self) -> Changes {
        self.remove_matching(|e| e.server_id.is_some(), CloseReason::ServerClosed, false)
    }
    /// Apply privacy retroactively; original bodies are deliberately not retained.
    pub fn set_policy(&mut self, policy: StorePolicy) -> Result<Changes, Error> {
        policy.validate()?;
        let mut changes = self.remove_matching(
            |e| policy.denied_apps.contains(&e.notification.app_name),
            CloseReason::Filtered,
            false,
        );
        self.policy = policy;
        if self.policy.redact_bodies {
            for index in 0..self.entries.len() {
                if self.entries[index]
                    .notification
                    .body
                    .as_deref()
                    .is_some_and(|b| b != "Hidden")
                {
                    let rev = self.revision();
                    let n = &mut self.entries[index].notification;
                    n.body = Some("Hidden".into());
                    n.rev = rev;
                    changes
                        .events
                        .push(Delta::NotificationReceived(NotificationReceived(n.clone())));
                }
            }
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_ids_skip_live_collisions_and_revisions_wrap() {
        let mut store = NotificationStore {
            next_id: u32::MAX,
            rev: u32::MAX,
            ..NotificationStore::default()
        };
        let first = store
            .insert(NotificationInput::hermes("first"), 0.0)
            .unwrap();
        assert_eq!(first.id.as_deref(), Some("h-0"));
        assert_eq!(store.list()[0].rev, 0);
        store.next_id = u32::MAX;
        let second = store
            .insert(NotificationInput::hermes("second"), 0.0)
            .unwrap();
        assert_eq!(second.id.as_deref(), Some("h-1"));
        assert_eq!(store.list()[0].rev, 1);
    }
}
