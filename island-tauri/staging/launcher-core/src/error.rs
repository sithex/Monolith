use std::fmt;

/// Typed errors returned by `parse_desktop_entry`. Nothing is partially
/// consumed: an error means no entry is produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// No `[Desktop Entry]` group is present in the file.
    NoDesktopEntryGroup,
    /// The `[Desktop Entry]` group has no `Type` key.
    MissingType,
    /// The `Type` key is present but not exactly `Application`.
    NotApplication(String),
    /// No `Name` key resolves for the requested locale.
    MissingName,
    /// No `Exec` key is present.
    MissingExec,
    /// Input exceeds 64 KiB (bytes).
    InputTooLarge,
    /// Input contains a NUL character.
    ContainsNul,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ParseError::NoDesktopEntryGroup => "no [Desktop Entry] group found",
            ParseError::MissingType => "missing Type key in [Desktop Entry]",
            ParseError::NotApplication(t) => {
                return write!(f, "Type is {t:?}, expected \"Application\"");
            }
            ParseError::MissingName => "missing Name key in [Desktop Entry]",
            ParseError::MissingExec => "missing Exec key in [Desktop Entry]",
            ParseError::InputTooLarge => "input exceeds the 64 KiB limit",
            ParseError::ContainsNul => "input contains a NUL character",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for ParseError {}

/// Typed errors returned by `build_argv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecError {
    /// A `%x` field code not in the recognised set (incl. deprecated).
    UnknownFieldCode(char),
    /// More than one of `%f`/`%F`/`%u`/`%U` on the command line.
    MultipleFileCodes,
    /// `%F`, `%U` or `%i` used inside a larger argument.
    FieldCodeNotStandalone(char),
    /// A field code appeared inside a double-quoted argument (undefined by spec).
    FieldCodeInQuotedArgument,
    /// A double quote was opened but never closed.
    UnterminatedQuote,
    /// The expanded command line is empty.
    EmptyCommand,
    /// The executable (first argument) contains `=`.
    ExecutableContainsEquals,
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ExecError::UnknownFieldCode(c) => {
                return write!(f, "unknown field code %{c}");
            }
            ExecError::MultipleFileCodes => {
                "more than one of %f / %F / %u / %U on the command line"
            }
            ExecError::FieldCodeNotStandalone(c) => {
                return write!(f, "field code %{c} must be an argument on its own");
            }
            ExecError::FieldCodeInQuotedArgument => {
                "field code inside a double-quoted argument (undefined by spec)"
            }
            ExecError::UnterminatedQuote => "unterminated double quote",
            ExecError::EmptyCommand => "empty command line",
            ExecError::ExecutableContainsEquals => "executable must not contain '='",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for ExecError {}
