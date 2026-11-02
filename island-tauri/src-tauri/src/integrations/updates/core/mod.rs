//! Pure core behind SPEC §0.5 item (12) — the read-only "package updates"
//! badge for CachyOS/Arch. The Tauri actor runs `checkupdates`
//! (pacman-contrib) and `paru -Qua`; this crate parses their exit code /
//! stdout / stderr into an update list and computes the badge state
//! (counts, reboot hint, staleness, error) and the check schedule. It
//! never runs anything.
//!
//! Platform-free: no IO, no network, no threads, no clock, no `unsafe`,
//! and it never panics on any input. Wire types use `camelCase` through
//! serde and accept no aliases.

#![forbid(unsafe_code)]

mod argv;
mod error;
mod model;
mod parse;
mod schedule;

pub use argv::{CHECKUPDATES_ARGV, PARU_QUA_ARGV};
pub use error::UpdatesError;
pub use model::{is_kernel, Badge, Source, Update, STALE_AFTER_SECS};
pub use parse::{parse_checkupdates, parse_paru_qua, MAX_LINES, MAX_STDOUT_BYTES};
pub use schedule::{backoff_minutes, Schedule};
