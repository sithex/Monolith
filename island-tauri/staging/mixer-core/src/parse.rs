//! Parsing of `pactl` text output into [`Sink`] and [`Stream`] records.
//!
//! Tolerant per element: an element missing its required fields
//! (`index`/`name`/`volume` for sinks, `index`/`sink`/`volume` for
//! streams) or carrying wrong types is skipped and counted. The top level
//! must be a JSON array (else a typed error), and input larger than
//! [`MAX_INPUT_BYTES`] is rejected before parsing.

use crate::error::ParseError;
use crate::model::{Sink, Stream};
use serde_json::{Map, Value};

/// Maximum accepted pactl payload size in bytes (4 MiB).
pub const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;

/// PipeWire/pulse "norm" volume: 65536 = 100 % (PA_VOLUME_NORM).
pub const PA_VOLUME_NORM: u64 = 65536;

/// Convert a pactl volume `value` to a percent: `round(value × 100 /
/// 65536)` with ties rounding half away from zero (round half up). The
/// exact half occurs when `value × 100 ≡ 32768 (mod 65536)`, e.g. value
/// 8192 → 12.5 % → 13 %. Saturates at `u32::MAX` for pathological inputs;
/// never panics.
pub(crate) fn value_to_percent(value: u64) -> u32 {
    let pct = (value as u128 * 100 + (PA_VOLUME_NORM as u128 / 2)) / PA_VOLUME_NORM as u128;
    pct.min(u32::MAX as u128) as u32
}

/// Average of channel percents; `values` must be non-empty.
fn avg_channels(values: &[u32]) -> u32 {
    let sum: u64 = values.iter().map(|&v| v as u64).sum();
    (sum / values.len() as u64) as u32
}

/// Extract valid channel percents from a `volume` object. A channel is
/// valid when its entry is an object with an integer `value`; entries of
/// any other shape are ignored (not counted, not averaged).
fn channel_percents(volume: &Value) -> Vec<u32> {
    let mut out = Vec::new();
    if let Some(map) = volume.as_object() {
        for v in map.values() {
            if let Some(val) = v.get("value").and_then(|x| x.as_u64()) {
                out.push(value_to_percent(val));
            }
        }
    }
    out
}

/// Parse the top level as a JSON array, enforcing the size cap.
fn top_level_array(input: &str) -> Result<Vec<Value>, ParseError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(ParseError::TooLarge);
    }
    let value: Value = serde_json::from_str(input).map_err(|_| ParseError::InvalidJson)?;
    Ok(value.as_array().ok_or(ParseError::NotAnArray)?.clone())
}

fn get_u32(obj: &Map<String, Value>, key: &str) -> Option<u32> {
    obj.get(key)
        .and_then(|v| v.as_u64())
        .and_then(|n| u32::try_from(n).ok())
}

fn get_str(obj: &Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn volume_field(obj: &Map<String, Value>) -> Option<&Value> {
    obj.get("volume").filter(|v| v.is_object())
}

/// Parse one sinks-array element tolerantly; `None` when skipped.
fn parse_sink_element(item: &Value) -> Option<Sink> {
    let obj = item.as_object()?;
    let index = get_u32(obj, "index")?;
    let name = get_str(obj, "name")?;
    let volume = volume_field(obj)?;
    let channels = channel_percents(volume);
    let volume_percent = if channels.is_empty() {
        None
    } else {
        Some(avg_channels(&channels))
    };
    Some(Sink {
        index,
        name,
        description: get_str(obj, "description").unwrap_or_default(),
        muted: obj.get("mute").and_then(|v| v.as_bool()).unwrap_or(false),
        volume_percent,
        channels: channels.len() as u8,
        state: get_str(obj, "state").unwrap_or_default(),
    })
}

/// Parse one sink-inputs-array element tolerantly; `None` when skipped.
fn parse_stream_element(item: &Value) -> Option<Stream> {
    let obj = item.as_object()?;
    let index = get_u32(obj, "index")?;
    let sink_index = get_u32(obj, "sink")?;
    let volume = volume_field(obj)?;
    let channels = channel_percents(volume);
    let volume_percent = if channels.is_empty() {
        None
    } else {
        Some(avg_channels(&channels))
    };
    let props = obj.get("properties").and_then(|v| v.as_object());
    let prop_str = |key: &str| {
        props
            .and_then(|p| p.get(key))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    let application_name = prop_str("application.name");
    let process_binary = prop_str("application.process.binary");
    let media_name = prop_str("media.name");
    let app_name = application_name
        .as_ref()
        .or(process_binary.as_ref())
        .or(media_name.as_ref())
        .cloned()
        .unwrap_or_else(|| "Unknown".to_string());
    let pid = props
        .and_then(|p| p.get("application.process.id"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u32>().ok());
    Some(Stream {
        index,
        sink_index,
        app_name,
        binary: process_binary,
        pid,
        media_name,
        icon_name: prop_str("application.icon_name"),
        muted: obj.get("mute").and_then(|v| v.as_bool()).unwrap_or(false),
        corked: obj.get("corked").and_then(|v| v.as_bool()).unwrap_or(false),
        volume_percent,
    })
}

/// Parse `pactl -f json list sinks` output. Returns the parsed sinks and
/// the number of skipped (invalid) elements.
pub fn parse_sinks_report(input: &str) -> Result<(Vec<Sink>, u32), ParseError> {
    let array = top_level_array(input)?;
    let mut sinks = Vec::new();
    let mut skipped = 0u32;
    for item in &array {
        match parse_sink_element(item) {
            Some(s) => sinks.push(s),
            None => skipped = skipped.saturating_add(1),
        }
    }
    Ok((sinks, skipped))
}

/// Parse `pactl -f json list sinks` output. Invalid elements are skipped.
pub fn parse_sinks(input: &str) -> Result<Vec<Sink>, ParseError> {
    parse_sinks_report(input).map(|(sinks, _)| sinks)
}

/// Parse `pactl -f json list sink-inputs` output. Returns the parsed
/// streams and the number of skipped elements.
pub fn parse_sink_inputs_report(input: &str) -> Result<(Vec<Stream>, u32), ParseError> {
    let array = top_level_array(input)?;
    let mut streams = Vec::new();
    let mut skipped = 0u32;
    for item in &array {
        match parse_stream_element(item) {
            Some(s) => streams.push(s),
            None => skipped = skipped.saturating_add(1),
        }
    }
    Ok((streams, skipped))
}

/// Parse `pactl -f json list sink-inputs` output. Invalid elements are
/// skipped.
pub fn parse_sink_inputs(input: &str) -> Result<Vec<Stream>, ParseError> {
    parse_sink_inputs_report(input).map(|(streams, _)| streams)
}

/// Parse `pactl get-default-sink` output: the trimmed first line; empty
/// input (or an empty first line) yields `None`.
pub fn parse_default_sink(input: &str) -> Option<String> {
    let line = input.trim().lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_to_percent_boundaries() {
        assert_eq!(value_to_percent(0), 0);
        assert_eq!(value_to_percent(32768), 50);
        assert_eq!(value_to_percent(65536), 100);
        assert_eq!(value_to_percent(98304), 150);
        // 655 ≈ 1.0 % (0.9994 → 1)
        assert_eq!(value_to_percent(655), 1);
        // 327 ≈ 0.499 % → rounds down to 0; 328 ≈ 0.5005 % → rounds up
        assert_eq!(value_to_percent(327), 0);
        assert_eq!(value_to_percent(328), 1);
        // exact half: 8192 → 12.5 % → round half up → 13
        assert_eq!(value_to_percent(8192), 13);
        // saturation on absurd input
        assert_eq!(value_to_percent(u64::MAX), u32::MAX);
    }

    #[test]
    fn avg_channels_averages() {
        assert_eq!(avg_channels(&[50, 100]), 75);
        assert_eq!(avg_channels(&[50]), 50);
        assert_eq!(avg_channels(&[150, 150, 150]), 150);
    }
}
