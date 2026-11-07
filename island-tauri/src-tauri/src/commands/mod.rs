//! Tauri commands (SPEC §4.3). Blocking work always runs on `spawn_blocking`; every command
//! returns `Result<_, IslandError>`. Keep `lib.rs::specta_builder`, `build.rs::COMMANDS` and
//! `permissions/island.toml` in sync (`command_lists_match` fails otherwise).

pub mod agent_live;
pub mod ai;
pub mod approvals;
pub mod apps;
pub mod clipboard;
pub mod controls;
pub mod desktop_apps;
pub mod fonts;
pub mod frontend;
pub mod island;
pub mod media;
pub mod notifications;
pub mod probe;
pub mod settings;
pub mod system;
pub mod terminal;
pub mod voice;
pub mod workspace;

use crate::error::IslandError;

/// Run blocking work off the async runtime and flatten the join error.
pub(crate) async fn blocking<T, F>(work: F) -> Result<T, IslandError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, IslandError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| IslandError::internal(format!("worker failed: {e}")))?
}
