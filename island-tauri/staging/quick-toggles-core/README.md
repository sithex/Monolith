# quick-toggles-core

Pure core behind SPEC §0.5 "Design direction" (bullet "Quick toggles") —
the Controls quick toggles for the Hermes Island: **power profile**
(power-profiles-daemon), **mic mute** (PipeWire's pulse server via
`pactl`) and **Do Not Disturb**.

- No IO, no network, no threads, no clock, no process spawning, no
  `unsafe`; never panics on any input.
- Wire types are `camelCase` via serde; no `#[serde(alias)]`.
- Dependencies: serde (derive) + serde_json only.
- The Tauri actor owns D-Bus/`pactl` (and runs pactl with LC_ALL=C);
  this crate only builds exact argv and parses text.

Contents: `src/power.rs` (power tile + next-profile cycling), `src/mic.rs`
(pactl argv + `Mute: yes|no` parsing), `src/toggle.rs` (generic optimistic
toggle state machine), `src/model.rs` (wire types), `src/error.rs`.

See VERIFICATION.md for gates, mutation checks and decisions.
