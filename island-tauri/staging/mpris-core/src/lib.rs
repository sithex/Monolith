//! Pure core behind the "Now playing" row (SPEC §3.5) and the Phase 6 lyrics
//! timing: given MPRIS D-Bus values (already converted to
//! `serde_json::Value`) and an injected monotonic clock, answer "what is
//! playing, and at what playback position right now". No IO, no network, no
//! threads, no clock reading, and never panics on any input.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Upper bound on the number of simultaneously tracked players. Beyond this
/// the oldest inactive player is evicted.
pub const MAX_PLAYERS: usize = 32;

/// Every accepted MPRIS bus name starts with this prefix.
const BUS_PREFIX: &str = "org.mpris.MediaPlayer2.";

/// Strings taken from metadata are capped at this many characters.
const MAX_STR_CHARS: usize = 512;

/// Maximum number of artists kept from `xesam:artist`.
const MAX_ARTISTS: usize = 16;

/// MPRIS `PlaybackStatus` values, exactly as sent on D-Bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

/// Parse an exact MPRIS `PlaybackStatus` string. Any other string (including
/// differently-cased spellings) yields `None`.
pub fn parse_status(s: &str) -> Option<PlaybackStatus> {
    match s {
        "Playing" => Some(PlaybackStatus::Playing),
        "Paused" => Some(PlaybackStatus::Paused),
        "Stopped" => Some(PlaybackStatus::Stopped),
        _ => None,
    }
}

/// Metadata for one track, parsed from an MPRIS metadata dictionary.
///
/// Serde wires these fields as camelCase (e.g. `trackId`, `artUrl`,
/// `lengthMs`); no aliases are accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// `mpris:trackid`.
    pub track_id: Option<String>,
    /// `xesam:title` (always present; the parser never fabricates one).
    pub title: String,
    /// `xesam:artist` (array of strings, or a single string).
    pub artists: Vec<String>,
    /// `xesam:album`.
    pub album: Option<String>,
    /// `mpris:artUrl`.
    pub art_url: Option<String>,
    /// `mpris:length` in microseconds, converted to milliseconds.
    pub length_ms: Option<u64>,
}

/// A snapshot of one MPRIS player's state at a point on the monotonic clock.
///
/// Serde wires these fields as camelCase (`busName`, `positionMs`,
/// `positionAtMs`, `lastPlayingAtMs`); no aliases are accepted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    /// Full D-Bus name, e.g. `org.mpris.MediaPlayer2.spotify`.
    pub bus_name: String,
    pub status: PlaybackStatus,
    pub track: Option<Track>,
    /// Playback rate (1.0 when normal).
    pub rate: f64,
    /// Last known position in milliseconds.
    pub position_ms: u64,
    /// Monotonic time at which `position_ms` was sampled.
    pub position_at_ms: u64,
    /// Monotonic time it was last `Playing` (None if never played).
    pub last_playing_at_ms: Option<u64>,
}

impl PlayerState {
    fn new(bus_name: &str, now: u64) -> Self {
        PlayerState {
            bus_name: bus_name.to_string(),
            status: PlaybackStatus::Stopped,
            track: None,
            rate: 1.0,
            position_ms: 0,
            position_at_ms: now,
            last_playing_at_ms: None,
        }
    }
}

/// Interpolate a player's playback position at monotonic time `now_ms`.
///
/// `Playing`: `positionMs + (now - positionAtMs) * rate`, clamped to
/// `[0, lengthMs]` when the track length is known. A non-finite or non-positive
/// rate is treated as 1.0. Clocks never run backwards: when `now_ms <
/// positionAtMs` the frozen `positionMs` is returned. `Paused` returns the
/// frozen `positionMs`; `Stopped` returns 0.
pub fn position_at(p: &PlayerState, now_ms: u64) -> u64 {
    match p.status {
        PlaybackStatus::Stopped => 0,
        PlaybackStatus::Paused => p.position_ms,
        PlaybackStatus::Playing => {
            if now_ms < p.position_at_ms {
                return p.position_ms;
            }
            let rate = if p.rate.is_finite() && p.rate > 0.0 {
                p.rate
            } else {
                1.0
            };
            let elapsed = (now_ms - p.position_at_ms) as f64;
            let pos = p.position_ms as f64 + elapsed * rate;
            if let Some(len) = p.track.as_ref().and_then(|t| t.length_ms) {
                if pos < 0.0 {
                    0
                } else if pos > len as f64 {
                    len
                } else {
                    pos as u64
                }
            } else if pos < 0.0 {
                0
            } else {
                pos as u64
            }
        }
    }
}

fn valid_bus(bus: &str) -> bool {
    bus.starts_with(BUS_PREFIX)
}

fn trim_cap(s: &str) -> String {
    s.trim().chars().take(MAX_STR_CHARS).collect()
}

fn parse_artists(value: Option<&Value>) -> Vec<String> {
    let mut out = Vec::new();
    match value {
        None => {}
        Some(Value::String(s)) => {
            let artist = trim_cap(s);
            if !artist.is_empty() {
                out.push(artist);
            }
        }
        Some(Value::Array(items)) => {
            for item in items {
                if out.len() >= MAX_ARTISTS {
                    break;
                }
                if let Value::String(s) = item {
                    let artist = trim_cap(s);
                    if !artist.is_empty() {
                        out.push(artist);
                    }
                }
            }
        }
        Some(_) => {}
    }
    out
}

/// Read a JSON number as a non-negative integer, accepting integral floats.
fn value_as_nonneg_integer(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                return Some(u);
            }
            if let Some(i) = n.as_i64() {
                return (i >= 0).then_some(i as u64);
            }
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f >= 0.0 && (f - f.trunc()).abs() < 1e-9 && f <= u64::MAX as f64
                {
                    return Some(f as u64);
                }
            }
            None
        }
        _ => None,
    }
}

/// Parse `mpris:length` (microseconds, integer) into milliseconds. Zero,
/// negative and non-integer values yield `None`.
fn parse_length_ms(value: Option<&Value>) -> Option<u64> {
    let us = value_as_nonneg_integer(value?)?;
    if us == 0 {
        return None;
    }
    Some(us / 1000)
}

/// Parse an MPRIS metadata dictionary into a [`Track`].
///
/// Returns `None` when the title is missing or blank (never fabricated). All
/// strings are trimmed and capped at 512 chars; blank artists are dropped and
/// at most 16 artists are kept. `mpris:length` must be a positive integer
/// number of microseconds (else `length_ms` is `None`).
pub fn parse_metadata(meta: &Map<String, Value>) -> Option<Track> {
    let title = match meta.get("xesam:title") {
        Some(Value::String(s)) => trim_cap(s),
        _ => return None,
    };
    if title.is_empty() {
        return None;
    }

    let track_id = match meta.get("mpris:trackid") {
        Some(Value::String(s)) => {
            let t = trim_cap(s);
            (!t.is_empty()).then_some(t)
        }
        _ => None,
    };
    let album = match meta.get("xesam:album") {
        Some(Value::String(s)) => {
            let a = trim_cap(s);
            (!a.is_empty()).then_some(a)
        }
        _ => None,
    };
    let art_url = match meta.get("mpris:artUrl") {
        Some(Value::String(s)) => {
            let a = trim_cap(s);
            (!a.is_empty()).then_some(a)
        }
        _ => None,
    };
    let artists = parse_artists(meta.get("xesam:artist"));
    let length_ms = parse_length_ms(meta.get("mpris:length"));

    Some(Track {
        track_id,
        title,
        artists,
        album,
        art_url,
        length_ms,
    })
}

fn us_to_ms(position_us: i64) -> u64 {
    if position_us < 0 {
        0
    } else {
        position_us as u64 / 1000
    }
}

/// Tracks the set of MPRIS players and picks the preferred one.
#[derive(Debug, Clone, Default)]
pub struct Players {
    players: Vec<PlayerState>,
}

/// Index of the player to evict: non-Playing players first, then oldest
/// `last_playing_at_ms` (never-played counts as oldest), then lexical bus name.
fn eviction_index(players: &[PlayerState]) -> usize {
    players
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let ka = (
                a.status == PlaybackStatus::Playing,
                a.last_playing_at_ms,
                &a.bus_name,
            );
            let kb = (
                b.status == PlaybackStatus::Playing,
                b.last_playing_at_ms,
                &b.bus_name,
            );
            ka.cmp(&kb)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Return the index of the player with `bus`, inserting a new one (evicting
/// the oldest inactive player if at capacity) when absent.
fn ensure_player(players: &mut Vec<PlayerState>, bus: &str, now: u64) -> usize {
    if let Some(i) = players.iter().position(|p| p.bus_name == bus) {
        return i;
    }
    if players.len() >= MAX_PLAYERS {
        let evict = eviction_index(players);
        players.swap_remove(evict);
    }
    players.push(PlayerState::new(bus, now));
    players.len() - 1
}

/// `Ordering` for preferred-player selection: latest `last_playing_at_ms`
/// wins; ties resolve to the lexically smallest bus name.
fn compare_preferred(a: &PlayerState, b: &PlayerState) -> std::cmp::Ordering {
    let ka = (a.last_playing_at_ms, std::cmp::Reverse(&a.bus_name));
    let kb = (b.last_playing_at_ms, std::cmp::Reverse(&b.bus_name));
    ka.cmp(&kb)
}

impl Players {
    /// Create or update a player's playback status at time `now`.
    ///
    /// Entering `Playing` re-anchors the position (keeping continuity) and
    /// records `last_playing_at_ms = Some(now)`. Leaving `Playing` freezes the
    /// interpolated position; entering `Stopped` zeroes it. Bus names that do
    /// not start with `org.mpris.MediaPlayer2.` are ignored.
    pub fn upsert_status(&mut self, bus: &str, status: PlaybackStatus, now: u64) {
        if !valid_bus(bus) {
            return;
        }
        let idx = ensure_player(&mut self.players, bus, now);
        let p = &mut self.players[idx];
        match status {
            PlaybackStatus::Playing => {
                p.position_ms = position_at(p, now);
                p.position_at_ms = now;
                p.last_playing_at_ms = Some(now);
            }
            PlaybackStatus::Paused => {
                if p.status == PlaybackStatus::Playing {
                    p.position_ms = position_at(p, now);
                }
                p.position_at_ms = now;
            }
            PlaybackStatus::Stopped => {
                p.position_ms = 0;
                p.position_at_ms = now;
            }
        }
        p.status = status;
    }

    /// Update a player's track metadata at time `now`.
    ///
    /// A changed track id or title resets the position to 0 at `now`.
    /// Unparseable metadata (missing/blank title) is ignored.
    pub fn set_metadata(&mut self, bus: &str, meta: &Map<String, Value>, now: u64) {
        if !valid_bus(bus) {
            return;
        }
        let Some(track) = parse_metadata(meta) else {
            return;
        };
        let idx = ensure_player(&mut self.players, bus, now);
        let p = &mut self.players[idx];
        let changed = match &p.track {
            Some(old) => old.track_id != track.track_id || old.title != track.title,
            None => true,
        };
        if changed {
            p.position_ms = 0;
            p.position_at_ms = now;
        }
        p.track = Some(track);
    }

    /// Apply an MPRIS `Seeked` signal: `position_us` microseconds (negative →
    /// 0). Only affects a known player.
    pub fn seeked(&mut self, bus: &str, position_us: i64, now: u64) {
        if !valid_bus(bus) {
            return;
        }
        let Some(idx) = self.players.iter().position(|p| p.bus_name == bus) else {
            return;
        };
        let p = &mut self.players[idx];
        p.position_ms = us_to_ms(position_us);
        p.position_at_ms = now;
    }

    /// Set an absolute playback position, `position_us` microseconds (negative
    /// → 0). Only affects a known player.
    pub fn set_position(&mut self, bus: &str, position_us: i64, now: u64) {
        if !valid_bus(bus) {
            return;
        }
        let Some(idx) = self.players.iter().position(|p| p.bus_name == bus) else {
            return;
        };
        let p = &mut self.players[idx];
        p.position_ms = us_to_ms(position_us);
        p.position_at_ms = now;
    }

    /// Change playback rate, re-anchoring the position at `now` first.
    pub fn set_rate(&mut self, bus: &str, rate: f64, now: u64) {
        if !valid_bus(bus) {
            return;
        }
        let Some(idx) = self.players.iter().position(|p| p.bus_name == bus) else {
            return;
        };
        let p = &mut self.players[idx];
        p.position_ms = position_at(p, now);
        p.position_at_ms = now;
        p.rate = rate;
    }

    /// Forget a player (e.g. D-Bus `NameOwnerChanged` to an empty owner).
    pub fn remove(&mut self, bus: &str) {
        if !valid_bus(bus) {
            return;
        }
        if let Some(idx) = self.players.iter().position(|p| p.bus_name == bus) {
            self.players.swap_remove(idx);
        }
    }

    /// The preferred player: the one currently `Playing` with the latest
    /// `last_playing_at_ms` (ties broken by lexical bus name); if none is
    /// playing, the player with the latest `last_playing_at_ms`; if no player
    /// has ever played, the lexically first player that has a track.
    pub fn preferred(&self) -> Option<&PlayerState> {
        let playing: Vec<&PlayerState> = self
            .players
            .iter()
            .filter(|p| p.status == PlaybackStatus::Playing)
            .collect();
        if !playing.is_empty() {
            return playing.into_iter().max_by(|a, b| compare_preferred(a, b));
        }
        let ever_played: Vec<&PlayerState> = self
            .players
            .iter()
            .filter(|p| p.last_playing_at_ms.is_some())
            .collect();
        if !ever_played.is_empty() {
            return ever_played
                .into_iter()
                .max_by(|a, b| compare_preferred(a, b));
        }
        self.players
            .iter()
            .filter(|p| p.track.is_some())
            .min_by(|a, b| a.bus_name.cmp(&b.bus_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus(i: usize) -> String {
        format!("org.mpris.MediaPlayer2.p{i:02}")
    }

    #[test]
    fn eviction_caps_at_32() {
        let mut ps = Players::default();
        for i in 0..40 {
            ps.upsert_status(&bus(i), PlaybackStatus::Stopped, i as u64);
        }
        assert_eq!(ps.players.len(), MAX_PLAYERS);
    }

    #[test]
    fn eviction_prefers_nonplaying_then_oldest_then_lexical() {
        let mut ps = Players::default();
        // p99 is Playing and must survive; p00 is the oldest non-playing.
        ps.upsert_status(&bus(99), PlaybackStatus::Playing, 0);
        for i in 0..31 {
            ps.upsert_status(&bus(i), PlaybackStatus::Stopped, i as u64);
        }
        assert_eq!(ps.players.len(), 32);
        ps.upsert_status(&bus(31), PlaybackStatus::Stopped, 31);
        assert_eq!(ps.players.len(), 32);
        assert!(ps.players.iter().any(|p| p.bus_name == bus(99)));
        assert!(!ps.players.iter().any(|p| p.bus_name == bus(0)));
    }

    #[test]
    fn eviction_oldest_last_playing_first() {
        let mut ps = Players::default();
        // p00 played at t=1000 (old); p01..p31 never played.
        ps.upsert_status(&bus(0), PlaybackStatus::Playing, 1000);
        ps.upsert_status(&bus(0), PlaybackStatus::Stopped, 2000);
        ps.upsert_status(&bus(1), PlaybackStatus::Stopped, 0);
        for i in 2..32 {
            ps.upsert_status(&bus(i), PlaybackStatus::Stopped, i as u64);
        }
        assert_eq!(ps.players.len(), 32);
        ps.upsert_status(&bus(40), PlaybackStatus::Stopped, 40);
        // Never-played players (None) are older-inactive than p00 (Some(1000));
        // the lexically first never-played one (p01) is evicted.
        assert!(!ps.players.iter().any(|p| p.bus_name == bus(1)));
        assert!(ps.players.iter().any(|p| p.bus_name == bus(0)));
    }
}
