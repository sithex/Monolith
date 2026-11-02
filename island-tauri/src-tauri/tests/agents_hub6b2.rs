#![allow(clippy::unwrap_used)]
use hermes_island_lib::{
    contract::settings::Settings,
    error::IslandError,
    integrations::{
        agents::{core::Event, AgentEvents, AgentHubSink},
        notifications::hub::Hub,
    },
    services::{autostart::Autostart, settings::SettingsService},
    state::{
        actor,
        machine::{Clock, Effect},
    },
};
use std::{fs, sync::Arc};
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
fn agents_started_finished_reach_the_private_hub_and_activity_and_clear_on_shutdown() {
    let root = std::env::temp_dir().join(format!("hi-6b2-agent-hub-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
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
    sink.deliver(&Event::Started {
        id: "fixture".into(),
    })
    .unwrap();
    tauri::async_runtime::block_on(async {
        assert_eq!(
            handle.snapshot().await.unwrap().presentation.compact,
            vec!["agent:fixture"]
        );
    });
    assert_eq!(hub.list().unwrap()[0].title, "Agent run started");
    sink.deliver(&Event::Stalled {
        id: "fixture".into(),
    })
    .unwrap();
    sink.deliver(&Event::Finished {
        id: "fixture".into(),
        reason: None,
        elapsed_secs: 90.,
    })
    .unwrap();
    tauri::async_runtime::block_on(async {
        assert_eq!(
            handle.snapshot().await.unwrap().presentation.compact,
            vec!["agent-finished:fixture"]
        );
    });
    assert_eq!(hub.list().unwrap().len(), 2);
    assert_eq!(hub.list().unwrap()[0].title, "Agent run finished");
    sink.clear().unwrap();
    tauri::async_runtime::block_on(async {
        assert!(handle
            .snapshot()
            .await
            .unwrap()
            .presentation
            .compact
            .is_empty());
        handle.shutdown();
        task.await.unwrap().unwrap();
    });
    fs::remove_dir_all(root).unwrap();
}
