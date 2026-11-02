//! Passive mirror. A remains a normal bus peer; B is consumed into a receive-only stream.
//! Neither connection ever owns org.freedesktop.Notifications.
use super::{
    dbus::{HintValue, ImageData, NotifyArgs, NotifyCorrelator, CORRELATION_CAPACITY},
    hub::Hub,
};
use crate::{contract::system::NotificationMonitorStatus, error::IslandError};
use std::{
    collections::{BTreeMap, VecDeque},
    future::poll_fn,
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use zbus::{
    export::futures_core::Stream, message::Type, Connection, Message, MessageStream, OwnedVariant,
};

pub const NAME: &str = "org.freedesktop.Notifications";
pub const PATH: &str = "/org/freedesktop/Notifications";
const DBUS: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const CALL_TIMEOUT: Duration = Duration::from_secs(2);
const PENDING_TTL: Duration = Duration::from_secs(5);
const MAX_MESSAGE: usize = 128 * 1024;

pub async fn next(stream: &mut MessageStream) -> Option<zbus::Result<Message>> {
    poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}
fn transport(_: zbus::Error) -> IslandError {
    IslandError::unavailable("Notification bus transport failed")
}
async fn connect(address: &str) -> Result<Connection, IslandError> {
    tokio::time::timeout(
        CALL_TIMEOUT,
        zbus::connection::Builder::address(address).build(),
    )
    .await
    .map_err(|_| IslandError::timeout("Notification bus connect"))?
    .map_err(transport)
}
async fn owner(a: &Connection) -> Result<Option<String>, IslandError> {
    match tokio::time::timeout(
        CALL_TIMEOUT,
        a.call_method(Some(DBUS), DBUS_PATH, Some(DBUS), "GetNameOwner", &(NAME,)),
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
        Err(_) => Err(IslandError::timeout("Notification owner lookup")),
    }
}
#[derive(Default)]
pub struct Monitor {
    control: Mutex<Option<(Connection, String)>>,
    report: Mutex<NotificationMonitorStatus>,
}
impl Monitor {
    pub fn status(&self) -> NotificationMonitorStatus {
        self.report
            .lock()
            .map(|v| v.clone())
            .unwrap_or_else(|_| NotificationMonitorStatus {
                detail: "monitor-state-poisoned".into(),
                ..Default::default()
            })
    }
    fn update(&self, f: impl FnOnce(&mut NotificationMonitorStatus)) -> Result<(), IslandError> {
        f(&mut *self
            .report
            .lock()
            .map_err(|_| IslandError::internal("Monitor status poisoned"))?);
        Ok(())
    }
    pub fn current_owner(&self) -> Option<String> {
        self.control
            .lock()
            .ok()
            .and_then(|c| c.as_ref().map(|(_, owner)| owner.clone()))
    }
    /// A queued close captures the owner at local removal. Recheck the well-known owner,
    /// then address the UNIQUE peer: an owner replacement cannot close a reused id.
    pub async fn close(&self, expected: Option<&str>, id: u32) -> Result<bool, IslandError> {
        let control = self
            .control
            .lock()
            .map_err(|_| IslandError::internal("Monitor control poisoned"))?
            .clone();
        let Some((a, current)) = control else {
            return Ok(false);
        };
        if expected != Some(current.as_str())
            || owner(&a).await?.as_deref() != Some(current.as_str())
        {
            return Ok(false);
        }
        tokio::time::timeout(
            CALL_TIMEOUT,
            a.call_method(
                Some(current.as_str()),
                PATH,
                Some(NAME),
                "CloseNotification",
                &(id,),
            ),
        )
        .await
        .map_err(|_| IslandError::timeout("Remote notification close"))?
        .map_err(transport)?;
        Ok(true)
    }
    pub async fn run(
        self: Arc<Self>,
        hub: Arc<Hub>,
        address: String,
        mut enabled: watch::Receiver<bool>,
        mut stopped: watch::Receiver<bool>,
    ) -> Result<(), IslandError> {
        let mut backoff = 1;
        loop {
            if *stopped.borrow() {
                break;
            }
            if !*enabled.borrow_and_update() {
                self.update(|r| {
                    r.available = false;
                    r.detail = "disabled".into();
                })?;
                tokio::select! { _=stopped.changed()=>break, result=enabled.changed()=>{if result.is_err(){break;}} }
                continue;
            }
            let result = tokio::select! {
                _=stopped.changed()=>break,
                result=self.session(&hub, &address, &mut enabled)=>result,
            };
            *self
                .control
                .lock()
                .map_err(|_| IslandError::internal("Monitor control poisoned"))? = None;
            hub.reset_server()?;
            if let Err(error) = result {
                tracing::warn!(%error, "Notification mirror disconnected");
                self.update(|r| {
                    r.available = false;
                    r.detail = "bus-unavailable".into();
                })?;
                tokio::select! { _=stopped.changed()=>break, _=enabled.changed()=>{}, ()=tokio::time::sleep(Duration::from_secs(backoff))=>{} }
                backoff = (backoff * 2).min(60);
            } else {
                backoff = 1;
            }
        }
        *self
            .control
            .lock()
            .map_err(|_| IslandError::internal("Monitor control poisoned"))? = None;
        self.update(|r| {
            r.available = false;
            r.detail = "stopped".into();
        })?;
        Ok(())
    }
    async fn session(
        &self,
        hub: &Hub,
        address: &str,
        enabled: &mut watch::Receiver<bool>,
    ) -> Result<(), IslandError> {
        let a = connect(address).await?;
        // Subscribe BEFORE GetNameOwner. Queued owner signals are processed in order and
        // resolved again, so a restart during bootstrap cannot leave B bound to a stale peer.
        let rule = "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.freedesktop.Notifications'";
        let mut names = MessageStream::for_match_rule(rule, &a, NonZeroUsize::new(32))
            .await
            .map_err(transport)?;
        let mut current = owner(&a).await?;
        let mut stream = self.arm(&a, address, current.as_deref()).await?;
        let mut core = Decoder::default();
        loop {
            let deadline = core.pending.front().map(|(_, _, at)| *at + PENDING_TTL);
            let expiry = async {
                match deadline {
                    Some(at) => tokio::time::sleep_until(at.into()).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! { biased;
                changed=enabled.changed()=>{ if changed.is_err() || !*enabled.borrow_and_update(){return Ok(());} },
                ()=expiry=>{
                    let expired=core.expire(Instant::now());
                    self.update(|r|{r.timed_out=r.timed_out.wrapping_add(expired);r.pending=core.pending.len() as u32;})?;
                },
                message=next(&mut names)=>{
                    let message=message.ok_or_else(|| IslandError::unavailable("Owner stream ended"))?.map_err(transport)?;
                    let header=message.header();
                    if header.sender().map(|s|s.as_str()) != Some(DBUS) {continue;}
                    let (name, _, _): (String,String,String)=message.body().deserialize().map_err(transport)?;
                    if name != NAME {continue;}
                    let replacement=owner(&a).await?;
                    if replacement != current {
                        // Rebuild B, reset BOTH correlation and server identity maps.
                        drop(stream.take());
                        core.reset();
                        hub.reset_server()?;
                        current=replacement;
                        stream=self.arm(&a,address,current.as_deref()).await?;
                        self.update(|r|r.rearms=r.rearms.wrapping_add(1))?;
                    }
                },
                message=async {match stream.as_mut(){Some(s)=>next(s).await,None=>std::future::pending().await}}=>{
                    let message=message.ok_or_else(||IslandError::unavailable("Monitor stream ended"))?.map_err(transport)?;
                    match core.observe(&message,current.as_deref().unwrap_or("")) {
                        Ok(Observed::Notification(mut n))=>{
                            if let Some(raw)=n.image_data.take(){
                                n.input.icon_png=Some(tokio::task::spawn_blocking(move||encode(raw)).await.map_err(|_|IslandError::internal("Image worker failed"))??);
                            }
                            hub.insert(n.input)?;
                        },
                        Ok(Observed::Closed(id))=>hub.server_closed(id)?,
                        Ok(Observed::Action)=>self.update(|r|r.observed_actions=r.observed_actions.wrapping_add(1))?,
                        Ok(Observed::Ignored)=>{},
                        Err(_)=>self.update(|r|r.invalid=r.invalid.wrapping_add(1))?,
                    }
                    self.update(|r| {r.pending=core.pending.len() as u32;r.overflow=core.overflow;r.loop_dropped=core.loop_dropped;})?;
                },
            }
        }
    }
    async fn arm(
        &self,
        a: &Connection,
        address: &str,
        owner: Option<&str>,
    ) -> Result<Option<MessageStream>, IslandError> {
        *self
            .control
            .lock()
            .map_err(|_| IslandError::internal("Monitor control poisoned"))? = None;
        let Some(owner) = owner else {
            self.update(|r| {
                r.available = false;
                r.detail = "no-notification-owner".into();
                r.pending = 0;
            })?;
            return Ok(None);
        };
        let b = connect(address).await?;
        // Start reading before BecomeMonitor so its immediate first traffic cannot be lost.
        let stream = MessageStream::from(&b);
        let rules = vec![
            "type='method_call',interface='org.freedesktop.Notifications',member='Notify'"
                .to_string(),
            format!("type='method_return',sender='{owner}'"),
            "type='signal',interface='org.freedesktop.Notifications',member='NotificationClosed'"
                .into(),
            "type='signal',interface='org.freedesktop.Notifications',member='ActionInvoked'".into(),
        ];
        let reply = tokio::time::timeout(
            CALL_TIMEOUT,
            b.call_method(
                Some(DBUS),
                DBUS_PATH,
                Some("org.freedesktop.DBus.Monitoring"),
                "BecomeMonitor",
                &(rules, 0u32),
            ),
        )
        .await;
        match reply {
            Ok(Ok(_)) => {
                *self
                    .control
                    .lock()
                    .map_err(|_| IslandError::internal("Monitor control poisoned"))? =
                    Some((a.clone(), owner.into()));
                self.update(|r| {
                    r.available = true;
                    r.detail = "monitoring".into();
                    r.pending = 0;
                })?;
                // Only the receive stream escapes. Never call AddMatch or send on B again.
                Ok(Some(stream))
            }
            Ok(Err(zbus::Error::FDO(e))) if matches!(*e, zbus::fdo::Error::AccessDenied(_)) => {
                self.update(|r| {
                    r.available = false;
                    r.detail = "monitor-permission-denied".into();
                    r.pending = 0;
                })?;
                Ok(None)
            }
            Ok(Err(e)) => Err(transport(e)),
            Err(_) => Err(IslandError::timeout("BecomeMonitor timed out")),
        }
    }
}

pub enum Observed {
    Notification(Box<super::dbus::DbusNotification>),
    Closed(u32),
    Action,
    Ignored,
}
#[derive(Default)]
pub struct Decoder {
    correlation: NotifyCorrelator,
    pending: VecDeque<(String, u32, Instant)>,
    pub overflow: u32,
    pub loop_dropped: u32,
}
impl Decoder {
    pub fn reset(&mut self) {
        self.correlation.reset();
        self.pending.clear();
    }
    pub fn expire(&mut self, now: Instant) -> u32 {
        let mut count = 0;
        while self
            .pending
            .front()
            .is_some_and(|(_, _, at)| now.saturating_duration_since(*at) >= PENDING_TTL)
        {
            if let Some((sender, serial, _)) = self.pending.pop_front() {
                self.correlation.forget(&sender, serial);
                count += 1;
            }
        }
        count
    }
    pub fn observe(&mut self, message: &Message, owner: &str) -> Result<Observed, IslandError> {
        if message.data().len() > MAX_MESSAGE {
            return Err(IslandError::invalid("Oversized notification message"));
        }
        let h = message.header();
        let sender = h.sender().map(|s| s.as_str()).unwrap_or("");
        let destination = h.destination().map(|s| s.as_str()).unwrap_or("");
        if message.message_type() == Type::MethodCall
            && h.interface().map(|v| v.as_str()) == Some(NAME)
            && h.member().map(|v| v.as_str()) == Some("Notify")
            && h.path().map(|v| v.as_str()) == Some(PATH)
            && (destination == NAME || destination == owner)
        {
            let args = decode_args(message)?;
            // Monitor-layer guard: do not even retain our own payload/correlation.
            if args.app_name == "Hermes Island" {
                self.loop_dropped = self.loop_dropped.wrapping_add(1);
                return Ok(Observed::Ignored);
            }
            let serial = h.primary().serial_num().get();
            self.correlation
                .observe_call(sender, serial, args)
                .map_err(|_| IslandError::invalid("Invalid Notify"))?;
            self.pending.retain(|(s, n, _)| s != sender || *n != serial);
            if self.pending.len() == CORRELATION_CAPACITY {
                self.pending.pop_front();
                self.overflow = self.overflow.wrapping_add(1);
            }
            self.pending
                .push_back((sender.into(), serial, Instant::now()));
        } else if message.message_type() == Type::MethodReturn && sender == owner {
            let Some(serial) = h.reply_serial().map(|n| n.get()) else {
                return Ok(Observed::Ignored);
            };
            let Ok(id) = message.body().deserialize::<u32>() else {
                return Ok(Observed::Ignored);
            };
            if let Some(n) = self
                .correlation
                .method_return(destination, serial, id)
                .map_err(|_| IslandError::invalid("Invalid Notify reply"))?
            {
                self.pending
                    .retain(|(s, n, _)| s != destination || *n != serial);
                return Ok(Observed::Notification(Box::new(n)));
            }
        } else if message.message_type() == Type::Signal
            && sender == owner
            && h.interface().map(|v| v.as_str()) == Some(NAME)
            && h.path().map(|v| v.as_str()) == Some(PATH)
        {
            match h.member().map(|v| v.as_str()) {
                Some("NotificationClosed") => {
                    let (id, _): (u32, u32) = message.body().deserialize().map_err(transport)?;
                    return Ok(Observed::Closed(id));
                }
                Some("ActionInvoked") => {
                    let (_, _): (u32, String) = message.body().deserialize().map_err(transport)?;
                    return Ok(Observed::Action);
                }
                _ => {}
            }
        }
        Ok(Observed::Ignored)
    }
}
fn decode_args(message: &Message) -> Result<NotifyArgs, IslandError> {
    type Args = (
        String,
        u32,
        String,
        String,
        String,
        Vec<String>,
        BTreeMap<String, OwnedVariant>,
        i32,
    );
    let (app_name, replaces_id, app_icon, summary, body, actions, raw, expire_timeout): Args =
        message.body().deserialize().map_err(transport)?;
    let mut hints = BTreeMap::new();
    for (key, value) in raw {
        let parsed = match key.as_str() {
            "urgency" => u8::try_from(value).map(HintValue::Byte),
            "transient" => bool::try_from(value).map(HintValue::Bool),
            "desktop-entry" | "category" | "image-path" | "image_path" | "x-kde-origin-name" => {
                String::try_from(value).map(HintValue::String)
            }
            "image-data" | "icon_data" => {
                type RawImage = (i32, i32, i32, bool, i32, i32, Vec<u8>);
                RawImage::try_from(value).map(|(w, h, stride, alpha, bits, channels, data)| {
                    HintValue::ImageData(ImageData {
                        width: w as u32,
                        height: h as u32,
                        rowstride: stride as u32,
                        has_alpha: alpha,
                        bits_per_sample: bits as u32,
                        channels: channels as u32,
                        data,
                    })
                })
            }
            _ => Ok(HintValue::Other),
        }
        .map_err(transport)?;
        hints.insert(key, parsed);
    }
    Ok(NotifyArgs {
        app_name,
        replaces_id,
        app_icon,
        summary,
        body,
        actions,
        hints,
        expire_timeout,
    })
}
fn encode(raw: ImageData) -> Result<String, IslandError> {
    use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
    let mut pixels = Vec::with_capacity((raw.width * raw.height * raw.channels) as usize);
    for row in raw.data.chunks_exact(raw.rowstride as usize) {
        pixels.extend_from_slice(&row[..(raw.width * raw.channels) as usize]);
    }
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            &pixels,
            raw.width,
            raw.height,
            if raw.has_alpha {
                ExtendedColorType::Rgba8
            } else {
                ExtendedColorType::Rgb8
            },
        )
        .map_err(|_| IslandError::invalid("Notification image encoding failed"))?;
    // Tiny standard base64 encoder, avoiding an otherwise unneeded dependency.
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::from("data:image/png;base64,");
    for bytes in png.chunks(3) {
        let a = bytes[0];
        let b = bytes.get(1).copied().unwrap_or(0);
        let c = bytes.get(2).copied().unwrap_or(0);
        for index in [
            a >> 2,
            ((a & 3) << 4) | (b >> 4),
            ((b & 15) << 2) | (c >> 6),
            c & 63,
        ] {
            text.push(TABLE[index as usize] as char);
        }
        if bytes.len() < 3 {
            text.pop();
            text.push('=');
        }
        if bytes.len() < 2 {
            let end = text.len() - 2;
            text.replace_range(end..end + 1, "=");
        }
    }
    if text.len() > super::MAX_ICON_BYTES {
        return Err(IslandError::invalid("Encoded notification icon too large"));
    }
    Ok(text)
}
