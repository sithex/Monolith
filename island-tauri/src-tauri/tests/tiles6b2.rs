#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]
#[path = "helpers/control_fakes.rs"]
mod control_fakes;
#[path = "helpers/extra_controls_fixture.rs"]
mod extra_controls_fixture;
#[path = "helpers/private_bus.rs"]
mod private_bus;
use control_fakes::Fakes;
use extra_controls_fixture::ExtraMock;
use hermes_island_lib::{
    contract::tiles::{TilesChanged, TilesOp},
    error::IslandError,
    integrations::{
        control_dbus::{PropertyFeed, KWIN, NM, SCREENSAVER},
        tiles::{ColourClipboard, TilesService},
    },
};
use private_bus::PrivateBus;
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[derive(Default)]
struct Clipboard(Mutex<Vec<String>>);
impl ColourClipboard for Clipboard {
    fn copy_colour(&self, text: &str) -> Result<(), IslandError> {
        self.0.lock().unwrap().push(text.into());
        Ok(())
    }
}
struct Service(Arc<TilesService>);
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
async fn wait(service: &TilesService, probe: impl Fn(&TilesChanged) -> bool) -> TilesChanged {
    let until = Instant::now() + Duration::from_secs(7);
    loop {
        let v = service.current();
        if probe(&v) {
            return v;
        }
        assert!(Instant::now() < until, "tiles condition {v:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tiles_private_keepawake_output_network_colour_screenshot_and_voice() {
    let bus = PrivateBus::start("tiles-main");
    let mut mock = ExtraMock::start(&bus);
    let f = Fakes::new("tiles-main");
    let clip = Arc::new(Clipboard::default());
    let s = Service(TilesService::start(
        f.runner(),
        Some(bus.address()),
        Some(bus.address()),
        clip.clone(),
    ));
    let first = wait(&s.0, |v| {
        v.output.available
            && v.network.available
            && v.voice.available
            && v.keep_awake.available
            && v.picker_available
    })
    .await;
    assert_eq!(first.output.current.as_deref(), Some("headset"));
    assert_eq!(first.network.state.as_deref(), Some("connectedGlobal"));
    assert!(!first.network.show_wifi);
    assert_eq!(first.tailscale.peers[0].online, None);
    assert_eq!(first.voice.active.as_deref(), Some("fixture-a"));
    s.0.request(TilesOp::KeepAwake {
        on: true,
        duration_ms: None,
    })
    .await
    .unwrap();
    wait(&s.0, |v| v.keep_awake.on == Some(true)).await;
    mock.send(json!({"op":"assert_cookies","count":1}));
    s.0.request(TilesOp::KeepAwake {
        on: false,
        duration_ms: None,
    })
    .await
    .unwrap();
    wait(&s.0, |v| v.keep_awake.on == Some(false)).await;
    mock.send(json!({"op":"assert_cookies","count":0}));
    s.0.request(TilesOp::Output {
        name: "hdmi".into(),
    })
    .await
    .unwrap();
    wait(&s.0, |v| v.output.current.as_deref() == Some("hdmi")).await;
    mock.send(json!({"op":"set","name":NM,"property":"State","signature":"u","value":20}));
    wait(&s.0, |v| v.network.state.as_deref() == Some("disconnected")).await;
    s.0.request(TilesOp::PickColour).await.unwrap();
    wait(&s.0, |v| {
        v.last_colour.as_deref() == Some("#12ab34") && !v.picker_busy
    })
    .await;
    assert_eq!(*clip.0.lock().unwrap(), vec!["#12ab34"]);
    s.0.request(TilesOp::Screenshot).await.unwrap();
    wait(&s.0, |v| !v.screenshot_busy).await;
    assert!(f
        .calls()
        .iter()
        .any(|c| c[0] == "spectacle" && c[1] == json!(["-r"])));
    s.0.request(TilesOp::Voice {
        name: "fixture-b".into(),
    })
    .await
    .unwrap();
    wait(&s.0, |v| v.voice.active.as_deref() == Some("fixture-b")).await;
    assert!(f
        .calls()
        .iter()
        .any(|c| c[0] == "hermes-fish" && c[1] == json!(["fixture-b"])));
    assert!(s
        .0
        .request(TilesOp::Voice {
            name: "current".into()
        })
        .await
        .is_err());
    assert!(s
        .0
        .request(TilesOp::Voice {
            name: "-bad".into()
        })
        .await
        .is_err());
    mock.send(json!({"op":"release","name":NM}));
    wait(&s.0, |v| !v.network.available && v.network.state.is_none()).await;
    mock.send(json!({"op":"acquire","name":NM}));
    wait(&s.0, |v| v.network.available).await;
    s.0.request(TilesOp::KeepAwake {
        on: true,
        duration_ms: None,
    })
    .await
    .unwrap();
    wait(&s.0, |v| v.keep_awake.on == Some(true)).await;
    s.0.shutdown().unwrap();
    mock.send(json!({"op":"assert_cookies","count":0}));
    let calls = mock.calls();
    assert_eq!(calls.iter().filter(|c| c[1] == "Inhibit").count(), 2);
    assert_eq!(calls.iter().filter(|c| c[1] == "UnInhibit").count(), 2);
    assert!(calls
        .iter()
        .any(|c| c[1] == "Inhibit" && c[2][0] == "Hermes Island"));
    assert!(calls
        .iter()
        .any(|c| c[0] == "/ColorPicker" && c[1] == "pick"));
    assert!(!calls
        .iter()
        .any(|c| c[0] == "/org/freedesktop/NetworkManager"
            && c[1].as_str().unwrap().starts_with("Set")));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tiles_keepawake_deadline_releases_without_another_command() {
    let bus = PrivateBus::start("tiles-timed");
    let mut mock = ExtraMock::start(&bus);
    let f = Fakes::new("tiles-timed");
    let s = Service(TilesService::start(
        f.runner(),
        Some(bus.address()),
        Some(bus.address()),
        Arc::new(Clipboard::default()),
    ));
    wait(&s.0, |v| v.keep_awake.available).await;
    s.0.request(TilesOp::KeepAwake {
        on: true,
        duration_ms: Some(200),
    })
    .await
    .unwrap();
    wait(&s.0, |v| v.keep_awake.on == Some(true)).await;
    wait(&s.0, |v| v.keep_awake.on == Some(false)).await;
    mock.send(json!({"op":"assert_cookies","count":0}));
}
async fn rogue_owner(name: &'static str, tag: &str) {
    let bus = PrivateBus::start(tag);
    let mut mock = ExtraMock::start(&bus);
    let feed = Feed(PropertyFeed::start(Some(bus.address()), name));
    let until = Instant::now() + Duration::from_secs(4);
    while feed.0.current().owner.is_none() {
        assert!(Instant::now() < until);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let before = feed.0.current();
    mock.send(json!({"op":"rogue_noc","name":name}));
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(feed.0.current(), before);
    mock.send(json!({"op":"release","name":name}));
    let until = Instant::now() + Duration::from_secs(4);
    while feed.0.current().owner.is_some() {
        assert!(Instant::now() < until);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    mock.send(json!({"op":"acquire","name":name}));
    let until = Instant::now() + Duration::from_secs(4);
    while feed.0.current().owner.is_none() {
        assert!(Instant::now() < until);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn screensaver_rogue_name_owner_changed_is_ignored() {
    rogue_owner(SCREENSAVER, "ss-rogue").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn picker_rogue_name_owner_changed_is_ignored() {
    rogue_owner(KWIN, "picker-rogue").await;
}
