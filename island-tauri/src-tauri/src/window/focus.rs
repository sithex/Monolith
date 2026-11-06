//! Explicit activation, never a keyboard grab. Hover previews remain keyboard-inert.
//!
//! KWin 6.8 activates a mapped Top/Overlay layer surface accepting input. It does not
//! deactivate on None, and gtk-layer-shell's try_force_commit may skip pending commits.
//! Remap when necessary instead of relying on two setters making two Wayland commits.
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
pub(super) fn platform(window: &WebviewWindow, action: FocusPlan) -> Result<(), IslandError> {
    use gtk::prelude::*;
    use gtk_layer_shell::{KeyboardMode, LayerShell};
    let gtk = window
        .gtk_window()
        .map_err(|e| IslandError::unavailable(e.to_string()))?;
    if super::wayland::is_wayland(&gtk) {
        // None is the initial/preview state. Remapping with OnDemand activates on KWin
        // without Exclusive; returning to None must unmap even before keyboard-enter
        // arrives, since KWin does not deactivate on acceptsFocusChanged(false).
        let armed = gtk.keyboard_mode() != KeyboardMode::None;
        match action {
            FocusPlan::Activate => {
                if armed {
                    gtk.hide();
                }
                gtk.set_keyboard_mode(KeyboardMode::OnDemand);
                gtk.show();
                gtk.queue_draw();
            }
            FocusPlan::Release => {
                gtk.set_keyboard_mode(KeyboardMode::None);
                if armed || gtk.has_toplevel_focus() {
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
        gtk.hide();
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
