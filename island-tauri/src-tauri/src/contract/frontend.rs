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
    /// Accent and tint taken from the playing track's artwork (Nov 5); falls back to Skin.
    Album,
    // Community colour schemes (Nov 6), one id per variant; the UI groups them into
    // families. Palettes and ANSI 16 colours (with sources) live in schemes.ts.
    #[serde(rename = "catppuccin-mocha")]
    CatppuccinMocha,
    #[serde(rename = "catppuccin-macchiato")]
    CatppuccinMacchiato,
    #[serde(rename = "catppuccin-frappe")]
    CatppuccinFrappe,
    #[serde(rename = "catppuccin-latte")]
    CatppuccinLatte,
    Dracula,
    Nord,
    #[serde(rename = "gruvbox-dark")]
    GruvboxDark,
    #[serde(rename = "gruvbox-light")]
    GruvboxLight,
    #[serde(rename = "tokyo-night")]
    TokyoNight,
    #[serde(rename = "tokyo-storm")]
    TokyoStorm,
    #[serde(rename = "rose-pine")]
    RosePine,
    #[serde(rename = "rose-pine-moon")]
    RosePineMoon,
    #[serde(rename = "rose-pine-dawn")]
    RosePineDawn,
    #[serde(rename = "one-dark")]
    OneDark,
    #[serde(rename = "solarized-dark")]
    SolarizedDark,
    #[serde(rename = "solarized-light")]
    SolarizedLight,
}

#[cfg(test)]
mod appearance_ids {
    use super::Appearance;

    /// The ids are persisted in appearance.json and matched by ui/island/tokens/schemes.ts.
    #[test]
    fn community_scheme_ids_are_kebab_case_and_round_trip() {
        let cases = [
            (Appearance::CatppuccinMocha, "catppuccin-mocha"),
            (Appearance::CatppuccinMacchiato, "catppuccin-macchiato"),
            (Appearance::CatppuccinFrappe, "catppuccin-frappe"),
            (Appearance::CatppuccinLatte, "catppuccin-latte"),
            (Appearance::Dracula, "dracula"),
            (Appearance::Nord, "nord"),
            (Appearance::GruvboxDark, "gruvbox-dark"),
            (Appearance::GruvboxLight, "gruvbox-light"),
            (Appearance::TokyoNight, "tokyo-night"),
            (Appearance::TokyoStorm, "tokyo-storm"),
            (Appearance::RosePine, "rose-pine"),
            (Appearance::RosePineMoon, "rose-pine-moon"),
            (Appearance::RosePineDawn, "rose-pine-dawn"),
            (Appearance::OneDark, "one-dark"),
            (Appearance::SolarizedDark, "solarized-dark"),
            (Appearance::SolarizedLight, "solarized-light"),
            (Appearance::Green, "green"),
            (Appearance::Album, "album"),
        ];
        for (variant, id) in cases {
            let json = format!("\"{id}\"");
            assert_eq!(serde_json::to_string(&variant).unwrap(), json);
            assert_eq!(serde_json::from_str::<Appearance>(&json).unwrap(), variant);
        }
        assert!(serde_json::from_str::<Appearance>("\"catppuccinMocha\"").is_err());
    }
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct VisualChanged {
    pub rev: u32,
    pub settings: super::settings::Settings,
    pub palette: super::palette::Palette,
    pub appearance: Appearance,
}
