use super::blocking;
use crate::{
    contract::notify::IslandNotification, error::IslandError,
    integrations::notifications::hub::Notifications,
};
#[tauri::command]
#[specta::specta]
pub async fn notifications_list(
    notifications: tauri::State<'_, Notifications>,
) -> Result<Vec<IslandNotification>, IslandError> {
    let hub = notifications.hub.clone();
    blocking(move || hub.list()).await
}
#[tauri::command]
#[specta::specta]
pub async fn notifications_dismiss(
    id: String,
    notifications: tauri::State<'_, Notifications>,
) -> Result<(), IslandError> {
    if id.is_empty() || id.len() > 256 {
        return Err(IslandError::invalid("Invalid notification id"));
    }
    let hub = notifications.hub.clone();
    blocking(move || hub.dismiss(&id)).await
}
#[tauri::command]
#[specta::specta]
pub async fn notifications_clear(
    notifications: tauri::State<'_, Notifications>,
) -> Result<(), IslandError> {
    let hub = notifications.hub.clone();
    blocking(move || hub.clear()).await
}
