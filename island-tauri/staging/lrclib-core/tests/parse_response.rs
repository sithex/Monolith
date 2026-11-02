use lrclib_core::{parse_response, Lookup};

const ONE_MIB: usize = 1024 * 1024;

fn rec(instrumental: bool, plain: Option<&str>, synced: Option<&str>) -> String {
    serde_json::json!({
        "id": 42,
        "trackName": "T",
        "artistName": "A",
        "albumName": "Al",
        "duration": 180.5,
        "instrumental": instrumental,
        "plainLyrics": plain,
        "syncedLyrics": synced,
        "hasWordSync": true,
        "lyricsfile": null,
    })
    .to_string()
}

fn parse(ct: Option<&str>, retry: Option<&str>, body: &[u8]) -> Lookup {
    parse_response(200, ct, retry, body)
}

#[test]
fn synced_wins_with_inline_plain() {
    let body = rec(false, Some("Plain lyrics"), Some("[00:01.00]Line"));
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::Synced {
            lrc: "[00:01.00]Line".into(),
            plain: Some("Plain lyrics".into()),
        }
    );
}

#[test]
fn synced_with_blank_or_null_plain() {
    let body = rec(false, Some("   "), Some("[00:01.00]Line"));
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::Synced {
            lrc: "[00:01.00]Line".into(),
            plain: None,
        }
    );
    let body = rec(false, None, Some("[00:01.00]Line"));
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::Synced {
            lrc: "[00:01.00]Line".into(),
            plain: None,
        }
    );
}

#[test]
fn plain_only_and_trimmed() {
    let body = rec(false, Some("  Plain lyrics  "), None);
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::Plain("Plain lyrics".into())
    );
}

#[test]
fn instrumental_beats_any_lyrics() {
    let body = rec(true, Some("Plain"), Some("[00:01.00]Line"));
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::Instrumental
    );
}

#[test]
fn both_lyrics_null_or_blank_is_not_found() {
    let body = rec(false, None, None);
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::NotFound
    );
    let body = rec(false, Some("   "), Some(" \t "));
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::NotFound
    );
}

#[test]
fn unknown_extra_fields_ignored() {
    let body = rec(false, Some("P"), Some("S"));
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::Synced {
            lrc: "S".into(),
            plain: Some("P".into()),
        }
    );
}

#[test]
fn json_content_type_allows_params_case_insensitive() {
    for ct in [
        "application/json",
        "Application/JSON",
        "application/json; charset=utf-8",
        "application/json;charset=UTF-8",
        " application/json ;charset=utf-8",
    ] {
        let body = rec(false, Some("P"), None);
        assert_eq!(
            parse(Some(ct), None, body.as_bytes()),
            Lookup::Plain("P".into()),
            "ct={ct}"
        );
    }
}

#[test]
fn non_json_200_is_failed() {
    for ct in [
        None,
        Some(""),
        Some("text/plain"),
        Some("application/problem+json"),
        Some("text/json"),
    ] {
        let body = b"ok";
        assert_eq!(
            parse(ct, None, body),
            Lookup::Failed { status: 200 },
            "ct={ct:?}"
        );
    }
}

#[test]
fn invalid_json_or_invalid_record_200_is_failed() {
    for body in [&b"{not json"[..], &b"null"[..], &b"[]"[..], &b"[[1,2]"[..]] {
        assert_eq!(
            parse(Some("application/json"), None, body),
            Lookup::Failed { status: 200 }
        );
    }
}

#[test]
fn album_null_with_synced_is_synced() {
    let body = serde_json::json!({
        "id": 42,
        "trackName": "T",
        "artistName": "A",
        "albumName": null,
        "duration": 180.5,
        "instrumental": false,
        "plainLyrics": null,
        "syncedLyrics": "[00:01.00]Line",
    })
    .to_string();
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::Synced {
            lrc: "[00:01.00]Line".into(),
            plain: None,
        }
    );
}

#[test]
fn record_missing_metadata_with_plain_is_plain() {
    let body = serde_json::json!({
        "syncedLyrics": null,
        "plainLyrics": "Plain lyrics",
    })
    .to_string();
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::Plain("Plain lyrics".into())
    );
}

#[test]
fn missing_instrumental_with_synced_is_synced() {
    let body = serde_json::json!({
        "syncedLyrics": "[00:01.00]Line",
        "plainLyrics": null,
    })
    .to_string();
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::Synced {
            lrc: "[00:01.00]Line".into(),
            plain: None,
        }
    );
}

#[test]
fn wrong_type_synced_lyrics_is_failed_200() {
    let body = serde_json::json!({ "syncedLyrics": 5 }).to_string();
    assert_eq!(
        parse(Some("application/json"), None, body.as_bytes()),
        Lookup::Failed { status: 200 }
    );
}

#[test]
fn json_array_body_is_failed_200() {
    assert_eq!(
        parse(Some("application/json"), None, b"[]"),
        Lookup::Failed { status: 200 }
    );
}

#[test]
fn empty_object_is_not_found() {
    assert_eq!(
        parse(Some("application/json"), None, b"{}"),
        Lookup::NotFound
    );
}

#[test]
fn status_404_is_not_found_whatever_the_body() {
    for body in [
        &b"{\"message\":\"Track not found\",\"name\":\"TrackNotFound\",\"statusCode\":404}"[..],
        &b"Not Found"[..],
        &b""[..],
    ] {
        assert_eq!(
            parse_response(404, Some("text/plain"), None, body),
            Lookup::NotFound
        );
    }
}

#[test]
fn status_429_retry_after_delta_seconds() {
    let cases: &[(&str, u64)] = &[
        ("5", 5),
        (" 42 ", 42),
        ("3600", 3600),
        ("3601", 3600),
        ("99999", 3600),
        ("1", 1),
        ("0", 1),
        ("abc", 60),
        ("", 60),
        ("5.5", 60),
        ("-3", 60),
        ("99999999999999999999999", 60),
    ];
    for &(retry, expected) in cases {
        let body = b"rate limited";
        assert_eq!(
            parse_response(429, Some("text/plain"), Some(retry), body),
            Lookup::RetryAfter { seconds: expected },
            "retry={retry:?}"
        );
    }
    assert_eq!(
        parse_response(429, None, None, b""),
        Lookup::RetryAfter { seconds: 60 }
    );
}

#[test]
fn status_503_retry_after() {
    assert_eq!(
        parse_response(503, Some("application/json"), Some("12"), b"{}"),
        Lookup::RetryAfter { seconds: 12 }
    );
    assert_eq!(
        parse_response(503, None, None, b""),
        Lookup::RetryAfter { seconds: 60 }
    );
}

#[test]
fn any_other_status_is_failed_with_status() {
    for status in [0u16, 100, 201, 301, 400, 418, 500, 599] {
        assert_eq!(
            parse_response(status, None, None, b"x"),
            Lookup::Failed { status },
            "status={status}"
        );
    }
}

#[test]
fn oversize_body_fails_before_status_dispatch() {
    let body = vec![b'a'; ONE_MIB + 1];
    assert_eq!(
        parse_response(200, Some("application/json"), None, &body),
        Lookup::Failed { status: 200 }
    );
    assert_eq!(
        parse_response(404, None, None, &body),
        Lookup::Failed { status: 404 }
    );
    assert_eq!(
        parse_response(429, None, Some("5"), &body),
        Lookup::Failed { status: 429 }
    );
    assert_eq!(
        parse_response(503, None, None, &body),
        Lookup::Failed { status: 503 }
    );
    assert_eq!(
        parse_response(418, None, None, &body),
        Lookup::Failed { status: 418 }
    );
}

#[test]
fn exactly_one_mib_is_parsed_but_not_more() {
    let mut body = rec(false, Some("P"), None).into_bytes();
    while body.len() < ONE_MIB {
        body.push(b' ');
    }
    assert_eq!(body.len(), ONE_MIB);
    assert_eq!(
        parse(Some("application/json"), None, &body),
        Lookup::Plain("P".into())
    );
    body.push(b' ');
    assert_eq!(
        parse(Some("application/json"), None, &body),
        Lookup::Failed { status: 200 }
    );
}
