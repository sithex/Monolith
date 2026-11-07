//! Presentation lifetime is independent of notification history and the server's timeout.
use crate::contract::{
    notify::IslandNotification,
    settings::{RuleUrgency, Settings},
};
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Popup {
    pub notification: IslandNotification,
    pub remaining_ms: Option<f64>,
    pub ttl_ms: f64,
    pub paused: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPopups {
    pub rev: u32,
    pub items: Vec<Popup>,
}
#[derive(Default)]
pub struct Popups {
    rows: VecDeque<Popup>,
    last: u64,
    rev: u32,
    dirty: bool,
}
pub fn app_rule<'a>(
    settings: &'a Settings,
    app: &str,
) -> Option<&'a crate::contract::settings::NotificationAppRule> {
    settings
        .notification_app_rules
        .iter()
        .find(|r| r.app == app)
}
pub fn allowed(settings: &Settings, app: &str) -> bool {
    settings.notifications_enabled
        && !settings.dnd
        && !settings.game_mode
        && !settings.notification_denied_apps.iter().any(|a| a == app)
        && app_rule(settings, app).is_none_or(|r| r.popup)
}
pub fn urgency(settings: &Settings, app: &str, original: super::Urgency) -> super::Urgency {
    use super::Urgency;
    match app_rule(settings, app).map(|r| r.urgency) {
        Some(RuleUrgency::Low) => Urgency::Low,
        Some(RuleUrgency::Normal) => Urgency::Normal,
        Some(RuleUrgency::Critical) => Urgency::Critical,
        _ => original,
    }
}
impl Popups {
    pub fn insert(&mut self, notification: IslandNotification, settings: &Settings, now: u64) {
        self.tick(now);
        self.remove(&notification.id);
        if !allowed(settings, &notification.app_name) {
            return;
        }
        let critical = notification.urgency == crate::contract::notify::Urgency::Critical;
        self.rows.push_front(Popup {
            notification,
            remaining_ms: (!critical).then_some(settings.toast_ttl_ms),
            ttl_ms: settings.toast_ttl_ms,
            paused: false,
        });
        self.rows.truncate(32);
        self.changed();
    }
    fn changed(&mut self) {
        self.rev = self.rev.wrapping_add(1);
        self.dirty = true;
    }
    pub fn update(&mut self, notification: IslandNotification) {
        if let Some(p) = self
            .rows
            .iter_mut()
            .find(|p| p.notification.id == notification.id)
        {
            p.notification = notification;
            self.changed();
        }
    }
    pub fn remove(&mut self, id: &str) {
        let n = self.rows.len();
        self.rows.retain(|p| p.notification.id != id);
        if n != self.rows.len() {
            self.changed();
        }
    }
    pub fn pause(&mut self, id: &str, paused: bool, now: u64) {
        self.tick(now);
        if let Some(p) = self.rows.iter_mut().find(|p| p.notification.id == id) {
            if p.paused != paused {
                p.paused = paused;
                self.changed();
            }
        }
    }
    pub fn configure(&mut self, settings: &Settings) {
        let before = self.rows.clone();
        self.rows
            .retain(|p| allowed(settings, &p.notification.app_name));
        for p in &mut self.rows {
            if settings.redact_bodies {
                p.notification.body = None;
            }
        }
        if before != self.rows {
            self.changed();
        }
    }
    pub fn clear(&mut self) {
        if !self.rows.is_empty() {
            self.rows.clear();
            self.changed();
        }
    }
    pub fn tick(&mut self, now: u64) {
        let elapsed = now.saturating_sub(self.last) as f64;
        self.last = now;
        let n = self.rows.len();
        // Hidden cards do not consume their lifetime before they can be seen.
        for p in self.rows.iter_mut().take(3) {
            if !p.paused {
                if let Some(ms) = &mut p.remaining_ms {
                    *ms = (*ms - elapsed).max(0.);
                }
            }
        }
        self.rows.retain(|p| p.remaining_ms != Some(0.));
        if n != self.rows.len() {
            self.changed();
        }
    }
    pub fn deadline(&self, now: u64) -> Option<u64> {
        self.rows
            .iter()
            .take(3)
            .filter(|p| !p.paused)
            .filter_map(|p| p.remaining_ms)
            .map(|ms| now.saturating_add(ms.ceil() as u64))
            .min()
    }
    pub fn snapshot(&self) -> NotificationPopups {
        NotificationPopups {
            rev: self.rev,
            items: self.rows.iter().cloned().collect(),
        }
    }
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn note(id: &str, critical: bool) -> IslandNotification {
        IslandNotification {
            rev: 1,
            id: id.into(),
            source: crate::contract::notify::NotificationSource::System,
            app_name: "Mail".into(),
            title: "Message".into(),
            body: Some("secret".into()),
            icon_png: None,
            urgency: if critical {
                crate::contract::notify::Urgency::Critical
            } else {
                crate::contract::notify::Urgency::Normal
            },
            received_at: 0.,
            actions: vec![],
            can_invoke_actions: false,
            progress: None,
        }
    }
    #[test]
    fn exact_app_rules_override_urgency_and_popup_only_not_history() {
        use crate::contract::settings::NotificationAppRule;
        let mut s = Settings::default();
        s.notification_app_rules.push(NotificationAppRule {
            app: "Mail".into(),
            popup: false,
            sound: false,
            urgency: RuleUrgency::Critical,
            sound_id: String::new(),
        });
        assert!(!allowed(&s, "Mail"));
        assert!(allowed(&s, "mail"));
        assert_eq!(
            urgency(&s, "Mail", super::super::Urgency::Low),
            super::super::Urgency::Critical
        );
        s.notification_app_rules[0].popup = true;
        let mut q = Popups::default();
        let mut n = note("1", false);
        n.urgency = crate::contract::notify::Urgency::Critical;
        q.insert(n, &s, 0);
        q.tick(100000);
        assert_eq!(q.snapshot().items.len(), 1);
        s.notification_app_rules[0].popup = false;
        q.configure(&s);
        assert!(q.snapshot().items.is_empty());
    }
    #[test]
    fn auto_dismiss_pause_and_critical_are_independent_of_history() {
        let s = Settings::default();
        let mut q = Popups::default();
        q.insert(note("normal", false), &s, 0);
        q.insert(note("critical", true), &s, 0);
        q.pause("normal", true, 1000);
        q.tick(30000);
        assert_eq!(q.snapshot().items.len(), 2);
        assert_eq!(
            q.snapshot().items[1].remaining_ms,
            Some(s.toast_ttl_ms - 1000.)
        );
        q.pause("normal", false, 30000);
        q.tick(30000 + s.toast_ttl_ms as u64);
        assert_eq!(q.snapshot().items.len(), 1);
        assert_eq!(q.snapshot().items[0].notification.id, "critical");
        q.remove("critical");
        assert!(q.snapshot().items.is_empty());
    }
    #[test]
    fn backlog_waits_for_a_visible_slot_and_replacement_resets_timer() {
        let s = Settings::default();
        let mut q = Popups::default();
        for id in ["1", "2", "3", "4"] {
            q.insert(note(id, false), &s, 0);
        }
        q.tick(s.toast_ttl_ms as u64);
        assert_eq!(q.snapshot().items.len(), 1);
        assert_eq!(q.snapshot().items[0].remaining_ms, Some(s.toast_ttl_ms));
        q.insert(note("1", false), &s, s.toast_ttl_ms as u64 + 1000);
        assert_eq!(q.snapshot().items.len(), 1);
        assert_eq!(q.snapshot().items[0].remaining_ms, Some(s.toast_ttl_ms));
    }
    #[test]
    fn policy_hides_existing_cards_and_redacts_without_retoasting() {
        let mut s = Settings::default();
        let mut q = Popups::default();
        q.insert(note("1", false), &s, 0);
        s.redact_bodies = true;
        q.configure(&s);
        assert_eq!(q.snapshot().items[0].notification.body, None);
        s.game_mode = true;
        q.configure(&s);
        assert!(q.snapshot().items.is_empty());
        q.insert(note("2", true), &s, 1);
        assert!(q.snapshot().items.is_empty());
        s.game_mode = false;
        s.dnd = true;
        assert!(!allowed(&s, "Mail"));
    }
}
