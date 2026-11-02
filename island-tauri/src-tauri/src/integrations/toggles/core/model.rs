//! Wire types for the quick-toggles tiles. camelCase on the wire via
//! serde, no `#[serde(alias)]`.

use serde::{Deserialize, Serialize};

/// One of the three power-profiles-daemon profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    PowerSaver,
    Balanced,
    Performance,
}

impl std::str::FromStr for Profile {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "power-saver" => Ok(Profile::PowerSaver),
            "balanced" => Ok(Profile::Balanced),
            "performance" => Ok(Profile::Performance),
            _ => Err(()),
        }
    }
}

impl Profile {
    /// Canonical order: power-saver, balanced, performance.
    pub const ALL: [Profile; 3] = [Profile::PowerSaver, Profile::Balanced, Profile::Performance];
}

/// One entry of `Profiles` aa{sv}: the profile id plus its driver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PpdProfile {
    pub profile: String,
    pub driver: Option<String>,
}

/// One entry of `ActiveProfileHolds` aa{sv}.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PpdHold {
    pub profile: String,
    pub reason: String,
    pub application_id: String,
}

/// The zbus-derived snapshot of the power-profiles-daemon (converted by
/// the Tauri side): `ActiveProfile`, `Profiles`, `PerformanceDegraded`
/// and `ActiveProfileHolds`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PpdSnapshot {
    pub active: String,
    pub profiles: Vec<PpdProfile>,
    pub degraded: String,
    pub holds: Vec<PpdHold>,
}

/// The power-profile tile shown in Controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerTile {
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub active: Option<Profile>,
    /// Known profiles present, in canonical order power-saver, balanced,
    /// performance.
    pub choices: Vec<Profile>,
    /// `PerformanceDegraded` reason; `None` when not degraded.
    pub degraded: Option<String>,
    /// Hold reasons (non-empty) in list order.
    pub held_by: Vec<String>,
    /// Number of unknown profile strings dropped from `choices`.
    pub ignored: usize,
}

/// The Do Not Disturb tile (trivial; the settings module owns the state).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DndTile {
    pub on: bool,
}
