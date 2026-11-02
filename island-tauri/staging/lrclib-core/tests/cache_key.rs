use lrclib_core::{cache_key, TrackQuery};

fn q(title: &str, artist: &str, album: Option<&str>, length_ms: Option<u64>) -> TrackQuery {
    TrackQuery {
        title: title.into(),
        artist: artist.into(),
        album: album.map(str::to_owned),
        length_ms,
    }
}

#[test]
fn deterministic_lowercase_hex() {
    let a = cache_key(&q("Song", "Artist", None, None)).unwrap_or_default();
    let b = cache_key(&q("Song", "Artist", None, None)).unwrap_or_default();
    assert_eq!(a, b);
    assert_eq!(a.len(), 16);
    assert!(a
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[test]
fn normalization_trims_lowercases_collapses_whitespace() {
    let a = cache_key(&q("  Hello World  ", " Artist ", None, None));
    let b = cache_key(&q("hello   world", "  artist \t", None, None));
    assert_eq!(a, b);
    assert!(a.is_some());
}

#[test]
fn blank_title_or_artist_is_none() {
    assert_eq!(cache_key(&q("", "a", None, None)), None);
    assert_eq!(cache_key(&q(" ", "a", None, None)), None);
    assert_eq!(cache_key(&q("a", "", None, None)), None);
    assert_eq!(cache_key(&q("a", "\t", None, None)), None);
}

#[test]
fn blank_album_eq_absent_album() {
    let a = cache_key(&q("Song", "Artist", None, None));
    let b = cache_key(&q("Song", "Artist", Some(""), None));
    let c = cache_key(&q("Song", "Artist", Some("   "), None));
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn omitted_duration_eq_unknown_duration() {
    let a = cache_key(&q("Song", "Artist", None, None));
    assert_eq!(a, cache_key(&q("Song", "Artist", None, Some(0))));
    assert_eq!(a, cache_key(&q("Song", "Artist", None, Some(499))));
    assert_eq!(a, cache_key(&q("Song", "Artist", None, Some(3_600_500))));
    assert_ne!(a, cache_key(&q("Song", "Artist", None, Some(500))));
    assert_ne!(a, cache_key(&q("Song", "Artist", None, Some(3_600_000))));
}

#[test]
fn separator_makes_component_boundaries_distinct() {
    // Naive concatenation would collide on these pairs.
    assert_ne!(
        cache_key(&q("a", "bc", None, None)),
        cache_key(&q("ab", "c", None, None))
    );
    // Explicit requirement from the spec.
    assert_ne!(
        cache_key(&q("a b", "c", None, None)),
        cache_key(&q("a", "b c", None, None))
    );
    assert_ne!(
        cache_key(&q("a", "b", Some("c"), None)),
        cache_key(&q("a", "b", None, None))
    );
}

#[test]
fn different_fields_produce_different_keys() {
    assert_ne!(
        cache_key(&q("a", "x", None, None)),
        cache_key(&q("a", "y", None, None))
    );
    assert_ne!(
        cache_key(&q("x", "b", None, None)),
        cache_key(&q("y", "b", None, None))
    );
    assert_ne!(
        cache_key(&q("a", "b", Some("x"), None)),
        cache_key(&q("a", "b", Some("y"), None))
    );
}
