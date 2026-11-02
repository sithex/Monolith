//! Serialized notification storage and bounded event delivery. Only StateActor emits mode.
use super::{
    hermes_ingest::{Handler, IngestServer},
    ingest::IngestSession,
    *,
};
use crate::{
    contract::notify as wire,
    error::IslandError,
    services::settings::SettingsService,
    state::{actor::Handle, machine::Intent},
    window::ISLAND_LABEL,
};
use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri_specta::Event;
use tokio::sync::{watch, Notify};

struct Inner {
    store: NotificationStore,
    pending: VecDeque<(Changes, Option<String>)>,
    repair_needed: bool,
}
pub struct Hub {
    inner: Mutex<Inner>,
    wake: Notify,
    actor: Handle,
    settings: Arc<SettingsService>,
    epoch: f64,
    origin: Instant,
    pub dropped: AtomicU32,
    pub monitor: Arc<super::linux_monitor::Monitor>,
    mirror_enabled: watch::Sender<bool>,
}
impl Hub {
    pub fn new(actor: Handle, settings: Arc<SettingsService>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                store: NotificationStore::default(),
                pending: VecDeque::new(),
                repair_needed: false,
            }),
            wake: Notify::new(),
            actor,
            settings,
            epoch: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs_f64() * 1000.)
                .unwrap_or(0.),
            origin: Instant::now(),
            dropped: AtomicU32::new(0),
            monitor: Arc::default(),
            mirror_enabled: watch::channel(false).0,
        })
    }
    fn now(&self) -> f64 {
        self.epoch + self.origin.elapsed().as_secs_f64() * 1000.
    }
    fn mutate<T>(
        &self,
        work: impl FnOnce(&mut NotificationStore, f64) -> Result<(T, Changes), IslandError>,
    ) -> Result<T, IslandError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| IslandError::internal("Notification store poisoned"))?;
        let settings = self.settings.get()?;
        let mut policy = inner.store.policy().clone();
        policy.dnd = self.actor.state().dnd;
        policy.redact_bodies = settings.redact_bodies;
        policy.denied_apps = settings.notification_denied_apps.into_iter().collect();
        self.mirror_enabled.send_if_modified(|enabled| {
            let changed = *enabled != settings.system_notification_mirror;
            *enabled = settings.system_notification_mirror;
            changed
        });
        let privacy = inner.store.set_policy(policy).map_err(core_error)?;
        self.queue(&mut inner, privacy);
        let (value, changes) = work(&mut inner.store, self.now())?;
        self.queue(&mut inner, changes);
        drop(inner);
        self.wake.notify_one();
        Ok(value)
    }
    fn queue(&self, inner: &mut Inner, changes: Changes) {
        if changes.events.is_empty() && changes.close_server_ids.is_empty() {
            return;
        }
        // Capacity is EVENTS, not batches (clear/privacy operations can emit 200 deltas).
        let mut close_server_ids = changes.close_server_ids;
        for event in changes.events {
            if inner.pending.len() == 128 {
                inner.pending.pop_front();
                inner.repair_needed = true;
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            let received = matches!(&event, Delta::NotificationReceived(_));
            inner.pending.push_back((
                Changes {
                    events: vec![event],
                    id: changes.id.clone(),
                    toast: changes.toast && received,
                    close_server_ids: std::mem::take(&mut close_server_ids),
                },
                self.monitor.current_owner(),
            ));
        }
    }
    pub fn refresh_policy(&self) -> Result<(), IslandError> {
        self.mutate(|_, _| Ok(((), Changes::default())))
    }
    pub fn list(&self) -> Result<Vec<wire::IslandNotification>, IslandError> {
        self.mutate(|store, now| {
            let changes = store.expire(now).map_err(core_error)?;
            Ok((store.list().into_iter().map(Into::into).collect(), changes))
        })
    }
    pub fn insert(&self, input: NotificationInput) -> Result<(), IslandError> {
        self.mutate(|store, now| Ok(((), store.insert(input, now).map_err(core_error)?)))
    }
    pub fn dismiss(&self, id: &str) -> Result<(), IslandError> {
        self.mutate(|store, _| Ok(((), store.dismiss(id))))
    }
    pub fn clear(&self) -> Result<(), IslandError> {
        self.mutate(|store, _| Ok(((), store.clear())))
    }
    pub fn server_closed(&self, id: u32) -> Result<(), IslandError> {
        self.mutate(|store, _| Ok(((), store.server_closed(id))))
    }
    pub fn reset_server(&self) -> Result<(), IslandError> {
        self.mutate(|store, _| Ok(((), store.reset_server())))
    }
    pub fn denied_apps(&self, apps: std::collections::BTreeSet<String>) -> Result<(), IslandError> {
        self.mutate(|store, _| {
            let mut p = store.policy().clone();
            p.denied_apps = apps;
            Ok(((), store.set_policy(p).map_err(core_error)?))
        })
    }
    async fn run(
        self: Arc<Self>,
        app: tauri::AppHandle,
        mut stopped: watch::Receiver<bool>,
    ) -> Result<(), IslandError> {
        loop {
            // Register before inspecting the queue, avoiding lost wakeups between drain/wait.
            let notified = self.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (pending, unread, expiry, repair) = {
                let mut inner = self
                    .inner
                    .lock()
                    .map_err(|_| IslandError::internal("Notification store poisoned"))?;
                let expired = inner.store.expire(self.now()).map_err(core_error)?;
                self.queue(&mut inner, expired);
                (
                    inner.pending.drain(..).collect::<Vec<_>>(),
                    inner.store.len() as u32,
                    inner.store.next_expiry(),
                    if std::mem::take(&mut inner.repair_needed) {
                        Some(wire::NotificationSnapshot {
                            rev: inner.store.current_revision(),
                            items: inner.store.list().into_iter().map(Into::into).collect(),
                        })
                    } else {
                        None
                    },
                )
            };
            if let Some(repair) = repair {
                repair
                    .emit_to(&app, ISLAND_LABEL)
                    .map_err(|_| IslandError::internal("NotificationSnapshot emit failed"))?;
            }
            for (changes, close_owner) in pending {
                for delta in changes.events {
                    match delta {
                        Delta::NotificationReceived(n) => {
                            // A drained batch can be superseded by privacy/close while
                            // awaiting the actor. Never re-emit an obsolete private payload.
                            let current = self
                                .inner
                                .lock()
                                .map_err(|_| IslandError::internal("Notification store poisoned"))?
                                .store
                                .is_current(&n.0.id, n.0.rev);
                            if current {
                                if changes.toast {
                                    self.actor
                                        .dispatch(Intent::NotificationArrived(n.0.id.clone()))
                                        .await?;
                                }
                                // Recheck after the actor await and hold serialization
                                // through emit, so a concurrent scrub cannot be undone.
                                let inner = self.inner.lock().map_err(|_| {
                                    IslandError::internal("Notification store poisoned")
                                })?;
                                if inner.store.is_current(&n.0.id, n.0.rev) {
                                    wire::NotificationReceived(n.0.into())
                                        .emit_to(&app, ISLAND_LABEL)
                                        .map_err(|_| {
                                            IslandError::internal(
                                                "NotificationReceived emit failed",
                                            )
                                        })?;
                                }
                            }
                        }
                        Delta::NotificationClosed(c) => wire::NotificationClosed {
                            rev: c.rev,
                            id: c.id,
                            reason: c.reason.into(),
                        }
                        .emit_to(&app, ISLAND_LABEL)
                        .map_err(|_| IslandError::internal("NotificationClosed emit failed"))?,
                    }
                }
                // Remote close is deliberately independent of local removal. Monitor wiring
                // supplies connection A; errors must never resurrect the local notification.
                for id in changes.close_server_ids {
                    if let Err(error) = self.monitor.close(close_owner.as_deref(), id).await {
                        tracing::warn!(%error, "Best-effort remote notification close failed");
                    }
                }
            }
            self.actor
                .dispatch(Intent::NotificationsSync(unread))
                .await?;
            let wait = async {
                match expiry {
                    Some(deadline) => {
                        tokio::time::sleep(Duration::from_secs_f64(
                            ((deadline - self.now()).max(0.)) / 1000.,
                        ))
                        .await
                    }
                    None => std::future::pending::<()>().await,
                }
            };
            tokio::select! { biased;
                result=stopped.changed()=>{if result.is_err()||*stopped.borrow(){break;}},
                ()=&mut notified=>{},
                ()=wait=>{},
            }
        }
        Ok(())
    }
}
impl Handler for Hub {
    fn handle(
        &self,
        session: &mut IngestSession,
        frame: &[u8],
        _transport_now: f64,
    ) -> Result<String, IslandError> {
        self.mutate(|store, now| {
            let result = session.handle(store, frame, now);
            Ok((
                result
                    .reply
                    .to_line()
                    .map_err(|_| IslandError::internal("Ingest encoding failed"))?,
                result.changes,
            ))
        })
    }
}
fn core_error(_: Error) -> IslandError {
    IslandError::invalid("Invalid notification data or policy")
}

pub struct Notifications {
    pub hub: Arc<Hub>,
    stop: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<Result<(), IslandError>>>>,
    ingest: Mutex<Option<IngestServer>>,
    monitor_task: Mutex<Option<tauri::async_runtime::JoinHandle<Result<(), IslandError>>>>,
}
impl Notifications {
    pub fn start(
        app: tauri::AppHandle,
        actor: Handle,
        settings: Arc<SettingsService>,
        data: &Path,
    ) -> Result<Self, IslandError> {
        let hub = Hub::new(actor, settings);
        let (stop, stopped) = watch::channel(false);
        let ingest = IngestServer::start(data, hub.clone())?;
        hub.refresh_policy()?;
        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
        let monitor_task = tauri::async_runtime::spawn(hub.monitor.clone().run(
            hub.clone(),
            address,
            hub.mirror_enabled.subscribe(),
            stopped.clone(),
        ));
        let task = tauri::async_runtime::spawn(hub.clone().run(app, stopped));
        Ok(Self {
            hub,
            stop,
            task: Mutex::new(Some(task)),
            ingest: Mutex::new(Some(ingest)),
            monitor_task: Mutex::new(Some(monitor_task)),
        })
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        self.stop.send_replace(true);
        let ingest = self
            .ingest
            .lock()
            .map_err(|_| IslandError::internal("Ingest slot poisoned"))?
            .take();
        let task = self
            .task
            .lock()
            .map_err(|_| IslandError::internal("Notification slot poisoned"))?
            .take();
        let monitor_task = self
            .monitor_task
            .lock()
            .map_err(|_| IslandError::internal("Monitor slot poisoned"))?
            .take();
        tauri::async_runtime::block_on(async move {
            if let Some(mut ingest) = ingest {
                ingest.shutdown().await?;
            }
            if let Some(task) = task {
                tokio::time::timeout(Duration::from_secs(2), task)
                    .await
                    .map_err(|_| IslandError::timeout("Notification shutdown"))?
                    .map_err(|_| IslandError::internal("Notification task panicked"))??;
            }
            if let Some(task) = monitor_task {
                tokio::time::timeout(Duration::from_secs(3), task)
                    .await
                    .map_err(|_| IslandError::timeout("Monitor shutdown"))?
                    .map_err(|_| IslandError::internal("Monitor task panicked"))??;
            }
            Ok(())
        })
    }
}
impl From<IslandNotification> for wire::IslandNotification {
    fn from(n: IslandNotification) -> Self {
        Self {
            rev: n.rev,
            id: n.id,
            source: match n.source {
                NotificationSource::System => wire::NotificationSource::System,
                NotificationSource::Hermes => wire::NotificationSource::Hermes,
                NotificationSource::Alert => wire::NotificationSource::Alert,
            },
            app_name: n.app_name,
            title: n.title,
            body: n.body,
            icon_png: n.icon_png,
            urgency: match n.urgency {
                Urgency::Low => wire::Urgency::Low,
                Urgency::Normal => wire::Urgency::Normal,
                Urgency::Critical => wire::Urgency::Critical,
            },
            received_at: n.received_at,
            actions: n
                .actions
                .into_iter()
                .map(|a| wire::NotificationAction {
                    key: a.key,
                    label: a.label,
                })
                .collect(),
            can_invoke_actions: false,
            progress: n.progress,
        }
    }
}
impl From<CloseReason> for wire::CloseReason {
    fn from(r: CloseReason) -> Self {
        match r {
            CloseReason::Expired => Self::Expired,
            CloseReason::Dismissed => Self::Dismissed,
            CloseReason::Cleared => Self::Cleared,
            CloseReason::Evicted => Self::Evicted,
            CloseReason::Filtered => Self::Filtered,
            CloseReason::Replaced => Self::Replaced,
            CloseReason::ServerClosed => Self::ServerClosed,
        }
    }
}
