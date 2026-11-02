use std::fmt;

/// Errors from parsing pactl text output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// Input exceeds the 4 MiB cap.
    TooLarge,
    /// Input is not valid JSON.
    InvalidJson,
    /// The top-level JSON value is not an array.
    NotAnArray,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ParseError::TooLarge => "input exceeds the 4 MiB cap",
            ParseError::InvalidJson => "input is not valid JSON",
            ParseError::NotAnArray => "top-level JSON value is not an array",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for ParseError {}

/// Errors from building pactl command argv.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdError {
    /// Sink name is empty, over 256 bytes, contains whitespace or NUL, or
    /// starts with `-`.
    InvalidSinkName,
    /// Volume percent outside 0..=150.
    PercentOutOfRange,
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            CmdError::InvalidSinkName => {
                "sink name must be non-empty, ≤ 256 bytes, no whitespace/NUL, no leading '-'"
            }
            CmdError::PercentOutOfRange => "volume percent must be within 0..=150",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for CmdError {}
