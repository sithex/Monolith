use std::fmt;

/// Typed errors returned by the launcher-arrange core. Operations that fail
/// leave the arrangement state unchanged (atomic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrangeError {
    /// `Arrangement::new` was called with no items.
    EmptyIds,
    /// An item id is longer than 64 bytes.
    IdTooLong,
    /// Two items share the same id.
    DuplicateId,
    /// More than 256 items were supplied to `new`.
    TooManyItems,
    /// An edit op (move/disable/enable/cancel/commit) ran outside an edit
    /// session.
    NotEditing,
    /// No item carries the requested id.
    UnknownId,
    /// `disable` was attempted on a locked item.
    Locked,
    /// `begin_edit` was called while an edit session was already open.
    AlreadyEditing,
    /// `move_item` was called on an item that is not visible (disabled).
    NotVisible,
}

impl fmt::Display for ArrangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ArrangeError::EmptyIds => "item list must not be empty",
            ArrangeError::IdTooLong => "item id exceeds the 64-byte limit",
            ArrangeError::DuplicateId => "item ids must be unique",
            ArrangeError::TooManyItems => "more than 256 items are not allowed",
            ArrangeError::NotEditing => "no edit session is open",
            ArrangeError::UnknownId => "no item has this id",
            ArrangeError::Locked => "this item is locked and cannot be disabled",
            ArrangeError::AlreadyEditing => "an edit session is already open",
            ArrangeError::NotVisible => "this item is hidden; only visible items can be moved",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for ArrangeError {}
