use std::fmt;

/// Errors produced by [`build_dashboard`](crate::build_dashboard).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashError {
    /// `utc_offset_s` is outside the valid range −14h..=+14h.
    InvalidUtcOffset(i32),
}

impl fmt::Display for DashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DashError::InvalidUtcOffset(v) => write!(f, "invalid UTC offset seconds: {v}"),
        }
    }
}

impl std::error::Error for DashError {}
