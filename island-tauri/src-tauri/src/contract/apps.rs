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
    pub shelf: Option<ShelfChanged>,
    pub autoclicker: Option<AutoclickerChanged>,
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
/// One parked path on the drop shelf. Paths only: the shelf never reads file contents.
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShelfRow {
    /// Opaque id for `shelf_command` (the item's `file://` URI); the path comes from Rust.
    pub id: String,
    pub name: String,
    /// For display and "Copy path" (lossy only for names that are not UTF-8).
    pub path: String,
    /// Percent-encoded `file://` URI, used as the drag-out `text/uri-list`.
    pub uri: String,
    pub folder: bool,
    /// False when the file was moved or deleted since it was parked (re-checked on read).
    pub present: bool,
    /// Unix time in ms.
    pub added_ms: f64,
}
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ShelfChanged {
    pub rev: u32,
    /// `shelfEnabled`: off means the notch takes no drops (the list stays).
    pub enabled: bool,
    pub max_items: u32,
    pub items: Vec<ShelfRow>,
}
#[derive(Deserialize, Clone, Debug, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ShelfOp {
    Open { id: String },
    Reveal { id: String },
    Remove { id: String },
    Clear {},
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

// ---- Autoclicker -------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ClickButton {
    Left,
    Right,
    Middle,
}
/// What each click presses: a mouse button or a keyboard key.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ClickTarget {
    Mouse,
    Key,
}
/// `hold`: press once when the run starts and keep it down until it stops.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ClickKind {
    Single,
    Double,
    Hold,
}
/// `toggle`: one press of the hotkey starts, the next stops. `hold`: clicks while the
/// hotkey is held down. `burst`: each press sends `burstCount` clicks.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TriggerMode {
    Toggle,
    Hold,
    Burst,
}
/// Saved to `autoclicker.json`. Every field is range-checked by
/// `integrations::autoclicker::core::validate`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoclickerConfig {
    /// Time between clicks (1 ms ..= 1 h).
    pub interval_ms: u32,
    /// Random ± spread on each interval, in percent (0 ..= 50).
    pub jitter_percent: u32,
    pub target: ClickTarget,
    pub button: ClickButton,
    /// Key text for `target: key` (`E`, `Space`, `Ctrl+C`).
    pub key: String,
    pub kind: ClickKind,
    /// How long each press is held down before release (0 ..= 1000 ms).
    pub press_ms: u32,
    pub mode: TriggerMode,
    /// Clicks per hotkey press in burst mode (1 ..= 1000).
    pub burst_count: u32,
    /// Stop after this many clicks (0 = no limit).
    pub click_limit: u32,
    /// Stop after this many seconds (0 = no limit).
    pub time_limit_s: u32,
    /// Delay before clicking when started from the island's Start button (0 ..= 10 s).
    pub start_delay_s: u32,
    /// KGlobalAccel key text (`F8`, `Ctrl+Alt+K`); `""` = no hotkey.
    pub hotkey: String,
    /// Always stops clicking; registered with KDE only while clicking. `""` = none.
    pub stop_hotkey: String,
}
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct AutoclickerChanged {
    pub rev: u32,
    /// False when `/dev/uinput` cannot be opened for writing.
    pub available: bool,
    pub detail: String,
    pub config: AutoclickerConfig,
    /// The hotkey is registered with KDE only while armed.
    pub armed: bool,
    pub running: bool,
    /// Counting down the start delay (running is true too).
    pub starting: bool,
    /// Clicks in the current (or last) run.
    pub clicks: f64,
    /// Clicking time of the current (or last) run, excluding the start delay.
    pub session_ms: f64,
    /// All runs ever (until reset).
    pub lifetime_clicks: f64,
    pub lifetime_ms: f64,
    /// `off` | `bound` | `none` | `conflict` | `invalid` | `unavailable` | `pending`.
    pub hotkey_state: String,
    pub hotkey_detail: String,
    /// The stop hotkey, same states (`off` while not clicking).
    pub stop_hotkey_state: String,
    pub stop_hotkey_detail: String,
}
#[derive(Deserialize, Clone, Debug, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AutoclickerOp {
    Configure { config: AutoclickerConfig },
    Arm { armed: bool },
    Start {},
    Stop {},
    ResetStats {},
}
