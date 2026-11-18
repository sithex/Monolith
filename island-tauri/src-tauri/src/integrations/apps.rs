//! Phase 6b1 lifecycle/wiring. No legacy view changes. Each app follows moduleRegistry enablement.
use crate::{
    bus::{Latest, COALESCE},
    contract::{apps::*, settings::Settings},
    error::IslandError,
    integrations::{
        agents::{AgentConfig, AgentHubSink, AgentService},
        autoclicker::Autoclicker,
        brightness::BrightnessService,
        calendar::{CalendarConfig, CalendarService, TzConfig},
        control_process::{fixture_from_env, Runner},
        downloads::{DesktopOpener, DownloadsService, Opener},
        jobs::TransfersService,
        mixer::MixerService,
        notifications::{hub::Hub, NotificationInput, NotificationSource, Urgency},
        power::PowerService,
        scratchpad::ScratchpadService,
        shelf::ShelfStore,
        tiles::TilesService,
        timer::{
            alarm::{AlarmPlayer, Chime},
            core::TimerEvent,
            CompletionSink, TimerService,
        },
        toggles::TogglesService,
        updates::UpdatesService,
    },
    state::{
        actor::{Handle, MonotonicClock},
        machine::{Activity, Intent},
    },
    window::ISLAND_LABEL,
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::Manager;
use tauri_specta::Event as _;
use tokio::sync::watch;

#[derive(Default)]
struct Slots {
    timer: Option<Arc<TimerService>>,
    scratchpad: Option<Arc<ScratchpadService>>,
    downloads: Option<Arc<DownloadsService>>,
    calendar: Option<Arc<CalendarService>>,
    brightness: Option<Arc<BrightnessService>>,
    agents: Option<Arc<AgentService>>,
    mixer: Option<Arc<MixerService>>,
    toggles: Option<Arc<TogglesService>>,
    updates: Option<Arc<UpdatesService>>,
    tiles: Option<Arc<TilesService>>,
}
pub struct AppServices {
    game_pause: super::plasma::game_effects::PollPause,
    /// File-copy progress (KDE job views). Not a launcher module: `notchCopyProgress` gates it.
    pub transfers: Arc<TransfersService>,
    /// Drop shelf (paths parked from the notch). Not a service: no thread, no watcher.
    pub shelf: Arc<ShelfStore>,
    /// Autoclicker (virtual mouse + KDE hotkey). Idle until started.
    pub autoclicker: Arc<Autoclicker>,
    slots: Mutex<Slots>,
    /// Modules the side buttons need (`side_button_needs`) and the settings last configured,
    /// so a layout change can re-run `configure`.
    side_needs: Mutex<Vec<&'static str>>,
    last_settings: Mutex<Option<Settings>>,
    app: tauri::AppHandle,
    data: PathBuf,
    test_instance: bool,
    hub: Arc<Hub>,
    actor: Handle,
    power: Arc<PowerService>,
}
pub struct TimerHubSink {
    pub hub: Arc<Hub>,
    pub actor: Handle,
    /// `None` in test instances: they never make real sounds.
    pub alarm: Option<AlarmPlayer>,
}
impl CompletionSink for TimerHubSink {
    fn complete(&self, event: TimerEvent, now: u64) -> Result<(), IslandError> {
        let (title, chime) = match event {
            TimerEvent::CountdownFinished => ("Timer finished", Chime::Alarm),
            TimerEvent::PomodoroPhaseEnded { .. } => ("Pomodoro phase finished", Chime::Phase),
        };
        if let Some(alarm) = &self.alarm {
            alarm.play(chime);
        }
        self.hub.insert(NotificationInput {
            source: NotificationSource::Alert,
            app_name: "Timer".into(),
            title: title.into(),
            body: None,
            icon_png: None,
            urgency: Urgency::Normal,
            actions: vec![],
            progress: None,
            dedupe_key: None,
            server_id: None,
            replaces_id: 0,
            transient: false,
            ttl_ms: None,
        })?;
        self.actor.try_dispatch(Intent::UpsertActivity(Activity {
            id: "timer-completed".into(),
            source: "timer".into(),
            priority: 80,
            compact: true,
            expanded: true,
            started_ms: now,
            expires_ms: None,
        }))
    }
    fn clear(&self) -> Result<(), IslandError> {
        if let Some(alarm) = &self.alarm {
            alarm.stop();
        }
        self.actor
            .try_dispatch(Intent::RemoveActivity("timer-completed".into()))
    }
}
struct TestOpener;
impl Opener for TestOpener {
    fn open(&self, _: &[OsString]) -> Result<(), IslandError> {
        Err(IslandError::unsupported(
            "Desktop opener disabled in test instances",
        ))
    }
}
/// A `data:` PNG of one solid colour (`#RRGGBB`), for the "colour copied" notification.
pub fn colour_swatch(hex: &str) -> Option<String> {
    use base64::Engine as _;
    use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
    let digits = hex.strip_prefix('#').filter(|d| d.len() >= 6)?;
    let channel = |i: usize| u8::from_str_radix(digits.get(i..i + 2)?, 16).ok();
    let rgb = [channel(0)?, channel(2)?, channel(4)?];
    const SIDE: u32 = 48;
    let pixels: Vec<u8> = rgb
        .iter()
        .copied()
        .cycle()
        .take((SIDE * SIDE * 3) as usize)
        .collect();
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&pixels, SIDE, SIDE, ExtendedColorType::Rgb8)
        .ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

/// Each finished colour pick posts one notification: the colour with a swatch, or why it
/// failed (a cancelled pick posts nothing). The success card stays until clicked
/// (`popups::sticky`) and the island draws it as a colour card with a Copy button. A pick from a side button tucks the island away,
/// so this is the only place the result shows.
async fn announce_colours(
    mut tiles: watch::Receiver<Option<crate::contract::tiles::TilesChanged>>,
    hub: Arc<Hub>,
) {
    let mut busy = false;
    loop {
        let (now_busy, colour, error) = match tiles.borrow_and_update().as_ref() {
            Some(t) => (t.picker_busy, t.last_colour.clone(), t.action_error.clone()),
            None => (false, None, None),
        };
        if busy && !now_busy {
            // Esc in the picker is a choice, not a failure.
            let cancelled = error
                .as_deref()
                .is_some_and(|e| e.to_ascii_lowercase().contains("cancel"));
            let note = match (&error, colour) {
                (None, Some(colour)) => Some((
                    format!("Copied {colour}"),
                    "The colour is on your clipboard.".to_owned(),
                    colour_swatch(&colour),
                )),
                (Some(error), _) if !cancelled => {
                    Some(("Couldn't pick a colour".to_owned(), error.clone(), None))
                }
                _ => None,
            };
            if let Some((title, body, icon_png)) = note {
                let posted = hub.insert(NotificationInput {
                    source: NotificationSource::Alert,
                    app_name: crate::integrations::notifications::popups::COLOUR_PICKER_APP.into(),
                    title,
                    body: Some(body),
                    icon_png,
                    urgency: Urgency::Low,
                    actions: vec![],
                    progress: None,
                    dedupe_key: None,
                    server_id: None,
                    replaces_id: 0,
                    transient: true,
                    ttl_ms: Some(6000.0),
                });
                if let Err(error) = posted {
                    tracing::warn!(%error, "Colour notification not posted");
                }
            }
        }
        busy = now_busy;
        if tiles.changed().await.is_err() {
            break;
        }
    }
}

/// The modules a side button's quick action runs through. They are started for the button
/// even while their app is switched off (the colour picker needs System tiles, the mic button
/// Quick toggles), so a button never reports "module disabled".
pub fn side_button_needs<'a>(
    actions: impl IntoIterator<Item = &'a crate::contract::workspace::LayoutAction>,
) -> Vec<&'static str> {
    use crate::contract::workspace::LayoutAction as A;
    let mut needs: Vec<&'static str> = actions
        .into_iter()
        .filter_map(|action| match action {
            A::Screenshot {} | A::PickColour {} | A::KeepAwake {} => Some("system-tiles"),
            A::MicMute {} | A::CyclePowerProfile {} => Some("quick-toggles"),
            _ => None,
        })
        .collect();
    needs.sort_unstable();
    needs.dedup();
    needs
}
/// The Controls page is the "quick-toggles" tile; its Keep awake, audio output, network and
/// voice cards come from "system-tiles". So an enabled Controls app runs both, and the
/// launcher draws only the one tile (Launcher.tsx skips "system-tiles").
fn controls_needs(settings: &Settings, id: &str) -> bool {
    id == "system-tiles" && active(settings, "quick-toggles")
}
fn active(settings: &Settings, id: &str) -> bool {
    settings
        .module_registry
        .modules()
        .iter()
        .any(|m| m.id.as_str() == id && m.is_active())
}
/// Test instances never discover the user's real XDG directory.
pub fn download_directory(test: bool, env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if test {
        return env("HERMES_ISLAND_DOWNLOAD_DIR")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
    }
    let home = env("HOME").map(PathBuf::from)?;
    let config = env("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    if let Ok(text) = std::fs::read_to_string(config.join("user-dirs.dirs")) {
        for line in text.lines() {
            if let Some(value) = line
                .trim()
                .strip_prefix("XDG_DOWNLOAD_DIR=\"")
                .and_then(|s| s.strip_suffix('"'))
            {
                if let Some(tail) = value.strip_prefix("$HOME/") {
                    return Some(home.join(tail));
                }
                let path = PathBuf::from(value);
                if path.is_absolute() {
                    return Some(path);
                }
            }
        }
    }
    Some(home.join("Downloads"))
}
impl AppServices {
    pub fn snapshot(&self) -> Result<AppsSnapshot, IslandError> {
        let s = self.lock()?;
        Ok(AppsSnapshot {
            tiles: s.tiles.as_ref().map(|s| s.current()),
            updates: s.updates.as_ref().map(|s| s.current()),
            toggles: s.toggles.as_ref().map(|s| s.current()),
            mixer: s.mixer.as_ref().map(|s| s.current()),
            agents: s.agents.as_ref().map(|a| a.current()).transpose()?,
            brightness: s.brightness.as_ref().map(|b| b.current()),
            timer: s
                .timer
                .as_ref()
                .and_then(|t| t.topic.subscribe().borrow().clone()),
            scratchpad: s.scratchpad.as_ref().map(|s| s.current()).transpose()?,
            downloads: s.downloads.as_ref().map(|s| s.current()).transpose()?,
            transfers: Some(self.transfers.current()),
            shelf: self.shelf.current().ok(),
            autoclicker: self.autoclicker.current().ok(),
            calendar: s
                .calendar
                .as_ref()
                .and_then(|c| c.topic.subscribe().borrow().clone()),
        })
    }
    pub fn new(
        app: &tauri::AppHandle,
        data: &Path,
        test_instance: bool,
        hub: Arc<Hub>,
        power: Arc<PowerService>,
    ) -> Self {
        Self::new_gated(app, data, test_instance, hub, power, Default::default())
    }
    pub fn new_gated(
        app: &tauri::AppHandle,
        data: &Path,
        test_instance: bool,
        hub: Arc<Hub>,
        power: Arc<PowerService>,
        game_pause: super::plasma::game_effects::PollPause,
    ) -> Self {
        let opener: Arc<dyn Opener> = if test_instance {
            Arc::new(TestOpener)
        } else {
            Arc::new(DesktopOpener)
        };
        let shelf = ShelfStore::new(data, opener.clone());
        let autoclicker = Autoclicker::new(data, test_instance);
        let transfers = TransfersService::start(
            super::system_services::session_bus_address(
                test_instance,
                std::env::var("DBUS_SESSION_BUS_ADDRESS").ok(),
                std::env::var("XDG_RUNTIME_DIR").ok(),
            ),
            false,
            opener,
        );
        let app_handle = app.clone();
        tauri::async_runtime::spawn(transfers.topic.forwarder(COALESCE, move |v| {
            if v.emit_to(&app_handle, ISLAND_LABEL).is_err() {
                tracing::warn!("App event delivery failed");
            }
        }));
        let app_handle = app.clone();
        tauri::async_runtime::spawn(shelf.topic.forwarder(COALESCE, move |v| {
            if v.emit_to(&app_handle, ISLAND_LABEL).is_err() {
                tracing::warn!("App event delivery failed");
            }
        }));
        let app_handle = app.clone();
        tauri::async_runtime::spawn(autoclicker.topic.forwarder(COALESCE, move |v| {
            if v.emit_to(&app_handle, ISLAND_LABEL).is_err() {
                tracing::warn!("App event delivery failed");
            }
        }));
        Self {
            game_pause,
            transfers,
            shelf,
            autoclicker,
            power,
            slots: Mutex::new(Slots::default()),
            side_needs: Mutex::new(Vec::new()),
            last_settings: Mutex::new(None),
            app: app.clone(),
            data: data.into(),
            test_instance,
            hub,
            actor: app
                .state::<crate::state::host::IslandActor>()
                .handle
                .clone(),
        }
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Slots>, IslandError> {
        self.slots
            .lock()
            .map_err(|_| IslandError::internal("App services poisoned"))
    }
    fn forward<T: Clone + Send + Sync + tauri_specta::Event + serde::Serialize + 'static>(
        &self,
        topic: &Latest<T>,
    ) {
        let app = self.app.clone();
        tauri::async_runtime::spawn(topic.forwarder(COALESCE, move |v| {
            if v.emit_to(&app, ISLAND_LABEL).is_err() {
                tracing::warn!("App event delivery failed");
            }
        }));
    }
    /// The side buttons changed: start (or stop) the modules their quick actions need.
    pub fn side_buttons<'a>(
        &self,
        actions: impl IntoIterator<Item = &'a crate::contract::workspace::LayoutAction>,
    ) -> Result<(), IslandError> {
        let needs = side_button_needs(actions);
        {
            let mut current = self
                .side_needs
                .lock()
                .map_err(|_| IslandError::internal("Side button needs poisoned"))?;
            if *current == needs {
                return Ok(());
            }
            *current = needs;
        }
        let settings = self.last_settings.lock().ok().and_then(|s| s.clone());
        match settings {
            Some(settings) => self.configure(&settings),
            None => Ok(()),
        }
    }
    fn wanted(&self, settings: &Settings, id: &str) -> bool {
        active(settings, id)
            || controls_needs(settings, id)
            || self
                .side_needs
                .lock()
                .is_ok_and(|needs| needs.contains(&id))
    }
    pub fn configure(&self, settings: &Settings) -> Result<(), IslandError> {
        if let Ok(mut last) = self.last_settings.lock() {
            *last = Some(settings.clone());
        }
        self.transfers.configure(settings.notch_copy_progress);
        self.shelf
            .configure(settings.shelf_enabled, settings.shelf_max_items);
        let mut slots = self.lock()?;
        if self.wanted(settings, "timer") {
            if slots.timer.is_none() {
                let timer = TimerService::start(
                    Arc::new(MonotonicClock::default()),
                    Arc::new(TimerHubSink {
                        hub: self.hub.clone(),
                        actor: self.actor.clone(),
                        alarm: (!self.test_instance).then(AlarmPlayer::default),
                    }),
                )?;
                self.forward(&timer.topic);
                slots.timer = Some(timer);
            }
        } else if let Some(timer) = slots.timer.take() {
            timer.shutdown()?;
        }
        if self.wanted(settings, "scratchpad") {
            if slots.scratchpad.is_none() {
                let scratch = ScratchpadService::new(self.data.clone());
                scratch.start()?;
                self.forward(&scratch.topic);
                slots.scratchpad = Some(scratch);
            }
        } else if let Some(scratch) = slots.scratchpad.take() {
            scratch.shutdown()?;
        }
        if self.wanted(settings, "downloads") {
            if slots.downloads.is_none() {
                let opener: Arc<dyn Opener> = if self.test_instance {
                    Arc::new(TestOpener)
                } else {
                    Arc::new(DesktopOpener)
                };
                let downloads = DownloadsService::start(
                    download_directory(self.test_instance, |n| std::env::var(n).ok()),
                    opener,
                )?;
                self.forward(&downloads.topic);
                slots.downloads = Some(downloads);
            }
        } else if let Some(downloads) = slots.downloads.take() {
            downloads.shutdown()?;
        }
        if self.wanted(settings, "calendar") {
            if slots.calendar.is_none() {
                let calendar = CalendarService::new_gated(
                    CalendarConfig {
                        data: self.data.clone(),
                        loopback_test: self.test_instance,
                        timeout: Duration::from_secs(10),
                        tz: if self.test_instance {
                            TzConfig {
                                local: Some("UTC".into()),
                                dir: None,
                            }
                        } else {
                            TzConfig::default()
                        },
                    },
                    self.game_pause.clone(),
                )?;
                self.forward(&calendar.topic);
                calendar.start()?;
                slots.calendar = Some(calendar);
            }
        } else if let Some(calendar) = slots.calendar.take() {
            calendar.shutdown()?;
        }
        if self.wanted(settings, "system-tiles") {
            if slots.tiles.is_none() {
                let session = if self.test_instance {
                    std::env::var("HERMES_ISLAND_CONTROLS_SESSION_BUS").ok()
                } else {
                    super::system_services::session_bus_address(
                        false,
                        std::env::var("DBUS_SESSION_BUS_ADDRESS").ok(),
                        std::env::var("XDG_RUNTIME_DIR").ok(),
                    )
                };
                let system = if self.test_instance {
                    std::env::var("HERMES_ISLAND_CONTROLS_SYSTEM_BUS").ok()
                } else {
                    super::system_services::system_bus_address(
                        false,
                        std::env::var("DBUS_SYSTEM_BUS_ADDRESS").ok(),
                    )
                };
                // The clipboard stand-in is always managed; only a failed stand-in is missing.
                match self.app.try_state::<Arc<super::clipboard::Clipboard>>() {
                    Some(clipboard) => {
                        let service = TilesService::start(
                            Runner::for_instance(self.test_instance, fixture_from_env()),
                            session,
                            system,
                            clipboard.inner().clone(),
                        );
                        self.forward(&service.topic);
                        tauri::async_runtime::spawn(announce_colours(
                            service.topic.subscribe(),
                            self.hub.clone(),
                        ));
                        slots.tiles = Some(service);
                    }
                    None => tracing::warn!(
                        "System tiles need the clipboard service, which failed to start"
                    ),
                }
            }
        } else if let Some(service) = slots.tiles.take() {
            service.shutdown()?;
        }
        if self.wanted(settings, "update-badge") {
            if slots.updates.is_none() {
                let service = UpdatesService::start_gated(
                    Runner::for_instance(self.test_instance, fixture_from_env()),
                    Arc::new(MonotonicClock::default()),
                    self.power.topic.subscribe(),
                    self.game_pause.clone(),
                );
                self.forward(&service.topic);
                slots.updates = Some(service);
            }
        } else if let Some(service) = slots.updates.take() {
            service.shutdown()?;
        }
        if self.wanted(settings, "quick-toggles") {
            if slots.toggles.is_none() {
                let address = if self.test_instance {
                    std::env::var("HERMES_ISLAND_CONTROLS_SYSTEM_BUS").ok()
                } else {
                    super::system_services::system_bus_address(
                        false,
                        std::env::var("DBUS_SYSTEM_BUS_ADDRESS").ok(),
                    )
                };
                let service = TogglesService::start(
                    Runner::for_instance(self.test_instance, fixture_from_env()),
                    address,
                    self.actor.clone(),
                );
                self.forward(&service.topic);
                slots.toggles = Some(service);
            }
        } else if let Some(service) = slots.toggles.take() {
            service.shutdown()?;
        }
        if self.wanted(settings, "mixer") {
            if slots.mixer.is_none() {
                let service = MixerService::start(Runner::for_instance(
                    self.test_instance,
                    fixture_from_env(),
                ));
                self.forward(&service.topic);
                slots.mixer = Some(service);
            }
        } else if let Some(service) = slots.mixer.take() {
            service.shutdown()?;
        }
        if self.wanted(settings, "brightness") {
            if slots.brightness.is_none() {
                // A test instance requires an explicit private endpoint. Never implicitly use
                // its inherited session bus, which may be the user's desktop bus.
                let address = if self.test_instance {
                    std::env::var("HERMES_ISLAND_CONTROLS_SESSION_BUS").ok()
                } else {
                    super::system_services::session_bus_address(
                        false,
                        std::env::var("DBUS_SESSION_BUS_ADDRESS").ok(),
                        std::env::var("XDG_RUNTIME_DIR").ok(),
                    )
                };
                let service = BrightnessService::start(address);
                self.forward(&service.topic);
                slots.brightness = Some(service);
            }
        } else if let Some(service) = slots.brightness.take() {
            service.shutdown()?;
        }
        if self.wanted(settings, "agent-tracker") {
            if slots.agents.is_none() {
                let config = AgentConfig::for_instance(
                    self.test_instance,
                    std::env::var_os("HOME").map(PathBuf::from),
                    std::env::var_os("HERMES_ISLAND_AGENT_DB").map(PathBuf::from),
                );
                let service = AgentService::new_gated(
                    config,
                    Arc::new(AgentHubSink::new(
                        self.hub.clone(),
                        self.actor.clone(),
                        Arc::new(MonotonicClock::default()),
                    )),
                    self.game_pause.clone(),
                );
                self.forward(&service.topic);
                service.start()?;
                slots.agents = Some(service);
            }
        } else if let Some(service) = slots.agents.take() {
            service.shutdown()?;
        }
        Ok(())
    }
    pub fn mixer(&self) -> Result<Arc<MixerService>, IslandError> {
        self.lock()?
            .mixer
            .clone()
            .ok_or_else(|| IslandError::unavailable("Mixer module disabled"))
    }
    pub fn toggles(&self) -> Result<Arc<TogglesService>, IslandError> {
        self.lock()?
            .toggles
            .clone()
            .ok_or_else(|| IslandError::unavailable("Quick toggles module disabled"))
    }
    pub fn updates(&self) -> Result<Arc<UpdatesService>, IslandError> {
        self.lock()?
            .updates
            .clone()
            .ok_or_else(|| IslandError::unavailable("Update badge module disabled"))
    }
    pub fn tiles(&self) -> Result<Arc<TilesService>, IslandError> {
        self.lock()?
            .tiles
            .clone()
            .ok_or_else(|| IslandError::unavailable("System tiles module disabled"))
    }
    pub fn agents(&self) -> Result<Arc<AgentService>, IslandError> {
        self.lock()?
            .agents
            .clone()
            .ok_or_else(|| IslandError::unavailable("Agent tracker module disabled"))
    }
    pub fn brightness(&self) -> Result<Arc<BrightnessService>, IslandError> {
        self.lock()?
            .brightness
            .clone()
            .ok_or_else(|| IslandError::unavailable("Brightness module disabled"))
    }
    pub fn timer(&self) -> Result<Arc<TimerService>, IslandError> {
        self.lock()?
            .timer
            .clone()
            .ok_or_else(|| IslandError::unavailable("Timer module disabled"))
    }
    pub fn scratchpad(&self) -> Result<Arc<ScratchpadService>, IslandError> {
        self.lock()?
            .scratchpad
            .clone()
            .ok_or_else(|| IslandError::unavailable("Scratchpad module disabled"))
    }
    pub fn downloads(&self) -> Result<Arc<DownloadsService>, IslandError> {
        self.lock()?
            .downloads
            .clone()
            .ok_or_else(|| IslandError::unavailable("Downloads module disabled"))
    }
    pub fn calendar(&self) -> Result<Arc<CalendarService>, IslandError> {
        self.lock()?
            .calendar
            .clone()
            .ok_or_else(|| IslandError::unavailable("Calendar module disabled"))
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        self.transfers.shutdown();
        self.autoclicker.shutdown();
        let mut slots = self.lock()?;
        let mut errors = vec![];
        if let Some(service) = slots.tiles.take() {
            if let Err(error) = service.shutdown() {
                errors.push(error);
            }
        }
        if let Some(service) = slots.updates.take() {
            if let Err(error) = service.shutdown() {
                errors.push(error);
            }
        }
        if let Some(service) = slots.toggles.take() {
            if let Err(error) = service.shutdown() {
                errors.push(error);
            }
        }
        if let Some(service) = slots.mixer.take() {
            if let Err(error) = service.shutdown() {
                errors.push(error);
            }
        }
        if let Some(service) = slots.agents.take() {
            if let Err(error) = service.shutdown() {
                errors.push(error);
            }
        }
        if let Some(service) = slots.brightness.take() {
            if let Err(error) = service.shutdown() {
                errors.push(error);
            }
        }
        if let Some(s) = slots.timer.take() {
            if let Err(e) = s.shutdown() {
                errors.push(e);
            }
        }
        if let Some(s) = slots.scratchpad.take() {
            if let Err(e) = s.shutdown() {
                errors.push(e);
            }
        }
        if let Some(s) = slots.downloads.take() {
            if let Err(e) = s.shutdown() {
                errors.push(e);
            }
        }
        if let Some(s) = slots.calendar.take() {
            if let Err(e) = s.shutdown() {
                errors.push(e);
            }
        }
        match errors.into_iter().next() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod side_button_tests {
    use super::side_button_needs;
    use crate::contract::{island::IslandView, workspace::LayoutAction as A};

    #[test]
    fn quick_actions_keep_their_module_running_and_pages_do_not() {
        assert_eq!(side_button_needs(&[A::PickColour {}]), ["system-tiles"]);
        assert_eq!(
            side_button_needs(&[
                A::Screenshot {},
                A::KeepAwake {},
                A::MicMute {},
                A::CyclePowerProfile {}
            ]),
            ["quick-toggles", "system-tiles"]
        );
        let pages = [
            A::OpenApp {
                view: IslandView::Controls,
            },
            A::ToggleAutoclicker {},
            A::PlayPause {},
        ];
        assert!(side_button_needs(&pages).is_empty());
    }
}

#[cfg(test)]
mod controls_tests {
    use crate::contract::{modules::Registry, settings::Settings};

    fn with_controls(on: bool) -> Settings {
        let defaults = Registry::defaults();
        let modules = defaults
            .modules()
            .iter()
            .cloned()
            .map(|mut m| {
                if m.id.as_str() == "quick-toggles" {
                    m.enabled = on;
                }
                m
            })
            .collect();
        Settings {
            module_registry: Registry::new(crate::contract::modules::CURRENT_VERSION, modules)
                .unwrap(),
            ..Settings::default()
        }
    }

    #[test]
    fn the_controls_app_runs_system_tiles_even_while_that_module_is_off() {
        // Defaults: Controls (quick-toggles) on, system-tiles off; its cards still need it.
        assert!(!super::active(&with_controls(true), "system-tiles"));
        assert!(super::controls_needs(&with_controls(true), "system-tiles"));
        assert!(!super::controls_needs(
            &with_controls(false),
            "system-tiles"
        ));
        assert!(!super::controls_needs(&with_controls(true), "clipboard"));
    }
}

#[cfg(test)]
mod colour_tests {
    #[test]
    fn swatch_is_a_small_png_data_url_and_junk_is_none() {
        let swatch = super::colour_swatch("#3a7bd5").unwrap();
        assert!(swatch.starts_with("data:image/png;base64,iVBOR"));
        assert!(swatch.len() < 4096);
        assert_eq!(super::colour_swatch("3a7bd5"), None);
        assert_eq!(super::colour_swatch("#zz0000"), None);
        assert_eq!(super::colour_swatch("#abc"), None);
    }
}
