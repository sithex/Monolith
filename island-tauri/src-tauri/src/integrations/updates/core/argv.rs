//! argv constants for the read-only update checks. Never `-S`/`-U`,
//! never `sudo`, never `--noconfirm` — these commands only query.

use crate::integrations::updates::core::dnf::valid_key;

/// `checkupdates` (pacman-contrib).
pub const CHECKUPDATES_ARGV: [&str; 1] = ["checkupdates"];

/// `paru -Qua` (AUR query, read-only).
pub const PARU_QUA_ARGV: [&str; 2] = ["paru", "-Qua"];

/// `dnf check-upgrade --json` (dnf5): read-only, runs as the user from the user metadata cache.
/// Never `--refresh`: dnf5-makecache.timer keeps the system cache fresh.
pub const DNF_CHECK_UPGRADE_ARGV: [&str; 4] = ["dnf", "-q", "check-upgrade", "--json"];

/// Same, cache only (`-C`): used on battery so a scheduled check never downloads metadata.
pub const DNF_CHECK_UPGRADE_CACHED_ARGV: [&str; 5] = ["dnf", "-q", "-C", "check-upgrade", "--json"];

/// `rpm -q` installed-version query; `name.arch` keys follow the `--`.
pub const RPM_QUERY_PREFIX: [&str; 5] = ["rpm", "-q", "--qf", "%{NAME}.%{ARCH}\t%{EVR}\n", "--"];

/// Keys per `rpm -q` call: far below ARG_MAX even at 128-byte names.
pub const RPM_BATCH: usize = 512;

/// `rpm -q` argv for `keys` (callers pass at most [`RPM_BATCH`] validated keys).
pub fn rpm_query_argv(keys: &[String]) -> Vec<String> {
    RPM_QUERY_PREFIX
        .iter()
        .map(|s| (*s).to_string())
        .chain(keys.iter().cloned())
        .collect()
}

/// The only `dnf` / `rpm` command lines the Controls runner may start: the two check-upgrade
/// shapes, and the rpm query prefix followed by 1..=[`RPM_BATCH`] valid `name.arch` keys.
/// Any other executable returns `true` here (its own allow-list applies).
pub fn dnf_rpm_argv_allowed(argv: &[String]) -> bool {
    let same =
        |shape: &[&str]| argv.len() == shape.len() && argv.iter().zip(shape).all(|(a, b)| a == b);
    match argv.first().map(String::as_str) {
        Some("dnf") => same(&DNF_CHECK_UPGRADE_ARGV) || same(&DNF_CHECK_UPGRADE_CACHED_ARGV),
        Some("rpm") => {
            let keys = argv.len().saturating_sub(RPM_QUERY_PREFIX.len());
            argv.len() > RPM_QUERY_PREFIX.len()
                && keys <= RPM_BATCH
                && same_prefix(argv, &RPM_QUERY_PREFIX)
                && argv[RPM_QUERY_PREFIX.len()..]
                    .iter()
                    .all(|key| valid_key(key))
        }
        _ => true,
    }
}

fn same_prefix(argv: &[String], prefix: &[&str]) -> bool {
    argv.len() >= prefix.len() && argv.iter().zip(prefix).all(|(a, b)| a == b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(argv: &[&str]) -> Vec<String> {
        argv.iter().map(|s| (*s).to_string()).collect()
    }

    /// Every argv this module can produce is a query: no mutating verb, no privilege, no refresh.
    #[test]
    fn update_argv_constants_contain_no_mutating_verb() {
        let mutating = [
            "upgrade",
            "update",
            "install",
            "remove",
            "erase",
            "reinstall",
            "downgrade",
            "distro-sync",
            "swap",
            "autoremove",
            "makecache",
            "--refresh",
            "-y",
            "--assumeyes",
            "sudo",
            "pkexec",
            "run0",
            "-i",
            "-U",
            "-e",
            "-F",
            "--install",
            "--upgrade",
            "--erase",
            "--freshen",
            "--noconfirm",
            "-S",
            "-Syu",
        ];
        let rpm = rpm_query_argv(&v(&["glibc.x86_64"]));
        for argv in [
            v(&CHECKUPDATES_ARGV),
            v(&PARU_QUA_ARGV),
            v(&DNF_CHECK_UPGRADE_ARGV),
            v(&DNF_CHECK_UPGRADE_CACHED_ARGV),
            rpm,
        ] {
            for arg in &argv {
                assert!(!mutating.contains(&arg.as_str()), "{argv:?} contains {arg}");
            }
        }
        assert_eq!(
            DNF_CHECK_UPGRADE_ARGV,
            ["dnf", "-q", "check-upgrade", "--json"]
        );
        assert_eq!(
            DNF_CHECK_UPGRADE_CACHED_ARGV,
            ["dnf", "-q", "-C", "check-upgrade", "--json"]
        );
        assert_eq!(
            RPM_QUERY_PREFIX,
            ["rpm", "-q", "--qf", "%{NAME}.%{ARCH}\t%{EVR}\n", "--"]
        );
    }

    #[test]
    fn dnf_rpm_allow_list_accepts_exact_shapes_only() {
        assert!(dnf_rpm_argv_allowed(&v(&DNF_CHECK_UPGRADE_ARGV)));
        assert!(dnf_rpm_argv_allowed(&v(&DNF_CHECK_UPGRADE_CACHED_ARGV)));
        assert!(dnf_rpm_argv_allowed(&rpm_query_argv(&v(&[
            "glibc.i686",
            "LibRaw.x86_64"
        ]))));
        let full: Vec<String> = (0..RPM_BATCH).map(|i| format!("p{i}.noarch")).collect();
        assert!(dnf_rpm_argv_allowed(&rpm_query_argv(&full)));
        // Not dnf/rpm: decided by the executable allow-list, not here.
        assert!(dnf_rpm_argv_allowed(&v(&["checkupdates"])));
        for bad in [
            v(&["dnf"]),
            v(&["dnf", "upgrade"]),
            v(&["dnf", "-q", "check-upgrade", "--json", "--refresh"]),
            v(&["dnf", "-q", "check-upgrade"]),
            v(&["dnf", "-y", "-q", "check-upgrade", "--json"]),
            v(&["dnf", "-q", "-C", "check-upgrade", "--json", "x"]),
            v(&RPM_QUERY_PREFIX),
            v(&["rpm", "-q", "glibc.x86_64"]),
            v(&[
                "rpm",
                "-q",
                "--qf",
                "%{NAME}.%{ARCH}\t%{EVR}\n",
                "glibc.x86_64",
            ]),
            rpm_query_argv(&v(&["-e"])),
            rpm_query_argv(&v(&["--erase"])),
            rpm_query_argv(&v(&["glibc"])),
            rpm_query_argv(&v(&["glibc.sparc"])),
            rpm_query_argv(&v(&["a b.x86_64"])),
            v(&[
                "rpm",
                "-Uvh",
                "--qf",
                "%{NAME}.%{ARCH}\t%{EVR}\n",
                "--",
                "x.noarch",
            ]),
        ] {
            assert!(!dnf_rpm_argv_allowed(&bad), "{bad:?}");
        }
        let mut over = full;
        over.push("extra.noarch".into());
        assert!(!dnf_rpm_argv_allowed(&rpm_query_argv(&over)));
    }
}
