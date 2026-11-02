//! Native Wayland window path (`displayBackend: wayland`, Nov 2c).
//!
//! The XWayland path relies on X11 for three things: absolute placement over the panel,
//! ShapeInput, and KWin's `_KDE_NET_WM_BLUR_BEHIND_REGION`. Their Wayland equivalents:
//! - placement: a wlr-layer-shell OVERLAY surface anchored top-left on the panel's output with
//!   margins (normal toplevels cannot position themselves, and only the overlay layer is drawn
//!   above Plasma's panel); exclusive zone -1 so the panel's zone does not push it down;
//! - input region: `wl_surface.set_input_region` (GTK's own call is only flushed on the next
//!   paint, so the region is also written straight to the surface and committed);
//! - blur: `ext_background_effect_manager_v1` (what Plasma 6.5+ KWin advertises; the older
//!   `org_kde_kwin_blur_manager` is no longer offered to clients) with the KDE protocol as a
//!   fallback for older compositors.
//!
//! All requests go through GDK's own `wl_display` (a foreign libwayland connection) on the GTK
//! main thread, with a private event queue used only for the initial registry round trip.

use crate::error::IslandError;
use crate::window::shape::ShapeRow;
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;
use wayland_client::{
    backend::{Backend, ObjectId},
    globals::{registry_queue_init, GlobalListContents},
    protocol::{
        wl_compositor::WlCompositor, wl_region::WlRegion, wl_registry::WlRegistry,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use wayland_protocols_plasma::blur::client::{
    org_kde_kwin_blur::OrgKdeKwinBlur, org_kde_kwin_blur_manager::OrgKdeKwinBlurManager,
};

pub const NAMESPACE: &str = "hermes-island";

pub fn is_wayland(window: &gtk::ApplicationWindow) -> bool {
    window.display().backend().is_wayland()
}

/// Turn the (not yet mapped) island window into a layer-shell overlay at `origin`, given in
/// global logical coordinates. Must run before the window is first realized/mapped.
pub fn init_layer_surface(
    window: &gtk::ApplicationWindow,
    origin: (f64, f64),
) -> Result<(), IslandError> {
    use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
    if !gtk_layer_shell::is_supported() {
        return Err(IslandError::unsupported(
            "the compositor has no wlr-layer-shell; use the X11 (XWayland) display backend",
        ));
    }
    if window.is_realized() {
        tracing::info!("island window was realized before layer-shell init; re-realizing");
        window.unrealize();
    }
    window.init_layer_shell();
    window.set_namespace(NAMESPACE);
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    let display = window.display();
    let (x, y) = (origin.0.round() as i32, origin.1.round() as i32);
    let monitor = display
        .monitor_at_point(x + 1, y + 1)
        .ok_or_else(|| IslandError::unavailable("no monitor at the island position"))?;
    let area = monitor.geometry();
    LayerShell::set_monitor(window, &monitor);
    window.set_layer_shell_margin(Edge::Left, (x - area.x()).max(0));
    window.set_layer_shell_margin(Edge::Top, (y - area.y()).max(0));
    tracing::info!(
        monitor = monitor.model().as_deref().unwrap_or("?"),
        margin_left = x - area.x(),
        margin_top = y - area.y(),
        "island is a layer-shell overlay surface"
    );
    Ok(())
}

struct Globals;
impl Dispatch<WlRegistry, GlobalListContents> for Globals {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
wayland_client::delegate_noop!(Globals: ignore WlCompositor);
wayland_client::delegate_noop!(Globals: ignore WlRegion);
wayland_client::delegate_noop!(Globals: ignore OrgKdeKwinBlurManager);
wayland_client::delegate_noop!(Globals: ignore OrgKdeKwinBlur);
wayland_client::delegate_noop!(Globals: ignore ExtBackgroundEffectManagerV1);
wayland_client::delegate_noop!(Globals: ignore ExtBackgroundEffectSurfaceV1);

/// The island's own `wl_surface` plus the globals needed to shape it.
pub struct WaylandSurface {
    conn: Connection,
    qh: QueueHandle<Globals>,
    surface: WlSurface,
    compositor: WlCompositor,
    /// Preferred: the standard background-effect surface (created once, region updated).
    effect: Option<ExtBackgroundEffectSurfaceV1>,
    blur_manager: Option<OrgKdeKwinBlurManager>,
    blur: Option<OrgKdeKwinBlur>,
    scale: i32,
    // Keeps the private queue (and so `qh`) alive.
    _queue: wayland_client::EventQueue<Globals>,
}

// SAFETY-free note: every request below is issued on the GTK main thread only (the backend
// marshals through `run_on_main_thread`); the type is moved into that thread as a whole.
impl WaylandSurface {
    /// Must be called on the GTK main thread after the window is realized.
    pub fn new(window: &gtk::ApplicationWindow) -> Result<Self, IslandError> {
        let gdk_window = window
            .window()
            .ok_or_else(|| IslandError::unavailable("island window is not realized"))?;
        let display = gdk_window.display();
        let unavailable = |what: &str| IslandError::unavailable(format!("Wayland: {what}"));
        // SAFETY: both pointers come from GDK's live Wayland backend for this realized window.
        // `from_foreign_display` does not take ownership of the display (GDK keeps it), and the
        // surface id is a foreign proxy we only send requests on.
        let (conn, surface) = unsafe {
            let raw_display = gdk_wayland_sys::gdk_wayland_display_get_wl_display(
                ToGlibPtr::<*mut gdk::ffi::GdkDisplay>::to_glib_none(&display).0
                    as *mut gdk_wayland_sys::GdkWaylandDisplay,
            );
            let raw_surface = gdk_wayland_sys::gdk_wayland_window_get_wl_surface(
                ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_none(&gdk_window).0
                    as *mut gdk_wayland_sys::GdkWaylandWindow,
            );
            if raw_display.is_null() || raw_surface.is_null() {
                return Err(unavailable(
                    "GDK has no wl_display/wl_surface for the island",
                ));
            }
            let conn = Connection::from_backend(Backend::from_foreign_display(raw_display.cast()));
            let id = ObjectId::from_ptr(WlSurface::interface(), raw_surface.cast())
                .map_err(|e| unavailable(&format!("wl_surface id: {e}")))?;
            let surface = WlSurface::from_id(&conn, id)
                .map_err(|e| unavailable(&format!("wl_surface proxy: {e}")))?;
            (conn, surface)
        };
        let (globals, queue) = registry_queue_init::<Globals>(&conn)
            .map_err(|e| unavailable(&format!("registry: {e}")))?;
        let qh = queue.handle();
        let compositor: WlCompositor = globals
            .bind(&qh, 1..=4, ())
            .map_err(|e| unavailable(&format!("wl_compositor: {e}")))?;
        let effect = globals
            .bind::<ExtBackgroundEffectManagerV1, _, _>(&qh, 1..=1, ())
            .ok()
            .map(|manager| manager.get_background_effect(&surface, &qh, ()));
        let blur_manager: Option<OrgKdeKwinBlurManager> = if effect.is_some() {
            None
        } else {
            globals.bind(&qh, 1..=1, ()).ok()
        };
        tracing::info!(
            ext_background_effect = effect.is_some(),
            kde_blur = blur_manager.is_some(),
            "island blur protocol"
        );
        Ok(Self {
            conn,
            qh,
            surface,
            compositor,
            effect,
            blur_manager,
            blur: None,
            scale: window.scale_factor().max(1),
            _queue: queue,
        })
    }

    fn region(&self, rows: &[ShapeRow]) -> WlRegion {
        let region = self.compositor.create_region(&self.qh, ());
        for r in rows {
            // Rows are physical pixels; Wayland regions are surface-local logical pixels.
            let s = self.scale;
            region.add(
                i32::from(r.x) / s,
                i32::from(r.y) / s,
                (i32::from(r.width) + s - 1) / s,
                (i32::from(r.height) + s - 1) / s,
            );
        }
        region
    }

    pub fn set_input_region(&self, rows: &[ShapeRow]) -> Result<(), IslandError> {
        let region = self.region(rows);
        self.surface.set_input_region(Some(&region));
        region.destroy();
        self.surface.commit();
        self.flush()
    }

    /// Blur exactly `rows` (empty = no blur at all).
    pub fn set_blur_region(&mut self, rows: &[ShapeRow]) -> Result<(), IslandError> {
        if let Some(effect) = &self.effect {
            // An empty region blurs nothing (never pass null: that is "no region set").
            let region = self.region(rows);
            effect.set_blur_region(Some(&region));
            region.destroy();
            self.surface.commit();
            return self.flush();
        }
        let Some(manager) = &self.blur_manager else {
            return Ok(());
        };
        if rows.is_empty() {
            if let Some(blur) = self.blur.take() {
                blur.release();
                manager.unset(&self.surface);
            }
        } else {
            let blur = match &self.blur {
                Some(blur) => blur.clone(),
                None => {
                    let blur = manager.create(&self.surface, &self.qh, ());
                    self.blur = Some(blur.clone());
                    blur
                }
            };
            let region = self.region(rows);
            blur.set_region(Some(&region));
            blur.commit();
            region.destroy();
        }
        self.surface.commit();
        self.flush()
    }

    fn flush(&self) -> Result<(), IslandError> {
        self.conn
            .flush()
            .map_err(|e| IslandError::io(format!("Wayland flush: {e}")))
    }
}
