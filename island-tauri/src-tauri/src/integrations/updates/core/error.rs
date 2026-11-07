use std::fmt;

/// Errors from parsing update-command output or a failed check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatesError {
    /// `stdout` exceeds the 1 MiB cap.
    StdoutTooLarge,
    /// More than 10 000 non-blank lines.
    TooManyLines,
    /// The command reported a failure (unknown exit code, or `paru`
    /// stderr noise).
    CommandFailed,
    /// Every non-blank line present was malformed; zero updates parsed.
    AllMalformed,
    /// `dnf check-upgrade --json` exited 0 but stdout was not the documented JSON object.
    InvalidJson,
}

impl fmt::Display for UpdatesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            UpdatesError::StdoutTooLarge => "stdout exceeds the 1 MiB cap",
            UpdatesError::TooManyLines => "more than 10 000 lines",
            UpdatesError::CommandFailed => "command reported a failure",
            UpdatesError::AllMalformed => "every line was malformed",
            UpdatesError::InvalidJson => "output was not the expected JSON",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for UpdatesError {}
