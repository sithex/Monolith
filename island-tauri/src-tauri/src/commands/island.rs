use super::blocking;
use crate::{
    contract::{
        island::{BootstrapSnapshot, GeometryAck, GeometryReport, IslandState, UiIntent},
        system::ServiceCapabilities,
    },
    error::IslandError,
    services::Services,
    state::host::IslandActor,
    window::IslandRuntime,
};
use std::sync::Arc;

/// Full snapshot on connect (SPEC §5.3). `state` is the state actor's current value (the
/// single authority); a palette failure is an explicit error, not a silently incomplete
/// successful bootstrap.
#[tauri::command]
#[specta::specta]
pub async fn island_bootstrap(
    app: tauri::AppHandle,
    services: tauri::State<'_, Services>,
    actor: tauri::State<'_, IslandActor>,
) -> Result<BootstrapSnapshot, IslandError> {
    use tauri::Manager;
    let settings = Arc::clone(&services.settings);
    let palette = Arc::clone(&services.palette);
    let runtime = services.runtime.clone();
    let state = actor.handle.snapshot().await?.state;
    // Optional subsystems that failed to start are reported unavailable, never a failed boot.
    let notifications = app
        .try_state::<crate::integrations::notifications::hub::Notifications>()
        .map(|n| (n.hub.clone(), n.status.clone()));
    let clipboard = app
        .try_state::<Arc<crate::integrations::clipboard::Clipboard>>()
        .map(|c| c.inner().clone());
    let system = app
        .try_state::<Arc<crate::integrations::system_services::SystemServices>>()
        .map(|s| s.inner().clone());
    blocking(move || {
        let parts = optional_parts(notifications, clipboard.as_deref(), system)?;
        Ok(BootstrapSnapshot {
            apps: parts.apps,
            runtime,
            state,
            settings: settings.get()?,
            palette: palette.get()?,
            capabilities: parts.capabilities,
            power: parts.power,
            weather: parts.weather,
            media: parts.media,
            media_rev: parts.media_rev,
            lyrics: parts.lyrics,
            bluetooth: parts.bluetooth,
            bluetooth_rev: parts.bluetooth_rev,
            notifications: parts.notifications,
        })
    })
    .await
}

/// The boot snapshot fields that come from the optional subsystems.
pub struct OptionalParts {
    pub apps: crate::contract::apps::AppsSnapshot,
    pub capabilities: ServiceCapabilities,
    pub power: Option<crate::contract::system::PowerState>,
    pub weather: Option<crate::contract::system::WeatherState>,
    pub media: Option<crate::contract::system::NowPlaying>,
    pub media_rev: u32,
    pub lyrics: Option<crate::contract::system::LyricsChanged>,
    pub bluetooth: Vec<crate::contract::system::BtDevice>,
    pub bluetooth_rev: u32,
    pub notifications: Vec<crate::contract::notify::IslandNotification>,
}

/// Everything in the boot snapshot that comes from the optional subsystems. A missing one
/// reads `available: false` with `start-failed`, an empty list or `null`.
pub fn optional_parts(
    notifications: Option<(
        Arc<crate::integrations::notifications::hub::Hub>,
        Arc<crate::integrations::service_status::Reporter>,
    )>,
    clipboard: Option<&crate::integrations::clipboard::Clipboard>,
    system: Option<Arc<crate::integrations::system_services::SystemServices>>,
) -> Result<OptionalParts, IslandError> {
    use crate::services::degraded::start_failed;
    let clipboard_status = match clipboard {
        Some(clipboard) => clipboard.status()?,
        None => crate::contract::clipboard::ClipboardStatus {
            detail: crate::services::degraded::START_FAILED.into(),
            ..Default::default()
        },
    };
    let (monitor, notifications_status, items) = match &notifications {
        Some((hub, status)) => (
            hub.monitor.status(),
            status.get(),
            hub.list()?.into_iter().take(50).collect(),
        ),
        None => (
            Default::default(),
            start_failed("notifications"),
            Vec::new(),
        ),
    };
    let mut snapshot = OptionalParts {
        apps: Default::default(),
        capabilities: ServiceCapabilities {
            notifications: notifications.is_some(),
            notifications_status,
            can_invoke_actions: false,
            system_notification_mirror: monitor.available,
            notification_monitor: monitor,
            clipboard: clipboard_status.available,
            clipboard_status,
            power_status: start_failed("power"),
            weather_status: start_failed("weather"),
            bluetooth_status: start_failed("bluetooth"),
            media_status: start_failed("media"),
            lyrics_status: start_failed("lyrics"),
            equalizer_status: start_failed("equalizer"),
            ..ServiceCapabilities::phase2()
        },
        power: None,
        weather: None,
        media: None,
        media_rev: 0,
        lyrics: None,
        bluetooth: Vec::new(),
        bluetooth_rev: 0,
        notifications: items,
    };
    if let Some(system) = system {
        let caps = &mut snapshot.capabilities;
        let (power_status, weather_status, bluetooth_status) = (
            system.power.status(),
            system.weather.status(),
            system.bluetooth.status(),
        );
        let (media_status, lyrics_status, equalizer_status) = (
            system.media.status(),
            system.lyrics.status(),
            system.equalizer.status(),
        );
        caps.power = power_status.available;
        caps.power_status = power_status;
        caps.weather = weather_status.available;
        caps.weather_status = weather_status;
        caps.bluetooth = bluetooth_status.available;
        caps.bluetooth_status = bluetooth_status;
        caps.media = media_status.available;
        caps.media_status = media_status;
        caps.lyrics_status = lyrics_status;
        caps.equalizer_status = equalizer_status;
        let bluetooth = system.bluetooth.current();
        let media = system.media.current();
        snapshot.apps = system.apps.snapshot()?;
        snapshot.power = Some(system.power.current());
        snapshot.weather = Some(system.weather.current());
        snapshot.media = media.now_playing;
        snapshot.media_rev = media.rev;
        snapshot.lyrics = Some(system.lyrics.current());
        snapshot.bluetooth = bluetooth.devices;
        snapshot.bluetooth_rev = bluetooth.rev;
    }
    Ok(snapshot)
}

/// Frontend intent → state actor (SPEC §4.3). The reply is the state after the intent; the
/// store still only advances from `IslandStateChanged`, so the reply is informational.
#[tauri::command]
#[specta::specta]
pub async fn island_dispatch(
    intent: UiIntent,
    actor: tauri::State<'_, IslandActor>,
) -> Result<IslandState, IslandError> {
    Ok(actor.handle.dispatch_ui(intent.into()).await?.state)
}

/// Frontend → Rust geometry report (SPEC §4.4). Never blocks the async runtime: the controller
/// waits on the GTK main thread, so it runs on a blocking worker.
///
/// A report is accepted only when its `stateRev` equals the actor's CURRENT revision and its
/// mode agrees with the actor's mode; anything else is `applied: false` (stale, not an error).
/// That strict rule applies to the product page (`--ui legacy`); the earlier-phase harness and
/// spike fixture pages simulate their own state and keep the controller-only checks.
/// Several reports per revision are legal (the controller re-animates layout-only changes).
#[tauri::command]
#[specta::specta]
pub async fn island_report_geometry(
    app: tauri::AppHandle,
    report: GeometryReport,
    runtime: tauri::State<'_, IslandRuntime>,
    actor: tauri::State<'_, IslandActor>,
) -> Result<GeometryAck, IslandError> {
    let region = Arc::clone(&runtime.region);
    let handle = actor.handle.clone();
    let actor_geometry = runtime.actor_geometry;
    blocking(move || {
        let mut controller = region
            .lock()
            .map_err(|_| IslandError::internal("input region controller is poisoned"))?;
        // Read the authority AFTER taking the region lock: reports serialize here, and the
        // actor publishes its state before it emits the event the renderer reacts to.
        let current = handle.state();
        if actor_geometry && (report.state_rev != current.rev || report.mode != current.mode) {
            return Ok(GeometryAck {
                applied: false,
                rows: controller.applied_row_count(),
            });
        }
        let ack = controller.report(&report)?;
        drop(controller);
        if ack.applied {
            crate::integrations::plasma::runtime::nudge_status(&app);
        }
        Ok(ack)
    })
    .await
}
