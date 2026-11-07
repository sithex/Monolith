//! Phase 7b authoritative workspace contract. No shell actions or opaque command IDs.
use super::{island::IslandView, settings::ObjectOnly};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum LauncherOp {
    Begin {},
    Move { id: String, to: u32 },
    Disable { id: String },
    Enable { id: String },
    Cancel {},
    Commit {},
}

/// Full permutation, including hidden and unknown/inert modules. Only `known` items have routes.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LauncherItem {
    pub id: String,
    pub enabled: bool,
    pub locked: bool,
    pub known: bool,
    pub view: Option<IslandView>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct LauncherChanged {
    pub rev: u32,
    pub items: Vec<LauncherItem>,
    pub editing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LayoutSide {
    Left,
    Right,
    Bottom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LayoutAction {
    OpenApp { view: IslandView },
    AppGrid {},
    Settings {},
    Volume {},
    NowPlaying {},
    Timer {},
    // One-tap quick actions (Nov 2). Each maps to an existing typed command; none runs a shell.
    PlayPause {},
    NextTrack {},
    PreviousTrack {},
    ToggleDnd {},
    MicMute {},
    CyclePowerProfile {},
    Screenshot {},
    PickColour {},
    KeepAwake {},
    Collapse {},
    // Manual game mode on/off (`settings.gameMode`).
    ToggleGameMode {},
    CommandPalette {},
    LaunchDesktop { desktop_id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LayoutPreset {
    Compact {},
    Spacious {},
    Bigger {},
    Custom { width: f64, max_height: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutButton {
    pub id: String,
    pub side: LayoutSide,
    pub slot: u32,
    pub action: LayoutAction,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum LayoutOp {
    Preset {
        preset: LayoutPreset,
    },
    Add {
        button: ObjectOnly<LayoutButton>,
    },
    Remove {
        id: String,
    },
    Edit {
        id: String,
        action: LayoutAction,
        label: Option<String>,
    },
    Move {
        id: String,
        side: LayoutSide,
        slot: u32,
    },
    Diameter {
        diameter: f64,
    },
    Gap {
        gap: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LayoutModel {
    pub version: u32,
    pub preset: LayoutPreset,
    pub button_diameter: f64,
    pub gap: f64,
    pub buttons: Vec<LayoutButton>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct LayoutChanged {
    pub rev: u32,
    pub layout: LayoutModel,
    /// Keep ratified 1040px terminal / 640px other endpoints until an explicit preset command.
    pub unchanged_sizing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutGeometryQuery {
    pub island: ObjectOnly<LayoutRect>,
    pub viewport_width: f64,
    pub viewport_height: f64,
    /// Include empty + slots in miniature geometry; never paint these as live buttons.
    pub include_empty: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LayoutSlotRect {
    pub id: Option<String>,
    pub side: LayoutSide,
    pub slot: u32,
    pub rect: LayoutRect,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LayoutGeometry {
    pub rev: u32,
    pub island: LayoutRect,
    pub buttons: Vec<LayoutSlotRect>,
    pub empty_slots: Vec<LayoutSlotRect>,
}
