use super::blocking;
use crate::{
    contract::system::{GeoPlace, SystemStats, WeatherState},
    error::IslandError,
    integrations::system_services::SystemServices,
    services::Services,
};
use std::sync::Arc;

#[tauri::command]
#[specta::specta]
pub async fn system_stats(
    services: tauri::State<'_, Services>,
) -> Result<SystemStats, IslandError> {
    let stats = Arc::clone(&services.stats);
    blocking(move || stats.sample()).await
}

/// SPEC §4.3 `weather_refresh`: fetch now (only when weather is enabled with a location;
/// otherwise no request is made) and return the current state.
#[tauri::command]
#[specta::specta]
pub async fn weather_refresh(
    system: tauri::State<'_, Arc<SystemServices>>,
) -> Result<WeatherState, IslandError> {
    let weather = Arc::clone(&system.weather);
    weather.refresh().await
}

/// SPEC §4.3 `weather_geocode`: user-initiated place search (never automatic, no IP lookup).
#[tauri::command]
#[specta::specta]
pub async fn weather_geocode(
    query: String,
    system: tauri::State<'_, Arc<SystemServices>>,
) -> Result<Vec<GeoPlace>, IslandError> {
    let weather = Arc::clone(&system.weather);
    weather.geocode(&query).await
}
