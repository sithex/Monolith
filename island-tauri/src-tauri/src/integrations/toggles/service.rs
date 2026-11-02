//! Power profile + mic + existing actor DND. One serialized writer, request IDs and deadlines.
use super::core::{self, Profile, ToggleState};
use crate::{
    contract::controls::{TogglesChanged, TogglesOp},
    error::IslandError,
    integrations::{
        control_actor::{respond, ControlActor, Request},
        control_dbus::{Properties, PropertyFeed, SetProfile, PPD},
        control_process::{self as process, argv, Lines, OwnedChild, Runner},
    },
    state::{actor::Handle, machine::Intent},
};
use std::{
    future::{pending, Future},
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
pub type TogglesService = ControlActor<TogglesChanged, TogglesOp>;
fn empty(detail: &str) -> TogglesChanged {
    TogglesChanged {
        rev: 0,
        available: false,
        detail: detail.into(),
        power_available: false,
        power_detail: "starting".into(),
        power_profile: None,
        power_choices: vec![],
        degraded: None,
        held_by: vec![],
        mic_available: false,
        mic_detail: "starting".into(),
        mic_muted: None,
        dnd: false,
        power_request: None,
        mic_request: None,
    }
}
impl TogglesService {
    pub fn start(runner: Runner, address: Option<String>, actor: Handle) -> Arc<Self> {
        let (service, rx) = Self::new(empty("starting"));
        let owned = service.clone();
        service.own(tauri::async_runtime::spawn(async move {
            worker(owned, runner, address, actor, rx).await
        }));
        service
    }
}
struct FeedGuard(Arc<PropertyFeed>);
impl Drop for FeedGuard {
    fn drop(&mut self) {
        // Drop can run while Tokio destroys a cancelled task. Never synchronously join
        // another Tokio task there; the normal exit path joins it asynchronously below.
        self.0.stop.send_replace(true);
    }
}
struct MicSession {
    child: OwnedChild,
    lines: Lines,
    errors: Pin<Box<dyn Future<Output = Result<(), IslandError>> + Send>>,
}
impl MicSession {
    async fn start(runner: &Runner) -> Result<(Self, bool), IslandError> {
        let mut child = runner.spawn(&argv(&["pactl", "subscribe"]))?;
        let lines = Lines::new(child.stdout()?);
        let errors = Box::pin(process::drain(child.stderr()?));
        let observed = mic_read(runner).await?;
        Ok((
            Self {
                child,
                lines,
                errors,
            },
            observed,
        ))
    }
}
async fn mic_read(runner: &Runner) -> Result<bool, IslandError> {
    core::parse_mic_mute(&runner.text(&core::mic_get_argv()).await?)
        .map_err(|_| IslandError::unavailable("Microphone observation invalid"))
}
pub fn profile_name(profile: Profile) -> &'static str {
    match profile {
        Profile::PowerSaver => "power-saver",
        Profile::Balanced => "balanced",
        Profile::Performance => "performance",
    }
}
fn snapshot(props: &Properties) -> Result<core::PpdSnapshot, IslandError> {
    let bad = || IslandError::unavailable("Power profile observations incomplete");
    let p = props.values.as_ref().ok_or_else(bad)?;
    let text = |key: &str| {
        p.get(key)
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(bad)
    };
    let profiles = p
        .get("Profiles")
        .and_then(|v| v.as_array())
        .ok_or_else(bad)?
        .iter()
        .map(|v| {
            Ok(core::PpdProfile {
                profile: v
                    .get("Profile")
                    .and_then(|v| v.as_str())
                    .ok_or_else(bad)?
                    .into(),
                driver: v.get("Driver").and_then(|v| v.as_str()).map(str::to_owned),
            })
        })
        .collect::<Result<Vec<_>, IslandError>>()?;
    let holds = p
        .get("ActiveProfileHolds")
        .and_then(|v| v.as_array())
        .ok_or_else(bad)?
        .iter()
        .map(|v| {
            Ok(core::PpdHold {
                profile: v
                    .get("Profile")
                    .and_then(|v| v.as_str())
                    .ok_or_else(bad)?
                    .into(),
                reason: v
                    .get("Reason")
                    .and_then(|v| v.as_str())
                    .ok_or_else(bad)?
                    .into(),
                application_id: v
                    .get("ApplicationId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(bad)?
                    .into(),
            })
        })
        .collect::<Result<Vec<_>, IslandError>>()?;
    Ok(core::PpdSnapshot {
        active: text("ActiveProfile")?,
        profiles,
        degraded: text("PerformanceDegraded")?,
        holds,
    })
}
struct Model {
    power: ToggleState<Profile>,
    mic: ToggleState<bool>,
    tile: core::PowerTile,
    power_detail: String,
    mic_detail: String,
}
impl Model {
    fn update_power(&mut self, properties: &Properties, now: u64) {
        match snapshot(properties) {
            Ok(snapshot) => {
                self.tile = core::power_tile(Some(&snapshot));
                self.power_detail = "power-profiles-daemon".into();
                if let Some(active) = self.tile.active {
                    self.power.confirm(active, now);
                } else {
                    self.power = ToggleState::new();
                }
            }
            Err(_) => {
                self.tile = core::power_tile(None);
                self.power = ToggleState::new();
                self.power_detail = properties.detail.clone();
            }
        }
    }
    fn wire(&self, dnd: bool) -> TogglesChanged {
        TogglesChanged {
            rev: 0,
            available: self.tile.available || self.mic.confirmed().is_some(),
            detail: "controls".into(),
            power_available: self.tile.available,
            power_detail: self.power_detail.clone(),
            power_profile: self.power.display().map(|p| profile_name(p).into()),
            power_choices: self
                .tile
                .choices
                .iter()
                .map(|p| profile_name(*p).into())
                .collect(),
            degraded: self.tile.degraded.clone(),
            held_by: self.tile.held_by.clone(),
            mic_available: self.mic.confirmed().is_some(),
            mic_detail: self.mic_detail.clone(),
            mic_muted: self.mic.display(),
            dnd: core::dnd_tile(dnd).on,
            power_request: self.power.pending().map(|p| p.request_id.to_string()),
            mic_request: self.mic.pending().map(|p| p.request_id.to_string()),
        }
    }
    fn deadline(&self) -> Option<u64> {
        self.power
            .pending()
            .map(|p| p.sent_at_ms.saturating_add(core::PENDING_TIMEOUT_MS))
            .into_iter()
            .chain(
                self.mic
                    .pending()
                    .map(|p| p.sent_at_ms.saturating_add(core::PENDING_TIMEOUT_MS)),
            )
            .min()
    }
}
async fn deadline(at: Option<u64>, now: u64) {
    match at {
        Some(at) => tokio::time::sleep(Duration::from_millis(at.saturating_sub(now))).await,
        None => pending::<()>().await,
    }
}
async fn worker(
    service: Arc<TogglesService>,
    runner: Runner,
    address: Option<String>,
    actor: Handle,
    mut rx: mpsc::Receiver<Request<TogglesChanged, TogglesOp>>,
) {
    let feed = FeedGuard(PropertyFeed::start(address, PPD));
    let mut power = feed.0.topic.subscribe();
    let mut stop = service.stop.subscribe();
    let mut dnd = actor.watch();
    let origin = Instant::now();
    let now = || origin.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let mut model = Model {
        power: ToggleState::new(),
        mic: ToggleState::new(),
        tile: core::power_tile(None),
        power_detail: "starting".into(),
        mic_detail: "starting".into(),
    };
    let mut mic: Option<MicSession> = None;
    let mut retry = 0;
    let mut backoff = 1000;
    loop {
        if *stop.borrow() {
            break;
        }
        let wake = model
            .deadline()
            .into_iter()
            .chain(mic.is_none().then_some(retry))
            .min();
        service.publish(model.wire(dnd.borrow().dnd));
        tokio::select! {
            _=stop.changed()=>break,
            result=power.changed()=>{if result.is_err(){break;}
                if let Some(props)=power.borrow_and_update().as_ref(){model.update_power(props,now());}},
            result=dnd.changed()=>{if result.is_err(){break;}},
            ()=deadline(wake,now())=>{
                model.power.tick(now());model.mic.tick(now());
                if mic.is_none() && now()>=retry {
                    match MicSession::start(&runner).await {
                        Ok((session,observed))=>{mic=Some(session);model.mic.confirm(observed,now());model.mic_detail="default-source".into();backoff=1000;}
                        Err(_)=>{model.mic=ToggleState::new();model.mic_detail="pactl-unavailable".into();retry=now().saturating_add(backoff);backoff=(backoff*2).min(60_000);}
                    }
                }
            }
            result=async {match mic.as_mut(){Some(s)=>tokio::select!{line=s.lines.next()=>line,result=&mut s.errors=>{result?;Err(IslandError::unavailable("Microphone subscription ended"))}},None=>pending::<Result<String,IslandError>>().await}}=>{
                let result=match result{Ok(line)=>{
                    let relevant=super::super::mixer::core::parse_event(&line).is_some_and(|e|matches!(e.facility,super::super::mixer::core::PaFacility::Source|super::super::mixer::core::PaFacility::Server));
                    if relevant {mic_read(&runner).await.map(Some)}else{Ok(None)}
                },Err(error)=>Err(error)};
                match result{Ok(Some(value))=>model.mic.confirm(value,now()),Ok(None)=>{},Err(_)=>{
                    if let Some(session)=mic.take(){if let Err(error)=session.child.finish().await{tracing::debug!(%error,"Mic cleanup failed");}}
                    model.mic=ToggleState::new();model.mic_detail="pactl-unavailable".into();retry=now().saturating_add(backoff);
                }}
            }
            request=rx.recv()=>{
                let Some(request)=request else{break;};if request.reply.is_closed(){continue;}
                let mic_connected=mic.is_some();
                let result=async {match request.op {
                    TogglesOp::Dnd{on}=>{actor.dispatch_ui(Intent::SetDnd(on)).await?;}
                    TogglesOp::MicMute{muted}=>{
                        if !mic_connected{return Err(IslandError::unavailable("Microphone unavailable"));}
                        let id=model.mic.request(muted,now());service.publish(model.wire(dnd.borrow().dnd));
                        let op=if muted{core::MicOp::Mute}else{core::MicOp::Unmute};
                        if let Err(error)=runner.text(&core::mic_set_argv(op)).await {model.mic.fail(id,now());return Err(error);}
                        match mic_read(&runner).await {Ok(value)=>model.mic.confirm(value,now()),Err(error)=>{model.mic=ToggleState::new();return Err(error);}}
                    }
                    op=>{
                        let profile=match op {TogglesOp::PowerProfile{profile}=>profile.parse::<Profile>().map_err(|_|IslandError::invalid("Invalid power profile"))?,TogglesOp::CycleProfile=>core::next_profile(&model.tile).ok_or_else(||IslandError::unavailable("Power profile cannot cycle"))?,_=>return Err(IslandError::internal("Toggle operation mismatch"))};
                        if !model.tile.available || !model.tile.choices.contains(&profile){return Err(IslandError::unavailable("Power profile unavailable"));}
                        let id=model.power.request(profile,now());service.publish(model.wire(dnd.borrow().dnd));
                        match feed.0.request(SetProfile(profile_name(profile).into())).await {
                            Ok(properties)=>model.update_power(&properties,now()),Err(error)=>{model.power.fail(id,now());return Err(error);}
                        }
                    }
                } Ok(service.publish(model.wire(actor.state().dnd)))}.await;
                respond(request.reply,result);
            }
        }
    }
    if let Some(session) = mic {
        if let Err(error) = session.child.finish().await {
            tracing::warn!(%error,"Mic shutdown failed");
        }
    }
    if let Err(error) = feed.0.shutdown_async().await {
        tracing::warn!(%error,"Power profile observer shutdown failed");
    }
    service.publish(empty("stopped"));
}
