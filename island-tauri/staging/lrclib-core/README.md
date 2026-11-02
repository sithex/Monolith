# lrclib-core

Pure Rust core for the LRCLIB lyrics lookup (SPEC §8 Phase 6 "Music extras").
Builds the `GET /api/get` query, classifies the HTTP response into a
[`Lookup`], and derives a deterministic cache key. HTTP transport itself lives
in the integrating phase; this crate does no IO, no network, no threads, reads
no clock, uses no `unsafe`, and never panics on any input.

Upstream contract (https://lrclib.net/docs): keyless lyrics API. `track_name`
and `artist_name` are required; `album_name` and `duration` (seconds, 1..=3600,
matched within ±2 s) are optional. A record is JSON with `id`, `trackName`,
`artistName`, `albumName`, `duration` (may be fractional), `instrumental`,
`plainLyrics`, `syncedLyrics` (LRC), plus other fields that must be ignored.
Errors are `{message, name, statusCode}`; 404 `TrackNotFound`, 429
`RateLimitExceeded` and 503 `ServerOverloaded` honor `Retry-After`; some
rejections are plain text, so status and Content-Type are checked before
assuming JSON. Clients must send [`USER_AGENT`].

## API

- `USER_AGENT` — `HermesIsland/<crate version> (https://github.com/NousResearch/hermes-agent)`.
- `TrackQuery { title, artist, album, length_ms }` — a lookup.
- `get_path(&TrackQuery) -> Option<String>` — relative request path; fields
  trimmed, capped at 512 chars, form-urlencoded; duration rounded to nearest
  second and omitted outside 1..=3600.
- `Lookup` — `Synced { lrc, plain } | Plain(String) | Instrumental |
  NotFound | RetryAfter { seconds } | Failed { status }`.
- `parse_response(status, content_type, retry_after, body) -> Lookup` —
  classifies an HTTP response (1 MiB body cap checked first; 404 → `NotFound`;
  429/503 → `RetryAfter` with `Retry-After` delta-seconds clamped to 1..=3600,
  default 60).
- `cache_key(&TrackQuery) -> Option<String>` — xxh3-64 lowercase hex of the
  normalized, length-prefixed `(title, artist, album-or-"", duration-seconds-or-"")`
  tuple.

Semantics are documented on the items in `src/lib.rs`; the gate output and
mutation record live in `VERIFICATION.md`.

## Dependencies

serde (derive), serde_json, xxhash-rust (xxh3). No other crates.
