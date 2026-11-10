//! Keeps the island page at 100 % zoom.
//!
//! WebKitGTK turns a touchpad pinch into a page scale and treats Ctrl+wheel as zoom input, so
//! a pinch over the island (or Ctrl+scroll) shrank/grew the whole UI inside its overlay window
//! with no way back short of a restart. `zoomHotkeysEnabled: false` only covers the keyboard.
//!
//! The filter is the process-wide GDK event handler: it runs before GTK dispatches an event
//! to any widget, so the zoom input never reaches WebKit (whatever gesture/class handler it
//! uses internally). GTK 3 has no public capture signal (`captured-event` is not a signal:
//! connecting to it panics). Plain scrolling and two-finger swipes pass through.

/// What a captured event is, reduced to the bits the zoom decision needs (testable without GDK).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Pinch,
    Scroll { ctrl: bool },
    Other,
}

/// True when the event would change the page zoom and must be swallowed.
pub fn blocks(input: Input) -> bool {
    matches!(input, Input::Pinch | Input::Scroll { ctrl: true })
}

fn classify(event: &gdk::Event) -> Input {
    match event.event_type() {
        gdk::EventType::TouchpadPinch => Input::Pinch,
        gdk::EventType::Scroll => Input::Scroll {
            ctrl: event
                .state()
                .is_some_and(|s| s.contains(gdk::ModifierType::CONTROL_MASK)),
        },
        _ => Input::Other,
    }
}

/// Install the filter (main thread, after GTK init). Idempotent; covers every window of the
/// process, which is what we want: no Monolith surface should ever be page-zoomed.
pub fn install() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    gdk::Event::set_handler(Some(|event: &mut gdk::Event| {
        if !blocks(classify(event)) {
            gtk::main_do_event(event);
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinch_and_ctrl_scroll_are_blocked() {
        assert!(blocks(Input::Pinch));
        assert!(blocks(Input::Scroll { ctrl: true }));
    }

    #[test]
    fn plain_scroll_and_other_input_pass() {
        assert!(!blocks(Input::Scroll { ctrl: false }));
        assert!(!blocks(Input::Other));
    }
}
