//! Island window creation (SPEC §2.3 steps 4–7, Phase 1 subset).
//!
//! The window is created hidden from `tauri.conf.json` (`create: false`), gets its platform
//! hooks and its initial (empty) input region while hidden, and is shown last, so there is never
//! a frame where an 1180×820 transparent rectangle eats clicks or steals focus.

#[cfg(target_os = "linux")]
pub mod crossing;
pub mod focus;
#[cfg(target_os = "linux")]
pub mod frame_clock;
pub mod input_region;
pub mod placement;
pub mod shape;
pub mod spike;
#[cfg(target_os = "linux")]
pub mod wayland;
#[cfg(target_os = "linux")]
pub mod zoom_guard;

use crate::{
    cli::Cli,
    contract::island::{GeometryReport, IslandMode, RoundedRect, UiKind},
    error::IslandError,
    identity,
};
use input_region::{InputRegionController, PlatformBackend};
use std::sync::{Arc, Mutex};
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const ISLAND_LABEL: &str = "island";

/// The island page's overlay window (Nov 2c: was 1180×820, the size of the other pages'
/// windows in tauri.conf.json). Larger so custom island sizes can grow; clamped to the screen
/// by `placement::over_panel`. The window is transparent and click-through outside the input
/// region, so the extra area costs nothing visible.
pub const ISLAND_OVERLAY_WIDTH: u32 = 1960;
pub const ISLAND_OVERLAY_HEIGHT: u32 = 1360;

/// Managed state shared with the IPC commands.
pub struct IslandRuntime {
    pub region: Arc<Mutex<InputRegionController<PlatformBackend>>>,
    /// SPEC §8 Phase 3: geometry reports must carry the actor's CURRENT `rev` and mode. True for
    /// the product page (`--ui legacy`). The Phase 1 / 1.5 / 2.5 harness and spike pages are
    /// fixtures that simulate their own state and rev counters (and their earlier gates may not
    /// change), so for them the controller's own monotonic-rev rules are the only check.
    pub actor_geometry: bool,
    /// Where the window was placed (global logical px). Wayland cannot report a toplevel's
    /// position, so status `bounds` use this on the native Wayland path.
    pub placement: Mutex<(f64, f64, f64, f64)>,
    pub wayland: bool,
    /// The native notch's CURRENT window-relative box. The initialization script only knows the
    /// startup snapshot, so every page load (a `reload-ui` deploy, a web-process restart)
    /// re-applies this one; otherwise a reload after a relocation draws the island at the
    /// startup position (e.g. 8 px low once a maximized window docked the floating panel).
    pub native_pill: Mutex<Option<RoundedRect>>,
    /// Output name of the notch the island sits under (published with the pill, so the page
    /// can mirror per-monitor notch layout such as the companion's clock shift).
    pub native_output: Mutex<Option<String>>,
}

/// JS that publishes `pill` as the `--native-pill-*` CSS variables and tells the island
/// renderer to re-measure. Shared by relocation and the page-load re-apply.
/// Window-local y of the placement's top edge: the Wayland layer surface starts
/// `wayland::TOP_BLEED` px above it (see there). Main thread only (asks GDK).
#[cfg(target_os = "linux")]
fn top_bleed() -> f64 {
    use gtk::prelude::*;
    if gdk::Display::default().is_some_and(|display| display.backend().is_wayland()) {
        f64::from(wayland::TOP_BLEED)
    } else {
        0.
    }
}
#[cfg(not(target_os = "linux"))]
fn top_bleed() -> f64 {
    0.
}
/// JS statement that publishes the notch's output name as `<html data-native-pill-output>`.
fn native_output_js(output: Option<&str>) -> String {
    match output {
        Some(name) => format!(
            "document.documentElement.dataset.nativePillOutput={};",
            serde_json::json!(name)
        ),
        None => "delete document.documentElement.dataset.nativePillOutput;".into(),
    }
}
pub fn native_pill_script(pill: &RoundedRect, output: Option<&str>) -> String {
    let values = serde_json::json!({ "x":pill.x,"y":pill.y,"width":pill.width,"height":pill.height,"radius":pill.radius });
    let output = native_output_js(output);
    format!("(()=>{{const v={values};for(const [k,x] of Object.entries(v))document.documentElement.style.setProperty('--native-pill-'+k,x+'px');{output}window.dispatchEvent(new Event('island-native-pill'));window.dispatchEvent(new Event('resize'));window.dispatchEvent(new Event('island-surfaces-changed'));}})()")
}

/// The page the window loads for `--ui` (`harness` is `index.html` from the config).
pub const LEGACY_PAGE: &str = "legacy.html";

/// The preview (`--ui legacy` without `--overlay`) never receives geometry reports from the
/// legacy renderer (it only calls `layout` when it is the overlay), so the whole window is
/// made interactive once, like island-rust's preview.
pub fn preview_report() -> GeometryReport {
    GeometryReport {
        state_rev: 0,
        mode: IslandMode::Large,
        rects: vec![RoundedRect {
            x: 0.0,
            y: 0.0,
            width: 1180.0,
            height: 820.0,
            radius: 0.0,
        }],
        scale: 1.0,
        settled: true,
    }
}

pub fn window_title(cli: &Cli) -> &'static str {
    if cli.overlay {
        identity::TITLE_OVERLAY
    } else {
        identity::TITLE_PREVIEW
    }
}

#[cfg(target_os = "linux")]
fn apply_platform_hooks(
    window: &WebviewWindow,
    spike: Option<&spike::SpikeConfig>,
    origin: (f64, f64),
) -> Result<(), IslandError> {
    use gtk::prelude::*;
    let gtk_window = window
        .gtk_window()
        .map_err(|e| IslandError::unavailable(format!("no GTK window: {e}")))?;
    if wayland::is_wayland(&gtk_window) {
        // Native Wayland: a layer-shell overlay replaces X11 type hints / keep-above / move.
        // tao gives undecorated Wayland windows an empty CSD titlebar; a layer surface needs
        // none (it would add GTK's client-side frame/shadow margins around the page).
        gtk_window.set_titlebar(None::<&gtk::Widget>);
        wayland::init_layer_surface(&gtk_window, origin)?;
    }
    // Must happen before the window is mapped. Matches the proven island-rust host.
    // Phase 3 adopts the accepted real-panel NOTIFICATION + keep-above decision.
    // Explicit Phase 2.5 variants remain historical experimental controls.
    let hint = spike.map_or(gdk::WindowTypeHint::Notification, |s| match s.window_type {
        spike::SpikeWindowType::Toolbar => gdk::WindowTypeHint::Toolbar,
        spike::SpikeWindowType::Notification => gdk::WindowTypeHint::Notification,
        spike::SpikeWindowType::Utility => gdk::WindowTypeHint::Utility,
        spike::SpikeWindowType::Splash => gdk::WindowTypeHint::Splashscreen,
    });
    gtk_window.set_type_hint(hint);
    gtk_window.set_skip_pager_hint(true);
    gtk_window.set_focus_on_map(false);
    gtk_window.set_accept_focus(true);
    gtk_window.stick();
    gtk_window.set_keep_above(spike.is_none_or(|s| s.above));
    // Create the X11 window (without mapping it) so its id exists and the input region can be
    // installed before the first frame is ever shown. Without this `window_handle()` reports
    // "the underlying handle is not available" during setup.
    gtk_window.realize();
    frame_clock::install(&gtk_window);
    // A pinch / Ctrl+wheel over the page must not rescale the island inside its overlay.
    zoom_guard::install();
    // WebKitGTK does not turn the X11 LeaveNotify into DOM pointer events when the pointer
    // leaves the island straight onto another surface (another monitor, the panel), so a
    // hovered side button stayed highlighted. Forward the real crossing to the page.
    gtk_window.add_events(gdk::EventMask::LEAVE_NOTIFY_MASK);
    let page = window.clone();
    gtk_window.connect_leave_notify_event(move |_, event| {
        if event.detail() != gdk::NotifyType::Inferior && event.mode() == gdk::CrossingMode::Normal {
            let script = format!(
                "window.dispatchEvent(new CustomEvent('island-pointer-left',{{detail:'{:?}/{:?}'}}))",
                event.detail(),
                event.mode()
            );
            if let Err(error) = page.eval(&script) {
                tracing::debug!(%error, "pointer-left forward failed");
            }
        }
        gtk::glib::Propagation::Proceed
    });
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn apply_platform_hooks(
    _window: &WebviewWindow,
    _spike: Option<&spike::SpikeConfig>,
    _origin: (f64, f64),
) -> Result<(), IslandError> {
    Err(IslandError::unsupported(
        "window platform hooks are Phase 8 on this platform",
    ))
}

pub fn create_island_window(
    app: &tauri::AppHandle,
    cli: &Cli,
) -> Result<WebviewWindow, IslandError> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == ISLAND_LABEL)
        .ok_or_else(|| IslandError::internal("tauri.conf.json has no `island` window"))?;
    let mut origin = placement::resolve(app);
    if let Some(anchor) = app.try_state::<crate::integrations::plasma::anchor::PlasmaAnchor>() {
        if let Some(anchor) = &anchor.0 {
            let target = crate::integrations::plasma::anchor::below_panel(anchor, 1180, 820)?;
            origin.x = f64::from(target.x);
            origin.y = f64::from(target.y);
        }
    }
    let mut config = config.clone();
    let mut overlap_pill = None;
    let mut overlap_output: Option<String> = None;
    if cli.ui == UiKind::Island && cli.overlay && cli.spike.is_none() {
        if let Some(anchor) = app.try_state::<crate::integrations::plasma::anchor::PlasmaAnchor>() {
            if let Some(anchor) = anchor.0.as_ref().filter(|a| a.native) {
                let target =
                    placement::over_panel(anchor, ISLAND_OVERLAY_WIDTH, ISLAND_OVERLAY_HEIGHT)?;
                origin.x = f64::from(target.x);
                origin.y = f64::from(target.y);
                config.width = f64::from(target.width);
                config.height = f64::from(target.height);
                overlap_pill = Some(RoundedRect {
                    x: anchor.pill.x - origin.x,
                    y: anchor.pill.y - origin.y + top_bleed(),
                    width: anchor.pill.width,
                    height: anchor.pill.height,
                    radius: anchor.pill_radius,
                });
                overlap_output = Some(anchor.output.clone());
            }
        }
    }
    if cli.ui == UiKind::Legacy {
        config.url = WebviewUrl::App(LEGACY_PAGE.into());
    }
    if cli.ui == UiKind::Island {
        config.url = WebviewUrl::App("island.html".into());
    }
    if let Some(spike) = &cli.spike {
        // Phase 2.5 (tests only): the spike page, and a window top that overlaps the panel.
        config.url = WebviewUrl::App(spike::SPIKE_PAGE.into());
        origin.y = f64::from(spike.y);
    }
    let mut builder = WebviewWindowBuilder::from_config(app, &config)
        .map_err(|e| IslandError::internal(format!("window config rejected: {e}")))?
        .title(window_title(cli))
        .position(origin.x, origin.y);
    if cli.test_instance && cli.ui == UiKind::Island {
        builder = builder.initialization_script("window.__phase7bErrors=[];addEventListener('error',e=>window.__phase7bErrors.push(String(e.error?.stack||e.message)));addEventListener('unhandledrejection',e=>window.__phase7bErrors.push(String(e.reason?.stack||e.reason)));");
    }
    if cli.ui == UiKind::Island {
        // Which display server this process runs on (Settings → Display server shows it; the
        // choice itself is the persisted `displayBackend`, applied by run.sh at launch).
        let backend = match std::env::var("GDK_BACKEND")
            .ok()
            .as_deref()
            .and_then(|v| v.split(',').next())
        {
            Some("wayland") => "wayland",
            _ => "x11",
        };
        builder = builder.initialization_script(format!(
            "(()=>{{const set=()=>{{if(!document.documentElement)return false;document.documentElement.dataset.displayBackend='{backend}';return true;}};if(!set()){{const o=new MutationObserver(()=>{{if(set())o.disconnect();}});o.observe(document,{{childList:true}});}}}})();"
        ));
    }
    if let Some(pill) = overlap_pill {
        // Geometry-only CSS configuration, installed before the island module/first paint.
        // No IPC type or duplicate state authority; native input uses this exact same snapshot.
        let values = serde_json::json!({ "x":pill.x,"y":pill.y,"width":pill.width,"height":pill.height,"radius":pill.radius });
        let output = native_output_js(overlap_output.as_deref());
        builder = builder.initialization_script(format!(r#"(()=>{{const values={values};const apply=()=>{{if(!document.documentElement)return false;for(const [k,v] of Object.entries(values))document.documentElement.style.setProperty('--native-pill-'+k,v+'px');{output}return true;}};if(!apply()){{const observer=new MutationObserver(()=>{{if(apply())observer.disconnect();}});observer.observe(document,{{childList:true}});}}}})();"#));
    }
    if overlap_pill.is_some() {
        builder = builder.on_page_load(|window, payload| {
            if payload.event() != tauri::webview::PageLoadEvent::Finished {
                return;
            }
            let Some(runtime) = window.try_state::<IslandRuntime>() else { return };
            let current = runtime.native_pill.lock().ok().and_then(|pill| *pill);
            let output = runtime.native_output.lock().ok().and_then(|o| o.clone());
            if let Some(pill) = current {
                if let Err(error) = window.eval(native_pill_script(&pill, output.as_deref())) {
                    tracing::warn!(%error, "Could not re-apply the notch position after a page load");
                }
            }
        });
    }
    let window = builder
        .build()
        .map_err(|e| IslandError::internal(format!("window creation failed: {e}")))?;

    apply_platform_hooks(&window, cli.spike.as_ref(), (origin.x, origin.y))?;

    #[cfg(target_os = "linux")]
    if matches!(cli.ui, UiKind::Legacy | UiKind::Island) && cli.spike.is_none() {
        let gtk_window = window
            .gtk_window()
            .map_err(|e| IslandError::unavailable(format!("no GTK window: {e}")))?;
        crossing::install(
            &gtk_window,
            app.state::<crate::state::host::IslandActor>()
                .handle
                .clone(),
        );
    }

    let backend = PlatformBackend::new(window.clone())?
        .with_blur_region(cli.spike.is_some_and(|spike| spike.blur));
    #[cfg(target_os = "linux")]
    let on_wayland = backend.is_wayland();
    #[cfg(not(target_os = "linux"))]
    let on_wayland = false;
    #[cfg(target_os = "linux")]
    let backend = backend.with_painted_blur(cli.ui == UiKind::Island && cli.spike.is_none());
    // The Phase 1 shape harness simulates a native pill. Phase 2 legacy bootstrap
    // reports panelAnchor=false until Phase 3 discovery exists, so its rendered pill
    // must remain interactive; an empty native-pill region would make it unreachable.
    let native_pill = cli.overlay
        && (cli.ui == UiKind::Harness
            || app
                .try_state::<crate::services::Services>()
                .is_some_and(|s| s.runtime.panel_anchor));
    let mut controller = InputRegionController::new(backend, native_pill);
    if cli.ui == UiKind::Island && cli.spike.is_none() {
        controller = controller.with_island_geometry(overlap_pill);
    }
    controller.install()?;
    if cli.ui == UiKind::Legacy && !cli.overlay {
        controller.report(&preview_report())?;
    }
    app.manage(IslandRuntime {
        region: Arc::new(Mutex::new(controller)),
        actor_geometry: matches!(cli.ui, UiKind::Legacy | UiKind::Island),
        placement: Mutex::new((origin.x, origin.y, config.width, config.height)),
        wayland: on_wayland,
        native_pill: Mutex::new(overlap_pill),
        native_output: Mutex::new(overlap_output),
    });

    window
        .show()
        .map_err(|e| IslandError::internal(format!("show failed: {e}")))?;
    Ok(window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_pill_script_publishes_every_css_variable_and_remeasures() {
        let script = native_pill_script(
            &RoundedRect {
                x: 1050.0,
                y: 0.0,
                width: 200.0,
                height: 32.0,
                radius: 16.0,
            },
            Some("DP-1"),
        );
        assert!(
            script.contains("dataset.nativePillOutput=\"DP-1\""),
            "{script}"
        );
        assert!(native_pill_script(
            &RoundedRect {
                x: 0.,
                y: 0.,
                width: 1.,
                height: 1.,
                radius: 0.
            },
            None
        )
        .contains("delete document.documentElement.dataset.nativePillOutput"));
        for (key, value) in [
            ("x", "1050"),
            ("y", "0"),
            ("width", "200"),
            ("height", "32"),
            ("radius", "16"),
        ] {
            assert!(
                script.contains(&format!("\"{key}\":{value}")),
                "{key} missing in {script}"
            );
        }
        assert!(script.contains("'--native-pill-'+k"));
        for event in ["island-native-pill", "resize", "island-surfaces-changed"] {
            assert!(script.contains(&format!("new Event('{event}')")));
        }
    }

    #[test]
    fn preview_report_covers_the_whole_window_and_is_settled() {
        let report = preview_report();
        assert!(report.settled);
        assert_eq!(report.mode, IslandMode::Large);
        assert_eq!(report.rects.len(), 1);
        let rect = report.rects[0];
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height, rect.radius),
            (0.0, 0.0, 1180.0, 820.0, 0.0)
        );
        // Through the real controller math: one full-window region, valid for the 1180x820 window.
        let rows = shape::region_rows(&report.rects, report.mode, report.scale, false).unwrap();
        assert!(!rows.is_empty());
    }
}
