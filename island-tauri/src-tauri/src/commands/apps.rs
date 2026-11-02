//! App commands: bounded typed inputs; all filesystem/subprocess work uses blocking workers.
use crate::{
    contract::{apps::*, settings::ObjectOnly},
    error::IslandError,
    integrations::system_services::SystemServices,
};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
#[tauri::command]
#[specta::specta]
pub async fn timer_get(
    system: tauri::State<'_, Arc<SystemServices>>,
) -> Result<TimerChanged, IslandError> {
    system.apps.timer()?.request(None).await
}
#[tauri::command]
#[specta::specta]
pub async fn timer_command(
    system: tauri::State<'_, Arc<SystemServices>>,
    op: TimerOp,
) -> Result<TimerChanged, IslandError> {
    system.apps.timer()?.request(Some(op)).await
}
#[tauri::command]
#[specta::specta]
pub async fn scratchpad_get(
    system: tauri::State<'_, Arc<SystemServices>>,
) -> Result<ScratchpadChanged, IslandError> {
    let s = system.apps.scratchpad()?;
    super::blocking(move || s.current()).await
}
#[tauri::command]
#[specta::specta]
pub async fn scratchpad_command(
    system: tauri::State<'_, Arc<SystemServices>>,
    op: ScratchpadOp,
) -> Result<ScratchpadChanged, IslandError> {
    let s = system.apps.scratchpad()?;
    super::blocking(move || s.apply(s.now(), op)).await
}
#[tauri::command]
#[specta::specta]
pub async fn downloads_get(
    system: tauri::State<'_, Arc<SystemServices>>,
) -> Result<DownloadsChanged, IslandError> {
    let s = system.apps.downloads()?;
    super::blocking(move || s.current()).await
}
#[tauri::command]
#[specta::specta]
pub async fn downloads_open(
    system: tauri::State<'_, Arc<SystemServices>>,
    name: String,
    op: OpenDownload,
) -> Result<(), IslandError> {
    let s = system.apps.downloads()?;
    super::blocking(move || s.open(&name, op)).await
}

#[tauri::command]
#[specta::specta]
pub async fn calendar_get(
    system: tauri::State<'_, Arc<SystemServices>>,
    query: ObjectOnly<CalendarQuery>,
) -> Result<CalendarChanged, IslandError> {
    let q = query.0;
    if !q.start_s.is_finite()
        || !q.end_s.is_finite()
        || q.start_s.fract() != 0.
        || q.end_s.fract() != 0.
        || q.start_s.abs() > 253_402_300_800.
        || q.end_s.abs() > 253_402_300_800.
        || q.local_offset_s.unsigned_abs() > 86400
    {
        return Err(IslandError::invalid("Invalid calendar window"));
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| IslandError::unavailable("Wall clock unavailable"))?
        .as_secs_f64()
        * 1000.;
    let s = system.apps.calendar()?;
    super::blocking(move || s.current(now, q.start_s as i64, q.end_s as i64, q.local_offset_s))
        .await
}
#[tauri::command]
#[specta::specta]
pub async fn calendar_command(
    system: tauri::State<'_, Arc<SystemServices>>,
    op: CalendarOp,
) -> Result<(), IslandError> {
    let s = system.apps.calendar()?;
    super::blocking(move || s.apply(op)).await
}
#[tauri::command]
#[specta::specta]
pub async fn calendar_refresh(
    system: tauri::State<'_, Arc<SystemServices>>,
) -> Result<(), IslandError> {
    let s = system.apps.calendar()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| IslandError::unavailable("Wall clock unavailable"))?
        .as_secs_f64()
        * 1000.;
    s.refresh(now).await
}
/// Write-only: the subscription link is stored privately and never returned.
#[tauri::command]
#[specta::specta]
pub async fn calendar_set_subscription(
    system: tauri::State<'_, Arc<SystemServices>>,
    url: Option<String>,
) -> Result<(), IslandError> {
    if url
        .as_ref()
        .is_some_and(|u| u.len() > crate::integrations::calendar::URL_CAP)
    {
        return Err(IslandError::invalid("Calendar URL too long"));
    }
    let s = system.apps.calendar()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| IslandError::unavailable("Wall clock unavailable"))?
        .as_secs_f64()
        * 1000.;
    s.set_subscription(url, now).await
}
