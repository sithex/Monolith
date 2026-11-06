//! Phase 5 clipboard service. The audited staged core is ported unchanged except imports.
//! Blocking configuration, history commands and shutdown belong on a blocking worker;
//! `set_paused` is the actor-safe exception: atomics + watch publication, no IO.
mod control_copy;
pub mod history;
#[cfg(target_os = "linux")]
mod linux_wayland;
mod persistence;
#[cfg(test)]
mod tests;
#[cfg(not(target_os = "linux"))]
mod unsupported;

use crate::{
    contract::{clipboard as wire, settings::Settings},
    error::{ErrorCode, IslandError},
    window::ISLAND_LABEL,
};
use history::{ClipInput, ClipPayload, HistoryConfig, HistoryStore, OfferOutcome, SkipReason};
#[cfg(target_os = "linux")]
use linux_wayland as platform;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri_specta::Event;
use tokio::sync::{watch, Notify};
#[cfg(not(target_os = "linux"))]
use unsupported as platform;

type EventTask = tauri::async_runtime::JoinHandle<()>;
struct Inner {
    store: HistoryStore,
    config: HistoryConfig,
    persist: bool,
    rev: u32,
    pending: VecDeque<wire::ClipSummary>,
}
pub(super) struct Shared {
    inner: Mutex<Inner>,
    enabled: AtomicBool,
    paused: AtomicBool,
    epoch: AtomicU32,
    status: watch::Sender<wire::ClipboardStatus>,
    wake: Notify,
    file: PathBuf,
    /// Non-blocking nudge for the running watcher's poll loop (a clone of its wake socket).
    /// Pause/resume must wake it so the epoch fence is posted BEFORE the next selection event;
    /// otherwise the first copy after a resume is discarded as "queued while paused".
    #[cfg(target_os = "linux")]
    watcher_wake: Mutex<Option<std::os::unix::net::UnixStream>>,
}
pub struct Clipboard {
    shared: Arc<Shared>,
    worker: Mutex<Option<platform::Worker>>,
    events: Mutex<Option<EventTask>>,
    event_stop: watch::Sender<bool>,
    closed: AtomicBool,
}
impl Clipboard {
    pub fn new(data: PathBuf, settings: Settings) -> Result<Arc<Self>, IslandError> {
        let config = config(&settings)?;
        let store = if settings.clipboard_persist {
            persistence::load(&data.join("clipboard.json"), config.clone())?
        } else {
            HistoryStore::new(config.clone()).map_err(core_error)?
        };
        let service = Arc::new(Self {
            shared: Arc::new(Shared {
                inner: Mutex::new(Inner {
                    store,
                    config,
                    persist: settings.clipboard_persist,
                    rev: 0,
                    pending: VecDeque::with_capacity(128),
                }),
                enabled: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                epoch: AtomicU32::new(0),
                status: watch::channel(wire::ClipboardStatus {
                    available: false,
                    enabled: false,
                    paused: false,
                    detail: "disabled".into(),
                    watcher_running: false,
                    connections_open: 0,
                    connection_attempts: 0,
                    dropped: 0,
                    captured: 0,
                    sensitive_skipped: 0,
                    denied: 0,
                    echoes: 0,
                })
                .0,
                wake: Notify::new(),
                file: data.join("clipboard.json"),
                #[cfg(target_os = "linux")]
                watcher_wake: Mutex::new(None),
            }),
            worker: Mutex::new(None),
            events: Mutex::new(None),
            event_stop: watch::channel(false).0,
            closed: AtomicBool::new(false),
        });
        service.configure(settings)?;
        Ok(service)
    }
    pub fn configure(&self, settings: Settings) -> Result<(), IslandError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(IslandError::unavailable("Clipboard service stopped"));
        }
        let cfg = config(&settings)?;
        // Serialize enable/disable and joins; capture never needs the worker slot lock.
        let mut worker = lock(&self.worker)?;
        let enabled = settings.clipboard_history;
        if !enabled {
            self.shared.enabled.store(false, Ordering::Release);
            self.shared.epoch.fetch_add(1, Ordering::AcqRel);
            self.shared.inner()?.pending.clear();
            if let Some(mut running) = worker.take() {
                running.stop()?;
            }
        }
        {
            let mut inner = self.shared.inner()?;
            if cfg != inner.config {
                let snapshot = inner.store.to_persisted(true).map_err(core_error)?;
                inner.store =
                    HistoryStore::from_persisted(cfg.clone(), &snapshot).map_err(core_error)?;
                inner.config = cfg;
                inner.rev = inner.rev.wrapping_add(1);
                inner.pending.clear();
            }
            let was_persist = inner.persist;
            inner.persist = settings.clipboard_persist;
            if inner.persist {
                persistence::save(&self.shared.file, &inner.store)?;
            } else if was_persist {
                persistence::remove(&self.shared.file)?;
            }
        }
        self.shared.enabled.store(enabled, Ordering::Release);
        self.shared.status.send_modify(|s| {
            s.enabled = enabled;
            if !enabled {
                s.available = false;
                s.detail = "disabled".into();
            }
        });
        if enabled && worker.as_ref().is_none_or(platform::Worker::is_finished) {
            if let Some(mut old) = worker.take() {
                old.stop()?;
            }
            *worker = Some(platform::Worker::start(self.shared.clone())?);
        }
        Ok(())
    }
    pub fn status(&self) -> Result<wire::ClipboardStatus, IslandError> {
        Ok(self.shared.status.borrow().clone())
    }
    pub fn watch_status(&self) -> watch::Receiver<wire::ClipboardStatus> {
        self.shared.status.subscribe()
    }
    pub fn list(
        &self,
        query: Option<&str>,
        limit: u16,
    ) -> Result<Vec<wire::ClipSummary>, IslandError> {
        if query.is_some_and(|q| q.len() > 4096) {
            return Err(IslandError::invalid("Clipboard query too long"));
        }
        let inner = self.shared.inner()?;
        Ok(inner
            .store
            .list(query, limit.min(500))
            .into_iter()
            .map(|s| summary(s, inner.rev))
            .collect())
    }
    pub fn get(&self, id: &str) -> Result<wire::ClipContent, IslandError> {
        self.shared
            .inner()?
            .store
            .get(id)
            .map(content)
            .ok_or_else(|| IslandError::invalid("Clipboard entry not found"))
    }
    pub fn copy(&self, id: &str) -> Result<(), IslandError> {
        let payload = self
            .shared
            .inner()?
            .store
            .get(id)
            .ok_or_else(|| IslandError::invalid("Clipboard entry not found"))?;
        let worker = lock(&self.worker)?;
        worker
            .as_ref()
            .ok_or_else(|| IslandError::unavailable("Clipboard monitoring is disabled"))?
            .copy(payload)
    }
    pub fn pin(&self, id: &str, pinned: bool) -> Result<(), IslandError> {
        self.mutate(|store| store.pin(id, pinned).map_err(core_error))
    }
    pub fn delete(&self, id: &str) -> Result<(), IslandError> {
        self.mutate(|s| s.delete(id).map_err(core_error))
    }
    pub fn clear(&self) -> Result<(), IslandError> {
        self.mutate(|s| {
            s.clear();
            Ok(())
        })
    }
    fn mutate(
        &self,
        change: impl FnOnce(&mut HistoryStore) -> Result<(), IslandError>,
    ) -> Result<(), IslandError> {
        let mut inner = self.shared.inner()?;
        change(&mut inner.store)?;
        inner.rev = inner.rev.wrapping_add(1);
        // Queue contains only Added; do not resurrect a removed/changed entry during delivery.
        inner.pending.clear();
        if inner.persist {
            persistence::save(&self.shared.file, &inner.store)?;
        }
        Ok(())
    }
    pub fn set_paused(&self, paused: bool) -> Result<(), IslandError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(IslandError::unavailable("Clipboard service stopped"));
        }
        self.shared.paused.store(paused, Ordering::Release);
        self.shared.epoch.fetch_add(1, Ordering::AcqRel);
        self.shared.status.send_modify(|s| s.paused = paused);
        // In-flight captures are dropped by the epoch check on the loop's next event. No worker-slot lock,
        // thread join or history serialization is performed by this actor effect; the only IO is a
        // one-byte non-blocking write that wakes an idle watcher so it fences the new epoch now.
        #[cfg(target_os = "linux")]
        if let Some(wake) = lock(&self.shared.watcher_wake)?.as_ref() {
            platform::wake(wake)?;
        }
        Ok(())
    }
    pub fn start_events(self: &Arc<Self>, app: tauri::AppHandle) -> Result<(), IslandError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(IslandError::unavailable("Clipboard service stopped"));
        }
        let mut task = lock(&self.events)?;
        if task.is_some() {
            return Err(IslandError::new(
                ErrorCode::Busy,
                "Clipboard event delivery already started",
            ));
        }
        let shared = self.shared.clone();
        let mut stopped = self.event_stop.subscribe();
        let mut status = self.watch_status();
        *task = Some(tauri::async_runtime::spawn(async move {
            let mut revision = 0u32;
            loop {
                let notified = shared.wake.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                // Take one at a time: no hidden, unbounded drained delivery batch.
                match shared.emit_one(&app) {
                    Ok(true) => {
                        tokio::task::yield_now().await;
                        continue;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        tracing::error!(%error, "Clipboard event store failed");
                        break;
                    }
                }
                tokio::select! { biased;
                    changed = stopped.changed() => { if changed.is_err() || *stopped.borrow() {break;} },
                    changed = status.changed() => {
                        if changed.is_err() { break; }
                        // Latest-value status is coalesced; no appearance/status polling.
                        tokio::select! {
                            _ = stopped.changed() => break,
                            () = tokio::time::sleep(std::time::Duration::from_millis(100)) => {},
                        }
                        revision = revision.wrapping_add(1);
                        let latest = status.borrow_and_update().clone();
                        if let Err(error) = (wire::ClipboardStatusChanged { rev: revision, status: latest }).emit_to(&app, ISLAND_LABEL) {
                            tracing::warn!(%error, "Clipboard status delivery failed");
                        }
                    },
                    () = &mut notified => {},
                }
            }
        }));
        Ok(())
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        self.closed.store(true, Ordering::Release);
        self.shared.enabled.store(false, Ordering::Release);
        self.shared.epoch.fetch_add(1, Ordering::AcqRel);
        self.event_stop.send_replace(true);
        if let Some(task) = lock(&self.events)?.take() {
            task.abort();
        }
        self.shared.inner()?.pending.clear();
        if let Some(mut worker) = lock(&self.worker)?.take() {
            worker.stop()?;
        }
        self.shared.status.send_modify(|s| {
            s.enabled = false;
            s.available = false;
            s.detail = "stopped".into();
        });
        Ok(())
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::error!(%error, "Clipboard teardown failed");
        }
    }
}
impl Shared {
    fn emit_one(&self, app: &tauri::AppHandle) -> Result<bool, IslandError> {
        let mut inner = self.inner()?;
        if !self.enabled.load(Ordering::Acquire) {
            inner.pending.clear();
            return Ok(false);
        }
        let Some(item) = inner.pending.pop_front() else {
            return Ok(false);
        };
        if inner.store.list(None, 500).iter().any(|s| s.id == item.id) {
            if let Err(error) = wire::ClipboardAdded(item).emit_to(app, ISLAND_LABEL) {
                tracing::warn!(%error, "Clipboard event delivery failed");
                self.status_change(|s| s.dropped = s.dropped.saturating_add(1));
            }
        }
        Ok(true)
    }
    fn inner(&self) -> Result<MutexGuard<'_, Inner>, IslandError> {
        lock(&self.inner)
    }
    fn status_change(&self, f: impl FnOnce(&mut wire::ClipboardStatus)) {
        self.status.send_modify(f);
    }
    fn active(&self, epoch: u32) -> bool {
        self.enabled.load(Ordering::Acquire)
            && !self.paused.load(Ordering::Acquire)
            && self.epoch.load(Ordering::Acquire) == epoch
    }
    fn offer(
        &self,
        payload: ClipPayload,
        mimes: Vec<String>,
        epoch: u32,
    ) -> Result<(), IslandError> {
        if !self.active(epoch) {
            return Ok(());
        }
        let mut inner = self.inner()?;
        if !self.active(epoch) {
            return Ok(());
        }
        inner.store.set_paused(false);
        let outcome = inner.store.offer(
            ClipInput {
                content: payload,
                mime_types: mimes,
                password_hint: None,
                source_app: None,
            },
            now(),
        );
        let changed = matches!(
            &outcome,
            OfferOutcome::Added { .. }
                | OfferOutcome::Reordered { .. }
                | OfferOutcome::Skipped(SkipReason::Echo)
        );
        match outcome {
            OfferOutcome::Added { id } => {
                inner.rev = inner.rev.wrapping_add(1);
                if let Some(s) = inner.store.list(None, 500).into_iter().find(|s| s.id == id) {
                    if inner.pending.len() == 128 {
                        inner.pending.pop_front();
                        self.status_change(|s| s.dropped = s.dropped.saturating_add(1));
                    }
                    let rev = inner.rev;
                    inner.pending.push_back(summary(s, rev));
                    self.wake.notify_one();
                }
                self.status_change(|s| s.captured = s.captured.saturating_add(1));
            }
            OfferOutcome::Skipped(SkipReason::Echo) => {
                inner.rev = inner.rev.wrapping_add(1);
                self.status_change(|s| s.echoes = s.echoes.saturating_add(1))
            }
            OfferOutcome::Skipped(SkipReason::Denied) => {
                self.status_change(|s| s.denied = s.denied.saturating_add(1))
            }
            OfferOutcome::Skipped(SkipReason::Sensitive) => {
                self.status_change(|s| s.sensitive_skipped = s.sensitive_skipped.saturating_add(1))
            }
            OfferOutcome::Skipped(_) => {
                self.status_change(|s| s.dropped = s.dropped.saturating_add(1))
            }
            OfferOutcome::Reordered { .. } => {
                inner.rev = inner.rev.wrapping_add(1);
            }
        }
        // Capture runs on hi-clipboard, never the actor. Preserve pinned ordering
        // and the allocation counter as well as explicit pin/delete/clear changes.
        if changed && inner.persist {
            persistence::save(&self.file, &inner.store)?;
        }
        Ok(())
    }
    fn expect_echo(&self, payload: ClipPayload) -> Result<(), IslandError> {
        let hash = HistoryStore::hash_of(&ClipInput {
            content: payload,
            mime_types: vec![],
            password_hint: None,
            source_app: None,
        });
        self.inner()?.store.expect_echo(hash, now());
        Ok(())
    }
}
fn lock<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, IslandError> {
    m.lock()
        .map_err(|_| IslandError::internal("Clipboard state poisoned"))
}
fn config(s: &Settings) -> Result<HistoryConfig, IslandError> {
    HistoryConfig {
        capacity: s.clipboard_history_size as usize,
        deny_patterns: s.clipboard_deny_patterns.clone(),
        ..HistoryConfig::default()
    }
    .validated()
    .map_err(core_error)
}
fn core_error(_: history::Error) -> IslandError {
    IslandError::invalid("Invalid clipboard entry, policy or persistence")
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}
fn summary(s: history::ClipSummary, rev: u32) -> wire::ClipSummary {
    wire::ClipSummary {
        rev,
        id: s.id,
        kind: match s.kind {
            history::ClipKind::Text => wire::ClipKind::Text,
            history::ClipKind::Image => wire::ClipKind::Image,
            history::ClipKind::Files => wire::ClipKind::Files,
        },
        preview: s.preview,
        bytes: s.bytes,
        pinned: s.pinned,
        copied_at: s.copied_at,
        source_app: s.source_app,
    }
}
fn content(c: history::ClipContent) -> wire::ClipContent {
    match c {
        history::ClipContent::Text { text } => wire::ClipContent::Text { text },
        history::ClipContent::Image {
            png_data_url,
            width,
            height,
        } => wire::ClipContent::Image {
            png_data_url,
            width,
            height,
        },
        history::ClipContent::Files { uris } => wire::ClipContent::Files { uris },
    }
}
