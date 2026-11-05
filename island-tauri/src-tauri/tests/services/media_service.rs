//! Phase 6m: MPRIS against MOCK players on a private, scratch-only bus (never the user's
//! session bus, never a real player). Commands are sent to mocks only.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::private_bus;

use hermes_island_lib::{
    contract::system::{MediaCommand, NowPlaying},
    error::ErrorCode,
    integrations::media::MediaService,
};
use private_bus::{wait_until, PlayerMock, PrivateBus};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

const T: Duration = Duration::from_secs(10);

fn song(title: &str, track: &str) -> Value {
    json!({
        "xesam:title": ["s", title],
        "xesam:artist": ["as", ["Artist"]],
        "xesam:album": ["s", "Album"],
        "mpris:trackid": ["o", track],
        "mpris:length": ["x", 200_000_000i64],
    })
}

fn play(mock: &mut PlayerMock, title: &str, track: &str, status: &str) {
    mock.send(json!({"op":"set","props":{
        "Metadata": song(title, track), "PlaybackStatus": status, "Position": 5_000_000i64}}));
}

struct Rig {
    svc: Arc<MediaService>,
    bus: PrivateBus,
}

fn rig(tag: &str) -> Rig {
    Rig {
        svc: MediaService::new(),
        bus: PrivateBus::start(tag),
    }
}

impl Rig {
    fn start(&self) {
        self.svc.start(Some(self.bus.address()));
        wait_until("MPRIS session", T, || {
            self.svc.status().available.then_some(())
        });
    }
    fn now(&self) -> Option<NowPlaying> {
        self.svc.current().now_playing
    }
    fn wait_title(&self, title: &str) -> NowPlaying {
        wait_until(&format!("now playing {title}"), T, || {
            self.now().filter(|n| n.title == title)
        })
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.svc.shutdown();
    }
}

#[test]
fn media_initial_snapshot_property_change_event_seeked_and_quit_clears() {
    let r = rig("media-basic");
    let mut alpha = PlayerMock::start(&r.bus, "alpha");
    play(&mut alpha, "Song A", "/t/1", "Playing");
    r.start();
    let first = r.wait_title("Song A");
    assert_eq!(
        (
            first.player.as_str(),
            first.artist.as_str(),
            first.album.as_str()
        ),
        ("alpha", "Artist", "Album")
    );
    assert!(first.playing && first.status == "playing");
    assert_eq!(first.duration_ms, Some(200_000.0));
    assert_eq!(first.position_ms, Some(5000.0), "GetAll Position");
    assert!(first.can_control && first.can_seek && first.can_go_next);
    assert_eq!(first.bus_name, alpha.bus_name());
    assert_eq!(first.track_id.as_deref(), Some("/t/1"));
    let status = r.svc.status();
    assert_eq!((status.detail.as_str(), status.connects), ("mpris", 1));

    // PropertiesChanged reaches the latest-value topic (what the event forwards).
    let mut topic = r.svc.topic.subscribe();
    topic.borrow_and_update();
    let before = r.svc.current().rev;
    alpha.send(json!({"op":"set","props":{"PlaybackStatus":"Paused"}}));
    wait_until("paused", T, || {
        r.now().filter(|n| !n.playing && n.status == "paused")
    });
    assert!(r.svc.current().rev > before);
    assert!(topic.has_changed().unwrap());
    assert_eq!(
        topic
            .borrow()
            .as_ref()
            .unwrap()
            .now_playing
            .as_ref()
            .unwrap()
            .status,
        "paused"
    );

    // A new track: metadata changes, the position restarts at 0 (a known position).
    alpha.send(json!({"op":"set","props":{"Metadata": song("Song B", "/t/2")}}));
    let second = r.wait_title("Song B");
    assert_eq!(second.position_ms, Some(0.0));
    assert_eq!(second.track_id.as_deref(), Some("/t/2"));

    // Seeked updates the anchor position.
    alpha.send(json!({"op":"seeked","us":30_000_000i64}));
    wait_until("seeked position", T, || {
        r.now().filter(|n| n.position_ms == Some(30_000.0))
    });

    // The player quits: the state is CLEARED, not left stale; the service stays available.
    let before_quit = r.svc.current().rev;
    alpha.quit();
    wait_until("cleared after quit", T, || r.now().is_none().then_some(()));
    assert!(r.svc.current().rev > before_quit);
    assert!(r.svc.status().available, "the bus itself is fine");
}

#[test]
fn media_without_players_is_empty_and_available_and_without_a_bus_it_says_why() {
    let r = rig("media-empty");
    r.start();
    assert_eq!(r.now(), None);
    let error = tauri::async_runtime::block_on(r.svc.control(MediaCommand::Play)).unwrap_err();
    assert_eq!(error.code, ErrorCode::Unavailable);
    let none = MediaService::new();
    none.start(None);
    wait_until("explicit reason", T, || {
        (none.status().detail == "no-session-bus-configured").then_some(())
    });
    assert!(!none.status().available && none.current().now_playing.is_none());
    assert_eq!(none.status().connects, 0);
    none.shutdown();
}

#[test]
fn media_prefers_the_most_recently_playing_player() {
    let r = rig("media-prefer");
    let mut alpha = PlayerMock::start(&r.bus, "alpha");
    play(&mut alpha, "Alpha song", "/a/1", "Playing");
    r.start();
    r.wait_title("Alpha song");
    std::thread::sleep(Duration::from_millis(30));
    let mut beta = PlayerMock::start(&r.bus, "beta");
    play(&mut beta, "Beta song", "/b/1", "Playing");
    let now = r.wait_title("Beta song");
    assert_eq!(now.player, "beta", "the later Playing player wins");
    // Beta pauses: the only Playing player is alpha again.
    beta.send(json!({"op":"set","props":{"PlaybackStatus":"Paused"}}));
    assert_eq!(r.wait_title("Alpha song").player, "alpha");
    // Beta resumes, then quits: alpha is shown again, never a stale beta.
    beta.send(json!({"op":"set","props":{"PlaybackStatus":"Playing"}}));
    r.wait_title("Beta song");
    beta.quit();
    r.wait_title("Alpha song");
}

#[test]
fn media_control_reaches_only_the_preferred_mock_and_only_advertised_capabilities() {
    let r = rig("media-control");
    let mut alpha = PlayerMock::start(&r.bus, "alpha");
    let mut beta = PlayerMock::start(&r.bus, "beta");
    play(&mut alpha, "Alpha song", "/a/1", "Paused");
    r.start();
    r.wait_title("Alpha song");
    std::thread::sleep(Duration::from_millis(30));
    play(&mut beta, "Beta song", "/b/1", "Playing");
    r.wait_title("Beta song");
    let run = |command| tauri::async_runtime::block_on(r.svc.control(command));
    run(MediaCommand::Pause).unwrap();
    run(MediaCommand::PlayPause).unwrap();
    run(MediaCommand::Next).unwrap();
    run(MediaCommand::Previous).unwrap();
    run(MediaCommand::Play).unwrap();
    run(MediaCommand::Seek {
        position_ms: 12_345.0,
    })
    .unwrap();
    wait_until("beta received every call", T, || {
        (beta.calls().len() == 6).then_some(())
    });
    let calls = beta.calls();
    let members: Vec<&str> = calls.iter().map(|c| c.split(' ').next().unwrap()).collect();
    assert_eq!(
        members,
        [
            "Pause",
            "PlayPause",
            "Next",
            "Previous",
            "Play",
            "SetPosition"
        ]
    );
    assert!(
        calls[5].contains("/b/1") && calls[5].contains("12345000"),
        "{calls:?}"
    );
    assert!(
        alpha.calls().is_empty(),
        "the non-preferred player was never addressed"
    );

    // A capability the player does not advertise is refused locally; nothing reaches it.
    beta.send(json!({"op":"set","props":{"CanGoNext": false, "CanSeek": false}}));
    wait_until("capabilities updated", T, || {
        r.now().filter(|n| !n.can_go_next && !n.can_seek)
    });
    for command in [MediaCommand::Next, MediaCommand::Seek { position_ms: 1.0 }] {
        let error = run(command).unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
    }
    for bad in [-1.0, f64::NAN, 300_000.0] {
        assert!(
            run(MediaCommand::Seek { position_ms: bad }).is_err(),
            "{bad}"
        );
    }
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(beta.calls().len(), 6, "refused commands made no call");
    assert!(alpha.calls().is_empty());
}

#[test]
fn media_position_is_extrapolated_in_milliseconds_at_the_players_rate() {
    let r = rig("media-position");
    let mut alpha = PlayerMock::start(&r.bus, "alpha");
    play(&mut alpha, "Song A", "/t/1", "Playing");
    r.start();
    r.wait_title("Song A");
    let (p0, rate, playing) = r.svc.position_now().expect("position known from GetAll");
    assert!(playing && (rate - 1.0).abs() < 1e-9);
    std::thread::sleep(Duration::from_millis(500));
    let (p1, _, _) = r.svc.position_now().unwrap();
    let moved = p1 - p0;
    assert!(
        (450..=800).contains(&moved),
        "500 ms of playback moved {moved} ms"
    );
    // Paused: frozen. Rate 2.0: twice as fast.
    alpha.send(json!({"op":"set","props":{"PlaybackStatus":"Paused"}}));
    wait_until("paused", T, || r.now().filter(|n| !n.playing));
    let (frozen, _, _) = r.svc.position_now().unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(r.svc.position_now().unwrap().0, frozen);
    alpha.send(json!({"op":"set","props":{"PlaybackStatus":"Playing","Rate":2.0}}));
    wait_until("rate 2", T, || {
        r.now().filter(|n| n.playing && n.rate == 2.0)
    });
    let (q0, _, _) = r.svc.position_now().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let moved = r.svc.position_now().unwrap().0 - q0;
    assert!(
        (900..=1500).contains(&moved),
        "500 ms at rate 2 moved {moved} ms"
    );
}

#[test]
fn media_ignores_properties_changed_from_a_rogue_sender() {
    let r = rig("media-rogue-props");
    let mut alpha = PlayerMock::start(&r.bus, "alpha");
    play(&mut alpha, "Song A", "/t/1", "Playing");
    r.start();
    r.wait_title("Song A");
    let base = r.svc.current();
    alpha.send(
        json!({"op":"rogue_props","props":{"PlaybackStatus":"Paused",
        "Metadata": song("Forged", "/forged")}}),
    );
    alpha.send(json!({"op":"set","props":{"CanGoNext": false}}));
    wait_until("sentinel capability change", T, || {
        r.now().filter(|n| !n.can_go_next)
    });
    let after = r.svc.current();
    assert_eq!(
        after.rev,
        base.rev + 1,
        "forged signal produced no revision"
    );
    let now = after.now_playing.unwrap();
    assert_eq!((now.title.as_str(), now.playing), ("Song A", true));
}

#[test]
fn media_ignores_seeked_from_a_rogue_sender() {
    let r = rig("media-rogue-seeked");
    let mut alpha = PlayerMock::start(&r.bus, "alpha");
    play(&mut alpha, "Song A", "/t/1", "Paused");
    r.start();
    r.wait_title("Song A");
    let base = r.svc.current();
    alpha.send(json!({"op":"rogue_seeked","us":99_000_000i64}));
    alpha.send(json!({"op":"seeked","us":7_000_000i64}));
    wait_until("genuine seek", T, || {
        r.now().filter(|n| n.position_ms == Some(7000.0))
    });
    let after = r.svc.current();
    assert_eq!(
        after.rev,
        base.rev + 1,
        "forged Seeked produced no revision"
    );
}

#[test]
fn media_ignores_a_spoofed_name_owner_changed_from_a_rogue_peer() {
    let r = rig("media-rogue-noc");
    let mut alpha = PlayerMock::start(&r.bus, "alpha");
    play(&mut alpha, "Song A", "/t/1", "Playing");
    r.start();
    r.wait_title("Song A");
    let base = r.svc.current();
    for _ in 0..2 {
        alpha.send(json!({"op":"rogue_noc","name": alpha.bus_name(), "new": ""}));
    }
    alpha.send(json!({"op":"set","props":{"CanGoNext": false}}));
    wait_until("sentinel capability change", T, || {
        r.now().filter(|n| !n.can_go_next)
    });
    std::thread::sleep(Duration::from_millis(300));
    let after = r.svc.current();
    assert_eq!(
        after.rev,
        base.rev + 1,
        "the spoofed name loss was not believed"
    );
    assert_eq!(after.now_playing.unwrap().title, "Song A");
}

#[test]
fn media_bus_loss_clears_the_state_and_says_why() {
    let mut r = rig("media-busloss");
    let mut alpha = PlayerMock::start(&r.bus, "alpha");
    play(&mut alpha, "Song A", "/t/1", "Playing");
    r.start();
    r.wait_title("Song A");
    r.bus.stop_daemon();
    wait_until("bus loss reported", T, || {
        let s = r.svc.status();
        (!s.available && s.detail == "bus-unavailable").then_some(())
    });
    assert_eq!(r.now(), None, "no stale track after the bus is gone");
    let error = tauri::async_runtime::block_on(r.svc.control(MediaCommand::Pause)).unwrap_err();
    assert_eq!(error.code, ErrorCode::Unavailable);
}
