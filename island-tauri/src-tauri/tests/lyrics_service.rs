//! Phase 6m: lyrics against a LOCAL LRCLIB fixture with an injected base URL, driven by MPRIS
//! mock players on a private bus. No test here contacts lrclib.net; the single real call is
//! `real_lrclib_smoke` (`#[ignore]`, never part of the gate).
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "helpers/lrclib_fixture.rs"]
mod lrclib_fixture;
#[path = "helpers/private_bus.rs"]
mod private_bus;

use hermes_island_lib::{
    contract::system::{LyricsChanged, LyricsStatus},
    integrations::media::{
        lrclib::{Lookup, TrackQuery},
        lyrics::{LyricsConfig, LyricsService},
        MediaService,
    },
};
use lrclib_fixture::Lrclib;
use private_bus::{wait_until, PlayerMock, PrivateBus};
use serde_json::json;
use std::{os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc, time::Duration};

const T: Duration = Duration::from_secs(10);

fn meta(title: &str) -> serde_json::Value {
    json!({
        "xesam:title": ["s", title],
        "xesam:artist": ["as", ["Artist", "Other"]],
        "xesam:album": ["s", "Album"],
        "mpris:trackid": ["o", "/t/1"],
        "mpris:length": ["x", 200_000_000i64],
    })
}

struct Rig {
    bus: PrivateBus,
    media: Arc<MediaService>,
    fixture: Lrclib,
    cache: PathBuf,
    lyrics: Option<Arc<LyricsService>>,
}

fn rig(tag: &str) -> Rig {
    let bus = PrivateBus::start(tag);
    let media = MediaService::new();
    media.start(Some(bus.address()));
    let cache = bus.root.join("home/lyrics-cache");
    Rig {
        bus,
        media,
        fixture: Lrclib::start(),
        cache,
        lyrics: None,
    }
}

impl Rig {
    fn lyrics(&mut self, enabled: bool) -> Arc<LyricsService> {
        let service = LyricsService::new(
            LyricsConfig {
                base: self.fixture.base(),
                cache_dir: Some(self.cache.clone()),
            },
            enabled,
        )
        .unwrap();
        service.start(Arc::clone(&self.media));
        if let Some(old) = self.lyrics.replace(Arc::clone(&service)) {
            old.shutdown();
        }
        service
    }

    fn wait(&self, what: &str, probe: impl Fn(&LyricsChanged) -> bool) -> LyricsChanged {
        let service = self.lyrics.as_ref().unwrap();
        wait_until(what, T, || {
            let state = service.current();
            probe(&state).then_some(state)
        })
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(lyrics) = &self.lyrics {
            lyrics.shutdown();
        }
        self.media.shutdown();
    }
}

fn start_track(mock: &mut PlayerMock, title: &str, status: &str) {
    mock.send(json!({"op":"set","props":{
        "Metadata": meta(title), "PlaybackStatus": status, "Position": 0}}));
}

#[test]
fn lyrics_are_off_by_default_and_make_no_request_or_cache() {
    let mut r = rig("lyrics-off");
    let mut player = PlayerMock::start(&r.bus, "alpha");
    start_track(&mut player, "Synced Song", "Playing");
    wait_until("track", T, || r.media.current().now_playing.map(|_| ()));
    let service = r.lyrics(false);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(service.current().status, LyricsStatus::Off);
    assert_eq!(service.status().detail, "disabled");
    assert_eq!(r.fixture.count(), 0, "opt-in: no request while disabled");
    assert!(!r.cache.exists(), "no cache while disabled");
    // Switching it on fetches; switching it off again stops using the network.
    service.configure(true);
    r.wait("synced", |s| s.status == LyricsStatus::Synced);
    assert_eq!(r.fixture.count(), 1);
    service.configure(false);
    r.wait("off again", |s| s.status == LyricsStatus::Off);
    let state = service.current();
    assert!(state.lines.is_empty() && state.plain.is_none(), "{state:?}");
    start_track(&mut player, "Plain Song", "Playing");
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        r.fixture.count(),
        1,
        "a new track made no request while off"
    );
}

#[test]
fn synced_lyrics_follow_the_mpris_position_seek_and_pause_and_are_cached_privately() {
    let mut r = rig("lyrics-synced");
    let mut player = PlayerMock::start(&r.bus, "alpha");
    start_track(&mut player, "Synced Song", "Paused");
    wait_until("track", T, || r.media.current().now_playing.map(|_| ()));
    r.lyrics(true);
    let state = r.wait("synced lines", |s| s.status == LyricsStatus::Synced);
    let texts: Vec<&str> = state.lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["one", "two", "three"]);
    assert_eq!(state.lines[0].time_ms, 500.0);
    assert_eq!(state.plain.as_deref(), Some("one\ntwo\nthree"));
    assert_eq!(
        state.active_index, None,
        "paused at 0 ms: before the first line"
    );
    // The fixture saw the first artist, the album and the rounded duration (200 s).
    let request = r.fixture.requests()[0].clone();
    assert!(
        request.starts_with(
            "/api/get?track_name=Synced+Song&artist_name=Artist&album_name=Album&duration=200"
        ),
        "{request}"
    );

    // Seeking while paused moves the active line without any timer.
    player.send(json!({"op":"seeked","us":1_200_000i64}));
    r.wait("line two while paused", |s| s.active_index == Some(1));
    // Playing from 1.2 s: line three is due at 2.0 s (~0.8 s later).
    player.send(json!({"op":"set","props":{"PlaybackStatus":"Playing"}}));
    let started = std::time::Instant::now();
    r.wait("line three after playing", |s| s.active_index == Some(2));
    let waited = started.elapsed();
    assert!(
        (Duration::from_millis(500)..Duration::from_millis(1500)).contains(&waited),
        "line three came due after {waited:?}"
    );
    // A backwards seek goes back; pausing freezes whatever line is active.
    player.send(json!({"op":"seeked","us":600_000i64}));
    r.wait("line one after seeking back", |s| s.active_index == Some(0));
    player.send(json!({"op":"set","props":{"PlaybackStatus":"Paused"}}));
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(r.lyrics.as_ref().unwrap().current().active_index, Some(0));

    // Cached privately; a fresh service for the same track makes no second request.
    assert_eq!(r.fixture.count(), 1);
    let files: Vec<_> = std::fs::read_dir(&r.cache)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(
        std::fs::metadata(&files[0]).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(&r.cache).unwrap().permissions().mode() & 0o777,
        0o700
    );
    r.lyrics(true);
    let cached = r.wait("synced from cache", |s| s.status == LyricsStatus::Synced);
    assert_eq!(cached.lines.len(), 3);
    assert_eq!(r.fixture.count(), 1, "served from the cache");
}

#[test]
fn plain_instrumental_not_found_array_wrong_type_and_rate_limit_are_each_classified() {
    let mut r = rig("lyrics-kinds");
    let mut player = PlayerMock::start(&r.bus, "alpha");
    start_track(&mut player, "Plain Song", "Playing");
    wait_until("track", T, || r.media.current().now_playing.map(|_| ()));
    let service = r.lyrics(true);
    let plain = r.wait("plain", |s| s.status == LyricsStatus::Plain);
    assert_eq!(plain.plain.as_deref(), Some("just words\nno timing"));
    assert!(plain.lines.is_empty() && plain.active_index.is_none());

    start_track(&mut player, "Instrumental Song", "Playing");
    let instrumental = r.wait("instrumental", |s| s.status == LyricsStatus::Instrumental);
    assert!(instrumental.lines.is_empty() && instrumental.plain.is_none());

    start_track(&mut player, "Missing Song", "Playing");
    r.wait("not found", |s| s.status == LyricsStatus::NotFound);
    assert_eq!(service.status().detail, "not-found");
    assert!(
        service.status().available,
        "a clean 404 means the service works"
    );

    // A 200 whose body is a JSON ARRAY is rejected, never read as a positional record.
    start_track(&mut player, "Array Song", "Playing");
    let array = r.wait("array body rejected", |s| s.status == LyricsStatus::Error);
    assert!(array.lines.is_empty() && array.plain.is_none(), "{array:?}");
    assert_eq!(service.status().detail, "invalid-response");

    start_track(&mut player, "Wrong Type Song", "Playing");
    wait_until("html content type", T, || {
        (service.status().detail == "invalid-response"
            && service.current().track_key.is_some()
            && r.fixture
                .requests()
                .iter()
                .any(|q| q.contains("Wrong+Type+Song")))
        .then_some(())
    });
    assert_eq!(service.current().status, LyricsStatus::Error);

    start_track(&mut player, "Limited Song", "Playing");
    wait_until("rate limited", T, || {
        (service.status().detail == "rate-limited").then_some(())
    });
    assert_eq!(service.current().status, LyricsStatus::Error);
    assert!(!service.status().available);

    // Only real answers are cached: the not-found / array / limited tracks are retried later.
    let cached = std::fs::read_dir(&r.cache).unwrap().count();
    assert_eq!(cached, 2, "plain + instrumental only");

    // Nothing playing: idle, and the previous lyrics are gone.
    player.quit();
    r.wait("idle", |s| s.status == LyricsStatus::Idle);
    assert!(service.current().lines.is_empty() && service.current().track_key.is_none());
}

#[test]
#[ignore = "contacts the real lrclib.net; run by hand only"]
fn real_lrclib_smoke() {
    let service = LyricsService::new(
        LyricsConfig {
            base: "https://lrclib.net".into(),
            cache_dir: None,
        },
        true,
    )
    .unwrap();
    // Exactly one real request, by hand: a well-known track must classify as an answer, never
    // as a transport / parse failure.
    let lookup = tauri::async_runtime::block_on(service.fetch(&TrackQuery {
        title: "Never Gonna Give You Up".into(),
        artist: "Rick Astley".into(),
        album: Some("Whenever You Need Somebody".into()),
        length_ms: Some(213_000),
    }));
    assert!(
        matches!(
            lookup,
            Lookup::Synced { .. } | Lookup::Plain(_) | Lookup::Instrumental | Lookup::NotFound
        ),
        "{lookup:?}"
    );
}
