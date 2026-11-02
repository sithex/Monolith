use std::fmt;

/// Errors produced by the clipboard history store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Configuration was rejected (capacity clamp is silent; deny-pattern
    /// violations are hard errors).
    InvalidConfig(String),
    /// Persisted data used an unknown format version.
    UnknownVersion(u64),
    /// Persisted data was malformed.
    Corrupt(String),
    /// `pin`/`delete`/`get` referenced a non-existent id.
    NotFound,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidConfig(msg) => write!(f, "invalid configuration: {msg}"),
            Error::UnknownVersion(v) => write!(f, "unknown persisted format version: {v}"),
            Error::Corrupt(msg) => write!(f, "corrupt persisted data: {msg}"),
            Error::NotFound => write!(f, "entry not found"),
        }
    }
}

impl std::error::Error for Error {}
