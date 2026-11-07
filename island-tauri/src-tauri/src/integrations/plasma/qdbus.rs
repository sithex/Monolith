//! Which `qdbus` the Plasma bridge runs.
//!
//! Arch ships Qt 6's qdbus as `/usr/bin/qdbus6` (and `/usr/lib/qt6/bin/qdbus`). Fedora has no
//! `qdbus6`: `qt6-qttools` ships `/usr/bin/qdbus-qt6` and `/usr/lib64/qt6/bin/qdbus`. Bare
//! `qdbus` is never used, because on some systems it is Qt 5's.
//!
//! Each answer is looked up once per process and cached. Nothing here runs a program.
use std::{ffi::OsStr, os::unix::fs::PermissionsExt, path::Path, sync::OnceLock};

/// Absolute candidates, in order. The first one is also the fallback, so a system without
/// any of them fails exactly as before (spawning `/usr/bin/qdbus6` reports ENOENT).
const CANDIDATES: [&str; 4] = [
    "/usr/bin/qdbus6",
    "/usr/lib/qt6/bin/qdbus",
    "/usr/bin/qdbus-qt6",
    "/usr/lib64/qt6/bin/qdbus",
];
/// Names looked up on `PATH` for panel discovery, in order.
const PATH_NAMES: [&str; 2] = ["qdbus6", "qdbus-qt6"];

/// The qdbus binary for the absolute call sites (KWin scripting, PlasmaShell scripts).
pub(super) fn program() -> &'static Path {
    static PROGRAM: OnceLock<&'static Path> = OnceLock::new();
    PROGRAM.get_or_init(|| Path::new(pick(&CANDIDATES, is_executable)))
}

/// The qdbus program for panel discovery, which has always searched `PATH` (a test fixture
/// puts a fake `qdbus6` there). `PATH` hits come back as bare names so the spawn still
/// searches `PATH`.
pub(super) fn panel_program() -> &'static OsStr {
    static PROGRAM: OnceLock<&'static OsStr> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        let path = std::env::var_os("PATH");
        OsStr::new(pick_panel(
            &PATH_NAMES,
            &CANDIDATES,
            path.as_deref(),
            is_executable,
        ))
    })
}

/// First of `candidates` that `exists` accepts, else the first one.
fn pick(candidates: &[&'static str], exists: impl Fn(&Path) -> bool) -> &'static str {
    candidates
        .iter()
        .copied()
        .find(|c| exists(Path::new(c)))
        .unwrap_or(candidates[0])
}

/// The first of `names` found in a `PATH` directory (in `names` order), then the first
/// existing absolute candidate. With none of them it stays the bare first name, as it always
/// was (same ENOENT).
fn pick_panel(
    names: &[&'static str],
    candidates: &[&'static str],
    path: Option<&OsStr>,
    exists: impl Fn(&Path) -> bool,
) -> &'static str {
    let dirs: Vec<_> = path
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    // An empty PATH entry means the current directory; never search it.
    let on_path = |name: &str| {
        dirs.iter()
            .filter(|d| !d.as_os_str().is_empty())
            .any(|d| exists(&d.join(name)))
    };
    names
        .iter()
        .copied()
        .find(|name| on_path(name))
        .or_else(|| candidates.iter().copied().find(|c| exists(Path::new(c))))
        .unwrap_or(names[0])
}

/// A regular file (after symlinks) with an execute bit.
fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashSet, ffi::OsString};

    fn fs(paths: &[&str]) -> impl Fn(&Path) -> bool {
        let set: HashSet<String> = paths.iter().map(|p| p.to_string()).collect();
        move |p: &Path| set.contains(p.to_str().unwrap())
    }
    /// What CachyOS/Arch `qt6-tools` installs (plus a Qt 5 tool that must not matter).
    const ARCH: [&str; 3] = [
        "/usr/bin/qdbus6",
        "/usr/lib/qt6/bin/qdbus",
        "/usr/bin/qdbuscpp2xml",
    ];
    /// What Fedora 45 `qt6-qttools` installs, plus a Qt 5 `qdbus` on PATH.
    const FEDORA: [&str; 3] = [
        "/usr/bin/qdbus-qt6",
        "/usr/lib64/qt6/bin/qdbus",
        "/usr/bin/qdbus",
    ];
    fn path(s: &str) -> OsString {
        OsString::from(s)
    }
    fn panel(path: Option<&OsStr>, exists: impl Fn(&Path) -> bool) -> &'static str {
        pick_panel(&PATH_NAMES, &CANDIDATES, path, exists)
    }

    #[test]
    fn arch_keeps_todays_programs() {
        assert_eq!(pick(&CANDIDATES, fs(&ARCH)), "/usr/bin/qdbus6");
        let p = path("/opt/tools/bin:/usr/local/bin:/usr/bin");
        assert_eq!(panel(Some(&p), fs(&ARCH)), "qdbus6");
    }

    #[test]
    fn fedora_uses_qdbus_qt6_never_bare_qdbus() {
        assert_eq!(pick(&CANDIDATES, fs(&FEDORA)), "/usr/bin/qdbus-qt6");
        let p = path("/usr/local/bin:/usr/bin:/bin");
        assert_eq!(panel(Some(&p), fs(&FEDORA)), "qdbus-qt6");
        // Only lib64's Qt 6 tool and a Qt 5 `qdbus` on PATH: the lib64 path wins.
        let lib64_only = fs(&["/usr/lib64/qt6/bin/qdbus", "/usr/bin/qdbus"]);
        assert_eq!(pick(&CANDIDATES, &lib64_only), "/usr/lib64/qt6/bin/qdbus");
        assert_eq!(panel(Some(&p), &lib64_only), "/usr/lib64/qt6/bin/qdbus");
    }

    #[test]
    fn candidate_order_is_fixed_and_missing_falls_back_unchanged() {
        assert_eq!(
            pick(
                &CANDIDATES,
                fs(&["/usr/lib/qt6/bin/qdbus", "/usr/bin/qdbus-qt6"])
            ),
            "/usr/lib/qt6/bin/qdbus"
        );
        assert_eq!(
            pick(
                &CANDIDATES,
                fs(&["/usr/lib64/qt6/bin/qdbus", "/usr/bin/qdbus-qt6"])
            ),
            "/usr/bin/qdbus-qt6"
        );
        assert_eq!(pick(&CANDIDATES, fs(&[])), "/usr/bin/qdbus6");
        assert_eq!(
            pick(&CANDIDATES, fs(&["/usr/bin/qdbus"])),
            "/usr/bin/qdbus6"
        );
        let p = path("/usr/bin");
        assert_eq!(panel(Some(&p), fs(&["/usr/bin/qdbus"])), "qdbus6");
        assert_eq!(panel(None, fs(&[])), "qdbus6");
    }

    #[test]
    fn path_lookup_prefers_qdbus6_anywhere_and_skips_empty_entries() {
        // The kwin_phase3_host fixture's fake qdbus6 is found even after a real qdbus-qt6.
        let p = path("/usr/bin:/tmp/fixture/bin");
        let exists = fs(&["/usr/bin/qdbus-qt6", "/tmp/fixture/bin/qdbus6"]);
        assert_eq!(panel(Some(&p), exists), "qdbus6");
        // PATH beats the absolute list.
        let p = path("/opt/qt/bin");
        let exists = fs(&["/opt/qt/bin/qdbus-qt6", "/usr/lib/qt6/bin/qdbus"]);
        assert_eq!(panel(Some(&p), exists), "qdbus-qt6");
        // An empty entry (the current directory) is never searched.
        let p = path(":/usr/bin");
        assert_eq!(
            panel(Some(&p), |c: &Path| c == Path::new("qdbus6")),
            "qdbus6"
        );
        assert_eq!(
            panel(Some(&p), |c: &Path| c == Path::new("qdbus-qt6")),
            "qdbus6"
        );
    }

    #[test]
    fn is_executable_needs_an_exec_bit_on_a_file() {
        let base = std::env::var_os("TMPDIR").map_or_else(std::env::temp_dir, Into::into);
        let dir = base.join(format!("qdbus-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("tool");
        std::fs::write(&file, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!is_executable(&file));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(is_executable(&file));
        assert!(!is_executable(&dir));
        assert!(!is_executable(&dir.join("missing")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cached_programs_come_from_the_lists() {
        assert!(CANDIDATES.contains(&program().to_str().unwrap()));
        let panel = panel_program().to_str().unwrap();
        assert!(PATH_NAMES.contains(&panel) || CANDIDATES.contains(&panel));
        // Same answer every call (cached).
        assert!(std::ptr::eq(program(), program()));
    }
}
