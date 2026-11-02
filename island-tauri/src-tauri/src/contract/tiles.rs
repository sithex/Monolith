//! Typed system-tiles wire contract. Unknown observations remain nullable.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeepAwakeTile {
    pub available: bool,
    pub detail: String,
    pub on: Option<bool>,
    pub remaining_ms: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OutputChoice {
    pub name: String,
    pub description: String,
    pub muted: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OutputTile {
    pub available: bool,
    pub detail: String,
    pub current: Option<String>,
    pub choices: Vec<OutputChoice>,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NetworkTile {
    pub available: bool,
    pub detail: String,
    pub state: Option<String>,
    pub connectivity: Option<String>,
    pub kind: Option<String>,
    pub show_wifi: bool,
    pub wifi_enabled: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TailPeer {
    pub host_name: String,
    pub online: Option<bool>,
    pub os: String,
    pub last_seen: Option<String>,
    pub first_ip: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleTile {
    pub available: bool,
    pub detail: String,
    pub backend: Option<String>,
    pub self_name: Option<String>,
    pub peers: Vec<TailPeer>,
    pub online_count: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceTile {
    pub available: bool,
    pub detail: String,
    pub active: Option<String>,
    pub choices: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct TilesChanged {
    pub rev: u32,
    pub available: bool,
    pub detail: String,
    pub keep_awake: KeepAwakeTile,
    pub output: OutputTile,
    pub network: NetworkTile,
    pub tailscale: TailscaleTile,
    pub voice: VoiceTile,
    pub picker_available: bool,
    pub picker_busy: bool,
    pub screenshot_busy: bool,
    pub last_colour: Option<String>,
    pub action_error: Option<String>,
}
#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TilesOp {
    Refresh,
    KeepAwake { on: bool, duration_ms: Option<u32> },
    Output { name: String },
    PickColour,
    Screenshot,
    Voice { name: String },
}
