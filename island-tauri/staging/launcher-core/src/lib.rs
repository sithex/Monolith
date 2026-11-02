//! Pure core behind SPEC §0.5 "Design direction" (Apps bullet) — the Phase 7
//! app-grid launcher for the Hermes Island widget: parses `.desktop` file
//! text into launcher entries and turns an entry's `Exec` line into an argv.
//! The Tauri side reads `.desktop` files from the XDG application dirs and
//! spawns the resulting processes; this crate only decides **what** an entry
//! is and **how** to invoke it.
//!
//! Implements the freedesktop.org Desktop Entry Specification (latest):
//! §3–§6 summarised (groups, keys, values, lists, localized keys, booleans)
//! and §7 "The Exec key" copied exactly (quoting, escaping, field codes).
//!
//! Platform-free: no IO, no network, no threads, no clock, no process
//! spawning, no `unsafe`, and it never panics on any input. Wire types use
//! camelCase through serde and accept no aliases.

#![forbid(unsafe_code)]

mod catalog;
mod error;
mod exec;
mod model;
mod parse;

pub use catalog::Catalog;
pub use error::{ExecError, ParseError};
pub use exec::{build_argv, ExecContext};
pub use model::{visible_in, AppEntry, Warning};
pub use parse::parse_desktop_entry;
