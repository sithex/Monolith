//! Explicit third-party interception capability until the platform listener is implemented.
use super::hub::Hub;
use crate::{contract::system::NotificationMonitorStatus, error::IslandError};
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Default)]
pub struct Monitor;
impl Monitor {
    pub fn status(&self) -> NotificationMonitorStatus {
        NotificationMonitorStatus {
            available: false,
            detail: "unsupported".into(),
            ..Default::default()
        }
    }
    pub fn current_owner(&self) -> Option<String> {
        None
    }
    pub async fn close(&self, _: Option<&str>, _: u32) -> Result<bool, IslandError> {
        Err(IslandError::unsupported(
            "Third-party notification interception is unsupported",
        ))
    }
    pub async fn run(
        self: Arc<Self>,
        _: Arc<Hub>,
        _: String,
        _: watch::Receiver<bool>,
        mut stopped: watch::Receiver<bool>,
    ) -> Result<(), IslandError> {
        while !*stopped.borrow_and_update() {
            if stopped.changed().await.is_err() {
                break;
            }
        }
        Ok(())
    }
}
