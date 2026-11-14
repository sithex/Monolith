//! Additive Phase 6b1 app contracts. Core u64/i64 values are converted here, never exported.
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone, Debug, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CalendarQuery {
    pub start_s: f64,
    pub end_s: f64,
    pub local_offset_s: i32,
}
#[derive(Serialize, Clone, Debug, Default, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppsSnapshot {
    pub timer: Option<TimerChanged>,
    pub scratchpad: Option<ScratchpadChanged>,
    pub downloads: Option<DownloadsChanged>,
    pub transfers: Option<TransfersChanged>,
    pub calendar: Option<CalendarChanged>,
    pub brightness: Option<super::controls::BrightnessChanged>,
    pub agents: Option<super::controls::AgentsChanged>,
    pub mixer: Option<super::controls::MixerChanged>,
    pub toggles: Option<super::controls::TogglesChanged>,
    pub updates: Option<super::controls::UpdatesChanged>,
    pub tiles: Option<super::tiles::TilesChanged>,
}

#[derive(Deserialize, Clone, Copy, Debug, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TimerKind {
    Countdown,
    Pomodoro,
    Stopwatch,
}
#[derive(Deserialize, Clone, Debug, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TimerOp {
    SetCountdown {
        duration_ms: u32,
    },
    Start {
        kind: TimerKind,
    },
    Pause {
        kind: TimerKind,
    },
    Resume {
        kind: TimerKind,
    },
    Cancel {
        kind: TimerKind,
    },
    AddTime {
        duration_ms: u32,
    },
    Skip,
    Lap,
    ConfigurePomodoro {
        focus_ms: u32,
        short_break_ms: u32,
        long_break_ms: u32,
        rounds: u32,
        auto_start: bool,
    },
}
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TimerLap {
    pub number: f64,
    pub elapsed_ms: f64,
}
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TimerCompletion {
    Countdown,
    Pomodoro,
}
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct TimerChanged {
    pub rev: u32,
    pub completed_kind: Option<TimerCompletion>,
    pub available: bool,
    pub detail: String,
    pub sampled_at_ms: f64,
    pub countdown_status: String,
    pub remaining_ms: f64,
    pub pomodoro_status: String,
    pub phase: String,
    pub phase_remaining_ms: f64,
    pub round: u32,
    pub stopwatch_status: String,
    pub elapsed_ms: f64,
    pub laps: Vec<TimerLap>,
}

#[derive(Deserialize, Clone, Debug, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ScratchpadOp {
    Create { id: String },
    Edit { id: String, body: String },
    Delete { id: String },
    Pin { id: String, pinned: bool },
}
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppNote {
    pub id: String,
    pub title: String,
    pub body: String,
    pub created_ms: f64,
    pub updated_ms: f64,
    pub pinned: bool,
}
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ScratchpadChanged {
    pub rev: u32,
    pub available: bool,
    pub detail: String,
    pub dirty: bool,
    pub notes: Vec<AppNote>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRow {
    pub name: String,
    pub size_bytes: f64,
    pub mtime_ms: f64,
    pub kind: String,
    pub state: String,
}
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsChanged {
    pub rev: u32,
    pub available: bool,
    pub detail: String,
    pub items: Vec<DownloadRow>,
}
/// One KDE file job (copy, move, download through KIO) seen on the session bus.
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferRow {
    /// Opaque id for `transfer_reveal` (the job's object path).
    pub id: String,
    /// The app's desktop entry or name, e.g. `org.kde.dolphin`.
    pub app: String,
    pub title: String,
    /// 0..=100, or -1 while the job has not reported one.
    pub percent: f64,
    pub processed_bytes: f64,
    pub total_bytes: f64,
    pub processed_files: f64,
    pub total_files: f64,
    /// Bytes per second (0 = unknown).
    pub speed: f64,
    pub suspended: bool,
    /// Local destination folder shown on the card ("" = not a local path).
    pub destination: String,
}
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct TransfersChanged {
    pub rev: u32,
    pub available: bool,
    pub detail: String,
    pub items: Vec<TransferRow>,
}
#[derive(Deserialize, Clone, Copy, Debug, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OpenDownload {
    Open,
    Reveal,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CalendarSource {
    Local,
    Subscription,
}
#[derive(Deserialize, Clone, Debug, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CalendarOp {
    /// A bounded single-event calendar document. Parsed by the accepted ICS core.
    Create {
        ics: String,
    },
    Edit {
        source: CalendarSource,
        uid: String,
        ics: String,
    },
    Delete {
        source: CalendarSource,
        uid: String,
    },
}
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgendaRow {
    pub uid: String,
    pub summary: String,
    pub location: Option<String>,
    pub start_s: f64,
    pub end_s: f64,
    pub all_day: bool,
    pub source: CalendarSource,
    pub read_only: bool,
}
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct CalendarChanged {
    pub rev: u32,
    pub available: bool,
    pub detail: String,
    pub configured: bool,
    pub stale: bool,
    pub age_ms: Option<f64>,
    pub updated_at: Option<f64>,
    pub agenda: Vec<AgendaRow>,
}
