//! Additive Phase 6b2 wire types. Services keep their audited cores private to Rust.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BrightnessDisplay {
    pub name: String,
    pub label: String,
    pub is_internal: bool,
    pub percent: Option<u8>,
    pub dragging: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct BrightnessChanged {
    pub rev: u32,
    pub available: bool,
    pub detail: String,
    pub displays: Vec<BrightnessDisplay>,
}

#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BrightnessOp {
    Drag { name: String, percent: u8 },
    Release { name: String },
    Step { action: BrightnessStep },
}

#[derive(Clone, Copy, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BrightnessStep {
    Increase,
    Decrease,
    IncreaseSmall,
    DecreaseSmall,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentChip {
    pub id: String,
    pub label: String,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub elapsed_secs: f64,
    pub api_call_count: Option<f64>,
    pub is_delegated: bool,
    pub children: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct AgentsChanged {
    pub rev: u32,
    pub available: bool,
    pub detail: String,
    pub chips: Vec<AgentChip>,
    pub overflow: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MixerSink {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub muted: bool,
    pub volume_percent: Option<u32>,
    pub channels: u8,
    pub state: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MixerStream {
    pub index: u32,
    pub sink_index: u32,
    pub app_name: String,
    pub binary: Option<String>,
    pub pid: Option<u32>,
    pub media_name: Option<String>,
    pub icon_name: Option<String>,
    pub muted: bool,
    pub corked: bool,
    pub volume_percent: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MixerApp {
    pub app_name: String,
    pub streams: Vec<u32>,
    pub volume_percent: Option<u32>,
    pub muted: bool,
    pub sink_index: Option<u32>,
    pub idle: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct MixerChanged {
    pub rev: u32,
    pub available: bool,
    pub detail: String,
    pub sinks: Vec<MixerSink>,
    pub streams: Vec<MixerStream>,
    pub apps: Vec<MixerApp>,
    pub default_sink: Option<String>,
}
#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum MixerOp {
    SinkVolume { name: String, percent: u32 },
    SinkMute { name: String, muted: bool },
    StreamVolume { index: u32, percent: u32 },
    StreamMute { index: u32, muted: bool },
    AppVolume { stream: u32, percent: u32 },
    MoveStream { index: u32, sink: String },
    DefaultSink { name: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct TogglesChanged {
    pub rev: u32,
    pub available: bool,
    pub detail: String,
    pub power_available: bool,
    pub power_detail: String,
    pub power_profile: Option<String>,
    pub power_choices: Vec<String>,
    pub degraded: Option<String>,
    pub held_by: Vec<String>,
    pub mic_available: bool,
    pub mic_detail: String,
    pub mic_muted: Option<bool>,
    pub dnd: bool,
    pub power_request: Option<String>,
    pub mic_request: Option<String>,
}
#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TogglesOp {
    PowerProfile { profile: String },
    CycleProfile,
    MicMute { muted: bool },
    Dnd { on: bool },
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackageUpdate {
    pub name: String,
    /// Empty when the installed version is unknown (dnf: rpm doesn't list the package).
    pub old_version: String,
    pub new_version: String,
    pub ignored: bool,
    /// `repo` / `aur` (pacman backend) or `dnf`.
    pub source: String,
    /// RPM architecture for dnf updates (multilib lists `name.arch` separately); null on pacman.
    pub arch: Option<String>,
}
/// Package manager behind the update badge, chosen once from os-release at service start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UpdatesBackend {
    Pacman,
    Dnf,
    /// Image-based or unknown system: nothing is checked.
    Unsupported,
}
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesChanged {
    pub rev: u32,
    pub backend: UpdatesBackend,
    pub available: bool,
    pub detail: String,
    pub count: Option<u32>,
    pub repo_count: Option<u32>,
    pub aur_count: Option<u32>,
    pub needs_reboot: bool,
    pub stale: bool,
    pub error: Option<String>,
    pub items: Vec<PackageUpdate>,
    pub age_secs: Option<f64>,
    pub next_check_in_secs: f64,
}
#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum UpdatesOp {
    Refresh,
}
