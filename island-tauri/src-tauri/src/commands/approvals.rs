use super::blocking;
use crate::{
    contract::approvals::{ApprovalAnswer, ApprovalChanged},
    error::IslandError,
    services::Services,
};
use std::sync::Arc;

/// The prompt to show right now (the page also hears every change as `ApprovalChanged`).
#[tauri::command]
#[specta::specta]
pub async fn approval_get(
    services: tauri::State<'_, Services>,
) -> Result<ApprovalChanged, IslandError> {
    Ok(services.approvals.snapshot())
}

/// Answer the prompt the page saw. The prompt id must still be the pending one; every guard
/// is re-checked in the service before the quick-pick digit is typed into the PTY.
#[tauri::command]
#[specta::specta]
pub async fn approval_answer(
    answer: ApprovalAnswer,
    services: tauri::State<'_, Services>,
) -> Result<ApprovalChanged, IslandError> {
    let approvals = Arc::clone(&services.approvals);
    let terminal = Arc::clone(&services.terminal);
    blocking(move || approvals.answer(&answer.prompt_id, &answer.choice, terminal.as_ref())).await
}
