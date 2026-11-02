//! Lyrics for the playing track (SPEC §8 Phase 6 "Music extras").
//!
//! Opt-in (`lyricsEnabled`): with it off the service makes NO network call and no cache
//! read. A lookup sends title / first artist / album / rounded duration to LRCLIB (keyless,
//! `lrclib.rs` builds the path and classifies the answer; the 200 body must be a JSON
//! object). Results are cached privately in the data dir (`lyrics-cache/<key>.json`, 0600,
//! atomic temp + rename, never outside it). The active synced line follows the MPRIS
//! position through the audited `lrc` core; the worker sleeps until the next line is due and
//! re-evaluates on every media change (seek, pause, track).

use super::{
    lrc,
    lrclib::{self, Lookup, TrackQuery},
    MediaService,
};
use crate::{
    bus::Latest,
    contract::system::{LyricLine, LyricsChanged, LyricsStatus, NowPlaying, ServiceStatus},
    integrations::service_status::Reporter,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub const DEFAULT_BASE: &str = "https://lrclib.net";
pub const ENV_BASE: &str = "HERMES_ISLAND_LRCLIB_BASE";
const TIMEOUT: Duration = Duration::from_secs(10);
/// Bodies larger than this are rejected by the core; reading stops one byte past it.
const MAX_BODY: usize = 1024 * 1024;
/// Cached lookups kept at most (no new file is written beyond this).
pub const MAX_CACHE_FILES: usize = 500;

#[derive(Clone, Debug)]
pub struct LyricsConfig {
    pub base: String,
    /// `<data>/lyrics-cache`; `None` disables caching.
    pub cache_dir: Option<PathBuf>,
}

impl LyricsConfig {
    /// The base URL can be redirected by the environment in test instances only.
    pub fn for_instance(
        test_instance: bool,
        cache_dir: Option<PathBuf>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Self {
        let base = if test_instance { env(ENV_BASE) } else { None };
        Self {
            base: base.unwrap_or_else(|| DEFAULT_BASE.to_owned()),
            cache_dir,
        }
    }
}

/// What the cache stores: the classified answer, never the raw body.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Cached {
    Synced { lrc: String, plain: Option<String> },
    Plain { plain: String },
    Instrumental,
}

impl Cached {
    fn from_lookup(lookup: &Lookup) -> Option<Self> {
        match lookup {
            Lookup::Synced { lrc, plain } => Some(Self::Synced {
                lrc: lrc.clone(),
                plain: plain.clone(),
            }),
            Lookup::Plain(plain) => Some(Self::Plain {
                plain: plain.clone(),
            }),
            Lookup::Instrumental => Some(Self::Instrumental),
            Lookup::NotFound | Lookup::RetryAfter { .. } | Lookup::Failed { .. } => None,
        }
    }

    fn into_lookup(self) -> Lookup {
        match self {
            Self::Synced { lrc, plain } => Lookup::Synced { lrc, plain },
            Self::Plain { plain } => Lookup::Plain(plain),
            Self::Instrumental => Lookup::Instrumental,
        }
    }
}

fn valid_key(key: &str) -> bool {
    key.len() == 16 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

fn cache_read(dir: &std::path::Path, key: &str) -> Option<Lookup> {
    use std::os::unix::fs::MetadataExt;
    if !valid_key(key) {
        return None;
    }
    let path = dir.join(format!("{key}.json"));
    let metadata = fs::symlink_metadata(&path).ok()?;
    // Only a private regular file we wrote ourselves is believed.
    if !metadata.is_file() || metadata.mode() & 0o077 != 0 || metadata.len() > (MAX_BODY as u64) * 2
    {
        return None;
    }
    let bytes = fs::read(&path).ok()?;
    serde_json::from_slice::<Cached>(&bytes)
        .ok()
        .map(Cached::into_lookup)
}

fn cache_write(dir: &std::path::Path, key: &str, lookup: &Lookup) -> bool {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let Some(cached) = Cached::from_lookup(lookup) else {
        return false;
    };
    if !valid_key(key) {
        return false;
    }
    if fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .is_err()
    {
        return false;
    }
    let target = dir.join(format!("{key}.json"));
    let existing = fs::read_dir(dir).map(|d| d.count()).unwrap_or(0);
    if existing >= MAX_CACHE_FILES && !target.exists() {
        return false;
    }
    let Ok(bytes) = serde_json::to_vec(&cached) else {
        return false;
    };
    let temp = dir.join(format!(".{key}.{}.tmp", std::process::id()));
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .and_then(|mut file| file.write_all(&bytes).and_then(|()| file.sync_all()))
        .and_then(|()| fs::rename(&temp, &target));
    if written.is_err() {
        let _removed = fs::remove_file(&temp);
        return false;
    }
    true
}

/// A resolved lookup, ready to publish.
struct Loaded {
    key: Option<String>,
    status: LyricsStatus,
    detail: &'static str,
    synced: Option<lrc::Lyrics>,
    plain: Option<String>,
}

impl Loaded {
    fn terminal(key: Option<String>, status: LyricsStatus, detail: &'static str) -> Self {
        Self {
            key,
            status,
            detail,
            synced: None,
            plain: None,
        }
    }

    fn from_lookup(key: Option<String>, lookup: Lookup) -> Self {
        match lookup {
            Lookup::Synced { lrc: text, plain } => match lrc::parse(&text) {
                Ok(synced) => Self {
                    key,
                    status: LyricsStatus::Synced,
                    detail: "lrclib",
                    synced: Some(synced),
                    plain,
                },
                // Synced text that does not parse still has plain lyrics, or nothing.
                Err(_) => match plain {
                    Some(plain) => Self {
                        key,
                        status: LyricsStatus::Plain,
                        detail: "lrclib",
                        synced: None,
                        plain: Some(plain),
                    },
                    None => Self::terminal(key, LyricsStatus::Error, "invalid-lyrics"),
                },
            },
            Lookup::Plain(plain) => Self {
                key,
                status: LyricsStatus::Plain,
                detail: "lrclib",
                synced: None,
                plain: Some(plain),
            },
            Lookup::Instrumental => Self::terminal(key, LyricsStatus::Instrumental, "lrclib"),
            Lookup::NotFound => Self::terminal(key, LyricsStatus::NotFound, "not-found"),
            Lookup::RetryAfter { .. } => Self::terminal(key, LyricsStatus::Error, "rate-limited"),
            Lookup::Failed { status: 200 } => {
                Self::terminal(key, LyricsStatus::Error, "invalid-response")
            }
            Lookup::Failed { .. } => Self::terminal(key, LyricsStatus::Error, "http-error"),
        }
    }
}

struct State {
    current: LyricsChanged,
}

pub struct LyricsService {
    pub status: Arc<Reporter>,
    pub topic: Latest<LyricsChanged>,
    config: LyricsConfig,
    client: reqwest::Client,
    state: Mutex<State>,
    enabled: watch::Sender<bool>,
    stop: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    retry_until: Mutex<Option<Instant>>,
}

fn query_for(now: &NowPlaying) -> TrackQuery {
    TrackQuery {
        title: now.title.clone(),
        artist: now.artist.split(", ").next().unwrap_or_default().to_owned(),
        album: (!now.album.is_empty()).then(|| now.album.clone()),
        length_ms: now.duration_ms.map(|ms| ms as u64),
    }
}

impl LyricsService {
    pub fn new(config: LyricsConfig, enabled: bool) -> Result<Arc<Self>, reqwest::Error> {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .connect_timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(lrclib::USER_AGENT)
            .build()?;
        let current = LyricsChanged {
            rev: 0,
            status: if enabled {
                LyricsStatus::Idle
            } else {
                LyricsStatus::Off
            },
            track_key: None,
            lines: Vec::new(),
            plain: None,
            active_index: None,
        };
        let topic = Latest::new();
        topic.publish(current.clone());
        let service = Arc::new(Self {
            status: Reporter::new("lyrics"),
            topic,
            config,
            client,
            state: Mutex::new(State { current }),
            enabled: watch::channel(enabled).0,
            stop: watch::channel(false).0,
            task: Mutex::new(None),
            retry_until: Mutex::new(None),
        });
        if !enabled {
            service.status.unavailable("disabled");
        }
        Ok(service)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn current(&self) -> LyricsChanged {
        self.lock().current.clone()
    }

    pub fn status(&self) -> ServiceStatus {
        self.status.get()
    }

    /// Opt-in switch. Turning it off stops all network use immediately.
    pub fn configure(&self, enabled: bool) {
        self.enabled.send_if_modified(|current| {
            let changed = *current != enabled;
            *current = enabled;
            changed
        });
    }

    fn publish(
        &self,
        status: LyricsStatus,
        key: Option<&str>,
        loaded: Option<&Loaded>,
        active: Option<usize>,
    ) {
        let lines: Vec<LyricLine> = loaded
            .and_then(|l| l.synced.as_ref())
            .map(|synced| {
                synced
                    .lines
                    .iter()
                    .map(|line| LyricLine {
                        time_ms: (line.start_ms as i64 - synced.offset_ms).max(0) as f64,
                        text: line.text.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let next = LyricsChanged {
            rev: 0,
            status,
            track_key: key.map(str::to_owned),
            lines,
            plain: loaded.and_then(|l| l.plain.clone()),
            active_index: active.map(|i| i as u32),
        };
        let mut state = self.lock();
        let unchanged = LyricsChanged {
            rev: state.current.rev,
            ..next.clone()
        } == state.current;
        if !unchanged {
            let mut next = next;
            next.rev = state.current.rev.wrapping_add(1);
            state.current = next.clone();
            self.topic.publish(next);
        }
    }

    /// One LRCLIB lookup (no cache). Public so the `#[ignore]`d real-network smoke can call it.
    pub async fn fetch(&self, query: &TrackQuery) -> Lookup {
        if let Some(until) = self.retry_until.lock().ok().and_then(|slot| *slot) {
            if let Some(remaining) = until.checked_duration_since(Instant::now()) {
                return Lookup::RetryAfter {
                    seconds: remaining.as_secs().saturating_add(1),
                };
            }
        }
        let Some(path) = lrclib::get_path(query) else {
            return Lookup::NotFound;
        };
        let mut result = self.fetch_path(query, &path, false).await;
        if result == Lookup::NotFound {
            if let Some(path) = lrclib::search_path(query) {
                tokio::time::sleep(Duration::from_millis(250)).await;
                result = self.fetch_path(query, &path, true).await;
            }
        }
        if let Lookup::RetryAfter { seconds } = &result {
            if let Ok(mut until) = self.retry_until.lock() {
                *until = Some(Instant::now() + Duration::from_secs(*seconds));
            }
        }
        result
    }

    async fn fetch_path(&self, query: &TrackQuery, path: &str, search: bool) -> Lookup {
        let url = format!("{}{}", self.config.base.trim_end_matches('/'), path);
        let Ok(mut response) = self
            .client
            .get(url)
            .header("accept", "application/json")
            .send()
            .await
        else {
            return Lookup::Failed { status: 0 };
        };
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let mut body = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    body.extend_from_slice(&chunk);
                    if body.len() > MAX_BODY {
                        break; // the core rejects an oversize body
                    }
                }
                Ok(None) => break,
                Err(_) => return Lookup::Failed { status },
            }
        }
        if search {
            lrclib::parse_search(
                query,
                status,
                content_type.as_deref(),
                retry_after.as_deref(),
                &body,
            )
        } else {
            lrclib::parse_response(
                status,
                content_type.as_deref(),
                retry_after.as_deref(),
                &body,
            )
        }
    }

    async fn resolve(&self, query: &TrackQuery, key: Option<String>) -> Loaded {
        if let (Some(dir), Some(key)) = (&self.config.cache_dir, &key) {
            if let Some(lookup) = cache_read(dir, key) {
                return Loaded::from_lookup(Some(key.clone()), lookup);
            }
        }
        self.status.unavailable("fetching");
        let lookup = self.fetch(query).await;
        if let (Some(dir), Some(key)) = (&self.config.cache_dir, &key) {
            let _cached = cache_write(dir, key, &lookup);
        }
        Loaded::from_lookup(key, lookup)
    }

    pub fn start(self: &Arc<Self>, media: Arc<MediaService>) {
        let service = Arc::clone(self);
        let stop = self.stop.subscribe();
        let enabled = self.enabled.subscribe();
        let task = tauri::async_runtime::spawn(service.worker(media, enabled, stop));
        if let Ok(mut slot) = self.task.lock() {
            *slot = Some(task);
        }
    }

    async fn worker(
        self: Arc<Self>,
        media: Arc<MediaService>,
        mut enabled: watch::Receiver<bool>,
        mut stop: watch::Receiver<bool>,
    ) {
        let mut media_rx = media.topic.subscribe();
        let mut loaded: Option<Loaded> = None;
        loop {
            if *stop.borrow() {
                return;
            }
            let on = *enabled.borrow_and_update();
            media_rx.borrow_and_update();
            let now = media.current().now_playing;
            match (on, now) {
                (false, _) => {
                    loaded = None;
                    self.publish(LyricsStatus::Off, None, None, None);
                    self.status.unavailable("disabled");
                }
                (true, None) => {
                    loaded = None;
                    self.publish(LyricsStatus::Idle, None, None, None);
                    self.status.unavailable("idle");
                }
                (true, Some(now)) => {
                    let query = query_for(&now);
                    let key = lrclib::cache_key(&query);
                    if loaded.as_ref().map(|l| &l.key) != Some(&key) {
                        self.publish(LyricsStatus::Loading, key.as_deref(), None, None);
                        let resolved = tokio::select! {
                            _ = stop.changed() => return,
                            _ = enabled.changed() => continue,
                            _ = media_rx.changed() => continue,
                            resolved = self.resolve(&query, key.clone()) => resolved,
                        };
                        match resolved.status {
                            LyricsStatus::Error => self.status.unavailable(resolved.detail),
                            _ => self.status.available(resolved.detail),
                        }
                        loaded = Some(resolved);
                    }
                }
            }
            let mut deadline = None;
            if let Some(current) = loaded.as_ref() {
                let (active, due) = Self::timing(&media, current);
                self.publish(
                    current.status,
                    current.key.as_deref(),
                    Some(current),
                    active,
                );
                deadline = due;
            }
            tokio::select! {
                _ = stop.changed() => return,
                _ = enabled.changed() => {}
                _ = media_rx.changed() => {}
                () = async {
                    match deadline {
                        Some(delay) => tokio::time::sleep(delay).await,
                        None => std::future::pending().await,
                    }
                } => {}
            }
        }
    }

    /// Active line index at the preferred player's position, and how long until the next
    /// line is due (only while playing).
    fn timing(media: &MediaService, loaded: &Loaded) -> (Option<usize>, Option<Duration>) {
        let Some(synced) = loaded.synced.as_ref() else {
            return (None, None);
        };
        let Some((position, rate, playing)) = media.position_now() else {
            return (None, None);
        };
        let active = synced.active_index(position);
        if !playing {
            return (active, None);
        }
        let next = active.map_or(0, |i| i + 1);
        let due = synced.lines.get(next).map(|line| {
            let line_at = line.start_ms as i128 - synced.offset_ms as i128;
            let wait_ms = (line_at - position as i128).max(0) as f64 / rate;
            Duration::from_millis(wait_ms.ceil() as u64 + 1)
        });
        (active, due)
    }

    pub fn shutdown(&self) {
        self.stop.send_replace(true);
        let task = self.task.lock().ok().and_then(|mut slot| slot.take());
        if let Some(task) = task {
            let _ = tauri::async_runtime::block_on(async {
                tokio::time::timeout(Duration::from_secs(3), task).await
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "explicit read-only LRCLIB network smoke"]
    async fn spotify_track_lyrics_parse_through_real_service() {
        let service = LyricsService::new(
            LyricsConfig {
                base: DEFAULT_BASE.into(),
                cache_dir: None,
            },
            true,
        )
        .unwrap();
        let query = TrackQuery {
            title: "八方來財(Stacks from All Sides)".into(),
            artist: "攬佬SKAI ISYOURGOD".into(),
            album: Some("八方來財(Stacks from All Sides)".into()),
            length_ms: Some(173_037),
        };
        let loaded = Loaded::from_lookup(lrclib::cache_key(&query), service.fetch(&query).await);
        assert_eq!(loaded.status, LyricsStatus::Synced);
        println!(
            "Actual Spotify track: {} parsed synced lyric lines",
            loaded.synced.unwrap().lines.len()
        );
    }

    #[tokio::test]
    async fn retry_after_applies_across_track_changes_and_reenable() {
        let service = LyricsService::new(
            LyricsConfig {
                base: "http://127.0.0.1:1".into(),
                cache_dir: None,
            },
            true,
        )
        .unwrap();
        *service.retry_until.lock().unwrap() = Some(Instant::now() + Duration::from_secs(120));
        service.configure(false);
        service.configure(true);
        assert!(
            matches!(service.fetch(&TrackQuery { title: "Another".into(), artist: "Artist".into(), album: None, length_ms: None }).await, Lookup::RetryAfter { seconds } if seconds >= 119)
        );
    }

    #[test]
    fn malformed_sync_falls_back_to_plain_lyrics() {
        let loaded = Loaded::from_lookup(
            None,
            Lookup::Synced {
                lrc: "not an LRC document".into(),
                plain: Some("Plain lyrics".into()),
            },
        );
        assert_eq!(loaded.status, LyricsStatus::Plain);
        assert_eq!(loaded.plain.as_deref(), Some("Plain lyrics"));
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hi-lyrics-{tag}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _removed = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn cache_roundtrips_private_files_and_ignores_everything_else() {
        let root = scratch("cache");
        let _guard = Cleanup(root.clone());
        let dir = root.join("lyrics-cache");
        let key = "0123456789abcdef";
        let synced = Lookup::Synced {
            lrc: "[00:01.00]hi".into(),
            plain: Some("hi".into()),
        };
        assert!(cache_write(&dir, key, &synced));
        assert_eq!(cache_read(&dir, key), Some(synced));
        use std::os::unix::fs::PermissionsExt;
        let file = dir.join(format!("{key}.json"));
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        // Wider permissions, garbage and unknown shapes are not believed.
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        assert_eq!(
            cache_read(&dir, key),
            None,
            "group-only access is not private"
        );
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(cache_read(&dir, key), None);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&file, br#"{"kind":"synced","lrc":"x","extra":1}"#).unwrap();
        assert_eq!(cache_read(&dir, key), None);
        // Only answers are cached; misses and failures are retried next time.
        for lookup in [
            Lookup::NotFound,
            Lookup::RetryAfter { seconds: 5 },
            Lookup::Failed { status: 500 },
        ] {
            assert!(!cache_write(&dir, "fedcba9876543210", &lookup));
        }
        // A key is a 16-digit hex id; nothing can escape the directory.
        assert!(!cache_write(&dir, "../../etc/x", &Lookup::Instrumental));
        assert_eq!(cache_read(&dir, "../x"), None);
    }

    #[test]
    fn query_uses_the_first_artist_and_rounded_duration() {
        let now = NowPlaying {
            player: "p".into(),
            title: "Song".into(),
            artist: "A, B".into(),
            album: String::new(),
            playing: true,
            position_ms: None,
            duration_ms: Some(200_400.0),
            art_url: None,
            can_control: false,
            bus_name: "org.mpris.MediaPlayer2.p".into(),
            status: "playing".into(),
            rate: 1.0,
            sampled_at_ms: 0.0,
            track_id: None,
            can_seek: false,
            can_go_next: false,
            can_go_previous: false,
            volume_percent: None,
            volume_writable: false,
            pid: None,
        };
        let query = query_for(&now);
        assert_eq!(query.artist, "A");
        assert_eq!(query.album, None, "an empty album is not sent");
        assert_eq!(query.length_ms, Some(200_400));
    }

    #[test]
    fn config_redirects_the_base_in_test_instances_only() {
        let env = |name: &str| (name == ENV_BASE).then(|| "http://127.0.0.1:1".to_owned());
        assert_eq!(
            LyricsConfig::for_instance(true, None, env).base,
            "http://127.0.0.1:1"
        );
        assert_eq!(
            LyricsConfig::for_instance(false, None, env).base,
            DEFAULT_BASE
        );
    }
}
