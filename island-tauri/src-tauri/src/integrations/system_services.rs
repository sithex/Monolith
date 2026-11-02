//! Lifecycle wiring of the Phase 6a OS-integration services: power (UPower), weather
//! (Open-Meteo) and Bluetooth (BlueZ). Each owns its worker, its latest-value topics and its
//! availability report; this type starts them, forwards their topics to the webview and stops
//! them. Battery alerts become `NotificationStore` system alerts (SPEC §3.5).

use crate::{
    bus::COALESCE,
    contract::{
        events::{PowerChanged, WeatherChanged},
        settings::Settings,
        system::{BluetoothChanged, EqualizerChanged, LyricsChanged, MediaChanged, ServiceStatus},
    },
    error::IslandError,
    integrations::{
        bluetooth::BluetoothService,
        media::{
            equalizer::{EqConfig, EqualizerService, MIN_EMIT_GAP},
            lyrics::{LyricsConfig, LyricsService},
            MediaService,
        },
        notifications::{hub::Hub, NotificationInput, NotificationSource, Urgency},
        power::{model::AlertKind, AlertSettings, PowerService},
        weather::{WeatherConfig, WeatherService, WeatherSettings},
    },
    window::ISLAND_LABEL,
};
use std::{path::Path, sync::Arc};
use tauri_specta::Event;

pub const DEFAULT_SYSTEM_BUS: &str = "unix:path=/run/dbus/system_bus_socket";

/// The session bus MPRIS is read from. An explicit `DBUS_SESSION_BUS_ADDRESS` wins; a test
/// instance WITHOUT one never falls back to the user's real session bus (tests must never
/// talk to real players); a real run uses the default per-user socket.
pub fn session_bus_address(
    test_instance: bool,
    env: Option<String>,
    runtime_dir: Option<String>,
) -> Option<String> {
    match env.filter(|v| !v.trim().is_empty()) {
        Some(address) => Some(address),
        None if test_instance => None,
        None => runtime_dir
            .filter(|d| !d.trim().is_empty())
            .map(|dir| format!("unix:path={dir}/bus")),
    }
}

pub const ENV_MPRIS_BUS: &str = "HERMES_ISLAND_MPRIS_BUS";

/// MPRIS bus: a TEST instance may point it at a private mock bus
/// (`HERMES_ISLAND_MPRIS_BUS`, ignored by real runs, so a stray variable can never redirect
/// the user's players); otherwise the session bus above.
pub fn mpris_bus_address(
    test_instance: bool,
    override_env: Option<String>,
    session_env: Option<String>,
    runtime_dir: Option<String>,
) -> Option<String> {
    let override_env = override_env.filter(|v| test_instance && !v.trim().is_empty());
    override_env.or_else(|| session_bus_address(test_instance, session_env, runtime_dir))
}

/// The system bus a service may use. An explicit `DBUS_SYSTEM_BUS_ADDRESS` wins. A test
/// instance WITHOUT one never falls back to the machine's real system bus (SPEC rule 2: tests
/// never call UPower/BlueZ for real): its power/Bluetooth services stay unavailable.
pub fn system_bus_address(test_instance: bool, env: Option<String>) -> Option<String> {
    match env.filter(|v| !v.trim().is_empty()) {
        Some(address) => Some(address),
        None if test_instance => None,
        None => Some(DEFAULT_SYSTEM_BUS.into()),
    }
}

/// Battery alert → store input. Critical is urgent (resident until dismissed).
pub fn battery_alert(kind: AlertKind, percent: Option<f64>) -> NotificationInput {
    let (title, urgency) = match kind {
        AlertKind::Low => ("Battery low", Urgency::Normal),
        AlertKind::Critical => ("Battery critically low", Urgency::Critical),
        AlertKind::Full => ("Battery full", Urgency::Normal),
    };
    let body = match (kind, percent) {
        (AlertKind::Full, _) => Some("Fully charged".to_owned()),
        (_, Some(p)) => Some(format!("{p:.0}% remaining")),
        (_, None) => None,
    };
    NotificationInput {
        source: NotificationSource::Alert,
        app_name: "Battery".into(),
        title: title.into(),
        body,
        icon_png: None,
        urgency,
        actions: vec![],
        progress: None,
        // `dedupeKey` is Hermes-only in the store; alerts fire once per crossing anyway.
        dedupe_key: None,
        server_id: None,
        replaces_id: 0,
        transient: false,
        ttl_ms: None,
    }
}

pub struct SystemServices {
    pub apps: super::apps::AppServices,
    pub power: Arc<PowerService>,
    pub weather: Arc<WeatherService>,
    pub bluetooth: Arc<BluetoothService>,
    pub media: Arc<MediaService>,
    pub lyrics: Arc<LyricsService>,
    pub equalizer: Arc<EqualizerService>,
}

impl SystemServices {
    pub fn start(
        app: &tauri::AppHandle,
        settings: &Settings,
        test_instance: bool,
        hub: Arc<Hub>,
        data_dir: &Path,
    ) -> Result<Self, IslandError> {
        let bus = system_bus_address(test_instance, std::env::var("DBUS_SYSTEM_BUS_ADDRESS").ok());
        let sink_hub = Arc::clone(&hub);
        let power = PowerService::new(
            AlertSettings::from(settings),
            Arc::new(move |kind, snapshot| {
                if let Err(error) = sink_hub.insert(battery_alert(kind, snapshot.percentage())) {
                    tracing::warn!(%error, "Battery alert was not stored");
                }
            }),
        );
        let weather = WeatherService::new(
            WeatherConfig::for_instance(test_instance, |name| std::env::var(name).ok()),
            WeatherSettings::from(settings),
        )?;
        let bluetooth = BluetoothService::new();
        let media = MediaService::new();
        let lyrics = LyricsService::new(
            LyricsConfig::for_instance(
                test_instance,
                Some(data_dir.join("lyrics-cache")),
                |name| std::env::var(name).ok(),
            ),
            settings.lyrics_enabled,
        )
        .map_err(|_| IslandError::internal("Lyrics HTTP client unavailable"))?;
        let equalizer = EqualizerService::new(
            EqConfig::default(),
            settings.equalizer_enabled,
            settings.equalizer_bands as usize,
        );
        Self::forward(app, &power, &weather, &bluetooth);
        Self::forward_music(app, &media, &lyrics, &equalizer);
        let apps = super::apps::AppServices::new(app, data_dir, test_instance, hub, power.clone());
        apps.configure(settings)?;
        power.start(bus.clone());
        bluetooth.start(bus);
        weather.start();
        media.start(mpris_bus_address(
            test_instance,
            std::env::var(ENV_MPRIS_BUS).ok(),
            std::env::var("DBUS_SESSION_BUS_ADDRESS").ok(),
            std::env::var("XDG_RUNTIME_DIR").ok(),
        ));
        lyrics.start(Arc::clone(&media));
        equalizer.start();
        Ok(Self {
            apps,
            power,
            weather,
            bluetooth,
            media,
            lyrics,
            equalizer,
        })
    }

    /// Media, lyrics and equalizer topics → the webview, and the playing player → the
    /// equalizer (capture follows playback).
    fn forward_music(
        app: &tauri::AppHandle,
        media: &Arc<MediaService>,
        lyrics: &Arc<LyricsService>,
        equalizer: &Arc<EqualizerService>,
    ) {
        let handle = app.clone();
        tauri::async_runtime::spawn(media.topic.forwarder(COALESCE, move |state: MediaChanged| {
            if let Err(error) = state.emit_to(&handle, ISLAND_LABEL) {
                tracing::warn!(%error, "MediaChanged emit failed");
            }
        }));
        let handle = app.clone();
        tauri::async_runtime::spawn(lyrics.topic.forwarder(
            COALESCE,
            move |state: LyricsChanged| {
                if let Err(error) = state.emit_to(&handle, ISLAND_LABEL) {
                    tracing::warn!(%error, "LyricsChanged emit failed");
                }
            },
        ));
        // ≤ 30 Hz: the forwarder sleeps one gap after every emit.
        let handle = app.clone();
        tauri::async_runtime::spawn(equalizer.topic.forwarder(
            MIN_EMIT_GAP,
            move |state: EqualizerChanged| {
                if let Err(error) = state.emit_to(&handle, ISLAND_LABEL) {
                    tracing::warn!(%error, "EqualizerChanged emit failed");
                }
            },
        ));
        for (name, topic) in [
            ("media", &media.status.topic),
            ("lyrics", &lyrics.status.topic),
            ("equalizer", &equalizer.status.topic),
        ] {
            let handle = app.clone();
            tauri::async_runtime::spawn(topic.forwarder(COALESCE, move |status: ServiceStatus| {
                if let Err(error) = status.emit_to(&handle, ISLAND_LABEL) {
                    tracing::warn!(%error, service = name, "ServiceStatus emit failed");
                }
            }));
        }
        let mut changes = media.topic.subscribe();
        let (media, equalizer) = (Arc::clone(media), Arc::clone(equalizer));
        tauri::async_runtime::spawn(async move {
            equalizer.set_source(media.playing_hint());
            while changes.changed().await.is_ok() {
                changes.borrow_and_update();
                equalizer.set_source(media.playing_hint());
            }
        });
    }

    fn forward(
        app: &tauri::AppHandle,
        power: &Arc<PowerService>,
        weather: &Arc<WeatherService>,
        bluetooth: &Arc<BluetoothService>,
    ) {
        let handle = app.clone();
        tauri::async_runtime::spawn(power.topic.forwarder(COALESCE, move |state| {
            if let Err(error) = PowerChanged(state).emit_to(&handle, ISLAND_LABEL) {
                tracing::warn!(%error, "PowerChanged emit failed");
            }
        }));
        let handle = app.clone();
        tauri::async_runtime::spawn(weather.topic.forwarder(COALESCE, move |state| {
            if let Err(error) = WeatherChanged(state).emit_to(&handle, ISLAND_LABEL) {
                tracing::warn!(%error, "WeatherChanged emit failed");
            }
        }));
        let handle = app.clone();
        tauri::async_runtime::spawn(bluetooth.topic.forwarder(
            COALESCE,
            move |devices: BluetoothChanged| {
                if let Err(error) = devices.emit_to(&handle, ISLAND_LABEL) {
                    tracing::warn!(%error, "BluetoothChanged emit failed");
                }
            },
        ));
        for (name, topic) in [
            ("power", &power.status.topic),
            ("weather", &weather.status.topic),
            ("bluetooth", &bluetooth.status.topic),
        ] {
            let handle = app.clone();
            tauri::async_runtime::spawn(topic.forwarder(COALESCE, move |status: ServiceStatus| {
                if let Err(error) = status.emit_to(&handle, ISLAND_LABEL) {
                    tracing::warn!(%error, service = name, "ServiceStatus emit failed");
                }
            }));
        }
    }

    /// A settings change reached the persisted store: tell each service.
    pub fn configure(&self, settings: &Settings) -> Result<(), IslandError> {
        self.apps.configure(settings)?;
        self.power.configure(AlertSettings::from(settings));
        self.weather.configure(WeatherSettings::from(settings));
        self.lyrics.configure(settings.lyrics_enabled);
        self.equalizer.configure(
            settings.equalizer_enabled,
            settings.equalizer_bands as usize,
        );
        Ok(())
    }

    pub fn shutdown(&self) {
        if let Err(error) = self.apps.shutdown() {
            tracing::error!(%error, "App shutdown failed");
        }
        self.equalizer.shutdown();
        self.lyrics.shutdown();
        self.media.shutdown();
        self.power.shutdown();
        self.bluetooth.shutdown();
        self.weather.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpris_bus_override_applies_to_test_instances_only_and_tests_never_use_the_real_bus() {
        let some = |s: &str| Some(s.to_owned());
        // Test instance: override > explicit session bus > nothing (never the real bus).
        assert_eq!(
            mpris_bus_address(
                true,
                some("unix:path=/mock"),
                some("unix:path=/s"),
                some("/run/user/1")
            )
            .as_deref(),
            Some("unix:path=/mock")
        );
        assert_eq!(
            mpris_bus_address(true, None, some("unix:path=/s"), some("/run/user/1")).as_deref(),
            Some("unix:path=/s")
        );
        assert_eq!(
            mpris_bus_address(true, None, None, some("/run/user/1")),
            None
        );
        assert_eq!(
            mpris_bus_address(true, some("  "), None, some("/run/user/1")),
            None
        );
        // A real run ignores the override and uses the per-user socket.
        assert_eq!(
            mpris_bus_address(false, some("unix:path=/mock"), None, some("/run/user/1000"))
                .as_deref(),
            Some("unix:path=/run/user/1000/bus")
        );
        assert_eq!(mpris_bus_address(false, None, None, None), None);
    }

    #[test]
    fn test_instances_never_fall_back_to_the_real_system_bus() {
        assert_eq!(system_bus_address(true, None), None);
        assert_eq!(system_bus_address(true, Some(String::new())), None);
        assert_eq!(system_bus_address(true, Some("  ".into())), None);
        assert_eq!(
            system_bus_address(true, Some("unix:path=/x/bus".into())).as_deref(),
            Some("unix:path=/x/bus")
        );
        assert_eq!(
            system_bus_address(false, None).as_deref(),
            Some(DEFAULT_SYSTEM_BUS)
        );
    }

    #[test]
    fn alerts_are_accepted_system_alerts_by_the_real_store() {
        use crate::integrations::notifications::NotificationStore;
        let low = battery_alert(AlertKind::Low, Some(19.6));
        assert_eq!(low.source, NotificationSource::Alert);
        assert_eq!(low.body.as_deref(), Some("20% remaining"));
        let critical = battery_alert(AlertKind::Critical, Some(9.0));
        assert_eq!(critical.urgency, Urgency::Critical);
        assert_eq!(
            battery_alert(AlertKind::Full, None).body.as_deref(),
            Some("Fully charged")
        );
        assert_eq!(battery_alert(AlertKind::Low, None).body, None);
        // The store validates every insert (e.g. `dedupeKey` is Hermes-only): a shape it
        // would reject must fail HERE, not silently in the running app.
        let mut store = NotificationStore::default();
        for (kind, percent) in [
            (AlertKind::Low, Some(19.0)),
            (AlertKind::Critical, Some(9.0)),
            (AlertKind::Critical, None),
            (AlertKind::Full, Some(100.0)),
        ] {
            let changes = store
                .insert(battery_alert(kind, percent), 1_000.0)
                .unwrap_or_else(|e| panic!("{kind:?} rejected: {e:?}"));
            assert!(!changes.events.is_empty(), "{kind:?}");
        }
    }
}
