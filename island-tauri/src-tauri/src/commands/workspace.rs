//! Thin IPC adapters. Launcher commit reuses the existing settings_patch side effects.
use super::blocking;
use crate::{
    contract::{settings::ObjectOnly, workspace::*},
    error::IslandError,
    services::{workspace::WorkspaceService, Services},
    window::ISLAND_LABEL,
};
use std::{future::Future, sync::Arc};
use tauri::Manager;
use tauri_specta::Event;

static SETTINGS_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
tokio::task_local! { static SETTINGS_TRANSACTION: (); }

/// settings_patch takes this guard too. The only re-entry is its direct call from a scoped
/// launcher transaction, on the same task. Spawned tasks never inherit the scope.
pub(crate) async fn settings_guard() -> Option<tokio::sync::MutexGuard<'static, ()>> {
    if SETTINGS_TRANSACTION.try_with(|()| ()).is_ok() {
        None
    } else {
        Some(SETTINGS_GATE.lock().await)
    }
}
pub async fn serialized_settings<F: Future>(future: F) -> F::Output {
    let _guard = SETTINGS_GATE.lock().await;
    SETTINGS_TRANSACTION.scope((), future).await
}

#[tauri::command]
#[specta::specta]
pub async fn launcher_get(app: tauri::AppHandle) -> Result<LauncherChanged, IslandError> {
    let _guard = settings_guard().await;
    blocking(move || {
        app.state::<Arc<WorkspaceService>>()
            .launcher_get(&app.state::<Services>().settings.get()?.module_registry)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn launcher_command(
    op: ObjectOnly<LauncherOp>,
    app: tauri::AppHandle,
) -> Result<LauncherChanged, IslandError> {
    serialized_settings(async move {
        let ObjectOnly(op) = op;
        let worker = app.clone();
        let snapshot = if matches!(op, LauncherOp::Commit {}) {
            let patch = blocking(move || {
                let current = worker.state::<Services>().settings.get()?;
                worker
                    .state::<Arc<WorkspaceService>>()
                    .prepare_commit(&current.module_registry)
            })
            .await?;
            if let Some(patch) = patch {
                // No std guard is held. This is the exact existing one-write path, including
                // actor, notification, clipboard and system reconfigure + SettingsChanged.
                let persisted = super::settings::settings_patch(
                    ObjectOnly(patch),
                    app.clone(),
                    app.state::<Services>(),
                    app.state::<crate::state::host::IslandActor>(),
                )
                .await?;
                let worker = app.clone();
                blocking(move || {
                    worker
                        .state::<Arc<WorkspaceService>>()
                        .committed(&persisted.module_registry)
                })
                .await?
            } else {
                let worker = app.clone();
                blocking(move || {
                    worker
                        .state::<Arc<WorkspaceService>>()
                        .launcher_get(&worker.state::<Services>().settings.get()?.module_registry)
                })
                .await?
            }
        } else {
            blocking(move || {
                worker.state::<Arc<WorkspaceService>>().launcher_command(
                    &worker.state::<Services>().settings.get()?.module_registry,
                    &op,
                )
            })
            .await?
        };
        snapshot
            .emit_to(&app, ISLAND_LABEL)
            .map_err(|_| IslandError::internal("Cannot publish launcher snapshot"))?;
        Ok(snapshot)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn layout_get(
    workspace: tauri::State<'_, Arc<WorkspaceService>>,
) -> Result<LayoutChanged, IslandError> {
    let workspace = workspace.inner().clone();
    blocking(move || workspace.layout_get()).await
}

#[tauri::command]
#[specta::specta]
pub async fn layout_command(
    op: ObjectOnly<LayoutOp>,
    app: tauri::AppHandle,
) -> Result<LayoutChanged, IslandError> {
    let workspace = app.state::<Arc<WorkspaceService>>().inner().clone();
    let snapshot = blocking(move || workspace.layout_command(op.0)).await?;
    snapshot
        .emit_to(&app, ISLAND_LABEL)
        .map_err(|_| IslandError::internal("Cannot publish layout snapshot"))?;
    Ok(snapshot)
}

#[tauri::command]
#[specta::specta]
pub async fn layout_geometry(
    query: ObjectOnly<LayoutGeometryQuery>,
    workspace: tauri::State<'_, Arc<WorkspaceService>>,
) -> Result<LayoutGeometry, IslandError> {
    let workspace = workspace.inner().clone();
    blocking(move || workspace.layout_geometry(query.0)).await
}

/// External registry changes invalidate the edit draft and publish a fresh authoritative value.
/// Delayed settings wakeups sample current state rather than replaying an obsolete registry.
pub fn publish(app: &tauri::AppHandle) {
    if app.state::<Services>().runtime.ui != crate::contract::island::UiKind::Island {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match launcher_get(app.clone()).await {
            Ok(snapshot) => {
                if let Err(error) = snapshot.emit_to(&app, ISLAND_LABEL) {
                    tracing::warn!(%error, "LauncherChanged emit failed");
                }
            }
            Err(error) => tracing::warn!(%error, "Launcher snapshot unavailable"),
        }
    });
}
