//! Pure core behind SPEC §0.5 "More tiles/features Nick picked" — the
//! Controls system tiles for the Hermes Island widget:
//!
//! 1. **Keep awake** — session-bus `org.freedesktop.ScreenSaver`
//!    Inhibit/UnInhibit lifecycle modelled as a state machine
//!    ([`KeepAwake`]) with an injected clock.
//! 3. **Audio output switch** — parse `pactl -f json list sinks` and
//!    `pactl get-default-sink`, assemble an [`OutputTile`], build
//!    `pactl set-default-sink <name>` argv.
//! 4. **Network / Tailscale** — map NetworkManager D-Bus numeric
//!    properties to a [`NetworkTile`]; parse `tailscale status --json`
//!    into a [`TailscaleTile`].
//! 6. **Screenshot / colour picker** — KWin ARGB u32 → `#rrggbb` /
//!    CSS rgba; constant `spectacle -r` argv.
//! 7. **Hermes voice switcher** — parse `hermes-fish` output into a
//!    [`VoiceList`]; build `hermes-fish <name>` argv.
//!
//! The Tauri actor runs the commands and D-Bus calls; this crate only
//! models, parses and builds argv. It never runs anything.
//!
//! Platform-free: no IO, no network, no threads, no clock (time is always
//! an injected parameter), no `unsafe`, and it never panics on any input.
//! Wire types use `camelCase` through serde and accept no aliases.

#![forbid(unsafe_code)]

mod cmd;
mod color;
mod error;
mod keepawake;
mod model;
mod parse;
mod tiles;

pub use cmd::{screenshot_argv, set_default_argv, switch_argv};
pub use color::{argb_to_hex, argb_to_rgba_css};
pub use error::{CmdError, ParseError};
pub use keepawake::{
    Action, KeepAwake, KeepAwakeState, AUTO_OFF_1H, AUTO_OFF_2H, AUTO_OFF_30M, INHIBIT_APP,
};
pub use model::{
    BackendState, NetKind, NetworkTile, NmConnectivity, NmSnapshot, NmState, OutputTile, Peer,
    Sink, TailscaleTile, VoiceList,
};
pub use parse::{
    is_valid_voice_name, parse_current, parse_default_sink, parse_sinks, parse_sinks_report,
    parse_tailscale_status, parse_tailscale_status_report, parse_voice_list,
    parse_voice_list_report, MAX_SINKS_BYTES, MAX_TAILSCALE_BYTES, MAX_VOICE_BYTES,
};
pub use tiles::{network_tile, network_tile_report, output_tile};
