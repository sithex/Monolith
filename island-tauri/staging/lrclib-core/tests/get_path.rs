use lrclib_core::{get_path, TrackQuery};

fn query(title: &str, artist: &str) -> TrackQuery {
    TrackQuery {
        title: title.into(),
        artist: artist.into(),
        album: None,
        length_ms: None,
    }
}

fn query_full(
    title: &str,
    artist: &str,
    album: Option<&str>,
    length_ms: Option<u64>,
) -> TrackQuery {
    TrackQuery {
        title: title.into(),
        artist: artist.into(),
        album: album.map(str::to_owned),
        length_ms,
    }
}

#[test]
fn minimal_title_and_artist() {
    let path = get_path(&query("Song", "Artist"));
    assert_eq!(
        path.as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist")
    );
}

#[test]
fn trims_title_and_artist() {
    let path = get_path(&query("  Song  ", "  Artist \t"));
    assert_eq!(
        path.as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist")
    );
}

#[test]
fn blank_title_or_artist_is_none() {
    assert_eq!(get_path(&query("", "Artist")), None);
    assert_eq!(get_path(&query("Song", "  ")), None);
    assert_eq!(get_path(&query(" ", "")), None);
}

#[test]
fn album_included_when_nonblank_and_trimmed() {
    let path = get_path(&query_full("Song", "Artist", Some("  Album  "), None));
    assert_eq!(
        path.as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist&album_name=Album")
    );
}

#[test]
fn blank_album_omitted() {
    for album in [Some(""), Some("   "), None] {
        let path = get_path(&query_full("Song", "Artist", album, None));
        assert_eq!(
            path.as_deref(),
            Some("/api/get?track_name=Song&artist_name=Artist")
        );
    }
}

#[test]
fn duration_omitted_when_unknown_or_out_of_range() {
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, None)).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist")
    );
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, Some(0))).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist")
    );
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, Some(499))).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist")
    );
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, Some(3_600_500))).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist")
    );
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, Some(3_600_999))).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist")
    );
}

#[test]
fn duration_rounds_to_nearest_second_500_up() {
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, Some(500))).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist&duration=1")
    );
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, Some(899))).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist&duration=1")
    );
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, Some(900))).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist&duration=1")
    );
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, Some(1499))).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist&duration=1")
    );
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, Some(1500))).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist&duration=2")
    );
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, Some(3_599_500))).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist&duration=3600")
    );
    assert_eq!(
        get_path(&query_full("Song", "Artist", None, Some(3_600_000))).as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist&duration=3600")
    );
}

#[test]
fn param_order_is_track_artist_album_duration() {
    let path = get_path(&query_full("Song", "Artist", Some("Album"), Some(1500)));
    assert_eq!(
        path.as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist&album_name=Album&duration=2")
    );
}

#[test]
fn forms_urlencoded_punctuation_and_space() {
    let path = get_path(&query("It's 100% & easy+=", "A"));
    assert_eq!(
        path.as_deref(),
        Some("/api/get?track_name=It%27s+100%25+%26+easy%2B%3D&artist_name=A")
    );
}

#[test]
fn unreserved_characters_kept() {
    let path = get_path(&query("A-Za-z0-9-._~", "A"));
    assert_eq!(
        path.as_deref(),
        Some("/api/get?track_name=A-Za-z0-9-._~&artist_name=A")
    );
}

#[test]
fn multibyte_utf8_encoded_per_byte_upper_hex() {
    let path = get_path(&query("héllo wörld", "Ж"));
    assert_eq!(
        path.as_deref(),
        Some("/api/get?track_name=h%C3%A9llo+w%C3%B6rld&artist_name=%D0%96")
    );
}

#[test]
fn album_is_encoded_too() {
    let path = get_path(&query_full("Song", "Artist", Some("B b&c"), None));
    assert_eq!(
        path.as_deref(),
        Some("/api/get?track_name=Song&artist_name=Artist&album_name=B+b%26c")
    );
}

#[test]
fn fields_capped_at_512_chars() {
    let long = "é".repeat(600);
    let path = get_path(&query(&long, "Artist")).unwrap_or_default();
    assert!(path.contains(&"%C3%A9".repeat(512)));
    assert!(!path.contains(&"%C3%A9".repeat(513)));
    assert!(path.ends_with("&artist_name=Artist"));
}

#[test]
fn cap_boundary_keeps_full_multibyte_scalar() {
    // 511 ASCII chars then two 2-byte chars: cap at 512 chars keeps one é whole.
    let title = format!("{}éé", "a".repeat(511));
    let path = get_path(&query(&title, "Artist")).unwrap_or_default();
    let kept = format!("{}%C3%A9&artist_name=Artist", "a".repeat(511));
    assert!(path.contains(&kept));
    assert!(!path.contains(&format!("{}%C3%A9%C3%A9&artist_name=", "a".repeat(511))));
}
