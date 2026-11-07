//! Installed desktop applications for the on-demand side-button action picker.
//! Only a desktop ID crosses launch IPC; never an executable, path, URI, or shell command.

use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DesktopApp {
    /// XDG desktop-file ID without the final `.desktop` suffix.
    pub id: String,
    /// The entry's unlocalised `Name=`, not `GenericName=` or the desktop-file ID.
    /// Untruncated: the layout editor must respect its separate 32-character label limit.
    pub name: String,
    /// An island monochrome glyph inferred from freedesktop `Categories=`.
    pub glyph: String,
}
