//! Pure core for LRCLIB lyrics lookup (SPEC §8 Phase 6 "Music extras").
//!
//! Builds the `GET /api/get` query, classifies an HTTP response into a
//! [`Lookup`], and derives a cache key. The HTTP transport itself lives in the
//! integrating phase; this crate does no IO, no network, no threads, reads no
//! clock, uses no `unsafe`, and never panics on any input.
//!
//! Upstream contract (https://lrclib.net/docs): keyless lyrics API. A record
//! is JSON with `id` (number), `trackName`/`artistName`/`albumName` (strings),
//! `duration` (number, seconds, may be fractional), `instrumental` (bool),
//! `plainLyrics` (string|null), `syncedLyrics` (string|null, LRC text), plus
//! other fields that must be ignored. Errors are JSON `{message, name,
//! statusCode}`; 404 name `TrackNotFound`; 429 `RateLimitExceeded` and 503
//! `ServerOverloaded` honor `Retry-After` (seconds); some rejections are plain
//! text, so status and Content-Type are checked before assuming JSON.

#![forbid(unsafe_code)]

use serde::Deserialize;
use std::hash::Hasher;
use xxhash_rust::xxh3::Xxh3;

/// `User-Agent` header value every client must send: app name, crate version,
/// and a homepage.
pub const USER_AGENT: &str = concat!(
    "HermesIsland/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/NousResearch/hermes-agent)"
);

/// A track lookup query. `length_ms` may be unknown (`None`); when known it is
/// rounded to the nearest second and included only if within 1..=3600.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackQuery {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub length_ms: Option<u64>,
}

/// The classification of an LRCLIB HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// Synced LRC lyrics found; `plain` carries non-blank plain lyrics if any.
    Synced { lrc: String, plain: Option<String> },
    /// Only plain lyrics found.
    Plain(String),
    /// Track is instrumental (no lyrics).
    Instrumental,
    /// No matching track / no lyrics (404 or record with blank lyrics).
    NotFound,
    /// Rate-limited (429) or overloaded (503); retry after this many seconds.
    RetryAfter { seconds: u64 },
    /// Any other failure; carries the HTTP status.
    Failed { status: u16 },
}

/// Maximum accepted response body, in bytes (1 MiB, checked before parsing).
const MAX_BODY: usize = 1024 * 1024;

/// Cap for each query field, in characters (char-boundary safe).
const FIELD_CAP: usize = 512;

/// Returns the relative request path for `q`, or `None` when the title or
/// artist is blank after trimming. Fields are trimmed, capped at 512 chars,
/// and percent-encoded with application/x-www-form-urlencoded rules.
pub fn get_path(q: &TrackQuery) -> Option<String> {
    let title = trimmed_capped(&q.title);
    if title.is_empty() {
        return None;
    }
    let artist = trimmed_capped(&q.artist);
    if artist.is_empty() {
        return None;
    }

    let mut path = format!(
        "/api/get?track_name={}&artist_name={}",
        form_encode(&title),
        form_encode(&artist)
    );

    if let Some(album) = q.album.as_deref() {
        let album = trimmed_capped(album);
        if !album.is_empty() {
            path.push_str("&album_name=");
            path.push_str(&form_encode(&album));
        }
    }

    if let Some(secs) = duration_seconds(q.length_ms) {
        path.push_str("&duration=");
        path.push_str(&secs.to_string());
    }

    Some(path)
}

/// Classifies an HTTP response into a [`Lookup`]. Never panics on any bytes.
///
/// * Body > 1 MiB → `Failed { status }`, checked before anything else.
/// * 200: JSON `Content-Type` (`application/json`, params allowed,
///   case-insensitive) and a valid record required; otherwise `Failed { 200 }`.
/// * 404 → `NotFound` regardless of body. 429/503 → `RetryAfter` with
///   `Retry-After` delta-seconds (default 60 when missing/unparseable, clamped
///   to 1..=3600). Any other status → `Failed { status }`.
pub fn parse_response(
    status: u16,
    content_type: Option<&str>,
    retry_after: Option<&str>,
    body: &[u8],
) -> Lookup {
    if body.len() > MAX_BODY {
        return Lookup::Failed { status };
    }

    match status {
        200 => {
            if !is_json_content_type(content_type) {
                return Lookup::Failed { status: 200 };
            }
            match serde_json::from_slice::<LyricsRecord>(body) {
                Ok(record) => classify_record(record),
                Err(_) => Lookup::Failed { status: 200 },
            }
        }
        404 => Lookup::NotFound,
        429 | 503 => Lookup::RetryAfter {
            seconds: retry_seconds(retry_after),
        },
        other => Lookup::Failed { status: other },
    }
}

/// Deterministic cache key: xxh3-64 lowercase hex of the normalized
/// `(title, artist, album-or-"", duration-seconds-or-"")` tuple, where
/// normalization trims, Unicode-lowercases, and collapses internal whitespace
/// runs to a single space. Components are length-prefixed so field boundaries
/// are unambiguous (e.g. ("a b","c") and ("a","b c") differ). Returns `None`
/// for a blank title or artist.
pub fn cache_key(q: &TrackQuery) -> Option<String> {
    let title = normalize(&q.title);
    if title.is_empty() {
        return None;
    }
    let artist = normalize(&q.artist);
    if artist.is_empty() {
        return None;
    }
    let album = match q.album.as_deref() {
        Some(album) => normalize(album),
        None => String::new(),
    };
    let duration = match duration_seconds(q.length_ms) {
        Some(secs) => secs.to_string(),
        None => String::new(),
    };

    let mut hasher = Xxh3::new();
    for field in [&title, &artist, &album, &duration] {
        hasher.write_u64(field.len() as u64);
        hasher.write(field.as_bytes());
    }
    Some(format!("{:016x}", hasher.finish()))
}

/// A valid LRCLIB lyrics record. Only the fields the crate actually reads are
/// deserialized: the rest of the upstream record (`id`, `trackName`,
/// `artistName`, `albumName`, `duration`, …) is ignored, so live data with a
/// null/missing album or a missing/integer duration still classifies.
/// `instrumental` defaults to false when absent or null
/// (`Option<bool>` + `default`); `plainLyrics`/`syncedLyrics` are optional and
/// null-tolerant. Unknown fields are ignored (no `deny_unknown_fields`). A
/// body that is not an object, or where one of these fields has the wrong type
/// (e.g. a number for `syncedLyrics`), fails to deserialize and yields
/// `Failed { 200 }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LyricsRecord {
    #[serde(default)]
    instrumental: Option<bool>,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

fn classify_record(record: LyricsRecord) -> Lookup {
    if record.instrumental.unwrap_or_default() {
        return Lookup::Instrumental;
    }
    if let Some(synced) = record.synced_lyrics {
        let synced = synced.trim();
        if !synced.is_empty() {
            let plain = record
                .plain_lyrics
                .as_deref()
                .map(str::trim)
                .filter(|plain| !plain.is_empty())
                .map(str::to_owned);
            return Lookup::Synced {
                lrc: synced.to_owned(),
                plain,
            };
        }
    }
    if let Some(plain) = record.plain_lyrics {
        let plain = plain.trim();
        if !plain.is_empty() {
            return Lookup::Plain(plain.to_owned());
        }
    }
    Lookup::NotFound
}

/// `Retry-After` delta-seconds: trimmed integer, default 60 when missing or
/// unparseable, clamped to 1..=3600.
fn retry_seconds(retry_after: Option<&str>) -> u64 {
    let Some(raw) = retry_after else {
        return 60;
    };
    let Ok(secs) = raw.trim().parse::<u64>() else {
        return 60;
    };
    secs.clamp(1, 3600)
}

/// True when `content_type` is `application/json` (media type before any `;`
/// parameter, trimmed, ASCII case-insensitive).
fn is_json_content_type(content_type: Option<&str>) -> bool {
    match content_type {
        None => false,
        Some(content_type) => {
            let media = content_type.split(';').next().unwrap_or("").trim();
            media.eq_ignore_ascii_case("application/json")
        }
    }
}

/// Rounds `length_ms` to the nearest second (500 ms rounds up) and keeps it
/// only when within 1..=3600.
fn duration_seconds(length_ms: Option<u64>) -> Option<u64> {
    let secs = length_ms?.saturating_add(500) / 1000;
    if (1..=3600).contains(&secs) {
        Some(secs)
    } else {
        None
    }
}

/// Trim, then cap at 512 chars on a char boundary.
fn trimmed_capped(s: &str) -> String {
    s.trim().chars().take(FIELD_CAP).collect()
}

/// Normalize for cache keys: trim + Unicode lowercase + collapse internal
/// whitespace runs to one space.
fn normalize(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Application/x-www-form-urlencoded encoding: unreserved `A-Za-z0-9-._~`
/// kept, space → `+`, everything else `%XX` uppercase per UTF-8 byte.
fn form_encode(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for &byte in s.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_encode_keeps_unreserved_and_space_becomes_plus() {
        assert_eq!(form_encode("A-Za-z0-9-._~ "), "A-Za-z0-9-._~+");
    }

    #[test]
    fn form_encode_percent_upper_hex_per_byte() {
        assert_eq!(form_encode("'&=+%"), "%27%26%3D%2B%25");
        assert_eq!(form_encode("héllo"), "h%C3%A9llo");
    }

    #[test]
    fn normalize_trims_lowercases_collapses_whitespace() {
        assert_eq!(normalize("  Hello   World\t\n "), "hello world");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn duration_seconds_rounds_nearest_500_up() {
        assert_eq!(duration_seconds(None), None);
        assert_eq!(duration_seconds(Some(0)), None);
        assert_eq!(duration_seconds(Some(499)), None);
        assert_eq!(duration_seconds(Some(500)), Some(1));
        assert_eq!(duration_seconds(Some(899)), Some(1));
        assert_eq!(duration_seconds(Some(900)), Some(1));
        assert_eq!(duration_seconds(Some(1499)), Some(1));
        assert_eq!(duration_seconds(Some(1500)), Some(2));
        assert_eq!(duration_seconds(Some(3_599_500)), Some(3600));
        assert_eq!(duration_seconds(Some(3_600_000)), Some(3600));
        assert_eq!(duration_seconds(Some(3_600_500)), None);
        assert_eq!(duration_seconds(Some(u64::MAX)), None);
    }

    #[test]
    fn json_content_type_media_type_param_insensitive() {
        assert!(is_json_content_type(Some("application/json")));
        assert!(is_json_content_type(Some("Application/JSON")));
        assert!(is_json_content_type(Some(
            "application/json; charset=utf-8"
        )));
        assert!(is_json_content_type(Some(
            " application/json ;charset=UTF-8"
        )));
        assert!(!is_json_content_type(Some("text/plain")));
        assert!(!is_json_content_type(Some("application/problem+json")));
        assert!(!is_json_content_type(Some("")));
        assert!(!is_json_content_type(None));
    }
}
