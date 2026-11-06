//! Phase 6b1 lifecycle/wiring. No legacy view changes. Each app follows moduleRegistry enablement.
use crate::{
    bus::{Latest, COALESCE},
    contract::{apps::*, settings::Settings},
    error::IslandError,
    integrations::{
        agents::{AgentConfig, AgentHubSink, AgentService},
        brightness::BrightnessService,
        calendar::{CalendarConfig, CalendarService, TzConfig},
        control_process::{fixture_from_env, Runner},
        downloads::{DesktopOpener, DownloadsService, Opener},
        mixer::MixerService,
        notifications::{hub::Hub, NotificationInput, NotificationSource, Urgency},
        power::PowerService,
        scratchpad::ScratchpadService,
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
    slots: Mutex<Slots>,
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
        Self {
            game_pause,
            power,
            slots: Mutex::new(Slots::default()),
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
    pub fn configure(&self, settings: &Settings) -> Result<(), IslandError> {
        let mut slots = self.lock()?;
        if active(settings, "timer") {
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
        if active(settings, "scratchpad") {
            if slots.scratchpad.is_none() {
                let scratch = ScratchpadService::new(self.data.clone());
                scratch.start()?;
                self.forward(&scratch.topic);
                slots.scratchpad = Some(scratch);
            }
        } else if let Some(scratch) = slots.scratchpad.take() {
            scratch.shutdown()?;
        }
        if active(settings, "downloads") {
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
        if active(settings, "calendar") {
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
        if active(settings, "system-tiles") {
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
                let clipboard = self
                    .app
                    .state::<Arc<super::clipboard::Clipboard>>()
                    .inner()
                    .clone();
                let service = TilesService::start(
                    Runner::for_instance(self.test_instance, fixture_from_env()),
                    session,
                    system,
                    clipboard,
                );
                self.forward(&service.topic);
                slots.tiles = Some(service);
            }
        } else if let Some(service) = slots.tiles.take() {
            service.shutdown()?;
        }
        if active(settings, "update-badge") {
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
        if active(settings, "quick-toggles") {
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
        if active(settings, "mixer") {
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
        if active(settings, "brightness") {
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
        if active(settings, "agent-tracker") {
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
