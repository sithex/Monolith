//! Pure core behind SPEC §0.5 "Design direction" (Apps bullet) and §8
//! "Phase 6b" — the Controls + Volume mixer app for the Hermes Island
//! widget. The Tauri actor runs `pactl` (PipeWire's pulse compatibility)
//! and passes its TEXT output here; this crate parses it into a mixer
//! model and builds the argv for volume/mute commands. It never runs
//! anything.
//!
//! Platform-free: no IO, no network, no threads, no clock, no `unsafe`,
//! and it never panics on any input. Wire types use `camelCase` through
//! serde and accept no aliases.

#![forbid(unsafe_code)]

mod cmd;
mod error;
mod events;
mod model;
mod parse;

pub use cmd::{
    move_stream, mute_sink, mute_stream, set_app_volume, set_default_sink, set_sink_volume,
    set_stream_volume, MuteOp, MAX_PERCENT, MAX_SINK_NAME_BYTES,
};
pub use error::{CmdError, ParseError};
pub use events::{needs_refresh, parse_event, PaEvent, PaFacility, PaKind, RefreshScope};
pub use model::{AppGroup, MixerModel, Sink, Stream};
pub use parse::{
    parse_default_sink, parse_sink_inputs, parse_sink_inputs_report, parse_sinks,
    parse_sinks_report, MAX_INPUT_BYTES,
};
