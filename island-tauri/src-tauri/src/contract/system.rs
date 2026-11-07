//! `system_stats` payload: the exact JSON shape of island-rust `stats.rs`, nulls preserved
//! (an unavailable value is `null`, never a fabricated zero).
//!
//! Byte counts are `f64` (IPC forbids 64-bit integers; every value here is < 2^53).

use serde::{Deserialize, Serialize};

/// Explicit user-selected weather location (SPEC §3.5, §5.2).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeoPlace {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub country: String,
}

impl GeoPlace {
    pub fn validate(&self) -> Result<(), crate::error::IslandError> {
        if self.name.trim().is_empty()
            || self.name.len() > 256
            || self.country.len() > 256
            || self
                .name
                .chars()
                .chain(self.country.chars())
                .any(char::is_control)
            || !self.latitude.is_finite()
            || !(-90.0..=90.0).contains(&self.latitude)
            || !self.longitude.is_finite()
            || !(-180.0..=180.0).contains(&self.longitude)
        {
            return Err(crate::error::IslandError::invalid(
                "Invalid weatherLocation",
            ));
        }
        Ok(())
    }
}

/// Latest-value topic (`PowerChanged`). Revised in Phase 6a (additive + honest nulls):
/// `rev` for the stale-revision guard, `state` (exact UPower state name, so "unknown" is not
/// confused with "not charging"), and `onBattery` is `null` until UPower reports it.
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PowerState {
    pub rev: u32,
    pub available: bool,
    pub present: bool,
    pub percent: Option<f64>,
    /// `unknown | charging | discharging | empty | fullyCharged | pendingCharge | pendingDischarge`
    pub state: String,
    pub charging: bool,
    pub on_battery: Option<bool>,
    pub time_to_empty: Option<f64>,
    pub time_to_full: Option<f64>,
}

/// Latest-value topic (`WeatherChanged`). Revised in Phase 6a: `isDay`/`updatedAt` are `null`
/// (never `false`/`0`) until a fetch succeeded; `temp`/`unit` carry the value in the user's
/// `weatherUnit` (`tempC` stays the stored Celsius value); `updatedAt` is epoch milliseconds.
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WeatherState {
    pub rev: u32,
    pub available: bool,
    pub stale: bool,
    pub temp_c: Option<f64>,
    pub temp: Option<f64>,
    pub unit: super::settings::TempUnit,
    pub code: Option<u16>,
    pub icon: Option<String>,
    pub is_day: Option<bool>,
    pub place: Option<GeoPlace>,
    pub updated_at: Option<f64>,
}

/// Revised in Phase 6a: `icon` is optional (BlueZ may not report one), plus object path and
/// paired flag (additive).
#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BtDevice {
    pub address: String,
    pub name: String,
    pub icon: Option<String>,
    pub connected: bool,
    pub battery_percent: Option<u8>,
    pub path: String,
    pub paired: bool,
}

/// Latest-value topic (`BluetoothChanged`); SPEC §3.1 requires a revision on every payload.
#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct BluetoothChanged {
    pub rev: u32,
    pub devices: Vec<BtDevice>,
}

/// SPEC §4.5 `ServiceStatus` (latest-value): availability of one OS-integration service with
/// a reason. `connects` counts successful (re)connections to the daemon/endpoint.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub rev: u32,
    pub service: String,
    pub available: bool,
    pub detail: String,
    pub connects: u32,
}

/// Phase 6m. `position_ms` is the position at `sampled_at_ms` (epoch ms); a playing track's
/// current position is `position_ms + (now - sampled_at_ms) * rate`, extrapolated by the UI
/// (SPEC §3.5 "position interpolated in UI"). Absent values are `null`, never 0.
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub player: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    pub position_ms: Option<f64>,
    pub duration_ms: Option<f64>,
    pub art_url: Option<String>,
    pub can_control: bool,
    /// Full bus name of the preferred player, e.g. `org.mpris.MediaPlayer2.spotify`.
    pub bus_name: String,
    /// `playing | paused | stopped`.
    pub status: String,
    pub rate: f64,
    /// Epoch milliseconds at which `position_ms` was valid.
    pub sampled_at_ms: f64,
    pub track_id: Option<String>,
    pub can_seek: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    /// Player-local MPRIS volume; never the system output volume.
    pub volume_percent: Option<f64>,
    pub volume_writable: bool,
    /// Hint for matching the player's output stream if MPRIS volume is unavailable.
    pub pid: Option<u32>,
}

/// Latest-value topic (`MediaChanged`). `nowPlaying: null` = no player (cleared, never stale).
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct MediaChanged {
    pub rev: u32,
    pub now_playing: Option<NowPlaying>,
}

/// `media_control` argument (SPEC §4.3). Commands reach only the preferred MPRIS player.
#[derive(Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type",
    deny_unknown_fields
)]
pub enum MediaCommand {
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
    Volume {
        percent: f64,
    },
    /// Absolute position in the current track.
    Seek {
        position_ms: f64,
    },
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LyricsStatus {
    /// Lyrics are switched off (`lyricsEnabled` false): no network call is ever made.
    Off,
    /// Nothing is playing.
    Idle,
    Loading,
    Synced,
    Plain,
    Instrumental,
    NotFound,
    /// Lookup failed (network, rate limit, bad response); `plain`/`lines` are empty.
    Error,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    pub time_ms: f64,
    pub text: String,
}

/// Latest-value topic (`LyricsChanged`). `activeIndex` follows the MPRIS position.
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct LyricsChanged {
    pub rev: u32,
    pub status: LyricsStatus,
    /// Cache key of the track the lyrics belong to (`null` when none).
    pub track_key: Option<String>,
    pub lines: Vec<LyricLine>,
    pub plain: Option<String>,
    pub active_index: Option<u32>,
}

/// Latest-value topic (`EqualizerChanged`), emitted at most 30 times per second and only while
/// the equalizer is capturing. `active: false` = capture stopped (bands empty).
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct EqualizerChanged {
    pub rev: u32,
    pub active: bool,
    /// Band levels in 0..=1, low to high frequency.
    pub bands: Vec<f64>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub id: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub primary: bool,
}

/// Availability of implemented service families, not platform promises (SPEC §5.3).
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NotificationMonitorStatus {
    pub available: bool,
    pub detail: String,
    pub pending: u32,
    pub overflow: u32,
    pub timed_out: u32,
    pub invalid: u32,
    pub rearms: u32,
    pub loop_dropped: u32,
    pub observed_actions: u32,
}

/// Availability of implemented service families, not platform promises (SPEC §5.3).
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCapabilities {
    pub terminal: bool,
    pub settings: bool,
    pub palette: bool,
    pub system_stats: bool,
    pub ai_usage: bool,
    pub voice: bool,
    pub notifications: bool,
    pub system_notification_mirror: bool,
    pub notification_monitor: NotificationMonitorStatus,
    pub can_invoke_actions: bool,
    pub clipboard: bool,
    pub clipboard_status: super::clipboard::ClipboardStatus,
    pub power: bool,
    pub power_status: ServiceStatus,
    pub weather: bool,
    pub weather_status: ServiceStatus,
    pub bluetooth: bool,
    pub bluetooth_status: ServiceStatus,
    pub media: bool,
    /// Phase 6m (additive): MPRIS availability with a reason.
    pub media_status: ServiceStatus,
    /// Phase 6m (additive): LRCLIB lyrics (`disabled` until opted in).
    pub lyrics_status: ServiceStatus,
    /// Phase 6m (additive): equalizer capture (`disabled` until opted in).
    pub equalizer_status: ServiceStatus,
    pub displays: bool,
}

impl ServiceCapabilities {
    pub fn phase2() -> Self {
        Self {
            terminal: true,
            settings: true,
            palette: true,
            system_stats: true,
            ai_usage: true,
            voice: true,
            ..Self::default()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiskStats {
    pub total: f64,
    pub used: f64,
    pub available: f64,
    pub percent: f64,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStats {
    pub rx_bytes_per_second: f64,
    pub tx_bytes_per_second: f64,
    pub interfaces: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SystemStats {
    pub cpu: Option<f64>,
    pub mem_pct: Option<f64>,
    pub mem_used: Option<f64>,
    pub mem_total: Option<f64>,
    pub uptime: Option<f64>,
    pub load_average: Option<[f64; 3]>,
    pub cpu_count: Option<u32>,
    pub hostname: Option<String>,
    pub platform: String,
    pub disk: Option<DiskStats>,
    pub network: Option<NetworkStats>,
}

/// Monolith's own running cost (Monitor view, "Monolith cost" card). CPU is a percentage of
/// ONE core averaged over `windowSecs` (the last ~10 s); every value is `null` while it is
/// unavailable (first sample, process not running), never a fabricated zero.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MonolithCost {
    /// The island's whole process tree: main, WebKit processes, helpers, terminal + children.
    pub cpu: Option<f64>,
    /// Proportional set size of that tree, in bytes.
    pub pss_bytes: Option<f64>,
    /// Scheduler wakeups per second (voluntary + involuntary context switches, all threads).
    pub wakeups: Option<f64>,
    pub processes: u32,
    pub threads: u32,
    pub plasmashell_cpu: Option<f64>,
    pub kwin_cpu: Option<f64>,
    pub window_secs: f64,
}

/// One process in the Monitor → Processes list. CPU is a percentage of the WHOLE machine
/// (all cores = 100) over the interval since the previous `process_list` call; GPU is the
/// share of the GPU's render+compute engines (amdgpu/i915/xe `drm-engine-*` fdinfo).
/// `null` = unavailable (first sample, another user's process, no DRM client), never zero.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRow {
    pub pid: u32,
    pub ppid: u32,
    /// Process start time in clock ticks since boot; `process_kill` requires it so a
    /// recycled pid is never signalled.
    pub start_ticks: f64,
    pub name: String,
    pub command: String,
    pub cpu: Option<f64>,
    /// Private (anonymous) resident memory in bytes: what the process itself holds.
    pub memory_bytes: f64,
    pub gpu: Option<f64>,
    /// VRAM held by the process's DRM clients, in bytes.
    pub vram_bytes: Option<f64>,
    pub threads: u32,
    /// Owned by the current user, so it can be signalled.
    pub own: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessList {
    pub rows: Vec<ProcessRow>,
    /// Whether any process exposed GPU engine counters (false: GPU column hidden).
    pub gpu_available: bool,
    pub memory_total: f64,
    pub cpu_count: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum KillSignal {
    /// SIGTERM: ask the process to quit.
    Term,
    /// SIGKILL: end it immediately.
    Kill,
}
