//! Live config/state policy and one-time, verified, copy-only migration.
//! Explicit test directories never enter this module's startup migration.
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

pub const CONFIG_FILES: [&str; 3] = ["settings.json", "layout.json", "appearance.json"];
const MARKER: &str = "MIGRATED-FROM-REPO.json";
const PENDING: &str = ".migration-pending";
const NOTE: &str = "MOVED-TO-XDG.txt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDirs {
    pub config: PathBuf,
    pub state: PathBuf,
}

impl DataDirs {
    /// XDG values must be absolute. No IO, so callers/tests can supply their own environment.
    pub fn resolve(home: &Path, config: Option<&Path>, state: Option<&Path>) -> Self {
        let base = |value: Option<&Path>, default: &str| {
            value
                .filter(|p| p.is_absolute())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| home.join(default))
                .join("monolith")
        };
        Self {
            config: base(config, ".config"),
            state: base(state, ".local/state"),
        }
    }

    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| PathBuf::from("/"));
        let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
        let state = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from);
        Self::resolve(&home, config.as_deref(), state.as_deref())
    }

    /// Read-only client/launcher selection, including startup failure and partial-commit fallback.
    pub fn selected(&self, old: &Path) -> Self {
        if old.is_dir()
            && ((!self.state.join(MARKER).is_file()
                && [&self.config, &self.state]
                    .into_iter()
                    .any(|d| fs::symlink_metadata(d.join(PENDING)).is_ok()))
                || old.join(".xdg-migration-failed").exists()
                || (!has_data(&self.config) && !has_data(&self.state)))
        {
            Self {
                config: old.to_owned(),
                state: old.to_owned(),
            }
        } else {
            self.clone()
        }
    }
}

fn has_data(dir: &Path) -> bool {
    match fs::read_dir(dir) {
        Ok(mut entries) => entries.any(|e| {
            e.map_or(true, |e| {
                e.file_name() != "anchor-client.py" || !e.file_type().is_ok_and(|t| t.is_symlink())
            })
        }),
        Err(e) => e.kind() != io::ErrorKind::NotFound,
    }
}

pub(crate) fn private_dir(dir: &Path) -> io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let m = fs::symlink_metadata(dir)?;
    // SAFETY: geteuid has no preconditions.
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::other(
            "Directory is not a real directory owned by this user",
        ));
    }
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    let result = f.write_all(bytes).and_then(|()| f.sync_all());
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

fn read_nofollow(path: &Path) -> io::Result<Vec<u8>> {
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    if !f.metadata()?.is_file() {
        return Err(io::Error::other("Not a regular file"));
    }
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn runtime_only(name: &str) -> bool {
    let preference_temp = name
        .strip_suffix(".new")
        .and_then(|stem| stem.rsplit_once('.'))
        .is_some_and(|(base, pid)| {
            matches!(
                base,
                "settings.json" | "layout.json" | "appearance.json" | "agent-live-blocks.json"
            ) && !pid.is_empty()
                && pid.bytes().all(|b| b.is_ascii_digit())
        });
    preference_temp
        || name.ends_with(".sock")
        || name.ends_with(".lock")
        || name.ends_with(".pid")
        || name.ends_with(".tmp")
        || name == NOTE
        || name == PENDING
        || name == MARKER
        || name == ".xdg-migration-failed"
}

// Open each directory without following a link, then enumerate through the owned descriptor.
// Recursion is anchored to that descriptor even if a source directory is renamed concurrently.
fn copy_tree(
    source: &Path,
    dest: &Path,
    relative: &Path,
    skipped: &mut Vec<String>,
) -> io::Result<()> {
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(source)?;
    let anchored = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
    for entry in fs::read_dir(&anchored)? {
        let entry = entry?;
        let name = entry.file_name();
        let from = entry.path();
        let to = dest.join(&name);
        let meta = fs::symlink_metadata(&from)?;
        if meta.file_type().is_symlink() {
            skipped.push(relative.join(&name).to_string_lossy().into_owned());
            continue;
        }
        if runtime_only(&name.to_string_lossy()) {
            continue;
        }
        if meta.is_dir() {
            private_dir(&to)?;
            copy_tree(&from, &to, &relative.join(&name), skipped)?;
            sync_dir(&to)?;
        } else if meta.is_file() {
            let mut input = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
                .open(&from)?;
            let before = input.metadata()?;
            if !before.is_file() || before.dev() != meta.dev() || before.ino() != meta.ino() {
                return Err(io::Error::other("Source changed while copying"));
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&to)?;
            io::copy(&mut input, &mut output)?;
            output.set_permissions(fs::Permissions::from_mode(before.mode() & 0o777))?;
            output.sync_all()?;
            verify(&from, &to)?;
        } else {
            skipped.push(relative.join(&name).to_string_lossy().into_owned());
        }
    }
    Ok(())
}

fn fingerprint(path: &Path) -> io::Result<(u64, Vec<u8>)> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Not a regular file"));
    }
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        hash.update(&buffer[..n]);
    }
    Ok((size, hash.finalize().to_vec()))
}

fn verify(source: &Path, target: &Path) -> io::Result<()> {
    if fingerprint(source)? != fingerprint(target)? {
        return Err(io::Error::other(
            "Migration size/SHA-256 verification failed",
        ));
    }
    Ok(())
}

fn discard(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(path),
        Ok(_) => Err(io::Error::other(
            "Migration staging path is not a real directory",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Keeps launches from racing a migration, including an older binary using the old locks.
fn lock(path: &Path) -> io::Result<File> {
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    let m = f.metadata()?;
    // SAFETY: geteuid is unconditional.
    if !m.is_file()
        || m.nlink() != 1
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
    {
        return Err(io::Error::other("Unsafe migration/instance lock file"));
    }
    // Test builds only: a child spawned by another test thread keeps a copy of a just-released
    // lock descriptor until its exec closes it, so wait briefly for that copy to go away. The
    // island migrates before it starts any thread or child, so production never waits.
    let patience = std::time::Duration::from_secs(if cfg!(test) { 2 } else { 0 });
    let started = std::time::Instant::now();
    // SAFETY: flock is applied to an owned descriptor.
    while unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        if started.elapsed() >= patience {
            // Another island (old or new binary) is running or migrating: not a failure.
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Another Monolith instance holds the data directory",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let current = fs::symlink_metadata(path)?;
    if current.dev() != m.dev() || current.ino() != m.ino() {
        return Err(io::Error::other("Migration lock was replaced"));
    }
    Ok(f)
}

/// Returns true only for this launch's successful migration. On error callers use BOTH old dirs.
/// Each installed side carries a source-specific pending tag. Until the final marker is durable,
/// a later launch removes only tagged sides and redoes the copy. Existing data is never merged.
pub fn prepare(dirs: &DataDirs, old: &Path) -> io::Result<bool> {
    if !old.exists() {
        private_dir(&dirs.config)?;
        private_dir(&dirs.state)?;
        return Ok(false);
    }
    if !fs::symlink_metadata(old)?.is_dir() {
        return Err(io::Error::other(
            "Legacy data directory is a symlink or not a directory",
        ));
    }
    let canonical =
        |p: &Path| crate::paths::canonical_lenient(p).map_err(|e| io::Error::other(e.to_string()));
    let real_old = canonical(old)?;
    let real_config = canonical(&dirs.config)?;
    let real_state = canonical(&dirs.state)?;
    if [&real_config, &real_state]
        .into_iter()
        .any(|p| real_old.starts_with(p) || p.starts_with(&real_old))
        || (dirs.config != dirs.state
            && (real_config.starts_with(&real_state) || real_state.starts_with(&real_config)))
    {
        return Err(io::Error::other(
            "Migration directories overlap, including through an alias",
        ));
    }
    // Already committed: read-only no-op apart from tidying. Takes no lock, so a second launch
    // beside a running island fails later in the singleton without touching migration state.
    if dirs.state.join(MARKER).is_file() {
        return finish_committed(dirs, old);
    }
    let _legacy_locks = [
        lock(&old.join("island.lock"))?,
        lock(&old.join("instance.lock"))?,
    ];
    let parent = dirs
        .state
        .parent()
        .ok_or_else(|| io::Error::other("Missing state parent"))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let _state_lock = lock(&parent.join(".monolith-instance.lock"))?;
    let _migration_lock = lock(&parent.join(".monolith-migration.lock"))?;
    let tag = serde_json::to_vec(&(old, &dirs.config, &dirs.state))?;
    let targets = if dirs.config == dirs.state {
        vec![dirs.state.clone()]
    } else {
        vec![dirs.config.clone(), dirs.state.clone()]
    };
    if targets
        .iter()
        .any(|p| old.starts_with(p) || p.starts_with(old))
        || (dirs.config != dirs.state
            && (dirs.config.starts_with(&dirs.state) || dirs.state.starts_with(&dirs.config)))
    {
        return Err(io::Error::other("Migration directories overlap"));
    }
    let stages: Vec<PathBuf> = targets
        .iter()
        .enumerate()
        .map(|(i, target)| target.with_file_name(format!(".monolith-migration-{i}.tmp")))
        .collect();
    if dirs.state.join(MARKER).is_file() {
        // Another launch committed while this one waited for nothing (locks are non-blocking).
        return finish_committed(dirs, old);
    }
    for target in &targets {
        if read_nofollow(&target.join(PENDING)).ok().as_deref() == Some(&tag) {
            discard(target)?;
        }
    }
    if has_data(&dirs.config) || has_data(&dirs.state) {
        // Files in the new folders but no migration record. If the old folder still holds
        // saved data and was never migrated (no note of ours), those files are not proof that
        // the user's preferences moved: keep the old folder rather than start from defaults.
        if has_saved_data(old)? && !our_note(old) {
            return Err(io::Error::other(
                "XDG Monolith folders already contain files but no migration record; \
                 keeping the original data directory",
            ));
        }
        private_dir(&dirs.config)?;
        private_dir(&dirs.state)?;
        let _ = fs::remove_file(old.join(".xdg-migration-failed"));
        return Ok(false);
    }
    for stage in &stages {
        discard(stage)?;
    }
    let mut installed = Vec::new();
    let result = (|| {
        for stage in &stages {
            private_dir(stage)?;
        }
        // Initially copy everything into state, then split the three preference files.
        let state_stage = stages
            .last()
            .ok_or_else(|| io::Error::other("No migration target"))?;
        let mut skipped = Vec::new();
        copy_tree(old, state_stage, Path::new(""), &mut skipped)?;
        if targets.len() == 2 {
            for name in CONFIG_FILES {
                let source = state_stage.join(name);
                if source.exists() {
                    // XDG config and state may live on different filesystems.
                    let target = stages[0].join(name);
                    fs::copy(&source, &target)?;
                    File::open(&target)?.sync_all()?;
                    verify(&source, &target)?;
                    fs::remove_file(source)?;
                }
            }
        }
        for stage in &stages {
            write_private(&stage.join(PENDING), &tag)?;
            sync_dir(stage)?;
        }
        for (stage, target) in stages.iter().zip(&targets) {
            if target.exists() {
                // The only allowed non-data entry is the published client symlink.
                let link = target.join("anchor-client.py");
                if fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()) {
                    fs::remove_file(link)?;
                }
                fs::remove_dir(target)?;
            }
            fs::rename(stage, target)?;
            installed.push(target.clone());
            sync_dir(
                target
                    .parent()
                    .ok_or_else(|| io::Error::other("Missing target parent"))?,
            )?;
        }
        let note = format!("{NOTE_PREFIX}\nConfiguration: {}\nState: {}\nThe original files are retained here as a backup. Sockets and locks still use this directory.\n", dirs.config.display(), dirs.state.display());
        // Replace only our previous identical note after an interrupted transaction.
        if read_nofollow(&old.join(NOTE)).ok().as_deref() != Some(note.as_bytes()) {
            let temp_note = old.join(".monolith-moved-note.tmp");
            match fs::remove_file(&temp_note) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
            write_private(&temp_note, note.as_bytes())?;
            // Publish atomically without replacing an unrelated existing note.
            let publish = fs::hard_link(&temp_note, old.join(NOTE));
            fs::remove_file(temp_note)?;
            publish?;
        }
        sync_dir(old)?;
        let record = serde_json::to_vec_pretty(&serde_json::json!({"version":1,"source":old,
            "config":dirs.config,"state":dirs.state,"skippedSymlinksAndSpecialFiles":skipped}))?;
        let marker_temp = dirs.state.join(".migration-marker.tmp");
        write_private(&marker_temp, &record)?;
        fs::rename(marker_temp, dirs.state.join(MARKER))?;
        sync_dir(&dirs.state)?;
        for target in &targets {
            if let Err(error) = fs::remove_file(target.join(PENDING)) {
                tracing::warn!(%error, "Migration committed; pending tag cleanup deferred");
            }
        }
        for name in &skipped {
            tracing::warn!(entry = %name, "Migration skipped symlink/special file");
        }
        Ok(true)
    })();
    if result.is_err() && !dirs.state.join(MARKER).exists() {
        for target in installed {
            discard(&target)?;
        }
        for stage in &stages {
            discard(stage)?;
        }
    }
    if let Err(error) = &result {
        if dirs.state.join(MARKER).is_file() {
            // The marker is the commit point: falling back now would split saved data.
            tracing::warn!(%error, "Migration committed; a later tidy-up step failed");
            let _ = fs::remove_file(old.join(".xdg-migration-failed"));
            return Ok(true);
        }
    }
    if result.is_ok() {
        let _ = fs::remove_file(old.join(".xdg-migration-failed"));
    }
    result
}

const NOTE_PREFIX: &str = "Monolith copied your data to XDG locations.";

fn our_note(old: &Path) -> bool {
    read_nofollow(&old.join(NOTE)).is_ok_and(|bytes| bytes.starts_with(NOTE_PREFIX.as_bytes()))
}

/// True when the legacy folder holds anything migration would copy (not just sockets/locks).
fn has_saved_data(old: &Path) -> io::Result<bool> {
    for entry in fs::read_dir(old)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if (kind.is_file() || kind.is_dir()) && !runtime_only(&entry.file_name().to_string_lossy())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn finish_committed(dirs: &DataDirs, old: &Path) -> io::Result<bool> {
    private_dir(&dirs.config)?;
    private_dir(&dirs.state)?;
    // A crash during tag cleanup needs no recopy; remove only our own pending tags.
    let tag = serde_json::to_vec(&(old, &dirs.config, &dirs.state))?;
    for target in [&dirs.config, &dirs.state] {
        if read_nofollow(&target.join(PENDING)).ok().as_deref() == Some(&tag) {
            let _ = fs::remove_file(target.join(PENDING));
        }
    }
    let _ = fs::remove_file(old.join(".xdg-migration-failed"));
    Ok(false)
}

/// Tests here release and re-take flock()s. A child spawned by another test thread briefly
/// inherits every open descriptor (until its exec closes them), which can make a just-released
/// lock look held. Production migrates before any thread or child exists, so only tests need
/// this: every test in this file runs one at a time.
#[cfg(test)]
pub(crate) fn test_serial() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::fs::symlink, process::Command};

    struct Fixture(
        PathBuf,
        #[allow(dead_code)] std::sync::MutexGuard<'static, ()>,
    );
    impl Fixture {
        fn new() -> Self {
            let serial = test_serial();
            Self(crate::services::scratch("xdg"), serial)
        }
        fn dirs(&self) -> DataDirs {
            DataDirs::resolve(&self.0, None, None)
        }
        fn old(&self) -> PathBuf {
            self.0.join("repo/.island-data-overlay")
        }
        fn populate(&self) {
            private_dir(&self.old()).unwrap();
            for (name, bytes) in [
                ("settings.json", b"{\"autostart\":false}".as_slice()),
                ("layout.json", b"layout"),
                ("appearance.json", b"appearance"),
                ("calendar-secret-url", b"private fixture"),
                ("clipboard.json", b"clipboard"),
                ("status.json", b"{\"pid\":2147483647}"),
                ("shelf.json", b"shelf"),
                ("scratchpad.json", b"notes"),
                ("monolith.log", b"log"),
                ("calendar-local.ics", b"calendar"),
                ("agent-live-blocks.json", b"blocks"),
            ] {
                write_private(&self.old().join(name), bytes).unwrap();
            }
            private_dir(&self.old().join("lyrics-cache")).unwrap();
            write_private(&self.old().join("lyrics-cache/track.json"), b"lyrics").unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn fresh_install_creates_private_xdg_dirs_without_migration() {
        let f = Fixture::new();
        let dirs = f.dirs();
        assert!(!prepare(&dirs, &f.old()).unwrap());
        assert!(!f.old().exists());
        for dir in [&dirs.config, &dirs.state] {
            assert_eq!(fs::metadata(dir).unwrap().mode() & 0o777, 0o700);
            assert_eq!(fs::read_dir(dir).unwrap().count(), 0);
        }
    }

    #[test]
    fn existing_install_copies_all_data_with_identical_bytes_modes_and_retains_source() {
        let f = Fixture::new();
        f.populate();
        let dirs = f.dirs();
        fs::set_permissions(
            f.old().join("monolith.log"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        assert!(prepare(&dirs, &f.old()).unwrap());
        for e in fs::read_dir(f.old()).unwrap() {
            let e = e.unwrap();
            let name = e.file_name();
            if name == NOTE || runtime_only(&name.to_string_lossy()) {
                continue;
            }
            let dest = if matches!(
                name.to_str(),
                Some("settings.json" | "layout.json" | "appearance.json")
            ) {
                &dirs.config
            } else {
                &dirs.state
            };
            if e.file_type().unwrap().is_file() {
                assert_eq!(
                    fingerprint(&e.path()).unwrap(),
                    fingerprint(&dest.join(&name)).unwrap(),
                    "{name:?}"
                );
                assert_eq!(
                    fs::metadata(e.path()).unwrap().mode() & 0o777,
                    fs::metadata(dest.join(name)).unwrap().mode() & 0o777
                );
            }
        }
        assert_eq!(
            fs::read(dirs.state.join("lyrics-cache/track.json")).unwrap(),
            b"lyrics"
        );
        assert!(!dirs.state.join("settings.json").exists());
        assert!(!dirs.config.join("clipboard.json").exists());
        assert!(fs::read_to_string(f.old().join(NOTE))
            .unwrap()
            .contains(&dirs.config.display().to_string()));
        assert!(dirs.state.join(MARKER).is_file());
        assert_eq!(
            fs::metadata(dirs.state.join("calendar-secret-url"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn second_start_is_noop_even_if_old_copy_changes() {
        let f = Fixture::new();
        f.populate();
        let dirs = f.dirs();
        assert!(prepare(&dirs, &f.old()).unwrap());
        let marker = fs::read(dirs.state.join(MARKER)).unwrap();
        fs::write(f.old().join("settings.json"), "changed old copy").unwrap();
        assert!(!prepare(&dirs, &f.old()).unwrap());
        assert_eq!(fs::read(dirs.state.join(MARKER)).unwrap(), marker);
        assert_eq!(
            fs::read(dirs.config.join("settings.json")).unwrap(),
            b"{\"autostart\":false}"
        );
    }

    #[test]
    fn crashed_staging_and_partial_commit_are_discarded_and_recopied() {
        let f = Fixture::new();
        f.populate();
        let dirs = f.dirs();
        private_dir(&dirs.config).unwrap();
        let tag = serde_json::to_vec(&(&f.old(), &dirs.config, &dirs.state)).unwrap();
        write_private(&dirs.config.join(PENDING), &tag).unwrap();
        write_private(&dirs.config.join("settings.json"), b"truncated").unwrap();
        let stage = dirs.state.with_file_name(".monolith-migration-1.tmp");
        private_dir(&stage).unwrap();
        write_private(&stage.join("partial"), b"crash").unwrap();
        assert_eq!(dirs.selected(&f.old()).config, f.old());
        assert!(prepare(&dirs, &f.old()).unwrap());
        assert!(!stage.exists());
        assert!(!dirs.config.join(PENDING).exists());
        assert_eq!(
            fs::read(dirs.config.join("settings.json")).unwrap(),
            b"{\"autostart\":false}"
        );
    }

    #[test]
    fn symlinks_special_files_and_runtime_artifacts_are_never_copied() {
        let f = Fixture::new();
        f.populate();
        let dirs = f.dirs();
        fs::write(f.0.join("secret"), "must not copy").unwrap();
        symlink(f.0.join("secret"), f.old().join("link")).unwrap();
        symlink(f.0.join("secret"), f.old().join("lyrics-cache/link")).unwrap();
        for name in [
            "panel.sock",
            "stale.pid",
            ".apps-1-2.tmp",
            "settings.json.123.new",
        ] {
            write_private(&f.old().join(name), b"runtime").unwrap();
        }
        assert!(prepare(&dirs, &f.old()).unwrap());
        for name in [
            "link",
            "lyrics-cache/link",
            "panel.sock",
            "island.lock",
            "instance.lock",
            "stale.pid",
            ".apps-1-2.tmp",
            "settings.json.123.new",
        ] {
            assert!(!dirs.state.join(name).exists(), "{name}");
        }
        let marker = fs::read_to_string(dirs.state.join(MARKER)).unwrap();
        assert!(marker.contains("link"));
        assert!(fs::symlink_metadata(f.old().join("link"))
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn migration_failure_leaves_old_data_usable_and_partial_targets_unpublished() {
        let f = Fixture::new();
        f.populate();
        let dirs = f.dirs();
        // Fail after both renames: the note cannot be created over an unrelated file.
        write_private(&f.old().join(NOTE), b"user file, do not replace").unwrap();
        assert!(prepare(&dirs, &f.old()).is_err());
        assert!(!dirs.config.exists());
        assert!(!dirs.state.exists());
        assert_eq!(dirs.selected(&f.old()).state, f.old());
        assert_eq!(
            fs::read(f.old().join("clipboard.json")).unwrap(),
            b"clipboard"
        );
        assert_eq!(
            fs::read(f.old().join(NOTE)).unwrap(),
            b"user file, do not replace"
        );
    }

    #[test]
    fn unexplained_xdg_files_keep_the_unmigrated_old_folder_and_merge_nothing() {
        let f = Fixture::new();
        f.populate();
        let dirs = f.dirs();
        private_dir(&dirs.config).unwrap();
        write_private(&dirs.config.join("settings.json"), b"new prefs").unwrap();
        assert!(prepare(&dirs, &f.old()).is_err());
        assert_eq!(
            fs::read(dirs.config.join("settings.json")).unwrap(),
            b"new prefs"
        );
        assert!(!dirs.state.join("clipboard.json").exists());
        assert!(!f.old().join(NOTE).exists());
        // Startup keeps the user's saved preferences and every client follows it.
        let paths = crate::paths::prepare_live_dirs(dirs.clone(), f.old()).unwrap();
        assert_eq!(paths.config_dir, f.old());
        assert_eq!(paths.data_dir, f.old());
        assert!(paths.migration_error.is_some());
        assert_eq!(dirs.selected(&f.old()).config, f.old());
    }

    #[test]
    fn migrated_install_with_cleared_state_keeps_using_xdg_config() {
        let f = Fixture::new();
        f.populate();
        let dirs = f.dirs();
        assert!(prepare(&dirs, &f.old()).unwrap());
        fs::write(dirs.config.join("settings.json"), "changed after migration").unwrap();
        fs::remove_dir_all(&dirs.state).unwrap();
        assert!(!prepare(&dirs, &f.old()).unwrap());
        assert_eq!(
            fs::read(dirs.config.join("settings.json")).unwrap(),
            b"changed after migration"
        );
        assert_eq!(dirs.selected(&f.old()), dirs);
    }

    #[test]
    fn fresh_install_socket_folder_does_not_block_using_xdg() {
        let f = Fixture::new();
        let dirs = f.dirs();
        private_dir(&f.old()).unwrap();
        for name in ["island.lock", "instance.lock", "panel.sock"] {
            write_private(&f.old().join(name), b"").unwrap();
        }
        private_dir(&dirs.state).unwrap();
        write_private(&dirs.state.join("status.json"), b"{}").unwrap();
        assert!(!prepare(&dirs, &f.old()).unwrap());
        assert!(!f.old().join(".xdg-migration-failed").exists());
        assert_eq!(dirs.selected(&f.old()), dirs);
    }

    #[test]
    fn a_second_launch_beside_a_running_island_never_marks_a_failure() {
        let f = Fixture::new();
        f.populate();
        let dirs = f.dirs();
        // The singleton refuses a status file without its socket; this fixture has none.
        fs::remove_file(f.old().join("status.json")).unwrap();
        // Before migration: an older island holds the legacy locks.
        let holder = crate::singleton::InstanceGuard::acquire(&f.old()).unwrap();
        assert!(crate::paths::prepare_live_dirs(dirs.clone(), f.old()).is_err());
        assert!(!f.old().join(".xdg-migration-failed").exists());
        assert!(!dirs.config.exists() && !dirs.state.exists());
        drop(holder);
        assert!(prepare(&dirs, &f.old()).unwrap());
        // After migration: the running island holds every lock; a second start is a no-op.
        let running =
            crate::singleton::InstanceGuard::acquire_with_status(&f.old(), &dirs.state).unwrap();
        let paths = crate::paths::prepare_live_dirs(dirs.clone(), f.old()).unwrap();
        assert_eq!(paths.config_dir, dirs.config);
        assert!(!f.old().join(".xdg-migration-failed").exists());
        assert_eq!(dirs.selected(&f.old()), dirs);
        drop(running);
    }

    #[test]
    fn same_xdg_config_and_state_home_supports_one_atomic_target() {
        let f = Fixture::new();
        f.populate();
        let shared = f.0.join("shared");
        let dirs = DataDirs::resolve(&f.0, Some(&shared), Some(&shared));
        assert!(prepare(&dirs, &f.old()).unwrap());
        assert!(dirs.config.join("settings.json").is_file());
        assert!(dirs.state.join("clipboard.json").is_file());
        assert!(!prepare(&dirs, &f.old()).unwrap());
    }

    #[test]
    fn verification_detects_same_size_corruption() {
        let f = Fixture::new();
        let a = f.0.join("a");
        let b = f.0.join("b");
        fs::write(&a, "aaaa").unwrap();
        fs::write(&b, "aaab").unwrap();
        assert!(verify(&a, &b).is_err());
    }

    #[test]
    fn rust_python_and_shell_resolvers_agree_for_same_environment() {
        let f = Fixture::new();
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bridge = repo.join("island/anchor-client.py");
        let launcher = repo.join("island-tauri/run.sh");
        for (config, state) in [
            (None, None),
            (Some("relative"), Some("also-relative")),
            (Some(""), Some("")),
            (Some("/absolute/config"), Some("/absolute/state")),
        ] {
            // Replace absolute placeholders with private scratch paths; no real XDG IO.
            let c = config.map(|s| {
                if s.starts_with('/') {
                    f.0.join("custom-config")
                } else {
                    PathBuf::from(s)
                }
            });
            let s = state.map(|s| {
                if s.starts_with('/') {
                    f.0.join("custom-state")
                } else {
                    PathBuf::from(s)
                }
            });
            let dirs = DataDirs::resolve(&f.0, c.as_deref(), s.as_deref());
            for phase in 0..5 {
                if phase == 1 {
                    f.populate();
                }
                if phase == 2 {
                    prepare(&dirs, &f.old()).unwrap();
                }
                if phase == 3 {
                    write_private(&f.old().join(".xdg-migration-failed"), b"failure").unwrap();
                }
                if phase == 4 {
                    fs::remove_file(f.old().join(".xdg-migration-failed")).unwrap();
                    write_private(&dirs.config.join(PENDING), b"pending").unwrap();
                }
                let expected = dirs.selected(&f.old());
                let expected = format!(
                    "{}\n{}\n{}\n",
                    expected.config.display(),
                    expected.state.display(),
                    f.old().display()
                );
                for language in ["python", "shell"] {
                    let mut command = if language == "python" {
                        let mut cmd = Command::new("/usr/bin/python3");
                        cmd.args(["-I", "-S", "-c", "import runpy,sys; m=runpy.run_path(sys.argv[1]); print(*m['resolve_data_dirs'](root=sys.argv[2]), sep='\\n')"])
                            .arg(&bridge).arg(f.0.join("repo"));
                        cmd
                    } else {
                        let mut cmd = Command::new("/bin/bash");
                        cmd.args(["-c", "source \"$1\"; monolith_data_dirs \"$2\" 1; printf '%s\\n' \"$MONOLITH_CONFIG\" \"$MONOLITH_STATE\" \"$MONOLITH_RUNTIME\"", "resolver"])
                            .arg(&launcher).arg(f.0.join("repo"));
                        cmd
                    };
                    command
                        .env_clear()
                        .env("PATH", "/usr/bin:/bin")
                        .env("HOME", &f.0);
                    if let Some(c) = &c {
                        command.env("XDG_CONFIG_HOME", c);
                    }
                    if let Some(s) = &s {
                        command.env("XDG_STATE_HOME", s);
                    }
                    let output = command.output().unwrap();
                    assert!(
                        output.status.success(),
                        "{language}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert_eq!(
                        String::from_utf8(output.stdout).unwrap(),
                        expected,
                        "{language} phase {phase}"
                    );
                }
                if phase == 4 {
                    fs::remove_file(dirs.config.join(PENDING)).unwrap();
                }
            }
            fs::remove_dir_all(f.old()).unwrap();
            fs::remove_dir_all(&dirs.config).unwrap();
            if dirs.state != dirs.config {
                fs::remove_dir_all(&dirs.state).unwrap();
            }
        }
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;

    #[test]
    fn startup_failure_returns_old_config_and_state_reports_error_and_retries() {
        let _serial = test_serial();
        let root = crate::services::scratch("xdg-fallback");
        let old = root.join("repo/.island-data-overlay");
        private_dir(&old).unwrap();
        write_private(&old.join("settings.json"), b"saved prefs").unwrap();
        write_private(&old.join(NOTE), b"unrelated file").unwrap();
        let dirs = DataDirs::resolve(&root, None, None);
        let paths = crate::paths::prepare_live_dirs(dirs.clone(), old.clone()).unwrap();
        assert_eq!(paths.data_dir, old);
        assert_eq!(paths.config_dir, old);
        assert_eq!(paths.status, old.join("status.json"));
        assert!(paths.migration_error.is_some());
        assert_eq!(dirs.selected(&old).config, old);
        assert_eq!(fs::read(old.join("settings.json")).unwrap(), b"saved prefs");
        fs::remove_file(old.join(NOTE)).unwrap();
        let paths = crate::paths::prepare_live_dirs(dirs.clone(), old.clone()).unwrap();
        assert_eq!(paths.config_dir, dirs.config);
        assert_eq!(paths.data_dir, dirs.state);
        assert!(paths.migration_error.is_none());
        assert!(!old.join(".xdg-migration-failed").exists());
        assert_eq!(dirs.selected(&old), dirs);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_test_instance_keeps_all_paths_in_scratch_and_leaves_legacy_data_untouched() {
        let _serial = test_serial();
        let root = crate::services::scratch("xdg-test-isolation");
        let old = root.join("repo/.island-data-overlay");
        private_dir(&old).unwrap();
        write_private(&old.join("settings.json"), b"do not migrate").unwrap();
        let scratch = root.join("instance");
        let cli = crate::cli::Cli {
            test_instance: true,
            data_dir: Some(scratch.clone()),
            ..Default::default()
        };
        let paths = crate::paths::resolve(&cli, &root.join("repo")).unwrap();
        assert_eq!(paths.data_dir, scratch);
        assert_eq!(paths.config_dir, scratch);
        assert_eq!(paths.runtime_dir, scratch);
        assert!(paths.migration_error.is_none());
        assert_eq!(fs::read_dir(old).unwrap().count(), 1);
        assert_eq!(fs::read_dir(scratch).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn test_resolver_refuses_xdg_directories_and_symlink_aliases() {
        let _serial = test_serial();
        let root = crate::services::scratch("xdg-test-refusal");
        let dirs = DataDirs::resolve(&root, None, None);
        private_dir(&dirs.config).unwrap();
        private_dir(&dirs.state).unwrap();
        for dir in [&dirs.config, &dirs.state] {
            assert!(crate::paths::refuses_xdg_test_dir(dir, &dirs));
            assert!(crate::paths::refuses_xdg_test_dir(
                &dir.join("nested"),
                &dirs
            ));
        }
        assert!(!crate::paths::refuses_xdg_test_dir(
            &root.join("scratch"),
            &dirs
        ));
        let alias = root.join("state-alias");
        std::os::unix::fs::symlink(&dirs.state, &alias).unwrap();
        let resolved = crate::paths::canonical_lenient(&alias.join("nested")).unwrap();
        assert!(crate::paths::refuses_xdg_test_dir(&resolved, &dirs));
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    #[test]
    fn different_checkouts_cannot_own_shared_state_simultaneously() {
        let _serial = test_serial();
        let root = crate::services::scratch("xdg-ownership");
        let state = root.join("state");
        private_dir(&state).unwrap();
        let a = root.join("checkout-a");
        let b = root.join("checkout-b");
        let first = crate::singleton::InstanceGuard::acquire_with_status(&a, &state).unwrap();
        assert!(crate::singleton::InstanceGuard::acquire_with_status(&b, &state).is_err());
        drop(first);
        assert!(crate::singleton::InstanceGuard::acquire_with_status(&b, &state).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod alias_tests {
    use super::*;
    #[test]
    fn alias_into_legacy_directory_is_refused_without_copying_or_recursing() {
        let _serial = test_serial();
        let root = crate::services::scratch("xdg-overlap");
        let old = root.join("repo/.island-data-overlay");
        private_dir(&old).unwrap();
        write_private(&old.join("settings.json"), b"saved").unwrap();
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&old, &alias).unwrap();
        let dirs = DataDirs {
            config: alias.join("config/monolith"),
            state: root.join("state/monolith"),
        };
        assert!(prepare(&dirs, &old).is_err());
        assert_eq!(fs::read_dir(&old).unwrap().count(), 1);
        assert!(!dirs.state.exists());
        assert!(!old.join("config").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
