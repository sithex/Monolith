use std::fmt;

/// Typed errors produced by the crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IcsError {
    /// Input text exceeds 8 MiB.
    TooLarge,
    /// Input text contains a NUL byte.
    ContainsNul,
    /// No `BEGIN:VCALENDAR` component found.
    MissingCalendar,
    /// Agenda window is longer than 400 days.
    WindowTooLong,
}

impl fmt::Display for IcsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IcsError::TooLarge => write!(f, "input larger than 8 MiB"),
            IcsError::ContainsNul => write!(f, "input contains a NUL byte"),
            IcsError::MissingCalendar => write!(f, "no VCALENDAR component found"),
            IcsError::WindowTooLong => write!(f, "agenda window longer than 400 days"),
        }
    }
}

impl std::error::Error for IcsError {}

/// A non-fatal condition detected while parsing or expanding.
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
