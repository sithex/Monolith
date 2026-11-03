//! Ported island-rust services (SPEC §1.1). Every service takes its directories explicitly so
//! tests run against scratch dirs and a scratch `HOME`.

pub mod ai_usage;
pub mod autostart;
pub mod frontend;
pub mod palette;
pub mod process;
pub mod settings;
pub mod stats;
pub mod terminal;
pub mod voice;
pub mod workspace;

use crate::{
    bus::{self, Latest},
    cli::Cli,
    contract::{
        island::{RuntimeInfo, UiKind},
        palette::Palette,
        settings::Settings,
    },
    error::IslandError,
    testhooks::TestHooks,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// `<home>/.hermes/hermes-agent/.hermes/bin/hermes`
pub fn hermes_launcher(home: &Path) -> PathBuf {
    home.join(".hermes/hermes-agent/.hermes/bin/hermes")
}

/// The Hermes Python parsed from the launcher's `exec <abs>/python3 …` line (same lenient rule
/// as island-rust: first matching `exec` line whose first token is an absolute existing file).
pub fn hermes_python(home: &Path) -> Result<PathBuf, IslandError> {
    let unresolved = || IslandError::unavailable("Cannot resolve Hermes Python from launcher");
    let launcher = fs::read_to_string(hermes_launcher(home)).map_err(|_| unresolved())?;
    for line in launcher.lines() {
        if let Some(rest) = line.strip_prefix("exec ") {
            if let Some(file) = rest.split_whitespace().next() {
                let path = PathBuf::from(file);
                if path.is_absolute() && file.ends_with("python3") && path.is_file() {
                    return Ok(path);
                }
            }
        }
    }
    Err(unresolved())
}

/// Scratch-directory helper for unit tests (never touches the real home).
#[cfg(test)]
pub(crate) fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "hi-tauri-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Hermes profile directory: `HERMES_HOME` if set, else `<home>/.hermes`.
pub fn profile_dir(hermes_home: Option<std::ffi::OsString>, home: &Path) -> PathBuf {
    hermes_home
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".hermes"))
}

/// Directories and flags a [`Services`] instance is built from. Everything is explicit so unit
/// tests can use scratch directories and a scratch `HOME`.
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub repo_root: PathBuf,
    pub data_dir: PathBuf,
    pub home: PathBuf,
    pub overlay: bool,
    pub test_instance: bool,
    pub fixture_pty: bool,
    pub ui: UiKind,
    pub test_hooks: bool,
}

impl ServiceConfig {
    pub fn from_cli(cli: &Cli, repo_root: PathBuf, data_dir: PathBuf, test_hooks: bool) -> Self {
        Self {
            repo_root,
            data_dir,
            home: std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/")),
            overlay: cli.overlay,
            test_instance: cli.test_instance,
            fixture_pty: cli.fixture_pty,
            ui: cli.ui,
            test_hooks,
        }
    }
}

/// Everything the IPC commands need, managed as Tauri state.
pub struct Services {
    pub runtime: RuntimeInfo,
    pub settings: Arc<settings::SettingsService>,
    pub palette: Arc<palette::PaletteService>,
    pub voice: Arc<voice::VoiceService>,
    pub ai: Arc<ai_usage::AiUsageService>,
    pub stats: Arc<stats::StatsService>,
    pub terminal: Arc<terminal::Terminal>,
    pub settings_topic: Arc<Latest<Settings>>,
    pub palette_topic: Arc<Latest<Palette>>,
    pub test_hooks: Mutex<Option<Arc<TestHooks>>>,
    palette_watcher: Mutex<Option<palette::PaletteWatcher>>,
    pub overlay: bool,
    /// Nov 2: the island's active colour-scheme roles (UI-owned palettes), so the native
    /// notch (hover tint, clock text) follows the scheme. `None` = follow the Hermes skin.
    pub scheme_palette: Mutex<Option<Palette>>,
}

impl Services {
    pub fn new(cfg: &ServiceConfig) -> Self {
        let autostart = autostart::Autostart::new(
            cfg.home.clone(),
            cfg.repo_root.clone(),
            cfg.repo_root.join("island-tauri/run.sh"),
            // Test instances never touch a real home (SPEC rule 2).
            cfg.test_instance,
        );
        let settings = Arc::new(settings::SettingsService::new(
            cfg.data_dir.clone(),
            autostart,
        ));
        Self {
            runtime: RuntimeInfo {
                overlay: cfg.overlay,
                panel_anchor: false,
                platform: std::env::consts::OS.to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                test_instance: cfg.test_instance,
                test_hooks: cfg.test_hooks,
                ui: cfg.ui,
            },
            settings: settings.clone(),
            palette: Arc::new(palette::PaletteService::new(
                cfg.repo_root.clone(),
                cfg.home.clone(),
            )),
            voice: Arc::new(voice::VoiceService::isolated(&cfg.home, cfg.test_instance)),
            ai: Arc::new(ai_usage::AiUsageService::new(
                cfg.repo_root.clone(),
                cfg.home.clone(),
            )),
            stats: Arc::new(stats::StatsService::new(cfg.home.clone())),
            terminal: Arc::new(terminal::Terminal::with_settings(
                terminal::TerminalConfig::new(
                    cfg.repo_root.clone(),
                    cfg.home.clone(),
                    cfg.fixture_pty,
                ),
                settings,
            )),
            settings_topic: Arc::new(Latest::new()),
            palette_topic: Arc::new(Latest::new()),
            test_hooks: Mutex::new(None),
            palette_watcher: Mutex::new(None),
            overlay: cfg.overlay,
            scheme_palette: Mutex::new(None),
        }
    }

    /// Spawn the coalescing forwarders that turn topic values into typed island events.
    pub fn start_event_forwarders(&self, app: &tauri::AppHandle) {
        let handle = app.clone();
        tauri::async_runtime::spawn(self.settings_topic.forwarder(bus::COALESCE, move |s| {
            bus::emit_settings(&handle, s);
            crate::integrations::plasma::runtime::nudge_status(&handle);
            frontend::publish(&handle);
            crate::commands::workspace::publish(&handle);
        }));
        let handle = app.clone();
        tauri::async_runtime::spawn(self.palette_topic.forwarder(bus::COALESCE, move |p| {
            bus::emit_palette(&handle, p);
            crate::integrations::plasma::runtime::nudge_status(&handle);
            frontend::publish(&handle);
        }));
    }

    /// Watch the Hermes profile directory for skin changes (see [`profile_dir`]).
    pub fn start_palette_watcher(&self, profile: PathBuf) {
        let topic = Arc::clone(&self.palette_topic);
        let watcher = palette::PaletteWatcher::start(
            self.palette.clone(),
            profile,
            palette::FALLBACK,
            Arc::new(move |p| topic.publish(p)),
        );
        match self.palette_watcher.lock() {
            Ok(mut slot) => *slot = Some(watcher),
            Err(_) => tracing::warn!("palette watcher slot is poisoned"),
        }
    }

    /// Stop watchers/hooks and kill the PTY's owned process tree (RunEvent::Exit).
    pub fn shutdown(&self) {
        if let Ok(mut slot) = self.palette_watcher.lock() {
            if let Some(mut watcher) = slot.take() {
                watcher.stop();
            }
        }
        if let Ok(mut slot) = self.test_hooks.lock() {
            if let Some(hooks) = slot.take() {
                hooks.shutdown();
            }
        }
        self.terminal.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn config(dir: &Path, test_instance: bool) -> ServiceConfig {
        ServiceConfig {
            repo_root: dir.join("repo"),
            data_dir: dir.join("data"),
            home: dir.join("home"),
            overlay: true,
            test_instance,
            fixture_pty: false,
            ui: UiKind::Legacy,
            test_hooks: false,
        }
    }

    #[test]
    fn profile_dir_prefers_hermes_home() {
        let home = Path::new("/h");
        assert_eq!(profile_dir(None, home), PathBuf::from("/h/.hermes"));
        assert_eq!(profile_dir(Some("/p".into()), home), PathBuf::from("/p"));
    }

    #[test]
    fn services_wire_up_in_scratch_dirs_and_shut_down_cleanly() {
        let dir = scratch("services");
        let cfg = config(&dir, true);
        let services = Services::new(&cfg);
        let runtime = serde_json::to_value(&services.runtime).unwrap();
        assert_eq!(runtime["overlay"], true);
        assert_eq!(runtime["panelAnchor"], false);
        assert_eq!(runtime["testInstance"], true);
        assert_eq!(runtime["testHooks"], false);
        assert_eq!(runtime["ui"], "legacy");
        assert_eq!(runtime["platform"], "linux");
        // Settings live in the scratch data dir; autostart is refused for a test instance and
        // the scratch home stays untouched.
        let patch: crate::contract::settings::SettingsPatch =
            serde_json::from_value(serde_json::json!({"autostart": true})).unwrap();
        assert!(services.settings.patch(&patch).is_err());
        assert!(!dir.join("home/.config").exists());
        services.start_palette_watcher(dir.join("home/.hermes"));
        std::thread::sleep(Duration::from_millis(50));
        let start = std::time::Instant::now();
        services.shutdown();
        assert!(start.elapsed() < Duration::from_secs(2));
        services.shutdown(); // idempotent
        fs::remove_dir_all(dir).unwrap();
    }
}
