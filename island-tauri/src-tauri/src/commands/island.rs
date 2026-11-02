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
    services: tauri::State<'_, Services>,
    actor: tauri::State<'_, IslandActor>,
    notifications: tauri::State<'_, crate::integrations::notifications::hub::Notifications>,
    clipboard: tauri::State<'_, Arc<crate::integrations::clipboard::Clipboard>>,
    system: tauri::State<'_, Arc<crate::integrations::system_services::SystemServices>>,
) -> Result<BootstrapSnapshot, IslandError> {
    let settings = Arc::clone(&services.settings);
    let palette = Arc::clone(&services.palette);
    let runtime = services.runtime.clone();
    let state = actor.handle.snapshot().await?.state;
    let hub = notifications.hub.clone();
    let clipboard = clipboard.inner().clone();
    let system = system.inner().clone();
    blocking(move || {
        let monitor = hub.monitor.status();
        let clipboard_status = clipboard.status()?;
        let (power_status, weather_status, bluetooth_status) = (
            system.power.status(),
            system.weather.status(),
            system.bluetooth.status(),
        );
        let bluetooth = system.bluetooth.current();
        let media = system.media.current();
        let (media_status, lyrics_status, equalizer_status) = (
            system.media.status(),
            system.lyrics.status(),
            system.equalizer.status(),
        );
        Ok(BootstrapSnapshot {
            apps: system.apps.snapshot()?,
            runtime,
            state,
            settings: settings.get()?,
            palette: palette.get()?,
            capabilities: ServiceCapabilities {
                notifications: true,
                can_invoke_actions: false,
                system_notification_mirror: monitor.available,
                notification_monitor: monitor,
                clipboard: clipboard_status.available,
                clipboard_status,
                power: power_status.available,
                power_status,
                weather: weather_status.available,
                weather_status,
                bluetooth: bluetooth_status.available,
                bluetooth_status,
                media: media_status.available,
                media_status,
                lyrics_status,
                equalizer_status,
                ..ServiceCapabilities::phase2()
            },
            power: Some(system.power.current()),
            weather: Some(system.weather.current()),
            media: media.now_playing,
            media_rev: media.rev,
            lyrics: Some(system.lyrics.current()),
            bluetooth: bluetooth.devices,
            bluetooth_rev: bluetooth.rev,
            notifications: hub.list()?.into_iter().take(50).collect(),
        })
    })
    .await
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
