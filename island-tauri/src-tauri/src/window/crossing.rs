//! Native pointer crossings: WebKit may swallow the DOM exit from an input-shaped window.
use crate::state::{actor::Handle, machine::Intent};
use gtk::prelude::*;

pub fn intent(enter: bool, detail: gdk::NotifyType) -> Option<Intent> {
    if detail == gdk::NotifyType::Inferior {
        return None;
    }
    Some(if enter {
        Intent::PointerEnter
    } else {
        Intent::PointerLeave
    })
}

pub fn install(window: &gtk::ApplicationWindow, actor: Handle) {
    window.add_events(gdk::EventMask::ENTER_NOTIFY_MASK | gdk::EventMask::LEAVE_NOTIFY_MASK);
    let entered = actor.clone();
    window.connect_enter_notify_event(move |_, event| {
        if let Some(intent) = intent(true, event.detail()) {
            if let Err(error) = entered.try_dispatch(intent) {
                tracing::warn!(%error, "Native pointer enter refused");
            }
        }
        glib::Propagation::Proceed
    });
    window.connect_leave_notify_event(move |_, event| {
        if let Some(intent) = intent(false, event.detail()) {
            if let Err(error) = actor.try_dispatch(intent) {
                tracing::warn!(%error, "Native pointer leave refused");
            }
        }
        glib::Propagation::Proceed
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notify_inferior_is_not_a_boundary_crossing() {
        assert!(intent(true, gdk::NotifyType::Inferior).is_none());
        assert!(intent(false, gdk::NotifyType::Inferior).is_none());
    }
    #[test]
    fn native_boundary_crossings_map_to_actor_intents() {
        for detail in [
            gdk::NotifyType::Ancestor,
            gdk::NotifyType::Virtual,
            gdk::NotifyType::Nonlinear,
            gdk::NotifyType::NonlinearVirtual,
        ] {
            assert!(matches!(intent(true, detail), Some(Intent::PointerEnter)));
            assert!(matches!(intent(false, detail), Some(Intent::PointerLeave)));
        }
    }
}
