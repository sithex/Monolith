//! Pure core behind SPEC §0.5 "Design direction" (bullet "Quick toggles"):
//! the Controls quick toggles for the Hermes Island — power profile
//! (power-profiles-daemon), mic mute (PipeWire's pulse server via
//! `pactl`) and Do Not Disturb. The Tauri actor owns the D-Bus and
//! process machinery; this crate turns zbus-derived values into tile
//! states, builds exact `pactl` argv, parses `pactl` text output and
//! runs the optimistic toggle state machine. It never runs anything.
//!
//! Platform-free: no IO, no network, no threads, no clock, no `unsafe`,
//! and it never panics on any input. Wire types use `camelCase` through
//! serde and accept no aliases.

#![forbid(unsafe_code)]

mod error;
mod mic;
mod model;
mod power;
mod toggle;

pub use error::ParseError;
pub use mic::{
    mic_get_argv, mic_set_argv, parse_mic_mute, MicOp, DEFAULT_SOURCE_ARG, MAX_MUTE_OUTPUT_BYTES,
};
pub use model::{DndTile, PowerTile, PpdHold, PpdProfile, PpdSnapshot, Profile};
pub use power::{dnd_tile, next_profile, power_tile, UNAVAILABLE_REASON};
pub use toggle::{Pending, ToggleState, PENDING_TIMEOUT_MS};
