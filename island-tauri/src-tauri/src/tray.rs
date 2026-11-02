//! The tray is an input adapter only: every action is a typed actor intent.
use crate::{
    contract::island::{IslandMode, IslandState, IslandView, UiIntent},
    error::IslandError,
    state::actor::Handle,
};
use std::sync::Mutex;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIcon, TrayIconBuilder},
};

pub fn action(id: &str, state: &IslandState) -> Option<UiIntent> {
    Some(match id {
        "toggle" => {
            if state.mode == IslandMode::Still {
                UiIntent::Pin
            } else {
                UiIntent::Collapse
            }
        }
        "terminal" => UiIntent::SelectView {
            view: IslandView::Terminal,
        },
        "notifications" => UiIntent::SelectView {
            view: IslandView::Notifications,
        },
        "clipboard" => UiIntent::SelectView {
            view: IslandView::Clipboard,
        },
        "dnd" => UiIntent::SetDnd { on: !state.dnd },
        "settings" => UiIntent::SelectView {
            view: IslandView::Settings,
        },
        "restart" => UiIntent::RestartTerminal,
        "quit" => UiIntent::Quit,
        _ => return None,
    })
}
/// Tauri's GTK tray backend silently ignores `set_tooltip`. Ayatana 0.6 exposes the
/// tooltip as GObject properties, but libappindicator-rs does not expose its GObject.
/// Capture it through the public `new-icon` signal while refreshing OUR icon once;
/// never inspect the private Rust wrapper layout or replace the native SNI service.
#[cfg(target_os = "linux")]
mod linux_tooltip {
    use super::*;
    use glib::{prelude::*, translate::*, Object, SendWeakRef};
    use std::ptr;

    pub struct Tooltip(SendWeakRef<Object>);

    impl Tooltip {
        pub fn new(icon: &TrayIcon) -> Result<Self, IslandError> {
            icon.with_inner_tray_icon(|inner| {
                let kind = glib::Type::from_name("AppIndicator")
                    .ok_or_else(|| IslandError::unavailable("AppIndicator type unavailable"))?;
                // SAFETY: GLib owns the type and signal metadata. The hook is synchronous,
                // removed before `captured` leaves scope, and only retains a GObject ref.
                unsafe extern "C" fn capture(
                    _: *mut glib::gobject_ffi::GSignalInvocationHint,
                    count: u32,
                    values: *const glib::gobject_ffi::GValue,
                    data: glib::ffi::gpointer,
                ) -> glib::ffi::gboolean {
                    if count > 0 && !values.is_null() {
                        let raw = glib::gobject_ffi::g_value_get_object(values);
                        if !raw.is_null() {
                            let object: Object = from_glib_none(raw);
                            if object.property::<String>("id") == "tray-icon tray app hermes-island"
                            {
                                *data.cast::<Option<Object>>() = Some(object);
                            }
                        }
                    }
                    1 // keep hook until the caller removes it, even for an unrelated indicator
                }
                let mut captured: Option<Object> = None;
                let signal = unsafe {
                    glib::gobject_ffi::g_signal_lookup(c"new-icon".as_ptr(), kind.into_glib())
                };
                if signal == 0 {
                    return Err(IslandError::unavailable(
                        "AppIndicator new-icon unavailable",
                    ));
                }
                let hook = unsafe {
                    glib::gobject_ffi::g_signal_add_emission_hook(
                        signal,
                        0,
                        Some(capture),
                        ptr::from_mut(&mut captured).cast(),
                        None,
                    )
                };
                if hook == 0 {
                    return Err(IslandError::unavailable("Cannot observe AppIndicator"));
                }
                let refreshed =
                    tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
                        .and_then(|image| image.try_into())
                        .map_err(|_| IslandError::internal("Cannot decode tray icon"))
                        .and_then(|image| {
                            inner
                                .set_icon(Some(image))
                                .map_err(|_| IslandError::unavailable("Cannot refresh tray icon"))
                        });
                unsafe { glib::gobject_ffi::g_signal_remove_emission_hook(signal, hook) };
                refreshed?;
                let object = captured
                    .ok_or_else(|| IslandError::unavailable("Native tray object unavailable"))?;
                if object.find_property("tooltip-title").is_none() {
                    return Err(IslandError::unsupported("Ayatana tooltip support required"));
                }
                Ok(Self(object.downgrade().into()))
            })
            .map_err(|_| IslandError::unavailable("Cannot access native tray"))?
        }

        pub fn set(&self, icon: &TrayIcon, text: String) -> Result<(), IslandError> {
            let object = self.0.clone();
            icon.with_inner_tray_icon(move |_| {
                // SendWeakRef is only upgraded on the GTK thread where it was created.
                let object = object
                    .upgrade()
                    .ok_or_else(|| IslandError::unavailable("Native tray object disappeared"))?;
                object.set_property("tooltip-title", text);
                Ok(())
            })
            .map_err(|_| IslandError::unavailable("Cannot update native tray tooltip"))?
        }
    }
}

pub struct Tray {
    _icon: TrayIcon,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}
impl Tray {
    pub fn start(app: &tauri::AppHandle, actor: Handle) -> Result<Self, IslandError> {
        // Subscribe BEFORE constructing native widgets: changes during construction must
        // not be lost, and the first refresh must not wait for another actor publication.
        let mut state = actor.watch();
        let initial = state.borrow().clone();
        use tauri::Manager;
        let clipboard = app
            .state::<std::sync::Arc<crate::integrations::clipboard::Clipboard>>()
            .inner()
            .clone();
        let mut clipboard_state = clipboard.watch_status();
        let initial_clipboard = clipboard_state.borrow().clone();
        let build = || -> tauri::Result<_> {
            let toggle = MenuItem::with_id(app, "toggle", "Show/Hide island", true, None::<&str>)?;
            let terminal = MenuItem::with_id(app, "terminal", "Open terminal", true, None::<&str>)?;
            let notifications = MenuItem::with_id(
                app,
                "notifications",
                format!("Notifications ({})", initial.unread),
                true,
                None::<&str>,
            )?;
            let dnd = CheckMenuItem::with_id(
                app,
                "dnd",
                "Do Not Disturb",
                true,
                initial.dnd,
                None::<&str>,
            )?;
            let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let clipboard_open =
                MenuItem::with_id(app, "clipboard", "Clipboard history", true, None::<&str>)?;
            let pause = CheckMenuItem::with_id(
                app,
                "clipboard-pause",
                "Clipboard paused",
                initial_clipboard.enabled,
                initial_clipboard.paused,
                None::<&str>,
            )?;
            let separator = PredefinedMenuItem::separator(app)?;
            let restart =
                MenuItem::with_id(app, "restart", "Restart terminal", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &toggle,
                    &terminal,
                    &notifications,
                    &dnd,
                    &settings,
                    &clipboard_open,
                    &pause,
                    &separator,
                    &restart,
                    &quit,
                ],
            )?;
            let router = actor.clone();
            let icon = TrayIconBuilder::with_id("hermes-island")
                .icon(tauri::image::Image::from_bytes(include_bytes!(
                    "../icons/32x32.png"
                ))?)
                .menu(&menu)
                .tooltip(format!("Hermes Island — {} unread", initial.unread))
                .on_menu_event(move |_, event| {
                    let intent = if event.id.as_ref() == "clipboard-pause" {
                        clipboard
                            .status()
                            .ok()
                            .map(|s| UiIntent::SetClipboardPaused { paused: !s.paused })
                    } else {
                        action(event.id.as_ref(), &router.state())
                    };
                    if let Some(intent) = intent {
                        if let Err(error) = router.try_dispatch(intent.into()) {
                            tracing::warn!(%error, "Tray action refused by actor");
                        }
                    }
                })
                .build(app)?;
            Ok((icon, notifications, dnd, pause))
        };
        let (icon, notifications, dnd, pause) =
            build().map_err(|_| IslandError::unavailable("Cannot create system tray"))?;
        #[cfg(target_os = "linux")]
        let tooltip = linux_tooltip::Tooltip::new(&icon)?;
        let update_icon = icon.clone();
        let task = tauri::async_runtime::spawn(async move {
            loop {
                let next = state.borrow_and_update().clone();
                let clip = clipboard_state.borrow_and_update().clone();
                if let Err(error) = pause
                    .set_checked(clip.paused)
                    .and_then(|()| pause.set_enabled(clip.enabled))
                {
                    tracing::warn!(%error, "Tray clipboard pause update failed");
                }
                if let Err(error) =
                    notifications.set_text(format!("Notifications ({})", next.unread))
                {
                    tracing::warn!(%error, "Tray count update failed");
                }
                if let Err(error) = dnd.set_checked(next.dnd) {
                    tracing::warn!(%error, "Tray DND update failed");
                }
                let text = format!("Hermes Island — {} unread", next.unread);
                #[cfg(target_os = "linux")]
                let updated = tooltip.set(&update_icon, text);
                #[cfg(not(target_os = "linux"))]
                let updated = update_icon.set_tooltip(Some(text));
                if let Err(error) = updated {
                    tracing::warn!(%error, "Tray tooltip update failed");
                }
                tokio::select! {
                    changed = state.changed() => { if changed.is_err() { break; } },
                    changed = clipboard_state.changed() => { if changed.is_err() { break; } },
                }
            }
        });
        Ok(Self {
            _icon: icon,
            task: Mutex::new(Some(task)),
        })
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        if let Some(task) = self
            .task
            .lock()
            .map_err(|_| IslandError::internal("Tray slot poisoned"))?
            .take()
        {
            task.abort();
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contract::settings::Settings,
        state::{
            actor::{self, MonotonicClock, Sink, Snapshot},
            machine::{Clock, Effect, Intent, Machine},
        },
    };
    use std::sync::Arc;
    struct Quiet;
    impl Sink for Quiet {
        fn apply(&mut self, _: &[Effect], _: &Snapshot) -> Result<(), IslandError> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn every_tray_action_maps_to_a_typed_actor_intent() {
        let (actor, task) = actor::spawn(
            Settings::default(),
            Arc::new(MonotonicClock::default()),
            Quiet,
        );
        for (id, view) in [
            ("terminal", IslandView::Terminal),
            ("notifications", IslandView::Notifications),
            ("settings", IslandView::Settings),
        ] {
            let intent = action(id, &actor.state()).unwrap();
            let changed = actor.dispatch(intent.into()).await.unwrap();
            assert_eq!(changed.state.view, view);
            assert_eq!(changed.state.mode, IslandMode::Large);
        }
        actor
            .dispatch(action("toggle", &actor.state()).unwrap().into())
            .await
            .unwrap();
        assert_eq!(actor.state().mode, IslandMode::Still);
        actor
            .dispatch(action("toggle", &actor.state()).unwrap().into())
            .await
            .unwrap();
        assert_eq!(actor.state().mode, IslandMode::Large);
        actor
            .dispatch(action("dnd", &actor.state()).unwrap().into())
            .await
            .unwrap();
        assert!(actor.state().dnd);
        assert!(matches!(
            action("restart", &actor.state()),
            Some(UiIntent::RestartTerminal)
        ));
        assert!(matches!(
            action("quit", &actor.state()),
            Some(UiIntent::Quit)
        ));
        assert!(action("unknown", &actor.state()).is_none());
        actor.shutdown();
        task.await.unwrap().unwrap();
    }
    #[test]
    fn restart_and_quit_are_effects_of_the_actor_not_tray_side_effects() {
        struct Now;
        impl Clock for Now {
            fn now_ms(&self) -> u64 {
                0
            }
        }
        let settings = Settings::default();
        let mut m = Machine::new(&settings);
        assert!(m
            .reduce(Intent::from(UiIntent::RestartTerminal), &settings, &Now)
            .unwrap()
            .contains(&Effect::RestartTerminal));
        assert!(m
            .reduce(Intent::from(UiIntent::Quit), &settings, &Now)
            .unwrap()
            .contains(&Effect::Quit));
    }
}
