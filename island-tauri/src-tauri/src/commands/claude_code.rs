use super::blocking;
use crate::{
    contract::claude_code::{ClaudeCodeRaw, ClaudeCodeRequest},
    error::IslandError,
    services::Services,
};
use std::sync::Arc;

/// One read-only, bounded poll of Claude Code registry and transcripts.
#[tauri::command]
#[specta::specta]
pub async fn claude_code_raw(
    req: ClaudeCodeRequest,
    services: tauri::State<'_, Services>,
) -> Result<ClaudeCodeRaw, IslandError> {
    let svc = Arc::clone(&services.claude_code);
    blocking(move || Ok(svc.raw(&req))).await
}
