//! Rust → frontend events (SPEC §4.5). Emitted with `emit_to("island", …)`.

use super::{island::IslandState, palette::Palette, settings::Settings};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
pub struct SettingsChanged(pub Settings);

#[derive(
    Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type, tauri_specta::Event,
)]
pub struct PaletteChanged(pub Palette);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AnchorKind {
    Enter,
    Leave,
    Pin,
}

/// Phase 2–6 only: feeds the legacy shim's `onAnchor`. Listener-only in Phase 2; the emitter
/// arrives with `panel.sock` in Phase 3 (typed helper: `bus::emit_anchor`).
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type, tauri_specta::Event,
)]
pub struct AnchorSignal(pub AnchorKind);

/// SPEC §4.5: the state actor is the ONLY emitter. Latest-value: consumers drop older `rev`s.
#[derive(
    Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type, tauri_specta::Event,
)]
pub struct IslandStateChanged(pub IslandState);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CueKind {
    Click,
}

/// Added in Phase 3 (SPEC §4.5, additive): the actor's `PlayClick` effect. The renderer plays
/// it through its existing click-sound module, because only the renderer owns audio.
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type, tauri_specta::Event,
)]
pub struct IslandCue {
    pub kind: CueKind,
}

/// SPEC §4.5 latest-value topics added in Phase 6a (additive).
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
pub struct PowerChanged(pub super::system::PowerState);

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
pub struct WeatherChanged(pub super::system::WeatherState);
