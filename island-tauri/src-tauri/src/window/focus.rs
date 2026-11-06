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
                if armed || interactive || gtk.has_toplevel_focus() {
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
