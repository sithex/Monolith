//! Pure core behind SPEC §0.5 "Design direction" (Side buttons bullet) for
//! the Phase 7 side-button Layout editor of the Hermes Island widget. This
//! crate owns the persisted layout model and the geometry math the
//! shape/input-region owner will use (SPEC §2.4, single input-region owner);
//! it renders nothing.
//!
//! Platform-free: no IO, no network, no threads, no clock, no `unsafe`, and
//! it never panics on input reached through its API. Wire types use camelCase
//! through serde and accept no aliases. All geometry is logical px as `f64`
//! and must be finite.

#![forbid(unsafe_code)]

mod error;
mod geometry;
mod model;
mod persistence;

pub use error::LayoutError;
pub use geometry::Rect;
pub use model::{
    ButtonAction, Layout, LoadReport, Side, SideButton, SizePreset, CUSTOM_MAX_MAX_HEIGHT,
    CUSTOM_MAX_WIDTH, CUSTOM_MIN_MAX_HEIGHT, CUSTOM_MIN_WIDTH, DEFAULT_BUTTON_DIAMETER,
    DEFAULT_GAP, MAX_BUTTON_DIAMETER, MAX_GAP, MAX_ID_LEN, MAX_LABEL_CHARS, MAX_SLOTS_BOTTOM,
    MAX_SLOTS_LEFT_RIGHT, MIN_BUTTON_DIAMETER, MIN_GAP, MIN_ID_LEN, PRESET_COMPACT_MAX_HEIGHT,
    PRESET_COMPACT_WIDTH, PRESET_SPACIOUS_MAX_HEIGHT, PRESET_SPACIOUS_WIDTH, VERSION,
};
