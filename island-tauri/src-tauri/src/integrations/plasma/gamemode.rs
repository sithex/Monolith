//! Game mode focus detection + the global "toggle island" shortcut. Linux/Plasma only.
//!
//! Fullscreen detection: a tiny KWin script (`gamemode-focus.qml`, loaded only while
//! `gameModeAuto` is on) reports `Fullscreen(b)` to this process's `local.hermes.Monolith.GameMode`
//! endpoint on every focus change and on the focused window's `fullScreenChanged`. It ignores
//! the island itself, panels, OSDs, popups and notifications.
//!
//! Global shortcut: `openShortcut` is registered with KGlobalAccel (`org.kde.kglobalaccel`,
//! component `hermes-island`, action `toggle-island`). A key already used by another component
//! is never taken: the action stays unbound and the conflict is logged. A press is the same
//! `PanelPin` toggle a notch click dispatches.
//!
//! Everything is event-driven: the worker sleeps on settings changes, bus-owner changes of
//! `org.kde.KWin` / `org.kde.kglobalaccel`, and the shortcut signal. When KWin goes away the
//! fullscreen flag is cleared (game mode falls back to manual only) and the script is reloaded
//! ONCE when KWin is back; a failed load is logged once and not retried until KWin or the
//! setting changes again, so a crashing KWin can never make the island spin.
use crate::integrations::dbus_util;
use crate::keyseq;
use anyhow::{Context, Result};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::watch;
use zbus::Connection;

pub const BUS_NAME: &str = "local.hermes.Monolith.GameMode";
pub const OBJECT_PATH: &str = "/GameMode";
pub const INTERFACE: &str = "local.hermes.Monolith.GameMode";
/// The KWin plugin name the island owns; only this name is ever unloaded.
pub const SCRIPT_NAME: &str = "monolith-gamemode-focus";
const SCRIPT: &str = include_str!("gamemode-focus.qml");

pub const KWIN: &str = "org.kde.KWin";
pub const ACCEL: &str = "org.kde.kglobalaccel";
const ACCEL_PATH: &str = "/kglobalaccel";
const ACCEL_IFACE: &str = "org.kde.KGlobalAccel";
const COMPONENT_IFACE: &str = "org.kde.kglobalaccel.Component";
pub const COMPONENT: &str = "hermes-island";
pub const COMPONENT_FRIENDLY: &str = "Hermes Island";
pub const ACTION: &str = "toggle-island";
pub const ACTION_FRIENDLY: &str = "Toggle Hermes Island";
/// KGlobalAccel `SetShortcutFlag`s.
const SET_PRESENT: u32 = 2;
const NO_AUTOLOADING: u32 = 4;

/// What the worker tells the island.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The focused window is (not) fullscreen. `false` too when detection became unavailable.
    Fullscreen(bool),
    /// The global shortcut was pressed.
    Toggle,
}

/// The two settings the worker follows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Desired {
    pub auto: bool,
    pub shortcut: String,
}

impl Desired {
    pub fn from_settings(s: &crate::contract::settings::Settings) -> Self {
        Self {
            auto: s.game_mode_auto,
            shortcut: s.open_shortcut.clone(),
        }
    }
}

/// Outcome of the last shortcut registration (for logs and tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShortcutStatus {
    /// Nothing requested yet, or KGlobalAccel is not running.
    Pending,
    /// `openShortcut` is empty; the action is unregistered.
    Unbound,
    Bound(String),
    /// Another component owns the key; the island action stays unbound.
    Conflict {
        key: String,
        owners: Vec<String>,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub script_loaded: bool,
    pub shortcut: ShortcutStatus,
}

// ---------------------------------------------------------------------------------------------
// Pure reconciler: what to do after any input. No I/O, unit-tested below.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Script {
    Absent,
    /// A load was requested and its result is not in yet.
    Loading,
    Loaded,
    /// Load failed for this KWin incarnation + setting; never retried until one changes.
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    LoadScript,
    UnloadScript,
    /// Clear the fullscreen flag (detection unavailable or switched off).
    ClearFullscreen,
    ApplyShortcut(String),
}

#[derive(Clone, Debug)]
pub struct Plan {
    /// Unique names of the current owners (`None` = not running).
    kwin: Option<String>,
    accel: Option<String>,
    want: Desired,
    script: Script,
    /// The shortcut text last applied to the current KGlobalAccel incarnation.
    applied: Option<String>,
}

impl Plan {
    pub fn new(want: Desired) -> Self {
        Self {
            kwin: None,
            accel: None,
            want,
            script: Script::Absent,
            applied: None,
        }
    }
    pub fn settings(&mut self, want: Desired) -> Vec<Action> {
        if want.auto != self.want.auto && self.script == Script::Failed {
            self.script = Script::Absent;
        }
        self.want = want;
        self.reconcile()
    }
    /// The owner of `org.kde.KWin` now. A new owner is a new incarnation: state is redone;
    /// a repeated report of the same owner (startup query racing the signal) is a no-op.
    pub fn kwin(&mut self, owner: Option<String>) -> Vec<Action> {
        if owner.is_some() && owner == self.kwin {
            return Vec::new();
        }
        let up = owner.is_some();
        let mut out = Vec::new();
        if matches!(self.script, Script::Loaded | Script::Loading) || !up {
            out.push(Action::ClearFullscreen);
        }
        self.kwin = owner;
        self.script = Script::Absent;
        out.extend(self.reconcile());
        out.dedup();
        out
    }
    pub fn accel(&mut self, owner: Option<String>) -> Vec<Action> {
        if owner.is_some() && owner == self.accel {
            return Vec::new();
        }
        self.accel = owner;
        self.applied = None;
        self.reconcile()
    }
    pub fn script_done(&mut self, ok: bool) -> Vec<Action> {
        if self.script != Script::Loading {
            // KWin went away meanwhile; its owner change already reset the state.
            return Vec::new();
        }
        self.script = match (ok, self.want.auto) {
            (true, true) => Script::Loaded,
            (false, true) => Script::Failed,
            (_, false) => Script::Absent,
        };
        match (ok, self.want.auto) {
            (true, true) => Vec::new(),
            // Switched off while loading: take it out again.
            (true, false) => vec![Action::UnloadScript, Action::ClearFullscreen],
            (false, _) => vec![Action::ClearFullscreen],
        }
    }
    pub fn script_loaded(&self) -> bool {
        self.script == Script::Loaded
    }
    fn reconcile(&mut self) -> Vec<Action> {
        let mut out = Vec::new();
        if self.kwin.is_some() {
            match (self.want.auto, self.script) {
                (true, Script::Absent) => {
                    self.script = Script::Loading;
                    out.push(Action::LoadScript);
                }
                (false, Script::Loaded) => {
                    self.script = Script::Absent;
                    out.push(Action::UnloadScript);
                    out.push(Action::ClearFullscreen);
                }
                _ => {}
            }
        }
        if self.accel.is_some() && self.applied.as_deref() != Some(self.want.shortcut.as_str()) {
            self.applied = Some(self.want.shortcut.clone());
            out.push(Action::ApplyShortcut(self.want.shortcut.clone()));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// D-Bus endpoint the KWin script calls.

struct Endpoint {
    sink: Arc<dyn Fn(Event) + Send + Sync>,
}

#[zbus::interface(name = "local.hermes.Monolith.GameMode")]
impl Endpoint {
    /// The focused window is (not) fullscreen.
    async fn fullscreen(&self, on: bool) {
        (self.sink)(Event::Fullscreen(on));
    }
}

// ---------------------------------------------------------------------------------------------
// I/O helpers (all bounded).

/// A private per-user copy of the script (KWin loads scripts from a file path).
fn write_script(dir: &Path) -> Result<PathBuf> {
    if !dir.exists() {
        fs::DirBuilder::new().mode(0o700).create(dir)?;
    }
    let meta = fs::symlink_metadata(dir)?;
    // SAFETY: geteuid has no preconditions.
    if !meta.is_dir()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.permissions().mode() & 0o077 != 0
    {
        anyhow::bail!("Game mode script directory must be private");
    }
    let path = dir.join("gamemode-focus.qml");
    let temp = dir.join(format!(".gamemode-focus-{}.qml", std::process::id()));
    {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temp)?;
        file.write_all(SCRIPT.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&temp, &path)?;
    Ok(path)
}

async fn unload_script(conn: &Connection) -> Result<()> {
    let _: bool = dbus_util::call(
        conn,
        KWIN,
        "/Scripting",
        "org.kde.kwin.Scripting",
        "unloadScript",
        &(SCRIPT_NAME,),
    )
    .await?;
    Ok(())
}

/// (Re)loads and starts the focus script. A copy left by a crashed island is replaced.
async fn load_script(conn: &Connection, dir: &Path) -> Result<()> {
    // Own subdirectory: KWin's QML loader caches directory listings, so a file added to a
    // directory it already listed (the companion script's) fails with "File name case
    // mismatch". This directory is only ever seen by KWin with the script already in it.
    let path = write_script(&dir.join("gamemode"))?;
    let path = path
        .to_str()
        .context("Game mode script path must be UTF-8")?;
    unload_script(conn).await?;
    let id: i32 = dbus_util::call(
        conn,
        KWIN,
        "/Scripting",
        "org.kde.kwin.Scripting",
        "loadDeclarativeScript",
        &(path, SCRIPT_NAME),
    )
    .await?;
    if id < 0 {
        anyhow::bail!("KWin refused the game mode script");
    }
    let () = dbus_util::call(
        conn,
        KWIN,
        &format!("/Scripting/Script{id}"),
        "org.kde.kwin.Script",
        "run",
        &(),
    )
    .await?;
    Ok(())
}

fn action_id() -> Vec<&'static str> {
    vec![COMPONENT, ACTION, COMPONENT_FRIENDLY, ACTION_FRIENDLY]
}

type Keys = Vec<(Vec<i32>,)>;
/// KGlobalShortcutInfo: unique, friendly, componentUnique, componentFriendly, contextUnique,
/// contextFriendly, keys, defaultKeys.
type ShortcutInfo = (
    String,
    String,
    String,
    String,
    String,
    String,
    Vec<i32>,
    Vec<i32>,
);

/// Who else uses `code` (best effort, for the conflict report only).
async fn owners(conn: &Connection, code: i32) -> Vec<String> {
    let found: Result<Vec<ShortcutInfo>, _> = dbus_util::call(
        conn,
        ACCEL,
        ACCEL_PATH,
        ACCEL_IFACE,
        "globalShortcutsByKey",
        &((vec![code],), (0i32,)),
    )
    .await;
    found
        .map(|list| {
            list.into_iter()
                .filter(|s| s.2 != COMPONENT)
                .map(|s| format!("{} / {}", s.3, s.1))
                .collect()
        })
        .unwrap_or_default()
}

/// Registers `text` for the island action without ever taking another component's key.
async fn apply_shortcut(conn: &Connection, text: &str) -> ShortcutStatus {
    let code = match keyseq::parse(text) {
        Ok(Some(code)) => code,
        Ok(None) => {
            let result: Result<bool, _> = dbus_util::call(
                conn,
                ACCEL,
                ACCEL_PATH,
                ACCEL_IFACE,
                "unregister",
                &(COMPONENT, ACTION),
            )
            .await;
            return match result {
                Ok(_) => ShortcutStatus::Unbound,
                Err(e) => ShortcutStatus::Failed(e.to_string()),
            };
        }
        Err(e) => return ShortcutStatus::Failed(format!("{text}: {e}")),
    };
    let id = action_id();
    let run = async {
        let () =
            dbus_util::call(conn, ACCEL, ACCEL_PATH, ACCEL_IFACE, "doRegister", &(&id,)).await?;
        let current: Keys = dbus_util::call(
            conn,
            ACCEL,
            ACCEL_PATH,
            ACCEL_IFACE,
            "shortcutKeys",
            &(&id,),
        )
        .await?;
        let ours = current.iter().any(|k| k.0.first() == Some(&code));
        let free: bool = ours
            || dbus_util::call(
                conn,
                ACCEL,
                ACCEL_PATH,
                ACCEL_IFACE,
                "globalShortcutAvailable",
                &((vec![code],), COMPONENT),
            )
            .await?;
        if !free {
            // Keep the action registered but keyless (autoloading keeps a binding the user
            // made for it in System Settings, if any).
            let _: Keys = dbus_util::call(
                conn,
                ACCEL,
                ACCEL_PATH,
                ACCEL_IFACE,
                "setShortcutKeys",
                &(&id, Keys::new(), SET_PRESENT),
            )
            .await?;
            return Ok::<_, crate::error::IslandError>(false);
        }
        let set: Keys = dbus_util::call(
            conn,
            ACCEL,
            ACCEL_PATH,
            ACCEL_IFACE,
            "setShortcutKeys",
            &(&id, vec![(vec![code],)], SET_PRESENT | NO_AUTOLOADING),
        )
        .await?;
        Ok(set.iter().any(|k| k.0.first() == Some(&code)))
    };
    match run.await {
        Ok(true) => ShortcutStatus::Bound(text.to_owned()),
        Ok(false) => ShortcutStatus::Conflict {
            key: text.to_owned(),
            owners: owners(conn, code).await,
        },
        Err(e) => ShortcutStatus::Failed(e.to_string()),
    }
}

fn owner_rule(name: &str) -> String {
    format!("type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{name}'")
}

// ---------------------------------------------------------------------------------------------
// The worker.

pub struct GameModeHost {
    stop: watch::Sender<bool>,
    status: watch::Receiver<Status>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl GameModeHost {
    /// `address` = `None` for the session bus. `script_dir` must be private (0700).
    pub async fn start(
        address: Option<&str>,
        script_dir: PathBuf,
        settings: watch::Receiver<Desired>,
        sink: Arc<dyn Fn(Event) + Send + Sync>,
    ) -> Result<Self> {
        let builder = match address {
            Some(a) => zbus::connection::Builder::address(a),
            None => zbus::connection::Builder::session(),
        };
        let conn = tokio::time::timeout(
            Duration::from_secs(3),
            builder
                .name(BUS_NAME)
                .serve_at(OBJECT_PATH, Endpoint { sink: sink.clone() })
                .build(),
        )
        .await
        .context("Game mode D-Bus start timed out")??;
        // Subscribe before reading owners so no change in between is missed.
        let kwin_owner = dbus_util::subscribe(&conn, &owner_rule(KWIN)).await?;
        let accel_owner = dbus_util::subscribe(&conn, &owner_rule(ACCEL)).await?;
        let pressed = dbus_util::subscribe(
            &conn,
            &format!("type='signal',interface='{COMPONENT_IFACE}',member='globalShortcutPressed',arg0='{COMPONENT}'"),
        )
        .await?;
        let (stop, stopped) = watch::channel(false);
        let (status_tx, status) = watch::channel(Status {
            script_loaded: false,
            shortcut: ShortcutStatus::Pending,
        });
        let worker = Worker {
            conn,
            dir: script_dir,
            sink,
            status: status_tx,
            accel: None,
            warned_script: false,
            last_shortcut_log: None,
        };
        let task = tokio::spawn(worker.run(settings, stopped, kwin_owner, accel_owner, pressed));
        Ok(Self {
            stop,
            status,
            task: Some(task),
        })
    }

    pub fn status(&self) -> watch::Receiver<Status> {
        self.status.clone()
    }

    /// Stops the worker, unloads the script and deactivates the shortcut (bounded).
    pub async fn shutdown(mut self) {
        self.stop.send_replace(true);
        if let Some(task) = self.task.take() {
            let _ = tokio::time::timeout(Duration::from_secs(3), task).await;
        }
    }
}

impl Drop for GameModeHost {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

struct Worker {
    conn: Connection,
    dir: PathBuf,
    sink: Arc<dyn Fn(Event) + Send + Sync>,
    status: watch::Sender<Status>,
    /// Unique name of the current KGlobalAccel owner (presses from anyone else are ignored).
    accel: Option<String>,
    warned_script: bool,
    last_shortcut_log: Option<ShortcutStatus>,
}

/// The new owner from a bus-driver `NameOwnerChanged` (`""` = the name was released).
fn owner_change(msg: &zbus::Message) -> Option<String> {
    if msg.header().sender().map(|s| s.as_str()) != Some(dbus_util::DBUS) {
        return None;
    }
    let (_name, _old, new): (String, String, String) = msg.body().deserialize().ok()?;
    Some(new)
}

impl Worker {
    async fn run(
        mut self,
        mut settings: watch::Receiver<Desired>,
        mut stopped: watch::Receiver<bool>,
        mut kwin_owner: zbus::MessageStream,
        mut accel_owner: zbus::MessageStream,
        mut pressed: zbus::MessageStream,
    ) {
        let mut plan = Plan::new(settings.borrow_and_update().clone());
        let kwin = dbus_util::name_owner(&self.conn, KWIN).await.ok().flatten();
        self.accel = dbus_util::name_owner(&self.conn, ACCEL)
            .await
            .ok()
            .flatten();
        let mut todo = plan.kwin(kwin);
        todo.extend(plan.accel(self.accel.clone()));
        loop {
            self.perform(&mut plan, std::mem::take(&mut todo)).await;
            tokio::select! {
                changed = stopped.changed() => if changed.is_err() || *stopped.borrow() { break },
                changed = settings.changed() => {
                    if changed.is_err() { break; }
                    let want = settings.borrow_and_update().clone();
                    todo = plan.settings(want);
                }
                msg = dbus_util::next(&mut kwin_owner) => match msg {
                    Some(Ok(msg)) => if let Some(new) = owner_change(&msg) {
                        todo = plan.kwin((!new.is_empty()).then_some(new));
                    },
                    _ => break,
                },
                msg = dbus_util::next(&mut accel_owner) => match msg {
                    Some(Ok(msg)) => if let Some(new) = owner_change(&msg) {
                        self.accel = (!new.is_empty()).then_some(new);
                        todo = plan.accel(self.accel.clone());
                    },
                    _ => break,
                },
                msg = dbus_util::next(&mut pressed) => match msg {
                    Some(Ok(msg)) => self.on_pressed(&msg),
                    _ => break,
                },
            }
        }
        // Best-effort cleanup; both calls are bounded and harmless when the peer is gone.
        if plan.script_loaded() {
            let _ = unload_script(&self.conn).await;
        }
        if self.accel.is_some() && !plan.want.shortcut.is_empty() {
            let _: Result<(), _> = dbus_util::call(
                &self.conn,
                ACCEL,
                ACCEL_PATH,
                ACCEL_IFACE,
                "setInactive",
                &(action_id(),),
            )
            .await;
        }
    }

    fn on_pressed(&self, msg: &zbus::Message) {
        let header = msg.header();
        let from_accel =
            matches!((header.sender(), &self.accel), (Some(s), Some(owner)) if s.as_str() == owner);
        if !from_accel || msg.body().len() > dbus_util::MAX_SIGNAL_BYTES {
            return;
        }
        if let Ok((component, action, _ts)) = msg.body().deserialize::<(String, String, i64)>() {
            if component == COMPONENT && action == ACTION {
                (self.sink)(Event::Toggle);
            }
        }
    }

    async fn perform(&mut self, plan: &mut Plan, mut todo: Vec<Action>) {
        while !todo.is_empty() {
            for action in std::mem::take(&mut todo) {
                match action {
                    Action::ClearFullscreen => (self.sink)(Event::Fullscreen(false)),
                    Action::UnloadScript => {
                        if let Err(error) = unload_script(&self.conn).await {
                            tracing::debug!(%error, "Game mode script unload failed");
                        }
                    }
                    Action::LoadScript => {
                        let result = load_script(&self.conn, &self.dir).await;
                        match &result {
                            Ok(()) => {
                                self.warned_script = false;
                                tracing::info!("Game mode fullscreen detection on");
                            }
                            Err(error) if !self.warned_script => {
                                self.warned_script = true;
                                tracing::warn!(%error, "Game mode fullscreen detection unavailable; manual game mode only");
                            }
                            Err(_) => {}
                        }
                        todo.extend(plan.script_done(result.is_ok()));
                    }
                    Action::ApplyShortcut(text) => {
                        let status = apply_shortcut(&self.conn, &text).await;
                        if self.last_shortcut_log.as_ref() != Some(&status) {
                            match &status {
                                ShortcutStatus::Bound(key) => {
                                    tracing::info!(%key, "Island toggle shortcut registered")
                                }
                                ShortcutStatus::Unbound => {
                                    tracing::info!("Island toggle shortcut off")
                                }
                                ShortcutStatus::Conflict { key, owners } => tracing::warn!(
                                    %key, ?owners,
                                    "Island toggle shortcut is already used elsewhere; left unbound"
                                ),
                                ShortcutStatus::Failed(error) => {
                                    tracing::warn!(%error, "Island toggle shortcut unavailable")
                                }
                                ShortcutStatus::Pending => {}
                            }
                            self.last_shortcut_log = Some(status.clone());
                        }
                        self.status.send_modify(|s| s.shortcut = status);
                    }
                }
            }
        }
        let loaded = plan.script_loaded();
        self.status.send_if_modified(|s| {
            let changed = s.script_loaded != loaded;
            s.script_loaded = loaded;
            changed
        });
    }
}

/// Managed Tauri state holding the live worker (absent in test instances / when unavailable).
pub struct GameModeSlot(pub std::sync::Mutex<Option<GameModeHost>>);

/// Starts the worker for the product island: never in a test instance or a non-overlay run,
/// so tests never load KWin scripts or register real global shortcuts.
pub fn start_for_app(app: &tauri::AppHandle) {
    use crate::{services::Services, state::host::IslandActor, state::machine::Intent};
    use tauri::Manager;
    let services = app.state::<Services>();
    if services.runtime.test_instance || !services.overlay {
        return;
    }
    let initial = match services.settings.get() {
        Ok(settings) => Desired::from_settings(&settings),
        Err(error) => {
            tracing::warn!(%error, "Game mode unavailable: settings unreadable");
            return;
        }
    };
    let (tx, rx) = watch::channel(initial);
    let mut topic = services.settings_topic.subscribe();
    tauri::async_runtime::spawn(async move {
        while topic.changed().await.is_ok() {
            let Some(settings) = topic.borrow_and_update().clone() else {
                continue;
            };
            let next = Desired::from_settings(&settings);
            tx.send_if_modified(|d| {
                let changed = *d != next;
                *d = next;
                changed
            });
        }
    });
    let actor = app.state::<IslandActor>().handle.clone();
    let sink: Arc<dyn Fn(Event) + Send + Sync> = Arc::new(move |event| {
        let intent = match event {
            Event::Fullscreen(on) => Intent::Fullscreen(on),
            // Exactly what a click on the notch dispatches.
            Event::Toggle => Intent::PanelPin,
        };
        if let Err(error) = actor.try_dispatch(intent) {
            tracing::warn!(%error, "State actor refused a game mode event");
        }
    });
    let host = runtime_script_dir()
        .and_then(|dir| tauri::async_runtime::block_on(GameModeHost::start(None, dir, rx, sink)));
    match host {
        Ok(host) => {
            app.manage(GameModeSlot(std::sync::Mutex::new(Some(host))));
        }
        Err(error) => {
            tracing::warn!(%error, "Game mode detection and the toggle shortcut are unavailable")
        }
    }
}

/// Bounded cleanup at exit: unloads the KWin script and deactivates the shortcut.
pub fn shutdown_for_app(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(slot) = app.try_state::<GameModeSlot>() else {
        return;
    };
    let host = slot.0.lock().ok().and_then(|mut h| h.take());
    if let Some(host) = host {
        tauri::async_runtime::block_on(host.shutdown());
    }
}

/// `$XDG_RUNTIME_DIR/monolith`, the private directory KWin reads the script from.
pub fn runtime_script_dir() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;
    Ok(PathBuf::from(base).join("monolith"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KW: &str = ":1.1";
    const AC: &str = ":1.2";
    fn up(owner: &str) -> Option<String> {
        Some(owner.to_owned())
    }
    fn want(auto: bool, key: &str) -> Desired {
        Desired {
            auto,
            shortcut: key.into(),
        }
    }

    #[test]
    fn script_loads_once_per_kwin_and_a_failure_never_retries_by_itself() {
        let mut p = Plan::new(want(true, ""));
        // KWin not running: nothing to load, the fullscreen flag is cleared.
        assert_eq!(p.kwin(None), vec![Action::ClearFullscreen]);
        assert_eq!(p.kwin(up(KW)), vec![Action::LoadScript]);
        // Any other input while the load is in flight does not request a second load.
        assert_eq!(p.accel(up(AC)), vec![Action::ApplyShortcut(String::new())]);
        // Failure: cleared, logged by the worker, and no automatic retry.
        assert_eq!(p.script_done(false), vec![Action::ClearFullscreen]);
        assert_eq!(p.settings(want(true, "")), vec![]);
        assert_eq!(p.accel(None), vec![]);
        assert_eq!(
            p.script_done(false),
            vec![],
            "a stale result changes nothing"
        );
        // KWin crashes and comes back: exactly one new attempt.
        assert_eq!(p.kwin(None), vec![Action::ClearFullscreen]);
        assert_eq!(p.kwin(up(KW)), vec![Action::LoadScript]);
        assert_eq!(p.script_done(true), vec![]);
        assert!(p.script_loaded());
        assert_eq!(p.settings(want(true, "")), vec![]);
        // The startup owner query racing the owner signal is not a restart.
        assert_eq!(p.kwin(up(KW)), vec![]);
        // KWin restart while loaded: flag cleared, then one reload.
        assert_eq!(
            p.kwin(up(":1.9")),
            vec![Action::ClearFullscreen, Action::LoadScript]
        );
        p.script_done(true);
        // Turning auto off unloads and clears; on again reloads.
        assert_eq!(
            p.settings(want(false, "")),
            vec![Action::UnloadScript, Action::ClearFullscreen]
        );
        assert_eq!(p.settings(want(true, "")), vec![Action::LoadScript]);
        // A failed load is retried when the user toggles the setting.
        p.script_done(false);
        assert_eq!(p.settings(want(false, "")), vec![]);
        assert_eq!(p.settings(want(true, "")), vec![Action::LoadScript]);
        // Switched off while the load is in flight: unloaded as soon as it lands.
        p.settings(want(false, ""));
        assert_eq!(
            p.script_done(true),
            vec![Action::UnloadScript, Action::ClearFullscreen]
        );
        assert!(!p.script_loaded());
    }

    #[test]
    fn shortcut_is_applied_per_change_and_per_kglobalaccel_incarnation() {
        let mut p = Plan::new(want(false, "Meta+`"));
        assert_eq!(p.accel(None), vec![]);
        assert_eq!(
            p.accel(up(AC)),
            vec![Action::ApplyShortcut("Meta+`".into())]
        );
        assert_eq!(p.accel(up(AC)), vec![], "same owner: nothing to redo");
        assert_eq!(p.settings(want(false, "Meta+`")), vec![]);
        assert_eq!(
            p.settings(want(false, "Ctrl+Alt+H")),
            vec![Action::ApplyShortcut("Ctrl+Alt+H".into())]
        );
        assert_eq!(
            p.settings(want(false, "")),
            vec![Action::ApplyShortcut(String::new())]
        );
        // Not re-applied while KGlobalAccel is gone; re-applied once it is back.
        p.accel(None);
        assert_eq!(p.settings(want(false, "F9")), vec![]);
        assert_eq!(p.accel(up(AC)), vec![Action::ApplyShortcut("F9".into())]);
    }

    #[test]
    fn script_is_event_driven_ignores_the_island_and_names_this_service() {
        assert!(SCRIPT.contains("onWindowActivated"));
        assert!(SCRIPT.contains("onFullScreenChanged"));
        assert!(!SCRIPT.contains("Timer"), "no timers, no polling");
        assert!(
            SCRIPT.contains("onFailed"),
            "backs off while the island is gone"
        );
        for ignored in ["hermes-island", "w.dock", "onScreenDisplay", "notification"] {
            assert!(SCRIPT.contains(ignored), "must ignore {ignored}");
        }
        for name in [BUS_NAME, OBJECT_PATH, INTERFACE, "\"Fullscreen\""] {
            assert!(SCRIPT.contains(name), "script must call {name}");
        }
    }
}
