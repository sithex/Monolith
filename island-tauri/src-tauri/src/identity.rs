//! Frozen window identity contract (SPEC §2.3 step 1).
//!
//! PID + title + `WM_CLASS` are shared by the GUI tests and status
//! tooling. The values below are not guesses: they were read back from the real X11 window of
//! this binary (`_NET_WM_PID`, `_NET_WM_NAME`, `WM_CLASS`) under Xvfb + xfwm4 on 2026-10-29
//! (see docs/history/VERIFICATION.md) and `tests/xvfb_shape.rs::assert_identity` fails if the live window ever
//! disagrees. Change them only together with every consumer.
//!
//! * `_NET_WM_PID`  = the island process id (no re-exec, so `std::process::id()`).
//! * `_NET_WM_NAME` = `TITLE_*` (UTF-8, em dash U+2014), selected by `--overlay`.
//! * `WM_CLASS`     = (`WM_CLASS_INSTANCE`, `WM_CLASS_CLASS`). GTK derives both from
//!   `glib::set_prgname(PRGNAME)`: instance = prgname, class = prgname with the first letter
//!   upper-cased. `tauri.conf.json` `enableGTKAppId` (app id `local.hermes.island`) does NOT
//!   influence them, which is why prgname must be set before the event loop starts.

pub const PRGNAME: &str = "hermes-island";
pub const APPLICATION_NAME: &str = "Hermes Island";
pub const TITLE_OVERLAY: &str = "Hermes Island — panel overlay";
pub const TITLE_PREVIEW: &str = "Hermes Island — isolated preview";
pub const WM_CLASS_INSTANCE: &str = "hermes-island";
pub const WM_CLASS_CLASS: &str = "Hermes-island";
/// Phase 2.5 real-panel decision: production is NOTIFICATION, still keep-above.
pub const WINDOW_TYPE_ATOM: &str = "_NET_WM_WINDOW_TYPE_NOTIFICATION";
