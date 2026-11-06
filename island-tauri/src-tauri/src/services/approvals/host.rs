//! Tauri side of the approval tracker: typed events to the page and hover-notice intents to
//! the state actor.

use super::ApprovalSink;
use crate::{
    contract::approvals::ApprovalChanged,
    state::{actor::Handle, machine::Intent},
    window::ISLAND_LABEL,
};
use tauri_specta::Event;

pub struct HostSink {
    app: tauri::AppHandle,
    actor: Handle,
}

impl HostSink {
    pub fn new(app: tauri::AppHandle, actor: Handle) -> Self {
        Self { app, actor }
    }
}

impl ApprovalSink for HostSink {
    fn changed(&self, change: &ApprovalChanged) {
        if let Err(error) = change.clone().emit_to(&self.app, ISLAND_LABEL) {
            tracing::warn!(%error, "ApprovalChanged emit failed");
        }
    }

    fn arrived(&self, id: &str) {
        if let Err(error) = self
            .actor
            .try_dispatch(Intent::ApprovalShown(id.to_owned()))
        {
            tracing::debug!(%error, "approval notice intent dropped");
        }
    }

    fn cleared(&self, id: &str) {
        if let Err(error) = self
            .actor
            .try_dispatch(Intent::ApprovalCleared(id.to_owned()))
        {
            tracing::debug!(%error, "approval clear intent dropped");
        }
    }
}
