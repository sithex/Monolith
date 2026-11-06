//! Notification hub; the pure core is ported from the audited former staging crate (git history).
pub mod dbus;
#[cfg(target_os = "linux")]
pub mod effects;
pub mod hermes_ingest;
pub mod hub;
pub mod ingest;
#[cfg(target_os = "linux")]
pub mod linux_monitor;
#[cfg(not(target_os = "linux"))]
#[path = "monitor_unsupported.rs"]
pub mod linux_monitor;
pub mod popup_commands;
pub mod popups;
#[cfg(all(test, target_os = "linux"))]
mod protocol_tests;
pub mod store;
#[cfg(target_os = "linux")]
pub mod takeover;
pub mod types;
pub use store::*;
pub use types::*;
