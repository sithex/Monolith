use crate::{error::IslandError, services::Services};

/// Page → Rust answer to a `test-control.sock` `eval`. `unsupported` unless the test hooks are
/// enabled (`--test-instance` and `HERMES_ISLAND_TEST=1`).
#[tauri::command]
#[specta::specta]
pub async fn test_probe_reply(
    id: u32,
    ok: bool,
    value_json: String,
    services: tauri::State<'_, Services>,
) -> Result<(), IslandError> {
    let hooks = services
        .test_hooks
        .lock()
        .map_err(|_| IslandError::internal("test hooks are poisoned"))?
        .clone();
    match hooks {
        Some(hooks) => hooks.reply(id, ok, value_json),
        None => Err(IslandError::unsupported("test hooks are not enabled")),
    }
}
