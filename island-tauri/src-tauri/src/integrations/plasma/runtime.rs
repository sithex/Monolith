//! Native ownership and legacy-compatible panel/status lifecycle (SPEC §7, §8 Phase 3).
//!
//! `enter|leave|pin` on `panel.sock` are routed straight into the state actor (not through
//! `AnchorSignal`, which is still emitted for compatibility but no longer consumed by the
//! renderer). `status.json` / the `status` reply report the ACTOR's mode (legacy names) and
//! view; the file is refreshed on every actor publication and region change, plus a 2 s
//! heartbeat, never faster than every `MIN_WRITE_INTERVAL`.
use super::{
    anchor::Anchor,
    panel_socket::{PanelCommand, PanelSocket},
};
use crate::{
    contract::{
        events::{AnchorKind, AnchorSignal},
        island::IslandMode,
    },
    services::Services,
    singleton::InstanceGuard,
    state::{host::IslandActor, machine::Intent},
    window::{IslandRuntime, ISLAND_LABEL},
};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
use tauri_specta::Event;

const HEARTBEAT: Duration = Duration::from_secs(2);
/// An unchanged status is rewritten only this often: just enough to keep `updated` fresh for
/// readers that treat a status older than 8 s as offline (`anchor-client.py online()`).
const STATUS_REFRESH: Duration = Duration::from_millis(3500);
const MIN_WRITE_INTERVAL: Duration = Duration::from_millis(100);
/// Notch positions read from Plasma (kscreen-doctor plus a plasmashell script) are reused for
/// this long after a click. A click on a widget the cache doesn't know, or a "moved" report,
/// re-reads immediately, so this only bounds drift Plasma never announces.
const ANCHOR_TTL: Duration = Duration::from_secs(60);

fn anchors_stale(read: Option<Instant>) -> bool {
    read.is_none_or(|t| t.elapsed() >= ANCHOR_TTL)
}

/// `home_widget` value meaning "routing off: every notch click is a plain toggle".
const ROUTE_OFF: u32 = u32::MAX;
enum StatusMsg {
    Stop,
    Refresh,
    PinAt(u32),
    /// A notch's panel docked or floated: re-read positions once (event-driven, coalesced).
    Moved,
}
/// Managed state: lets other modules request a (coalesced) status refresh.
pub struct StatusNudge(mpsc::SyncSender<StatusMsg>);

/// Ask the status worker for a refresh (e.g. after the input region changed). A missing or
/// stopped worker is not an error: there is simply no status file to keep current.
pub fn nudge_status(app: &tauri::AppHandle) {
    if let Some(nudge) = app.try_state::<StatusNudge>() {
        if matches!(
            nudge.0.try_send(StatusMsg::Refresh),
            Err(mpsc::TrySendError::Disconnected(_))
        ) {
            tracing::debug!("Status worker already stopped");
        }
    }
}

pub struct NativeHost {
    instance: Arc<Mutex<InstanceGuard>>,
    panel: Mutex<Option<PanelSocket>>,
    stop: mpsc::SyncSender<StatusMsg>,
    worker: Mutex<Option<JoinHandle<()>>>,
    watcher: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// The notch companion's pointer relay (never in test instances).
    companion: Mutex<Option<super::companion::CompanionBus>>,
}
/// The one explicit mapping from canonical modes to the legacy names external tooling reads.
fn legacy_mode_name(mode: IslandMode) -> &'static str {
    match mode {
        IslandMode::Still => "idle",
        IslandMode::Hover => "preview",
        IslandMode::Large => "pinned",
    }
}
/// Whether the notch should hold completely still: game mode is on and Nick kept
/// "freeze the companion in game mode" enabled. Sent to the plasmoid as `companionFrozen`
/// (a derived boolean, so the QML never has to know about game-mode sources or settings).
fn companion_frozen(game_mode: bool, freeze_setting: bool) -> bool {
    game_mode && freeze_setting
}

/// Sample the services the collapsed notch reflects. Missing or disabled modules simply
/// contribute nothing; this never blocks on a worker.
fn live_state(app: &tauri::AppHandle) -> super::notch_live::NotchLive {
    use super::notch_live::{timer_live, NotchLive};
    let services = app.try_state::<Services>();
    // Waiting is sent only when something shows it, so a muted state costs no Plasma push.
    let shows_waiting = services
        .as_ref()
        .and_then(|s| s.settings.get().ok())
        .is_some_and(|s| s.notch_waiting_pulse || (s.companion_enabled && s.companion_reactive));
    let mut live = NotchLive {
        working: services.as_ref().is_some_and(|s| s.terminal.working()),
        waiting: shows_waiting && services.as_ref().is_some_and(|s| s.terminal.waiting()),
        ..Default::default()
    };
    if let Some(system) =
        app.try_state::<Arc<crate::integrations::system_services::SystemServices>>()
    {
        if let Some((t, age)) = system.apps.timer().ok().and_then(|t| t.latest()) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            live.timer = timer_live(
                &t.countdown_status,
                t.remaining_ms,
                &t.pomodoro_status,
                t.phase_remaining_ms,
                age,
                now,
            );
        }
        live.mic_muted = system
            .apps
            .toggles()
            .is_ok_and(|t| t.current().mic_muted == Some(true));
    }
    live
}

fn snapshot(
    app: &tauri::AppHandle,
    anchor: &Option<Anchor>,
    bounds: &Value,
    live: &super::notch_live::NotchLive,
) -> Result<Value> {
    use crate::window::input_region::InputRegionBackend;
    let services = app.state::<Services>();
    let region = app.state::<IslandRuntime>();
    let controller = region
        .region
        .lock()
        .map_err(|_| anyhow::anyhow!("Region lock poisoned"))?;
    let rectangles: Vec<_> = crate::window::shape::band_rows(&controller.backend().readback()?)
        .into_iter()
        .map(|r| json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height}))
        .collect();
    let xid = controller.backend().xid();
    drop(controller);
    let state = app.state::<IslandActor>().handle.state();
    let settings = services.settings.get()?;
    let skin = match services.palette.get() {
        Ok(palette) => Some(palette),
        Err(error) => {
            tracing::debug!(%error,"Panel palette unavailable");
            None
        }
    };
    // The island's colour scheme (when one is chosen) recolours the notch too.
    let scheme = services.scheme_palette.lock().ok().and_then(|s| s.clone());
    let palette = scheme.or(skin);
    let color = |value: &str, fallback: &str| {
        if value.is_empty() {
            fallback.to_owned()
        } else {
            value.to_owned()
        }
    };
    let background = palette.as_ref().map(|palette| {
        color(
            &settings.panel_clock_color,
            &color(&settings.background, &palette.surface),
        )
    });
    let accent = palette.as_ref().map(|p| color(&settings.accent, &p.accent));
    let border_color = color(
        &settings.notch_border_color,
        accent.as_deref().unwrap_or("#f2cc60"),
    );
    Ok(
        // `view` and `stateRev` are additive fields (legacy readers ignore unknown keys).
        json!({"online":true,"pid":std::process::id(),"updated":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64(),
        "mode":legacy_mode_name(state.mode),"view":serde_json::to_value(state.view)?,"stateRev":state.rev,
        "anchor":anchor,"bounds":bounds,"windowId":xid,"rectangles":rectangles,"pty":services.terminal.pid().unwrap_or(0),
        "showDate":settings.show_date,"stealth":settings.stealth,"accent":accent,"notchWidth":settings.notch_width,
        "gameMode":state.game_mode,
        "companionFrozen":companion_frozen(state.game_mode, settings.game_mode_freeze_companion),
        "companion":settings.companion_enabled,
        "companionCharacter":settings.companion_character,
        "companionReactive":settings.companion_reactive,"companionMonitor":settings.companion_monitor,
        "waitingPulse":settings.notch_waiting_pulse,"hoverPeek":settings.notch_hover_peek,"hoverDelay":settings.hover_delay,
        "notchBorder":{"enabled":settings.notch_border_enabled,"color":border_color,"opacity":settings.notch_border_opacity,"width":settings.notch_border_width},
        "background":background,"text":palette.as_ref().map(|p|&p.text),"dim":palette.as_ref().map(|p|&p.dim),
        "live":live.to_json()}),
    )
}
/// Source widget ids are resolved only against fresh, connected native anchors.
fn select_monitor_anchor(
    anchors: &[Anchor],
    primary: Option<&Anchor>,
    current: Option<&Anchor>,
    clicked: Option<u32>,
    enabled: bool,
) -> Result<Option<Anchor>> {
    if let Some(widget) = clicked {
        let target = anchors
            .iter()
            .find(|a| a.widget == widget)
            .context("Clicked notch is disconnected or unavailable")?;
        if !enabled && primary.is_none_or(|a| a.output != target.output) {
            anyhow::bail!("Notch on every monitor is disabled");
        }
        return Ok(Some(target.clone()));
    }
    if enabled {
        if let Some(found) = current.and_then(|c| anchors.iter().find(|a| a.output == c.output)) {
            return Ok(Some(found.clone()));
        }
    }
    Ok(primary.filter(|a| a.native).cloned().or_else(|| {
        if enabled {
            anchors.first().cloned()
        } else {
            None
        }
    }))
}

fn relocate(app: &tauri::AppHandle, anchor: &Anchor) -> Result<Value> {
    use crate::window::{placement::over_panel, ISLAND_OVERLAY_HEIGHT, ISLAND_OVERLAY_WIDTH};
    let target = over_panel(anchor, ISLAND_OVERLAY_WIDTH, ISLAND_OVERLAY_HEIGHT)?;
    let runtime = app.state::<IslandRuntime>();
    let pill = crate::contract::island::RoundedRect {
        x: anchor.pill.x - f64::from(target.x),
        y: anchor.pill.y - f64::from(target.y),
        width: anchor.pill.width,
        height: anchor.pill.height,
        radius: anchor.pill_radius,
    };
    {
        let mut controller = runtime
            .region
            .lock()
            .map_err(|_| anyhow::anyhow!("Region poisoned"))?;
        controller.backend_mut().reposition(target)?;
        controller.relocated(pill)?;
    }
    *runtime
        .placement
        .lock()
        .map_err(|_| anyhow::anyhow!("Placement poisoned"))? = (
        f64::from(target.x),
        f64::from(target.y),
        f64::from(target.width),
        f64::from(target.height),
    );
    let window = app
        .get_webview_window(ISLAND_LABEL)
        .context("Missing island window")?;
    let values =
        json!({"x":pill.x,"y":pill.y,"width":pill.width,"height":pill.height,"radius":pill.radius});
    window.eval(format!("(()=>{{const v={values};for(const [k,x] of Object.entries(v))document.documentElement.style.setProperty('--native-pill-'+k,x+'px');window.dispatchEvent(new Event('island-native-pill'));window.dispatchEvent(new Event('resize'));window.dispatchEvent(new Event('island-surfaces-changed'));}})()"))?;
    Ok(json!({"x":target.x,"y":target.y,"width":target.width,"height":target.height}))
}

/// Logical origins of the outputs that may get an added notch (`None` = every monitor).
/// Plasma's script API knows screens only by index, so they are matched by origin, the same
/// way `anchor` pairs panels with outputs. A listed name that isn't connected matches nothing.
fn notch_screen_origins(outputs: &Value, names: &[String]) -> Option<Vec<[f64; 2]>> {
    if names.is_empty() {
        return None;
    }
    Some(
        outputs["outputs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|o| o["connected"] == true && o["enabled"] != false)
            .filter(|o| {
                o["name"]
                    .as_str()
                    .is_some_and(|n| names.iter().any(|w| w == n))
            })
            .filter_map(|o| Some([o["pos"]["x"].as_f64()?, o["pos"]["y"].as_f64()?]))
            .collect(),
    )
}

fn monitor_script(enabled: bool, style: &Value, origins: Option<&[[f64; 2]]>) -> String {
    include_str!("monitor-notches.js")
        .replace("__ENABLED__", if enabled { "true" } else { "false" })
        .replace("__STYLE__", &style.to_string())
        .replace("__ORIGINS__", &json!(origins).to_string())
}

fn panel_style_payload(raw: &Value) -> Value {
    let keys = [
        "background",
        "accent",
        "text",
        "dim",
        "showDate",
        "stealth",
        "notchBorder",
        "notchWidth",
        "companion",
        "companionCharacter",
        "companionReactive",
        "companionFrozen",
        "companionMonitor",
        "waitingPulse",
        "hoverPeek",
        "hoverDelay",
        "live",
    ];
    Value::Object(
        keys.into_iter()
            .filter_map(|key| raw.get(key).map(|v| (key.to_owned(), v.clone())))
            .collect(),
    )
}
fn panel_style_script(targets: &[(u32, u32)], style: &Value) -> String {
    let encoded = json!(style.to_string());
    let targets = json!(targets.iter().map(|(p, w)| [*p, *w]).collect::<Vec<_>>());
    format!("var t={targets};var n=0;panels().forEach(function(p){{p.widgets().forEach(function(w){{if(!t.some(function(x){{return x[0]===p.id&&x[1]===w.id}}))return;if(['local.hermes.island.notch3','local.hermes.island.notch2','local.hermes.island.notch'].indexOf(w.type)<0)return;w.currentConfigGroup=['General'];w.writeConfig('notchAppearance',{encoded});w.reloadConfig();n++;}})}});print('notch-style-applied '+n);")
}

#[cfg(test)]
mod notch_tests {
    use super::*;
    #[test]
    fn notch_panel_payload_is_public_only_and_honors_zero_opacity() {
        let raw = json!({"background":"#123456","accent":"#abcdef","showDate":true,
            "notchBorder":{"enabled":true,"color":"#000000","opacity":0,"width":4},
            "pid":99,"pty":42,"private":"must not reach Plasma"});
        let style = panel_style_payload(&raw);
        assert_eq!(style["notchBorder"]["opacity"], json!(0));
        assert_eq!(style["background"], json!("#123456"));
        assert!(style.get("private").is_none());
        assert!(style.get("pid").is_none());
        let script = panel_style_script(&[(145, 407), (97, 500)], &style);
        assert!(script.contains("[[145,407],[97,500]]"));
        assert!(script.contains("notchAppearance"));
        assert!(!script.contains("private"));
    }
    #[test]
    fn companion_freezes_only_in_game_mode_with_the_setting_on() {
        assert!(companion_frozen(true, true));
        assert!(
            !companion_frozen(true, false),
            "setting off: game mode alone does not freeze"
        );
        assert!(!companion_frozen(false, true), "no game mode, no freeze");
        assert!(!companion_frozen(false, false));
        let raw = json!({"companionFrozen":true,"gameMode":true,"pid":1});
        let style = panel_style_payload(&raw);
        assert_eq!(style["companionFrozen"], json!(true));
        assert!(
            style.get("gameMode").is_none(),
            "only the derived flag reaches Plasma"
        );
    }
    #[test]
    fn monitor_script_never_creates_panels_and_reports_placement() {
        let script = include_str!("monitor-notches.js");
        assert!(!script.contains("new Panel"));
        assert!(!script.contains("p.remove()"));
        assert!(script.contains("report.notches.push"));
        assert!(script.contains("__ENABLED__") && script.contains("__STYLE__"));
        assert!(script.contains("__ORIGINS__"));
        let filled = monitor_script(true, &json!({"a":1}), Some(&[[0.0, 0.0], [-1920.0, 0.0]]));
        assert!(filled.contains("(true, {\"a\":1}, [[0.0,0.0],[-1920.0,0.0]])"));
        assert!(monitor_script(true, &json!({}), None).contains(", null)"));
    }
    #[test]
    fn notch_monitor_filter_matches_connected_outputs_by_name() {
        let outputs = json!({"outputs":[
            {"name":"DP-1","connected":true,"enabled":true,"pos":{"x":0,"y":0}},
            {"name":"DP-2","connected":true,"enabled":true,"pos":{"x":-1920,"y":120}},
            {"name":"HDMI-A-1","connected":true,"enabled":false,"pos":{"x":2560,"y":0}},
            {"name":"DP-3","connected":false,"pos":{"x":9000,"y":0}}]});
        assert_eq!(
            notch_screen_origins(&outputs, &[]),
            None,
            "empty = every monitor"
        );
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            notch_screen_origins(&outputs, &names(&["DP-2"])),
            Some(vec![[-1920.0, 120.0]])
        );
        // Disabled, unplugged and unknown outputs match nothing (no screen qualifies).
        assert_eq!(
            notch_screen_origins(&outputs, &names(&["HDMI-A-1", "DP-3", "nope"])),
            Some(vec![])
        );
    }
}

impl NativeHost {
    pub fn start(
        app: &tauri::AppHandle,
        mut instance: InstanceGuard,
        anchor: Option<Anchor>,
    ) -> Result<Self> {
        let window = app
            .get_webview_window(ISLAND_LABEL)
            .context("Missing island window")?;
        let runtime = app.state::<IslandRuntime>();
        let mut bounds = if runtime.wayland {
            // Wayland never reports a surface's global position: publish the placement used.
            let (x, y, width, height) = *runtime
                .placement
                .lock()
                .map_err(|_| anyhow::anyhow!("Placement poisoned"))?;
            json!({"x":x.round() as i64,"y":y.round() as i64,"width":width.round() as i64,"height":height.round() as i64})
        } else {
            let position = window.outer_position()?;
            let size = window.inner_size()?;
            json!({"x":position.x,"y":position.y,"width":size.width,"height":size.height})
        };
        let actor = app.state::<IslandActor>().handle.clone();
        let mut last_live = live_state(app);
        let initial = snapshot(app, &anchor, &bounds, &last_live)?;
        instance.update_status(&initial)?;
        let cached = Arc::new(Mutex::new(initial));
        let state = cached.clone();
        let handle = app.clone();
        let router = actor.clone();
        let (stop, receive) = mpsc::sync_channel(32);
        let clicks = stop.clone();
        // Widget id of the notch the island currently sits on, published by the status worker
        // (ROUTE_OFF while "notch on every monitor" is off). A click on THAT notch is a plain
        // toggle, so it is dispatched right here like the classic `pin`, never queued behind the
        // worker; only a click on another monitor's notch needs the router.
        let home_widget = Arc::new(AtomicU32::new(ROUTE_OFF));
        let home = home_widget.clone();
        let panel = PanelSocket::start(&mut instance, move |command| {
            let (kind, intent) = match command {
                PanelCommand::PinAt(widget) => {
                    let current = home.load(Ordering::Acquire);
                    if current == ROUTE_OFF || current == widget {
                        (Some(AnchorKind::Pin), Some(Intent::PanelPin))
                    } else {
                        clicks
                            .try_send(StatusMsg::PinAt(widget))
                            .map_err(|_| anyhow::anyhow!("Monitor router stopped"))?;
                        (Some(AnchorKind::Pin), None)
                    }
                }
                PanelCommand::Status => (None, None),
                PanelCommand::Moved => {
                    // Best effort: a full queue already has a pending refresh to piggyback on.
                    let _ = clicks.try_send(StatusMsg::Moved);
                    (None, None)
                }
                PanelCommand::Enter => (Some(AnchorKind::Enter), Some(Intent::PanelEnter)),
                PanelCommand::Leave => (Some(AnchorKind::Leave), Some(Intent::PanelLeave)),
                PanelCommand::Pin => (Some(AnchorKind::Pin), Some(Intent::PanelPin)),
            };
            // The actor is the authority: enqueue without waiting (this thread serves other
            // panel clients too). A full queue refuses the request instead of dropping it.
            if let Some(intent) = intent {
                router
                    .try_dispatch(intent)
                    .map_err(|e| anyhow::anyhow!("State actor refused panel command: {e}"))?;
            }
            // Compatibility event, no longer consumed by the renderer (SPEC §4.5).
            if let Some(kind) = kind {
                AnchorSignal(kind).emit_to(&handle, ISLAND_LABEL)?;
            }
            let mut response = state
                .lock()
                .map_err(|_| anyhow::anyhow!("Panel cache poisoned"))?
                .clone();
            response["mode"] = json!(legacy_mode_name(router.state().mode));
            if kind.is_some() {
                response["ok"] = json!(true);
            }
            Ok(response)
        })?;
        let instance = Arc::new(Mutex::new(instance));
        let guard = instance.clone();
        let handle = app.clone();
        app.manage(StatusNudge(stop.clone()));
        let allow_panel_push = !app.state::<Services>().runtime.test_instance;
        // The companion's pointer relay. Optional: without it the eyes simply rest ahead.
        let companion = if allow_panel_push {
            match tauri::async_runtime::block_on(super::companion::CompanionBus::start()) {
                Ok(bus) => Some(bus),
                Err(error) => {
                    tracing::warn!(%error, "Notch companion pointer relay unavailable");
                    None
                }
            }
        } else {
            None
        };
        let relay = companion.is_some();
        let worker = std::thread::Builder::new()
            .name("ripple-status".into())
            .spawn(move || {
                let mut last_write = Instant::now();
                let mut last_panel_style = Value::Null;
                let mut anchor = anchor;
                // Reconciled when "every monitor" or its monitor list changes (or notch width).
                let mut last_monitors: Option<(bool, Vec<String>)> = None;
                let mut last_companion: Option<bool> = None;
                let mut anchors = Vec::<Anchor>::new();
                let mut anchors_read: Option<Instant> = None;
                let mut primary: Option<Anchor> = None;
                let mut monitor_report = Value::Null;
                let mut monitor_error: Option<String> = None;
                let mut unavailable = Vec::<String>::new();
                let mut pending = std::collections::VecDeque::new();
                // Last written status without its `updated` stamp, and when it was written.
                let mut last_body = Value::Null;
                let mut last_status_write: Option<Instant> = None;
                let mut last_error: Option<String> = None;
                'worker: loop {
                    let message = pending.pop_front().map(Ok).unwrap_or_else(|| receive.recv_timeout(HEARTBEAT));
                    let mut moved = matches!(message, Ok(StatusMsg::Moved));
                    let clicked = match message {
                        Ok(StatusMsg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Ok(StatusMsg::PinAt(widget)) => Some(widget),
                        Ok(StatusMsg::Refresh | StatusMsg::Moved) | Err(mpsc::RecvTimeoutError::Timeout) => None,
                    };
                    while let Ok(message) = receive.try_recv() {
                        match message {
                            StatusMsg::Stop => break 'worker,
                            StatusMsg::PinAt(_) => pending.push_back(message),
                            StatusMsg::Moved => moved = true,
                            StatusMsg::Refresh => {},
                        }
                    }
                    // Never coalesce away a click (including a rapid second-monitor click).
                    std::thread::sleep(MIN_WRITE_INTERVAL.saturating_sub(last_write.elapsed()));
                    let update = (|| -> Result<()> {
                        let settings = handle.state::<Services>().settings.get()?;
                        let enabled = settings.show_notch_on_every_monitor;
                        // The KWin pointer script runs only while the companion is on: loaded at
                        // startup and when the setting flips, never retried in a loop.
                        if relay && last_companion != Some(settings.companion_enabled) {
                            last_companion = Some(settings.companion_enabled);
                            let result = if settings.companion_enabled {
                                super::companion::load_script()
                            } else {
                                super::companion::unload_script()
                            };
                            if let Err(error) = result {
                                tracing::warn!(%error, "Notch companion pointer script unavailable");
                            }
                        }
                        let live = live_state(&handle).stabilised(Some(&last_live));
                        // Plasma is reconciled only at startup and when the setting, its monitor
                        // list (or notch width) changes: never periodically, never on a click,
                        // never retried.
                        let monitors = (enabled, settings.notch_monitors.clone());
                        if allow_panel_push && last_monitors.as_ref() != Some(&monitors) {
                            last_monitors = Some(monitors);
                            let result = (|| -> Result<()> {
                                let style = panel_style_payload(&snapshot(&handle, &anchor, &bounds, &live)?);
                                // Only a chosen subset needs the output names (one kscreen read).
                                let origins = if enabled && !settings.notch_monitors.is_empty() {
                                    let [outputs, _] = super::anchor::discovery_commands();
                                    let out = crate::services::process::run_bounded(outputs, Duration::from_secs(3), 1024 * 1024, false)?;
                                    let outputs: Value = serde_json::from_slice(&out).context("Invalid outputs")?;
                                    notch_screen_origins(&outputs, &settings.notch_monitors)
                                } else {
                                    None
                                };
                                let script = monitor_script(enabled, &style, origins.as_deref());
                                let reconcile = || -> Result<Value> {
                                    let mut command = std::process::Command::new("/usr/bin/qdbus6");
                                    command.args(["org.kde.plasmashell", "/PlasmaShell", "org.kde.PlasmaShell.evaluateScript", &script]);
                                    let out = crate::services::process::run_bounded(command, Duration::from_secs(3), 65536, false)?;
                                    Ok(serde_json::from_slice(&out).unwrap_or(Value::Null))
                                };
                                monitor_report = reconcile()?;
                                // One fixed second pass (not a retry): it adds nothing when the first
                                // pass succeeded and only re-asserts the centre slot of new notches.
                                if monitor_report["added"].as_array().is_some_and(|a| !a.is_empty()) {
                                    let second = reconcile()?;
                                    monitor_report["ordered"] = second["ordered"].clone();
                                    monitor_report["notches"] = second["notches"].clone();
                                }
                                tracing::info!(report = %monitor_report, "Monitor notches reconciled");
                                primary = tauri::async_runtime::block_on(super::anchor::discover()).ok();
                                if enabled {
                                    let (found, missing) = super::anchor::discover_all()?;
                                    anchors = found;
                                    unavailable = missing;
                                    anchors_read = Some(Instant::now());
                                } else {
                                    anchors.clear();
                                    anchors_read = None;
                                    unavailable.clear();
                                    // Turning the option off brings the island home to the primary notch.
                                    if let Some(home) = primary.clone().filter(|p| anchor.as_ref().is_some_and(|a| a.output != p.output)) {
                                        bounds = relocate(&handle, &home)?;
                                        anchor = Some(home);
                                    }
                                }
                                Ok(())
                            })();
                            monitor_error = result.err().map(|e| e.to_string());
                            if let Some(error) = &monitor_error { tracing::warn!(%error, "Monitor notch routing unavailable"); }
                        }
                        // Clicks use the cached anchors (no Plasma or kscreen calls before the
                        // click is served). Only a widget the cache doesn't know (Plasma restarted,
                        // a monitor was plugged in) costs one re-read first.
                        let mut click_handled = false;
                        if clicked.is_some() && enabled {
                            let known = clicked.is_some_and(|w| anchors.iter().any(|a| a.widget == w));
                            if !known {
                                match super::anchor::discover_all() {
                                    Ok((found, missing)) => {
                                        anchors = found;
                                        unavailable = missing;
                                        anchors_read = Some(Instant::now());
                                    }
                                    Err(error) => tracing::debug!(%error, "Notch position refresh unavailable"),
                                }
                            }
                            match select_monitor_anchor(&anchors, primary.as_ref(), anchor.as_ref(), clicked, enabled) {
                                Ok(Some(target)) => {
                                    let changed_output = anchor.as_ref().is_none_or(|a| a.output != target.output);
                                    if anchor.as_ref() != Some(&target) {
                                        bounds = relocate(&handle, &target)?;
                                        anchor = Some(target);
                                    }
                                    // Same output toggles; another output opens/transfers, never collapses.
                                    let intent = if changed_output { Intent::Pin } else { Intent::PanelPin };
                                    tauri::async_runtime::block_on(handle.state::<IslandActor>().handle.dispatch(intent))?;
                                    click_handled = true;
                                    // After the click is served, re-read positions when the cache is
                                    // old: Plasma may have moved the notches without telling us. Rapid
                                    // clicks reuse the fresh read instead of queueing behind new ones.
                                    let refresh = if anchors_stale(anchors_read) { Some(super::anchor::discover_all()) } else { None };
                                    match refresh {
                                        None => {}
                                        Some(Ok((found, missing))) => {
                                            anchors = found;
                                            unavailable = missing;
                                            anchors_read = Some(Instant::now());
                                            let output = anchor.as_ref().map(|a| a.output.clone());
                                            if let Some(fresh) = anchors.iter().find(|a| Some(&a.output) == output.as_ref()).cloned() {
                                                if anchor.as_ref() != Some(&fresh) {
                                                    bounds = relocate(&handle, &fresh)?;
                                                    anchor = Some(fresh);
                                                }
                                            }
                                        }
                                        Some(Err(error)) => tracing::debug!(%error, "Notch position refresh unavailable"),
                                    }
                                }
                                Ok(None) => {}
                                Err(error) => tracing::debug!(%error, "Notch click falls back to the primary toggle"),
                            }
                        }
                        // The widget reported its panel docked/floated (Plasma shifts a floating panel's
                        // contents when a window is maximized): re-read once and follow the notch.
                        if moved && !click_handled {
                            // No anchor yet (startup raced a Plasma restart): fall back to the
                            // primary notch, exactly like a fresh start would.
                            let fresh = if enabled && anchor.is_some() {
                                match super::anchor::discover_all() {
                                    Ok((found, missing)) => {
                                        anchors = found;
                                        unavailable = missing;
                                        anchors_read = Some(Instant::now());
                                        let output = anchor.as_ref().map(|a| a.output.clone());
                                        anchors.iter().find(|a| Some(&a.output) == output.as_ref()).cloned()
                                    }
                                    Err(error) => { tracing::debug!(%error, "Notch move refresh unavailable"); None }
                                }
                            } else {
                                match tauri::async_runtime::block_on(super::anchor::discover()) {
                                    Ok(found) => { primary = Some(found.clone()); Some(found) }
                                    Err(error) => { tracing::debug!(%error, "Notch move refresh unavailable"); None }
                                }
                            };
                            if let Some(fresh) = fresh.filter(|f| anchor.as_ref().is_none_or(|a| a.output == f.output)) {
                                if anchor.as_ref() != Some(&fresh) {
                                    bounds = relocate(&handle, &fresh)?;
                                    anchor = Some(fresh);
                                }
                            }
                        }
                        // A click is never lost: anything unroutable falls back to the primary toggle.
                        if clicked.is_some() && !click_handled {
                            if let Err(error) = tauri::async_runtime::block_on(handle.state::<IslandActor>().handle.dispatch(Intent::PanelPin)) {
                                tracing::warn!(%error, "Fallback notch click failed");
                            }
                        }
                        home_widget.store(
                            match anchor.as_ref() {
                                Some(a) if enabled && a.native => a.widget,
                                Some(_) if enabled => 0,
                                _ => ROUTE_OFF,
                            },
                            Ordering::Release,
                        );
                        let mut next = snapshot(&handle, &anchor, &bounds, &live)?;
                        last_live = live;
                        next["monitorNotches"] = monitor_report.clone();
                        next["monitorNotchError"] = json!(monitor_error);
                        next["notchUnavailableOutputs"] = json!(unavailable);
                        let mut body = next.clone();
                        if let Some(fields) = body.as_object_mut() {
                            fields.remove("updated");
                        }
                        // Write when something changed, otherwise only as a liveness heartbeat.
                        if body != last_body || last_status_write.is_none_or(|t| t.elapsed() >= STATUS_REFRESH) {
                            guard
                                .lock()
                                .map_err(|_| anyhow::anyhow!("Instance lock poisoned"))?
                                .update_status(&next)?;
                            last_body = body;
                            last_status_write = Some(Instant::now());
                        }
                        let style = panel_style_payload(&next);
                        *cached.lock().map_err(|_| anyhow::anyhow!("Panel cache poisoned"))? = next;
                        if allow_panel_push && style != last_panel_style {
                            // One script call restyles every notch (primary plus other monitors).
                            let mut targets: Vec<(u32, u32)> = anchor.iter().chain(anchors.iter()).filter(|a| a.native).map(|a| (a.panel, a.widget)).collect();
                            targets.sort_unstable();
                            targets.dedup();
                            if !targets.is_empty() {
                                let mut command = std::process::Command::new("/usr/bin/qdbus6");
                                command.args(["org.kde.plasmashell", "/PlasmaShell", "org.kde.PlasmaShell.evaluateScript", &panel_style_script(&targets, &style)]);
                                match crate::services::process::run_bounded(command, Duration::from_secs(1), 4096, false) {
                                    Ok(_) => {
                                        let width_changed = !last_panel_style.is_null()
                                            && last_panel_style.get("notchWidth") != style.get("notchWidth");
                                        last_panel_style = style;
                                        // A new width moves the notches: re-discover once (a setting change, not a loop).
                                        if width_changed { last_monitors = None; }
                                    }
                                    Err(error) => tracing::debug!(%error, "Native notch appearance push unavailable"),
                                }
                            }
                        }
                        Ok(())
                    })();
                    last_write = Instant::now();
                    match update {
                        Ok(()) => last_error = None,
                        // Someone else owns status.json now: stop writing for good.
                        Err(error) if error.downcast_ref::<super::status_file::OwnershipLost>().is_some() => {
                            tracing::error!(%error, "Native status ownership lost; refusing further ownership writes");
                            break;
                        }
                        // Anything else (a slow main thread, a busy Plasma) is transient: keep the
                        // notch and status alive and try again on the next message or heartbeat.
                        Err(error) => {
                            let text = error.to_string();
                            if last_error.as_deref() != Some(text.as_str()) {
                                tracing::warn!(error = %text, "Native status update failed; retrying");
                            }
                            last_error = Some(text);
                        }
                    }
                }
            })?;
        // Every actor publication refreshes the status file (event-driven, coalesced above).
        let mut changes = actor.watch();
        let nudge = stop.clone();
        let watcher = tauri::async_runtime::spawn(async move {
            while changes.changed().await.is_ok() {
                if matches!(
                    nudge.try_send(StatusMsg::Refresh),
                    Err(mpsc::TrySendError::Disconnected(_))
                ) {
                    break;
                }
            }
        });
        Ok(Self {
            instance,
            panel: Mutex::new(Some(panel)),
            stop,
            worker: Mutex::new(Some(worker)),
            watcher: Mutex::new(Some(watcher)),
            companion: Mutex::new(companion),
        })
    }
    pub fn shutdown(&self) -> Result<()> {
        if let Some(mut panel) = self
            .panel
            .lock()
            .map_err(|_| anyhow::anyhow!("Panel slot poisoned"))?
            .take()
        {
            panel.shutdown()?;
        }
        if let Some(watcher) = self
            .watcher
            .lock()
            .map_err(|_| anyhow::anyhow!("Status watcher slot poisoned"))?
            .take()
        {
            watcher.abort();
        }
        if self.stop.send(StatusMsg::Stop).is_err() {
            tracing::debug!("Native status worker already stopped");
        }
        if let Some(worker) = self
            .worker
            .lock()
            .map_err(|_| anyhow::anyhow!("Status worker slot poisoned"))?
            .take()
        {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("Status worker panicked"))?;
        }
        if let Some(bus) = self
            .companion
            .lock()
            .map_err(|_| anyhow::anyhow!("Companion slot poisoned"))?
            .take()
        {
            // Leave no pointer script behind in KWin once the island is gone.
            if let Err(error) = super::companion::unload_script() {
                tracing::debug!(%error, "Companion pointer script unload failed");
            }
            drop(bus);
        }
        self.instance
            .lock()
            .map_err(|_| anyhow::anyhow!("Instance lock poisoned"))?
            .shutdown()
    }
}
#[cfg(test)]
mod monitor_tests {
    use super::*;
    fn anchor(name: &str, x: i32, id: u32) -> Anchor {
        super::super::anchor::anchor_from_data(&json!({"outputs":[{"name":name,"connected":true,"enabled":true,"priority":1,"pos":{"x":x,"y":0},"size":{"width":1920,"height":1080}}]}),
        &json!([{"id":id,"x":x,"y":0,"height":32,"location":"top","widgets":[{"id":id,"type":"local.hermes.island.notch3","x":800,"width":144}]}])).unwrap()
    }
    #[test]
    fn notch_positions_are_reused_after_a_click_until_they_age_out() {
        assert!(anchors_stale(None), "never read: read before trusting");
        assert!(
            !anchors_stale(Some(Instant::now())),
            "a fresh read serves rapid clicks"
        );
        let old = Instant::now().checked_sub(ANCHOR_TTL + Duration::from_secs(1));
        assert!(old.is_none_or(|t| anchors_stale(Some(t))));
    }
    #[test]
    fn clicked_source_wins_primary_and_unplug_replug_setting_off_are_safe() {
        let a = anchor("A", 0, 11);
        let b = anchor("B", -1920, 22);
        let both = vec![a.clone(), b.clone()];
        assert_eq!(
            select_monitor_anchor(&both, Some(&a), Some(&a), Some(22), true).unwrap(),
            Some(b.clone())
        );
        assert_eq!(
            select_monitor_anchor(&both, Some(&a), Some(&b), None, true).unwrap(),
            Some(b.clone())
        );
        assert_eq!(
            select_monitor_anchor(&both, Some(&a), Some(&b), None, false).unwrap(),
            Some(a.clone())
        );
        assert!(select_monitor_anchor(&both, Some(&a), Some(&a), Some(22), false).is_err());
        assert!(select_monitor_anchor(&both, Some(&a), Some(&a), Some(999), true).is_err());
        assert_eq!(
            select_monitor_anchor(std::slice::from_ref(&a), Some(&a), Some(&b), None, true)
                .unwrap(),
            Some(a.clone())
        );
        assert!(select_monitor_anchor(
            std::slice::from_ref(&a),
            Some(&a),
            Some(&b),
            Some(22),
            true
        )
        .is_err());
        assert_eq!(
            select_monitor_anchor(&both, Some(&a), Some(&a), Some(22), true).unwrap(),
            Some(b)
        );
    }
}

impl Drop for NativeHost {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::warn!(%error,"Native host cleanup failed");
        }
    }
}
