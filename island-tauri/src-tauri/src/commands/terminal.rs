use super::blocking;
use crate::{
    contract::terminal::{TerminalEvent, TerminalSnapshot},
    error::IslandError,
    services::{terminal::TerminalSink, Services},
};
use std::sync::Arc;
use tauri::ipc::Channel;

/// Spawn the PTY on first call; later calls only replace the output channel (idempotent).
/// Returns the snapshot (replay + `seq`); the channel then carries only newer chunks.
#[tauri::command]
#[specta::specta]
pub async fn terminal_attach(
    on_event: Channel<TerminalEvent>,
    services: tauri::State<'_, Services>,
) -> Result<TerminalSnapshot, IslandError> {
    let terminal = Arc::clone(&services.terminal);
    let sink: Arc<dyn TerminalSink> =
        Arc::new(move |event: TerminalEvent| on_event.send(event).is_ok());
    blocking(move || terminal.attach(sink)).await
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_input(
    data: String,
    services: tauri::State<'_, Services>,
) -> Result<(), IslandError> {
    let terminal = Arc::clone(&services.terminal);
    blocking(move || terminal.input(&data)).await
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_resize(
    cols: u16,
    rows: u16,
    services: tauri::State<'_, Services>,
) -> Result<(), IslandError> {
    let terminal = Arc::clone(&services.terminal);
    blocking(move || terminal.resize(cols, rows)).await
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_restart(
    services: tauri::State<'_, Services>,
) -> Result<TerminalSnapshot, IslandError> {
    let terminal = Arc::clone(&services.terminal);
    blocking(move || terminal.restart()).await
}

/// The renderer has parsed everything up to `seq`: grant flow-control credit. Unknown or stale
/// sequence numbers are ignored, not errors.
#[tauri::command]
#[specta::specta]
pub async fn terminal_ack(
    seq: u32,
    services: tauri::State<'_, Services>,
) -> Result<(), IslandError> {
    services.terminal.ack(seq);
    Ok(())
}
