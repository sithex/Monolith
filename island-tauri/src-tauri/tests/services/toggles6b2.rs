#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]
use crate::control_fakes;
use crate::extra_controls_fixture;
use crate::private_bus;
use control_fakes::Fakes;
use extra_controls_fixture::ExtraMock;
use hermes_island_lib::{
    contract::{
        controls::{TogglesChanged, TogglesOp},
        settings::Settings,
    },
    error::IslandError,
    integrations::{
        control_dbus::{PropertyFeed, NM, PPD},
        toggles::TogglesService,
    },
    state::{
        actor::{self, MonotonicClock},
        machine::Effect,
    },
};
use private_bus::PrivateBus;
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
struct Sink;
impl actor::Sink for Sink {
    fn apply(&mut self, _: &[Effect], _: &actor::Snapshot) -> Result<(), IslandError> {
        Ok(())
    }
}
struct Authority {
    handle: actor::Handle,
    task: tokio::task::JoinHandle<Result<(), IslandError>>,
}
impl Authority {
    fn new() -> Self {
        let (handle, task) = actor::spawn(
            Settings::default(),
            Arc::new(MonotonicClock::default()),
            Sink,
        );
        Self { handle, task }
    }
}
impl Drop for Authority {
    fn drop(&mut self) {
        self.handle.shutdown();
        self.task.abort();
    }
}
struct Service(Arc<TogglesService>);
impl Drop for Service {
    fn drop(&mut self) {
        self.0.shutdown().unwrap();
    }
}
struct Feed(Arc<PropertyFeed>);
impl Drop for Feed {
    fn drop(&mut self) {
        self.0.shutdown().unwrap();
    }
}
async fn until<T>(probe: impl Fn() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(value) = probe() {
            return value;
        }
        assert!(Instant::now() < deadline, "private toggle condition");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
async fn wait(service: &TogglesService, p: impl Fn(&TogglesChanged) -> bool) -> TogglesChanged {
    until(|| {
        let v = service.current();
        p(&v).then_some(v)
    })
    .await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn toggles_private_power_profile_mic_dnd_and_optimistic_timeout() {
    let bus = PrivateBus::start("toggles-main");
    let mut mock = ExtraMock::start(&bus);
    let f = Fakes::new("toggles-main");
    let actor = Authority::new();
    let s = Service(TogglesService::start(
        f.runner(),
        Some(bus.address()),
        actor.handle.clone(),
    ));
    let first = wait(&s.0, |v| v.power_available && v.mic_available).await;
    assert_eq!(first.power_profile.as_deref(), Some("balanced"));
    assert_eq!(
        first.power_choices,
        vec!["power-saver", "balanced", "performance"]
    );
    assert_eq!(first.mic_muted, Some(false));
    let value =
        s.0.request(TogglesOp::PowerProfile {
            profile: "performance".into(),
        })
        .await
        .unwrap();
    assert_eq!(value.power_profile.as_deref(), Some("performance"));
    assert!(value.power_request.is_none());
    assert!(mock
        .calls()
        .iter()
        .any(|c| c[1] == "Set:ActiveProfile" && c[2] == json!(["performance"])));
    let value =
        s.0.request(TogglesOp::MicMute { muted: true })
            .await
            .unwrap();
    assert_eq!(value.mic_muted, Some(true));
    assert!(f
        .calls()
        .iter()
        .any(|c| c[1] == json!(["set-source-mute", "@DEFAULT_SOURCE@", "1"])));
    let value = s.0.request(TogglesOp::Dnd { on: true }).await.unwrap();
    assert!(value.dnd);
    assert!(actor.handle.state().dnd);
    actor
        .handle
        .dispatch(hermes_island_lib::state::machine::Intent::SetDnd(false))
        .await
        .unwrap();
    wait(&s.0, |v| !v.dnd).await;
    mock.send(json!({"op":"echo","on":false}));
    let value =
        s.0.request(TogglesOp::PowerProfile {
            profile: "balanced".into(),
        })
        .await
        .unwrap();
    assert_eq!(value.power_profile.as_deref(), Some("balanced"));
    assert!(value.power_request.is_some());
    let settled = wait(&s.0, |v| v.power_request.is_none()).await;
    assert_eq!(settled.power_profile.as_deref(), Some("performance"));
    mock.send(json!({"op":"release","name":PPD}));
    let lost = wait(&s.0, |v| !v.power_available).await;
    assert!(lost.power_profile.is_none());
    mock.send(json!({"op":"acquire","name":PPD}));
    wait(&s.0, |v| v.power_available).await;
    std::fs::write(f.root.join("offline"), "").unwrap();
    let lost = wait(&s.0, |v| !v.mic_available).await;
    assert_eq!(lost.mic_muted, None);
    std::fs::remove_file(f.root.join("offline")).unwrap();
    wait(&s.0, |v| v.mic_available).await;
    assert!(s
        .0
        .request(TogglesOp::PowerProfile {
            profile: "-bad".into()
        })
        .await
        .is_err());
}
async fn rogue_properties(name: &'static str, tag: &str) {
    let bus = PrivateBus::start(tag);
    let mut mock = ExtraMock::start(&bus);
    let feed = Feed(PropertyFeed::start(Some(bus.address()), name));
    until(|| feed.0.current().values.is_some().then_some(())).await;
    let before = mock.calls().len();
    let rev = feed.0.current().rev;
    mock.send(json!({"op":"rogue_properties","name":name}));
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        mock.calls().len(),
        before,
        "rogue PropertiesChanged must not trigger a read"
    );
    assert_eq!(feed.0.current().rev, rev);
    let (property, signature, value) = if name == PPD {
        ("ActiveProfile", "s", json!("power-saver"))
    } else {
        ("State", "u", json!(20))
    };
    mock.send(
        json!({"op":"set","name":name,"property":property,"signature":signature,"value":value}),
    );
    until(|| (feed.0.current().rev != rev).then_some(())).await;
}
async fn rogue_owner(name: &'static str, tag: &str) {
    let bus = PrivateBus::start(tag);
    let mut mock = ExtraMock::start(&bus);
    let feed = Feed(PropertyFeed::start(Some(bus.address()), name));
    until(|| feed.0.current().values.is_some().then_some(())).await;
    let before = feed.0.current();
    mock.send(json!({"op":"rogue_noc","name":name}));
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(feed.0.current(), before);
    mock.send(json!({"op":"release","name":name}));
    until(|| feed.0.current().values.is_none().then_some(())).await;
    mock.send(json!({"op":"acquire","name":name}));
    until(|| feed.0.current().values.is_some().then_some(())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn toggles_rogue_properties_changed_is_ignored() {
    rogue_properties(PPD, "ppd-rogue-props").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn toggles_rogue_name_owner_changed_is_ignored() {
    rogue_owner(PPD, "ppd-rogue-owner").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn network_rogue_properties_changed_is_ignored() {
    rogue_properties(NM, "nm-rogue-props").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn network_rogue_name_owner_changed_is_ignored() {
    rogue_owner(NM, "nm-rogue-owner").await;
}
