//! Command builders: produce the argv for `pactl` volume/mute commands.
//! Every builder validates its inputs and returns a typed [`CmdError`] on
//! invalid arguments; nothing here ever executes a command.

use crate::error::CmdError;
use crate::model::{AppGroup, Sink};

/// Highest accepted volume percent (values above this are a typed error,
/// never silently clamped).
pub const MAX_PERCENT: u32 = 150;

/// Longest accepted sink name in bytes.
pub const MAX_SINK_NAME_BYTES: usize = 256;

/// The mute argument for `pactl set-sink-mute` / `set-sink-input-mute`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteOp {
    /// `0` — unmute.
    Off,
    /// `1` — mute.
    On,
    /// `toggle`.
    Toggle,
}

impl MuteOp {
    fn as_arg(self) -> &'static str {
        match self {
            MuteOp::Off => "0",
            MuteOp::On => "1",
            MuteOp::Toggle => "toggle",
        }
    }
}

fn validate_pct(pct: u32) -> Result<(), CmdError> {
    if pct > MAX_PERCENT {
        Err(CmdError::PercentOutOfRange)
    } else {
        Ok(())
    }
}

/// Sink names must be non-empty, at most 256 bytes, contain no whitespace
/// or NUL, and not start with `-` (argv-injection guard).
fn validate_sink_name(name: &str) -> Result<(), CmdError> {
    let valid = !name.is_empty()
        && name.len() <= MAX_SINK_NAME_BYTES
        && !name.contains('\0')
        && !name.contains(char::is_whitespace)
        && !name.starts_with('-');
    if valid {
        Ok(())
    } else {
        Err(CmdError::InvalidSinkName)
    }
}

/// `pactl set-sink-volume <name> <N>%`
pub fn set_sink_volume(sink: &Sink, pct: u32) -> Result<Vec<String>, CmdError> {
    validate_sink_name(&sink.name)?;
    validate_pct(pct)?;
    Ok(vec![
        "pactl".to_string(),
        "set-sink-volume".to_string(),
        sink.name.clone(),
        format!("{pct}%"),
    ])
}

/// `pactl set-sink-input-volume <index> <N>%`
pub fn set_stream_volume(index: u32, pct: u32) -> Result<Vec<String>, CmdError> {
    validate_pct(pct)?;
    Ok(vec![
        "pactl".to_string(),
        "set-sink-input-volume".to_string(),
        index.to_string(),
        format!("{pct}%"),
    ])
}

/// One `set-sink-input-volume` argv per stream in the group.
pub fn set_app_volume(group: &AppGroup, pct: u32) -> Result<Vec<Vec<String>>, CmdError> {
    validate_pct(pct)?;
    Ok(group
        .streams
        .iter()
        .map(|&index| {
            vec![
                "pactl".to_string(),
                "set-sink-input-volume".to_string(),
                index.to_string(),
                format!("{pct}%"),
            ]
        })
        .collect())
}

/// `pactl set-sink-mute <name> <0|1|toggle>`
pub fn mute_sink(sink: &Sink, op: MuteOp) -> Result<Vec<String>, CmdError> {
    validate_sink_name(&sink.name)?;
    Ok(vec![
        "pactl".to_string(),
        "set-sink-mute".to_string(),
        sink.name.clone(),
        op.as_arg().to_string(),
    ])
}

/// `pactl set-sink-input-mute <index> <0|1|toggle>`
pub fn mute_stream(index: u32, op: MuteOp) -> Result<Vec<String>, CmdError> {
    Ok(vec![
        "pactl".to_string(),
        "set-sink-input-mute".to_string(),
        index.to_string(),
        op.as_arg().to_string(),
    ])
}

/// `pactl set-default-sink <name>`
pub fn set_default_sink(sink: &Sink) -> Result<Vec<String>, CmdError> {
    validate_sink_name(&sink.name)?;
    Ok(vec![
        "pactl".to_string(),
        "set-default-sink".to_string(),
        sink.name.clone(),
    ])
}

/// `pactl move-sink-input <index> <sink-name>`
pub fn move_stream(index: u32, sink: &Sink) -> Result<Vec<String>, CmdError> {
    validate_sink_name(&sink.name)?;
    Ok(vec![
        "pactl".to_string(),
        "move-sink-input".to_string(),
        index.to_string(),
        sink.name.clone(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Sink;

    fn odd_sink(name: &str) -> Sink {
        Sink {
            index: 0,
            name: name.to_string(),
            description: String::new(),
            muted: false,
            volume_percent: None,
            channels: 0,
            state: String::new(),
        }
    }

    #[test]
    fn mute_op_args() {
        assert_eq!(MuteOp::Off.as_arg(), "0");
        assert_eq!(MuteOp::On.as_arg(), "1");
        assert_eq!(MuteOp::Toggle.as_arg(), "toggle");
    }

    #[test]
    fn percent_bounds() {
        assert_eq!(validate_pct(0), Ok(()));
        assert_eq!(validate_pct(150), Ok(()));
        assert_eq!(validate_pct(151), Err(CmdError::PercentOutOfRange));
        assert_eq!(validate_pct(u32::MAX), Err(CmdError::PercentOutOfRange));
    }

    #[test]
    fn sink_name_validation() {
        assert_eq!(validate_sink_name("a"), Ok(()));
        assert_eq!(validate_sink_name(&"a".repeat(256)), Ok(()));
        assert_eq!(validate_sink_name("a-b_c.0"), Ok(()));
        assert_eq!(validate_sink_name(""), Err(CmdError::InvalidSinkName));
        assert_eq!(
            validate_sink_name(&"a".repeat(257)),
            Err(CmdError::InvalidSinkName)
        );
        assert_eq!(validate_sink_name("a b"), Err(CmdError::InvalidSinkName));
        assert_eq!(validate_sink_name("a\tb"), Err(CmdError::InvalidSinkName));
        assert_eq!(validate_sink_name("a\nb"), Err(CmdError::InvalidSinkName));
        assert_eq!(validate_sink_name("-x"), Err(CmdError::InvalidSinkName));
        assert_eq!(validate_sink_name("a\0b"), Err(CmdError::InvalidSinkName));
    }

    #[test]
    fn name_is_checked_before_percent() {
        assert_eq!(
            set_sink_volume(&odd_sink("-bad"), 200),
            Err(CmdError::InvalidSinkName)
        );
    }
}
