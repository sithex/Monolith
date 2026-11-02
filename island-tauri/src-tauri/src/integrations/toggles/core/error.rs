use std::fmt;

/// Errors from parsing `pactl get-source-mute` stdout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// Output exceeds 256 bytes.
    TooLarge,
    /// Output is not exactly `Mute: yes` / `Mute: no` (optionally
    /// followed by one `\n` or `\r\n`).
    InvalidOutput,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ParseError::TooLarge => "mute output exceeds 256 bytes",
            ParseError::InvalidOutput => "unexpected pactl mute output",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for ParseError {}
