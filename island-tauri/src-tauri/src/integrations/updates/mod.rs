//! Phase 6b2 updates (ported core; service adapter added separately).
pub mod core;
pub mod install;
mod service;
pub use service::UpdatesService;
