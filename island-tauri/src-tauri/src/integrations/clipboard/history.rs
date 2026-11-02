//! Pure clipboard history store for Hermes Island (SPEC §3.3).
//!
//! This crate is deliberately platform-free: it never touches the system
//! clipboard, never opens sockets, never performs filesystem IO, and never
//! accesses the network. It is a self-contained data structure that the
//! platform integration layer (`integrations/clipboard/` in the Tauri app)
//! will drive.
//!
//! # Safety
//! The crate is `#![forbid(unsafe_code)]` and denies `unwrap`/`expect` in
//! non-test code.

#![forbid(unsafe_code)]

pub mod config;
pub mod error;
pub mod model;
pub mod persist;
pub mod preview;
pub mod store;

pub use config::HistoryConfig;
pub use error::Error;
pub use model::{
    ClipContent, ClipInput, ClipKind, ClipPayload, ClipSummary, OfferOutcome, SkipReason,
};
pub use store::HistoryStore;
