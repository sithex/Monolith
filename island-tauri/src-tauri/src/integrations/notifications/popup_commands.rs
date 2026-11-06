use super::{hub::Notifications, popups::NotificationPopups};
use crate::error::IslandError;

#[derive(Clone, serde::Deserialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PopupOp {
    List,
    Dismiss { id: String },
    Pause { id: String, paused: bool },
    Invoke { id: String, key: String },
    Reply { id: String, text: String },
}
/// Popup dismissal is presentation-only. History dismissal remains a separate command.
#[tauri::command]
#[specta::specta]
pub async fn notification_popup(
    op: PopupOp,
    notifications: tauri::State<'_, Notifications>,
) -> Result<NotificationPopups, IslandError> {
    let hub = notifications.hub.clone();
    match op {
        PopupOp::List => {}
        PopupOp::Dismiss { id } => hub.popup_change(&id, None)?,
        PopupOp::Pause { id, paused } => hub.popup_change(&id, Some(paused))?,
        PopupOp::Reply { .. } => {
            return Err(IslandError::unsupported(
                "Plasma exposes no third-party inline-reply method; open the sending app to reply",
            ))
        }
        PopupOp::Invoke { id, key } => {
            let route = hub.action_route(&id, &key)?;
            #[cfg(target_os = "linux")]
            {
                let invoked = if route.live && (key != "default" || route.default_action) {
                    hub.monitor
                        .invoke(Some(&route.owner), route.server_id, &key)
                        .await?
                } else {
                    false
                };
                if !invoked {
                    if key != "default" {
                        return Err(IslandError::unavailable("Notification action has expired"));
                    }
                    let desktop = route.desktop_entry.ok_or_else(|| {
                        IslandError::unavailable("This notification has no desktop entry to open")
                    })?;
                    crate::commands::blocking(move || super::effects::launch(&desktop)).await?;
                }
            }
            #[cfg(not(target_os = "linux"))]
            return Err(IslandError::unsupported(
                "Notification activation is unavailable",
            ));
            hub.popup_change(&id, None)?;
        }
    }
    hub.popup_snapshot()
}
