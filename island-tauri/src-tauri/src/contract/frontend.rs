//! Additive Phase 7a visual topic. Legacy Settings/Palette payloads stay unchanged.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Appearance {
    #[default]
    Skin,
    Neutral,
    // Colour scheme presets (Nov 2); the UI owns their palettes (ui/island/tokens/schemes.ts).
    Ocean,
    Dune,
    Dusk,
    Silk,
    Aurora,
    Ember,
    Rose,
    Glacier,
    /// Fixed snapshot of the Necron-green Hermes look (Nov 2).
    Green,
    /// Palette derived from `customAccent`/`customBackground`/`customText` (Nov 2).
    Custom,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct VisualChanged {
    pub rev: u32,
    pub settings: super::settings::Settings,
    pub palette: super::palette::Palette,
    pub appearance: Appearance,
}
