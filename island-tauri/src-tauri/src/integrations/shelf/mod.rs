//! Drop shelf: files dragged onto the notch are parked here, as paths only, and shown on the
//! island's Shelf page. The notch hands the dropped URIs over D-Bus (`/Shelf`
//! `local.hermes.Monolith.Shelf.Add(as) → u`, served next to `/Companion`), so no shell ever
//! sees a file name. Nothing watches or polls: each item is re-checked with
//! `symlink_metadata` whenever the list is read, and `shelf.json` is written only when the
//! list changes.
pub mod core;

use crate::{
    bus::Latest,
    contract::apps::{ShelfChanged, ShelfOp, ShelfRow},
    error::IslandError,
    integrations::downloads::Opener,
};
use self::core::{Kind, Shelf};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const FILE: &str = "shelf.json";
const FILE_CAP: usize = 512 * 1024;

pub fn stat(path: &Path) -> Option<Kind> {
    let meta = fs::symlink_metadata(path).ok()?;
    let kind = meta.file_type();
    Some(if kind.is_symlink() {
        Kind::Symlink
    } else if kind.is_file() {
        Kind::File
    } else if kind.is_dir() {
        Kind::Folder
    } else {
        Kind::Other
    })
}

struct State {
    shelf: Shelf,
    enabled: bool,
    max: usize,
    rev: u32,
}

pub struct ShelfStore {
    file: PathBuf,
    opener: Arc<dyn Opener>,
    state: Mutex<State>,
    pub topic: Latest<ShelfChanged>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl ShelfStore {
    /// Loads `shelf.json` from the island's data dir (a missing or unreadable file is empty).
    pub fn new(data: &Path, opener: Arc<dyn Opener>) -> Arc<Self> {
        let file = data.join(FILE);
        let max = 12;
        let shelf = match crate::integrations::private_file::read(&file, FILE_CAP) {
            Ok(Some(bytes)) => Shelf::from_json(&String::from_utf8_lossy(&bytes), max),
            Ok(None) => Shelf::default(),
            Err(error) => {
                tracing::warn!(%error, "Shelf list unreadable; starting empty");
                Shelf::default()
            }
        };
        let store = Arc::new(Self {
            file,
            opener,
            state: Mutex::new(State {
                shelf,
                enabled: true,
                max,
                rev: 0,
            }),
            topic: Latest::new(),
        });
        if let Ok(state) = store.lock() {
            store.topic.publish(snapshot(&state));
        }
        store
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, IslandError> {
        self.state
            .lock()
            .map_err(|_| IslandError::internal("Shelf state poisoned"))
    }

    /// `shelfEnabled` and `shelfMaxItems`. Lowering the cap drops the oldest items.
    pub fn configure(&self, enabled: bool, max_items: f64) {
        let Ok(mut state) = self.lock() else { return };
        let max = core::clamp_max(max_items);
        let trimmed = state.shelf.trim(max);
        if state.enabled == enabled && state.max == max && !trimmed {
            return;
        }
        state.enabled = enabled;
        state.max = max;
        self.changed(&mut state, trimmed);
    }

    fn changed(&self, state: &mut State, save: bool) {
        state.rev = state.rev.wrapping_add(1);
        if save {
            if let Err(error) = crate::integrations::private_file::write(
                &self.file,
                state.shelf.to_json().as_bytes(),
                FILE_CAP,
            ) {
                tracing::warn!(%error, "Shelf list not saved");
            }
        }
        self.topic.publish(snapshot(state));
    }

    /// A drop from the notch. Every URI is validated (see `core::parse_uri`); the invalid
    /// ones are skipped. More than `MAX_URIS` refuses the whole drop. Returns how many items
    /// were parked; 0 while the shelf is off.
    pub fn add_uris(&self, uris: &[String]) -> u32 {
        if uris.is_empty() || uris.len() > core::MAX_URIS {
            return 0;
        }
        let Ok(mut state) = self.lock() else { return 0 };
        if !state.enabled {
            return 0;
        }
        let mut paths = Vec::new();
        for uri in uris {
            match core::accept(uri, stat) {
                Ok((path, _)) => {
                    if !paths.contains(&path) {
                        paths.push(path);
                    }
                }
                Err(reason) => tracing::info!(?reason, "Shelf refused a dropped item"),
            }
        }
        let accepted = paths.len().min(state.max) as u32;
        if paths.is_empty() {
            return 0;
        }
        let max = state.max;
        if state.shelf.add(paths, now_ms(), max) {
            self.changed(&mut state, true);
        }
        accepted
    }

    pub fn current(&self) -> Result<ShelfChanged, IslandError> {
        Ok(snapshot(&*self.lock()?))
    }

    fn path_of(state: &State, id: &str) -> Result<PathBuf, IslandError> {
        let path = core::parse_uri(id).map_err(|_| IslandError::invalid("Invalid shelf item"))?;
        state
            .shelf
            .items()
            .iter()
            .any(|i| i.path == path)
            .then_some(path)
            .ok_or_else(|| IslandError::unavailable("No longer on the shelf"))
    }

    pub fn command(&self, op: ShelfOp) -> Result<ShelfChanged, IslandError> {
        let mut state = self.lock()?;
        match op {
            ShelfOp::Remove { id } => {
                let path = Self::path_of(&state, &id)?;
                if state.shelf.remove(&path) {
                    self.changed(&mut state, true);
                }
            }
            ShelfOp::Clear {} => {
                if state.shelf.clear() {
                    self.changed(&mut state, true);
                }
            }
            ShelfOp::Open { id } => {
                let path = Self::path_of(&state, &id)?;
                drop(state);
                self.launch(false, path)?;
                return self.current();
            }
            ShelfOp::Reveal { id } => {
                let path = Self::path_of(&state, &id)?;
                drop(state);
                self.launch(true, path)?;
                return self.current();
            }
        }
        Ok(snapshot(&state))
    }
}

impl ShelfStore {
    /// Open = the default app (`xdg-open`); Reveal = the file manager with the item selected.
    /// The path comes from the shelf, never from the page, and is re-checked first.
    fn launch(&self, reveal: bool, path: PathBuf) -> Result<(), IslandError> {
        if !matches!(stat(&path), Some(Kind::File | Kind::Folder)) {
            return Err(IslandError::unavailable("Moved or deleted"));
        }
        let argv = if reveal {
            vec![
                OsString::from("dolphin"),
                OsString::from("--select"),
                path.into_os_string(),
            ]
        } else {
            vec![OsString::from("xdg-open"), path.into_os_string()]
        };
        self.opener.open(&argv)
    }
}

fn snapshot(state: &State) -> ShelfChanged {
    ShelfChanged {
        rev: state.rev,
        enabled: state.enabled,
        max_items: state.max as u32,
        items: state
            .shelf
            .items()
            .iter()
            .map(|item| {
                let kind = stat(&item.path);
                let uri = core::encode_uri(&item.path);
                ShelfRow {
                    id: uri.clone(),
                    name: item
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| item.path.to_string_lossy().into_owned()),
                    path: item.path.to_string_lossy().into_owned(),
                    uri,
                    folder: kind == Some(Kind::Folder),
                    present: matches!(kind, Some(Kind::File | Kind::Folder)),
                    added_ms: item.added_ms as f64,
                }
            })
            .collect(),
    }
}
