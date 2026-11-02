use std::fmt;

/// Typed errors returned by the scratchpad core. Operations that fail leave
/// the scratchpad state unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScratchpadError {
    /// An id is not 1..=64 chars of [A-Za-z0-9_-].
    InvalidId,
    /// An id already exists in the scratchpad.
    DuplicateId,
    /// No note has the requested id.
    NotFound,
    /// The body exceeds the 256 KiB limit (bytes).
    BodyTooLarge,
    /// The scratchpad is already at the 200-note maximum.
    TooManyNotes,
    /// The file is not valid JSON.
    InvalidJson,
    /// The top-level JSON value is not an object.
    NotAnObject,
    /// The file's `version` is missing or not 1.
    InvalidVersion,
    /// The top-level `notes` field is present but not an array.
    NotesNotArray,
}

impl fmt::Display for ScratchpadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ScratchpadError::InvalidId => "note id must be 1..=64 chars of [A-Za-z0-9_-]",
            ScratchpadError::DuplicateId => "a note with this id already exists",
            ScratchpadError::NotFound => "no note with this id",
            ScratchpadError::BodyTooLarge => "note body exceeds the 256 KiB limit",
            ScratchpadError::TooManyNotes => "scratchpad already holds its maximum 200 notes",
            ScratchpadError::InvalidJson => "file is not valid JSON",
            ScratchpadError::NotAnObject => "top-level JSON value is not an object",
            ScratchpadError::InvalidVersion => {
                "file version is missing or unsupported (expected 1)"
            }
            ScratchpadError::NotesNotArray => "top-level \"notes\" is not an array",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for ScratchpadError {}
