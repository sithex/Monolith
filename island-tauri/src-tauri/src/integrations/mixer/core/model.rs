//! The mixer model: parsed [`Sink`] and [`Stream`] records plus the
//! derived [`AppGroup`] view produced by [`MixerModel::apps`].

use serde::{Deserialize, Serialize};

/// A sink as reported by `pactl -f json list sinks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sink {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub muted: bool,
    /// Average volume of the sink's channels, as a percent; `None` when
    /// no channel had a numeric volume.
    pub volume_percent: Option<u32>,
    /// Number of channels with a numeric volume.
    pub channels: u8,
    pub state: String,
}

/// A sink-input (playback stream) as reported by
/// `pactl -f json list sink-inputs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub index: u32,
    pub sink_index: u32,
    /// `application.name`, else `application.process.binary`, else
    /// `media.name`, else `"Unknown"`.
    pub app_name: String,
    pub binary: Option<String>,
    pub pid: Option<u32>,
    pub media_name: Option<String>,
    pub icon_name: Option<String>,
    pub muted: bool,
    pub corked: bool,
    pub volume_percent: Option<u32>,
}

/// Streams grouped by (app name, pid). Two windows of the same app with
/// the same pid form one group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppGroup {
    pub app_name: String,
    /// Stream (`sink-input`) indexes in this group.
    pub streams: Vec<u32>,
    /// Maximum member volume percent; `None` when no member had a known
    /// volume (unknown is shown unavailable, never fabricated as 0).
    pub volume_percent: Option<u32>,
    /// True when every member is muted.
    pub muted: bool,
    /// The shared sink index when all members are on one sink.
    pub sink_index: Option<u32>,
    /// True when every member is corked (a paused/idle group).
    pub idle: bool,
}

/// The full mixer model the Tauri actor works from.
#[derive(Debug, Clone)]
pub struct MixerModel {
    sinks: Vec<Sink>,
    streams: Vec<Stream>,
    default_sink: Option<String>,
}

impl MixerModel {
    /// Build a model from parsed sink/stream lists and the default sink
    /// name (from `pactl get-default-sink`).
    pub fn from(sinks: Vec<Sink>, streams: Vec<Stream>, default_sink: Option<String>) -> Self {
        Self {
            sinks,
            streams,
            default_sink,
        }
    }

    /// Streams grouped by (app name, pid), sorted by app name
    /// case-insensitively then pid. Corked-only groups are kept and
    /// flagged `idle`.
    pub fn apps(&self) -> Vec<AppGroup> {
        let mut ordered: Vec<&Stream> = self.streams.iter().collect();
        ordered.sort_by(|a, b| {
            a.app_name
                .to_lowercase()
                .cmp(&b.app_name.to_lowercase())
                .then_with(|| a.pid.cmp(&b.pid))
        });

        let mut keys: Vec<(String, Option<u32>)> = Vec::new();
        let mut groups: Vec<AppGroup> = Vec::new();
        // Tracks whether a group's streams have disagreed on a sink index.
        // Once mixed, `sink_index` stays `None` even if later streams agree
        // on a sink.
        let mut mixed: Vec<bool> = Vec::new();
        for s in ordered {
            let key = (s.app_name.clone(), s.pid);
            let idx = match keys.iter().position(|k| *k == key) {
                Some(i) => i,
                None => {
                    keys.push(key);
                    groups.push(AppGroup {
                        app_name: s.app_name.clone(),
                        streams: Vec::new(),
                        volume_percent: None,
                        muted: true,
                        sink_index: None,
                        idle: true,
                    });
                    mixed.push(false);
                    groups.len() - 1
                }
            };
            let g = &mut groups[idx];
            g.streams.push(s.index);
            if let Some(v) = s.volume_percent {
                g.volume_percent = Some(match g.volume_percent {
                    Some(cur) => cur.max(v),
                    None => v,
                });
            }
            g.muted = g.muted && s.muted;
            g.idle = g.idle && s.corked;
            if !mixed[idx] {
                match g.sink_index {
                    None => g.sink_index = Some(s.sink_index),
                    Some(prev) if prev == s.sink_index => {}
                    Some(_) => {
                        g.sink_index = None;
                        mixed[idx] = true;
                    }
                }
            }
        }
        groups
    }

    /// The default sink by name, if it is present in the sink list.
    pub fn default_sink(&self) -> Option<&Sink> {
        self.sinks
            .iter()
            .find(|s| Some(s.name.as_str()) == self.default_sink.as_deref())
    }
}
