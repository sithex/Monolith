//! Pure core behind SPEC §0.5 item (12) — the read-only "package updates"
//! badge. Arch family (CachyOS/Arch): the Tauri actor runs `checkupdates`
//! (pacman-contrib) and `paru -Qua`. Fedora: `dnf check-upgrade --json` and
//! a batched `rpm -q` for installed versions. [`select_backend`] picks one
//! from os-release. This crate parses exit code / stdout / stderr into an
//! update list and computes the badge state (counts, reboot hint,
//! staleness, error) and the check schedule. It never runs anything.
//!
//! Platform-free: no IO, no network, no threads, no clock, no `unsafe`,
//! and it never panics on any input. Wire types use `camelCase` through
//! serde and accept no aliases.

#![forbid(unsafe_code)]

mod argv;
mod backend;
mod dnf;
mod error;
mod model;
mod parse;
mod schedule;

pub use argv::{
    dnf_rpm_argv_allowed, rpm_query_argv, CHECKUPDATES_ARGV, DNF_CHECK_UPGRADE_ARGV,
    DNF_CHECK_UPGRADE_CACHED_ARGV, PARU_QUA_ARGV, RPM_BATCH, RPM_QUERY_PREFIX,
};
pub use backend::{select_backend, Backend, Unsupported};
pub use dnf::{
    dnf_updates, evr_cmp, is_reboot_pkg_dnf, normalize_evr, parse_dnf_check_upgrade,
    parse_rpm_installed, rpm_vercmp, DnfUpgrade, DNF_MAX_STDOUT_BYTES,
};
pub use error::UpdatesError;
pub use model::{is_kernel, Badge, Source, Update, STALE_AFTER_SECS};
pub use parse::{parse_checkupdates, parse_paru_qua, MAX_LINES, MAX_STDOUT_BYTES};
pub use schedule::{backoff_minutes, Schedule};
