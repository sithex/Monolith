//! Honest unavailable adapter; no desktop access on unimplemented platforms.
use super::{history::ClipContent, Shared};
use crate::error::IslandError;
use std::sync::Arc;
pub(super) struct Worker;
impl Worker {
    pub fn start(shared: Arc<Shared>) -> Result<Self, IslandError> {
        shared.status_change(|s| {
            s.available = false;
            s.detail = "Clipboard backend is unsupported on this platform".into();
        });
        Ok(Self)
    }
    pub fn is_finished(&self) -> bool {
        false
    }
    pub fn copy(&self, _: ClipContent) -> Result<(), IslandError> {
        Err(IslandError::unsupported(
            "Clipboard backend is unsupported on this platform",
        ))
    }
    pub fn stop(&mut self) -> Result<(), IslandError> {
        Ok(())
    }
}
