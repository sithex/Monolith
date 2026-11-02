//! Mic-mute tile logic (PipeWire's pulse server via `pactl`). Only
//! argv building and text parsing live here; the Tauri side runs the
//! process (with LC_ALL=C) and feeds us stdout.

use crate::integrations::toggles::core::error::ParseError;

/// The default audio source target. Always used, never a device name
/// (nick-pc's default source is a DeepFilterNet virtual source).
pub const DEFAULT_SOURCE_ARG: &str = "@DEFAULT_SOURCE@";

/// Maximum accepted `get-source-mute` stdout size in bytes.
pub const MAX_MUTE_OUTPUT_BYTES: usize = 256;

/// The mute operation for `pactl set-source-mute`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicOp {
    Mute,
    Unmute,
    Toggle,
}

impl MicOp {
    fn as_arg(self) -> &'static str {
        match self {
            MicOp::Mute => "1",
            MicOp::Unmute => "0",
            MicOp::Toggle => "toggle",
        }
    }
}

/// `pactl get-source-mute @DEFAULT_SOURCE@`
pub fn mic_get_argv() -> Vec<String> {
    vec![
        "pactl".to_string(),
        "get-source-mute".to_string(),
        DEFAULT_SOURCE_ARG.to_string(),
    ]
}

/// `pactl set-source-mute @DEFAULT_SOURCE@ <0|1|toggle>`
pub fn mic_set_argv(op: MicOp) -> Vec<String> {
    vec![
        "pactl".to_string(),
        "set-source-mute".to_string(),
        DEFAULT_SOURCE_ARG.to_string(),
        op.as_arg().to_string(),
    ]
}

/// Parse `pactl get-source-mute` stdout. Accepts exactly `Mute: yes` or
/// `Mute: no`, optionally followed by a single `\n` or `\r\n`. Anything
/// else (localized text, extra lines, empty, whitespace, over 256 bytes)
/// is a typed error — never a guess.
pub fn parse_mic_mute(stdout: &str) -> Result<bool, ParseError> {
    if stdout.len() > MAX_MUTE_OUTPUT_BYTES {
        return Err(ParseError::TooLarge);
    }
    let body = stdout
        .strip_suffix("\r\n")
        .or_else(|| stdout.strip_suffix('\n'))
        .unwrap_or(stdout);
    match body {
        "Mute: yes" => Ok(true),
        "Mute: no" => Ok(false),
        _ => Err(ParseError::InvalidOutput),
    }
}
