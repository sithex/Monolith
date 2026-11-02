//! Parsers for the tile inputs: `pactl -f json list sinks` /
//! `pactl get-default-sink`, `tailscale status --json`, and `hermes-fish`
//! output. Every parser is total: oversized input, malformed lines and
//! unknown values give a typed error or a counted warning, never a panic
//! and never a fabricated value (unknown stays `None`).

use crate::error::ParseError;
use crate::model::{BackendState, Peer, Sink, TailscaleTile, VoiceList};
use serde_json::{Map, Value};

/// Maximum accepted `pactl -f json list sinks` payload size (4 MiB).
pub const MAX_SINKS_BYTES: usize = 4 * 1024 * 1024;

/// Maximum accepted `tailscale status --json` payload size (8 MiB).
pub const MAX_TAILSCALE_BYTES: usize = 8 * 1024 * 1024;

/// Maximum accepted `hermes-fish` list output size (64 KiB).
pub const MAX_VOICE_BYTES: usize = 64 * 1024;

/// tailscale `LastSeen` value meaning "never / unknown".
pub const TAILSCALE_ZERO_TIME: &str = "0001-01-01T00:00:00Z";

/// The `hermes-fish` list header line.
pub const VOICE_HEADER: &str = "Fish Audio voices:";

fn get_str(obj: &Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn get_u32(obj: &Map<String, Value>, key: &str) -> Option<u32> {
    obj.get(key)
        .and_then(|v| v.as_u64())
        .and_then(|n| u32::try_from(n).ok())
}

fn get_bool(obj: &Map<String, Value>, key: &str) -> Option<bool> {
    obj.get(key).and_then(|v| v.as_bool())
}

// ---------------------------------------------------------------------------
// Audio output (SPEC §0.5 item 3)
// ---------------------------------------------------------------------------

fn sink_element(item: &Value) -> Option<Sink> {
    let obj = item.as_object()?;
    let index = get_u32(obj, "index")?;
    let name = get_str(obj, "name")?;
    let description = get_str(obj, "description")?;
    Some(Sink {
        index,
        name,
        description,
        state: get_str(obj, "state").unwrap_or_default(),
        muted: get_bool(obj, "mute").unwrap_or(false),
    })
}

/// Parse `pactl -f json list sinks` output. Returns the parsed sinks and the
/// number of skipped (invalid) elements. The top level must be a JSON array
/// (else a typed error) and input larger than 4 MiB is rejected before
/// parsing. An element is skipped (and counted) when it is not an object or
/// is missing a numeric `index` or a string `name`/`description`.
pub fn parse_sinks_report(input: &str) -> Result<(Vec<Sink>, u32), ParseError> {
    if input.len() > MAX_SINKS_BYTES {
        return Err(ParseError::TooLarge {
            limit: MAX_SINKS_BYTES,
        });
    }
    let value: Value = serde_json::from_str(input).map_err(|_| ParseError::InvalidJson)?;
    let array = value.as_array().ok_or(ParseError::NotAnArray)?;
    let mut sinks = Vec::new();
    let mut skipped = 0u32;
    for item in array {
        match sink_element(item) {
            Some(sink) => sinks.push(sink),
            None => skipped = skipped.saturating_add(1),
        }
    }
    Ok((sinks, skipped))
}

/// Parse `pactl -f json list sinks` output; invalid elements are skipped.
pub fn parse_sinks(input: &str) -> Result<Vec<Sink>, ParseError> {
    parse_sinks_report(input).map(|(sinks, _)| sinks)
}

/// Parse `pactl get-default-sink` output: the trimmed first line; empty input
/// (or an empty first line) yields `None`.
pub fn parse_default_sink(input: &str) -> Option<String> {
    let line = input.trim().lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tailscale (SPEC §0.5 item 4)
// ---------------------------------------------------------------------------

fn peer_element(item: &Value) -> Option<Peer> {
    let obj = item.as_object()?;
    let host_name = get_str(obj, "HostName")?;
    let first_ip = obj
        .get("TailscaleIPs")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let last_seen = obj
        .get("LastSeen")
        .and_then(|v| v.as_str())
        .filter(|s| *s != TAILSCALE_ZERO_TIME)
        .map(str::to_string);
    Some(Peer {
        host_name,
        online: get_bool(obj, "Online").unwrap_or(false),
        os: get_str(obj, "OS").unwrap_or_default(),
        last_seen,
        first_ip,
    })
}

/// Parse `tailscale status --json`. Returns the tile and the number of
/// skipped (malformed) peer entries. `BackendState` is required (a typed
/// error when missing); a missing `Peer` map is an empty list, not an error.
/// The zero-time `LastSeen` (`0001-01-01T00:00:00Z`) becomes `None`.
/// Peers are sorted online-first, then by host name.
pub fn parse_tailscale_status_report(input: &str) -> Result<(TailscaleTile, u32), ParseError> {
    if input.len() > MAX_TAILSCALE_BYTES {
        return Err(ParseError::TooLarge {
            limit: MAX_TAILSCALE_BYTES,
        });
    }
    let value: Value = serde_json::from_str(input).map_err(|_| ParseError::InvalidJson)?;
    let obj = value.as_object().ok_or(ParseError::NotAnObject)?;

    let backend_raw = obj.get("BackendState").and_then(|v| v.as_str());
    let backend_raw = backend_raw.ok_or(ParseError::MissingBackendState)?;
    let backend = match backend_raw {
        "Running" => BackendState::Running,
        "Stopped" => BackendState::Stopped,
        "NeedsLogin" => BackendState::NeedsLogin,
        "Starting" => BackendState::Starting,
        "NoState" => BackendState::NoState,
        other => BackendState::Unknown(other.to_string()),
    };

    let self_name = obj
        .get("Self")
        .and_then(|v| v.as_object())
        .and_then(|s| get_str(s, "HostName"));

    let mut peers = Vec::new();
    let mut skipped = 0u32;
    if let Some(peer_map) = obj.get("Peer").and_then(|v| v.as_object()) {
        for peer_value in peer_map.values() {
            match peer_element(peer_value) {
                Some(peer) => peers.push(peer),
                None => skipped = skipped.saturating_add(1),
            }
        }
    }
    peers.sort_by(|a, b| {
        b.online
            .cmp(&a.online)
            .then_with(|| a.host_name.cmp(&b.host_name))
    });
    let online_count = peers.iter().filter(|p| p.online).count() as u32;

    Ok((
        TailscaleTile {
            backend,
            self_name,
            peers,
            online_count,
        },
        skipped,
    ))
}

/// Parse `tailscale status --json`; malformed peer entries are skipped.
pub fn parse_tailscale_status(input: &str) -> Result<TailscaleTile, ParseError> {
    parse_tailscale_status_report(input).map(|(tile, _)| tile)
}

// ---------------------------------------------------------------------------
// Hermes voice switcher (SPEC §0.5 item 7)
// ---------------------------------------------------------------------------

fn is_voice_name_char(c: char) -> bool {
    matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '.' | '-')
}

/// Voice names are 1..=64 characters of `[A-Za-z0-9_.-]`.
pub fn is_valid_voice_name(name: &str) -> bool {
    let n = name.chars().count();
    (1..=64).contains(&n) && name.chars().all(is_voice_name_char)
}

/// One voice line: exactly two leading spaces, a valid voice name, then
/// either nothing (or trailing whitespace), or exactly `  [active]` (then
/// optional trailing whitespace). Returns `(name, is_active)`.
fn voice_line(line: &str) -> Option<(String, bool)> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let rest = line.strip_prefix("  ")?;
    let name_len = rest.chars().take_while(|c| is_voice_name_char(*c)).count();
    if !(1..=64).contains(&name_len) {
        return None;
    }
    let mut chars = rest.chars();
    let name: String = chars.by_ref().take(name_len).collect();
    let tail = chars.as_str();
    if let Some(after) = tail.strip_prefix("  [active]") {
        if after.trim().is_empty() {
            Some((name, true))
        } else {
            None
        }
    } else if tail.trim().is_empty() {
        Some((name, false))
    } else {
        None
    }
}

/// Parse `hermes-fish` list output into a [`VoiceList`]. The header line
/// (`Fish Audio voices:`) and blank lines are skipped silently; any other
/// line that is not a valid voice line is counted as a warning and skipped.
/// More than one `[active]` line is a typed error. Input larger than 64 KiB
/// is rejected. Voice order is preserved.
pub fn parse_voice_list_report(input: &str) -> Result<(VoiceList, u32), ParseError> {
    if input.len() > MAX_VOICE_BYTES {
        return Err(ParseError::TooLarge {
            limit: MAX_VOICE_BYTES,
        });
    }
    let mut voices = Vec::new();
    let mut active: Option<String> = None;
    let mut warnings = 0u32;
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == VOICE_HEADER {
            continue;
        }
        match voice_line(line) {
            Some((name, is_active)) => {
                if is_active {
                    if active.is_some() {
                        return Err(ParseError::MultipleActiveVoices);
                    }
                    active = Some(name.clone());
                }
                voices.push(name);
            }
            None => warnings = warnings.saturating_add(1),
        }
    }
    Ok((VoiceList { voices, active }, warnings))
}

/// Parse `hermes-fish` list output; invalid lines are skipped.
pub fn parse_voice_list(input: &str) -> Result<VoiceList, ParseError> {
    parse_voice_list_report(input).map(|(list, _)| list)
}

/// Parse `hermes-fish current` output: the trimmed single line, validated
/// with the same voice-name rule.
pub fn parse_current(input: &str) -> Result<String, ParseError> {
    let line = input.trim().lines().next().unwrap_or("").trim();
    if is_valid_voice_name(line) {
        Ok(line.to_string())
    } else {
        Err(ParseError::InvalidVoiceName)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_line_accepts_active_and_plain() {
        assert_eq!(
            voice_line("  necron4  [active]"),
            Some(("necron4".to_string(), true))
        );
        assert_eq!(voice_line("  alex"), Some(("alex".to_string(), false)));
        assert_eq!(voice_line("  a  [active]  "), Some(("a".to_string(), true)));
        assert_eq!(voice_line("  a\r"), Some(("a".to_string(), false)));
    }

    #[test]
    fn voice_line_rejects_malformed() {
        assert_eq!(voice_line("  a[active]"), None); // missing two-space separator
        assert_eq!(voice_line("  a  [ACTIVE]"), None); // case-sensitive marker
        assert_eq!(voice_line("  a  [active]x"), None); // trailing junk
        assert_eq!(voice_line(" a"), None); // one leading space
        assert_eq!(voice_line("   a"), None); // three leading spaces
        assert_eq!(voice_line(""), None);
        assert_eq!(voice_line("  !bad"), None);
        assert_eq!(voice_line("  "), None);
        assert_eq!(voice_line(&format!("  {}", "a".repeat(65))), None);
        assert_eq!(
            voice_line(&format!("  {}", "a".repeat(64))),
            Some(("a".repeat(64), false))
        );
    }

    #[test]
    fn voice_name_rule_in_module() {
        assert!(is_valid_voice_name("a"));
        assert!(is_valid_voice_name("a-b_c.0"));
        assert!(is_valid_voice_name(&"a".repeat(64)));
        assert!(!is_valid_voice_name(""));
        assert!(!is_valid_voice_name(&"a".repeat(65)));
        assert!(!is_valid_voice_name("a b"));
        assert!(!is_valid_voice_name("a\nb"));
        assert!(!is_valid_voice_name("声音"));
    }
}
