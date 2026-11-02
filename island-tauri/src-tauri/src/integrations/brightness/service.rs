//! Owned PowerDevil adapter. The actor is the only model writer; no periodic brightness poll.
//! Root signals are authenticated against the current unique owner, never a well-known sender.
use super::core::{self, BrightnessModel, Event};
use crate::{
    bus::Latest,
    contract::controls::{BrightnessChanged, BrightnessDisplay, BrightnessOp, BrightnessStep},
    error::IslandError,
    integrations::dbus_util::{self as dbus, next, CALL_TIMEOUT, DBUS, MAX_SIGNAL_BYTES},
};
use std::{
    collections::HashMap,
    future::pending,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, watch};
use zbus::{Connection, Message, MessageStream, OwnedVariant};

pub const NAME: &str = "org.kde.ScreenBrightness";
pub const ROOT: &str = "/org/kde/ScreenBrightness";
const DISPLAY: &str = "org.kde.ScreenBrightness.Display";
const PROPS: &str = "org.freedesktop.DBus.Properties";
struct Request {
    op: BrightnessOp,
    reply: oneshot::Sender<Result<BrightnessChanged, IslandError>>,
}
pub struct BrightnessService {
    pub topic: Latest<BrightnessChanged>,
    current: Mutex<BrightnessChanged>,
    commands: mpsc::Sender<Request>,
    stop: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    origin: Instant,
}
impl BrightnessService {
    pub fn start(address: Option<String>) -> Arc<Self> {
        let initial = BrightnessChanged {
            rev: 0,
            available: false,
            detail: "starting".into(),
            displays: vec![],
        };
        let topic = Latest::new();
        topic.publish(initial.clone());
        let (commands, receiver) = mpsc::channel(16);
        let service = Arc::new(Self {
            topic,
            current: Mutex::new(initial),
            commands,
            stop: watch::channel(false).0,
            task: Mutex::new(None),
            origin: Instant::now(),
        });
        let owned = service.clone();
        let task = tauri::async_runtime::spawn(async move { owned.run(address, receiver).await });
        match service.task.lock() {
            Ok(mut slot) => *slot = Some(task),
            Err(_) => task.abort(),
        }
        service
    }
    pub fn current(&self) -> BrightnessChanged {
        match self.current.lock() {
            Ok(v) => v.clone(),
            Err(v) => v.into_inner().clone(),
        }
    }
    fn publish(&self, model: Option<&BrightnessModel>, detail: &str) -> BrightnessChanged {
        let mut value = match self.current.lock() {
            Ok(v) => v,
            Err(v) => v.into_inner(),
        };
        let displays = model
            .map(|m| {
                m.displays()
                    .into_iter()
                    .map(|d| BrightnessDisplay {
                        name: d.name,
                        label: d.label,
                        is_internal: d.is_internal,
                        percent: d.percent,
                        dragging: d.dragging,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let next = BrightnessChanged {
            rev: value.rev,
            available: model.is_some(),
            detail: detail.into(),
            displays,
        };
        if *value != next {
            *value = next;
            value.rev = value.rev.wrapping_add(1);
            self.topic.publish(value.clone());
        }
        value.clone()
    }
    fn now(&self) -> u64 {
        self.origin.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    pub async fn request(&self, op: BrightnessOp) -> Result<BrightnessChanged, IslandError> {
        if *self.stop.borrow() {
            return Err(IslandError::unavailable("Brightness stopped"));
        }
        let (reply, answer) = oneshot::channel();
        self.commands
            .try_send(Request { op, reply })
            .map_err(|_| IslandError::unavailable("Brightness busy or stopped"))?;
        tokio::time::timeout(Duration::from_secs(4), answer)
            .await
            .map_err(|_| IslandError::timeout("Brightness command"))?
            .map_err(|_| IslandError::unavailable("Brightness stopped"))?
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        self.stop.send_replace(true);
        let task = self
            .task
            .lock()
            .map_err(|_| IslandError::internal("Brightness worker poisoned"))?
            .take();
        if let Some(mut task) = task {
            // Synchronous configure/shutdown may also be called by an async test. Never
            // nest block_on inside its runtime; the small owned joiner has no desktop IO.
            std::thread::spawn(move || {
                tauri::async_runtime::block_on(async {
                    match tokio::time::timeout(Duration::from_secs(3), &mut task).await {
                        Ok(result) => {
                            result.map_err(|_| IslandError::internal("Brightness worker panicked"))
                        }
                        Err(_) => {
                            task.abort();
                            if task.await.is_err() {
                                tracing::debug!(
                                    "Brightness worker cancelled after shutdown timeout"
                                );
                            }
                            Err(IslandError::timeout("Brightness shutdown"))
                        }
                    }
                })
            })
            .join()
            .map_err(|_| IslandError::internal("Brightness joiner panicked"))??;
        }
        Ok(())
    }
    async fn run(self: Arc<Self>, address: Option<String>, mut rx: mpsc::Receiver<Request>) {
        let mut stop = self.stop.subscribe();
        let Some(address) = address else {
            self.publish(None, "no-session-bus-configured");
            return;
        };
        let mut delay = 1;
        loop {
            if *stop.borrow() {
                break;
            }
            let result = tokio::select! {
                _ = stop.changed() => break,
                result = self.session(&address, &mut rx) => result,
            };
            self.publish(None, "bus-unavailable");
            if let Err(error) = result {
                tracing::warn!(%error, "Brightness session ended");
            }
            tokio::select! { _ = stop.changed() => break, () = tokio::time::sleep(Duration::from_secs(delay)) => {} }
            delay = (delay * 2).min(60);
        }
        self.publish(None, "stopped");
    }
    async fn session(
        &self,
        address: &str,
        rx: &mut mpsc::Receiver<Request>,
    ) -> Result<(), IslandError> {
        let conn = dbus::connect(address).await?;
        // Subscribe before reads. No sender rule on the custom stream: the in-code owner
        // check below is deliberate and exercised separately for EVERY consumed signal type.
        let mut signals =
            subscribe(&conn, "type='signal',interface='org.kde.ScreenBrightness'").await?;
        let mut owners = subscribe(&conn, "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.kde.ScreenBrightness'").await?;
        let mut owner = dbus::name_owner(&conn, NAME).await?;
        let mut model = BrightnessModel::new();
        if let Some(name) = &owner {
            load(&conn, name, &mut model).await?;
            self.publish(Some(&model), "powerdevil");
        } else {
            self.publish(None, "no-owner");
        }
        loop {
            let wake = model.next_wakeup(self.now());
            tokio::select! {
                message = next(&mut owners) => {
                    let message = message.ok_or_else(|| IslandError::unavailable("Brightness owner stream ended"))?.map_err(dbus::transport)?;
                    if message.data().len() > MAX_SIGNAL_BYTES || message.header().sender().map(|s|s.as_str()) != Some(DBUS) { continue; }
                    let Ok((name, _, new)) = message.body().deserialize::<(String,String,String)>() else { continue; };
                    if name != NAME { continue; }
                    owner = (!new.is_empty()).then_some(new);
                    model = BrightnessModel::new();
                    if let Some(name) = &owner { load(&conn, name, &mut model).await?; self.publish(Some(&model), "powerdevil"); }
                    else { self.publish(None, "name-lost"); }
                }
                message = next(&mut signals) => {
                    let message = message.ok_or_else(|| IslandError::unavailable("Brightness signal stream ended"))?.map_err(dbus::transport)?;
                    if let Some(event) = decode(owner.as_deref(), &message) {
                        let applied = model.apply(event);
                        for name in applied.fetch { load_display(&conn, owner.as_deref().ok_or_else(||IslandError::unavailable("Brightness owner lost"))?, &mut model, &name).await?; }
                        self.publish(Some(&model), "powerdevil");
                    }
                }
                () = deadline(wake, self.now()) => {
                    let owner = owner.as_deref().ok_or_else(||IslandError::unavailable("Brightness owner lost"))?;
                    for request in model.poll(self.now()) { send(&conn, owner, &request).await?; }
                    self.publish(Some(&model), "powerdevil");
                }
                request = rx.recv() => {
                    let Some(request) = request else { return Ok(()); };
                    if request.reply.is_closed() { continue; }
                    let result = match owner.as_deref() {
                        None => Err(IslandError::unavailable("Brightness daemon unavailable")),
                        Some(owner) => command(&conn, owner, &mut model, request.op, self.now()).await,
                    };
                    let failed = result.as_ref().err().is_some_and(|e|e.code != crate::error::ErrorCode::Invalid);
                    let value = self.publish(if failed { None } else { Some(&model) }, if failed { "command-failed" } else { "powerdevil" });
                    if request.reply.send(result.map(|()|value)).is_err() { tracing::debug!("Brightness caller disconnected"); }
                    if failed && owner.is_some() { return Err(IslandError::unavailable("Brightness command failed")); }
                }
            }
        }
    }
}
async fn deadline(at: Option<u64>, now: u64) {
    match at {
        Some(at) => tokio::time::sleep(Duration::from_millis(at.saturating_sub(now))).await,
        None => pending::<()>().await,
    }
}
async fn subscribe(conn: &Connection, rule: &str) -> Result<MessageStream, IslandError> {
    tokio::time::timeout(CALL_TIMEOUT, dbus::subscribe(conn, rule))
        .await
        .map_err(|_| IslandError::timeout("Brightness subscription"))?
}
async fn call<B, R>(
    conn: &Connection,
    owner: &str,
    path: &str,
    iface: &str,
    member: &str,
    body: &B,
) -> Result<R, IslandError>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let reply = tokio::time::timeout(
        CALL_TIMEOUT,
        conn.call_method(Some(owner), path, Some(iface), member, body),
    )
    .await
    .map_err(|_| IslandError::timeout("Brightness method"))?
    .map_err(dbus::transport)?;
    if reply.data().len() > MAX_SIGNAL_BYTES {
        return Err(IslandError::unavailable("Brightness reply too large"));
    }
    reply.body().deserialize().map_err(dbus::transport)
}
async fn load(
    conn: &Connection,
    owner: &str,
    model: &mut BrightnessModel,
) -> Result<(), IslandError> {
    let value: OwnedVariant = call(
        conn,
        owner,
        ROOT,
        PROPS,
        "Get",
        &(NAME, "DisplaysDBusNames"),
    )
    .await?;
    let names = dbus::value_to_json(&value);
    let names = names
        .as_array()
        .ok_or_else(|| IslandError::unavailable("Brightness displays unavailable"))?;
    let names: Vec<String> = names
        .iter()
        .map(|n| {
            n.as_str()
                .map(str::to_owned)
                .ok_or_else(|| IslandError::unavailable("Brightness display name invalid"))
        })
        .collect::<Result<_, _>>()?;
    let applied = model.apply(Event::DisplayNames(names));
    for name in applied.fetch {
        load_display(conn, owner, model, &name).await?;
    }
    Ok(())
}
async fn load_display(
    conn: &Connection,
    owner: &str,
    model: &mut BrightnessModel,
    name: &str,
) -> Result<(), IslandError> {
    let path =
        core::display_path(name).map_err(|_| IslandError::invalid("Invalid brightness display"))?;
    let props: HashMap<String, OwnedVariant> =
        call(conn, owner, &path, PROPS, "GetAll", &(DISPLAY,)).await?;
    let props = dbus::props_to_json(&props);
    let error = || IslandError::unavailable("Brightness display properties unavailable");
    let label = props
        .get("Label")
        .and_then(|v| v.as_str())
        .ok_or_else(error)?
        .to_owned();
    let is_internal = props
        .get("IsInternal")
        .and_then(|v| v.as_bool())
        .ok_or_else(error)?;
    let number = |key| {
        props
            .get(key)
            .and_then(|v| v.as_i64())
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(error)
    };
    model.apply(Event::Props {
        name: name.into(),
        label,
        is_internal,
        max_brightness: number("MaxBrightness")?,
        brightness: number("Brightness")?,
    });
    Ok(())
}
fn decode(owner: Option<&str>, message: &Message) -> Option<Event> {
    let owner = owner?;
    let header = message.header();
    if message.data().len() > MAX_SIGNAL_BYTES
        || header.sender().map(|s| s.as_str()) != Some(owner)
        || header.path().map(|p| p.as_str()) != Some(ROOT)
        || header.interface().map(|i| i.as_str()) != Some(NAME)
    {
        return None;
    }
    match header.member()?.as_str() {
        "DisplayAdded" => message
            .body()
            .deserialize::<String>()
            .ok()
            .map(Event::DisplayAdded),
        "DisplayRemoved" => message
            .body()
            .deserialize::<String>()
            .ok()
            .map(Event::DisplayRemoved),
        "BrightnessChanged" => {
            let (name, brightness, source_client, context) = message.body().deserialize().ok()?;
            Some(Event::BrightnessChanged {
                name,
                brightness,
                source_client,
                context,
            })
        }
        "BrightnessRangeChanged" => {
            let (name, max_brightness, brightness) = message.body().deserialize().ok()?;
            Some(Event::RangeChanged {
                name,
                max_brightness,
                brightness,
            })
        }
        _ => None,
    }
}
async fn send(
    conn: &Connection,
    owner: &str,
    request: &core::SetRequest,
) -> Result<(), IslandError> {
    call::<_, ()>(
        conn,
        owner,
        &request.path,
        DISPLAY,
        "SetBrightnessWithContext",
        &(request.brightness, request.flags, request.context.as_str()),
    )
    .await
}
async fn command(
    conn: &Connection,
    owner: &str,
    model: &mut BrightnessModel,
    op: BrightnessOp,
    now: u64,
) -> Result<(), IslandError> {
    let requests = match op {
        BrightnessOp::Drag { name, percent } => {
            model
                .drag(&name, percent, now)
                .map_err(|_| IslandError::invalid("Brightness drag invalid or unavailable"))?;
            model.poll(now)
        }
        BrightnessOp::Release { name } => {
            core::validate_display_name(&name)
                .map_err(|_| IslandError::invalid("Invalid brightness display"))?;
            if model.max_brightness(&name).is_none() {
                return Err(IslandError::unavailable("Brightness display unavailable"));
            }
            model.release(&name, now).into_iter().collect()
        }
        BrightnessOp::Step { action } => {
            let step = core::step_request(match action {
                BrightnessStep::Increase => core::Step::Increase,
                BrightnessStep::Decrease => core::Step::Decrease,
                BrightnessStep::IncreaseSmall => core::Step::IncreaseSmall,
                BrightnessStep::DecreaseSmall => core::Step::DecreaseSmall,
            });
            return call::<_, ()>(
                conn,
                owner,
                ROOT,
                NAME,
                "AdjustBrightnessStepWithContext",
                &(step.step_action, step.flags, step.context.as_str()),
            )
            .await;
        }
    };
    for request in requests {
        if let Err(error) = send(conn, owner, &request).await {
            model.write_failed(&request.name);
            return Err(error);
        }
    }
    Ok(())
}
