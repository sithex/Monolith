# mpris-core

Pure Rust core behind the "Now playing" row (SPEC §3.5) and the Phase 6
lyrics timing for the Hermes Island music widget. MPRIS D-Bus values arrive
already converted to `serde_json::Value`; times are injected `u64` monotonic
milliseconds; nothing in this crate reads a clock.

No IO, no network, no threads, no `unsafe`, never panics on any input.

## API

- `PlaybackStatus`, `parse_status(&str) -> Option<PlaybackStatus>` — exact
  MPRIS strings `Playing` / `Paused` / `Stopped`.
- `Track`, `parse_metadata(&Map<String, Value>) -> Option<Track>` — `trackId`,
  `title` (required, never fabricated), `artists` (array or single string),
  `album`, `artUrl`, `lengthMs` (from `mpris:length` microseconds).
- `PlayerState`, `position_at(&PlayerState, now_ms) -> u64` — interpolation
  with rate, clamping ( length, no underflow.
- `Players` — `upsert_status`, `set_metadata`, `seeked`, `set_position`,
  `set_rate`, `remove`, `preferred`; max 32 players, oldest-inactive evicted.

Serde wires `Track`/`PlayerState` fields as camelCase (e.g. `trackId`,
`artUrl`, `lengthMs`, `positionAtMs`); no aliases are accepted.

Semantics are documented on the items in `src/lib.rs`; the gate output and
mutation record live in `VERIFICATION.md`.
