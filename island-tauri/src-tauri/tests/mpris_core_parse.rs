use hermes_island_lib::integrations::media::model::{
    parse_metadata, parse_status, PlaybackStatus, Track,
};
use serde_json::{json, Map, Value};

fn map(j: serde_json::Value) -> Map<String, Value> {
    match j {
        Value::Object(m) => m,
        _ => panic!("expected object"),
    }
}

#[test]
fn parse_status_exact_strings() {
    assert_eq!(parse_status("Playing"), Some(PlaybackStatus::Playing));
    assert_eq!(parse_status("Paused"), Some(PlaybackStatus::Paused));
    assert_eq!(parse_status("Stopped"), Some(PlaybackStatus::Stopped));
}

#[test]
fn parse_status_rejects_anything_else() {
    for s in [
        "playing",
        "PLAYING",
        "Paused ",
        "Stopped\n",
        "",
        "Pause",
        "Stoped",
        "foo",
        "123",
    ] {
        assert_eq!(parse_status(s), None);
    }
}

#[test]
fn metadata_minimal_title_only() {
    let t = parse_metadata(&map(json!({"xesam:title": "Song"}))).unwrap();
    assert_eq!(t.title, "Song");
    assert_eq!(t.track_id, None);
    assert!(t.artists.is_empty());
    assert_eq!(t.album, None);
    assert_eq!(t.art_url, None);
    assert_eq!(t.length_ms, None);
}

#[test]
fn metadata_missing_or_blank_title_is_none() {
    assert_eq!(parse_metadata(&map(json!({}))), None);
    assert_eq!(parse_metadata(&map(json!({"xesam:title": ""}))), None);
    assert_eq!(parse_metadata(&map(json!({"xesam:title": "   "}))), None);
    assert_eq!(parse_metadata(&map(json!({"xesam:title": 42}))), None);
    assert_eq!(parse_metadata(&map(json!({"xesam:title": null}))), None);
}

#[test]
fn metadata_artists_array_and_single_string() {
    let t = parse_metadata(&map(json!({
        "xesam:title": "S",
        "xesam:artist": ["A", "B"],
    })))
    .unwrap();
    assert_eq!(t.artists, vec!["A".to_string(), "B".to_string()]);

    let t2 = parse_metadata(&map(json!({
        "xesam:title": "S",
        "xesam:artist": "Solo",
    })))
    .unwrap();
    assert_eq!(t2.artists, vec!["Solo".to_string()]);

    let t3 = parse_metadata(&map(json!({
        "xesam:title": "S",
        "xesam:artist": 7,
    })))
    .unwrap();
    assert!(t3.artists.is_empty());
}

#[test]
fn metadata_drops_blank_artists_and_caps_at_sixteen() {
    let mut arr: Vec<Value> = Vec::new();
    for i in 0..30 {
        arr.push(json!(format!("Artist {i}")));
    }
    arr.push(json!("   "));
    arr.push(json!(""));
    let t = parse_metadata(&map(json!({
        "xesam:title": "S",
        "xesam:artist": arr,
    })))
    .unwrap();
    assert_eq!(t.artists.len(), 16);
    assert_eq!(t.artists[0], "Artist 0");
    assert_eq!(t.artists[15], "Artist 15");

    let t2 = parse_metadata(&map(json!({
        "xesam:title": "S",
        "xesam:artist": ["  ", "A", " ", "B", ""],
    })))
    .unwrap();
    assert_eq!(t2.artists, vec!["A".to_string(), "B".to_string()]);
}

#[test]
fn metadata_strings_trimmed_and_capped_multibyte_safe() {
    let long = "é".repeat(600);
    let t = parse_metadata(&map(json!({
        "xesam:title": long,
        "mpris:trackid": long,
        "xesam:album": long,
        "mpris:artUrl": long,
    })))
    .unwrap();
    for s in [
        &t.title,
        t.track_id.as_ref().unwrap(),
        t.album.as_ref().unwrap(),
        t.art_url.as_ref().unwrap(),
    ] {
        assert_eq!(s.chars().count(), 512);
        assert!(s.chars().all(|c| c == 'é'));
    }
}

#[test]
fn metadata_length_microseconds_to_ms() {
    let t = parse_metadata(&map(json!({
        "xesam:title": "S",
        "mpris:length": 12_345_678,
    })))
    .unwrap();
    assert_eq!(t.length_ms, Some(12345));

    let t2 = parse_metadata(&map(json!({
        "xesam:title": "S",
        "mpris:length": 999,
    })))
    .unwrap();
    assert_eq!(t2.length_ms, Some(0));

    let t3 = parse_metadata(&map(json!({
        "xesam:title": "S",
        "mpris:length": 1_000_000,
    })))
    .unwrap();
    assert_eq!(t3.length_ms, Some(1000));
}

#[test]
fn metadata_length_invalid_values_give_none() {
    for bad in [
        json!(0),
        json!(-5),
        json!(-1),
        json!(1.5),
        json!(0.0),
        json!("123"),
        json!(true),
    ] {
        let t = parse_metadata(&map(json!({
            "xesam:title": "S",
            "mpris:length": bad,
        })))
        .unwrap();
        assert_eq!(t.length_ms, None, "length {bad:?}");
    }
}

#[test]
fn track_wires_camelcase_and_no_alias() {
    let t = Track {
        track_id: Some("tid".into()),
        title: "T".into(),
        artists: vec!["A".into()],
        album: Some("Al".into()),
        art_url: Some("http://x".into()),
        length_ms: Some(1234),
    };
    let v = serde_json::to_value(&t).unwrap();
    let obj = v.as_object().unwrap();
    assert!(obj.contains_key("trackId"));
    assert!(obj.contains_key("artUrl"));
    assert!(obj.contains_key("lengthMs"));
    assert!(obj.contains_key("title"));
    assert!(!obj.contains_key("track_id"));

    let decoded: Track = serde_json::from_value(json!({
        "trackId": "real",
        "title": "T",
        "artists": [],
        "album": null,
        "artUrl": null,
        "lengthMs": null,
    }))
    .unwrap();
    assert_eq!(decoded.track_id.as_deref(), Some("real"));

    // snake_case keys must NOT populate the camelCase fields (no aliases).
    let decoded2: Track = serde_json::from_value(json!({
        "track_id": "fake",
        "title": "T",
        "artists": [],
    }))
    .unwrap();
    assert_eq!(decoded2.track_id, None);
}
