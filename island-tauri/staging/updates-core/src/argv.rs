//! argv constants for the read-only update checks. Never `-S`/`-U`,
//! never `sudo`, never `--noconfirm` — these commands only query.

/// `checkupdates` (pacman-contrib).
pub const CHECKUPDATES_ARGV: [&str; 1] = ["checkupdates"];

/// `paru -Qua` (AUR query, read-only).
pub const PARU_QUA_ARGV: [&str; 2] = ["paru", "-Qua"];
