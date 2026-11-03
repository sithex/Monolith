use super::blocking;
use crate::{
    contract::ai::{VoiceOp, VoicePanel, VoicePanelOp, VoiceStatus},
    error::IslandError,
    services::Services,
};
use std::sync::Arc;

#[tauri::command]
#[specta::specta]
pub async fn voice_command(
    op: VoiceOp,
    services: tauri::State<'_, Services>,
) -> Result<VoiceStatus, IslandError> {
    let voice = Arc::clone(&services.voice);
    blocking(move || voice.run(op)).await
}

/// Hermes page voice picker + TTS mute (optional helpers; missing ones report unavailable).
#[tauri::command]
#[specta::specta]
pub async fn voice_panel(
    op: VoicePanelOp,
    services: tauri::State<'_, Services>,
) -> Result<VoicePanel, IslandError> {
    let voice = Arc::clone(&services.voice);
    blocking(move || voice.panel(op)).await
}
