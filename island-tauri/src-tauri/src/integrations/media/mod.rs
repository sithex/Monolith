//! Now playing (SPEC §3.5 "Now playing", §8 Phase 6 "Music extras").
//!
//! * `model.rs` is the audited `staging/mpris-core` (player set, preferred-player choice,
//!   position extrapolation), `lrclib.rs` = `staging/lrclib-core`, `lrc.rs` =
//!   `staging/lrc-lyrics`, `spectrum.rs` = `staging/spectrum-bands`, all ported unchanged
//!   (one documented amendment: `lrclib::parse_response` requires a JSON object).
//! * `MediaService` (this file) owns the mutable player set, the revisioned `MediaChanged`
//!   topic and the availability report; `mpris.rs` feeds it from the session bus.
//! * `lyrics.rs` fetches/caches/times lyrics, `equalizer.rs` captures + analyses the playing
//!   stream's monitor.
//!
//! Nothing here ever writes to a player except `media_control`, and that reaches only the
//! preferred player. Missing values are `null`, never 0; a vanished player clears the state.

#[cfg(target_os = "linux")]
mod artwork;
pub mod equalizer;
pub mod lrc;
pub mod lrclib;
pub mod lyrics;
pub mod model;
#[cfg(target_os = "linux")]
mod mpris;
pub mod spectrum;

use crate::{
    bus::Latest,
    contract::system::{MediaChanged, MediaCommand, NowPlaying, ServiceStatus},
    error::IslandError,
    integrations::service_status::Reporter,
};
use model::{PlaybackStatus, PlayerState, Players};
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

pub const BUS_PREFIX: &str = "org.mpris.MediaPlayer2.";

/// What the equalizer needs to find the playing stream (never a microphone).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PlayerHint {
    pub pid: Option<u32>,
    /// Lower-case candidate application names (bus-name suffix, first dotted segment).
    pub names: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct Caps {
    control: bool,
    play: bool,
    pause: bool,
    seek: bool,
    next: bool,
    previous: bool,
}

/// Per-player bookkeeping the audited core does not keep.
#[derive(Clone, Debug, Default)]
struct Shadow {
    owner: String,
    pid: Option<u32>,
    track_key: Option<(Option<String>, String)>,
    position_known: bool,
    caps: Caps,
    volume_percent: Option<f64>,
    volume_writable: bool,
    artwork: Option<String>,
    artwork_key: Option<String>,
}

struct Shared {
    players: Players,
    shadow: HashMap<String, Shadow>,
    current: MediaChanged,
}

pub struct MediaService {
    pub status: Arc<Reporter>,
    pub topic: Latest<MediaChanged>,
    shared: Mutex<Shared>,
    origin: Instant,
    epoch_origin_ms: f64,
    #[cfg(target_os = "linux")]
    link: Mutex<Option<zbus::Connection>>,
    stop: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    art_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    art_changes: watch::Sender<()>,
}

fn epoch_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

/// MPRIS names are `org.mpris.MediaPlayer2.<suffix>`; the suffix's first dotted segment is a
/// good application-name candidate (`firefox.instance_1_5` → `firefox`).
fn name_candidates(bus: &str) -> Vec<String> {
    let suffix = bus.strip_prefix(BUS_PREFIX).unwrap_or(bus);
    let mut names = vec![suffix.to_lowercase()];
    let first = suffix.split('.').next().unwrap_or("").to_lowercase();
    if !first.is_empty() && !names.contains(&first) {
        names.push(first);
    }
    names
}

fn json_f64(value: &Value) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite())
}

fn json_bool(value: Option<&Value>) -> bool {
    value.and_then(Value::as_bool).unwrap_or(false)
}

impl MediaService {
    pub fn new() -> Arc<Self> {
        let current = MediaChanged {
            rev: 0,
            now_playing: None,
        };
        let topic = Latest::new();
        topic.publish(current.clone());
        Arc::new(Self {
            status: Reporter::new("media"),
            topic,
            shared: Mutex::new(Shared {
                players: Players::default(),
                shadow: HashMap::new(),
                current,
            }),
            origin: Instant::now(),
            epoch_origin_ms: epoch_ms(),
            #[cfg(target_os = "linux")]
            link: Mutex::new(None),
            stop: watch::channel(false).0,
            task: Mutex::new(None),
            art_task: Mutex::new(None),
            art_changes: watch::channel(()).0,
        })
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        match self.shared.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Monotonic milliseconds since the service was created (the core's injected clock).
    pub fn now_ms(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }

    pub fn current(&self) -> MediaChanged {
        self.lock().current.clone()
    }

    pub fn status(&self) -> ServiceStatus {
        self.status.get()
    }

    /// Position of the preferred player right now (extrapolated by the audited core), its
    /// rate and whether it is playing. `None` when there is no displayable player or the
    /// position was never learned.
    pub fn position_now(&self) -> Option<(u64, f64, bool)> {
        let shared = self.lock();
        let player = shared.players.preferred()?;
        player.track.as_ref()?;
        if !shared.shadow.get(&player.bus_name)?.position_known {
            return None;
        }
        let playing = player.status == PlaybackStatus::Playing;
        let rate = if player.rate.is_finite() && player.rate > 0.0 {
            player.rate
        } else {
            1.0
        };
        Some((model::position_at(player, self.now_ms()), rate, playing))
    }

    /// The preferred player as a stream hint for the equalizer, only while it is playing.
    pub fn playing_hint(&self) -> Option<PlayerHint> {
        let shared = self.lock();
        let player = shared.players.preferred()?;
        if player.status != PlaybackStatus::Playing || player.track.is_none() {
            return None;
        }
        Some(PlayerHint {
            pid: shared.shadow.get(&player.bus_name).and_then(|s| s.pid),
            names: name_candidates(&player.bus_name),
        })
    }

    // ---- feeding (mpris.rs and tests) -------------------------------------------------

    pub(crate) fn owner_of(&self, bus: &str) -> Option<String> {
        self.lock().shadow.get(bus).map(|s| s.owner.clone())
    }

    pub(crate) fn bus_for_owner(&self, unique: &str) -> Option<String> {
        self.lock()
            .shadow
            .iter()
            .find(|(_, s)| s.owner == unique)
            .map(|(bus, _)| bus.clone())
    }

    /// A player appeared (or was re-read): replace its state from `GetAll`.
    pub(crate) fn player_loaded(
        &self,
        bus: &str,
        owner: &str,
        pid: Option<u32>,
        props: &Map<String, Value>,
    ) {
        let mut shared = self.lock();
        shared.players.remove(bus);
        shared.shadow.insert(
            bus.to_owned(),
            Shadow {
                owner: owner.to_owned(),
                pid,
                ..Shadow::default()
            },
        );
        self.apply(&mut shared, bus, props, true);
        self.publish(&mut shared);
        self.art_changes.send_replace(());
    }

    pub(crate) fn player_props(&self, bus: &str, props: &Map<String, Value>) {
        let mut shared = self.lock();
        if !shared.shadow.contains_key(bus) {
            return;
        }
        self.apply(&mut shared, bus, props, false);
        self.publish(&mut shared);
        self.art_changes.send_replace(());
    }

    pub(crate) fn player_position(&self, bus: &str, position_us: i64) {
        let mut shared = self.lock();
        if !shared.shadow.contains_key(bus) {
            return;
        }
        let now = self.now_ms();
        shared.players.set_position(bus, position_us, now);
        if let Some(shadow) = shared.shadow.get_mut(bus) {
            shadow.position_known = true;
        }
        self.publish(&mut shared);
    }

    pub(crate) fn player_seeked(&self, bus: &str, position_us: i64) {
        let mut shared = self.lock();
        if !shared.shadow.contains_key(bus) {
            return;
        }
        let now = self.now_ms();
        shared.players.seeked(bus, position_us, now);
        if let Some(shadow) = shared.shadow.get_mut(bus) {
            shadow.position_known = true;
        }
        self.publish(&mut shared);
    }

    pub(crate) fn player_removed(&self, bus: &str) {
        let mut shared = self.lock();
        shared.players.remove(bus);
        shared.shadow.remove(bus);
        self.publish(&mut shared);
        self.art_changes.send_replace(());
    }

    /// The bus session ended: nothing is playing as far as we can know.
    pub(crate) fn lost(&self) {
        let mut shared = self.lock();
        shared.players = Players::default();
        shared.shadow.clear();
        self.publish(&mut shared);
        self.art_changes.send_replace(());
    }

    fn apply(&self, shared: &mut Shared, bus: &str, props: &Map<String, Value>, initial: bool) {
        let now = self.now_ms();
        if let Some(Value::Object(meta)) = props.get("Metadata") {
            shared.players.set_metadata(bus, meta, now);
            let key = model::parse_metadata(meta).map(|t| (t.track_id, t.title));
            if let Some(shadow) = shared.shadow.get_mut(bus) {
                let art = meta
                    .get("mpris:artUrl")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if shadow.track_key != key || shadow.artwork_key != art {
                    shadow.artwork = None;
                    shadow.artwork_key = art;
                }
                if let Some(pid) = meta
                    .get("kde:pid")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                {
                    shadow.pid = Some(pid);
                }
                // A track change after the first sighting starts at 0: a known position.
                if !initial
                    && key.is_some()
                    && shadow.track_key.is_some()
                    && shadow.track_key != key
                {
                    shadow.position_known = true;
                }
                if key.is_some() {
                    shadow.track_key = key;
                }
            }
        }
        if let Some(status) = props
            .get("PlaybackStatus")
            .and_then(Value::as_str)
            .and_then(model::parse_status)
        {
            shared.players.upsert_status(bus, status, now);
        }
        if let Some(rate) = props.get("Rate").and_then(json_f64) {
            shared.players.set_rate(bus, rate, now);
        }
        if let Some(position) = props.get("Position").and_then(Value::as_i64) {
            shared.players.set_position(bus, position, now);
            if let Some(shadow) = shared.shadow.get_mut(bus) {
                shadow.position_known = true;
            }
        }
        if let Some(shadow) = shared.shadow.get_mut(bus) {
            let caps = &mut shadow.caps;
            if let Some(v) = props.get("Volume") {
                shadow.volume_percent = json_f64(v)
                    .filter(|n| *n >= 0.0)
                    .map(|n| n * 100.0)
                    .filter(|n| n.is_finite());
            }
            if let Some(v) = props.get("VolumeWritable").and_then(Value::as_bool) {
                shadow.volume_writable = v;
            }
            for (key, slot) in [
                ("CanControl", &mut caps.control),
                ("CanPlay", &mut caps.play),
                ("CanPause", &mut caps.pause),
                ("CanSeek", &mut caps.seek),
                ("CanGoNext", &mut caps.next),
                ("CanGoPrevious", &mut caps.previous),
            ] {
                if let Some(value) = props.get(key) {
                    *slot = json_bool(Some(value));
                }
            }
        }
    }

    fn now_playing(
        &self,
        player: &PlayerState,
        shadow: &HashMap<String, Shadow>,
    ) -> Option<NowPlaying> {
        let track = player.track.as_ref()?;
        let shadow = shadow.get(&player.bus_name)?;
        let status = match player.status {
            PlaybackStatus::Playing => "playing",
            PlaybackStatus::Paused => "paused",
            PlaybackStatus::Stopped => "stopped",
        };
        let rate = if player.rate.is_finite() && player.rate > 0.0 {
            player.rate
        } else {
            1.0
        };
        Some(NowPlaying {
            player: player
                .bus_name
                .strip_prefix(BUS_PREFIX)
                .unwrap_or(&player.bus_name)
                .to_owned(),
            title: track.title.clone(),
            artist: track.artists.join(", "),
            album: track.album.clone().unwrap_or_default(),
            playing: player.status == PlaybackStatus::Playing,
            position_ms: shadow.position_known.then_some(player.position_ms as f64),
            duration_ms: track.length_ms.map(|ms| ms as f64),
            art_url: shadow.artwork.clone(),
            can_control: shadow.caps.control,
            bus_name: player.bus_name.clone(),
            status: status.to_owned(),
            rate,
            sampled_at_ms: self.epoch_origin_ms + player.position_at_ms as f64,
            track_id: track.track_id.clone(),
            can_seek: shadow.caps.control && shadow.caps.seek,
            can_go_next: shadow.caps.control && shadow.caps.next,
            can_go_previous: shadow.caps.control && shadow.caps.previous,
            volume_percent: shadow.volume_percent,
            volume_writable: shadow.caps.control && shadow.volume_writable,
            pid: shadow.pid,
        })
    }

    fn publish(&self, shared: &mut Shared) {
        let next = shared
            .players
            .preferred()
            .and_then(|p| self.now_playing(p, &shared.shadow));
        if next != shared.current.now_playing {
            let rev = shared.current.rev.wrapping_add(1);
            shared.current = MediaChanged {
                rev,
                now_playing: next,
            };
            self.topic.publish(shared.current.clone());
        }
    }

    // ---- control -----------------------------------------------------------------------

    #[cfg(target_os = "linux")]
    fn artwork_request(&self) -> Option<artwork::Key> {
        let shared = self.lock();
        let player = shared.players.preferred()?;
        let track = player.track.as_ref()?;
        Some((
            player.bus_name.clone(),
            track.track_id.clone(),
            track.title.clone(),
            track.art_url.clone()?,
        ))
    }

    #[cfg(target_os = "linux")]
    fn artwork_loaded(&self, key: &artwork::Key, image: Option<String>) {
        let mut shared = self.lock();
        let matches = shared.players.preferred().is_some_and(|p| {
            p.bus_name == key.0
                && p.track.as_ref().is_some_and(|t| {
                    t.track_id == key.1 && t.title == key.2 && t.art_url.as_ref() == Some(&key.3)
                })
        });
        if matches {
            if let Some(shadow) = shared.shadow.get_mut(&key.0) {
                shadow.artwork = image;
            }
            self.publish(&mut shared);
        }
    }

    /// `media_control`: reaches only the preferred player and only what it advertises.
    #[cfg(target_os = "linux")]
    pub async fn control(&self, command: MediaCommand) -> Result<(), IslandError> {
        let plan = self.plan(&command)?;
        let link = self
            .link
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .ok_or_else(|| IslandError::unavailable("Media service is not connected"))?;
        mpris::invoke(&link, &plan).await
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn control(&self, _command: MediaCommand) -> Result<(), IslandError> {
        Err(IslandError::unsupported(
            "Media control is not supported here",
        ))
    }

    /// Decide which player and call a command maps to, or why it is refused.
    pub(crate) fn plan(&self, command: &MediaCommand) -> Result<ControlPlan, IslandError> {
        let shared = self.lock();
        let player = shared
            .players
            .preferred()
            .filter(|p| p.track.is_some())
            .ok_or_else(|| IslandError::unavailable("Nothing is playing"))?;
        let shadow = shared
            .shadow
            .get(&player.bus_name)
            .ok_or_else(|| IslandError::unavailable("Nothing is playing"))?;
        let caps = &shadow.caps;
        let refuse = |what: &str| {
            Err(IslandError::unsupported(format!(
                "The player cannot {what}"
            )))
        };
        let call = match command {
            MediaCommand::Volume { percent } if caps.control && shadow.volume_writable => {
                if !percent.is_finite() || !(0.0..=100.0).contains(percent) {
                    return Err(IslandError::invalid(
                        "Player volume must be between 0 and 100",
                    ));
                }
                Call::Volume {
                    value: percent / 100.0,
                }
            }
            MediaCommand::Volume { .. } => return refuse("change volume"),
            MediaCommand::Play if caps.control && caps.play => Call::Method("Play"),
            MediaCommand::Pause if caps.control && caps.pause => Call::Method("Pause"),
            MediaCommand::PlayPause if caps.control && (caps.play || caps.pause) => {
                Call::Method("PlayPause")
            }
            MediaCommand::Next if caps.control && caps.next => Call::Method("Next"),
            MediaCommand::Previous if caps.control && caps.previous => Call::Method("Previous"),
            MediaCommand::Seek { position_ms } if caps.control && caps.seek => {
                if !position_ms.is_finite() || *position_ms < 0.0 {
                    return Err(IslandError::invalid(
                        "Seek position must be a finite, non-negative number",
                    ));
                }
                let target_us = (position_ms * 1000.0).round() as i64;
                let duration_us = player
                    .track
                    .as_ref()
                    .and_then(|t| t.length_ms)
                    .map(|ms| ms.saturating_mul(1000) as i64);
                if duration_us.is_some_and(|d| target_us > d) {
                    return Err(IslandError::invalid(
                        "Seek position is beyond the end of the track",
                    ));
                }
                match player.track.as_ref().and_then(|t| t.track_id.clone()) {
                    Some(track_id) if track_id.starts_with('/') && track_id.len() > 1 => {
                        Call::SetPosition {
                            track_id,
                            target_us,
                        }
                    }
                    _ => {
                        let current_us =
                            (model::position_at(player, self.now_ms()) as i64).saturating_mul(1000);
                        Call::Seek {
                            offset_us: target_us.saturating_sub(current_us),
                        }
                    }
                }
            }
            MediaCommand::Play | MediaCommand::PlayPause => return refuse("play"),
            MediaCommand::Pause => return refuse("pause"),
            MediaCommand::Next => return refuse("skip to the next track"),
            MediaCommand::Previous => return refuse("go to the previous track"),
            MediaCommand::Seek { .. } => return refuse("seek"),
        };
        Ok(ControlPlan {
            bus: player.bus_name.clone(),
            call,
        })
    }

    pub fn start(self: &Arc<Self>, address: Option<String>) {
        #[cfg(target_os = "linux")]
        if let Ok(mut slot) = self.art_task.lock() {
            *slot = Some(tauri::async_runtime::spawn(artwork::worker(
                Arc::clone(self),
                self.stop.subscribe(),
            )));
        }
        let service = Arc::clone(self);
        let stopped = self.stop.subscribe();
        let task = tauri::async_runtime::spawn(async move {
            #[cfg(target_os = "linux")]
            mpris::run(service, address, stopped).await;
            #[cfg(not(target_os = "linux"))]
            {
                let _ = (address, stopped);
                service.status.unavailable("unsupported-platform");
            }
        });
        if let Ok(mut slot) = self.task.lock() {
            *slot = Some(task);
        }
    }

    pub fn shutdown(&self) {
        self.stop.send_replace(true);
        if let Some(task) = self.art_task.lock().ok().and_then(|mut slot| slot.take()) {
            task.abort();
        }
        let task = self.task.lock().ok().and_then(|mut slot| slot.take());
        if let Some(task) = task {
            let _ = tauri::async_runtime::block_on(async {
                tokio::time::timeout(Duration::from_secs(3), task).await
            });
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn set_link(&self, link: Option<zbus::Connection>) {
        if let Ok(mut slot) = self.link.lock() {
            *slot = link;
        }
    }
}

/// The player call a command resolves to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Call {
    Volume { value: f64 },
    Method(&'static str),
    SetPosition { track_id: String, target_us: i64 },
    Seek { offset_us: i64 },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ControlPlan {
    pub bus: String,
    pub call: Call,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const A: &str = "org.mpris.MediaPlayer2.alpha";
    const B: &str = "org.mpris.MediaPlayer2.beta";

    fn props(status: &str, title: &str) -> Map<String, Value> {
        json!({
            "PlaybackStatus": status,
            "Metadata": {"xesam:title": title, "mpris:trackid": "/t/1", "mpris:length": 200_000_000i64},
            "Position": 5_000_000i64, "Rate": 1.0,
            "CanControl": true, "CanPlay": true, "CanPause": true, "CanSeek": true,
            "CanGoNext": true, "CanGoPrevious": false
        })
        .as_object()
        .cloned()
        .unwrap_or_default()
    }

    #[test]
    fn nothing_known_is_null_not_zero() {
        let svc = MediaService::new();
        assert_eq!(svc.current().now_playing, None);
        let mut p = props("Paused", "Song");
        p.remove("Position");
        svc.player_loaded(A, ":1.1", Some(10), &p);
        let now = svc.current().now_playing.unwrap();
        assert_eq!(now.position_ms, None, "never-learned position is null");
        assert_eq!(now.duration_ms, Some(200_000.0));
        svc.player_seeked(A, 7_000_000);
        assert_eq!(svc.current().now_playing.unwrap().position_ms, Some(7000.0));
    }

    #[test]
    fn removal_clears_and_second_player_preference_follows_playback() {
        let svc = MediaService::new();
        svc.player_loaded(A, ":1.1", None, &props("Playing", "One"));
        assert_eq!(svc.current().now_playing.unwrap().player, "alpha");
        std::thread::sleep(Duration::from_millis(5));
        svc.player_loaded(B, ":1.2", None, &props("Playing", "Two"));
        assert_eq!(svc.current().now_playing.unwrap().player, "beta");
        svc.player_removed(B);
        assert_eq!(svc.current().now_playing.unwrap().player, "alpha");
        svc.player_removed(A);
        assert_eq!(svc.current().now_playing, None);
        assert_eq!(
            svc.current().rev,
            4,
            "each observable change is one revision"
        );
    }

    #[test]
    fn plan_requires_advertised_capabilities_and_targets_the_preferred_player() {
        let svc = MediaService::new();
        assert!(svc.plan(&MediaCommand::Play).is_err(), "nothing playing");
        svc.player_loaded(A, ":1.1", None, &props("Playing", "One"));
        assert_eq!(
            svc.plan(&MediaCommand::Next).unwrap(),
            ControlPlan {
                bus: A.into(),
                call: Call::Method("Next")
            }
        );
        assert!(
            svc.plan(&MediaCommand::Previous).is_err(),
            "CanGoPrevious is false"
        );
        match svc
            .plan(&MediaCommand::Seek {
                position_ms: 12_345.0,
            })
            .unwrap()
            .call
        {
            Call::SetPosition {
                track_id,
                target_us,
            } => {
                assert_eq!((track_id.as_str(), target_us), ("/t/1", 12_345_000));
            }
            other => panic!("{other:?}"),
        }
        for bad in [f64::NAN, -1.0, 200_001.0] {
            assert!(
                svc.plan(&MediaCommand::Seek { position_ms: bad }).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn name_candidates_use_the_suffix_and_its_first_segment() {
        assert_eq!(
            name_candidates("org.mpris.MediaPlayer2.firefox.instance_1_5"),
            vec!["firefox.instance_1_5", "firefox"]
        );
        assert_eq!(
            name_candidates("org.mpris.MediaPlayer2.Spotify"),
            vec!["spotify"]
        );
    }

    #[test]
    fn volume_is_player_local_requires_writable_and_validates_finite_range() {
        let svc = MediaService::new();
        let mut p = props("Playing", "Song");
        p.insert("Volume".into(), json!(0.52));
        p.insert("VolumeWritable".into(), json!(true));
        svc.player_loaded(A, ":1.1", Some(123), &p);
        let now = svc.current().now_playing.unwrap();
        assert_eq!(now.volume_percent, Some(52.0));
        assert_eq!(now.pid, Some(123));
        assert_eq!(
            svc.plan(&MediaCommand::Volume { percent: 25.0 })
                .unwrap()
                .call,
            Call::Volume { value: 0.25 }
        );
        for percent in [f64::NAN, f64::INFINITY, -1.0, 101.0] {
            assert!(svc.plan(&MediaCommand::Volume { percent }).is_err());
        }
        p.insert("VolumeWritable".into(), json!(false));
        svc.player_props(A, &p);
        assert!(svc.plan(&MediaCommand::Volume { percent: 25.0 }).is_err());
    }

    #[test]
    fn artwork_never_publishes_raw_urls_and_stale_track_results_are_ignored() {
        let svc = MediaService::new();
        let mut p = props("Playing", "One");
        p["Metadata"]["mpris:artUrl"] = json!("https://i.scdn.co/image/a");
        svc.player_loaded(A, ":1.1", None, &p);
        let key = svc.artwork_request().unwrap();
        assert!(svc.current().now_playing.unwrap().art_url.is_none());
        p["Metadata"]["xesam:title"] = json!("Two");
        svc.player_props(A, &p);
        svc.artwork_loaded(&key, Some("data:image/jpeg;base64,AAAA".into()));
        assert!(svc.current().now_playing.unwrap().art_url.is_none());
        svc.artwork_loaded(
            &svc.artwork_request().unwrap(),
            Some("data:image/jpeg;base64,AAAA".into()),
        );
        assert!(svc.current().now_playing.unwrap().art_url.is_some());
    }
}
