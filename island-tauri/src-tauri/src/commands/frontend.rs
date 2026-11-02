use crate::{
    contract::frontend::{Appearance, VisualChanged},
    error::IslandError,
    services::{frontend::VisualService, Services},
    window::ISLAND_LABEL,
};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

#[tauri::command]
#[specta::specta]
pub async fn visual_get(app: AppHandle) -> Result<VisualChanged, IslandError> {
    super::blocking(move || {
        app.state::<VisualService>()
            .snapshot(&app.state::<Services>(), None)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn appearance_set(
    app: AppHandle,
    appearance: Appearance,
) -> Result<VisualChanged, IslandError> {
    super::blocking(move || {
        let snapshot = app
            .state::<VisualService>()
            .snapshot(&app.state::<Services>(), Some(appearance))?;
        snapshot
            .emit_to(&app, ISLAND_LABEL)
            .map_err(|_| IslandError::internal("Cannot publish appearance"))?;
        Ok(snapshot)
    })
    .await
}
