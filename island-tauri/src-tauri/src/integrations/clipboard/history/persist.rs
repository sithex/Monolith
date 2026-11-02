use super::model::ClipPayload;
use serde::{Deserialize, Serialize};

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
