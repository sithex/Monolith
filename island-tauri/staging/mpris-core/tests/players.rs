use mpris_core::{position_at, PlaybackStatus, PlayerState, Players, Track};
use serde_json::{json, Value};

fn meta(title: &str) -> serde_json::Map<String, Value> {
    match json!({"xesam:title": title}) {
        Value::Object(m) => m,
        _ => unreachable!(),
    }
}

fn meta_full(title: &str, track_id: Option<&str>) -> serde_json::Map<String, Value> {
    let mut m = meta(title);
    if let Some(id) = track_id {
        m.insert("mpris:trackid".into(), json!(id));
    }
    m
}

fn track(len: Option<u64>) -> Track {
    Track {
        track_id: None,
        title: "T".into(),
        artists: Vec::new(),
        album: None,
        art_url: None,
        length_ms: len,
    }
}

fn playing_state() -> PlayerState {
    PlayerState {
        bus_name: "org.mpris.MediaPlayer2.x".into(),
        status: PlaybackStatus::Playing,
        track: None,
        rate: 1.0,
        position_ms: 1000,
        position_at_ms: 5000,
        last_playing_at_ms: Some(5000),
    }
}

#[test]
fn position_at_playing_interpolates() {
    let p = playing_state();
    assert_eq!(position_at(&p, 7000), 3000);
}

#[test]
fn position_at_rate_scales() {
    let mut p = playing_state();
    p.rate = 2.0;
    assert_eq!(position_at(&p, 7000), 5000);
}

#[test]
fn position_at_rate_nonfinite_or_nonpositive_treated_as_one() {
    for r in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut p = playing_state();
        p.rate = r;
        assert_eq!(position_at(&p, 7000), 3000, "rate {r:?}");
    }
}

#[test]
fn position_at_paused_frozen() {
    let mut p = playing_state();
    p.status = PlaybackStatus::Paused;
    assert_eq!(position_at(&p, 999_999), 1000);
}

#[test]
fn position_at_stopped_zero() {
    let mut p = playing_state();
    p.status = PlaybackStatus::Stopped;
    p.position_ms = 5000;
    assert_eq!(position_at(&p, 999_999), 0);
}

#[test]
fn position_at_clock_backwards_returns_position() {
    let p = playing_state();
    assert_eq!(position_at(&p, 4999), 1000);
}

#[test]
fn position_at_clamps_to_length() {
    let mut p = playing_state();
    p.track = Some(track(Some(4000)));
    assert_eq!(position_at(&p, 999_999), 4000);
    assert_eq!(position_at(&p, 6000), 2000);
}

#[test]
fn position_at_zero_length_clamps_to_zero() {
    let mut p = playing_state();
    p.track = Some(track(Some(0)));
    assert_eq!(position_at(&p, 7000), 0);
}

#[test]
fn playing_counts_from_zero() {
    let mut ps = Players::default();
    ps.upsert_status("org.mpris.MediaPlayer2.spotify", PlaybackStatus::Playing, 0);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 5000), 5000);
}

#[test]
fn pause_freezes_and_resume_continues() {
    let mut ps = Players::default();
    let bus = "org.mpris.MediaPlayer2.spotify";
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.upsert_status(bus, PlaybackStatus::Paused, 5000);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 9000), 5000);
    ps.upsert_status(bus, PlaybackStatus::Playing, 9000);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 11_000), 7000);
}

#[test]
fn stopped_returns_zero_even_after_playing() {
    let mut ps = Players::default();
    let bus = "org.mpris.MediaPlayer2.spotify";
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.upsert_status(bus, PlaybackStatus::Stopped, 5000);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 9000), 0);
    assert_eq!(p.position_ms, 0);
}

#[test]
fn rate_change_reanchors() {
    let mut ps = Players::default();
    let bus = "org.mpris.MediaPlayer2.spotify";
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.set_rate(bus, 2.0, 2000);
    let p = ps.preferred().unwrap();
    // Re-anchored: 2000 + (4000-2000)*2 = 6000. Without re-anchoring it
    // would be 0 + (4000-0)*2 = 8000.
    assert_eq!(position_at(p, 4000), 6000);
}

#[test]
fn metadata_track_change_resets_position() {
    let mut ps = Players::default();
    let bus = "org.mpris.MediaPlayer2.spotify";
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.set_metadata(bus, &meta("First"), 0);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 5000), 5000);
    ps.set_metadata(bus, &meta("Second"), 5000);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 6000), 1000);
}

#[test]
fn metadata_same_track_does_not_reset() {
    let mut ps = Players::default();
    let bus = "org.mpris.MediaPlayer2.spotify";
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.set_metadata(bus, &meta("Same"), 0);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 5000), 5000);
    ps.set_metadata(bus, &meta("Same"), 5000);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 7000), 7000);
}

#[test]
fn metadata_title_change_resets_even_with_same_trackid() {
    let mut ps = Players::default();
    let bus = "org.mpris.MediaPlayer2.spotify";
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.set_metadata(bus, &meta_full("One", Some("id1")), 0);
    ps.set_metadata(bus, &meta_full("Two", Some("id1")), 5000);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 6000), 1000);
}

#[test]
fn metadata_trackid_change_resets_even_with_same_title() {
    let mut ps = Players::default();
    let bus = "org.mpris.MediaPlayer2.spotify";
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.set_metadata(bus, &meta_full("Same", Some("id1")), 0);
    ps.set_metadata(bus, &meta_full("Same", Some("id2")), 5000);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 6000), 1000);
}

#[test]
fn seek_while_paused_updates_frozen_position() {
    let mut ps = Players::default();
    let bus = "org.mpris.MediaPlayer2.spotify";
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.upsert_status(bus, PlaybackStatus::Paused, 4000);
    ps.seeked(bus, 5_000_000, 5000);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 999_999), 5000);
}

#[test]
fn seek_negative_clamps_to_zero() {
    let mut ps = Players::default();
    let bus = "org.mpris.MediaPlayer2.spotify";
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.seeked(bus, -10, 4000);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 4000), 0);
    assert_eq!(position_at(p, 5000), 1000);
}

#[test]
fn set_position_absolute() {
    let mut ps = Players::default();
    let bus = "org.mpris.MediaPlayer2.spotify";
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.set_position(bus, 3_000_000, 1000);
    let p = ps.preferred().unwrap();
    assert_eq!(position_at(p, 2000), 4000);
}

#[test]
fn invalid_bus_names_ignored() {
    let mut ps = Players::default();
    for bad in [
        "org.foo.Bar",
        "org.mpris.MediaPlayer2",
        "mpris.x",
        "",
        "org.MPRIS.MediaPlayer2.x",
    ] {
        ps.upsert_status(bad, PlaybackStatus::Playing, 0);
        ps.set_metadata(bad, &meta("T"), 0);
        ps.seeked(bad, 1000, 0);
        ps.set_position(bad, 1000, 0);
        ps.set_rate(bad, 2.0, 0);
        ps.remove(bad);
    }
    assert!(ps.preferred().is_none());
}

#[test]
fn preferred_playing_beats_later_last_playing() {
    let mut ps = Players::default();
    let a = "org.mpris.MediaPlayer2.aaa";
    let b = "org.mpris.MediaPlayer2.bbb";
    ps.upsert_status(b, PlaybackStatus::Playing, 200);
    ps.upsert_status(b, PlaybackStatus::Paused, 210);
    ps.upsert_status(a, PlaybackStatus::Playing, 100);
    let p = ps.preferred().unwrap();
    assert_eq!(p.bus_name, a);
}

#[test]
fn preferred_tie_break_lexical() {
    let mut ps = Players::default();
    let a = "org.mpris.MediaPlayer2.aaa";
    let b = "org.mpris.MediaPlayer2.bbb";
    ps.upsert_status(b, PlaybackStatus::Playing, 0);
    ps.upsert_status(a, PlaybackStatus::Playing, 0);
    let p = ps.preferred().unwrap();
    assert_eq!(p.bus_name, a);
}

#[test]
fn preferred_no_playing_latest_last_playing_wins() {
    let mut ps = Players::default();
    let a = "org.mpris.MediaPlayer2.aaa";
    let b = "org.mpris.MediaPlayer2.bbb";
    ps.upsert_status(a, PlaybackStatus::Playing, 100);
    ps.upsert_status(a, PlaybackStatus::Paused, 110);
    ps.upsert_status(b, PlaybackStatus::Playing, 200);
    ps.upsert_status(b, PlaybackStatus::Paused, 210);
    let p = ps.preferred().unwrap();
    assert_eq!(p.bus_name, b);
}

#[test]
fn preferred_never_played_lexical_first_with_track() {
    let mut ps = Players::default();
    let a = "org.mpris.MediaPlayer2.aaa";
    let b = "org.mpris.MediaPlayer2.bbb";
    let c = "org.mpris.MediaPlayer2.ccc";
    ps.set_metadata(a, &meta("A"), 0);
    assert_eq!(ps.preferred().unwrap().bus_name, a);
    ps.set_metadata(c, &meta("U"), 0);
    assert_eq!(ps.preferred().unwrap().bus_name, a); // still a (lexical)
    ps.set_metadata(b, &meta("T"), 0);
    assert_eq!(ps.preferred().unwrap().bus_name, a); // a still ahead of b/c
    ps.remove(a);
    assert_eq!(ps.preferred().unwrap().bus_name, b); // b comes before c
    ps.remove(b);
    assert_eq!(ps.preferred().unwrap().bus_name, c);
}

#[test]
fn preferred_empty_and_no_track_none() {
    let ps = Players::default();
    assert!(ps.preferred().is_none());
    let mut ps2 = Players::default();
    ps2.upsert_status("org.mpris.MediaPlayer2.aaa", PlaybackStatus::Stopped, 0);
    assert!(ps2.preferred().is_none());
}

#[test]
fn non_mpris_buses_ignored_by_all_mutating_methods() {
    // None of these is a valid MPRIS player bus name: a foreign namespace,
    // the bare prefix without the trailing "." + suffix, and the empty
    // string. Every mutating method must ignore them so nothing is stored
    // and `preferred()` stays None.
    for bad in [
        "org.freedesktop.Notifications",
        "org.mpris.MediaPlayer2", // prefix but no "." + suffix
        "",
    ] {
        let mut ps = Players::default();
        ps.upsert_status(bad, PlaybackStatus::Playing, 0);
        assert!(
            ps.preferred().is_none(),
            "{bad:?} visible after upsert_status"
        );
        ps.set_metadata(bad, &meta("T"), 1000);
        assert!(
            ps.preferred().is_none(),
            "{bad:?} visible after set_metadata"
        );
        ps.seeked(bad, 5_000_000, 2000);
        assert!(ps.preferred().is_none(), "{bad:?} visible after seeked");
        ps.set_position(bad, 3_000_000, 3000);
        assert!(
            ps.preferred().is_none(),
            "{bad:?} visible after set_position"
        );
        ps.set_rate(bad, 2.0, 4000);
        assert!(ps.preferred().is_none(), "{bad:?} visible after set_rate");
        ps.remove(bad);
        assert!(ps.preferred().is_none());
    }
}

#[test]
fn spotify_bus_accepted() {
    let mut ps = Players::default();
    ps.set_metadata("org.mpris.MediaPlayer2.spotify", &meta("T"), 0);
    let p = ps.preferred().unwrap();
    assert_eq!(p.bus_name, "org.mpris.MediaPlayer2.spotify");
}

#[test]
fn seek_and_set_position_negative_one_and_sub_ms_boundaries() {
    let bus = "org.mpris.MediaPlayer2.spotify";

    // Exactly −1 µs must clamp to position 0. This pins the
    // `if position_us < 0` boundary in `us_to_ms`: a `< -1` guard would let
    // `-1i64 as u64 / 1000` through as a huge position.
    let mut ps = Players::default();
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.seeked(bus, -1, 1000);
    let p = ps.preferred().unwrap();
    assert_eq!(p.position_ms, 0);
    assert_eq!(position_at(p, 1000), 0);

    // Same boundary through set_position.
    let mut ps = Players::default();
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.set_position(bus, -1, 1000);
    let p = ps.preferred().unwrap();
    assert_eq!(p.position_ms, 0);
    assert_eq!(position_at(p, 1000), 0);

    // Sub-millisecond µs values truncate on conversion: 999 µs → 0 ms,
    // 1000 µs → 1 ms.
    let mut ps = Players::default();
    ps.upsert_status(bus, PlaybackStatus::Playing, 0);
    ps.seeked(bus, 999, 1000);
    let p = ps.preferred().unwrap();
    assert_eq!(p.position_ms, 0);
    ps.seeked(bus, 1000, 2000);
    let p = ps.preferred().unwrap();
    assert_eq!(p.position_ms, 1);
}
