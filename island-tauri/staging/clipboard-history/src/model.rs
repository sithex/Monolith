use serde::{Deserialize, Serialize};

/// A clipboard offer ingested by the store.
#[derive(Clone, Debug)]
pub struct ClipInput {
    pub content: ClipPayload,
    pub mime_types: Vec<String>,
    pub password_hint: Option<String>,
    pub source_app: Option<String>,
}

/// The payload of a clipboard offer / stored entry.
#[derive(Clone, Debug)]
pub enum ClipPayload {
    Text(String),
    Image {
        png: Vec<u8>,
        width: u32,
        height: u32,
    },
    Files(Vec<String>),
}

/// Kind tag used in `ClipSummary` (serialized as lowercase: `text|image|files`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClipKind {
    Text,
    Image,
    Files,
}

/// IPC boundary summary type (SPEC §5.2 `clipboard.rs`).
///
/// Serde shape rules (§5.1): camelCase, no `u64`/`i64`/`usize`, epoch
/// milliseconds as `f64`, ids as `String`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClipSummary {
    pub id: String,
    pub kind: ClipKind,
    pub preview: String,
    pub bytes: u32,
    pub pinned: bool,
    pub copied_at: f64,
    pub source_app: Option<String>,
}

/// IPC boundary content type (SPEC §5.2 `clipboard.rs`), tagged by `type`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum ClipContent {
    Text {
        text: String,
    },
    Image {
        png_data_url: String,
        width: u32,
        height: u32,
    },
    Files {
        uris: Vec<String>,
    },
}

/// Reasons an offer was not added as a new entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Monitoring is paused.
    Paused,
    /// Password-hint or sensitive-MIME protected.
    Sensitive,
    /// Matched a user deny regex.
    Denied,
    /// Over a size limit.
    TooLarge,
    /// Empty / whitespace-only text, or zero files.
    Empty,
    /// Self-echo window hit (consumed; the entry was reordered).
    Echo,
    /// At capacity with every entry pinned.
    Full,
}

/// Result of `HistoryStore::offer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfferOutcome {
    /// A genuinely new entry was inserted.
    Added { id: String },
    /// Content already existed; the existing entry was promoted to the front
    /// (its id and pinned state are preserved).
    Reordered { id: String },
    /// The offer was skipped.
    Skipped(SkipReason),
}
