use serde::{Deserialize, Serialize};

/// A parsed launcher entry. Wire shape is camelCase; unknown JSON fields are
/// ignored, missing optional fields default to empty/`false`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    /// The desktop file id (e.g. `org.kde.dolphin.desktop`).
    pub desktop_id: String,
    /// Translated name (required by spec).
    pub name: String,
    #[serde(default)]
    pub generic_name: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub icon: String,
    /// Raw `Exec` line (as written in the file, after line trimming). Passed
    /// unchanged to `build_argv`, which applies the escape/quote/field-code
    /// rules.
    pub exec: String,
    #[serde(default)]
    pub try_exec: String,
    #[serde(default)]
    pub terminal: bool,
    #[serde(default)]
    pub no_display: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub only_show_in: Vec<String>,
    #[serde(default)]
    pub not_show_in: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// A non-fatal parse notice (currently only for malformed booleans).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub message: String,
}

impl Warning {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Whether `entry` would be shown in the current session desktops.
/// Case-sensitive (e.g. "KDE" != "kde").
pub fn visible_in(entry: &AppEntry, current_desktops: &[&str]) -> bool {
    if entry.hidden || entry.no_display {
        return false;
    }
    if !entry.only_show_in.is_empty()
        && !entry
            .only_show_in
            .iter()
            .any(|d| current_desktops.contains(&d.as_str()))
    {
        return false;
    }
    if entry
        .not_show_in
        .iter()
        .any(|d| current_desktops.contains(&d.as_str()))
    {
        return false;
    }
    true
}
