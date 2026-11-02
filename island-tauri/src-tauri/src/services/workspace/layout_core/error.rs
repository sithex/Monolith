use std::fmt;

/// Typed errors returned by the layout core. Operations that fail leave the
/// layout state unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutError {
    /// An id is not 1..=64 chars of [A-Za-z0-9_.-].
    InvalidId,
    /// An action carries an invalid embedded id (appId / id).
    InvalidAction,
    /// A label exceeds 32 chars.
    LabelTooLong,
    /// An id already exists.
    DuplicateId,
    /// No button has the requested id.
    NotFound,
    /// The slot is out of range for the side.
    SlotOutOfRange,
    /// The (side, slot) pair is already occupied.
    OccupiedSlot,
    /// A Custom preset has non-finite or out-of-range dimensions.
    InvalidPreset,
    /// The button diameter is not finite or outside 28..=56.
    InvalidDiameter,
    /// The gap is not finite or outside 4..=24.
    InvalidGap,
    /// Island geometry is not finite.
    NonFiniteGeometry,
    /// Island size is zero or negative.
    NegativeSizeGeometry,
    /// The file is not valid JSON.
    InvalidJson,
    /// The top-level JSON value is not an object.
    NotAnObject,
    /// The file's `version` is missing or not 1.
    InvalidVersion,
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            LayoutError::InvalidId => "id must be 1..=64 chars of [A-Za-z0-9_.-]",
            LayoutError::InvalidAction => "action carries an invalid embedded id (appId / id)",
            LayoutError::LabelTooLong => "label exceeds 32 chars",
            LayoutError::DuplicateId => "a button with this id already exists",
            LayoutError::NotFound => "no button with this id",
            LayoutError::SlotOutOfRange => "slot is out of range for the side",
            LayoutError::OccupiedSlot => "the (side, slot) pair is already occupied",
            LayoutError::InvalidPreset => {
                "Custom preset dimensions must be finite, width 300..=1800, max-height 240..=1280"
            }
            LayoutError::InvalidDiameter => "button diameter must be finite and within 28..=56",
            LayoutError::InvalidGap => "gap must be finite and within 4..=24",
            LayoutError::NonFiniteGeometry => "island geometry must be finite",
            LayoutError::NegativeSizeGeometry => "island size must be positive",
            LayoutError::InvalidJson => "file is not valid JSON",
            LayoutError::NotAnObject => "top-level JSON value is not an object",
            LayoutError::InvalidVersion => "file version is missing or unsupported (expected 1)",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for LayoutError {}
