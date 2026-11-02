//! Command builders for the tile actions. Nothing here executes anything;
//! every builder validates its inputs and returns a typed [`CmdError`] on
//! invalid arguments.

use crate::integrations::tiles::core::error::CmdError;
use crate::integrations::tiles::core::model::VoiceList;

/// Longest accepted sink name in bytes.
pub const MAX_SINK_NAME_BYTES: usize = 256;

/// `hermes-fish` subcommand words that are never valid voice names.
const RESERVED_VOICE_ARGS: [&str; 5] = ["current", "add", "remove", "id", "list"];

/// `pactl set-default-sink <name>`. The name must be non-empty, at most 256
/// bytes, not start with `-` (argv-injection guard), and contain no
/// whitespace or control characters.
pub fn set_default_argv(name: &str) -> Result<Vec<String>, CmdError> {
    let valid = !name.is_empty()
        && name.len() <= MAX_SINK_NAME_BYTES
        && !name.starts_with('-')
        && !name.chars().any(|c| c.is_whitespace() || c.is_control());
    if !valid {
        return Err(CmdError::InvalidSinkName);
    }
    Ok(vec![
        "pactl".to_string(),
        "set-default-sink".to_string(),
        name.to_string(),
    ])
}

/// `hermes-fish <name>` for a voice present in the parsed list. Reserved
/// subcommand words (`current`, `add`, `remove`, `id`, `list`) are always
/// rejected, even if they were in the list.
pub fn switch_argv(list: &VoiceList, name: &str) -> Result<Vec<String>, CmdError> {
    if RESERVED_VOICE_ARGS.contains(&name) {
        return Err(CmdError::ReservedWord);
    }
    if !list.voices.iter().any(|v| v == name) {
        return Err(CmdError::UnknownVoice);
    }
    Ok(vec!["hermes-fish".to_string(), name.to_string()])
}

/// `spectacle -r` (region screenshot); a constant per SPEC §0.5 item 6.
pub fn screenshot_argv() -> Vec<String> {
    vec!["spectacle".to_string(), "-r".to_string()]
}
