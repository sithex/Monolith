//! Data-directory resolution (SPEC §2.6). Test resolution refuses
//! the live ones (`<repo>/.island-data`, `<repo>/.island-data-overlay`) — including through
//! symlinks, `..` components and nested paths — before creating anything.

use crate::error::IslandError;
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Component, Path, PathBuf},
};

const LIVE_DIR_NAMES: [&str; 2] = [".island-data", ".island-data-overlay"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub status: PathBuf,
    pub panel_sock: PathBuf,
    pub ingest_sock: PathBuf,
    pub lock: PathBuf,
}

/// Repository root. Resolved at runtime from the running executable (the nearest ancestor
/// holding `island-tauri/run.sh`), so renaming or moving the checkout keeps working with the
/// existing build. Falls back to the compile-time crate location.
pub fn repo_root() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| root_containing(&exe))
        .unwrap_or_else(compile_time_root)
}

fn root_containing(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .skip(1)
        .find(|dir| dir.join("island-tauri/run.sh").is_file())
        .map(Path::to_path_buf)
}

fn compile_time_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

pub fn live_data_dirs(repo_root: &Path) -> Vec<PathBuf> {
    LIVE_DIR_NAMES.iter().map(|n| repo_root.join(n)).collect()
}

/// Pure live-path policy. No filesystem access, environment override or directory creation.
/// Production supplies the compile-time repository root; tests supply a scratch root.
pub fn resolve_live_dir(repo_root: &Path, overlay: bool) -> Paths {
    derived_paths(repo_root.join(if overlay {
        ".island-data-overlay"
    } else {
        ".island-data"
    }))
}

fn derived_paths(data_dir: PathBuf) -> Paths {
    Paths {
        status: data_dir.join("status.json"),
        panel_sock: data_dir.join("panel.sock"),
        ingest_sock: data_dir.join("ingest.sock"),
        lock: data_dir.join("island.lock"),
        data_dir,
    }
}

/// The only startup selector. Never weaken the test resolver to enable live mode.
pub fn resolve(cli: &crate::cli::Cli, root: &Path) -> Result<Paths, IslandError> {
    if cli.test_instance {
        let data = cli
            .data_dir
            .as_deref()
            .ok_or_else(|| IslandError::invalid("--test-instance requires --data-dir"))?;
        resolve_test_dir(data, root)
    } else if cli.data_dir.is_some() || cli.fixture_pty || cli.spike.is_some() {
        Err(IslandError::invalid("test-only options in live launch"))
    } else {
        Ok(resolve_live_dir(root, cli.overlay))
    }
}

/// Absolute path with symlinks resolved for the longest existing ancestor; the not-yet-existing
/// tail is appended verbatim. `..` in the tail is rejected (it could climb out after the check).
fn canonical_lenient(path: &Path) -> Result<PathBuf, IslandError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| IslandError::io(format!("cannot read current directory: {e}")))?
            .join(path)
    };
    let mut existing = absolute.clone();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        let name = existing
            .file_name()
            .ok_or_else(|| IslandError::invalid("data directory has no existing ancestor"))?
            .to_owned();
        tail.push(name);
        if !existing.pop() {
            return Err(IslandError::invalid(
                "data directory has no existing ancestor",
            ));
        }
    }
    if tail.iter().any(|c| c == "..") {
        return Err(IslandError::invalid("data directory must not contain `..`"));
    }
    let mut resolved = fs::canonicalize(&existing)
        .map_err(|e| IslandError::io(format!("cannot resolve data directory: {e}")))?;
    resolved.extend(tail.iter().rev());
    Ok(resolved)
}

pub fn resolve_test_dir(data_dir: &Path, repo_root: &Path) -> Result<Paths, IslandError> {
    let resolved = canonical_lenient(data_dir)?;
    let named_live = resolved.components().any(|c| match c {
        Component::Normal(n) => LIVE_DIR_NAMES.iter().any(|live| n == *live),
        _ => false,
    });
    let inside_live = live_data_dirs(repo_root).iter().any(|live| {
        canonical_lenient(live)
            .map(|live| resolved.starts_with(live))
            .unwrap_or(true) // cannot prove it is not live: refuse
    });
    if named_live || inside_live {
        return Err(IslandError::invalid(
            "refusing to use a live Hermes Island data directory in a test instance",
        ));
    }
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&resolved)
        .map_err(|e| IslandError::io(format!("cannot create data directory: {e}")))?;
    // Existing scratch directories (e.g. the inherited GUI harness) may have
    // been created with the process umask. Restrict only our validated owned dir.
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = fs::metadata(&resolved)
            .map_err(|_| IslandError::io("Cannot inspect test directory"))?;
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(IslandError::invalid(
                "Test data directory is not owned by this user",
            ));
        }
        fs::set_permissions(&resolved, fs::Permissions::from_mode(0o700))
            .map_err(|_| IslandError::io("Cannot restrict test data directory"))?;
    }
    Ok(derived_paths(resolved))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hi-tauri-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_a_private_scratch_directory_with_derived_paths() {
        let root = scratch("ok");
        let paths = resolve_test_dir(&root.join("a/b"), &root.join("repo")).unwrap();
        assert!(paths.data_dir.ends_with("a/b"));
        assert_eq!(
            fs::metadata(&paths.data_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(paths.panel_sock, paths.data_dir.join("panel.sock"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_live_dirs_nested_dirs_dotdot_and_symlinks_without_creating_anything() {
        let root = scratch("live");
        let repo = root.join("repo");
        let live = repo.join(".island-data-overlay");
        fs::create_dir_all(&live).unwrap();
        let link = root.join("innocent");
        symlink(&live, &link).unwrap();
        for bad in [
            live.clone(),
            live.join("nested/deeper"),
            repo.join(".island-data"),
            repo.join("sub/../.island-data-overlay"),
            link.clone(),
            link.join("x"),
            root.join("elsewhere/.island-data"),
        ] {
            let before = fs::read_dir(&live).unwrap().count();
            assert!(resolve_test_dir(&bad, &repo).is_err(), "{bad:?}");
            assert_eq!(fs::read_dir(&live).unwrap().count(), before);
        }
        assert!(!repo.join(".island-data").exists());
        assert!(!live.join("nested").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repo_root_points_at_the_checkout() {
        assert!(repo_root().join("island-tauri/run.sh").exists());
        assert_eq!(repo_root(), compile_time_root());
    }

    #[test]
    fn repo_root_follows_a_moved_checkout() {
        let root = scratch("moved");
        let exe = root.join("island-tauri/src-tauri/target/release/hermes-island");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(root.join("island-tauri/run.sh"), "").unwrap();
        assert_eq!(root_containing(&exe), Some(root.clone()));
        let elsewhere = scratch("unrelated");
        assert_eq!(root_containing(&elsewhere.join("bin/app")), None);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(elsewhere).unwrap();
    }
}

/// Stable, machine-independent location the Plasma notch widget runs its click client from:
/// `$XDG_STATE_HOME/monolith/anchor-client.py` (default `~/.local/state/...`). It is a
/// symlink to this checkout's `island/anchor-client.py`, so the widget never hardcodes
/// where the repository lives. The client resolves the link, so it still finds the repo.
pub fn anchor_client_link(state_home: Option<&Path>, home: &Path) -> PathBuf {
    state_home
        .filter(|p| p.is_absolute())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".local/state"))
        .join("monolith/anchor-client.py")
}

/// Point the link at `repo_root`'s client. Only ever replaces a symlink (never a real file)
/// and swaps atomically, so a click racing startup sees either the old or the new target.
pub fn publish_anchor_client(link: &Path, repo_root: &Path) -> std::io::Result<bool> {
    let target = repo_root.join("island/anchor-client.py");
    if !target.is_file() {
        return Ok(false);
    }
    match std::fs::symlink_metadata(link) {
        Ok(meta) if !meta.file_type().is_symlink() => return Ok(false),
        Ok(_) if std::fs::read_link(link).ok().as_deref() == Some(target.as_path()) => {
            return Ok(true)
        }
        _ => {}
    }
    let parent = link.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".anchor-client.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&staging);
    std::os::unix::fs::symlink(&target, &staging)?;
    std::fs::rename(&staging, link).inspect_err(|_| {
        let _ = std::fs::remove_file(&staging);
    })?;
    Ok(true)
}

#[cfg(test)]
mod anchor_link_tests {
    use super::*;

    #[test]
    fn link_location_honours_absolute_xdg_state_home_only() {
        let home = Path::new("/home/u");
        assert_eq!(
            anchor_client_link(None, home),
            Path::new("/home/u/.local/state/monolith/anchor-client.py")
        );
        assert_eq!(
            anchor_client_link(Some(Path::new("/s")), home),
            Path::new("/s/monolith/anchor-client.py")
        );
        assert_eq!(
            anchor_client_link(Some(Path::new("rel")), home),
            Path::new("/home/u/.local/state/monolith/anchor-client.py")
        );
    }

    #[test]
    fn publish_creates_retargets_and_never_clobbers_real_files() {
        let base = std::env::temp_dir().join(format!("anchor-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (a, b) = (base.join("a"), base.join("b"));
        for repo in [&a, &b] {
            std::fs::create_dir_all(repo.join("island")).unwrap();
            std::fs::write(repo.join("island/anchor-client.py"), "").unwrap();
        }
        let link = base.join("state/monolith/anchor-client.py");
        assert!(publish_anchor_client(&link, &a).unwrap());
        assert_eq!(std::fs::read_link(&link).unwrap(), a.join("island/anchor-client.py"));
        assert!(publish_anchor_client(&link, &a).unwrap());
        assert!(publish_anchor_client(&link, &b).unwrap());
        assert_eq!(std::fs::read_link(&link).unwrap(), b.join("island/anchor-client.py"));
        // Missing client in the checkout: leave things alone.
        assert!(!publish_anchor_client(&link, &base.join("nowhere")).unwrap());
        // A real file the user put there is never replaced.
        std::fs::remove_file(&link).unwrap();
        std::fs::write(&link, "mine").unwrap();
        assert!(!publish_anchor_client(&link, &a).unwrap());
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "mine");
        std::fs::remove_dir_all(&base).unwrap();
    }
}
