//! Linux backend: `gtk_widget_input_shape_combine_region` on the GTK main thread, `x11rb`
//! `ShapeGetRectangles(INPUT)` for readback (SPEC §2.4) on GDK X11 (XWayland). On native
//! Wayland (`window::wayland`) the region and blur go to the island's `wl_surface`; Wayland has
//! no input-region readback, so `readback` reports the last region the compositor was sent.
//! The visual (bounding) shape is never touched; transparency handles visuals.

use super::InputRegionBackend;
use crate::error::IslandError;
use crate::window::shape::ShapeRow;
use gtk::prelude::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{sync::mpsc, time::Duration};
use tauri::WebviewWindow;
use x11rb::{
    protocol::{
        shape::{ConnectionExt as _, SK},
        xproto::{AtomEnum, ConnectionExt as _, PropMode},
    },
    wrapper::ConnectionExt as _,
};

const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(2);

/// KWin's blur-behind property (SPEC §0.5 (3)): CARDINAL[4n] = n window-relative rectangles
/// `x, y, w, h`. An *empty* property would mean "blur the whole window", so an empty region
/// is expressed by deleting the property instead.
pub const BLUR_PROPERTY: &str = "_KDE_NET_WM_BLUR_BEHIND_REGION";

pub struct PlatformBackend {
    window: WebviewWindow,
    xid: u32,
    /// Native Wayland surface (`xid` is 0 then); every request runs on the GTK main thread.
    wayland: Option<std::sync::Arc<std::sync::Mutex<crate::window::wayland::WaylandSurface>>>,
    wayland_applied: Vec<ShapeRow>,
    /// HiDPI X11: GTK shapes are logical, so XShape reports the logical rectangles scaled
    /// back up. `(applied, expected readback)` lets verification accept exactly that.
    x11_scaled: Option<(Vec<ShapeRow>, Vec<ShapeRow>)>,
    /// Phase 2.5 spike (tests only): mirror every applied region into `BLUR_PROPERTY`.
    blur: bool,
    independent_blur: bool,
    /// Reused for the per-frame blur writes while the island morphs (one X connection and
    /// one interned atom instead of a fresh connect + InternAtom round trip every frame).
    blur_conn: Option<BlurConnection>,
}

/// An X11 connection with `BLUR_PROPERTY` already interned.
pub struct BlurConnection {
    conn: x11rb::rust_connection::RustConnection,
    atom: u32,
}

impl BlurConnection {
    pub fn open() -> Result<Self, IslandError> {
        let (conn, _screen) = x11rb::connect(None)
            .map_err(|e| IslandError::unavailable(format!("cannot connect to X11: {e}")))?;
        let atom = conn
            .intern_atom(false, BLUR_PROPERTY.as_bytes())
            .map_err(|e| IslandError::io(format!("InternAtom request: {e}")))?
            .reply()
            .map_err(|e| IslandError::io(format!("InternAtom reply: {e}")))?
            .atom;
        Ok(Self { conn, atom })
    }
}

impl PlatformBackend {
    pub fn new(window: WebviewWindow) -> Result<Self, IslandError> {
        let gtk_window = window
            .gtk_window()
            .map_err(|e| IslandError::unavailable(format!("no GTK window: {e}")))?;
        if crate::window::wayland::is_wayland(&gtk_window) {
            let surface = crate::window::wayland::WaylandSurface::new(&gtk_window)?;
            return Ok(Self {
                window,
                xid: 0,
                wayland: Some(std::sync::Arc::new(std::sync::Mutex::new(surface))),
                wayland_applied: Vec::new(),
                x11_scaled: None,
                blur: false,
                independent_blur: false,
                blur_conn: None,
            });
        }
        let xid = x11_window_id(&window)?;
        Ok(Self {
            window,
            xid,
            wayland: None,
            wayland_applied: Vec::new(),
            x11_scaled: None,
            blur: false,
            independent_blur: false,
            blur_conn: None,
        })
    }

    /// Opt in to keeping the KWin blur-behind region equal to the applied input region.
    pub fn with_blur_region(mut self, on: bool) -> Self {
        self.blur = on;
        self
    }

    pub fn with_painted_blur(mut self, on: bool) -> Self {
        self.independent_blur = on;
        self.blur |= on;
        self
    }

    /// Move the existing GTK/webview, never construct another window or terminal.
    /// Called off-main-thread, serialized with geometry reports by the region owner.
    pub fn reposition(
        &mut self,
        target: crate::integrations::plasma::anchor::Placement,
    ) -> Result<(), IslandError> {
        let window = self.window.clone();
        let surface = self.wayland.clone();
        let (tx, rx) = mpsc::channel();
        self.window
            .run_on_main_thread(move || {
                let result = (|| {
                    let gtk = window
                        .gtk_window()
                        .map_err(|e| IslandError::unavailable(e.to_string()))?;
                    if let Some(surface) = surface {
                        use gtk_layer_shell::{Edge, LayerShell};
                        let monitor = gtk
                            .display()
                            .monitor_at_point(target.x + 1, target.y + 1)
                            .ok_or_else(|| {
                                IslandError::unavailable("clicked monitor disconnected")
                            })?;
                        let area = monitor.geometry();
                        if target.x < area.x()
                            || target.y < area.y()
                            || target.x >= area.x() + area.width()
                            || target.y >= area.y() + area.height()
                        {
                            return Err(IslandError::unavailable(
                                "clicked monitor no longer exists",
                            ));
                        }
                        LayerShell::set_monitor(&gtk, &monitor);
                        gtk.set_layer_shell_margin(Edge::Left, target.x - area.x());
                        gtk.set_layer_shell_margin(Edge::Top, target.y - area.y());
                        gtk.resize(target.width as i32, target.height as i32);
                        surface
                            .lock()
                            .map_err(|_| IslandError::internal("surface lock poisoned"))?
                            .refresh(&gtk)?;
                    } else {
                        gtk.move_(target.x, target.y);
                        gtk.resize(target.width as i32, target.height as i32);
                    }
                    Ok(())
                })();
                let _ = tx.send(result);
            })
            .map_err(|e| IslandError::unavailable(e.to_string()))?;
        rx.recv_timeout(MAIN_THREAD_TIMEOUT)
            .map_err(|_| IslandError::timeout("monitor transfer timed out"))?
    }

    pub fn focus_restore(
        &self,
        action: crate::window::focus::FocusPlan,
        generation: u64,
        input: &[ShapeRow],
        paint: Option<&[ShapeRow]>,
    ) -> Result<(), IslandError> {
        let (window, surface, input, paint) = (
            self.window.clone(),
            self.wayland.clone(),
            input.to_vec(),
            paint.map(<[ShapeRow]>::to_vec),
        );
        let (tx, rx) = mpsc::channel();
        self.window
            .run_on_main_thread(move || {
                let result = (|| {
                    if !crate::window::focus::current(generation) {
                        return Ok(());
                    }
                    crate::window::focus::platform(&window, action)?;
                    if let Some(surface) = surface {
                        let gtk = window
                            .gtk_window()
                            .map_err(|e| IslandError::unavailable(e.to_string()))?;
                        let mut surface = surface
                            .lock()
                            .map_err(|_| IslandError::internal("surface lock poisoned"))?;
                        surface.refresh(&gtk)?;
                        surface.set_input_region(&input)?;
                        if let Some(paint) = paint {
                            surface.set_blur_region(&paint)?;
                        }
                    }
                    Ok(())
                })();
                let _ = tx.send(result);
            })
            .map_err(|e| IslandError::unavailable(e.to_string()))?;
        rx.recv_timeout(MAIN_THREAD_TIMEOUT)
            .map_err(|_| IslandError::timeout("focus restore timed out"))?
    }

    pub fn xid(&self) -> u32 {
        self.xid
    }

    pub fn is_wayland(&self) -> bool {
        self.wayland.is_some()
    }

    /// Run `f` with the Wayland surface on the GTK main thread and wait for it (bounded).
    fn on_wayland<F>(&self, f: F) -> Result<(), IslandError>
    where
        F: FnOnce(&mut crate::window::wayland::WaylandSurface) -> Result<(), IslandError>
            + Send
            + 'static,
    {
        let Some(surface) = self.wayland.clone() else {
            return Ok(());
        };
        let (tx, rx) = mpsc::channel();
        let self_window = self.window.clone();
        self.window
            .run_on_main_thread(move || {
                let gtk = self_window.gtk_window();
                let result = surface
                    .lock()
                    .map_err(|_| IslandError::internal("Wayland surface lock poisoned"))
                    .and_then(|mut s| {
                        let gtk = gtk.map_err(|e| IslandError::unavailable(e.to_string()))?;
                        s.refresh(&gtk)?;
                        f(&mut s)
                    });
                if tx.send(result).is_err() {
                    tracing::warn!("Wayland surface result dropped: caller already timed out");
                }
            })
            .map_err(|e| IslandError::unavailable(format!("main thread unavailable: {e}")))?;
        rx.recv_timeout(MAIN_THREAD_TIMEOUT).map_err(|_| {
            IslandError::timeout("GTK main thread did not update the Wayland surface")
        })?
    }
}

/// Set (or, for an empty region, delete) the blur-behind property over a fresh connection.
pub fn set_blur_region(xid: u32, rows: &[ShapeRow]) -> Result<(), IslandError> {
    write_blur_region(&BlurConnection::open()?, xid, rows)
}

/// Set (or delete) the blur-behind property over `blur`'s connection, then read it back.
pub fn write_blur_region(
    blur: &BlurConnection,
    xid: u32,
    rows: &[ShapeRow],
) -> Result<(), IslandError> {
    let (conn, atom) = (&blur.conn, blur.atom);
    let io = |what: &str, e: &dyn std::fmt::Display| IslandError::io(format!("{what}: {e}"));
    if rows.is_empty() {
        conn.delete_property(xid, atom)
            .map_err(|e| io("DeleteProperty", &e))?;
    } else {
        let data: Vec<u32> = rows
            .iter()
            .flat_map(|r| {
                [
                    i32::from(r.x) as u32,
                    i32::from(r.y) as u32,
                    u32::from(r.width),
                    u32::from(r.height),
                ]
            })
            .collect();
        conn.change_property32(PropMode::REPLACE, xid, atom, AtomEnum::CARDINAL, &data)
            .map_err(|e| io("ChangeProperty", &e))?;
    }
    // The readback request is ordered after the write on this connection, so its reply
    // reflects the write (no separate sync round trip).
    let actual = conn
        .get_property(false, xid, atom, AtomEnum::ANY, 0, u32::MAX)
        .map_err(|e| io("blur readback request", &e))?
        .reply()
        .map_err(|e| io("blur readback reply", &e))?;
    let expected: Vec<u32> = rows
        .iter()
        .flat_map(|r| {
            [
                i32::from(r.x) as u32,
                i32::from(r.y) as u32,
                u32::from(r.width),
                u32::from(r.height),
            ]
        })
        .collect();
    let verified = if rows.is_empty() {
        actual.type_ == u32::from(AtomEnum::NONE)
    } else {
        actual.type_ == u32::from(AtomEnum::CARDINAL)
            && actual.format == 32
            && actual
                .value32()
                .is_some_and(|v| v.collect::<Vec<_>>() == expected)
    };
    if !verified {
        return Err(IslandError::internal(
            "Painted blur region readback mismatch",
        ));
    }
    Ok(())
}

fn x11_window_id(window: &WebviewWindow) -> Result<u32, IslandError> {
    let handle = window
        .window_handle()
        .map_err(|e| IslandError::unavailable(format!("no native window handle: {e}")))?;
    match handle.as_raw() {
        RawWindowHandle::Xlib(h) => u32::try_from(h.window)
            .map_err(|_| IslandError::internal("X11 window id does not fit in 32 bits")),
        RawWindowHandle::Xcb(h) => Ok(h.window.get()),
        other => Err(IslandError::unsupported(format!(
            "input region needs GDK on X11 (XWayland); window handle is {other:?}"
        ))),
    }
}

/// Applies physical `rows` as GTK's input shape and returns the GTK scale used. GTK widget
/// shapes are in logical (scale-independent) pixels: passing physical rows at scale 2 put the
/// clickable area at twice the offset, off the island, so nothing could be hovered or clicked.
fn apply_on_main_thread(window: &WebviewWindow, rows: &[ShapeRow]) -> Result<i32, IslandError> {
    let gtk_window = window
        .gtk_window()
        .map_err(|e| IslandError::unavailable(format!("no GTK window: {e}")))?;
    let scale = gtk_window.scale_factor().max(1);
    let rects: Vec<gtk::cairo::RectangleInt> = crate::window::shape::logical_rects(rows, scale)
        .into_iter()
        .map(|(x, y, w, h)| gtk::cairo::RectangleInt::new(x, y, w, h))
        .collect();
    let region = if rects.is_empty() {
        gtk::cairo::Region::create()
    } else {
        gtk::cairo::Region::create_rectangles(&rects)
    };
    gtk_window.input_shape_combine_region(Some(&region));
    // GDK queues the XShape request on its own connection; flush it so another X connection
    // (our verification readback, the test harness, the compositor) observes it immediately.
    match gdk::Display::default() {
        Some(display) => display.sync(),
        None => return Err(IslandError::unavailable("no default GDK display")),
    }
    Ok(scale)
}

impl InputRegionBackend for PlatformBackend {
    fn focus(
        &mut self,
        action: crate::window::focus::FocusPlan,
        generation: u64,
        input: &[ShapeRow],
        paint: Option<&[ShapeRow]>,
    ) -> Result<(), IslandError> {
        self.focus_restore(action, generation, input, paint)?;
        if self.wayland.is_none() && crate::window::focus::current(generation) {
            self.apply(input)?;
            if let Some(paint) = paint {
                self.paint(paint)?;
            }
        }
        Ok(())
    }
    fn apply(&mut self, region: &[ShapeRow]) -> Result<(), IslandError> {
        let applied = region;
        // Identical coverage, far fewer rectangles for GTK, the compositor and the blur hint.
        let region = &crate::window::shape::band_rows(region)[..];
        let rows = region.to_vec();
        let (tx, rx) = mpsc::channel();
        let window = self.window.clone();
        self.window
            .run_on_main_thread(move || {
                let result = apply_on_main_thread(&window, &rows);
                if tx.send(result).is_err() {
                    tracing::warn!("input region result dropped: caller already timed out");
                }
            })
            .map_err(|e| IslandError::unavailable(format!("main thread unavailable: {e}")))?;
        let scale = rx.recv_timeout(MAIN_THREAD_TIMEOUT).map_err(|_| {
            IslandError::timeout("GTK main thread did not apply the input region")
        })??;
        self.x11_scaled = (scale > 1).then(|| {
            (
                crate::window::shape::normalize_rows(applied),
                crate::window::shape::rescaled_rows(applied, scale),
            )
        });
        if self.wayland.is_some() {
            let rows = region.to_vec();
            let blur_rows = (self.blur && !self.independent_blur).then(|| rows.clone());
            self.on_wayland(move |surface| {
                surface.set_input_region(&rows)?;
                if let Some(rows) = blur_rows {
                    surface.set_blur_region(&rows)?;
                }
                Ok(())
            })?;
            self.wayland_applied = applied.to_vec();
            return Ok(());
        }
        if self.blur && !self.independent_blur {
            // The blur region follows the exact rows the input-region owner just applied.
            set_blur_region(self.xid, region)?;
        }
        Ok(())
    }

    fn readback(&self) -> Result<Vec<ShapeRow>, IslandError> {
        if self.wayland.is_some() {
            return Ok(self.wayland_applied.clone());
        }
        let actual = read_input_region(self.xid)?;
        if let Some((applied, expected)) = &self.x11_scaled {
            if crate::window::shape::normalize_rows(&actual) == *expected {
                return Ok(applied.clone());
            }
        }
        Ok(actual)
    }
    fn paint(&mut self, region: &[ShapeRow]) -> Result<(), IslandError> {
        if !self.independent_blur {
            return Ok(());
        }
        let region = &crate::window::shape::band_rows(region)[..];
        if self.wayland.is_some() {
            let rows = region.to_vec();
            return self.on_wayland(move |surface| surface.set_blur_region(&rows));
        }
        // Reuse the connection; after any failure retry once on a fresh one (the X server
        // may have dropped it), still failing closed when that fails too.
        if let Some(blur) = &self.blur_conn {
            match write_blur_region(blur, self.xid, region) {
                Ok(()) => return Ok(()),
                Err(error) => tracing::debug!(%error, "blur connection failed; reconnecting"),
            }
        }
        self.blur_conn = None;
        let blur = BlurConnection::open()?;
        write_blur_region(&blur, self.xid, region)?;
        self.blur_conn = Some(blur);
        Ok(())
    }
}

/// Read a window's current X11 ShapeInput rectangles over a fresh connection.
pub fn read_input_region(xid: u32) -> Result<Vec<ShapeRow>, IslandError> {
    let (conn, _screen) = x11rb::connect(None)
        .map_err(|e| IslandError::unavailable(format!("cannot connect to X11: {e}")))?;
    let reply = conn
        .shape_get_rectangles(xid, SK::INPUT)
        .map_err(|e| IslandError::io(format!("ShapeGetRectangles request failed: {e}")))?
        .reply()
        .map_err(|e| IslandError::io(format!("ShapeGetRectangles reply failed: {e}")))?;
    Ok(reply
        .rectangles
        .iter()
        .map(|r| ShapeRow {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        })
        .collect())
}
