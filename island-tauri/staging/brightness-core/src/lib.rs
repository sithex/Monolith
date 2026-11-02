//! Pure core behind SPEC §0.5 "Design direction" (Apps bullet) and §8
//! "Phase 6b" — the Controls app's brightness slider for the Hermes
//! Island widget. The Tauri actor talks to KDE PowerDevil over D-Bus
//! (zbus) and passes DECODED values here; this crate keeps the display
//! model, converts percent <-> raw values, coalesces slider drags and
//! builds the method-call requests. It never does IO.
//!
//! Platform-free: no IO, no network, no threads, no clock, no `unsafe`,
//! and it never panics on any input. Wire types use `camelCase` through
//! serde and accept no aliases. All time is an injected `now_ms: u64`.
//!
//! Upstream facts (org.kde.ScreenBrightness + .Display, from PowerDevil's
//! daemon/dbus XML, confirmed by introspecting the Plasma 6 session bus)
//! are used verbatim as test fixtures; the fixtures never touch D-Bus.

#![forbid(unsafe_code)]

mod cmd;
mod error;
mod model;
mod name;
mod percent;

pub use cmd::{step_request, Step, StepRequest};
pub use error::{DragError, NameError, ValueError};
pub use model::{
    Applied, BrightnessModel, DisplayView, Event, SetRequest, MAX_DISPLAYS, SETTLE_MS, THROTTLE_MS,
};
pub use name::{display_path, validate_display_name, MAX_DISPLAY_NAME_BYTES};
pub use percent::{percent, raw_for_percent, Display, MAX_PERCENT};

/// `source_client_context` passed through unchanged into BrightnessChanged.
pub const CONTEXT: &str = "hermes-island";

/// flag bit 0x1: suppress the OSD indicator.
pub const SUPPRESS_INDICATOR: u32 = 0x1;
