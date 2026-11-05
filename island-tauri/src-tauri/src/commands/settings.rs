use super::blocking;
use crate::{
    contract::{
        palette::Palette,
        settings::{ObjectOnly, Settings, SettingsPatch},
    },
    error::IslandError,
    services::Services,
    state::host::IslandActor,
    window::ISLAND_LABEL,
};
use std::sync::Arc;
use tauri::Manager;

#[tauri::command]
#[specta::specta]
pub async fn settings_get(services: tauri::State<'_, Services>) -> Result<Settings, IslandError> {
    let settings = Arc::clone(&services.settings);
    blocking(move || settings.get()).await
}

/// Atomic: any invalid key rejects the whole patch. Side effects after success:
/// the taskbar flag is applied to the live window (overlay only) and `SettingsChanged` is
/// published (coalesced, ≤ 100 ms).
#[tauri::command]
#[specta::specta]
pub async fn settings_patch(
    patch: ObjectOnly<SettingsPatch>,
    app: tauri::AppHandle,
    services: tauri::State<'_, Services>,
    actor: tauri::State<'_, IslandActor>,
    notifications: tauri::State<'_, crate::integrations::notifications::hub::Notifications>,
    clipboard: tauri::State<'_, Arc<crate::integrations::clipboard::Clipboard>>,
    system: tauri::State<'_, Arc<crate::integrations::system_services::SystemServices>>,
) -> Result<Settings, IslandError> {
    // Phase 7b: registry edits and launcher commits share one transaction boundary.
    let _workspace_transaction = super::workspace::settings_guard().await;
    let ObjectOnly(patch) = patch;
    let service = Arc::clone(&services.settings);
    let requested = patch.clone();
    let next = blocking(move || service.patch(&requested)).await?;
    if services.overlay && patch.show_in_taskbar.is_some() {
        if let Some(window) = app.get_webview_window(ISLAND_LABEL) {
            window
                .set_skip_taskbar(!next.show_in_taskbar)
                .map_err(|e| IslandError::internal(format!("taskbar update failed: {e}")))?;
        }
    }
    // The state actor owns hoverDelay / clickSounds / toast / dnd decisions: it must see every
    // persisted change before the caller does.
    actor.handle.settings(next.clone()).await?;
    let hub = notifications.hub.clone();
    blocking(move || hub.refresh_policy()).await?;
    let clipboard = clipboard.inner().clone();
    let settings = services.settings.clone();
    blocking(move || clipboard.configure(settings.get()?)).await?;
    let system = system.inner().clone();
    let configured = next.clone();
    blocking(move || system.configure(&configured)).await?;
    services.settings_topic.publish(next.clone());
    Ok(next)
}

/// Nov 5: the schema defaults, so the UI's per-section Reset never duplicates them.
#[tauri::command]
#[specta::specta]
pub async fn settings_defaults() -> Result<Settings, IslandError> {
    Ok(Settings::default())
}

/// Nov 5: connected, enabled output names (`kscreen-doctor -j`) for the notch monitor
/// checklist. Read-only, bounded (3 s, 1 MiB); empty when KScreen is unavailable.
#[tauri::command]
#[specta::specta]
pub async fn display_outputs() -> Result<Vec<String>, IslandError> {
    #[cfg(target_os = "linux")]
    {
        blocking(|| {
            let mut command = std::process::Command::new("kscreen-doctor");
            command.arg("-j");
            let raw = crate::services::process::run_bounded(
                command,
                std::time::Duration::from_secs(3),
                1024 * 1024,
                false,
            )?;
            let value: serde_json::Value = serde_json::from_slice(&raw)
                .map_err(|_| IslandError::invalid("Invalid display helper JSON"))?;
            Ok(output_names(&value))
        })
        .await
    }
    #[cfg(not(target_os = "linux"))]
    Ok(Vec::new())
}

/// Names of connected, enabled outputs that pass the settings' output-name rule, deduped,
/// in KScreen order (at most `MAX_NOTCH_MONITORS`).
pub fn output_names(value: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for output in value["outputs"].as_array().into_iter().flatten() {
        if output["connected"] != true || output["enabled"] == false {
            continue;
        }
        if let Some(name) = output["name"].as_str() {
            if crate::contract::settings::is_output_name(name) && !names.iter().any(|n| n == name) {
                names.push(name.to_owned());
            }
        }
        if names.len() == crate::contract::settings::MAX_NOTCH_MONITORS {
            break;
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::output_names;
    use serde_json::json;

    #[test]
    fn output_names_keep_connected_enabled_valid_unique_outputs() {
        let value = json!({"outputs":[
            {"name":"DP-1","connected":true,"enabled":true},
            {"name":"HDMI-A-1","connected":true},
            {"name":"eDP-1","connected":true,"enabled":false},
            {"name":"DP-2","connected":false,"enabled":true},
            {"name":"bad name;","connected":true,"enabled":true},
            {"name":"DP-1","connected":true,"enabled":true}]});
        assert_eq!(output_names(&value), ["DP-1", "HDMI-A-1"]);
        assert!(output_names(&json!({})).is_empty());
        assert!(output_names(&json!([1, 2])).is_empty());
    }
}

/// Last known palette; computed (bounded) only when nothing is cached yet.
#[tauri::command]
#[specta::specta]
pub async fn palette_get(services: tauri::State<'_, Services>) -> Result<Palette, IslandError> {
    let palette = Arc::clone(&services.palette);
    blocking(move || palette.get()).await
}

/// Nov 2: mirror the island colour scheme into the Hermes skin (`Some`), or restore the skin
/// that was active before (`None`). Returns the active skin name.
#[tauri::command]
#[specta::specta]
pub async fn hermes_theme_sync(
    services: tauri::State<'_, Services>,
    palette: Option<Palette>,
) -> Result<String, IslandError> {
    let service = Arc::clone(&services.palette);
    blocking(move || service.sync_hermes_theme(palette.as_ref())).await
}

/// Nov 2: the island's active colour-scheme roles for the native notch (`None` = follow the
/// Hermes skin). Not persisted: the island page re-sends it on every load.
#[tauri::command]
#[specta::specta]
pub async fn notch_scheme_set(
    app: tauri::AppHandle,
    services: tauri::State<'_, Services>,
    palette: Option<Palette>,
) -> Result<(), IslandError> {
    if let Some(p) = &palette {
        let roles = [
            &p.background,
            &p.accent,
            &p.text,
            &p.primary,
            &p.dim,
            &p.border,
            &p.surface,
            &p.error,
        ];
        if !roles
            .iter()
            .all(|r| crate::contract::settings::is_hex_color(r))
        {
            return Err(IslandError::invalid("Invalid scheme palette"));
        }
    }
    let mut slot = services
        .scheme_palette
        .lock()
        .map_err(|_| IslandError::internal("Scheme palette poisoned"))?;
    if *slot != palette {
        *slot = palette;
        drop(slot);
        crate::integrations::plasma::runtime::nudge_status(&app);
    }
    Ok(())
}
