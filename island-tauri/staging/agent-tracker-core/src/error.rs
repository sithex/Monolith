use std::fmt;

/// Errors from constructing a [`TrackerConfig`](crate::TrackerConfig).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// `staleAfterSecs` outside 60..=86400.
    StaleAfterSecsOutOfRange,
    /// `lookbackSecs` is zero (must be positive).
    LookbackSecsZero,
    /// `maxChips` outside 1..=32.
    MaxChipsOutOfRange,
    /// A delegated source entry is an empty string.
    EmptyDelegatedSource,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ConfigError::StaleAfterSecsOutOfRange => "staleAfterSecs must be within 60..=86400",
            ConfigError::LookbackSecsZero => "lookbackSecs must be positive",
            ConfigError::MaxChipsOutOfRange => "maxChips must be within 1..=32",
            ConfigError::EmptyDelegatedSource => {
                "delegatedSources entries must be non-empty strings"
            }
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for ConfigError {}
