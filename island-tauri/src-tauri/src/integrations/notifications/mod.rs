//! Notification hub; the pure core is ported from the audited former staging crate (git history).
pub mod dbus;
pub mod hermes_ingest;
pub mod hub;
pub mod ingest;
#[cfg(target_os = "linux")]
pub mod linux_monitor;
#[cfg(not(target_os = "linux"))]
#[path = "monitor_unsupported.rs"]
pub mod linux_monitor;
pub mod store;
pub mod types;
pub use store::*;
pub use types::*;
