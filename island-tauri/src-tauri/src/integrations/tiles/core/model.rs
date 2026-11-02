//! Wire models for the tiles. All types that cross the IPC boundary use
//! `camelCase` via serde and accept no aliases.

use serde::{Deserialize, Serialize};

/// An audio sink as reported by `pactl -f json list sinks` (only the fields
/// the output tile models; every other pactl field is ignored).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sink {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub state: String,
    pub muted: bool,
}

/// The audio output tile: the current default sink (by name) and all sinks
/// in pactl order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputTile {
    pub current: Option<Sink>,
    pub choices: Vec<Sink>,
}

/// NetworkManager `State` u values (org.freedesktop.NetworkManager).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NmState {
    Unknown,
    Asleep,
    Disconnected,
    Disconnecting,
    Connecting,
    ConnectedLocal,
    ConnectedSite,
    ConnectedGlobal,
}

impl NmState {
    /// Map the D-Bus numeric `State` to the enum; unknown values are `None`.
    pub fn from_u32(value: u32) -> Option<NmState> {
        Some(match value {
            0 => NmState::Unknown,
            10 => NmState::Asleep,
            20 => NmState::Disconnected,
            30 => NmState::Disconnecting,
            40 => NmState::Connecting,
            50 => NmState::ConnectedLocal,
            60 => NmState::ConnectedSite,
            70 => NmState::ConnectedGlobal,
            _ => return None,
        })
    }
}

/// NetworkManager `Connectivity` u values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NmConnectivity {
    Unknown,
    None,
    Portal,
    Limited,
    Full,
}

impl NmConnectivity {
    /// Map the D-Bus numeric `Connectivity` to the enum; unknown values are
    /// `None`.
    pub fn from_u32(value: u32) -> Option<NmConnectivity> {
        Some(match value {
            0 => NmConnectivity::Unknown,
            1 => NmConnectivity::None,
            2 => NmConnectivity::Portal,
            3 => NmConnectivity::Limited,
            4 => NmConnectivity::Full,
            _ => return None,
        })
    }
}

/// Kind of the primary connection, derived from `PrimaryConnectionType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetKind {
    Wired,
    Wifi,
    Other,
    None,
}

/// The NetworkManager snapshot the Tauri actor feeds in (already decoded from
/// D-Bus properties on the system bus).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmSnapshot {
    pub state: Option<u32>,
    pub connectivity: Option<u32>,
    pub primary_connection_type: Option<String>,
    pub wireless_enabled: Option<bool>,
    pub wireless_hardware_enabled: Option<bool>,
    pub has_wireless_device: bool,
}

/// The Network tile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkTile {
    /// False when no NetworkManager snapshot was available (D-Bus down).
    pub available: bool,
    /// Human-readable reason when `available` is false.
    pub reason: Option<String>,
    pub state: Option<NmState>,
    pub connectivity: Option<NmConnectivity>,
    pub kind: NetKind,
    /// Only true when a wireless device exists.
    pub show_wifi_toggle: bool,
    pub wifi_enabled: Option<bool>,
}

/// tailscale status `BackendState` string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackendState {
    Running,
    Stopped,
    NeedsLogin,
    Starting,
    NoState,
    /// Any other `BackendState` string, kept verbatim.
    Unknown(String),
}

/// One tailscale peer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub host_name: String,
    /// `None` = the peer object carried no `Online` field (unknown is never reported as offline).
    pub online: Option<bool>,
    pub os: String,
    /// RFC3339 last-seen string; the zero time `0001-01-01T00:00:00Z`
    /// (never / unknown) is `None`.
    pub last_seen: Option<String>,
    /// First entry of `TailscaleIPs`, if any.
    pub first_ip: Option<String>,
}

/// The Tailscale tile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleTile {
    pub backend: BackendState,
    pub self_name: Option<String>,
    /// Peers sorted online-first, then by host name.
    pub peers: Vec<Peer>,
    pub online_count: u32,
}

/// The Hermes voice switcher list (`hermes-fish` output, order preserved).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceList {
    pub voices: Vec<String>,
    pub active: Option<String>,
}
