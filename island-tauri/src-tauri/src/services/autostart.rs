//! XDG autostart entry (port of island-rust `set_autostart` + its rollback semantics).
//!
//! The default launcher stays `island-rust/run.sh`; the Tauri binary is selected only in
//! Phase 9 (SPEC §7). Test instances REFUSE autostart changes so a GUI test can never touch a
//! real home directory.

use crate::error::IslandError;
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Autostart {
    home: PathBuf,
    root: PathBuf,
    launcher: PathBuf,
    refuse: bool,
}

impl Autostart {
    /// `launcher` is trusted host configuration only; it is never exposed to the renderer.
    pub fn new(home: PathBuf, root: PathBuf, launcher: PathBuf, refuse: bool) -> Self {
        Self {
            home,
            root,
            launcher,
            refuse,
        }
    }

    /// Default launcher: `<root>/island-rust/run.sh`.
    pub fn with_default_launcher(home: PathBuf, root: PathBuf, refuse: bool) -> Self {
        let launcher = root.join("island-rust/run.sh");
        Self::new(home, root, launcher, refuse)
    }

    pub fn entry_path(&self) -> PathBuf {
        self.home.join(".config/autostart/hermes-island.desktop")
    }

    pub fn is_refused(&self) -> bool {
        self.refuse
    }

    /// Current entry bytes (`None` = no entry) for rollback.
    pub fn snapshot(&self) -> Result<Option<Vec<u8>>, IslandError> {
        if self.refuse {
            return Err(refused());
        }
        match fs::read(self.entry_path()) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(IslandError::io("Cannot snapshot autostart entry")),
        }
    }

    pub fn set(&self, enabled: bool) -> Result<(), IslandError> {
        if self.refuse {
            return Err(refused());
        }
        let file = self.entry_path();
        if !enabled {
            return remove_entry(&file);
        }
        let path = self.root.to_string_lossy();
        if path.contains(['\n', '\r']) {
            return Err(IslandError::invalid("Invalid launcher path"));
        }
        let exec = self
            .launcher
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
            .replace('%', "%%");
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=Hermes Island\nComment=Hermes Dynamic Island overlay (Rust launcher)\nExec=\"{exec}\" --overlay\nPath={path}\nTerminal=false\nX-KDE-autostart-phase=2\n"
        );
        atomic_write(&file, entry.as_bytes(), "Cannot write autostart entry")
    }

    /// Put back exactly what `snapshot` saw.
    pub fn restore(&self, prior: Option<&[u8]>) -> Result<(), IslandError> {
        match prior {
            Some(bytes) => {
                atomic_write(&self.entry_path(), bytes, "Cannot restore autostart entry")
            }
            None => remove_entry(&self.entry_path()),
        }
    }
}

fn refused() -> IslandError {
    IslandError::unsupported("autostart changes are refused in --test-instance runs")
}

fn remove_entry(file: &Path) -> Result<(), IslandError> {
    match fs::remove_file(file) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(IslandError::io("Cannot remove autostart entry")),
    }
}

/// Temp file (0600, `create_new`) + fsync + rename + directory fsync.
pub fn atomic_write(file: &Path, bytes: &[u8], failure: &str) -> Result<(), IslandError> {
    let parent = file
        .parent()
        .ok_or_else(|| IslandError::invalid("Invalid storage path"))?;
    fs::create_dir_all(parent).map_err(|_| IslandError::io("Cannot create storage directory"))?;
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.new", std::process::id()));
    let tmp = parent.join(name);
    let result = (|| -> std::io::Result<()> {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        output.write_all(bytes)?;
        output.sync_all()?;
        fs::rename(&tmp, file)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        if let Err(error) = fs::remove_file(&tmp) {
            tracing::debug!(%error, "temp file cleanup failed");
        }
    }
    result.map_err(|_| IslandError::io(failure))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::scratch;

    #[test]
    fn writes_quoted_native_entry_and_removes_it() {
        let dir = scratch("auto");
        let root = dir.join("app with spaces");
        let auto = Autostart::new(
            dir.clone(),
            root.clone(),
            root.join("island-native/run.sh"),
            false,
        );
        auto.set(true).unwrap();
        let desktop = fs::read_to_string(auto.entry_path()).unwrap();
        assert!(desktop.contains(&format!(
            "Exec=\"{}/island-native/run.sh\" --overlay",
            root.display()
        )));
        assert!(!desktop.contains("island-rust/run.sh"));
        auto.set(false).unwrap();
        assert!(!auto.entry_path().exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn default_launcher_is_island_rust_and_refusal_touches_nothing() {
        let dir = scratch("auto-refuse");
        let root = dir.join("app");
        let auto = Autostart::with_default_launcher(dir.clone(), root.clone(), false);
        auto.set(true).unwrap();
        let desktop = fs::read_to_string(auto.entry_path()).unwrap();
        assert!(desktop.contains(&format!(
            "Exec=\"{}/island-rust/run.sh\" --overlay",
            root.display()
        )));
        fs::remove_dir_all(dir.join(".config")).unwrap();
        let refused = Autostart::with_default_launcher(dir.clone(), root, true);
        assert_eq!(
            refused.set(true).unwrap_err().code,
            crate::error::ErrorCode::Unsupported
        );
        assert!(refused.snapshot().is_err());
        assert!(!dir.join(".config").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
