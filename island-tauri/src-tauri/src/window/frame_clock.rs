//! GTK3 frame-clock pacing fix (both GDK backends) and the native-Wayland monitor kick.
//!
//! Root cause of the island's ~60 fps ceiling on a 240 Hz monitor (measured Nov 2 with
//! standalone GTK3/WebKitGTK probes, X11 and Wayland): GTK 3's `GdkFrameClockIdle` paces the
//! next frame at `previous frame + refresh_interval`, and falls back to a hard-coded 16 667 µs
//! (`FRAME_INTERVAL`) whenever the previous frame's timings carry no refresh interval. WebKit
//! presents through GL; on Wayland GDK skips its own frame callback for GL frames (EGL commits),
//! and on XWayland the compositor never reports frame timings, so the interval is always 0 and
//! every painting animation is held to 60 fps. Idle `requestAnimationFrame` (driven by WebKit's
//! own vblank monitor) was never affected, which is why it read 240.
//!
//! Fix: after every paint, fill the current frame's `refresh_interval` from the monitor the
//! window is on. Probe result on DP-1 (239.97 Hz): painting animation 60.5 → 240.0 fps on
//! XWayland and on a native Wayland layer surface.
//!
//! `GdkFrameTimings` is opaque; its GTK 3.24 layout puts `refresh_interval` at byte offset 56
//! on 64-bit targets. The write is guarded: it only happens when the raw field equals the
//! public getter `gdk_frame_timings_get_refresh_interval`, so a different layout disables it.
//! `HERMES_ISLAND_NO_REFRESH_FIX=1` turns it off for A/B measurement.

use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const REFRESH_INTERVAL_OFFSET: usize = 56;

pub fn install(window: &gtk::ApplicationWindow) {
    if window.display().backend().is_wayland() {
        // A transparent idle overlay may paint only once or twice after mapping, before its
        // surface has entered an output, so also re-announce the monitor on a short schedule
        // after every map (cheap; a no-op for WebKit when the display id is unchanged).
        window.connect_map(|w| {
            for delay in [150_u64, 400, 1000, 2500, 5000] {
                let w = w.clone();
                gtk::glib::timeout_add_local_once(
                    std::time::Duration::from_millis(delay),
                    move || send_configure(&w),
                );
            }
        });
    }
    if std::env::var_os("HERMES_ISLAND_NO_REFRESH_FIX").is_some() {
        tracing::info!("frame-clock refresh fix disabled by HERMES_ISLAND_NO_REFRESH_FIX");
        return;
    }
    attach(window);
    // A re-realize (layer-shell init on Wayland) replaces the GdkWindow and its frame clock.
    window.connect_realize(attach);
}

fn attach(window: &gtk::ApplicationWindow) {
    let Some(gdk_window) = window.window() else {
        return;
    };
    let Some(clock) = gdk_window.frame_clock() else {
        return;
    };
    let display = gdk_window.display();
    let wayland = display.backend().is_wayland();
    let disabled = Rc::new(Cell::new(false));
    let last_monitor: Rc<RefCell<Option<gdk::Monitor>>> = Rc::new(RefCell::new(None));
    let toplevel = window.clone();
    clock.connect_after_paint(move |clock| {
        let monitor = display.monitor_at_window(&gdk_window);
        if wayland && monitor != *last_monitor.borrow() {
            // WebKitGTK (GTK3) only re-reads the window's monitor on `configure-event`, which a
            // layer surface never receives after it enters its output: its display link then
            // stays on the primary monitor (or none at all, stalling requestAnimationFrame).
            *last_monitor.borrow_mut() = monitor.clone();
            let toplevel = toplevel.clone();
            gtk::glib::idle_add_local_once(move || send_configure(&toplevel));
        }
        if disabled.get() {
            return;
        }
        let Some(rate) = monitor.map(|m| m.refresh_rate()).filter(|r| *r > 0) else {
            return;
        };
        let interval = 1_000_000_000_i64 / i64::from(rate);
        let Some(timings) = clock.current_timings() else {
            return;
        };
        let public = timings
            .refresh_interval()
            .map_or(0, |v| i64::try_from(v.get()).unwrap_or(-1));
        let raw: *const gdk::ffi::GdkFrameTimings = timings.to_glib_none().0;
        // SAFETY: `raw` is a live, ref-held GdkFrameTimings (`timings` keeps a reference).
        // The offset is only trusted while the field it addresses reads back exactly what the
        // public getter reports; on the first disagreement the fix disables itself.
        unsafe {
            let field = raw.cast::<u8>().add(REFRESH_INTERVAL_OFFSET).cast::<i64>() as *mut i64;
            let current = field.read_unaligned();
            if current != public {
                tracing::warn!(
                    current,
                    public,
                    "GdkFrameTimings layout mismatch; refresh fix off"
                );
                disabled.set(true);
                return;
            }
            if current == 0 || current > interval {
                field.write_unaligned(interval);
            }
        }
    });
}

fn send_configure(window: &gtk::ApplicationWindow) {
    let Some(gdk_window) = window.window() else {
        return;
    };
    let allocation = window.allocation();
    let mut event = gdk::Event::new(gdk::EventType::Configure);
    // SAFETY: `event` is a freshly allocated GdkEvent of type GDK_CONFIGURE, so its union is a
    // GdkEventConfigure. The window reference is transferred (gdk_event_free unrefs it).
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent =
            gtk::glib::translate::ToGlibPtrMut::to_glib_none_mut(&mut event).0;
        let configure = raw.cast::<gdk::ffi::GdkEventConfigure>();
        (*configure).window = gdk_window.to_glib_full();
        (*configure).send_event = 1;
        (*configure).width = allocation.width();
        (*configure).height = allocation.height();
    }
    window.event(&event);
}
