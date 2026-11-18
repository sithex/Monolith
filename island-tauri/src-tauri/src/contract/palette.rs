//! Hermes skin palette (names and roles identical to `island/palette.py`).

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Palette {
    pub name: String,
    pub background: String,
    pub accent: String,
    pub text: String,
    pub primary: String,
    pub dim: String,
    pub border: String,
    pub surface: String,
    pub error: String,
}

/// Effective colours the native notch actually draws after every override fallback (one
/// source of truth for both the plasmoid status and the renderer's Follow-skin swatches).
/// Precedence mirrors `runtime::panel_colors`: the panel clock colour wins, then the island
/// background override, then the scheme/skin surface; the border colour wins similarly over
/// the accent. `background` is the notch/clock-pill fill, `accent` the hover tint, and
/// `border_color` the notch border when enabled.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveNotchColors {
    pub background: String,
    pub accent: String,
    pub border_color: String,
}
