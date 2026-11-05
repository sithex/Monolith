//! The tray is an input adapter only: every action is a typed actor intent.
use crate::{
    contract::island::{IslandMode, IslandState, UiIntent},
    error::IslandError,
    state::actor::Handle,
};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIcon, TrayIconBuilder},
};

const ICON: &[u8] = include_bytes!("../icons/tray.png");

/// Menu entries that are actor intents. Everything else on the menu is a host-side
/// system action (`system_action`), which never mutates island state directly.
pub fn action(id: &str, state: &IslandState) -> Option<UiIntent> {
    Some(match id {
        "toggle" => {
            if state.mode == IslandMode::Still {
                UiIntent::Pin
            } else {
                UiIntent::Collapse
            }
        }
        "dnd" => UiIntent::SetDnd { on: !state.dnd },
        "restart" => UiIntent::RestartTerminal,
        "quit" => UiIntent::Quit,
        _ => return None,
    })
}

/// Tray entries that act on the desktop/session rather than on the island UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAction {
    RestartApp,
    RestartPlasma,
    OpenFolder,
    OpenData,
    ViewLog,
    Autostart,
}

pub fn system_action(id: &str) -> Option<SystemAction> {
    Some(match id {
        "restart-app" => SystemAction::RestartApp,
        "restart-plasma" => SystemAction::RestartPlasma,
        "open-folder" => SystemAction::OpenFolder,
        "open-data" => SystemAction::OpenData,
        "view-log" => SystemAction::ViewLog,
        "autostart" => SystemAction::Autostart,
        _ => return None,
    })
}

/// Start `argv` in its own transient systemd scope so it outlives this process (and is not
/// killed with our cgroup when we quit). Falls back to a plain child if systemd-run is absent.
fn launch_detached(argv: &[String], unit: Option<&str>) -> std::io::Result<u32> {
    let mut command = Command::new("systemd-run");
    command.args(["--user", "--scope", "--collect", "--quiet"]);
    if let Some(unit) = unit {
        command.arg(format!("--unit={unit}"));
    }
    command.arg("--").args(argv);
    let spawned = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .or_else(|_| {
            Command::new(&argv[0])
                .args(&argv[1..])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        })?;
    let pid = spawned.id();
    std::thread::spawn(move || {
        let mut child = spawned;
        let _ = child.wait();
    });
    Ok(pid)
}

fn cgroup_of(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()
}

pub fn autostart_file() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("autostart/hermes-island.desktop"))
}

/// Enabled = the entry exists and is not marked `Hidden=true` (how Plasma disables entries).
pub fn autostart_enabled(text: Option<&str>) -> bool {
    text.is_some_and(|t| {
        !t.lines()
            .any(|l| l.trim().eq_ignore_ascii_case("hidden=true"))
    })
}

pub fn autostart_entry(root: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Monolith\nComment=Monolith dynamic island overlay\n\
         Exec=\"{}/island-tauri/run.sh\" --overlay\nPath={}\nTerminal=false\n\
         X-KDE-autostart-phase=2\n",
        root.display(),
        root.display()
    )
}

fn set_autostart(on: bool) -> std::io::Result<()> {
    let path = autostart_file().ok_or_else(|| std::io::Error::other("no config directory"))?;
    if on {
        // Rewrite from the running checkout: also repairs entries left pointing at an old path.
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, autostart_entry(&crate::paths::repo_root()))
    } else {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let mut kept: Vec<&str> = text
                    .lines()
                    .filter(|l| !l.trim().to_ascii_lowercase().starts_with("hidden="))
                    .collect();
                kept.push("Hidden=true");
                std::fs::write(&path, kept.join("\n") + "\n")
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

fn run_system(action: SystemAction, router: &Handle) -> Result<(), String> {
    let root = crate::paths::repo_root();
    let data = root.join(".island-data-overlay");
    let s = |p: &Path| p.display().to_string();
    match action {
        SystemAction::RestartApp => {
            let pid = std::process::id();
            let run = root.join("island-tauri/run.sh");
            if !run.is_file() {
                return Err(format!("{} missing", run.display()));
            }
            let mut argv = vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                // Wait (bounded) for this instance to release its locks, then relaunch.
                "i=0; while kill -0 \"$1\" 2>/dev/null && [ $i -lt 150 ]; do sleep 0.2; i=$((i+1)); done; shift; exec \"$@\"".to_owned(),
                "monolith-relaunch".to_owned(),
                pid.to_string(),
                s(&run),
            ];
            argv.extend(std::env::args().skip(1));
            let ours = cgroup_of(pid);
            let helper = launch_detached(
                &argv,
                Some(&format!(
                    "monolith-{pid}-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                )),
            )
            .map_err(|e| format!("relaunch helper: {e}"))?;
            // Quit only after the helper has left our cgroup, or a service-managed instance
            // would take it down on exit.
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                let theirs = cgroup_of(helper);
                if theirs.is_some() && theirs != ours {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            router
                .try_dispatch(UiIntent::Quit.into())
                .map_err(|e| e.to_string())
        }
        SystemAction::RestartPlasma => launch_detached(
            &[
                "systemctl".into(),
                "--user".into(),
                "restart".into(),
                "plasma-plasmashell.service".into(),
            ],
            None,
        )
        .map(drop)
        .map_err(|e| e.to_string()),
        SystemAction::OpenFolder => launch_detached(&["xdg-open".into(), s(&root)], None)
            .map(drop)
            .map_err(|e| e.to_string()),
        SystemAction::OpenData => launch_detached(&["xdg-open".into(), s(&data)], None)
            .map(drop)
            .map_err(|e| e.to_string()),
        SystemAction::ViewLog => launch_detached(
            &[
                "konsole".into(),
                "--separate".into(),
                "-p".into(),
                "tabtitle=Monolith log".into(),
                "-e".into(),
                "tail".into(),
                "-n".into(),
                "300".into(),
                "-F".into(),
                s(&data.join("monolith.log")),
            ],
            None,
        )
        .map(drop)
        .map_err(|e| e.to_string()),
        SystemAction::Autostart => {
            let now = autostart_enabled(
                autostart_file()
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .as_deref(),
            );
            set_autostart(!now).map_err(|e| e.to_string())
        }
    }
}

/// Tauri's GTK tray backend silently ignores `set_tooltip`. Ayatana 0.6 exposes the
/// tooltip as GObject properties, but libappindicator-rs does not expose its GObject.
/// Capture it through the public `new-icon` signal while refreshing OUR icon once;
/// never inspect the private Rust wrapper layout or replace the native SNI service.
#[cfg(target_os = "linux")]
mod linux_tooltip {
    use super::*;
    use glib::{prelude::*, translate::*, Object, SendWeakRef};
    use std::ptr;

    pub struct Tooltip(SendWeakRef<Object>);

    impl Tooltip {
        pub fn new(icon: &TrayIcon) -> Result<Self, IslandError> {
            icon.with_inner_tray_icon(|inner| {
                let kind = glib::Type::from_name("AppIndicator")
                    .ok_or_else(|| IslandError::unavailable("AppIndicator type unavailable"))?;
                // SAFETY: GLib owns the type and signal metadata. The hook is synchronous,
                // removed before `captured` leaves scope, and only retains a GObject ref.
                unsafe extern "C" fn capture(
                    _: *mut glib::gobject_ffi::GSignalInvocationHint,
                    count: u32,
                    values: *const glib::gobject_ffi::GValue,
                    data: glib::ffi::gpointer,
                ) -> glib::ffi::gboolean {
                    if count > 0 && !values.is_null() {
                        let raw = glib::gobject_ffi::g_value_get_object(values);
                        if !raw.is_null() {
                            let object: Object = from_glib_none(raw);
                            if object.property::<String>("id") == "tray-icon tray app hermes-island"
                            {
                                *data.cast::<Option<Object>>() = Some(object);
                            }
                        }
                    }
                    1 // keep hook until the caller removes it, even for an unrelated indicator
                }
                let mut captured: Option<Object> = None;
                let signal = unsafe {
                    glib::gobject_ffi::g_signal_lookup(c"new-icon".as_ptr(), kind.into_glib())
                };
                if signal == 0 {
                    return Err(IslandError::unavailable(
                        "AppIndicator new-icon unavailable",
                    ));
                }
                let hook = unsafe {
                    glib::gobject_ffi::g_signal_add_emission_hook(
                        signal,
                        0,
                        Some(capture),
                        ptr::from_mut(&mut captured).cast(),
                        None,
                    )
                };
                if hook == 0 {
                    return Err(IslandError::unavailable("Cannot observe AppIndicator"));
                }
                let refreshed = tauri::image::Image::from_bytes(ICON)
                    .and_then(|image| image.try_into())
                    .map_err(|_| IslandError::internal("Cannot decode tray icon"))
                    .and_then(|image| {
                        inner
                            .set_icon(Some(image))
                            .map_err(|_| IslandError::unavailable("Cannot refresh tray icon"))
                    });
                unsafe { glib::gobject_ffi::g_signal_remove_emission_hook(signal, hook) };
                refreshed?;
                let object = captured
                    .ok_or_else(|| IslandError::unavailable("Native tray object unavailable"))?;
                if object.find_property("tooltip-title").is_none() {
                    return Err(IslandError::unsupported("Ayatana tooltip support required"));
                }
                Ok(Self(object.downgrade().into()))
            })
            .map_err(|_| IslandError::unavailable("Cannot access native tray"))?
        }

        pub fn set(&self, icon: &TrayIcon, text: String) -> Result<(), IslandError> {
            let object = self.0.clone();
            icon.with_inner_tray_icon(move |_| {
                // SendWeakRef is only upgraded on the GTK thread where it was created.
                let object = object
                    .upgrade()
                    .ok_or_else(|| IslandError::unavailable("Native tray object disappeared"))?;
                object.set_property("tooltip-title", text);
                Ok(())
            })
            .map_err(|_| IslandError::unavailable("Cannot update native tray tooltip"))?
        }
    }
}

const GAME_MODE_ID: &str = "game-mode";

/// The manual game-mode setting a tray/side-control click asks for: on unless it is on now.
pub fn game_mode_request(state: &IslandState) -> bool {
    !state.game_mode
}

/// Persists `gameMode` through the normal settings path; the check mark always ends up
/// showing the effective state (GTK flips it on click before we know the outcome).
fn toggle_game_mode(app: &tauri::AppHandle, actor: &Handle, item: CheckMenuItem<tauri::Wry>) {
    let on = game_mode_request(&actor.state());
    let app = app.clone();
    let actor = actor.clone();
    tauri::async_runtime::spawn(async move {
        let patch = crate::contract::settings::SettingsPatch {
            game_mode: Some(on),
            ..Default::default()
        };
        if let Err(error) = crate::commands::settings::patch_from_host(&app, patch).await {
            tracing::warn!(%error, "Tray game mode change failed");
        }
        if let Err(error) = item.set_checked(actor.state().game_mode) {
            tracing::warn!(%error, "Tray game mode update failed");
        }
    });
}

pub struct Tray {
    _icon: TrayIcon,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}
impl Tray {
    pub fn start(app: &tauri::AppHandle, actor: Handle) -> Result<Self, IslandError> {
        // Subscribe BEFORE constructing native widgets: changes during construction must
        // not be lost, and the first refresh must not wait for another actor publication.
        let mut state = actor.watch();
        let initial = state.borrow().clone();
        let build = || -> tauri::Result<_> {
            let toggle = MenuItem::with_id(app, "toggle", "Show/Hide island", true, None::<&str>)?;
            let dnd = CheckMenuItem::with_id(
                app,
                "dnd",
                "Do Not Disturb",
                true,
                initial.dnd,
                None::<&str>,
            )?;
            let game = CheckMenuItem::with_id(
                app,
                GAME_MODE_ID,
                "Game mode",
                true,
                initial.game_mode,
                None::<&str>,
            )?;
            let restart_app =
                MenuItem::with_id(app, "restart-app", "Restart Monolith", true, None::<&str>)?;
            let restart =
                MenuItem::with_id(app, "restart", "Restart terminal", true, None::<&str>)?;
            let restart_plasma = MenuItem::with_id(
                app,
                "restart-plasma",
                "Restart Plasma panel",
                true,
                None::<&str>,
            )?;
            let open_folder = MenuItem::with_id(
                app,
                "open-folder",
                "Open Monolith folder",
                true,
                None::<&str>,
            )?;
            let open_data =
                MenuItem::with_id(app, "open-data", "Open data folder", true, None::<&str>)?;
            let view_log = MenuItem::with_id(app, "view-log", "View log", true, None::<&str>)?;
            let autostart = CheckMenuItem::with_id(
                app,
                "autostart",
                "Start at login",
                true,
                autostart_enabled(
                    autostart_file()
                        .and_then(|p| std::fs::read_to_string(p).ok())
                        .as_deref(),
                ),
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &toggle,
                    &dnd,
                    &game,
                    &PredefinedMenuItem::separator(app)?,
                    &restart_app,
                    &restart,
                    &restart_plasma,
                    &PredefinedMenuItem::separator(app)?,
                    &open_folder,
                    &open_data,
                    &view_log,
                    &PredefinedMenuItem::separator(app)?,
                    &autostart,
                    &quit,
                ],
            )?;
            let router = actor.clone();
            let autostart_item = autostart.clone();
            let game_item = game.clone();
            let icon = TrayIconBuilder::with_id("hermes-island")
                .icon(tauri::image::Image::from_bytes(ICON)?)
                .menu(&menu)
                .tooltip(format!("Monolith — {} unread", initial.unread))
                .on_menu_event(move |app, event| {
                    let id = event.id.as_ref();
                    if id == GAME_MODE_ID {
                        toggle_game_mode(app, &router, game_item.clone());
                    } else if let Some(system) = system_action(id) {
                        if let Err(error) = run_system(system, &router) {
                            tracing::warn!(%error, ?system, "Tray system action failed");
                        }
                        if system == SystemAction::Autostart {
                            let on = autostart_enabled(
                                autostart_file()
                                    .and_then(|p| std::fs::read_to_string(p).ok())
                                    .as_deref(),
                            );
                            if let Err(error) = autostart_item.set_checked(on) {
                                tracing::warn!(%error, "Tray autostart refresh failed");
                            }
                        }
                    } else if let Some(intent) = action(id, &router.state()) {
                        if let Err(error) = router.try_dispatch(intent.into()) {
                            tracing::warn!(%error, "Tray action refused by actor");
                        }
                    }
                })
                .build(app)?;
            Ok((icon, dnd, game))
        };
        let (icon, dnd, game) =
            build().map_err(|_| IslandError::unavailable("Cannot create system tray"))?;
        #[cfg(target_os = "linux")]
        let tooltip = linux_tooltip::Tooltip::new(&icon)?;
        let update_icon = icon.clone();
        let task = tauri::async_runtime::spawn(async move {
            loop {
                let next = state.borrow_and_update().clone();
                if let Err(error) = dnd.set_checked(next.dnd) {
                    tracing::warn!(%error, "Tray DND update failed");
                }
                if let Err(error) = game.set_checked(next.game_mode) {
                    tracing::warn!(%error, "Tray game mode update failed");
                }
                let text = format!("Monolith — {} unread", next.unread);
                #[cfg(target_os = "linux")]
                let updated = tooltip.set(&update_icon, text);
                #[cfg(not(target_os = "linux"))]
                let updated = update_icon.set_tooltip(Some(text));
                if let Err(error) = updated {
                    tracing::warn!(%error, "Tray tooltip update failed");
                }
                if state.changed().await.is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            _icon: icon,
            task: Mutex::new(Some(task)),
        })
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        if let Some(task) = self
            .task
            .lock()
            .map_err(|_| IslandError::internal("Tray slot poisoned"))?
            .take()
        {
            task.abort();
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contract::{island::IslandView, settings::Settings},
        state::{
            actor::{self, MonotonicClock, Sink, Snapshot},
            machine::{Clock, Effect, Intent, Machine},
        },
    };
    use std::sync::Arc;
    struct Quiet;
    impl Sink for Quiet {
        fn apply(&mut self, _: &[Effect], _: &Snapshot) -> Result<(), IslandError> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn every_tray_action_maps_to_a_typed_actor_intent() {
        let (actor, task) = actor::spawn(
            Settings::default(),
            Arc::new(MonotonicClock::default()),
            Quiet,
        );
        for id in ["terminal", "notifications", "settings", "clipboard"] {
            assert!(action(id, &actor.state()).is_none(), "{id} is in-app only");
        }
        actor
            .dispatch(
                UiIntent::SelectView {
                    view: IslandView::Terminal,
                }
                .into(),
            )
            .await
            .unwrap();
        assert_eq!(actor.state().mode, IslandMode::Large);
        actor
            .dispatch(action("toggle", &actor.state()).unwrap().into())
            .await
            .unwrap();
        assert_eq!(actor.state().mode, IslandMode::Still);
        actor
            .dispatch(action("toggle", &actor.state()).unwrap().into())
            .await
            .unwrap();
        assert_eq!(actor.state().mode, IslandMode::Large);
        actor
            .dispatch(action("dnd", &actor.state()).unwrap().into())
            .await
            .unwrap();
        assert!(actor.state().dnd);
        assert!(matches!(
            action("restart", &actor.state()),
            Some(UiIntent::RestartTerminal)
        ));
        assert!(matches!(
            action("quit", &actor.state()),
            Some(UiIntent::Quit)
        ));
        assert!(action("unknown", &actor.state()).is_none());
        actor.shutdown();
        task.await.unwrap().unwrap();
    }
    #[test]
    fn system_entries_are_not_actor_intents() {
        for (id, want) in [
            ("restart-app", SystemAction::RestartApp),
            ("restart-plasma", SystemAction::RestartPlasma),
            ("open-folder", SystemAction::OpenFolder),
            ("open-data", SystemAction::OpenData),
            ("view-log", SystemAction::ViewLog),
            ("autostart", SystemAction::Autostart),
        ] {
            assert_eq!(system_action(id), Some(want));
        }
        assert_eq!(system_action("quit"), None);
        assert_eq!(system_action("restart"), None);
    }
    #[test]
    fn autostart_state_and_entry() {
        assert!(!autostart_enabled(None));
        assert!(autostart_enabled(Some("[Desktop Entry]\nExec=x\n")));
        assert!(!autostart_enabled(Some("[Desktop Entry]\nHidden=true\n")));
        assert!(autostart_enabled(Some("[Desktop Entry]\nHidden=false\n")));
        let entry = autostart_entry(Path::new("/opt/Mono lith"));
        assert!(entry.contains("Exec=\"/opt/Mono lith/island-tauri/run.sh\" --overlay\n"));
        assert!(entry.contains("Path=/opt/Mono lith\n"));
        assert!(autostart_enabled(Some(&entry)));
    }
    #[test]
    fn restart_and_quit_are_effects_of_the_actor_not_tray_side_effects() {
        struct Now;
        impl Clock for Now {
            fn now_ms(&self) -> u64 {
                0
            }
        }
        let settings = Settings::default();
        let mut m = Machine::new(&settings);
        assert!(m
            .reduce(Intent::from(UiIntent::RestartTerminal), &settings, &Now)
            .unwrap()
            .contains(&Effect::RestartTerminal));
        assert!(m
            .reduce(Intent::from(UiIntent::Quit), &settings, &Now)
            .unwrap()
            .contains(&Effect::Quit));
    }
}
