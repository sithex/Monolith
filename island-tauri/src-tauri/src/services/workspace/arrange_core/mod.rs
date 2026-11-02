//! Pure core for SPEC §0.5 "Design direction" (App launcher + Launcher edit
//! mode bullets): the phone-style "jiggle" edit session for the Hermes Island
//! app grid. This crate decides the edit state machine — which tiles are
//! visible/hidden, where a moved tile lands, lock rules, cancel/commit
//! semantics, and how `reconcile` merges module-registry changes. The Tauri
//! side applies the resulting `ArrangeCommit` to the module registry; this
//! crate never imports it.
//!
//! Platform-free: no IO, no network, no threads, no clock, no `unsafe`, and
//! it never panics on any input. Wire types use camelCase through serde and
//! accept no aliases. Dependencies: serde (derive) + serde_json only.

#![forbid(unsafe_code)]

mod arrangement;
mod error;

pub use arrangement::{
    ArrangeCommit, Arrangement, EnabledChange, GridItem, MAX_ID_BYTES, MAX_ITEMS,
};
pub use error::ArrangeError;
