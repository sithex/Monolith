use std::fmt;

/// Errors from parsing the tile inputs. Every parser is total: oversized
/// input, malformed lines and unknown values are a typed error or a counted
/// warning, never a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// Input exceeds the parser's size cap (the cap is carried in the error).
    TooLarge { limit: usize },
    /// Input is not valid JSON.
    InvalidJson,
    /// The top-level JSON value is not an array (pactl sinks).
    NotAnArray,
    /// The top-level JSON value is not an object (tailscale status).
    NotAnObject,
    /// tailscale status JSON is missing a string `BackendState`.
    MissingBackendState,
    /// More than one voice line is marked `[active]`.
    MultipleActiveVoices,
    /// `parse_current` input is not a valid voice name.
    InvalidVoiceName,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::TooLarge { limit } => write!(f, "input exceeds the {limit}-byte cap"),
            ParseError::InvalidJson => write!(f, "input is not valid JSON"),
            ParseError::NotAnArray => write!(f, "top-level JSON value is not an array"),
            ParseError::NotAnObject => write!(f, "top-level JSON value is not an object"),
            ParseError::MissingBackendState => {
                write!(f, "tailscale status JSON is missing BackendState")
            }
            ParseError::MultipleActiveVoices => {
                write!(f, "more than one voice line is marked [active]")
            }
            ParseError::InvalidVoiceName => write!(f, "not a valid voice name"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Errors from building command argv.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdError {
    /// Sink name is empty, over 256 bytes, contains whitespace/control
    /// characters, or starts with `-`.
    InvalidSinkName,
    /// The requested voice is a reserved `hermes-fish` subcommand word
    /// (`current`, `add`, `remove`, `id`, `list`).
    ReservedWord,
    /// The requested voice is not present in the parsed voice list.
    UnknownVoice,
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmdError::InvalidSinkName => write!(
                f,
                "sink name must be non-empty, ≤ 256 bytes, no whitespace/control chars, no leading '-'"
            ),
            CmdError::ReservedWord => {
                write!(f, "reserved hermes-fish subcommand word")
            }
            CmdError::UnknownVoice => write!(f, "voice not in the parsed voice list"),
        }
    }
}

impl std::error::Error for CmdError {}
