//! Controls IPC is additive; no legacy renderer or Phase 7 component changes.
use crate::{
    contract::{
        controls::{
            AgentsChanged, BrightnessChanged, BrightnessOp, MixerChanged, MixerOp, TogglesChanged,
            TogglesOp, UpdateManager, UpdatesChanged, UpdatesOp,
        },
        settings::ObjectOnly,
        tiles::{TilesChanged, TilesOp},
    },
    error::IslandError,
    integrations::system_services::SystemServices,
};
use std::sync::Arc;
#[tauri::command]
#[specta::specta]
pub async fn agents_get(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
) -> Result<AgentsChanged, IslandError> {
    system.apps.agents()?.current()
}
#[tauri::command]
#[specta::specta]
pub async fn brightness_get(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
) -> Result<BrightnessChanged, IslandError> {
    Ok(system.apps.brightness()?.current())
}
#[tauri::command]
#[specta::specta]
pub async fn brightness_command(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
    op: ObjectOnly<BrightnessOp>,
) -> Result<BrightnessChanged, IslandError> {
    system.apps.brightness()?.request(op.0).await
}

#[tauri::command]
#[specta::specta]
pub async fn mixer_get(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
) -> Result<MixerChanged, IslandError> {
    Ok(system.apps.mixer()?.current())
}
#[tauri::command]
#[specta::specta]
pub async fn mixer_command(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
    op: ObjectOnly<MixerOp>,
) -> Result<MixerChanged, IslandError> {
    system.apps.mixer()?.request(op.0).await
}

#[tauri::command]
#[specta::specta]
pub async fn toggles_get(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
) -> Result<TogglesChanged, IslandError> {
    Ok(system.apps.toggles()?.current())
}
#[tauri::command]
#[specta::specta]
pub async fn toggles_command(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
    op: ObjectOnly<TogglesOp>,
) -> Result<TogglesChanged, IslandError> {
    system.apps.toggles()?.request(op.0).await
}

#[tauri::command]
#[specta::specta]
pub async fn updates_get(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
) -> Result<UpdatesChanged, IslandError> {
    Ok(system.apps.updates()?.current())
}
#[tauri::command]
#[specta::specta]
pub async fn updates_refresh(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
) -> Result<UpdatesChanged, IslandError> {
    system.apps.updates()?.request(UpdatesOp::Refresh).await
}

/// Opens Konsole running the full upgrade for `manager`; the list is re-checked once the
/// window closes. Returns as soon as the window is open.
#[tauri::command]
#[specta::specta]
pub async fn updates_install(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
    manager: UpdateManager,
) -> Result<bool, IslandError> {
    let service = system.apps.updates()?;
    let backend = service.current().backend;
    let after = service.clone();
    crate::integrations::updates::install::launch(manager, backend, move || {
        if let Err(error) = tauri::async_runtime::block_on(after.request(UpdatesOp::Refresh)) {
            tracing::debug!(%error, "Re-check after the update window failed");
        }
    })?;
    Ok(true)
}

#[tauri::command]
#[specta::specta]
pub async fn tiles_get(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
) -> Result<TilesChanged, IslandError> {
    Ok(system.apps.tiles()?.current())
}
#[tauri::command]
#[specta::specta]
pub async fn tiles_command(
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
    op: ObjectOnly<TilesOp>,
) -> Result<TilesChanged, IslandError> {
    system.apps.tiles()?.request(op.0).await
}
