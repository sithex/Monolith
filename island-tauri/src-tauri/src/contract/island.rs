//! Island mode + geometry reporting types (SPEC §5.2, §4.4).
//! No `u64`/`i64` crosses the boundary; inputs deny unknown fields.

use serde::{Deserialize, Serialize};

/// Canonical mode names only. Legacy renderer / status.json names (`idle|preview|pinned`) are
/// translated by the Phase 2–6 legacy shim, never by serde (SPEC decision, revised after
/// Phase 1): a serde alias makes specta emit separate serialize/deserialize types.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum IslandMode {
    Still,
    Hover,
    Large,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum IslandView {
    Terminal,
    Monitor,
    Ai,
    Settings,
    Notifications,
    Clipboard,
    Launcher,
    Music,
    QuickInfo,
    Timer,
    Calendar,
    Scratchpad,
    Downloads,
    Controls,
    Mixer,
    Brightness,
    Agents,
    Updates,
    Shelf,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", tag = "type", deny_unknown_fields)]
pub enum HoverCause {
    Pointer,
    PanelPill,
    Notification { id: String },
    Alert { id: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IslandState {
    pub rev: u32,
    pub mode: IslandMode,
    pub view: IslandView,
    pub hover_cause: Option<HoverCause>,
    pub dnd: bool,
    pub unread: u32,
    pub terminal_started: bool,
    /// Effective game mode: `settings.gameMode || (settings.gameModeAuto && fullscreen focus)`.
    /// While on, the island never previews, toasts or auto-expands; explicit opens still work.
    pub game_mode: bool,
}

#[derive(Deserialize, Clone, Debug, specta::Type)]
#[serde(rename_all = "camelCase", tag = "type", deny_unknown_fields)]
pub enum UiIntent {
    PointerEnter,
    PointerLeave,
    Pin,
    Collapse,
    /// Dismiss only the preview the user saw; never a newly opened terminal.
    DismissPreview {
        rev: u32,
    },
    SelectView {
        view: IslandView,
    },
    SetDnd {
        on: bool,
    },
    SetClipboardPaused {
        paused: bool,
    },
    RestartTerminal,
    Quit,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoundedRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}

#[derive(Deserialize, Clone, Debug, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeometryReport {
    pub state_rev: u32,
    pub mode: IslandMode,
    pub rects: Vec<RoundedRect>,
    pub scale: f64,
    pub settled: bool,
}

/// Which UI the island window loads (CLI `--ui`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UiKind {
    /// The Phase 1/1.5 geometry harness (`index.html`); default so those GUI tests are unchanged.
    #[default]
    Harness,
    /// The legacy fork renderer behind the `window.island` shim (`legacy.html`).
    Legacy,
    /// Phase 7 typed renderer; harness remains the default until Phase 9.
    Island,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub overlay: bool,
    /// No Plasma anchor discovery before Phase 3, so always `false` for now.
    pub panel_anchor: bool,
    pub platform: String,
    pub version: String,
    pub test_instance: bool,
    pub test_hooks: bool,
    pub ui: UiKind,
}

/// Full SPEC §5.3. Unbuilt topics are explicitly unavailable, never fabricated.
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapSnapshot {
    pub apps: super::apps::AppsSnapshot,
    pub runtime: RuntimeInfo,
    pub state: IslandState,
    pub settings: super::settings::Settings,
    pub palette: super::palette::Palette,
    pub capabilities: super::system::ServiceCapabilities,
    pub power: Option<super::system::PowerState>,
    pub weather: Option<super::system::WeatherState>,
    pub media: Option<super::system::NowPlaying>,
    /// Revision of `media` (additive, Phase 6m) for the `MediaChanged` rev guard.
    pub media_rev: u32,
    /// Latest lyrics state (additive, Phase 6m); `null` before the lyrics service exists.
    pub lyrics: Option<super::system::LyricsChanged>,
    pub bluetooth: Vec<super::system::BtDevice>,
    /// Revision of `bluetooth` (additive, Phase 6a) for the `BluetoothChanged` rev guard.
    pub bluetooth_rev: u32,
    pub notifications: Vec<super::notify::IslandNotification>,
}

/// `applied == false` is not an error: the report was stale or changed nothing.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GeometryAck {
    pub applied: bool,
    pub rows: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_accepts_only_canonical_names() {
        for (wire, expected) in [
            ("still", IslandMode::Still),
            ("hover", IslandMode::Hover),
            ("large", IslandMode::Large),
        ] {
            let mode: IslandMode = serde_json::from_str(&format!("\"{wire}\"")).unwrap();
            assert_eq!(mode, expected, "{wire}");
            assert_eq!(serde_json::to_string(&mode).unwrap(), format!("\"{wire}\""));
        }
        // Legacy names are the shim's job; the Rust contract must reject them.
        for legacy in ["idle", "preview", "pinned", "quick"] {
            assert!(
                serde_json::from_str::<IslandMode>(&format!("\"{legacy}\"")).is_err(),
                "{legacy} must not deserialize"
            );
        }
    }

    #[test]
    fn bootstrap_serializes_full_current_state_and_honest_unbuilt_topics() {
        use crate::contract::{palette::Palette, settings::Settings, system::ServiceCapabilities};
        let state = IslandState {
            rev: 42,
            mode: IslandMode::Hover,
            view: IslandView::Monitor,
            hover_cause: Some(HoverCause::Pointer),
            dnd: true,
            unread: 0,
            terminal_started: true,
            game_mode: false,
        };
        let snapshot = BootstrapSnapshot {
            apps: super::super::apps::AppsSnapshot::default(),
            runtime: RuntimeInfo {
                overlay: true,
                panel_anchor: false,
                platform: "fixture".into(),
                version: "fixture".into(),
                test_instance: true,
                test_hooks: true,
                ui: UiKind::Legacy,
            },
            state: state.clone(),
            settings: Settings::default(),
            palette: Palette {
                name: "fixture".into(),
                background: "#111111".into(),
                accent: "#222222".into(),
                text: "#333333".into(),
                primary: "#444444".into(),
                dim: "#555555".into(),
                border: "#666666".into(),
                surface: "#777777".into(),
                error: "#888888".into(),
            },
            capabilities: ServiceCapabilities::phase2(),
            power: None,
            weather: None,
            media: None,
            media_rev: 0,
            lyrics: None,
            bluetooth: Vec::new(),
            bluetooth_rev: 0,
            notifications: Vec::new(),
        };
        let raw = serde_json::to_value(snapshot).unwrap();
        assert_eq!(raw.as_object().unwrap().len(), 14);
        assert_eq!(raw["bluetoothRev"], 0);
        assert_eq!(raw["mediaRev"], 0);
        assert!(raw["lyrics"].is_null());
        assert_eq!(raw["state"], serde_json::to_value(state).unwrap());
        assert!(raw["palette"].is_object());
        for key in ["power", "weather", "media"] {
            assert!(raw[key].is_null(), "{key}");
        }
        for key in ["bluetooth", "notifications"] {
            assert_eq!(raw[key], serde_json::json!([]), "{key}");
        }
        for key in [
            "notifications",
            "systemNotificationMirror",
            "canInvokeActions",
            "clipboard",
            "power",
            "weather",
            "bluetooth",
            "media",
            "displays",
        ] {
            assert_eq!(raw["capabilities"][key], false, "unbuilt {key}");
        }
    }

    #[test]
    fn report_is_camel_case_and_rejects_unknown_fields() {
        let good = serde_json::json!({
            "stateRev": 3, "mode": "hover", "scale": 1.0, "settled": true,
            "rects": [{"x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0, "radius": 5.0}]
        });
        let report: GeometryReport = serde_json::from_value(good.clone()).unwrap();
        assert_eq!(report.state_rev, 3);
        assert_eq!(report.mode, IslandMode::Hover);
        let mut extra = good;
        extra["surprise"] = serde_json::json!(true);
        assert!(serde_json::from_value::<GeometryReport>(extra).is_err());
    }
}
