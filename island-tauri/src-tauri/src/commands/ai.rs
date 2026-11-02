use super::blocking;
use crate::{
    contract::ai::{AiConnectResult, AiProvider, AiUsage},
    error::IslandError,
    services::Services,
};
use std::sync::Arc;

/// Cached 60 s; concurrent callers are coalesced behind one refresh.
#[tauri::command]
#[specta::specta]
pub async fn ai_usage(services: tauri::State<'_, Services>) -> Result<AiUsage, IslandError> {
    let ai = Arc::clone(&services.ai);
    blocking(move || ai.usage()).await
}

/// `provider` is an enum allow-list: anything else is rejected before this runs.
#[tauri::command]
#[specta::specta]
pub async fn ai_connect(
    provider: AiProvider,
    services: tauri::State<'_, Services>,
) -> Result<AiConnectResult, IslandError> {
    let ai = Arc::clone(&services.ai);
    blocking(move || Ok(ai.connect(provider))).await
}
