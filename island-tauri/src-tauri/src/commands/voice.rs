use super::blocking;
use crate::{
    contract::ai::{VoiceOp, VoiceStatus},
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
