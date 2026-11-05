#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]
use crate::control_fakes;
use control_fakes::Fakes;
use hermes_island_lib::{
    contract::controls::{MixerChanged, MixerOp},
    integrations::{
        control_process::{argv, Runner},
        mixer::MixerService,
    },
};
use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};
struct Service(Arc<MixerService>);
impl Drop for Service {
    fn drop(&mut self) {
        self.0.shutdown().unwrap();
    }
}
async fn wait(
    service: &MixerService,
    what: &str,
    probe: impl Fn(&MixerChanged) -> bool,
) -> MixerChanged {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let current = service.current();
        if probe(&current) {
            return current;
        }
        assert!(Instant::now() < until, "waiting for {what}: {current:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mixer_private_subscribe_commands_loss_and_recovery() {
    let f = Fakes::new("mixer-main");
    let s = Service(MixerService::start(f.runner()));
    let first = wait(&s.0, "snapshot", |v| v.available).await;
    assert_eq!(first.default_sink.as_deref(), Some("headset"));
    assert_eq!(first.apps.len(), 1);
    assert_eq!(first.apps[0].streams, vec![11, 12]);
    assert_eq!(first.sinks[1].volume_percent, None);
    f.subscriptions(1).await;
    let mut state = f.state();
    state["sinks"][0]["mute"] = true.into();
    f.set(state);
    f.event("Event 'change' on sink #1\n");
    let event = wait(&s.0, "subscribe event", |v| v.available && v.sinks[0].muted).await;
    assert_ne!(first.rev, event.rev);
    let v =
        s.0.request(MixerOp::SinkVolume {
            name: "headset".into(),
            percent: 75,
        })
        .await
        .unwrap();
    assert_eq!(v.sinks[0].volume_percent, Some(75));
    let v =
        s.0.request(MixerOp::SinkMute {
            name: "headset".into(),
            muted: false,
        })
        .await
        .unwrap();
    assert!(!v.sinks[0].muted);
    let v =
        s.0.request(MixerOp::StreamMute {
            index: 11,
            muted: true,
        })
        .await
        .unwrap();
    assert!(v.streams[0].muted);
    let v =
        s.0.request(MixerOp::StreamVolume {
            index: 11,
            percent: 60,
        })
        .await
        .unwrap();
    assert_eq!(v.streams[0].volume_percent, Some(60));
    let v =
        s.0.request(MixerOp::AppVolume {
            stream: 11,
            percent: 40,
        })
        .await
        .unwrap();
    assert!(v.streams.iter().all(|s| s.volume_percent == Some(40)));
    let v =
        s.0.request(MixerOp::MoveStream {
            index: 11,
            sink: "hdmi".into(),
        })
        .await
        .unwrap();
    assert_eq!(v.streams[0].sink_index, 2);
    let v =
        s.0.request(MixerOp::DefaultSink {
            name: "hdmi".into(),
        })
        .await
        .unwrap();
    assert_eq!(v.default_sink.as_deref(), Some("hdmi"));
    let calls = f.calls();
    assert!(calls
        .iter()
        .any(|c| c[1] == serde_json::json!(["set-sink-volume", "headset", "75%"])));
    assert!(calls
        .iter()
        .all(|c| c[0] == "pactl" && c[2] == "C" && c[3] == f.root.join("home").to_str().unwrap()));
    let before = f.calls().len();
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(f.calls().len(), before, "no idle polling");
    fs::write(f.root.join("offline"), "").unwrap();
    wait(&s.0, "loss", |v| !v.available).await;
    fs::remove_file(f.root.join("offline")).unwrap();
    wait(&s.0, "recovery", |v| v.available).await;
    s.0.shutdown().unwrap();
    assert!(!s.0.current().available);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mixer_rejects_unknown_targets_and_invalid_percent_without_a_write() {
    let f = Fakes::new("mixer-invalid");
    let s = Service(MixerService::start(f.runner()));
    wait(&s.0, "initial", |v| v.available).await;
    let count = f.calls().len();
    for op in [
        MixerOp::SinkVolume {
            name: "headset".into(),
            percent: 151,
        },
        MixerOp::DefaultSink {
            name: "-bad".into(),
        },
        MixerOp::StreamVolume {
            index: 999,
            percent: 30,
        },
        MixerOp::AppVolume {
            stream: 999,
            percent: 30,
        },
        MixerOp::MoveStream {
            index: 11,
            sink: "gone".into(),
        },
    ] {
        assert!(s.0.request(op).await.is_err());
    }
    assert_eq!(f.calls().len(), count);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mixer_missing_mute_is_unavailable_not_fabricated_unmuted() {
    let f = Fakes::new("mixer-unknown");
    let mut state = f.state();
    state["sinks"][0].as_object_mut().unwrap().remove("mute");
    f.set(state);
    let s = Service(MixerService::start(f.runner()));
    wait(&s.0, "invalid data", |v| v.detail == "pactl-unavailable").await;
    assert!(!s.0.current().available);
    assert!(s.0.current().sinks.is_empty());
}
#[tokio::test]
async fn controls_test_runner_without_a_fixture_never_uses_real_path() {
    assert!(Runner::for_instance(true, None)
        .text(&argv(&["hermes-fish", "current"]))
        .await
        .is_err());
    assert!(Runner::for_instance(true, Some("/usr".into()))
        .text(&argv(&["pactl", "get-default-sink"]))
        .await
        .is_err());
}
#[tokio::test]
async fn controls_runner_preserves_exit_stderr_and_bounds_timeout_and_output() {
    let f = Fakes::new("runner");
    let r = f.runner();
    let mut state = f.state();
    state["checkupdates"] = serde_json::json!({"code":2,"stdout":"none","stderr":"fixture-error"});
    f.set(state);
    let output = r
        .run(&argv(&["checkupdates"]), Duration::from_secs(2), 1024)
        .await
        .unwrap();
    assert_eq!(output.code, 2);
    assert_eq!(output.stderr, "fixture-error");
    assert_eq!(output.stdout, "none");
    fs::write(f.root.join("hang"), "").unwrap();
    assert!(r
        .run(&argv(&["checkupdates"]), Duration::from_millis(100), 1024)
        .await
        .is_err());
    fs::remove_file(f.root.join("hang")).unwrap();
    fs::write(f.root.join("oversize"), "").unwrap();
    assert!(r
        .run(&argv(&["checkupdates"]), Duration::from_secs(2), 1024)
        .await
        .is_err());
}
