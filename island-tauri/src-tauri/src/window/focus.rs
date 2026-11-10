//! Explicit activation, never a keyboard grab. Hover previews remain keyboard-inert.
//!
//! KWin 6.8 activates a mapped Top/Overlay layer surface accepting input. It does not
//! deactivate on None, and gtk-layer-shell's try_force_commit may skip pending commits.
//! A committed None frame precedes OnDemand, bypassing new-window focus prevention.
//! Collapse remaps keyboard-inert to return focus through the compositor's history.
use crate::{contract::island::IslandMode, error::IslandError};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{Manager, WebviewWindow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusPlan {
    Keep,
    Activate,
    Release,
    /// Deferred collapse remap (never planned from a mode; see `remap_when_idle`). Runs through
    /// the region controller like every focus action, so the input/blur regions are re-applied
    /// to the NEW wl_surface in the same main-thread step: a fresh surface accepts input
    /// everywhere, and the invisible overlay would otherwise swallow clicks across the screen.
    Remap,
}
pub fn plan(mode: IslandMode, explicit: bool) -> FocusPlan {
    if mode != IslandMode::Large {
        FocusPlan::Release
    } else if explicit {
        FocusPlan::Activate
    } else {
        FocusPlan::Keep
    }
}

// There is exactly one island window. Invalidate a queued activation immediately on
// collapse, before a blocking region operation can reach the GTK main thread.
static GENERATION: AtomicU64 = AtomicU64::new(0);
pub fn apply(window: &WebviewWindow, action: FocusPlan) -> Result<(), IslandError> {
    if action == FocusPlan::Keep {
        return Ok(());
    }
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    let window = window.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(runtime) = window.try_state::<super::IslandRuntime>() else {
            return;
        };
        let Ok(mut region) = runtime.region.lock() else {
            return;
        };
        if current(generation) {
            if let Err(error) = region.focus(action, generation) {
                tracing::warn!(%error, "Island keyboard activation failed");
            }
        }
    });
    Ok(())
}
pub(super) fn current(generation: u64) -> bool {
    GENERATION.load(Ordering::SeqCst) == generation
}

/// The island's latest mode is Still (set by the state sink before every focus decision).
static STILL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
pub fn note_mode(mode: IslandMode) {
    STILL.store(mode == IslandMode::Still, Ordering::SeqCst);
}
/// A collapse owes KWin a keyboard-inert remap that has not run yet.
#[cfg(target_os = "linux")]
static REMAP_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// A deferred-remap timer is running (at most one; later releases join it).
#[cfg(target_os = "linux")]
static REMAP_TIMER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Bumped by every explicit open: cancels a deferred remap (releases do NOT reset it, they
/// arrive on every still/hover state change and would otherwise starve the timer).
#[cfg(target_os = "linux")]
static REMAP_EPOCH: AtomicU64 = AtomicU64::new(0);

/// The collapse remap (unmap + map) destroys the wl_surface: the shell vanishes and comes back
/// with a fresh buffer and blur. Done mid-collapse, that showed as a flash/ghost of the island
/// for however many frames the GPU needed to repaint it (one 4 ms frame at 240 Hz, several on
/// a 120/60 Hz iGPU). So the remap waits until the island has stopped painting (the shell has
/// handed back to the native notch and is invisible), bounded by a hard cap; a reopen cancels
/// it (REMAP_EPOCH), and it only runs while the island is Still (never under a hover preview).
#[cfg(target_os = "linux")]
fn remap_when_idle(gtk: &gtk::ApplicationWindow, window: &WebviewWindow) {
    if REMAP_TIMER.swap(true, Ordering::SeqCst) {
        return;
    }
    use gtk::prelude::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        time::{Duration, Instant},
    };
    const MIN: Duration = Duration::from_millis(150);
    const QUIET: Duration = Duration::from_millis(120);
    const MAX: Duration = Duration::from_millis(1500);
    let epoch = Cell::new(REMAP_EPOCH.load(Ordering::SeqCst));
    let start = Cell::new(Instant::now());
    let last_paint = Rc::new(Cell::new(start.get()));
    let hook = Rc::new(RefCell::new(gtk.frame_clock().map(|clock| {
        let seen = last_paint.clone();
        let id = clock.connect_after_paint(move |_| seen.set(Instant::now()));
        (clock, id)
    })));
    let weak = gtk.downgrade();
    let window = window.clone();
    gtk::glib::timeout_add_local(Duration::from_millis(30), move || {
        let now = Instant::now();
        let alive = weak.upgrade().is_some();
        let reopened = REMAP_EPOCH.load(Ordering::SeqCst) != epoch.get();
        if reopened && REMAP_PENDING.load(Ordering::SeqCst) && alive {
            // Reopened, then collapsed again before this tick: owe the remap from now on.
            epoch.set(REMAP_EPOCH.load(Ordering::SeqCst));
            start.set(now);
            return gtk::glib::ControlFlow::Continue;
        }
        let cancelled = !alive || reopened;
        let elapsed = now - start.get();
        let idle = elapsed >= MIN && now - last_paint.get() >= QUIET;
        if !cancelled && !idle && elapsed < MAX {
            return gtk::glib::ControlFlow::Continue;
        }
        if let Some((clock, id)) = hook.borrow_mut().take() {
            clock.disconnect(id);
        }
        REMAP_TIMER.store(false, Ordering::SeqCst);
        if !cancelled && STILL.load(Ordering::SeqCst) {
            // Not Still (a hover preview is showing): left pending for the next release.
            if let Err(error) = apply(&window, FocusPlan::Remap) {
                tracing::warn!(%error, "Deferred collapse remap failed");
            }
        }
        gtk::glib::ControlFlow::Break
    });
}

#[cfg(target_os = "linux")]
pub(super) fn platform(
    window: &WebviewWindow,
    action: FocusPlan,
    generation: u64,
) -> Result<(), IslandError> {
    use gtk::prelude::*;
    use gtk_layer_shell::{KeyboardMode, LayerShell};
    let gtk = window
        .gtk_window()
        .map_err(|e| IslandError::unavailable(e.to_string()))?;
    if super::wayland::is_wayland(&gtk) {
        // Track the requested activation, not just GTK's asynchronous keyboard-enter:
        // collapse may race a None frame while KWin is still processing the old open.
        static ARMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        match action {
            FocusPlan::Activate => {
                ARMED.store(true, Ordering::SeqCst);
                // Opened again before a deferred collapse remap ran: nothing to hand back.
                REMAP_PENDING.store(false, Ordering::SeqCst);
                REMAP_EPOCH.fetch_add(1, Ordering::SeqCst);
                gtk.set_keyboard_mode(KeyboardMode::None);
                if let Some(clock) = gtk.frame_clock() {
                    use std::{cell::RefCell, rc::Rc};
                    let signal = Rc::new(RefCell::new(None));
                    let slot = signal.clone();
                    let weak = gtk.downgrade();
                    // GDK commits pending pixels in its after-paint handler. Connect AFTER
                    // that handler so None is on the wire before requesting OnDemand.
                    // Unlike try_force_commit this cannot silently skip a pending frame.
                    let id = clock.connect_local("after-paint", true, move |args| {
                        if let Some(id) = slot.borrow_mut().take() {
                            if let Ok(clock) = args[0].get::<gdk::FrameClock>() {
                                clock.disconnect(id);
                            }
                        }
                        if current(generation) {
                            if let Some(gtk) = weak.upgrade() {
                                gtk.set_keyboard_mode(KeyboardMode::OnDemand);
                                gtk.queue_draw();
                            }
                        }
                        None
                    });
                    *signal.borrow_mut() = Some(id);
                    clock.request_phase(gdk::FrameClockPhase::AFTER_PAINT);
                }
                gtk.queue_draw();
            }
            FocusPlan::Release => {
                let armed = ARMED.swap(false, Ordering::SeqCst);
                let interactive = gtk.keyboard_mode() != KeyboardMode::None;
                gtk.set_keyboard_mode(KeyboardMode::None);
                let pending = REMAP_PENDING.load(Ordering::SeqCst);
                if armed || interactive || pending || gtk.has_toplevel_focus() {
                    REMAP_PENDING.store(true, Ordering::SeqCst);
                    remap_when_idle(&gtk, window);
                }
            }
            FocusPlan::Remap => {
                // Still owed (no reopen since), Still, and keyboard-inert: hand focus back.
                if REMAP_PENDING.load(Ordering::SeqCst)
                    && STILL.load(Ordering::SeqCst)
                    && gtk.keyboard_mode() == KeyboardMode::None
                {
                    REMAP_PENDING.store(false, Ordering::SeqCst);
                    gtk.hide();
                    gtk.show();
                    gtk.queue_draw();
                }
            }
            FocusPlan::Keep => {}
        }
    } else if action == FocusPlan::Activate {
        gtk.present_with_time(gtk::current_event_time());
        activate_x11(window)?;
    } else if action == FocusPlan::Release && gtk.has_toplevel_focus() {
        // Unmap/map hands the window back to the WM's placement policy, which drops it
        // somewhere else on screen (observed: +166 px in the nested KWin). Re-assert the
        // position we had so a collapse never moves the overlay.
        let (x, y) = gtk.position();
        gtk.hide();
        gtk.move_(x, y);
        gtk.show();
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn activate_x11(window: &WebviewWindow) -> Result<(), IslandError> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::{
        connection::Connection,
        protocol::xproto::{ClientMessageEvent, ConnectionExt, EventMask},
    };
    let handle = window
        .window_handle()
        .map_err(|e| IslandError::unavailable(e.to_string()))?;
    let RawWindowHandle::Xlib(handle) = handle.as_raw() else {
        return Ok(());
    };
    let request = || -> Result<(), Box<dyn std::error::Error>> {
        let (conn, screen) = x11rb::connect(None)?;
        let atom = conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")?
            .reply()?
            .atom;
        // EWMH source 2: a panel's explicit user command, not background activation.
        let message = ClientMessageEvent::new(32, handle.window as u32, atom, [2, 0, 0, 0, 0]);
        conn.send_event(
            false,
            conn.setup().roots[screen].root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            message,
        )?
        .check()?;
        conn.flush()?;
        Ok(())
    };
    request().map_err(|e| IslandError::unavailable(format!("X11 activation: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_large_opens_activate_and_every_other_mode_releases() {
        for mode in [IslandMode::Still, IslandMode::Hover, IslandMode::Large] {
            for explicit in [false, true] {
                assert_eq!(
                    plan(mode, explicit),
                    if mode != IslandMode::Large {
                        FocusPlan::Release
                    } else if explicit {
                        FocusPlan::Activate
                    } else {
                        FocusPlan::Keep
                    }
                );
            }
        }
    }
    #[test]
    fn collapse_invalidates_an_activation_waiting_for_the_main_thread() {
        let open = GENERATION.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        assert!(current(open));
        let close = GENERATION.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        assert!(!current(open));
        assert!(current(close));
    }
}
