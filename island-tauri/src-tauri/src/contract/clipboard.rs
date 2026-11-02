//! Clipboard wire shapes (SPEC §3.3/§5.2); monitoring is default-off.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ClipKind {
    Text,
    Image,
    Files,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClipSummary {
    pub rev: u32,
    pub id: String,
    pub kind: ClipKind,
    pub preview: String,
    pub bytes: u32,
    pub pinned: bool,
    pub copied_at: f64,
    pub source_app: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
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

#[derive(Serialize, Clone, Debug, PartialEq, Eq, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardStatus {
    pub available: bool,
    pub enabled: bool,
    pub paused: bool,
    pub detail: String,
    pub watcher_running: bool,
    pub connections_open: u32,
    pub connection_attempts: u32,
    pub dropped: u32,
    pub captured: u32,
    pub sensitive_skipped: u32,
    pub denied: u32,
    pub echoes: u32,
}

#[derive(Serialize, Clone, Debug, specta::Type, tauri_specta::Event)]
pub struct ClipboardAdded(pub ClipSummary);

#[derive(Serialize, Clone, Debug, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardStatusChanged {
    pub rev: u32,
    pub status: ClipboardStatus,
}
