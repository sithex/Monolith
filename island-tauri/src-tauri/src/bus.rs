//! Typed event emission + latest-value topics (SPEC §3.1).
//!
//! Latest-value topics (settings, palette, later stats/power/…) live in a `tokio::sync::watch`
//! channel; a forwarder coalesces and emits at most once per interval (≤ 100 ms), always
//! delivering the newest value last.

use crate::contract::{
    events::{
        AnchorKind, AnchorSignal, CueKind, IslandCue, IslandStateChanged, PaletteChanged,
        SettingsChanged,
    },
    island::IslandState,
    palette::Palette,
    settings::Settings,
};
use crate::error::IslandError;
use crate::window::ISLAND_LABEL;
use std::{future::Future, time::Duration};
use tauri::{Emitter, Manager, Runtime};
use tauri_specta::Event;
use tokio::sync::watch;

/// Upper bound of the emit rate of a latest-value topic.
pub const COALESCE: Duration = Duration::from_millis(100);

pub fn emit_settings<R: Runtime, H: Emitter<R> + Manager<R>>(handle: &H, settings: Settings) {
    if let Err(error) = SettingsChanged(settings).emit_to(handle, ISLAND_LABEL) {
        tracing::warn!(%error, "SettingsChanged emit failed");
    }
}

pub fn emit_palette<R: Runtime, H: Emitter<R> + Manager<R>>(handle: &H, palette: Palette) {
    if let Err(error) = PaletteChanged(palette).emit_to(handle, ISLAND_LABEL) {
        tracing::warn!(%error, "PaletteChanged emit failed");
    }
}

/// Phase 3 (`panel.sock`) is the only emitter; typed helper provided now.
pub fn emit_anchor<R: Runtime, H: Emitter<R> + Manager<R>>(handle: &H, kind: AnchorKind) {
    if let Err(error) = AnchorSignal(kind).emit_to(handle, ISLAND_LABEL) {
        tracing::warn!(%error, "AnchorSignal emit failed");
    }
}

/// The state actor's sink is the ONLY caller (SPEC §4.5). A failed emit is returned, never
/// swallowed: the actor treats it as fatal rather than letting the UI silently fall behind.
pub fn emit_state<R: Runtime, H: Emitter<R> + Manager<R>>(
    handle: &H,
    state: IslandState,
) -> Result<(), IslandError> {
    IslandStateChanged(state)
        .emit_to(handle, ISLAND_LABEL)
        .map_err(|e| IslandError::internal(format!("IslandStateChanged emit failed: {e}")))
}

/// The actor's `PlayClick` effect (additive event, SPEC §4.5).
pub fn emit_cue<R: Runtime, H: Emitter<R> + Manager<R>>(
    handle: &H,
    kind: CueKind,
) -> Result<(), IslandError> {
    IslandCue { kind }
        .emit_to(handle, ISLAND_LABEL)
        .map_err(|e| IslandError::internal(format!("IslandCue emit failed: {e}")))
}

/// Publisher half of a latest-value topic.
pub struct Latest<T> {
    tx: watch::Sender<Option<T>>,
}

impl<T: Clone + Send + Sync + 'static> Latest<T> {
    pub fn new() -> Self {
        Self {
            tx: watch::channel(None).0,
        }
    }

    /// Replace the topic's value (never blocks, never fails when nobody listens).
    pub fn publish(&self, value: T) {
        self.tx.send_replace(Some(value));
    }

    /// The most recently published value, if any.
    pub fn latest(&self) -> Option<T> {
        self.tx.borrow().clone()
    }

    /// Subscribe to raw values (tests and in-process consumers); `None` until first publish.
    pub fn subscribe(&self) -> watch::Receiver<Option<T>> {
        self.tx.subscribe()
    }

    /// The forwarder future: run it on an async runtime. Values published before the call are
    /// considered already seen.
    pub fn forwarder(
        &self,
        interval: Duration,
        emit: impl Fn(T) + Send + 'static,
    ) -> impl Future<Output = ()> + Send + 'static {
        let mut rx = self.tx.subscribe();
        async move {
            while rx.changed().await.is_ok() {
                let value = rx.borrow_and_update().clone();
                if let Some(value) = value {
                    emit(value);
                }
                tokio::time::sleep(interval).await;
            }
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Default for Latest<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn bursts_are_coalesced_and_the_newest_value_wins() {
        let topic = Latest::<u32>::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let task = tokio::spawn(topic.forwarder(Duration::from_millis(60), move |v| {
            sink.lock().unwrap().push(v);
        }));
        for value in 0..200 {
            topic.publish(value);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        let first = seen.lock().unwrap().clone();
        assert_eq!(
            first.last(),
            Some(&199),
            "newest value must be delivered last"
        );
        assert!(first.len() <= 3, "200 publishes must coalesce: {first:?}");
        assert!(
            first.windows(2).all(|w| w[0] < w[1]),
            "values only move forward"
        );
        // A quiet topic emits promptly, again at most once per interval.
        topic.publish(1000);
        tokio::time::sleep(Duration::from_millis(30)).await;
        topic.publish(1001);
        topic.publish(1002);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let all = seen.lock().unwrap().clone();
        assert_eq!(all.last(), Some(&1002));
        assert!(all.contains(&1000));
        assert!(all.len() <= first.len() + 3, "{all:?}");
        task.abort();
    }

    #[tokio::test]
    async fn forwarder_ends_when_the_topic_is_dropped() {
        let topic = Latest::<u8>::new();
        let task = tokio::spawn(topic.forwarder(Duration::from_millis(10), |_| {}));
        drop(topic);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("forwarder must end")
            .unwrap();
    }
}
