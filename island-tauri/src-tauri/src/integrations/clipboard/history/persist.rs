use super::model::ClipPayload;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Current persistence format version.
pub const PERSIST_VERSION: u64 = 1;

/// Internal serde mirror of `ClipPayload` used for persistence. This is NOT
/// the IPC boundary type (that is `ClipContent`, which carries a base64 data
/// URL instead of raw PNG bytes).
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum StoredPayload {
    Text {
        text: String,
    },
    Image {
        png: Vec<u8>,
        width: u32,
        height: u32,
    },
    Files {
        uris: Vec<String>,
    },
}

impl From<&ClipPayload> for StoredPayload {
    fn from(payload: &ClipPayload) -> Self {
        match payload {
            ClipPayload::Text(text) => StoredPayload::Text { text: text.clone() },
            ClipPayload::Image { png, width, height } => StoredPayload::Image {
                png: png.clone(),
                width: *width,
                height: *height,
            },
            ClipPayload::Files(uris) => StoredPayload::Files { uris: uris.clone() },
        }
    }
}

impl From<StoredPayload> for ClipPayload {
    fn from(stored: StoredPayload) -> Self {
        match stored {
            StoredPayload::Text { text } => ClipPayload::Text(text),
            StoredPayload::Image { png, width, height } => {
                ClipPayload::Image { png, width, height }
            }
            StoredPayload::Files { uris } => ClipPayload::Files(uris),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredEntry {
    pub id: String,
    pub payload: StoredPayload,
    pub pinned: bool,
    pub copied_at_ms: f64,
    pub source_app: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredFile {
    pub version: u64,
    pub next_id: u64,
    pub entries: Vec<StoredEntry>,
}

/// Borrowed serialisation mirror of [`StoredPayload`]: byte-identical JSON, no payload copy.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum PayloadRef<'a> {
    Text {
        text: &'a str,
    },
    Image {
        png: &'a [u8],
        width: u32,
        height: u32,
    },
    Files {
        uris: &'a [String],
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EntryRef<'a> {
    id: &'a str,
    payload: PayloadRef<'a>,
    pinned: bool,
    copied_at_ms: f64,
    source_app: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileRef<'a> {
    version: u64,
    next_id: u64,
    entries: Vec<EntryRef<'a>>,
}

#[derive(Debug)]
pub(crate) struct SnapshotEntry {
    pub(crate) id: String,
    pub(crate) payload: Arc<ClipPayload>,
    pub(crate) copied_at_ms: f64,
    pub(crate) source_app: Option<String>,
}

/// What a persisted snapshot contains, cheap to compare: the id counter plus the pinned
/// entries (id, content hash) in order. Two snapshots with equal keys serialise identically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistKey {
    next_id: u64,
    entries: Vec<(String, u64)>,
}

impl PersistKey {
    /// Same pinned entries in the same order (the id counter may differ).
    pub fn same_entries(&self, other: &PersistKey) -> bool {
        self.entries == other.entries
    }
}

/// A point-in-time copy of the persisted (pinned-only) state. Taking it costs one `Arc`
/// clone per pinned entry, so it is safe to take under a lock; serialising it
/// ([`PersistSnapshot::to_json`]) is the expensive part and needs no lock.
#[derive(Debug)]
pub struct PersistSnapshot {
    next_id: u64,
    entries: Vec<SnapshotEntry>,
    key: PersistKey,
}

impl PersistSnapshot {
    pub(crate) fn new(next_id: u64, entries: Vec<(SnapshotEntry, u64)>) -> Self {
        let key = PersistKey {
            next_id,
            entries: entries
                .iter()
                .map(|(e, hash)| (e.id.clone(), *hash))
                .collect(),
        };
        Self {
            next_id,
            entries: entries.into_iter().map(|(e, _)| e).collect(),
            key,
        }
    }

    pub fn key(&self) -> &PersistKey {
        &self.key
    }

    /// The same JSON `HistoryStore::to_persisted(false)` produces for this state.
    pub fn to_json(&self) -> Result<String, super::error::Error> {
        let file = FileRef {
            version: PERSIST_VERSION,
            next_id: self.next_id,
            entries: self
                .entries
                .iter()
                .map(|e| EntryRef {
                    id: &e.id,
                    payload: match &*e.payload {
                        ClipPayload::Text(text) => PayloadRef::Text { text },
                        ClipPayload::Image { png, width, height } => PayloadRef::Image {
                            png,
                            width: *width,
                            height: *height,
                        },
                        ClipPayload::Files(uris) => PayloadRef::Files { uris },
                    },
                    pinned: true,
                    copied_at_ms: e.copied_at_ms,
                    source_app: e.source_app.as_deref(),
                })
                .collect(),
        };
        serde_json::to_string(&file)
            .map_err(|e| super::error::Error::Corrupt(format!("serialize: {e}")))
    }
}
