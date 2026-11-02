//! Opt-in pinned-only filesystem adapter. Never called by the state actor.
use super::{
    core_error,
    history::{HistoryConfig, HistoryStore},
};
use crate::error::IslandError;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicU32, Ordering},
};
static SERIAL: AtomicU32 = AtomicU32::new(0);
const MAX_FILE: u64 = 64 * 1024 * 1024;

pub(super) fn load(path: &Path, config: HistoryConfig) -> Result<HistoryStore, IslandError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return HistoryStore::new(config).map_err(core_error)
        }
        Err(_) => return Err(IslandError::io("Cannot open clipboard persistence")),
    };
    let metadata = file
        .metadata()
        .map_err(|_| IslandError::io("Cannot inspect clipboard persistence"))?;
    if !metadata.is_file() || metadata.len() > MAX_FILE {
        return Err(IslandError::invalid(
            "Clipboard persistence exceeds file limit",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no preconditions and does not mutate process state.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(IslandError::invalid(
                "Clipboard persistence must be private and owned by this user",
            ));
        }
    }
    let mut json = String::new();
    file.take(MAX_FILE + 1)
        .read_to_string(&mut json)
        .map_err(|_| IslandError::io("Cannot read clipboard persistence"))?;
    if json.len() as u64 > MAX_FILE {
        return Err(IslandError::invalid(
            "Clipboard persistence exceeds file limit",
        ));
    }
    // Validate real image blobs too: the pure core deliberately trusts its caller's dimensions.
    #[cfg(target_os = "linux")]
    {
        use base64::Engine;
        let mut store = HistoryStore::from_persisted(config, &json).map_err(core_error)?;
        for s in store.list(None, 500) {
            if let Some(super::history::ClipContent::Image {
                png_data_url,
                width,
                height,
            }) = store.get(&s.id)
            {
                let valid = png_data_url.strip_prefix("data:image/png;base64,")
                    .and_then(|v| base64::engine::general_purpose::STANDARD.decode(v).ok())
                    .and_then(|v| super::linux_wayland::decode_png(v).ok())
                    .is_some_and(|p| matches!(p, super::ClipPayload::Image {width:w,height:h,..} if w==width && h==height));
                if !valid {
                    store.delete(&s.id).map_err(core_error)?;
                }
            }
        }
        Ok(store)
    }
    #[cfg(not(target_os = "linux"))]
    HistoryStore::from_persisted(config, &json).map_err(core_error)
}
pub(super) fn save(path: &Path, store: &HistoryStore) -> Result<(), IslandError> {
    let json = store.to_persisted(false).map_err(core_error)?;
    if json.len() as u64 > MAX_FILE {
        return Err(IslandError::invalid(
            "Pinned clipboard persistence exceeds file limit",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| IslandError::io("Invalid clipboard persistence directory"))?;
    fs::create_dir_all(parent)
        .map_err(|_| IslandError::io("Cannot create clipboard data directory"))?;
    let temporary = parent.join(format!(
        ".clipboard.{}.{}.tmp",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let result = (|| {
        let mut file = options
            .open(&temporary)
            .map_err(|_| IslandError::io("Cannot create private clipboard snapshot"))?;
        file.write_all(json.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|_| IslandError::io("Cannot write clipboard snapshot"))?;
        fs::rename(&temporary, path)
            .map_err(|_| IslandError::io("Cannot atomically replace clipboard snapshot"))?;
        Ok(())
    })();
    if result.is_err() {
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(%e,"Clipboard temporary cleanup failed"),
        }
    }
    result
}
pub(super) fn remove(path: &Path) -> Result<(), IslandError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(IslandError::io(
            "Cannot remove disabled clipboard persistence",
        )),
    }
}
