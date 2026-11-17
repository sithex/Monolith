#![allow(clippy::unwrap_used)]
use super::{
    core::{Event, SessionRow, Tracker},
    AgentEvents, AgentHubSink,
};
use crate::{
    contract::settings::Settings,
    error::IslandError,
    integrations::notifications::hub::Hub,
    services::{autostart::Autostart, settings::SettingsService},
    state::{
        actor,
        machine::{Clock, Effect},
    },
};
use std::sync::Arc;
fn row(id: &str) -> SessionRow {
    SessionRow {
        id: id.into(),
        source: "cli".into(),
        parent_session_id: None,
        model: Some("model-small".into()),
        billing_provider: None,
        started_at: 100.,
        ended_at: None,
        end_reason: None,
        last_activity_at: Some(110.),
        api_call_count: None,
        title: Some("Review audio routing".into()),
    }
}
#[test]
fn naming_title_model_fallback_and_unicode_boundary() {
    let mut tracker = Tracker::new();
    let mut r = row("run-1");
    assert_eq!(
        tracker.update(vec![r.clone()], 110.).notification_events[0]
            .body
            .as_deref(),
        Some("Review audio routing · run-1")
    );
    r.id = "run-2".into();
    r.title = Some(" \n ".into());
    assert_eq!(
        tracker.update(vec![r.clone()], 110.).notification_events[0]
            .body
            .as_deref(),
        Some("model-small · run-2")
    );
    r.id = "run-3".into();
    r.model = None;
    assert_eq!(
        tracker.update(vec![r.clone()], 110.).notification_events[0].body,
        None
    );
    r.id = "run-4".into();
    r.title = Some("界".repeat(61));
    assert_eq!(
        tracker.update(vec![r.clone()], 110.).notification_events[0].body,
        Some(format!("{}… · run-4", "界".repeat(59)))
    );
    r.id = "run-5".into();
    r.title = Some("é".repeat(60));
    assert_eq!(
        tracker.update(vec![r], 110.).notification_events[0].body,
        Some(format!("{} · run-5", "é".repeat(60)))
    );
}
#[test]
fn named_completions_use_selected_row_and_disambiguate_equal_names() {
    let mut tracker = Tracker::new();
    let a = row("same-prefix-one");
    let b = row("same-prefix-two");
    let start = tracker.update(vec![a.clone(), b.clone()], 110.);
    assert_ne!(
        start.notification_events[0].body,
        start.notification_events[1].body
    );
    let mut done = a.clone();
    done.ended_at = Some(120.);
    done.last_activity_at = Some(120.);
    done.title = Some("Updated title".into());
    let update = tracker.update(vec![a, done, b], 120.);
    assert!(matches!(
        update.notification_events[0].event,
        Event::Finished { .. }
    ));
    assert_eq!(
        update.notification_events[0].body.as_deref(),
        Some("Updated title · same-prefix-one")
    );
    assert_eq!(update.events.len(), update.notification_events.len());
}
struct Fake;
impl Clock for Fake {
    fn now_ms(&self) -> u64 {
        1000
    }
}
struct Sink;
impl actor::Sink for Sink {
    fn apply(&mut self, _: &[Effect], _: &actor::Snapshot) -> Result<(), IslandError> {
        Ok(())
    }
}
#[test]
fn named_event_reaches_notification_body_and_legacy_delivery_falls_back() {
    let root = std::env::temp_dir().join(format!("agent-names-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let clock = Arc::new(Fake);
    let (handle, task) = tauri::async_runtime::block_on(async {
        actor::spawn(Settings::default(), clock.clone(), Sink)
    });
    let settings = Arc::new(SettingsService::new(
        root.clone(),
        Autostart::with_default_launcher(root.clone(), root.join("app"), true),
    ));
    let hub = Hub::new(handle.clone(), settings);
    let sink = AgentHubSink::new(hub.clone(), handle.clone(), clock);
    let mut tracker = Tracker::new();
    let mut r = row("run-1");
    sink.deliver_named(&tracker.update(vec![r.clone()], 110.).notification_events[0])
        .unwrap();
    r.ended_at = Some(120.);
    sink.deliver_named(&tracker.update(vec![r], 120.).notification_events[0])
        .unwrap();
    let notifications = hub.list().unwrap();
    assert_eq!(notifications[0].title, "Agent run finished");
    assert_eq!(
        notifications[0].body.as_deref(),
        Some("Review audio routing · run-1")
    );
    assert_eq!(notifications[1].title, "Agent run started");
    assert_eq!(notifications[1].body, notifications[0].body);
    sink.deliver(&Event::Started {
        id: "unnamed".into(),
    })
    .unwrap();
    assert_eq!(hub.list().unwrap()[0].body, None);
    tauri::async_runtime::block_on(async {
        handle.shutdown();
        task.await.unwrap().unwrap();
    });
    std::fs::remove_dir_all(root).unwrap();
}
