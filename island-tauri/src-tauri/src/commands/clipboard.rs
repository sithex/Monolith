//! Clipboard commands use the blocking pool; pause is an actor effect.
use super::blocking;
use crate::{
    contract::clipboard::{ClipContent, ClipSummary},
    error::IslandError,
    integrations::clipboard::Clipboard,
    state::{host::IslandActor, machine::Intent},
};
use std::sync::Arc;

#[tauri::command]
#[specta::specta]
pub async fn clipboard_list(
    query: Option<String>,
    limit: u16,
    clipboard: tauri::State<'_, Arc<Clipboard>>,
) -> Result<Vec<ClipSummary>, IslandError> {
    let service = clipboard.inner().clone();
    blocking(move || service.list(query.as_deref(), limit)).await
}
#[tauri::command]
#[specta::specta]
pub async fn clipboard_get(
    id: String,
    clipboard: tauri::State<'_, Arc<Clipboard>>,
) -> Result<ClipContent, IslandError> {
    let service = clipboard.inner().clone();
    blocking(move || service.get(&id)).await
}
#[tauri::command]
#[specta::specta]
pub async fn clipboard_copy(
    id: String,
    clipboard: tauri::State<'_, Arc<Clipboard>>,
) -> Result<(), IslandError> {
    let service = clipboard.inner().clone();
    blocking(move || service.copy(&id)).await
}
#[tauri::command]
#[specta::specta]
pub async fn clipboard_pin(
    id: String,
    pinned: bool,
    clipboard: tauri::State<'_, Arc<Clipboard>>,
) -> Result<(), IslandError> {
    let service = clipboard.inner().clone();
    blocking(move || service.pin(&id, pinned)).await
}
#[tauri::command]
#[specta::specta]
pub async fn clipboard_delete(
    id: String,
    clipboard: tauri::State<'_, Arc<Clipboard>>,
) -> Result<(), IslandError> {
    let service = clipboard.inner().clone();
    blocking(move || service.delete(&id)).await
}
#[tauri::command]
#[specta::specta]
pub async fn clipboard_clear(
    clipboard: tauri::State<'_, Arc<Clipboard>>,
) -> Result<(), IslandError> {
    let service = clipboard.inner().clone();
    blocking(move || service.clear()).await
}
#[tauri::command]
#[specta::specta]
pub async fn clipboard_set_paused(
    paused: bool,
    actor: tauri::State<'_, IslandActor>,
) -> Result<(), IslandError> {
    actor
        .handle
        .dispatch(Intent::SetClipboardPaused(paused))
        .await?;
    Ok(())
}
