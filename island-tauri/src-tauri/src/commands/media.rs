//! Phase 6m media commands (SPEC §4.3 `media_control`).

use crate::{
    contract::system::MediaCommand, error::IslandError,
    integrations::system_services::SystemServices,
};
use std::sync::Arc;

/// Control the preferred MPRIS player: play / pause / next / previous / seek. Refused (typed
/// error) when nothing is playing or the player does not advertise the capability; reaches
/// no other peer.
#[tauri::command]
#[specta::specta]
pub async fn media_control(
    cmd: MediaCommand,
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
) -> Result<(), IslandError> {
    let media = Arc::clone(&system.media);
    media.control(cmd).await
}

/// The music view is (not) on screen. The equalizer captures audio only while this is true
/// (and it is enabled and a player is playing).
#[tauri::command]
#[specta::specta]
pub async fn media_set_visible(
    visible: bool,
    system: crate::commands::managed::Managed<'_, Arc<SystemServices>>,
) -> Result<(), IslandError> {
    system.equalizer.set_visible(visible);
    Ok(())
}
