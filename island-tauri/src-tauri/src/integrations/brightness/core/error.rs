//! Typed errors for name validation, percent conversion, and drag control.

use std::fmt;

/// Errors from validating a D-Bus display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    /// Name is empty, longer than 255 bytes, or contains characters outside
    /// ASCII `[A-Za-z0-9_]`.
    Invalid,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "display name must be 1..=255 ASCII [A-Za-z0-9_] bytes")
    }
}

impl std::error::Error for NameError {}

/// Errors from converting between brightness percent and raw values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueError {
    /// Percent is outside 0..=100.
    PercentOutOfRange,
    /// Max brightness is not positive (display unavailable).
    MaxBrightnessNotPositive,
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ValueError::PercentOutOfRange => "brightness percent must be within 0..=100",
            ValueError::MaxBrightnessNotPositive => "max brightness must be positive",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for ValueError {}

/// Errors from starting a slider drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragError {
    /// No display with that name in the model.
    UnknownDisplay,
    /// The display exists but is unavailable (max brightness <= 0).
    UnavailableDisplay,
    /// Percent is outside 0..=100.
    PercentOutOfRange,
}

impl fmt::Display for DragError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            DragError::UnknownDisplay => "unknown display",
            DragError::UnavailableDisplay => "display is unavailable (max brightness <= 0)",
            DragError::PercentOutOfRange => "brightness percent must be within 0..=100",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for DragError {}
