//! Serialized notification storage and bounded event delivery. Only StateActor emits mode.
use super::{
    hermes_ingest::{Handler, IngestServer},
    ingest::IngestSession,
    *,
};
use crate::{
    bus::COALESCE,
    contract::{notify as wire, system::ServiceStatus},
    error::IslandError,
    integrations::{
        pump::{deliver, supervise, Backoff, Delivery, PumpHealth, RestartPolicy},
        service_status::Reporter,
    },
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
    popups: popups::Popups,
    routes: std::collections::HashMap<String, Route>,
}
#[derive(Clone)]
pub struct Route {
    pub owner: String,
    pub server_id: u32,
    pub desktop_entry: Option<String>,
    pub live: bool,
    pub default_action: bool,
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
    pub takeover_enabled: watch::Sender<bool>,
    pub inhibited: std::sync::atomic::AtomicBool,
    #[cfg(target_os = "linux")]
    sound: effects::Sound,
}
impl Hub {
    pub fn new(actor: Handle, settings: Arc<SettingsService>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                store: NotificationStore::default(),
                pending: VecDeque::new(),
                repair_needed: false,
                popups: popups::Popups::default(),
                routes: std::collections::HashMap::new(),
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
            takeover_enabled: watch::channel(false).0,
            inhibited: std::sync::atomic::AtomicBool::new(false),
            #[cfg(target_os = "linux")]
            sound: effects::Sound::default(),
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
        let mut settings = self.settings.get()?;
        settings.dnd = self.actor.state().dnd;
        settings.game_mode = self.actor.state().game_mode;
        let mut policy = inner.store.policy().clone();
        policy.dnd = settings.dnd;
        policy.redact_bodies = settings.redact_bodies;
        policy.denied_apps = settings.notification_denied_apps.iter().cloned().collect();
        inner.popups.configure(&settings);
        self.takeover_enabled.send_if_modified(|enabled| {
            let next = settings.silence_kde_popups
                && settings.notifications_enabled
                && settings.system_notification_mirror
                && self.monitor.status().available;
            let changed = *enabled != next;
            *enabled = next;
            changed
        });
        self.mirror_enabled.send_if_modified(|enabled| {
            let changed = *enabled != settings.system_notification_mirror;
            *enabled = settings.system_notification_mirror;
            changed
        });
        let privacy = inner.store.set_policy(policy).map_err(core_error)?;
        self.queue(&mut inner, privacy);
        let (value, changes) = work(&mut inner.store, self.now())?;
        inner.store.retain_history();
        self.queue(&mut inner, changes);
        drop(inner);
        self.wake.notify_one();
        Ok(value)
    }
    fn queue(&self, inner: &mut Inner, changes: Changes) {
        if changes.events.is_empty() && changes.close_server_ids.is_empty() {
            return;
        }
        inner.popups.tick(self.origin.elapsed().as_millis() as u64);
        // Capacity is EVENTS, not batches (clear/privacy operations can emit 200 deltas).
        let mut close_server_ids = changes.close_server_ids;
        for mut event in changes.events {
            if let Delta::NotificationReceived(n) = &mut event {
                if changes.id.as_deref() == Some(n.0.id.as_str()) {
                    if let Ok(settings) = self.settings.get() {
                        n.0.urgency = popups::urgency(&settings, &n.0.app_name, n.0.urgency);
                        inner.store.override_urgency(&n.0.id, n.0.urgency);
                    }
                }
            }
            match &event {
                Delta::NotificationReceived(n)
                    if changes.id.as_deref() == Some(n.0.id.as_str()) =>
                {
                    if let Ok(mut settings) = self.settings.get() {
                        settings.dnd = self.actor.state().dnd;
                        settings.game_mode = self.actor.state().game_mode;
                        let mut notification: wire::IslandNotification = n.0.clone().into();
                        notification.can_invoke_actions =
                            inner.routes.get(&notification.id).is_some_and(|r| r.live);
                        inner.popups.insert(
                            notification,
                            &settings,
                            self.origin.elapsed().as_millis() as u64,
                        );
                    }
                }
                Delta::NotificationReceived(n) => inner.popups.update(n.0.clone().into()),
                Delta::NotificationClosed(c) => {
                    inner.popups.remove(&c.id);
                    inner.routes.remove(&c.id);
                }
            }
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
    pub fn insert(&self, mut input: NotificationInput) -> Result<(), IslandError> {
        let settings = self.settings.get()?;
        input.urgency = popups::urgency(&settings, &input.app_name, input.urgency);
        // History outlives presentation. The server still owns its own lifecycle.
        input.ttl_ms = Some(0.);
        self.mutate(|store, now| Ok(((), store.insert(input, now).map_err(core_error)?)))
    }
    #[cfg(target_os = "linux")]
    pub fn insert_system(&self, n: dbus::DbusNotification, owner: &str) -> Result<(), IslandError> {
        let mut settings = self.settings.get()?;
        settings.dnd = self.actor.state().dnd;
        settings.game_mode = self.actor.state().game_mode;
        if let Some(server_id) = n.input.server_id {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| IslandError::internal("Notification routes poisoned"))?;
            // Bounded by history, including rejected notifications.
            if inner.routes.len() >= STORE_CAPACITY {
                let ids: std::collections::HashSet<_> =
                    inner.store.list().into_iter().map(|n| n.id).collect();
                inner.routes.retain(|id, _| ids.contains(id));
            }
            inner.routes.insert(
                format!("d-{server_id}"),
                Route {
                    owner: owner.into(),
                    server_id,
                    desktop_entry: n.desktop_entry.clone(),
                    live: true,
                    default_action: n.input.actions.iter().any(|a| a.key == "default"),
                },
            );
        }
        self.sound
            .play(&n, &settings, self.inhibited.load(Ordering::Relaxed));
        self.insert(n.input)
    }
    pub fn popup_snapshot(&self) -> Result<popups::NotificationPopups, IslandError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| IslandError::internal("Notification popups poisoned"))?;
        inner.popups.tick(self.origin.elapsed().as_millis() as u64);
        Ok(inner.popups.snapshot())
    }
    pub fn popup_change(&self, id: &str, paused: Option<bool>) -> Result<(), IslandError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| IslandError::internal("Notification popups poisoned"))?;
        if let Some(paused) = paused {
            inner
                .popups
                .pause(id, paused, self.origin.elapsed().as_millis() as u64);
        } else {
            inner.popups.tick(self.origin.elapsed().as_millis() as u64);
            inner.popups.remove(id);
        }
        drop(inner);
        self.wake.notify_one();
        Ok(())
    }
    pub fn action_route(&self, id: &str, key: &str) -> Result<Route, IslandError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| IslandError::internal("Notification routes poisoned"))?;
        let item = inner
            .store
            .list()
            .into_iter()
            .find(|n| n.id == id)
            .ok_or_else(|| IslandError::unavailable("Notification is no longer available"))?;
        if key != "default"
            && !item
                .actions
                .iter()
                .any(|a| a.key == key && key != "inline-reply")
        {
            return Err(IslandError::invalid("Notification action was not offered"));
        }
        inner
            .routes
            .get(id)
            .cloned()
            .ok_or_else(|| IslandError::unavailable("Notification has no application route"))
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
    pub fn server_expired(&self, id: u32) -> Result<(), IslandError> {
        {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| IslandError::internal("Notification routes poisoned"))?;
            if let Some(route) = inner.routes.get_mut(&format!("d-{id}")) {
                route.live = false;
            }
        }
        self.mutate(|store, _| Ok(((), store.deactivate_server(id))))
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
    /// One delivery pump. It ends with `Ok` only on a stop request or when the state actor is
    /// gone; a full actor queue or a failed emit is retried with bounded backoff, and anything
    /// it could not deliver is re-sent as a fresh snapshot (`repair_needed`), never dropped.
    /// `resync`: a restarted pump re-sends the full list and popups the page may have missed.
    async fn run(
        self: Arc<Self>,
        out: Arc<dyn HubOutlet>,
        mut stopped: watch::Receiver<bool>,
        resync: bool,
    ) -> Result<(), IslandError> {
        let mut state_changes = self.actor.watch();
        // Popup ids last emitted: a new id grows the notch into the card, an empty queue
        // lets a notification-caused hover close (popups.rs owns the card lifetimes).
        let mut popup_ids: Vec<String> = Vec::new();
        // The newest popup intent the actor has not accepted yet (it was Busy).
        let mut owed: Option<Intent> = None;
        let mut backoff = Backoff::new(Duration::from_millis(50), Duration::from_secs(5));
        let mut failing = 0u32;
        if resync {
            let mut inner = self.lock_inner()?;
            inner.repair_needed = true;
            inner.popups.mark_dirty();
        }
        loop {
            // Register before inspecting the queue, avoiding lost wakeups between drain/wait.
            let notified = self.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (pending, unread, expiry, repair, popup_state) = {
                let mut inner = self.lock_inner()?;
                let expired = inner.store.expire(self.now()).map_err(core_error)?;
                self.queue(&mut inner, expired);
                let now = self.origin.elapsed().as_millis() as u64;
                inner.popups.tick(now);
                let popup_deadline = inner.popups.deadline(now).map(|at| self.epoch + at as f64);
                let expiry = match (inner.store.next_expiry(), popup_deadline) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                let popup_state = inner.popups.take_dirty().then(|| inner.popups.snapshot());
                (
                    inner.pending.drain(..).collect::<Vec<_>>(),
                    inner.store.len() as u32,
                    expiry,
                    if std::mem::take(&mut inner.repair_needed) {
                        Some(wire::NotificationSnapshot {
                            rev: inner.store.current_revision(),
                            items: inner.store.list().into_iter().map(Into::into).collect(),
                        })
                    } else {
                        None
                    },
                    popup_state,
                )
            };
            // `emit_failed`: the page missed something; it is re-synced from a snapshot next
            // round. `failed`: anything to retry (a failed emit or a Busy actor), after backoff.
            let mut emit_failed = false;
            let mut failed = false;
            if let Some(popups) = popup_state {
                let ids: Vec<String> = popups
                    .items
                    .iter()
                    .map(|p| p.notification.id.clone())
                    .collect();
                let fresh = ids.iter().find(|id| !popup_ids.contains(id)).cloned();
                let emptied = ids.is_empty() && !popup_ids.is_empty();
                if let Err(error) = out.send(Outbound::Popups(popups)) {
                    tracing::debug!(%error, "Notification popups delivery failed");
                    self.lock_inner()?.popups.mark_dirty();
                    emit_failed = true;
                } else {
                    popup_ids = ids;
                    if let Some(id) = fresh {
                        owed = Some(Intent::PopupShown(id));
                    } else if emptied {
                        owed = Some(Intent::PopupsEmpty);
                    }
                }
            }
            if let Some(intent) = owed.take() {
                let actor = &self.actor;
                match deliver(|| actor.dispatch(intent.clone()), &mut stopped).await {
                    Delivery::Done(_) => {}
                    Delivery::Busy => {
                        owed = Some(intent);
                        failed = true;
                    }
                    Delivery::Stopped => break,
                    Delivery::Failed(_) if !self.actor.is_running() => break,
                    Delivery::Failed(error) => {
                        tracing::warn!(%error, "State actor refused a popup intent")
                    }
                }
            }
            if let Some(repair) = repair {
                if let Err(error) = out.send(Outbound::Snapshot(repair)) {
                    tracing::debug!(%error, "NotificationSnapshot delivery failed");
                    emit_failed = true;
                }
            }
            for (changes, close_owner) in pending {
                // After a failed emit the rest of the drained deltas are not sent one by one:
                // the snapshot below carries the store as it is now.
                if !emit_failed {
                    if let Err(error) = self.emit_changes(out.as_ref(), changes.events) {
                        tracing::debug!(%error, "Notification event delivery failed");
                        emit_failed = true;
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
            if emit_failed {
                self.lock_inner()?.repair_needed = true;
                failed = true;
            }
            let actor = &self.actor;
            match deliver(|| actor.dispatch(Intent::NotificationsSync(unread)), &mut stopped).await {
                Delivery::Done(_) => {}
                // The count is recomputed and sent again next round.
                Delivery::Busy => failed = true,
                Delivery::Stopped => break,
                Delivery::Failed(_) if !self.actor.is_running() => break,
                Delivery::Failed(error) => {
                    tracing::warn!(%error, "State actor refused the notification count")
                }
            }
            let retry = if failed {
                failing = failing.saturating_add(1);
                if failing == 1 {
                    tracing::warn!("Notification delivery is failing; retrying with backoff");
                }
                Some(backoff.next_delay())
            } else {
                if failing > 0 {
                    tracing::info!(rounds = failing, "Notification delivery recovered");
                }
                failing = 0;
                backoff.reset();
                None
            };
            let wait = async {
                let expiry = expiry
                    .map(|deadline| Duration::from_secs_f64(((deadline - self.now()).max(0.)) / 1000.));
                match (expiry, retry) {
                    (Some(a), Some(b)) => tokio::time::sleep(a.min(b)).await,
                    (Some(a), None) | (None, Some(a)) => tokio::time::sleep(a).await,
                    (None, None) => std::future::pending::<()>().await,
                }
            };
            tokio::select! { biased;
                result=stopped.changed()=>{if result.is_err()||*stopped.borrow(){break;}},
                // A retry waits out its backoff even when new work arrives: no retry storm.
                ()=&mut notified, if retry.is_none() =>{},
                changed=state_changes.changed()=>{ if changed.is_err() { break; } self.refresh_policy()?; },
                ()=wait=>{},
            }
        }
        Ok(())
    }
    fn lock_inner(&self) -> Result<std::sync::MutexGuard<'_, Inner>, IslandError> {
        self.inner
            .lock()
            .map_err(|_| IslandError::internal("Notification store poisoned"))
    }
    /// Emits one drained batch; stops at the first failed emit.
    fn emit_changes(&self, out: &dyn HubOutlet, events: Vec<Delta>) -> Result<(), IslandError> {
        for delta in events {
            match delta {
                Delta::NotificationReceived(n) => {
                    // A drained batch can be superseded by privacy/close while awaiting the
                    // actor. Never re-emit an obsolete private payload. Recheck and hold
                    // serialization through emit, so a concurrent scrub cannot be undone.
                    let inner = self.lock_inner()?;
                    if inner.store.is_current(&n.0.id, n.0.rev) {
                        out.send(Outbound::Received(wire::NotificationReceived(n.0.into())))?;
                    }
                }
                Delta::NotificationClosed(c) => out.send(Outbound::Closed(wire::NotificationClosed {
                    rev: c.rev,
                    id: c.id,
                    reason: c.reason.into(),
                }))?,
            }
        }
        Ok(())
    }
}

/// Everything the hub sends to the page.
#[derive(Debug)]
pub enum Outbound {
    Popups(popups::NotificationPopups),
    Snapshot(wire::NotificationSnapshot),
    Received(wire::NotificationReceived),
    Closed(wire::NotificationClosed),
    Status(ServiceStatus),
}
/// Where hub events go: the island window in the app, a recorder in tests.
pub trait HubOutlet: Send + Sync + 'static {
    fn send(&self, event: Outbound) -> Result<(), IslandError>;
}
impl HubOutlet for tauri::AppHandle {
    fn send(&self, event: Outbound) -> Result<(), IslandError> {
        let sent = match event {
            Outbound::Popups(e) => e.emit_to(self, ISLAND_LABEL),
            Outbound::Snapshot(e) => e.emit_to(self, ISLAND_LABEL),
            Outbound::Received(e) => e.emit_to(self, ISLAND_LABEL),
            Outbound::Closed(e) => e.emit_to(self, ISLAND_LABEL),
            Outbound::Status(e) => e.emit_to(self, ISLAND_LABEL),
        };
        sent.map_err(|e| IslandError::internal(format!("Notification event emit failed: {e}")))
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
    /// `notifications` availability: delivery pump health, plus whether Hermes' ingest
    /// socket could be opened (desktop notifications work without it).
    pub status: Arc<Reporter>,
    stop: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<Result<(), IslandError>>>>,
    ingest: Mutex<Option<IngestServer>>,
    monitor_task: Mutex<Option<tauri::async_runtime::JoinHandle<Result<(), IslandError>>>>,
    #[cfg(target_os = "linux")]
    takeover_task: Mutex<Option<tauri::async_runtime::JoinHandle<Result<(), IslandError>>>>,
}
impl Notifications {
    pub fn start(
        app: tauri::AppHandle,
        actor: Handle,
        settings: Arc<SettingsService>,
        data: &Path,
    ) -> Result<Self, IslandError> {
        let bus = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
        Self::start_with(Arc::new(app), actor, settings, data, bus, RestartPolicy::standard())
    }
    /// `start` with the outlet, session bus address and restart policy supplied (tests use a
    /// recorder, a bus that does not exist and short pauses).
    pub fn start_with(
        out: Arc<dyn HubOutlet>,
        actor: Handle,
        settings: Arc<SettingsService>,
        data: &Path,
        bus: String,
        policy: RestartPolicy,
    ) -> Result<Self, IslandError> {
        let hub = Hub::new(actor, settings);
        let (stop, stopped) = watch::channel(false);
        let status = Reporter::new("notifications");
        // Optional: without the socket only Hermes' own alerts are missing. A stale socket
        // left by a crash used to stop the whole island from starting.
        let ingest = match IngestServer::start(data, hub.clone()) {
            Ok(ingest) => Some(ingest),
            Err(error) => {
                tracing::warn!(%error, "Hermes notification socket unavailable; desktop notifications still work");
                None
            }
        };
        let ingest_ok = ingest.is_some();
        if let Err(error) = hub.refresh_policy() {
            // The policy is refreshed again on every state change.
            tracing::warn!(%error, "Notification policy not applied at start");
        }
        let forward = out.clone();
        tauri::async_runtime::spawn(status.topic.forwarder(COALESCE, move |value: ServiceStatus| {
            if let Err(error) = forward.send(Outbound::Status(value)) {
                tracing::debug!(%error, "Notification status delivery failed");
            }
        }));
        let monitor_task = tauri::async_runtime::spawn(hub.monitor.clone().run(
            hub.clone(),
            bus.clone(),
            hub.mirror_enabled.subscribe(),
            stopped.clone(),
        ));
        let (pump_hub, pump_stop, report) = (hub.clone(), stopped.clone(), status.clone());
        let mut restarted = false;
        let task = tauri::async_runtime::spawn(async move {
            supervise(
                "notifications",
                policy,
                pump_stop.clone(),
                move || {
                    let resync = std::mem::replace(&mut restarted, true);
                    pump_hub.clone().run(out.clone(), pump_stop.clone(), resync)
                },
                move |health| match health {
                    PumpHealth::Running if ingest_ok => report.available("ready"),
                    PumpHealth::Running => report.unavailable("ingest-unavailable"),
                    PumpHealth::Restarting => report.unavailable("delivery-restarting"),
                    PumpHealth::Stopped => report.unavailable("delivery-stopped"),
                },
            )
            .await;
            Ok(())
        });
        #[cfg(target_os = "linux")]
        let takeover_task = tauri::async_runtime::spawn(takeover::run(
            hub.clone(),
            bus,
            hub.takeover_enabled.subscribe(),
            stopped,
        ));
        Ok(Self {
            hub,
            status,
            stop,
            task: Mutex::new(Some(task)),
            ingest: Mutex::new(ingest),
            monitor_task: Mutex::new(Some(monitor_task)),
            #[cfg(target_os = "linux")]
            takeover_task: Mutex::new(Some(takeover_task)),
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
        #[cfg(target_os = "linux")]
        let takeover_task = self
            .takeover_task
            .lock()
            .map_err(|_| IslandError::internal("Inhibitor slot poisoned"))?
            .take();
        tauri::async_runtime::block_on(async move {
            #[cfg(target_os = "linux")]
            if let Some(task) = takeover_task {
                tokio::time::timeout(Duration::from_secs(3), task)
                    .await
                    .map_err(|_| IslandError::timeout("KDE inhibitor shutdown"))?
                    .map_err(|_| IslandError::internal("KDE inhibitor task panicked"))??;
            }
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
