//! Pure core behind SPEC §0.5 "Design direction" (Apps bullet) and §8
//! "Phase 6b" — the Scratchpad app for the Hermes Island widget: plain-text
//! notes stored in the data dir with debounced, atomic saves. This crate
//! decides **what** to write (notes, titles, ordering, limits) and **when**
//! (the debounced save scheduler); the Tauri actor performs the actual file
//! IO against the data dir.
//!
//! Platform-free: no IO, no network, no threads, no clock, no `unsafe`, and
//! it never panics on any input. All times are injected `u64` milliseconds on
//! a monotonic clock; nothing in this crate reads a clock. Wire types use
//! camelCase through serde and accept no aliases.

#![forbid(unsafe_code)]

mod error;
mod note;
mod save_scheduler;
mod scratchpad;

pub use error::ScratchpadError;
pub use note::{Note, NoteSummary, MAX_BODY_BYTES, MAX_ID_LEN, MIN_ID_LEN, TITLE_MAX_CHARS};
pub use save_scheduler::SaveScheduler;
pub use scratchpad::{LoadReport, Scratchpad, MAX_NOTES};
