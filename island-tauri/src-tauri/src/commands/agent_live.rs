use super::blocking;
use crate::{
    contract::agent_live::{AgentLiveBlocks, AgentLiveBlocksOp, AgentLiveRaw, AgentLiveRequest},
    error::IslandError,
    services::Services,
};
use std::sync::Arc;

/// One read-only poll of the Hermes home for the AI Agents Live page (docs/agent-live.md).
#[tauri::command]
#[specta::specta]
pub async fn agent_live_raw(
    req: AgentLiveRequest,
    services: tauri::State<'_, Services>,
) -> Result<AgentLiveRaw, IslandError> {
    let live = Arc::clone(&services.agent_live);
    let terminal_pid = services.terminal.pid();
    blocking(move || Ok(live.raw(&req, terminal_pid))).await
}

/// Get / replace the persisted 5-hour block history (newest 200 kept).
#[tauri::command]
#[specta::specta]
pub async fn agent_live_blocks(
    op: AgentLiveBlocksOp,
    services: tauri::State<'_, Services>,
) -> Result<AgentLiveBlocks, IslandError> {
    let live = Arc::clone(&services.agent_live);
    blocking(move || live.blocks(op)).await
}
