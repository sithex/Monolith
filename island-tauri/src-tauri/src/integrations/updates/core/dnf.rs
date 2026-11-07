//! Fedora backend: parsing of `dnf -q check-upgrade --json` (dnf5) and of the batched
//! `rpm -q --qf '%{NAME}.%{ARCH}\t%{EVR}\n' -- <name.arch>…` installed-version query.
//!
//! Verified against real Fedora 45 output (tests/fixtures/updates/PROVENANCE.md): exit 0 with
//! `{"upgrades":[{name,arch,evr,repository}]}` or `{}`; exit 1 (stdout empty) on a metadata
//! failure. dnf prints the EVR without a zero epoch, rpm `%{EVR}` prints `0:` when the package
//! declares `Epoch: 0`, and both print a real epoch, so [`normalize_evr`] drops only `0:`.
//! `rpm -q` puts "package X is not installed" on stdout and exits with the number of misses.
//!
//! Only the `upgrades` section is read: dnf5 builds its "Obsoleting packages" section as a
//! subset of the upgrades query, so reading it too would double count. Unknown keys and fields
//! are ignored; an entry without a valid `name`, `arch` or `evr` is skipped and counted, and a
//! non-empty list where every entry is invalid is an error, never "0 updates".

use crate::integrations::updates::core::error::UpdatesError;
use crate::integrations::updates::core::model::{Source, Update};
use crate::integrations::updates::core::parse::MAX_LINES;
use std::{cmp::Ordering, collections::HashMap};

/// dnf's pretty JSON is ~120 bytes per entry; 4 MiB holds well over [`MAX_LINES`] entries.
pub const DNF_MAX_STDOUT_BYTES: usize = 4 * 1024 * 1024;

/// Architectures Fedora ships packages for (and `noarch`).
const ARCHES: [&str; 7] = [
    "x86_64", "i686", "noarch", "aarch64", "ppc64le", "s390x", "riscv64",
];

/// One upgrade candidate from `dnf check-upgrade`. `evr` is normalised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnfUpgrade {
    pub name: String,
    pub arch: String,
    pub evr: String,
}

impl DnfUpgrade {
    /// `name.arch`: the identity of an installed package (multilib installs one per arch).
    pub fn key(&self) -> String {
        format!("{}.{}", self.name, self.arch)
    }
}

/// Parse `dnf -q check-upgrade --json`. Exit 0 = JSON (with or without updates); exit 1 =
/// repository/metadata failure; any other exit = failure. Never `Ok(empty)` unless dnf exited 0
/// and printed a JSON object without upgrades.
pub fn parse_dnf_check_upgrade(
    exit: i32,
    stdout: &str,
    _stderr: &str,
) -> Result<Vec<DnfUpgrade>, UpdatesError> {
    if exit != 0 {
        return Err(UpdatesError::CommandFailed);
    }
    if stdout.len() > DNF_MAX_STDOUT_BYTES {
        return Err(UpdatesError::StdoutTooLarge);
    }
    let value: serde_json::Value =
        serde_json::from_str(stdout).map_err(|_| UpdatesError::InvalidJson)?;
    let object = value.as_object().ok_or(UpdatesError::InvalidJson)?;
    let Some(upgrades) = object.get("upgrades") else {
        return Ok(Vec::new());
    };
    let entries = upgrades.as_array().ok_or(UpdatesError::InvalidJson)?;
    if entries.len() > MAX_LINES {
        return Err(UpdatesError::TooManyLines);
    }
    let mut parsed: Vec<DnfUpgrade> = Vec::new();
    for entry in entries {
        let Some(upgrade) = parse_entry(entry) else {
            continue;
        };
        // dnf lists only the latest EVR per name.arch; a repeat is ignored, not double counted.
        if !parsed
            .iter()
            .any(|p| p.name == upgrade.name && p.arch == upgrade.arch)
        {
            parsed.push(upgrade);
        }
    }
    if !entries.is_empty() && parsed.is_empty() {
        return Err(UpdatesError::AllMalformed);
    }
    Ok(parsed)
}

fn parse_entry(entry: &serde_json::Value) -> Option<DnfUpgrade> {
    let field = |key: &str| entry.get(key).and_then(serde_json::Value::as_str);
    let (name, arch, evr) = (field("name")?, field("arch")?, field("evr")?);
    (valid_name(name) && valid_arch(arch) && valid_evr(evr)).then(|| DnfUpgrade {
        name: name.to_string(),
        arch: arch.to_string(),
        evr: normalize_evr(evr).to_string(),
    })
}

/// Installed EVRs by `name.arch` from `rpm -q --qf '%{NAME}.%{ARCH}\t%{EVR}\n'` stdout. The exit
/// code is not consulted (it counts missing packages). Lines without a tab ("package X is not
/// installed") and invalid lines are skipped. When several versions of one key are installed
/// (install-only kernels), the highest EVR is the one being upgraded from.
pub fn parse_rpm_installed(stdout: &str) -> HashMap<String, String> {
    let mut installed: HashMap<String, String> = HashMap::new();
    if stdout.len() > DNF_MAX_STDOUT_BYTES {
        return installed;
    }
    for line in stdout.lines().take(MAX_LINES * 4) {
        let Some((key, evr)) = line.trim_end().split_once('\t') else {
            continue;
        };
        if !valid_key(key) || !valid_evr(evr) {
            continue;
        }
        let evr = normalize_evr(evr);
        match installed.get_mut(key) {
            Some(current) if evr_cmp(evr, current) != Ordering::Greater => {}
            Some(current) => *current = evr.to_string(),
            None => {
                installed.insert(key.to_string(), evr.to_string());
            }
        }
    }
    installed
}

/// Join upgrades with installed versions. A package rpm doesn't know (an obsoleting package with
/// a new name, a race with a transaction) keeps an empty old version and is still listed.
pub fn dnf_updates(upgrades: Vec<DnfUpgrade>, installed: &HashMap<String, String>) -> Vec<Update> {
    upgrades
        .into_iter()
        .map(|u| Update {
            old_version: installed.get(&u.key()).cloned().unwrap_or_default(),
            name: u.name,
            new_version: u.evr,
            ignored: false,
            source: Source::Dnf,
            arch: Some(u.arch),
        })
        .collect()
}

/// Drops a zero epoch (`0:1.0-1` → `1.0-1`, also `00:`); keeps a real one (`2:9.2-1`).
pub fn normalize_evr(evr: &str) -> &str {
    match evr.split_once(':') {
        Some((epoch, rest))
            if !epoch.is_empty() && epoch.bytes().all(|b| b == b'0') && !rest.is_empty() =>
        {
            rest
        }
        _ => evr,
    }
}

/// RPM package names: `[A-Za-z0-9._+-]`, 1..=128 bytes, not starting with `-` or `.`
/// (Fedora has `LibRaw`, `NetworkManager`, `libstdc++`).
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with(['-', '.'])
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

pub fn valid_arch(arch: &str) -> bool {
    ARCHES.contains(&arch)
}

/// `[epoch:]version-release` characters, incl. `^` (snapshot) and `~` (pre-release):
/// `[0-9A-Za-z.:+~^_-]`, 1..=128 bytes, not starting with `-`.
pub fn valid_evr(evr: &str) -> bool {
    !evr.is_empty()
        && evr.len() <= 128
        && !evr.starts_with('-')
        && evr.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b':' | b'+' | b'~' | b'^' | b'_' | b'-')
        })
}

/// `name.arch` with a valid name and a known arch (arch never contains `.`).
pub fn valid_key(key: &str) -> bool {
    key.rsplit_once('.')
        .is_some_and(|(name, arch)| valid_name(name) && valid_arch(arch))
}

/// Compare two EVRs the way rpm orders them: epoch (missing = 0), then version, then release,
/// each with [`rpm_vercmp`]. Written from rpm's documented ordering rules; no rpm code is used.
pub fn evr_cmp(a: &str, b: &str) -> Ordering {
    let (ea, va, ra) = split_evr(a);
    let (eb, vb, rb) = split_evr(b);
    numeric_cmp(ea, eb)
        .then_with(|| rpm_vercmp(va, vb))
        .then_with(|| rpm_vercmp(ra, rb))
}

fn split_evr(evr: &str) -> (&str, &str, &str) {
    let (epoch, vr) = match evr.split_once(':') {
        Some((e, rest)) if e.bytes().all(|b| b.is_ascii_digit()) => (e, rest),
        _ => ("0", evr),
    };
    let (version, release) = vr.rsplit_once('-').unwrap_or((vr, ""));
    (epoch, version, release)
}

/// Compares digit strings of any length without overflow.
fn numeric_cmp(a: &str, b: &str) -> Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// rpm version-segment ordering: alphanumeric runs are compared pairwise (numbers numerically
/// and newer than letters, letters by byte order); other separators only split runs; `~` sorts
/// before anything, even the end of the string; `^` sorts after the end of the string but before
/// any further segment; when all compared runs tie, the string with runs left over is newer.
pub fn rpm_vercmp(a: &str, b: &str) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    let separator = |c: u8| !c.is_ascii_alphanumeric() && c != b'~' && c != b'^';
    loop {
        while a.get(i).is_some_and(|&c| separator(c)) {
            i += 1;
        }
        while b.get(j).is_some_and(|&c| separator(c)) {
            j += 1;
        }
        let (x, y) = (a.get(i).copied(), b.get(j).copied());
        if x == Some(b'~') || y == Some(b'~') {
            if x != Some(b'~') {
                return Ordering::Greater;
            }
            if y != Some(b'~') {
                return Ordering::Less;
            }
            i += 1;
            j += 1;
            continue;
        }
        if x == Some(b'^') || y == Some(b'^') {
            if x.is_none() {
                return Ordering::Less;
            }
            if y.is_none() {
                return Ordering::Greater;
            }
            if x != Some(b'^') {
                return Ordering::Greater;
            }
            if y != Some(b'^') {
                return Ordering::Less;
            }
            i += 1;
            j += 1;
            continue;
        }
        let (Some(x), Some(_)) = (x, y) else {
            break;
        };
        let numeric = x.is_ascii_digit();
        let run = |s: &[u8], start: usize| {
            let mut end = start;
            while s.get(end).is_some_and(|c| {
                if numeric {
                    c.is_ascii_digit()
                } else {
                    c.is_ascii_alphabetic()
                }
            }) {
                end += 1;
            }
            end
        };
        let (end_a, end_b) = (run(a, i), run(b, j));
        if end_b == j {
            // Different segment types: a number is newer than letters.
            return if numeric {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }
        // Both runs are ASCII by construction.
        let seg_a = std::str::from_utf8(&a[i..end_a]).unwrap_or_default();
        let seg_b = std::str::from_utf8(&b[j..end_b]).unwrap_or_default();
        let order = if numeric {
            numeric_cmp(seg_a, seg_b)
        } else {
            seg_a.cmp(seg_b)
        };
        if order != Ordering::Equal {
            return order;
        }
        i = end_a;
        j = end_b;
    }
    match (i >= a.len(), j >= b.len()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        _ => Ordering::Greater,
    }
}

/// Reboot-needing Fedora packages: the kernel and its subpackages, graphics drivers (Mesa,
/// NVIDIA incl. `akmod-nvidia`, `kmod-nvidia-*`, `xorg-x11-drv-nvidia*`), and the core
/// userspace a running session keeps mapped (`glibc`, `systemd`, `dbus-broker`, firmware).
pub fn is_reboot_pkg_dnf(name: &str) -> bool {
    matches!(
        name,
        "kernel" | "glibc" | "systemd" | "linux-firmware" | "dbus-broker"
    ) || name.starts_with("kernel-")
        || name.starts_with("mesa-")
        || name.contains("nvidia")
}

#[cfg(test)]
mod tests {
    use super::*;

    const UPGRADES: &str =
        include_str!("../../../../tests/fixtures/updates/dnf-check-upgrade-27.json");
    const NONE: &str =
        include_str!("../../../../tests/fixtures/updates/dnf-check-upgrade-none.json");
    const OFFLINE: &str =
        include_str!("../../../../tests/fixtures/updates/dnf-check-upgrade-offline.stderr");
    const NO_CACHE: &str = include_str!(
        "../../../../tests/fixtures/updates/dnf-check-upgrade-cacheonly-nocache.stderr"
    );
    const ZERO_EPOCH: &str =
        include_str!("../../../../tests/fixtures/updates/dnf-check-upgrade-zeroepoch.json");
    const RPM_27: &str = include_str!("../../../../tests/fixtures/updates/rpm-qf-27.txt");
    const RPM_27_ARGS: &str = include_str!("../../../../tests/fixtures/updates/rpm-qf-27.args");
    const RPM_MISSING: &str = include_str!("../../../../tests/fixtures/updates/rpm-qf-missing.txt");
    const RPM_ZERO_EPOCH: &str =
        include_str!("../../../../tests/fixtures/updates/rpm-qf-zeroepoch.txt");

    fn find<'a>(updates: &'a [Update], name: &str, arch: &str) -> &'a Update {
        updates
            .iter()
            .find(|u| u.name == name && u.arch.as_deref() == Some(arch))
            .unwrap()
    }

    #[test]
    fn dnf_real_fixture_with_rpm_old_versions() {
        let upgrades = parse_dnf_check_upgrade(0, UPGRADES, "").unwrap();
        assert_eq!(upgrades.len(), 27);
        // The rpm argv used for the fixture is exactly the keys of the JSON, in order.
        let keys: Vec<String> = upgrades.iter().map(DnfUpgrade::key).collect();
        assert_eq!(keys.join(" "), RPM_27_ARGS.trim());
        let installed = parse_rpm_installed(RPM_27);
        assert_eq!(installed.len(), 27);
        let updates = dnf_updates(upgrades, &installed);
        assert!(updates
            .iter()
            .all(|u| u.source == Source::Dnf && !u.ignored));
        assert!(updates.iter().all(|u| !u.old_version.is_empty()));
        // Multilib: glibc.x86_64 and glibc.i686 are distinct updates.
        assert_eq!(updates.iter().filter(|u| u.name == "glibc").count(), 2);
        let glibc = find(&updates, "glibc", "i686");
        assert_eq!(
            (glibc.old_version.as_str(), glibc.new_version.as_str()),
            ("2.44-1.fc45", "2.44-2.fc45")
        );
        // Mixed-case name, real epoch on both sides, ^ snapshot and ~ pre-release versions.
        let libraw = find(&updates, "LibRaw", "x86_64");
        assert_eq!(
            (libraw.old_version.as_str(), libraw.new_version.as_str()),
            ("0.22.2-1.fc45", "0.22.3-1.fc45")
        );
        let xxd = find(&updates, "xxd", "x86_64");
        assert_eq!(
            (xxd.old_version.as_str(), xxd.new_version.as_str()),
            ("2:9.2.1129-1.fc45", "2:9.2.1202-1.fc45")
        );
        let bind = find(&updates, "bind-libs", "x86_64");
        assert_eq!(bind.new_version, "32:9.18.51^git20261003.d0a87358-12.fc45");
        assert_eq!(bind.old_version, "32:9.18.50-35.fc45");
        let ibus = find(&updates, "ibus-libs", "x86_64");
        assert_eq!(
            (ibus.old_version.as_str(), ibus.new_version.as_str()),
            ("1.5.35~beta2-2.fc45", "1.5.35~rc1-1.fc45")
        );
        let cldr = find(&updates, "cldr-emoji-annotation", "noarch");
        assert_eq!(
            (cldr.old_version.as_str(), cldr.new_version.as_str()),
            ("1:49~beta2-1.fc45", "1:49-1.fc45")
        );
        // rpm's ordering agrees that every listed upgrade is newer than the installed build.
        for u in &updates {
            assert_eq!(
                evr_cmp(&u.new_version, &u.old_version),
                Ordering::Greater,
                "{u:?}"
            );
        }
        // Reboot list: glibc and mesa-* yes, LibRaw no.
        assert!(glibc.needs_reboot() && find(&updates, "mesa-filesystem", "x86_64").needs_reboot());
        assert!(!libraw.needs_reboot());
    }

    #[test]
    fn dnf_no_updates_is_empty_ok_only_on_exit_zero() {
        assert_eq!(NONE, "{}\n");
        assert_eq!(parse_dnf_check_upgrade(0, NONE, ""), Ok(Vec::new()));
        assert_eq!(
            parse_dnf_check_upgrade(0, "{\"upgrades\":[]}", ""),
            Ok(Vec::new())
        );
        // A future section without upgrades is still "no upgrades".
        assert_eq!(
            parse_dnf_check_upgrade(0, "{\"obsoleting_packages\":[]}", ""),
            Ok(Vec::new())
        );
    }

    #[test]
    fn dnf_exit_one_and_other_codes_are_errors_never_zero_updates() {
        // Real metadata failures: exit 1, empty stdout.
        assert_eq!(
            parse_dnf_check_upgrade(1, "", OFFLINE),
            Err(UpdatesError::CommandFailed)
        );
        assert_eq!(
            parse_dnf_check_upgrade(1, "", NO_CACHE),
            Err(UpdatesError::CommandFailed)
        );
        // Even if exit 1 came with a parseable empty object.
        assert_eq!(
            parse_dnf_check_upgrade(1, NONE, ""),
            Err(UpdatesError::CommandFailed)
        );
        for code in [-1, 2, 100, 127, 255] {
            assert_eq!(
                parse_dnf_check_upgrade(code, UPGRADES, ""),
                Err(UpdatesError::CommandFailed)
            );
        }
    }

    #[test]
    fn dnf_bad_json_shapes_are_errors() {
        for stdout in [
            "",
            "\n",
            "not json",
            "[]",
            "null",
            "42",
            "\"x\"",
            "{\"upgrades\":{}}",
            "{\"upgrades\":\"x\"}",
            "{\"upgrades\":[",
        ] {
            assert_eq!(
                parse_dnf_check_upgrade(0, stdout, ""),
                Err(UpdatesError::InvalidJson),
                "{stdout:?}"
            );
        }
        let huge = format!("{{\"upgrades\":[]{}}}", " ".repeat(DNF_MAX_STDOUT_BYTES));
        assert_eq!(
            parse_dnf_check_upgrade(0, &huge, ""),
            Err(UpdatesError::StdoutTooLarge)
        );
        let at_cap = format!("{{}}{}", " ".repeat(DNF_MAX_STDOUT_BYTES - 2));
        assert_eq!(parse_dnf_check_upgrade(0, &at_cap, ""), Ok(Vec::new()));
        let entry = r#"{"name":"a","arch":"noarch","evr":"1-1"}"#;
        let many = format!(
            "{{\"upgrades\":[{}]}}",
            vec![entry; MAX_LINES + 1].join(",")
        );
        assert_eq!(
            parse_dnf_check_upgrade(0, &many, ""),
            Err(UpdatesError::TooManyLines)
        );
    }

    #[test]
    fn dnf_invalid_entries_are_skipped_and_all_invalid_is_an_error() {
        let ok = r#"{"name":"kernel-core","arch":"x86_64","evr":"7.2.15-300.fc45","repository":"updates","future":1}"#;
        let bad = [
            r#"{"arch":"x86_64","evr":"1-1"}"#,
            r#"{"name":"x","evr":"1-1"}"#,
            r#"{"name":"x","arch":"x86_64"}"#,
            r#"{"name":1,"arch":"x86_64","evr":"1-1"}"#,
            r#"{"name":"-x","arch":"x86_64","evr":"1-1"}"#,
            r#"{"name":"x y","arch":"x86_64","evr":"1-1"}"#,
            r#"{"name":"x","arch":"sparc","evr":"1-1"}"#,
            r#"{"name":"x","arch":"x86_64","evr":"1=1"}"#,
            r#""string""#,
            "null",
        ];
        let mixed = format!("{{\"upgrades\":[{},{ok},{ok}]}}", bad.join(","));
        let parsed = parse_dnf_check_upgrade(0, &mixed, "").unwrap();
        assert_eq!(
            parsed,
            vec![DnfUpgrade {
                name: "kernel-core".into(),
                arch: "x86_64".into(),
                evr: "7.2.15-300.fc45".into()
            }]
        );
        let all_bad = format!("{{\"upgrades\":[{}]}}", bad.join(","));
        assert_eq!(
            parse_dnf_check_upgrade(0, &all_bad, ""),
            Err(UpdatesError::AllMalformed)
        );
        // Repository is not required (only shown sections and identities matter).
        assert_eq!(
            parse_dnf_check_upgrade(
                0,
                r#"{"upgrades":[{"name":"a","arch":"noarch","evr":"1-1"}]}"#,
                ""
            )
            .unwrap()
            .len(),
            1
        );
    }

    #[test]
    fn dnf_zero_epoch_is_dropped_on_both_sides_and_real_epochs_kept() {
        // Real pair: dnf prints 1.1-1, rpm prints 0:1.0-1 for a package with `Epoch: 0`.
        let upgrades = parse_dnf_check_upgrade(0, ZERO_EPOCH, "").unwrap();
        assert_eq!(RPM_ZERO_EPOCH, "mdistro-zeroepoch.noarch\t0:1.0-1\n");
        let updates = dnf_updates(upgrades, &parse_rpm_installed(RPM_ZERO_EPOCH));
        assert_eq!(
            (
                updates[0].old_version.as_str(),
                updates[0].new_version.as_str()
            ),
            ("1.0-1", "1.1-1")
        );
        assert_eq!(normalize_evr("0:1.0-1"), "1.0-1");
        assert_eq!(normalize_evr("00:1.0-1"), "1.0-1");
        assert_eq!(normalize_evr("2:9.2-1"), "2:9.2-1");
        assert_eq!(normalize_evr("10:1-1"), "10:1-1");
        assert_eq!(normalize_evr("1.0-1"), "1.0-1");
        assert_eq!(normalize_evr(":1.0"), ":1.0");
        assert_eq!(normalize_evr("0:"), "0:");
        // The zero epoch never reaches the UI from the dnf side either.
        let json = r#"{"upgrades":[{"name":"a","arch":"noarch","evr":"0:2-1"}]}"#;
        assert_eq!(parse_dnf_check_upgrade(0, json, "").unwrap()[0].evr, "2-1");
    }

    #[test]
    fn rpm_missing_packages_and_multiple_installed_versions() {
        // Real output: "package X is not installed" lines are on stdout and are skipped.
        let installed = parse_rpm_installed(RPM_MISSING);
        assert_eq!(installed.len(), 2);
        assert_eq!(installed["xxd.x86_64"], "2:9.2.1129-1.fc45");
        assert_eq!(installed["glibc.i686"], "2.44-1.fc45");
        let upgrades = vec![
            DnfUpgrade {
                name: "not-a-package".into(),
                arch: "x86_64".into(),
                evr: "1-1".into(),
            },
            DnfUpgrade {
                name: "xxd".into(),
                arch: "x86_64".into(),
                evr: "2:9.2.1202-1.fc45".into(),
            },
        ];
        let updates = dnf_updates(upgrades, &installed);
        assert_eq!(updates.len(), 2, "unknown old version still listed");
        assert_eq!(updates[0].old_version, "");
        // Install-only kernels: the highest installed build is the one being upgraded from,
        // whatever order rpm prints them in.
        let kernels = "kernel-core.x86_64\t7.2.13-300.fc45\nkernel-core.x86_64\t7.2.9-300.fc45\nkernel-core.x86_64\t7.2.11-300.fc45\n";
        assert_eq!(
            parse_rpm_installed(kernels)["kernel-core.x86_64"],
            "7.2.13-300.fc45"
        );
        // Junk lines and invalid keys are ignored without panicking.
        let junk = "\t\n.x86_64\t1-1\nfoo\t1-1\nfoo.sparc\t1-1\nfoo.noarch\t\nfoo.noarch\t1 1\nok.noarch\t1-1\r\n";
        assert_eq!(
            parse_rpm_installed(junk).into_iter().collect::<Vec<_>>(),
            vec![("ok.noarch".to_string(), "1-1".to_string())]
        );
    }

    #[test]
    fn rpm_version_ordering() {
        use Ordering::*;
        for (a, b, want) in [
            ("1.0", "1.0", Equal),
            ("1.0", "1.1", Less),
            ("1.10", "1.9", Greater),
            ("1.010", "1.10", Equal),
            ("1.0a", "1.0", Greater),
            ("1.0", "1.0.1", Less),
            ("1a", "1.1", Less),
            ("2", "a", Greater),
            ("1.0~rc1", "1.0", Less),
            ("1.0~rc1", "1.0~rc2", Less),
            ("1.0~~", "1.0~", Less),
            ("1.0^", "1.0", Greater),
            ("1.0^git1", "1.0", Greater),
            ("1.0^git1", "1.0.1", Less),
            ("1.0^git1", "1.0~rc1", Greater),
            ("1.0^", "1.0^1", Less),
            ("1_0", "1.0", Equal),
            ("99999999999999999999999", "9", Greater),
            ("", "", Equal),
            ("", "1", Less),
        ] {
            assert_eq!(rpm_vercmp(a, b), want, "{a} vs {b}");
            assert_eq!(rpm_vercmp(b, a), want.reverse(), "{b} vs {a}");
        }
        assert_eq!(evr_cmp("1:1.0-1", "2.0-1"), Greater);
        assert_eq!(evr_cmp("0:1.0-1", "1.0-1"), Equal);
        assert_eq!(evr_cmp("1.0-2.fc45", "1.0-10.fc45"), Less);
        assert_eq!(
            evr_cmp("9.18.51^git20261003.d0a87358-12.fc45", "9.18.50-35.fc45"),
            Greater
        );
    }

    #[test]
    fn dnf_reboot_list() {
        for name in [
            "kernel",
            "kernel-core",
            "kernel-modules",
            "kernel-modules-core",
            "kernel-modules-extra",
            "glibc",
            "systemd",
            "linux-firmware",
            "dbus-broker",
            "mesa-dri-drivers",
            "mesa-libGL",
            "akmod-nvidia",
            "kmod-nvidia-7.2.15-300.fc45.x86_64",
            "xorg-x11-drv-nvidia-cuda",
            "nvidia-settings",
        ] {
            assert!(is_reboot_pkg_dnf(name), "{name}");
        }
        for name in [
            "kernelshark",
            "glibc-common",
            "systemd-libs",
            "mesa",
            "linux",
            "linux-cachyos",
            "LibRaw",
            "firefox",
        ] {
            assert!(!is_reboot_pkg_dnf(name), "{name}");
        }
    }

    #[test]
    fn dnf_validators() {
        assert!(
            valid_name("LibRaw")
                && valid_name("libstdc++")
                && valid_name("NetworkManager-libnm")
                && valid_name("python3.15")
        );
        assert!(
            !valid_name("")
                && !valid_name("-x")
                && !valid_name(".x")
                && !valid_name("a b")
                && !valid_name("a@b")
                && !valid_name("a/b")
        );
        assert!(valid_name(&"a".repeat(128)) && !valid_name(&"a".repeat(129)));
        assert!(
            valid_evr("32:9.18.51^git20261003.d0a87358-12.fc45")
                && valid_evr("1:49~beta2-1.fc45")
                && valid_evr("1.0+git-1")
        );
        assert!(
            !valid_evr("")
                && !valid_evr("-1")
                && !valid_evr("1 1")
                && !valid_evr("1=1")
                && !valid_evr(&"1".repeat(129))
        );
        assert!(valid_key("python3.15.noarch") && valid_key("glibc.i686"));
        assert!(
            !valid_key("glibc")
                && !valid_key("glibc.")
                && !valid_key(".x86_64")
                && !valid_key("-e.x86_64")
        );
    }
}
