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
    sync::{mpsc, Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
use tauri_specta::Event;

const HEARTBEAT: Duration = Duration::from_secs(2);
const MIN_WRITE_INTERVAL: Duration = Duration::from_millis(100);

enum StatusMsg {
    Stop,
    Refresh,
}
/// Managed state: lets other modules request a (coalesced) status refresh.
pub struct StatusNudge(mpsc::Sender<StatusMsg>);

/// Ask the status worker for a refresh (e.g. after the input region changed). A missing or
/// stopped worker is not an error: there is simply no status file to keep current.
pub fn nudge_status(app: &tauri::AppHandle) {
    if let Some(nudge) = app.try_state::<StatusNudge>() {
        if nudge.0.send(StatusMsg::Refresh).is_err() {
            tracing::debug!("Status worker already stopped");
        }
    }
}

pub struct NativeHost {
    instance: Arc<Mutex<InstanceGuard>>,
    panel: Mutex<Option<PanelSocket>>,
    stop: mpsc::Sender<StatusMsg>,
    worker: Mutex<Option<JoinHandle<()>>>,
    watcher: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}
/// The one explicit mapping from canonical modes to the legacy names external tooling reads.
fn legacy_mode_name(mode: IslandMode) -> &'static str {
    match mode {
        IslandMode::Still => "idle",
        IslandMode::Hover => "preview",
        IslandMode::Large => "pinned",
    }
}
fn snapshot(app: &tauri::AppHandle, anchor: &Option<Anchor>, bounds: &Value) -> Result<Value> {
    use crate::window::input_region::InputRegionBackend;
    let services = app.state::<Services>();
    let region = app.state::<IslandRuntime>();
    let controller = region
        .region
        .lock()
        .map_err(|_| anyhow::anyhow!("Region lock poisoned"))?;
    let rectangles: Vec<_> = controller
        .backend()
        .readback()?
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
        "notchBorder":{"enabled":settings.notch_border_enabled,"color":border_color,"opacity":settings.notch_border_opacity,"width":settings.notch_border_width},
        "background":background,"text":palette.as_ref().map(|p|&p.text),"dim":palette.as_ref().map(|p|&p.dim)}),
    )
}
/// After the notch width changes, Plasma re-lays the panel out: rediscover the notch box and
/// move the island's notch endpoint (`--native-pill-*`) and native-pill input exclusion to it.
fn follow_notch_geometry(app: tauri::AppHandle) {
    let spawned = std::thread::Builder::new().name("ripple-notch-follow".into()).spawn(move || {
        // Plasma applies the KConfig push and relayouts asynchronously.
        std::thread::sleep(Duration::from_millis(700));
        let anchor = match tauri::async_runtime::block_on(super::anchor::discover()) {
            Ok(anchor) if anchor.native => anchor,
            Ok(_) => return,
            Err(error) => {
                tracing::debug!(%error, "Notch rediscovery after width change failed");
                return;
            }
        };
        let runtime = app.state::<IslandRuntime>();
        let (ox, oy, _, _) = runtime.placement;
        let pill = crate::contract::island::RoundedRect {
            x: anchor.pill.x - ox,
            y: anchor.pill.y - oy,
            width: anchor.pill.width,
            height: anchor.pill.height,
            radius: anchor.pill_radius,
        };
        if let Ok(mut controller) = runtime.region.lock() {
            controller.set_pill_exclusion(Some(pill));
        }
        if let Some(window) = app.get_webview_window(ISLAND_LABEL) {
            let values = json!({"x":pill.x,"y":pill.y,"width":pill.width,"height":pill.height,"radius":pill.radius});
            let script = format!("(()=>{{const v={values};for(const [k,x] of Object.entries(v))document.documentElement.style.setProperty('--native-pill-'+k,x+'px');window.dispatchEvent(new CustomEvent('island-native-pill'));}})()");
            if let Err(error) = window.eval(&script) {
                tracing::debug!(%error, "Notch geometry forward failed");
            }
        }
        tracing::info!(width = anchor.pill.width, x = anchor.pill.x, "Island follows the resized notch");
    });
    if let Err(error) = spawned {
        tracing::debug!(%error, "Notch follow thread unavailable");
    }
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
    ];
    Value::Object(
        keys.into_iter()
            .filter_map(|key| raw.get(key).map(|v| (key.to_owned(), v.clone())))
            .collect(),
    )
}
fn panel_style_script(panel: u32, widget: u32, style: &Value) -> String {
    let encoded = json!(style.to_string());
    format!("var p=panels().filter(p=>p.id==={panel})[0];var w=p&&p.widgets().filter(w=>w.id==={widget}&&(w.type==='local.hermes.island.notch3'||w.type==='local.hermes.island.notch2'||w.type==='local.hermes.island.notch'))[0];if(w){{w.currentConfigGroup=['General'];w.writeConfig('notchAppearance',{encoded});w.reloadConfig();print('notch-style-applied');}}")
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
        let script = panel_style_script(145, 407, &style);
        assert!(script.contains("407"));
        assert!(script.contains("notchAppearance"));
        assert!(!script.contains("private"));
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
        let bounds = if runtime.wayland {
            // Wayland never reports a surface's global position: publish the placement used.
            let (x, y, width, height) = runtime.placement;
            json!({"x":x.round() as i64,"y":y.round() as i64,"width":width.round() as i64,"height":height.round() as i64})
        } else {
            let position = window.outer_position()?;
            let size = window.inner_size()?;
            json!({"x":position.x,"y":position.y,"width":size.width,"height":size.height})
        };
        let actor = app.state::<IslandActor>().handle.clone();
        let initial = snapshot(app, &anchor, &bounds)?;
        instance.update_status(&initial)?;
        let cached = Arc::new(Mutex::new(initial));
        let state = cached.clone();
        let handle = app.clone();
        let router = actor.clone();
        let panel = PanelSocket::start(&mut instance, move |command| {
            let (kind, intent) = match command {
                PanelCommand::Status => (None, None),
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
        let (stop, receive) = mpsc::channel();
        app.manage(StatusNudge(stop.clone()));
        let allow_panel_push = !app.state::<Services>().runtime.test_instance;
        let worker = std::thread::Builder::new()
            .name("ripple-status".into())
            .spawn(move || {
                let mut last_write = Instant::now();
                let mut last_panel_style = Value::Null;
                loop {
                    match receive.recv_timeout(HEARTBEAT) {
                        Ok(StatusMsg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Ok(StatusMsg::Refresh) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    // Coalesce bursts: wait out the minimum interval, absorbing further nudges.
                    let wait = MIN_WRITE_INTERVAL.saturating_sub(last_write.elapsed());
                    if !wait.is_zero() {
                        match receive.recv_timeout(wait) {
                            Ok(StatusMsg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            Ok(StatusMsg::Refresh) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                        }
                    }
                    let update = (|| -> Result<()> {
                        let next = snapshot(&handle, &anchor, &bounds)?;
                        guard
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Instance lock poisoned"))?
                            .update_status(&next)?;
                        let style = panel_style_payload(&next);
                        *cached.lock().map_err(|_| anyhow::anyhow!("Panel cache poisoned"))? = next;
                        if allow_panel_push && style != last_panel_style {
                            if let Some(anchor) = anchor.as_ref().filter(|a| a.native) {
                                let mut command = std::process::Command::new("/usr/bin/qdbus6");
                                command.args(["org.kde.plasmashell", "/PlasmaShell", "org.kde.PlasmaShell.evaluateScript", &panel_style_script(anchor.panel, anchor.widget, &style)]);
                                match crate::services::process::run_bounded(command, Duration::from_secs(1), 4096, false) {
                                    Ok(_) => {
                                        let width_changed = !last_panel_style.is_null()
                                            && last_panel_style.get("notchWidth") != style.get("notchWidth");
                                        last_panel_style = style;
                                        if width_changed {
                                            follow_notch_geometry(handle.clone());
                                        }
                                    }
                                    Err(error) => tracing::debug!(%error, "Native notch appearance push unavailable"),
                                }
                            }
                        }
                        Ok(())
                    })();
                    last_write = Instant::now();
                    if let Err(error) = update {
                        tracing::error!(%error,"Native status update failed; refusing further ownership writes");
                        break;
                    }
                }
            })?;
        // Every actor publication refreshes the status file (event-driven, coalesced above).
        let mut changes = actor.watch();
        let nudge = stop.clone();
        let watcher = tauri::async_runtime::spawn(async move {
            while changes.changed().await.is_ok() {
                if nudge.send(StatusMsg::Refresh).is_err() {
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
        self.instance
            .lock()
            .map_err(|_| anyhow::anyhow!("Instance lock poisoned"))?
            .shutdown()
    }
}
impl Drop for NativeHost {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::warn!(%error,"Native host cleanup failed");
        }
    }
}
