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
