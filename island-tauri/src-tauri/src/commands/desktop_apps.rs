use super::blocking;
use crate::{contract::desktop_apps::DesktopApp, error::IslandError, services::desktop_apps};

/// Enumerate only when the action picker opens; no startup hook, watcher, or polling task.
#[tauri::command]
#[specta::specta]
pub async fn desktop_apps_list() -> Result<Vec<DesktopApp>, IslandError> {
    blocking(|| Ok(desktop_apps::list())).await
}

/// Request an installed application launch, not arbitrary command execution.
#[tauri::command]
#[specta::specta]
pub async fn desktop_app_launch(desktop_id: String) -> Result<(), IslandError> {
    blocking(move || desktop_apps::launch(&desktop_id)).await
}
