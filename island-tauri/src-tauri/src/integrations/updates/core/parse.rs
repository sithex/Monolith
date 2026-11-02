//! Parsing of `checkupdates` / `paru -Qua` stdout.
//!
//! Verified on nick-pc (controller): `checkupdates` (pacman-contrib
//! 1.13.1) exit codes 0 = updates (stdout lines), 1 = unknown failure,
//! 2 = no updates; each line is `<pkgname> <oldver> -> <newver>`,
//! optionally suffixed ` [ignored]`. `paru -Qua` lists AUR updates in the
//! same form; exit 0 with lines = updates, exit 1 with empty stdout +
//! stderr = none, exit 1 with stderr text or any other code = failure.
//!
//! Parsing is total: oversize stdout (> [`MAX_STDOUT_BYTES`]) or too many
//! lines (> [`MAX_LINES`]) give a typed error; a malformed non-blank line
//! is skipped and counted — if every non-blank line is malformed, that is
//! an [`UpdatesError::AllMalformed`] error, never a fabricated "0
//! updates".

use crate::integrations::updates::core::error::UpdatesError;
use crate::integrations::updates::core::model::{Source, Update};

/// Cap on accepted stdout payload size (1 MiB, strictly greater-than).
pub const MAX_STDOUT_BYTES: usize = 1024 * 1024;

/// Cap on accepted non-blank lines.
pub const MAX_LINES: usize = 10_000;

/// Parse `checkupdates` output. Exit 0 = updates, 2 = none (returns an
/// empty vec), anything else = failure. `stderr` is not consulted.
pub fn parse_checkupdates(
    exit: i32,
    stdout: &str,
    _stderr: &str,
) -> Result<Vec<Update>, UpdatesError> {
    match exit {
        0 => parse_lines(stdout, Source::Repo),
        2 => Ok(Vec::new()),
        _ => Err(UpdatesError::CommandFailed),
    }
}

/// Parse `paru -Qua` output. Exit 0 = updates. Exit 1 with empty stdout
/// and stderr = none; exit 1 with stderr text = failure; exit 1 with only
/// stdout non-empty is treated as updates (the lines are in the same
/// form). Any other exit code = failure.
pub fn parse_paru_qua(exit: i32, stdout: &str, stderr: &str) -> Result<Vec<Update>, UpdatesError> {
    match exit {
        0 => parse_lines(stdout, Source::Aur),
        1 => {
            if stdout.is_empty() && stderr.is_empty() {
                Ok(Vec::new())
            } else if !stderr.is_empty() {
                Err(UpdatesError::CommandFailed)
            } else {
                parse_lines(stdout, Source::Aur)
            }
        }
        _ => Err(UpdatesError::CommandFailed),
    }
}

fn parse_lines(stdout: &str, source: Source) -> Result<Vec<Update>, UpdatesError> {
    if stdout.len() > MAX_STDOUT_BYTES {
        return Err(UpdatesError::StdoutTooLarge);
    }
    let mut parsed = Vec::new();
    let mut non_blank: usize = 0;
    for line in stdout.split('\n') {
        let line = line.trim_end(); // tolerate \r / trailing whitespace
        if line.is_empty() {
            continue;
        }
        non_blank += 1;
        if non_blank > MAX_LINES {
            return Err(UpdatesError::TooManyLines);
        }
        if let Some(update) = parse_line(line, source) {
            parsed.push(update);
        }
    }
    if non_blank > 0 && parsed.is_empty() {
        return Err(UpdatesError::AllMalformed);
    }
    Ok(parsed)
}

/// Parse one line: `<name> <old> -> <new> [ignored]`. Returns `None`
/// (malformed) on any violation. Blank lines are handled by the caller.
fn parse_line(line: &str, source: Source) -> Option<Update> {
    let mut tokens = line.split_whitespace();
    let name = tokens.next()?;
    let old_version = tokens.next()?;
    let arrow = tokens.next()?;
    let new_version = tokens.next()?;
    let ignored = match tokens.next() {
        None => false,
        Some("[ignored]") => true,
        Some(_) => return None,
    };
    if tokens.next().is_some() || arrow != "->" {
        return None;
    }
    if !valid_pkg_name(name) || !valid_version(old_version) || !valid_version(new_version) {
        return None;
    }
    Some(Update {
        name: name.to_string(),
        old_version: old_version.to_string(),
        new_version: new_version.to_string(),
        ignored,
        source,
    })
}

/// Package names: `[a-z0-9@._+-]`, 1..=128 bytes, not starting with `-`
/// or `.`.
fn valid_pkg_name(name: &str) -> bool {
    let len = name.len();
    if len == 0 || len > 128 {
        return false;
    }
    let first = name.as_bytes()[0];
    if first == b'-' || first == b'.' {
        return false;
    }
    name.chars().all(|c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '@' | '.' | '_' | '+' | '-')
    })
}

/// Versions: non-empty, ≤ 128 bytes, no whitespace (guaranteed by
/// tokenization), characters from `[0-9A-Za-z.:+~_-]`. The stricter
/// `[epoch:]pkgver-pkgrel` shape is intentionally not enforced (see
/// VERIFICATION.md Decisions).
fn valid_version(ver: &str) -> bool {
    let len = ver.len();
    if len == 0 || len > 128 {
        return false;
    }
    ver.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '+' | '~' | '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_pkg_name_edges() {
        assert!(valid_pkg_name("linux"));
        assert!(valid_pkg_name("linux-cachyos-headers"));
        assert!(valid_pkg_name("lib32-systemd"));
        assert!(valid_pkg_name("a@b"));
        assert!(valid_pkg_name("a_b.c+d-e"));
        assert!(!valid_pkg_name(""));
        assert!(!valid_pkg_name("-foo"));
        assert!(!valid_pkg_name(".foo"));
        assert!(!valid_pkg_name("Foo"));
        assert!(!valid_pkg_name(&"a".repeat(129)));
        assert!(valid_pkg_name(&"a".repeat(128)));
    }

    #[test]
    fn valid_version_edges() {
        assert!(valid_version("1.0"));
        assert!(valid_version("1:2.4-1"));
        assert!(valid_version("1.0rc1"));
        assert!(valid_version("1.0~rc1"));
        assert!(!valid_version(""));
        assert!(!valid_version("1 2"));
        assert!(!valid_version("1=2"));
        assert!(!valid_version(&"a".repeat(129)));
        assert!(valid_version(&"a".repeat(128)));
    }

    #[test]
    fn parse_line_shapes() {
        let u = parse_line("pkg 1-1 -> 2-1", Source::Repo).unwrap();
        assert_eq!(u.name, "pkg");
        assert_eq!(u.old_version, "1-1");
        assert_eq!(u.new_version, "2-1");
        assert!(!u.ignored);
        assert_eq!(u.source, Source::Repo);

        let ui = parse_line("pkg 1-1 -> 2-1 [ignored]", Source::Repo).unwrap();
        assert!(ui.ignored);
        assert_eq!(ui.name, "pkg");

        assert!(parse_line("pkg 1-1 -> 2-1 [ignored] extra", Source::Aur).is_none());
        assert!(parse_line("pkg 1-1 2-1", Source::Aur).is_none());
        assert!(parse_line("pkg 1-1 -> 2-1 -> 3-1", Source::Aur).is_none());
        assert!(parse_line("-pkg 1-1 -> 2-1", Source::Aur).is_none());
        assert!(parse_line("pkg 1-1 -> 2=1", Source::Aur).is_none());
        assert!(parse_line("", Source::Aur).is_none());
    }
}
