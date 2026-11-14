//! File-copy progress: a passive session-bus monitor of KDE job views (`core` decodes).
//!
//! Plasma owns `org.kde.JobViewServer`; KDE apps (Dolphin, any KIO client with a job tracker)
//! report copies and moves to it. The island only watches: a second connection is turned into a
//! receive-only monitor (`BecomeMonitor`), exactly like the notification mirror, and never
//! sends anything on it. A third party cannot cancel a KDE job (KIO accepts `cancelRequested`
//! only from the server), so the island offers "Show in folder", not Cancel.
//! Event-driven: nothing polls; the loop wakes for bus messages and expiry deadlines only.
pub mod core;

use crate::{
    bus::Latest,
    contract::apps::{TransferRow, TransfersChanged},
    error::IslandError,
    integrations::downloads::Opener,
};
use std::{
    ffi::OsString,
    future::poll_fn,
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use zbus::{export::futures_core::Stream, Connection, Message, MessageStream};

const DBUS: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const CALL_TIMEOUT: Duration = Duration::from_secs(2);

struct Shared {
    rev: u32,
    available: bool,
    detail: String,
    rows: Vec<TransferRow>,
    dests: Vec<(String, String)>,
}

pub struct TransfersService {
    pub topic: Latest<TransfersChanged>,
    progress: watch::Sender<Option<u8>>,
    enabled: watch::Sender<bool>,
    stop: watch::Sender<bool>,
    shared: Mutex<Shared>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    opener: Arc<dyn Opener>,
}

async fn next(stream: &mut MessageStream) -> Option<zbus::Result<Message>> {
    poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}
fn transport(_: zbus::Error) -> IslandError {
    IslandError::unavailable("Job bus transport failed")
}
async fn connect(address: &str) -> Result<Connection, IslandError> {
    tokio::time::timeout(
        CALL_TIMEOUT,
        zbus::connection::Builder::address(address).build(),
    )
    .await
    .map_err(|_| IslandError::timeout("Job bus connect"))?
    .map_err(transport)
}
async fn owner(a: &Connection) -> Result<Option<String>, IslandError> {
    match tokio::time::timeout(
        CALL_TIMEOUT,
        a.call_method(
            Some(DBUS),
            DBUS_PATH,
            Some(DBUS),
            "GetNameOwner",
            &(core::SERVER_NAME,),
        ),
    )
    .await
    {
        Ok(Ok(m)) => m
            .body()
            .deserialize::<String>()
            .map(Some)
            .map_err(transport),
        Ok(Err(zbus::Error::FDO(e))) if matches!(*e, zbus::fdo::Error::NameHasNoOwner(_)) => {
            Ok(None)
        }
        Ok(Err(e)) => Err(transport(e)),
        Err(_) => Err(IslandError::timeout("Job server owner lookup")),
    }
}

/// The match rules handed to `BecomeMonitor` for the current server owner.
pub fn monitor_rules(owner: &str) -> Vec<String> {
    vec![
        format!(
            "type='method_call',interface='{}',member='requestView'",
            core::SERVER_V2
        ),
        format!(
            "type='method_call',interface='{}',member='requestView'",
            core::SERVER_V1
        ),
        format!("type='method_return',sender='{owner}'"),
        format!("type='method_call',interface='{}'", core::VIEW_V3),
        format!("type='method_call',interface='{}'", core::VIEW_V2),
    ]
}

impl TransfersService {
    /// Starts the monitor task. `address` is the session bus (`None`: stays unavailable).
    pub fn start(address: Option<String>, enabled: bool, opener: Arc<dyn Opener>) -> Arc<Self> {
        let service = Arc::new(Self {
            topic: Latest::new(),
            progress: watch::channel(None).0,
            enabled: watch::channel(enabled).0,
            stop: watch::channel(false).0,
            shared: Mutex::new(Shared {
                rev: 0,
                available: false,
                detail: "starting".into(),
                rows: Vec::new(),
                dests: Vec::new(),
            }),
            task: Mutex::new(None),
            opener,
        });
        service.publish(|_| {});
        let Some(address) = address else {
            service.publish(|s| s.detail = "bus-unavailable".into());
            return service;
        };
        let worker = Arc::clone(&service);
        let task = tauri::async_runtime::spawn(async move { worker.run(address).await });
        if let Ok(mut slot) = service.task.lock() {
            *slot = Some(task);
        }
        service
    }

    /// The notch's aggregate: `None` while no job runs, else 0..=100.
    pub fn progress(&self) -> watch::Receiver<Option<u8>> {
        self.progress.subscribe()
    }

    pub fn configure(&self, enabled: bool) {
        self.enabled.send_if_modified(|v| std::mem::replace(v, enabled) != enabled);
    }

    pub fn current(&self) -> TransfersChanged {
        match self.shared.lock() {
            Ok(s) => TransfersChanged {
                rev: s.rev,
                available: s.available,
                detail: s.detail.clone(),
                items: s.rows.clone(),
            },
            Err(_) => TransfersChanged {
                rev: 0,
                available: false,
                detail: "monitor-state-poisoned".into(),
                items: Vec::new(),
            },
        }
    }

    fn publish(&self, f: impl FnOnce(&mut Shared)) {
        let Ok(mut s) = self.shared.lock() else {
            return;
        };
        f(&mut s);
        s.rev = s.rev.wrapping_add(1);
        let value = TransfersChanged {
            rev: s.rev,
            available: s.available,
            detail: s.detail.clone(),
            items: s.rows.clone(),
        };
        drop(s);
        self.topic.publish(value);
    }

    fn show(&self, tracker: &core::Tracker) {
        let rows = tracker.rows();
        let dests = rows
            .iter()
            .filter_map(|r| Some((r.id.clone(), tracker.dest_url(&r.id)?.to_owned())))
            .collect();
        self.publish(|s| {
            s.rows = rows;
            s.dests = dests;
        });
        let aggregate = tracker.aggregate();
        self.progress
            .send_if_modified(|v| std::mem::replace(v, aggregate) != aggregate);
    }

    /// "Show in folder" for a running job. The path comes from the job's own `destUrl`, never
    /// from the page: a folder opens as is, a file is selected in Dolphin.
    pub fn reveal(&self, id: &str) -> Result<(), IslandError> {
        let url = self
            .shared
            .lock()
            .map_err(|_| IslandError::internal("Transfers state poisoned"))?
            .dests
            .iter()
            .find(|(job, _)| job == id)
            .map(|(_, url)| url.clone())
            .ok_or_else(|| IslandError::unavailable("Transfer no longer running"))?;
        let path = core::local_path(&url)
            .ok_or_else(|| IslandError::unsupported("Transfer destination is not a local folder"))?;
        let meta = std::fs::symlink_metadata(&path);
        let argv = match meta {
            Ok(m) if m.is_dir() => vec![OsString::from("xdg-open"), path.into_os_string()],
            Ok(m) if m.is_file() => vec![
                OsString::from("dolphin"),
                OsString::from("--select"),
                path.into_os_string(),
            ],
            _ => {
                let parent = path
                    .parent()
                    .filter(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()))
                    .ok_or_else(|| IslandError::unavailable("Transfer folder not found"))?;
                vec![OsString::from("xdg-open"), parent.as_os_str().to_owned()]
            }
        };
        self.opener.open(&argv)
    }

    pub fn shutdown(&self) {
        self.stop.send_replace(true);
        if let Some(task) = self.task.lock().ok().and_then(|mut t| t.take()) {
            task.abort();
        }
    }

    async fn run(self: Arc<Self>, address: String) {
        let mut enabled = self.enabled.subscribe();
        let mut stopped = self.stop.subscribe();
        let mut backoff = 1;
        loop {
            if *stopped.borrow() {
                break;
            }
            if !*enabled.borrow_and_update() {
                self.show(&core::Tracker::default());
                self.publish(|s| {
                    s.available = false;
                    s.detail = "disabled".into();
                });
                tokio::select! {
                    _ = stopped.changed() => break,
                    r = enabled.changed() => if r.is_err() { break },
                }
                continue;
            }
            let result = tokio::select! {
                _ = stopped.changed() => break,
                r = self.session(&address, &mut enabled) => r,
            };
            self.show(&core::Tracker::default());
            match result {
                Ok(()) => backoff = 1,
                Err(error) => {
                    tracing::warn!(%error, "File-copy monitor disconnected");
                    self.publish(|s| {
                        s.available = false;
                        s.detail = "bus-unavailable".into();
                    });
                    tokio::select! {
                        _ = stopped.changed() => break,
                        _ = enabled.changed() => {},
                        () = tokio::time::sleep(Duration::from_secs(backoff)) => {},
                    }
                    backoff = (backoff * 2).min(60);
                }
            }
        }
        self.show(&core::Tracker::default());
        self.publish(|s| {
            s.available = false;
            s.detail = "stopped".into();
        });
    }

    async fn session(
        &self,
        address: &str,
        enabled: &mut watch::Receiver<bool>,
    ) -> Result<(), IslandError> {
        let a = connect(address).await?;
        // Every NameOwnerChanged: the server's owner changing re-arms the monitor, and a client
        // leaving the bus ends its jobs (a crashed copy never leaves a stuck row).
        let rule = "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'";
        let mut names = MessageStream::for_match_rule(rule, &a, NonZeroUsize::new(64))
            .await
            .map_err(transport)?;
        let mut current = owner(&a).await?;
        let mut stream = self.arm(address, current.as_deref()).await?;
        let mut tracker = core::Tracker::default();
        loop {
            let deadline = tracker.next_expiry();
            let expiry = async {
                match deadline {
                    Some(at) => tokio::time::sleep_until(at.into()).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! { biased;
                changed = enabled.changed() => {
                    if changed.is_err() || !*enabled.borrow_and_update() { return Ok(()); }
                },
                () = expiry => {
                    if tracker.expire(Instant::now()) { self.show(&tracker); }
                },
                message = next(&mut names) => {
                    let message = message.ok_or_else(|| IslandError::unavailable("Owner stream ended"))?.map_err(transport)?;
                    if message.header().sender().map(|s| s.as_str()) != Some(DBUS) { continue; }
                    let Ok((name, _, new)) = message.body().deserialize::<(String, String, String)>() else { continue };
                    if name == core::SERVER_NAME {
                        let replacement = owner(&a).await?;
                        if replacement != current {
                            drop(stream.take());
                            tracker.reset();
                            self.show(&tracker);
                            current = replacement;
                            stream = self.arm(address, current.as_deref()).await?;
                        }
                    } else if new.is_empty() && name.starts_with(':') && tracker.client_gone(&name) {
                        self.show(&tracker);
                    }
                },
                message = async { match stream.as_mut() { Some(s) => next(s).await, None => std::future::pending().await } } => {
                    let message = message.ok_or_else(|| IslandError::unavailable("Monitor stream ended"))?.map_err(transport)?;
                    if tracker.observe(&message, current.as_deref().unwrap_or(""), Instant::now()) {
                        self.show(&tracker);
                    }
                },
            }
        }
    }

    async fn arm(
        &self,
        address: &str,
        owner: Option<&str>,
    ) -> Result<Option<MessageStream>, IslandError> {
        let Some(owner) = owner else {
            self.publish(|s| {
                s.available = false;
                s.detail = "no-owner".into();
            });
            return Ok(None);
        };
        let b = connect(address).await?;
        // Start reading before BecomeMonitor so its first traffic cannot be lost.
        let stream = MessageStream::from(&b);
        let reply = tokio::time::timeout(
            CALL_TIMEOUT,
            b.call_method(
                Some(DBUS),
                DBUS_PATH,
                Some("org.freedesktop.DBus.Monitoring"),
                "BecomeMonitor",
                &(monitor_rules(owner), 0u32),
            ),
        )
        .await;
        match reply {
            Ok(Ok(_)) => {
                self.publish(|s| {
                    s.available = true;
                    s.detail = "monitoring".into();
                });
                // Only the receive stream escapes. Never call AddMatch or send on B again.
                Ok(Some(stream))
            }
            Ok(Err(zbus::Error::FDO(e))) if matches!(*e, zbus::fdo::Error::AccessDenied(_)) => {
                self.publish(|s| {
                    s.available = false;
                    s.detail = "monitor-permission-denied".into();
                });
                Ok(None)
            }
            Ok(Err(e)) => Err(transport(e)),
            Err(_) => Err(IslandError::timeout("BecomeMonitor timed out")),
        }
    }
}

impl Drop for TransfersService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
