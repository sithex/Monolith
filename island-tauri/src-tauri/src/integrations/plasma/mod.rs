//! Plasma bridge components. None starts a worker or queries the desktop on import.
pub mod anchor;
#[cfg(target_os = "linux")]
pub mod companion;
#[cfg(target_os = "linux")]
pub mod notch_live;
#[cfg(target_os = "linux")]
pub mod panel_socket;
#[cfg(target_os = "linux")]
pub mod runtime;
#[cfg(target_os = "linux")]
pub mod status_file;
