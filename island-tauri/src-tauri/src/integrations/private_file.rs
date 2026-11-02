//! Private, bounded instance-only persistence used by the Phase 6b apps.
use crate::error::IslandError;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);

pub fn read(path: &Path, cap: usize) -> Result<Option<Vec<u8>>, IslandError> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(IslandError::io("Private file cannot be opened")),
    };
    let meta = file
        .metadata()
        .map_err(|_| IslandError::io("Private file metadata unavailable"))?;
    if !meta.is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
        || meta.len() > cap as u64
    {
        return Err(IslandError::invalid(
            "Private file permissions, type or size invalid",
        ));
    }
    let mut bytes = Vec::new();
    file.take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| IslandError::io("Private file read failed"))?;
    if bytes.len() > cap {
        return Err(IslandError::invalid("Private file too large"));
    }
    Ok(Some(bytes))
}

pub fn write(path: &Path, bytes: &[u8], cap: usize) -> Result<(), IslandError> {
    if bytes.len() > cap {
        return Err(IslandError::invalid("Private file too large"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| IslandError::invalid("Private file has no parent"))?;
    let temp = parent.join(format!(
        ".apps-{}-{}.tmp",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        if let Err(e) = fs::remove_file(&temp) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Private temporary file cleanup failed");
            }
        }
    }
    result.map_err(|_: std::io::Error| IslandError::io("Private atomic save failed"))
}
