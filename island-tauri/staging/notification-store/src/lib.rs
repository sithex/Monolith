//! Platform-free notification core. No clocks, IO, transport, or event emission.
#![forbid(unsafe_code)]

pub mod dbus;
pub mod ingest;
mod store;
mod types;

pub use store::*;
pub use types::*;
