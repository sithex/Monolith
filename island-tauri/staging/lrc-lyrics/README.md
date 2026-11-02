# lrc-lyrics

Pure Rust LRC synced-lyrics parser (LRCLIB `syncedLyrics` field) for the
Hermes Island music widget (SPEC §8 Phase 6 "Music extras").

No IO, no network, no `unsafe`, no external dependencies, never panics.

## API

- `parse(input: &str) -> Result<Lyrics, ParseError>`
- `Lyrics { lines, offset_ms, metadata }`, `Line { start_ms, text }`,
  `Metadata { title, artist, album, length_ms }`
- `Lyrics::active_index(position_ms) -> Option<usize>`
- `Lyrics::window(position_ms, before, after) -> &[Line]`

Semantics are documented on the items in `src/lib.rs`; the gate output and
mutation record live in `VERIFICATION.md`.
