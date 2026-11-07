use super::error::LayoutError;
use serde::{Deserialize, Serialize};

/// Minimum valid button/app id length in characters.
pub const MIN_ID_LEN: usize = 1;
/// Maximum valid button/app id length in characters.
pub const MAX_ID_LEN: usize = 64;
/// Maximum label length in characters.
pub const MAX_LABEL_CHARS: usize = 32;

/// Slot count on the Left / Right sides.
pub const MAX_SLOTS_LEFT_RIGHT: u8 = 5;
/// Slot count on the Bottom side.
pub const MAX_SLOTS_BOTTOM: u8 = 7;

/// Default button diameter (logical px).
pub const DEFAULT_BUTTON_DIAMETER: f64 = 36.0;
/// Minimum button diameter.
pub const MIN_BUTTON_DIAMETER: f64 = 28.0;
/// Maximum button diameter.
pub const MAX_BUTTON_DIAMETER: f64 = 56.0;
/// Default island-to-button gap.
pub const DEFAULT_GAP: f64 = 10.0;
/// Minimum gap.
pub const MIN_GAP: f64 = 4.0;
/// Maximum gap.
pub const MAX_GAP: f64 = 24.0;

/// Compact preset width.
pub const PRESET_COMPACT_WIDTH: f64 = 360.0;
/// Compact preset max height.
pub const PRESET_COMPACT_MAX_HEIGHT: f64 = 420.0;
/// Spacious preset width.
pub const PRESET_SPACIOUS_WIDTH: f64 = 520.0;
/// Spacious preset max height.
pub const PRESET_SPACIOUS_MAX_HEIGHT: f64 = 640.0;
/// Bigger preset width (the default since Nov 2).
pub const PRESET_BIGGER_WIDTH: f64 = 760.0;
/// Bigger preset max height.
pub const PRESET_BIGGER_MAX_HEIGHT: f64 = 700.0;
/// Custom preset minimum width.
pub const CUSTOM_MIN_WIDTH: f64 = 300.0;
/// Custom preset maximum width.
pub const CUSTOM_MAX_WIDTH: f64 = 1800.0;
/// Custom preset minimum max-height.
pub const CUSTOM_MIN_MAX_HEIGHT: f64 = 240.0;
/// Custom preset maximum max-height.
pub const CUSTOM_MAX_MAX_HEIGHT: f64 = 1280.0;

/// Persisted layout schema version.
pub const VERSION: u32 = 1;

/// Which side of the island a button lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Left,
    Right,
    Bottom,
}

impl Side {
    /// Number of slots available on this side.
    pub fn capacity(self) -> u8 {
        match self {
            Side::Left | Side::Right => MAX_SLOTS_LEFT_RIGHT,
            Side::Bottom => MAX_SLOTS_BOTTOM,
        }
    }
}

/// What a side button does when clicked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ButtonAction {
    OpenApp { app_id: String },
    AppGrid,
    Settings,
    Volume,
    NowPlaying,
    Timer,
    PlayPause,
    NextTrack,
    PreviousTrack,
    ToggleDnd,
    MicMute,
    CyclePowerProfile,
    Screenshot,
    PickColour,
    KeepAwake,
    Collapse,
    ToggleGameMode,
    CommandPalette,
    LaunchDesktop { desktop_id: String },
    Command { id: String },
}

impl ButtonAction {
    /// True when embedded ids (appId / id) are valid.
    pub(crate) fn is_valid(&self) -> bool {
        match self {
            ButtonAction::OpenApp { app_id } => valid_id(app_id),
            ButtonAction::Command { id } => valid_id(id),
            ButtonAction::LaunchDesktop { desktop_id } => {
                valid_id(desktop_id) && !desktop_id.starts_with(['.', '-'])
            }
            _ => true,
        }
    }
}

/// Island size preset. `Custom` dimensions must be finite and within their
/// bounds (`width` 300..=1800, `max_height` 240..=1280).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SizePreset {
    Compact,
    Spacious,
    Bigger,
    Custom { width: f64, max_height: f64 },
}

impl SizePreset {
    /// Effective island width for this preset.
    pub fn width(&self) -> f64 {
        match self {
            SizePreset::Compact => PRESET_COMPACT_WIDTH,
            SizePreset::Spacious => PRESET_SPACIOUS_WIDTH,
            SizePreset::Bigger => PRESET_BIGGER_WIDTH,
            SizePreset::Custom { width, .. } => *width,
        }
    }

    /// Effective island max-height for this preset.
    pub fn max_height(&self) -> f64 {
        match self {
            SizePreset::Compact => PRESET_COMPACT_MAX_HEIGHT,
            SizePreset::Spacious => PRESET_SPACIOUS_MAX_HEIGHT,
            SizePreset::Bigger => PRESET_BIGGER_MAX_HEIGHT,
            SizePreset::Custom { max_height, .. } => *max_height,
        }
    }

    /// True for Compact/Spacious, and for Custom when the stored dimensions
    /// are finite and within the documented bounds.
    pub(crate) fn is_valid(&self) -> bool {
        match self {
            SizePreset::Compact | SizePreset::Spacious | SizePreset::Bigger => true,
            SizePreset::Custom { width, max_height } => {
                width.is_finite()
                    && max_height.is_finite()
                    && (CUSTOM_MIN_WIDTH..=CUSTOM_MAX_WIDTH).contains(width)
                    && (CUSTOM_MIN_MAX_HEIGHT..=CUSTOM_MAX_MAX_HEIGHT).contains(max_height)
            }
        }
    }
}

/// A single side button placement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SideButton {
    /// Unique id: 1..=64 chars of [A-Za-z0-9_.-].
    pub id: String,
    /// Which side the button is on.
    pub side: Side,
    /// Slot index on that side (Left/Right 0..5, Bottom 0..7).
    pub slot: u8,
    /// What the button does.
    pub action: ButtonAction,
    /// Optional label, at most [`MAX_LABEL_CHARS`] chars.
    pub label: Option<String>,
}

/// The persisted layout model. The single owner of the layout state; all
/// editing operations validate and leave the layout unchanged on error.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// Schema version (always [`VERSION`]).
    pub version: u32,
    /// Island size preset.
    pub preset: SizePreset,
    /// Side-button diameter (logical px), 28..=56.
    pub button_diameter: f64,
    /// Gap between island and side buttons (logical px), 4..=24.
    pub gap: f64,
    /// Current side buttons (unique ids, unique (side, slot) pairs).
    pub buttons: Vec<SideButton>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            version: VERSION,
            preset: SizePreset::Bigger,
            button_diameter: DEFAULT_BUTTON_DIAMETER,
            gap: DEFAULT_GAP,
            buttons: vec![
                SideButton {
                    id: "apps".to_string(),
                    side: Side::Left,
                    slot: 0,
                    action: ButtonAction::AppGrid,
                    label: None,
                },
                SideButton {
                    id: "timer".to_string(),
                    side: Side::Left,
                    slot: 1,
                    action: ButtonAction::Timer,
                    label: None,
                },
                SideButton {
                    id: "settings".to_string(),
                    side: Side::Right,
                    slot: 0,
                    action: ButtonAction::Settings,
                    label: None,
                },
                SideButton {
                    id: "volume".to_string(),
                    side: Side::Right,
                    slot: 1,
                    action: ButtonAction::Volume,
                    label: None,
                },
                SideButton {
                    id: "nowplaying".to_string(),
                    side: Side::Bottom,
                    slot: 0,
                    action: ButtonAction::NowPlaying,
                    label: None,
                },
            ],
        }
    }
}

/// Result of [`Layout::from_json`]: tolerated problems are counted, never
/// fatal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoadReport {
    /// Number of buttons skipped (invalid id / duplicate id / slot out of
    /// range / occupied pair / invalid action / invalid label / non-object or
    /// wrong-typed element / non-array `buttons` field).
    pub skipped: u32,
    /// True when `preset` was present but invalid and fell back to Compact.
    pub preset_fallback: bool,
    /// True when `buttonDiameter` was present but invalid and fell back to
    /// [`DEFAULT_BUTTON_DIAMETER`].
    pub diameter_fallback: bool,
    /// True when `gap` was present but invalid and fell back to [`DEFAULT_GAP`].
    pub gap_fallback: bool,
}

/// True when `id` is 1..=64 chars of [A-Za-z0-9_.-].
pub(crate) fn valid_id(id: &str) -> bool {
    let mut len = 0usize;
    for c in id.chars() {
        if !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_') {
            return false;
        }
        len += 1;
        if len > MAX_ID_LEN {
            return false;
        }
    }
    len >= MIN_ID_LEN
}

/// True when `label` is at most [`MAX_LABEL_CHARS`] chars.
pub(crate) fn valid_label(label: &str) -> bool {
    label.chars().count() <= MAX_LABEL_CHARS
}

impl Layout {
    fn find_index(&self, id: &str) -> Option<usize> {
        self.buttons.iter().position(|b| b.id == id)
    }

    fn occupied(&self, side: Side, slot: u8) -> bool {
        self.buttons
            .iter()
            .any(|b| b.side == side && b.slot == slot)
    }

    /// Add a button. Validates id (1..=64 chars of [A-Za-z0-9_.-]),
    /// action, label length, slot range and global uniqueness (id and
    /// (side, slot) pair). On any error the layout is unchanged.
    pub fn add(&mut self, button: SideButton) -> Result<(), LayoutError> {
        if !valid_id(&button.id) {
            return Err(LayoutError::InvalidId);
        }
        if !button.action.is_valid() {
            return Err(LayoutError::InvalidAction);
        }
        if button.label.as_ref().is_some_and(|l| !valid_label(l)) {
            return Err(LayoutError::LabelTooLong);
        }
        if button.slot >= button.side.capacity() {
            return Err(LayoutError::SlotOutOfRange);
        }
        if self.find_index(&button.id).is_some() {
            return Err(LayoutError::DuplicateId);
        }
        if self.occupied(button.side, button.slot) {
            return Err(LayoutError::OccupiedSlot);
        }
        self.buttons.push(button);
        Ok(())
    }

    /// Remove the button with `id`.
    pub fn remove(&mut self, id: &str) -> Result<(), LayoutError> {
        let idx = self.find_index(id).ok_or(LayoutError::NotFound)?;
        self.buttons.remove(idx);
        Ok(())
    }

    /// Move the button with `id` to `(side, slot)`. Moving to the button's
    /// current slot is a no-op. If the target slot is occupied by another
    /// button, the two buttons swap places (not an error).
    pub fn move_to(&mut self, id: &str, side: Side, slot: u8) -> Result<(), LayoutError> {
        if slot >= side.capacity() {
            return Err(LayoutError::SlotOutOfRange);
        }
        let idx = self.find_index(id).ok_or(LayoutError::NotFound)?;
        let cur_side = self.buttons[idx].side;
        let cur_slot = self.buttons[idx].slot;
        if cur_side == side && cur_slot == slot {
            return Ok(());
        }
        if let Some(other) = self
            .buttons
            .iter_mut()
            .find(|b| b.side == side && b.slot == slot && b.id != id)
        {
            other.side = cur_side;
            other.slot = cur_slot;
        }
        self.buttons[idx].side = side;
        self.buttons[idx].slot = slot;
        Ok(())
    }

    /// Change a button's action and label.
    pub fn edit(
        &mut self,
        id: &str,
        action: ButtonAction,
        label: Option<String>,
    ) -> Result<(), LayoutError> {
        if !action.is_valid() {
            return Err(LayoutError::InvalidAction);
        }
        if label.as_ref().is_some_and(|l| !valid_label(l)) {
            return Err(LayoutError::LabelTooLong);
        }
        let idx = self.find_index(id).ok_or(LayoutError::NotFound)?;
        self.buttons[idx].action = action;
        self.buttons[idx].label = label;
        Ok(())
    }

    /// Set the island size preset. A `Custom` preset whose dimensions are not
    /// finite or are outside the documented bounds is a typed error.
    pub fn set_preset(&mut self, preset: SizePreset) -> Result<(), LayoutError> {
        if !preset.is_valid() {
            return Err(LayoutError::InvalidPreset);
        }
        self.preset = preset;
        Ok(())
    }

    /// Set the side-button diameter (28..=56, finite).
    pub fn set_button_diameter(&mut self, d: f64) -> Result<(), LayoutError> {
        if !d.is_finite() || !(MIN_BUTTON_DIAMETER..=MAX_BUTTON_DIAMETER).contains(&d) {
            return Err(LayoutError::InvalidDiameter);
        }
        self.button_diameter = d;
        Ok(())
    }

    /// Set the island-to-button gap (4..=24, finite).
    pub fn set_gap(&mut self, gap: f64) -> Result<(), LayoutError> {
        if !gap.is_finite() || !(MIN_GAP..=MAX_GAP).contains(&gap) {
            return Err(LayoutError::InvalidGap);
        }
        self.gap = gap;
        Ok(())
    }

    /// Empty (add-button) slots in display order: Left 0..5, Right 0..5,
    /// Bottom 0..7, ascending within each side.
    pub fn empty_slots(&self) -> Vec<(Side, u8)> {
        let mut out = Vec::new();
        for side in [Side::Left, Side::Right, Side::Bottom] {
            for slot in 0..side.capacity() {
                if !self.occupied(side, slot) {
                    out.push((side, slot));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_validation_boundaries() {
        assert!(valid_id("a"));
        assert!(valid_id("Z9"));
        assert!(valid_id("a.b_c-d"));
        assert!(valid_id(&"a".repeat(64)));
        assert!(valid_id(&format!("{}.{}", "a".repeat(62), "Z")));
        assert!(!valid_id(""));
        assert!(!valid_id(&"a".repeat(65)));
        assert!(!valid_id("a b"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id("é"));
        assert!(!valid_id("#"));
        assert!(!valid_id("a\n"));
    }

    #[test]
    fn label_validation_boundaries() {
        assert!(valid_label(""));
        assert!(valid_label(&"a".repeat(32)));
        assert!(!valid_label(&"a".repeat(33)));
        assert!(valid_label(&"é".repeat(32)));
        assert!(!valid_label(&"é".repeat(33)));
    }

    #[test]
    fn side_capacities() {
        assert_eq!(Side::Left.capacity(), 5);
        assert_eq!(Side::Right.capacity(), 5);
        assert_eq!(Side::Bottom.capacity(), 7);
    }
}
