//! Notification wire shapes (SPEC §5.2); no notification service starts in Phase 2.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum NotificationSource {
    System,
    Hermes,
    Alert,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NotificationAction {
    pub key: String,
    pub label: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IslandNotification {
    pub rev: u32,
    pub id: String,
    pub source: NotificationSource,
    pub app_name: String,
    pub title: String,
    pub body: Option<String>,
    pub icon_png: Option<String>,
    pub urgency: Urgency,
    pub received_at: f64,
    pub actions: Vec<NotificationAction>,
    pub can_invoke_actions: bool,
    pub progress: Option<f64>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
pub struct NotificationReceived(pub IslandNotification);

/// Loss repair for the bounded delta queue. One coalesced full ring, never a queue
/// of snapshots. `rev` is the store boundary, including closes absent from items.
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSnapshot {
    pub rev: u32,
    pub items: Vec<IslandNotification>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CloseReason {
    Expired,
    Dismissed,
    Cleared,
    Evicted,
    Filtered,
    Replaced,
    ServerClosed,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct NotificationClosed {
    pub rev: u32,
    pub id: String,
    pub reason: CloseReason,
}
