//! Atomic status snapshots as a REGULAR 0600 file (SPEC §8 Phase 3, revised after review).
//!
//! Legacy readers, rollback tools and crash recovery all expect `status.json` to be a regular
//! file. Ownership is the two singleton locks (`island.lock` + `instance.lock`, held by the
//! `InstanceGuard` that owns this writer), not the file's name: each update writes a temp file
//! IN THE SAME DIRECTORY and `rename`s it over `status.json`. On top of that contract the writer
//! stays fail-closed: it refuses to publish over anything that is not what it last wrote, and
//! it only ever removes the exact file it published.
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const LIMIT: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Identity(pub u64, pub u64);
impl Identity {
    pub(crate) fn at(path: &Path) -> Result<Self> {
        let m = fs::symlink_metadata(path)?;
        Ok(Self(m.dev(), m.ino()))
    }
    pub(crate) fn matches(self, path: &Path) -> bool {
        Self::at(path).is_ok_and(|current| current == self)
    }
}

struct Published {
    identity: Identity,
    bytes: Vec<u8>,
}

/// Keep this writer inside the instance guard, after acquiring both singleton locks.
/// No status path is claimed until `update` publishes its first complete snapshot.
pub struct StatusFile {
    data: PathBuf,
    path: PathBuf,
    published: Option<Published>,
    closed: bool,
}
impl StatusFile {
    pub fn new(data: &Path) -> Result<Self> {
        let meta = fs::symlink_metadata(data)?;
        // SAFETY: geteuid has no preconditions.
        ensure!(
            meta.is_dir()
                && meta.uid() == unsafe { libc::geteuid() }
                && meta.permissions().mode() & 0o077 == 0,
            "Status directory is not private"
        );
        Ok(Self {
            data: data.to_owned(),
            path: data.join("status.json"),
            published: None,
            closed: false,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Is `status.json` still exactly the file this writer published?
    fn still_ours(&self) -> bool {
        let Some(published) = &self.published else {
            return false;
        };
        if !published.identity.matches(&self.path) {
            return false;
        }
        let Ok(file) = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(&self.path)
        else {
            return false;
        };
        let mut bytes = Vec::with_capacity(published.bytes.len() + 1);
        file.take(published.bytes.len() as u64 + 1)
            .read_to_end(&mut bytes)
            .is_ok()
            && bytes == published.bytes
    }
    pub fn update(&mut self, snapshot: &Value) -> Result<()> {
        ensure!(!self.closed, "Status writer is closed");
        ensure!(
            snapshot.is_object() && snapshot["pid"].as_u64() == Some(u64::from(std::process::id())),
            "Status snapshot must name this process"
        );
        if self.published.is_some() {
            ensure!(self.still_ours(), "Status ownership was replaced");
        } else {
            match fs::symlink_metadata(&self.path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => anyhow::bail!("Refusing to overwrite an existing status"),
            }
        }
        let bytes = serde_json::to_vec(snapshot)?;
        ensure!(bytes.len() <= LIMIT, "Status snapshot exceeds limit");
        // Same directory as the destination, so `rename` is atomic on one filesystem.
        let temp = self.data.join(format!(
            ".status.json.{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let publish = (|| -> Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&temp)
                .context("Creating pending status")?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temp, &self.path).context("Publishing status")?;
            File::open(&self.data)?.sync_all()?;
            Ok(())
        })();
        if publish.is_err() {
            if let Err(error) = fs::remove_file(&temp) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(%error, "Cannot remove own pending status snapshot");
                }
            }
        }
        publish?;
        self.published = Some(Published {
            identity: Identity::at(&self.path)?,
            bytes,
        });
        Ok(())
    }
    /// Explicit shutdown reports errors; Drop also attempts cleanup and logs failures.
    /// Removes `status.json` only when it is byte-for-byte and inode-for-inode what this writer
    /// last published: a replacement (even one naming our PID) is never deleted.
    pub fn shutdown(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        if self.still_ours() {
            fs::remove_file(&self.path).context("Removing owned status file")?;
        }
        self.closed = true;
        Ok(())
    }
}
impl Drop for StatusFile {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::warn!(%error, "Status cleanup failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::singleton::tests::Fixture;
    use serde_json::json;
    use std::os::unix::fs::symlink;

    #[test]
    fn status_is_a_regular_private_file_updated_by_same_directory_rename() {
        let fixture = Fixture::new();
        let mut writer = StatusFile::new(&fixture.0).unwrap();
        let mut inodes = Vec::new();
        for mode in ["idle", "preview", "pinned"] {
            let snapshot = json!({"pid":std::process::id(),"mode":mode,"updated":1});
            writer.update(&snapshot).unwrap();
            let meta = fs::symlink_metadata(writer.path()).unwrap();
            assert!(meta.is_file(), "status.json must be a regular file");
            assert_eq!(meta.permissions().mode() & 0o777, 0o600);
            assert_eq!(meta.nlink(), 1);
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(writer.path()).unwrap()).unwrap(),
                snapshot
            );
            inodes.push(meta.ino());
        }
        // Rename-based publication: every update is a NEW inode (atomic replace), and no
        // temp file survives in the directory.
        assert!(
            inodes[0] != inodes[1] && inodes[1] != inodes[2],
            "{inodes:?}"
        );
        let leftovers: Vec<_> = fs::read_dir(&fixture.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        writer.shutdown().unwrap();
        assert!(fs::symlink_metadata(writer.path()).is_err());
    }

    #[test]
    fn a_reader_never_observes_a_partial_snapshot() {
        let fixture = Fixture::new();
        let mut writer = StatusFile::new(&fixture.0).unwrap();
        let path = writer.path().to_owned();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let reader = std::thread::spawn(move || {
            let mut seen = 0u32;
            while !flag.load(Ordering::Relaxed) {
                if let Ok(bytes) = fs::read(&path) {
                    let value: Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(value["pad"].as_str().unwrap().len(), 4096);
                    seen += 1;
                }
            }
            seen
        });
        for i in 0..200 {
            writer
                .update(&json!({"pid":std::process::id(),"i":i,"pad":"x".repeat(4096)}))
                .unwrap();
        }
        stop.store(true, Ordering::Relaxed);
        assert!(reader.join().unwrap() > 0);
    }

    #[test]
    fn atomic_status_updates_preserve_foreign_replacements_even_with_same_pid() {
        let fixture = Fixture::new();
        let mut writer = StatusFile::new(&fixture.0).unwrap();
        writer
            .update(&json!({"pid":std::process::id(),"mode":"idle"}))
            .unwrap();
        fs::remove_file(writer.path()).unwrap();
        let foreign = json!({"pid":std::process::id(),"foreign":true}).to_string();
        fs::write(writer.path(), &foreign).unwrap();
        assert!(writer.update(&json!({"pid":std::process::id()})).is_err());
        writer.shutdown().unwrap();
        assert_eq!(fs::read_to_string(writer.path()).unwrap(), foreign);
    }

    #[test]
    fn in_place_overwrite_of_our_inode_is_also_detected_and_preserved() {
        let fixture = Fixture::new();
        let mut writer = StatusFile::new(&fixture.0).unwrap();
        writer
            .update(&json!({"pid":std::process::id(),"mode":"idle"}))
            .unwrap();
        let foreign = json!({"pid":std::process::id(),"mode":"HIJACK"}).to_string();
        fs::write(writer.path(), &foreign).unwrap(); // same inode, different bytes
        assert!(writer.update(&json!({"pid":std::process::id()})).is_err());
        writer.shutdown().unwrap();
        assert_eq!(fs::read_to_string(writer.path()).unwrap(), foreign);
    }

    #[test]
    fn status_initial_publication_never_overwrites_existing_file_or_symlink() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("target"), "foreign").unwrap();
        symlink("target", fixture.0.join("status.json")).unwrap();
        let mut writer = StatusFile::new(&fixture.0).unwrap();
        assert!(writer.update(&json!({"pid":std::process::id()})).is_err());
        writer.shutdown().unwrap();
        assert_eq!(
            fs::read_to_string(fixture.0.join("status.json")).unwrap(),
            "foreign"
        );
        // a regular pre-existing file is refused too, and left untouched
        fs::remove_file(fixture.0.join("status.json")).unwrap();
        fs::write(fixture.0.join("status.json"), "regular").unwrap();
        let mut writer = StatusFile::new(&fixture.0).unwrap();
        assert!(writer.update(&json!({"pid":std::process::id()})).is_err());
        assert_eq!(
            fs::read_to_string(fixture.0.join("status.json")).unwrap(),
            "regular"
        );
    }

    #[test]
    fn snapshot_must_name_this_process_and_respect_the_size_limit() {
        let fixture = Fixture::new();
        let mut writer = StatusFile::new(&fixture.0).unwrap();
        assert!(writer.update(&json!({"pid":1})).is_err());
        assert!(writer.update(&json!([1])).is_err());
        let huge = json!({"pid":std::process::id(),"pad":"x".repeat(LIMIT)});
        assert!(writer.update(&huge).is_err());
        assert!(fs::symlink_metadata(writer.path()).is_err());
    }
}
