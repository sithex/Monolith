//! Hermes skin palette: bounded `island/palette.py` run + directory watcher (port of the
//! island-rust palette code in `services.rs` / `host.rs`).
//!
//! The watcher watches the Hermes profile dir and its `skins/` dir (non-recursive), ignores
//! `Access` events (reading the palette must not trigger another refresh), debounces 120 ms,
//! re-attaches the `skins/` watch when the directory is replaced, has a serial fallback refresh,
//! publishes only when the palette differs and is stopped + joined on shutdown.

use super::{hermes_launcher, hermes_python, process::run_bounded};
use crate::{
    contract::{palette::Palette, settings::is_hex_color},
    error::IslandError,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{mpsc, Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub const DEBOUNCE: Duration = Duration::from_millis(120);
pub const FALLBACK: Duration = Duration::from_secs(30 * 60);
/// Re-read this often when no file watcher could be started at all.
pub const UNWATCHED_FALLBACK: Duration = Duration::from_secs(30);

/// Strict validation (identical to island-rust `parse_palette`): a string `name` and eight
/// `#rrggbb` roles; everything else in the payload is discarded.
pub fn parse_palette(bytes: &[u8]) -> Result<Palette, IslandError> {
    let invalid = || IslandError::invalid("Invalid Hermes palette");
    let raw: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let role = |key: &str| -> Result<String, IslandError> {
        raw[key]
            .as_str()
            .filter(|s| is_hex_color(s))
            .map(str::to_owned)
            .ok_or_else(invalid)
    };
    let name = raw["name"].as_str().ok_or_else(invalid)?.to_owned();
    Ok(Palette {
        name,
        background: role("background")?,
        accent: role("accent")?,
        text: role("text")?,
        primary: role("primary")?,
        dim: role("dim")?,
        border: role("border")?,
        surface: role("surface")?,
        error: role("error")?,
    })
}

/// Does `event` concern the palette inputs (`config.yaml` or anything under `skins/`)?
pub fn palette_path_changed(profile: &Path, event: &notify::Event) -> bool {
    // Reading the palette must not trigger another refresh via Access events.
    !matches!(event.kind, notify::EventKind::Access(_))
        && event.paths.iter().any(|path| {
            path == &profile.join("config.yaml") || path.starts_with(profile.join("skins"))
        })
}

pub struct PaletteService {
    root: PathBuf,
    home: PathBuf,
    cache: Mutex<Option<Palette>>,
    /// Serializes Hermes skin writes (`sync_hermes_theme`).
    theme: Mutex<()>,
}

/// Bound for `island/hermes-theme.py`: it may run the Hermes CLI (`hermes skin use`) once.
pub const THEME_TIMEOUT: Duration = Duration::from_secs(45);

impl PaletteService {
    pub fn new(root: PathBuf, home: PathBuf) -> Self {
        Self {
            root,
            home,
            cache: Mutex::new(None),
            theme: Mutex::new(()),
        }
    }

    /// Nov 2 Hermes theme sync: `Some` writes the island scheme as the `ripple-island` skin and
    /// activates it (remembering the previous skin); `None` restores that previous skin. Only
    /// public Hermes surfaces are touched (a skin file + `hermes skin use`). Returns the active
    /// skin name. The palette watcher then publishes the new skin palette as usual.
    pub fn sync_hermes_theme(&self, palette: Option<&Palette>) -> Result<String, IslandError> {
        let _serial = self
            .theme
            .lock()
            .map_err(|_| IslandError::internal("Theme sync poisoned"))?;
        let args = match palette {
            Some(p) => {
                let roles = [
                    &p.background,
                    &p.accent,
                    &p.text,
                    &p.primary,
                    &p.dim,
                    &p.border,
                    &p.surface,
                    &p.error,
                ];
                if !roles.iter().all(|r| is_hex_color(r)) {
                    return Err(IslandError::invalid("Invalid theme palette"));
                }
                let json = serde_json::to_string(p)
                    .map_err(|_| IslandError::internal("Cannot encode theme palette"))?;
                vec!["apply".to_owned(), json]
            }
            None => vec!["restore".to_owned()],
        };
        let mut command = Command::new(hermes_python(&self.home)?);
        command
            .args(["-I", "-B"])
            .arg(self.root.join("island/hermes-theme.py"))
            .arg(self.home.join(".hermes/hermes-agent"))
            .arg(hermes_launcher(&self.home))
            .args(args);
        command.env("PYTHONDONTWRITEBYTECODE", "1");
        let out = run_bounded(command, THEME_TIMEOUT, 4096, false)?;
        let reply: Value = serde_json::from_slice(&out)
            .map_err(|_| IslandError::internal("Invalid Hermes theme reply"))?;
        reply["skin"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 80)
            .map(str::to_owned)
            .ok_or_else(|| IslandError::internal("Hermes theme sync failed"))
    }

    /// Run `palette.py` through the Hermes Python (bounded: 3 s, 16 KiB).
    pub fn compute(&self) -> Result<Palette, IslandError> {
        let mut command = Command::new(hermes_python(&self.home)?);
        command
            .args(["-I", "-B"])
            .arg(self.root.join("island/palette.py"))
            .arg(self.home.join(".hermes/hermes-agent"));
        command.env("PYTHONDONTWRITEBYTECODE", "1");
        parse_palette(&run_bounded(command, Duration::from_secs(3), 16384, false)?)
    }

    pub fn cached(&self) -> Option<Palette> {
        self.cache.lock().ok().and_then(|c| c.clone())
    }

    /// Cached palette, computing (bounded) when the cache is empty.
    pub fn get(&self) -> Result<Palette, IslandError> {
        if let Some(hit) = self.cached() {
            return Ok(hit);
        }
        let palette = self.compute()?;
        self.store(&palette);
        Ok(palette)
    }

    /// Store; `true` when it differs from the previous value.
    fn store(&self, palette: &Palette) -> bool {
        match self.cache.lock() {
            Ok(mut cache) => {
                let changed = cache.as_ref() != Some(palette);
                if changed {
                    *cache = Some(palette.clone());
                }
                changed
            }
            Err(_) => false,
        }
    }

    /// Recompute; `Some(palette)` only when it changed.
    pub fn refresh(&self) -> Option<Palette> {
        match self.compute() {
            Ok(palette) => self.store(&palette).then_some(palette),
            Err(error) => {
                tracing::debug!(%error, "palette refresh failed; keeping the cached palette");
                None
            }
        }
    }
}

enum Message {
    Fs(notify::Result<notify::Event>),
    Stop,
}

pub struct PaletteWatcher {
    stop: mpsc::Sender<Message>,
    worker: Option<JoinHandle<()>>,
}

impl PaletteWatcher {
    /// Start watching. `on_change` receives only genuinely new palettes (including the first
    /// one computed at startup).
    pub fn start(
        service: Arc<PaletteService>,
        profile: PathBuf,
        fallback: Duration,
        on_change: Arc<dyn Fn(Palette) + Send + Sync>,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<Message>();
        let event_tx = tx.clone();
        let skins = profile.join("skins");
        let config = profile.join("config.yaml");
        let (filter_profile, filter_skins) = (profile.clone(), skins.clone());
        let mut watcher = match notify::recommended_watcher(
            move |event: notify::Result<notify::Event>| {
                // Drop unrelated events here, before they reach the worker: the profile directory
                // also holds Hermes' busy database and logs.
                if let Ok(event) = &event {
                    if !palette_path_changed(&filter_profile, event)
                        && !event.paths.contains(&filter_skins)
                    {
                        return;
                    }
                }
                // A send error only means the worker already stopped.
                if event_tx.send(Message::Fs(event)).is_err() {
                    tracing::trace!("palette watcher event after shutdown");
                }
            },
        ) {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                tracing::warn!(%error, "palette file watcher unavailable; using the serial fallback only");
                None
            }
        };
        // Watch config.yaml itself rather than the whole profile directory: the directory sees
        // a constant stream of database writes, and every one of them woke this watcher. The
        // file watch is re-armed after each change (an atomic replace swaps the inode).
        // Without a config.yaml yet, the directory is watched so its creation is still seen.
        let mut config_watched = false;
        if let Some(watcher) = watcher.as_mut() {
            use notify::Watcher;
            config_watched = watcher
                .watch(&config, notify::RecursiveMode::NonRecursive)
                .is_ok();
            let dirs: &[&PathBuf] = if config_watched {
                &[&skins]
            } else {
                &[&profile, &skins]
            };
            for dir in dirs {
                if let Err(error) = watcher.watch(dir, notify::RecursiveMode::NonRecursive) {
                    tracing::warn!(%error, dir = %dir.display(), "cannot watch palette directory");
                }
            }
        }
        let fallback = if watcher.is_some() {
            fallback
        } else {
            fallback.min(UNWATCHED_FALLBACK)
        };
        let worker = std::thread::spawn(move || {
            use notify::Watcher;
            let publish = |service: &PaletteService| {
                if let Some(palette) = service.refresh() {
                    on_change(palette);
                }
            };
            publish(&service);
            let mut refreshed = Instant::now();
            let mut pending: Option<Instant> = None;
            loop {
                let now = Instant::now();
                let fallback_due = refreshed + fallback;
                let wake = pending.map_or(fallback_due, |p| p.min(fallback_due));
                match rx.recv_timeout(wake.saturating_duration_since(now)) {
                    Ok(Message::Fs(Ok(event))) => {
                        if palette_path_changed(&profile, &event) {
                            pending = Some(Instant::now() + DEBOUNCE);
                        }
                        // Re-attach after the skins directory is created/replaced.
                        if !matches!(event.kind, notify::EventKind::Access(_))
                            && event.paths.contains(&skins)
                        {
                            if let Some(watcher) = watcher.as_mut() {
                                if let Err(error) = watcher.unwatch(&skins) {
                                    tracing::trace!(%error, "skins unwatch");
                                }
                                if let Err(error) =
                                    watcher.watch(&skins, notify::RecursiveMode::NonRecursive)
                                {
                                    tracing::debug!(%error, "skins re-watch failed");
                                }
                            }
                        }
                    }
                    // The serial fallback also covers watch errors.
                    Ok(Message::Fs(Err(error))) => tracing::debug!(%error, "palette watch error"),
                    Ok(Message::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                let now = Instant::now();
                if pending.is_some_and(|deadline| now >= deadline) || now >= refreshed + fallback {
                    pending = None;
                    // Re-arm the config.yaml watch BEFORE reading, so a replace that lands
                    // after this point is still seen.
                    if config_watched {
                        if let Some(watcher) = watcher.as_mut() {
                            if let Err(error) = watcher.unwatch(&config) {
                                tracing::trace!(%error, "config unwatch");
                            }
                            if let Err(error) =
                                watcher.watch(&config, notify::RecursiveMode::NonRecursive)
                            {
                                tracing::debug!(%error, "config re-watch failed; watching the profile directory");
                                config_watched = false;
                                if let Err(error) =
                                    watcher.watch(&profile, notify::RecursiveMode::NonRecursive)
                                {
                                    tracing::warn!(%error, "cannot watch palette directory");
                                }
                            }
                        }
                    }
                    publish(&service);
                    refreshed = Instant::now();
                }
            }
        });
        Self {
            stop: tx,
            worker: Some(worker),
        }
    }

    /// Stop and join (idempotent).
    pub fn stop(&mut self) {
        if self.stop.send(Message::Stop).is_err() {
            tracing::trace!("palette watcher already stopped");
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::warn!("palette watcher thread panicked");
            }
        }
    }
}

impl Drop for PaletteWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::scratch;
    use serde_json::json;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn theme_sync_rejects_non_hex_roles_before_running_anything() {
        let dir = scratch("theme-sync");
        let service = PaletteService::new(dir.join("root"), dir.join("home"));
        let mut palette = Palette {
            name: "ripple-island".into(),
            background: "#080d0a".into(),
            accent: "#80ff36".into(),
            text: "#d8e2d7".into(),
            primary: "#e7eee4".into(),
            dim: "#8a9c8d".into(),
            border: "#3d6243".into(),
            surface: "#121c15".into(),
            error: "#ff7272".into(),
        };
        palette.dim = "red; rm -rf ~".into();
        let error = service.sync_hermes_theme(Some(&palette)).unwrap_err();
        assert!(
            error.to_string().contains("Invalid theme palette"),
            "{error}"
        );
        // A valid palette gets past validation and then needs a real Hermes install.
        palette.dim = "#8a9c8d".into();
        assert!(!service
            .sync_hermes_theme(Some(&palette))
            .unwrap_err()
            .to_string()
            .contains("Invalid theme palette"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn palette_contract_validation() {
        let good = json!({"name":"Necron","background":"#123456","accent":"#ABCDEF","text":"#123456","primary":"#123456","dim":"#123456","border":"#123456","surface":"#123456","error":"#123456","untrusted":"discard"});
        let parsed = parse_palette(&serde_json::to_vec(&good).unwrap()).unwrap();
        assert_eq!(parsed.accent, "#ABCDEF");
        assert_eq!(
            serde_json::to_value(&parsed)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            9
        );
        assert!(parse_palette(br##"{"name":"Bad","accent":"#abcdef"}"##).is_err());
        assert!(parse_palette(b"not json").is_err());
        let mut bad = good.clone();
        bad["dim"] = json!("red");
        assert!(parse_palette(&serde_json::to_vec(&bad).unwrap()).is_err());
        let mut bad = good;
        bad["name"] = json!(7);
        assert!(parse_palette(&serde_json::to_vec(&bad).unwrap()).is_err());
    }

    /// Home with a Hermes launcher and a `palette.py` that derives the accent from
    /// `<profile>/<watched>` so the test controls the palette by writing a file.
    fn fixture(source: &str) -> (PathBuf, PathBuf, Arc<PaletteService>) {
        let home = scratch("palette");
        let root = home.join("app");
        fs::create_dir_all(root.join("island")).unwrap();
        let launcher = home.join(".hermes/hermes-agent/.hermes/bin/hermes");
        fs::create_dir_all(launcher.parent().unwrap()).unwrap();
        fs::write(&launcher, "#!/bin/sh\nexec /usr/bin/python3 \"$@\"\n").unwrap();
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir_all(home.join(".hermes/skins")).unwrap();
        let script = format!(
            "import json,os,sys\nagent=sys.argv[1]\nsrc=os.path.join(os.path.dirname(agent),'{source}')\ntry:\n    accent=open(src).read().strip()\nexcept OSError:\n    accent='#000000'\nprint(json.dumps({{'name':'Fixture','background':'#111111','accent':accent,'text':'#ffffff','primary':'#eeeeee','dim':'#999999','border':'#444444','surface':'#222222','error':'#ff8888'}}))\n"
        );
        fs::write(root.join("island/palette.py"), script).unwrap();
        let service = Arc::new(PaletteService::new(root, home.clone()));
        let profile = home.join(".hermes");
        (home, profile, service)
    }

    fn collector() -> (Arc<dyn Fn(Palette) + Send + Sync>, mpsc::Receiver<Palette>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        (
            Arc::new(move |p| {
                if let Ok(tx) = tx.lock() {
                    if tx.send(p).is_err() {
                        eprintln!("palette receiver dropped");
                    }
                }
            }),
            rx,
        )
    }

    fn atomic_replace(target: &Path, text: &str) {
        let staged = target.with_extension("new");
        fs::write(&staged, text).unwrap();
        fs::rename(staged, target).unwrap();
    }

    #[test]
    fn service_caches_and_reports_only_changes() {
        let (home, profile, service) = fixture("skins/current.txt");
        fs::write(profile.join("skins/current.txt"), "#abcdef").unwrap();
        let first = service.get().unwrap();
        assert_eq!(first.accent, "#abcdef");
        fs::write(profile.join("skins/current.txt"), "#123456").unwrap();
        assert_eq!(
            service.get().unwrap().accent,
            "#abcdef",
            "get() serves the cache"
        );
        assert_eq!(service.refresh().unwrap().accent, "#123456");
        assert!(
            service.refresh().is_none(),
            "unchanged palette is not re-published"
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn appearance_watch_filters_reads_and_tracks_atomic_replacements() {
        // Part 1 (port of the island-rust test): the raw notify filter.
        let dir = scratch("palette-notify");
        let skins = dir.join("skins");
        fs::create_dir(&skins).unwrap();
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            if tx.send(event).is_err() {
                eprintln!("watch receiver dropped");
            }
        })
        .unwrap();
        use notify::Watcher;
        watcher
            .watch(&dir, notify::RecursiveMode::NonRecursive)
            .unwrap();
        watcher
            .watch(&skins, notify::RecursiveMode::NonRecursive)
            .unwrap();
        for target in [dir.join("config.yaml"), skins.join("fixture.yaml")] {
            atomic_replace(&target, "fixture: true");
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let event = rx
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .unwrap()
                    .unwrap();
                if event.paths.contains(&target) && palette_path_changed(&dir, &event) {
                    break;
                }
            }
        }
        let access = notify::Event::new(notify::EventKind::Access(notify::event::AccessKind::Read))
            .add_path(dir.join("config.yaml"));
        assert!(!palette_path_changed(&dir, &access));
        let unrelated = notify::Event::new(notify::EventKind::Any).add_path(dir.join("state.db"));
        assert!(!palette_path_changed(&dir, &unrelated));
        drop(watcher);
        fs::remove_dir_all(dir).unwrap();

        // Part 2: the real watcher publishes atomic replacements once, ignores unrelated files,
        // survives the skins dir being replaced, and stops promptly.
        let (home, profile, service) = fixture("skins/current.txt");
        fs::write(profile.join("skins/current.txt"), "#111111").unwrap();
        let (sink, events) = collector();
        let mut watcher =
            PaletteWatcher::start(service, profile.clone(), Duration::from_secs(3600), sink);
        assert_eq!(
            events.recv_timeout(Duration::from_secs(5)).unwrap().accent,
            "#111111"
        );
        atomic_replace(&profile.join("skins/current.txt"), "#222222");
        assert_eq!(
            events.recv_timeout(Duration::from_secs(5)).unwrap().accent,
            "#222222"
        );
        // Unrelated file in the profile dir: no event, no extra publish.
        fs::write(profile.join("state.db"), "x").unwrap();
        // Rewriting identical content publishes nothing either (palette unchanged).
        atomic_replace(&profile.join("skins/current.txt"), "#222222");
        assert!(events.recv_timeout(Duration::from_millis(700)).is_err());
        // skins/ replaced wholesale: the watch is re-attached and later edits are seen.
        fs::rename(profile.join("skins"), profile.join("skins.old")).unwrap();
        fs::create_dir(profile.join("skins")).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        fs::write(profile.join("skins/current.txt"), "#333333").unwrap();
        // The replacement itself legitimately publishes the (now empty) skin first; the edit
        // inside the NEW directory is only observable if the watch was re-attached (the serial
        // fallback is set to an hour here).
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let accent = events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("edit inside the replaced skins dir was never seen")
                .accent;
            if accent == "#333333" {
                break;
            }
            assert_eq!(accent, "#000000", "unexpected intermediate palette");
        }
        let start = Instant::now();
        watcher.stop();
        assert!(start.elapsed() < Duration::from_secs(2));
        watcher.stop();
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn config_file_watch_survives_repeated_atomic_replaces() {
        let (home, profile, service) = fixture("config.yaml");
        fs::write(profile.join("config.yaml"), "#a1a1a1").unwrap();
        let (sink, events) = collector();
        let mut watcher =
            PaletteWatcher::start(service, profile.clone(), Duration::from_secs(3600), sink);
        assert_eq!(
            events.recv_timeout(Duration::from_secs(5)).unwrap().accent,
            "#a1a1a1"
        );
        // Each replace swaps the inode; the watch must be re-armed every time.
        for accent in ["#b2b2b2", "#c3c3c3", "#d4d4d4"] {
            atomic_replace(&profile.join("config.yaml"), accent);
            assert_eq!(
                events.recv_timeout(Duration::from_secs(5)).unwrap().accent,
                accent
            );
        }
        // In-place edits are seen too.
        fs::write(profile.join("config.yaml"), "#e5e5e5").unwrap();
        assert_eq!(
            events.recv_timeout(Duration::from_secs(5)).unwrap().accent,
            "#e5e5e5"
        );
        // Database churn in the profile directory publishes nothing.
        for _ in 0..20 {
            fs::write(profile.join("state.db-wal"), "x").unwrap();
        }
        assert!(events.recv_timeout(Duration::from_millis(700)).is_err());
        watcher.stop();
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn serial_fallback_picks_up_changes_the_watcher_cannot_see() {
        // The palette source lives outside the watched directories.
        let (home, profile, service) = fixture("unwatched.txt");
        fs::write(home.join(".hermes/unwatched.txt"), "#101010").unwrap();
        fs::create_dir_all(home.join(".hermes/elsewhere")).unwrap();
        let src = home.join(".hermes/unwatched.txt");
        let (sink, events) = collector();
        // Watch a decoy profile so the real source is genuinely unwatched.
        let decoy = home.join("decoy");
        fs::create_dir_all(decoy.join("skins")).unwrap();
        let mut watcher = PaletteWatcher::start(service, decoy, Duration::from_millis(400), sink);
        assert_eq!(
            events.recv_timeout(Duration::from_secs(5)).unwrap().accent,
            "#101010"
        );
        fs::write(&src, "#202020").unwrap();
        assert_eq!(
            events.recv_timeout(Duration::from_secs(5)).unwrap().accent,
            "#202020"
        );
        watcher.stop();
        assert!(profile.exists());
        fs::remove_dir_all(home).unwrap();
    }
}
