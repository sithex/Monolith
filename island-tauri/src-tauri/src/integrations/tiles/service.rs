//! System tiles: independently unavailable sources, no network writes or shell commands.
use super::core;
use crate::{
    contract::{
        controls::{MixerChanged, MixerOp},
        tiles as wire,
    },
    error::IslandError,
    integrations::{
        control_actor::{respond, ControlActor, Request, State},
        control_dbus::{
            self as bus, Properties, PropertyFeed, KWIN, NM, PICKER_IFACE, PICKER_PATH,
            SCREENSAVER, SCREENSAVER_PATH,
        },
        control_process::{argv, Runner},
        dbus_util,
        mixer::MixerService,
    },
};
use std::{
    future::pending,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinSet};
use zbus::Connection;
impl State for wire::TilesChanged {
    fn rev(&mut self) -> &mut u32 {
        &mut self.rev
    }
}
pub type TilesService = ControlActor<wire::TilesChanged, wire::TilesOp>;
pub trait ColourClipboard: Send + Sync {
    fn copy_colour(&self, text: &str) -> Result<(), IslandError>;
}
impl ColourClipboard for crate::integrations::clipboard::Clipboard {
    fn copy_colour(&self, text: &str) -> Result<(), IslandError> {
        self.copy_colour_text(text)
    }
}
fn empty(detail: &str) -> wire::TilesChanged {
    wire::TilesChanged {
        rev: 0,
        available: false,
        detail: detail.into(),
        keep_awake: wire::KeepAwakeTile {
            available: false,
            detail: "starting".into(),
            on: None,
            remaining_ms: None,
        },
        output: wire::OutputTile {
            available: false,
            detail: "starting".into(),
            current: None,
            choices: vec![],
        },
        network: wire::NetworkTile {
            available: false,
            detail: "starting".into(),
            state: None,
            connectivity: None,
            kind: None,
            show_wifi: false,
            wifi_enabled: None,
        },
        tailscale: wire::TailscaleTile {
            available: false,
            detail: "starting".into(),
            backend: None,
            self_name: None,
            peers: vec![],
            online_count: None,
        },
        voice: wire::VoiceTile {
            available: false,
            detail: "starting".into(),
            active: None,
            choices: vec![],
        },
        picker_available: false,
        picker_busy: false,
        screenshot_busy: false,
        last_colour: None,
        action_error: None,
    }
}
impl TilesService {
    pub fn start(
        runner: Runner,
        session: Option<String>,
        system: Option<String>,
        clipboard: Arc<dyn ColourClipboard>,
    ) -> Arc<Self> {
        let (service, rx) = Self::new(empty("starting"));
        let owned = service.clone();
        service.own(tauri::async_runtime::spawn(async move {
            worker(owned, runner, session, system, clipboard, rx).await
        }));
        service
    }
}
struct Sources {
    nm: Arc<PropertyFeed>,
    ss: Arc<PropertyFeed>,
    kwin: Arc<PropertyFeed>,
    mixer: Arc<MixerService>,
}
impl Drop for Sources {
    fn drop(&mut self) {
        self.nm.stop.send_replace(true);
        self.ss.stop.send_replace(true);
        self.kwin.stop.send_replace(true);
        self.mixer.stop.send_replace(true);
    }
}
impl Sources {
    async fn shutdown(&self) {
        for service in [&self.nm, &self.ss, &self.kwin] {
            if let Err(error) = service.shutdown_async().await {
                tracing::warn!(%error,"Tiles bus observer shutdown failed");
            }
        }
        if let Err(error) = self.mixer.shutdown_async().await {
            tracing::warn!(%error,"Tiles mixer observer shutdown failed");
        }
    }
}
fn word(value: &impl serde::Serialize) -> Option<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
}
fn output(value: &MixerChanged) -> wire::OutputTile {
    let tile = core::output_tile(
        value
            .sinks
            .iter()
            .map(|s| core::Sink {
                index: s.index,
                name: s.name.clone(),
                description: s.description.clone(),
                state: s.state.clone(),
                muted: s.muted,
            })
            .collect(),
        value.default_sink.clone(),
    );
    wire::OutputTile {
        available: value.available,
        detail: value.detail.clone(),
        current: tile.current.map(|s| s.name),
        choices: tile
            .choices
            .into_iter()
            .map(|s| wire::OutputChoice {
                name: s.name,
                description: s.description,
                muted: s.muted,
            })
            .collect(),
    }
}
fn network(properties: &Properties) -> wire::NetworkTile {
    let snapshot = properties.values.as_ref().and_then(|p| {
        Some(core::NmSnapshot {
            state: p
                .get("State")
                .and_then(|v| v.as_u64())
                .and_then(|n| u32::try_from(n).ok()),
            connectivity: p
                .get("Connectivity")
                .and_then(|v| v.as_u64())
                .and_then(|n| u32::try_from(n).ok()),
            primary_connection_type: p
                .get("PrimaryConnectionType")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            wireless_enabled: p.get("WirelessEnabled").and_then(|v| v.as_bool()),
            wireless_hardware_enabled: p.get("WirelessHardwareEnabled").and_then(|v| v.as_bool()),
            has_wireless_device: p.get("HasWirelessDevice").and_then(|v| v.as_bool())?,
        })
    });
    let tile = core::network_tile(snapshot);
    wire::NetworkTile {
        available: tile.available,
        detail: properties.detail.clone(),
        state: tile.state.as_ref().and_then(word),
        connectivity: tile.connectivity.as_ref().and_then(word),
        kind: tile.available.then(|| word(&tile.kind)).flatten(),
        show_wifi: tile.show_wifi_toggle,
        wifi_enabled: tile.wifi_enabled,
    }
}
async fn tailscale(runner: &Runner) -> wire::TailscaleTile {
    let result = runner
        .run(
            &argv(&["tailscale", "status", "--json"]),
            Duration::from_secs(2),
            core::MAX_TAILSCALE_BYTES,
        )
        .await;
    let parsed = result
        .ok()
        .filter(|o| o.code == 0)
        .and_then(|o| core::parse_tailscale_status(&o.stdout).ok());
    match parsed {
        Some(tile) => wire::TailscaleTile {
            available: true,
            detail: "read-only-status".into(),
            backend: match &tile.backend {
                core::BackendState::Unknown(s) => Some(s.clone()),
                b => word(b),
            },
            self_name: tile.self_name,
            online_count: Some(tile.online_count),
            peers: tile
                .peers
                .into_iter()
                .map(|p| wire::TailPeer {
                    host_name: p.host_name,
                    online: p.online,
                    os: p.os,
                    last_seen: p.last_seen,
                    first_ip: p.first_ip,
                })
                .collect(),
        },
        None => wire::TailscaleTile {
            available: false,
            detail: "tailscale-unavailable".into(),
            backend: None,
            self_name: None,
            online_count: None,
            peers: vec![],
        },
    }
}
async fn voices(runner: &Runner) -> Result<core::VoiceList, IslandError> {
    let list = runner
        .run(
            &argv(&["hermes-fish"]),
            Duration::from_secs(2),
            core::MAX_VOICE_BYTES,
        )
        .await?;
    if list.code != 0 {
        return Err(IslandError::unavailable("Voice list unavailable"));
    }
    let mut list = core::parse_voice_list(&list.stdout)
        .map_err(|_| IslandError::unavailable("Voice list invalid"))?;
    let active = core::parse_current(&runner.text(&argv(&["hermes-fish", "current"])).await?)
        .map_err(|_| IslandError::unavailable("Current voice invalid"))?;
    if !list.voices.contains(&active) {
        return Err(IslandError::unavailable("Current voice not in list"));
    }
    list.active = Some(active);
    Ok(list)
}
fn voice(value: Result<core::VoiceList, IslandError>) -> wire::VoiceTile {
    match value {
        Ok(list) => wire::VoiceTile {
            available: true,
            detail: "saved-voices".into(),
            active: list.active,
            choices: list.voices,
        },
        Err(_) => wire::VoiceTile {
            available: false,
            detail: "voice-runner-unavailable".into(),
            active: None,
            choices: vec![],
        },
    }
}
struct Cookie {
    conn: Connection,
    owner: String,
    cookie: u32,
}
impl Cookie {
    async fn release(&self) -> Result<(), IslandError> {
        bus::call::<_, ()>(
            &self.conn,
            &self.owner,
            SCREENSAVER_PATH,
            SCREENSAVER,
            "UnInhibit",
            &(self.cookie,),
        )
        .await
    }
}
enum Job {
    Colour(Result<String, IslandError>),
    Screenshot(Result<(), IslandError>),
}
async fn pick(
    address: String,
    owner: String,
    clipboard: Arc<dyn ColourClipboard>,
) -> Result<String, IslandError> {
    let conn = dbus_util::connect(&address).await?;
    let reply = tokio::time::timeout(
        Duration::from_secs(60),
        conn.call_method(
            Some(owner.as_str()),
            PICKER_PATH,
            Some(PICKER_IFACE),
            "pick",
            &(),
        ),
    )
    .await
    .map_err(|_| IslandError::timeout("Colour picker timed out"))?
    .map_err(dbus_util::transport)?;
    if reply.data().len() > 1024 {
        return Err(IslandError::unavailable("Colour picker reply limit"));
    }
    let argb: u32 = reply.body().deserialize().map_err(dbus_util::transport)?;
    let colour = core::argb_to_hex(argb);
    let copy = colour.clone();
    tokio::task::spawn_blocking(move || clipboard.copy_colour(&copy))
        .await
        .map_err(|_| IslandError::internal("Colour copy worker failed"))??;
    Ok(colour)
}
async fn wait(at: Option<u64>, now: u64) {
    match at {
        Some(at) => tokio::time::sleep(Duration::from_millis(at.saturating_sub(now))).await,
        None => pending::<()>().await,
    }
}
async fn worker(
    service: Arc<TilesService>,
    runner: Runner,
    session: Option<String>,
    system: Option<String>,
    clipboard: Arc<dyn ColourClipboard>,
    mut rx: mpsc::Receiver<Request<wire::TilesChanged, wire::TilesOp>>,
) {
    let sources = Sources {
        nm: PropertyFeed::start(system, NM),
        ss: PropertyFeed::start(session.clone(), SCREENSAVER),
        kwin: PropertyFeed::start(session.clone(), KWIN),
        mixer: MixerService::start(runner.clone()),
    };
    let mut nm = sources.nm.topic.subscribe();
    let mut ss = sources.ss.topic.subscribe();
    let mut kwin = sources.kwin.topic.subscribe();
    let mut mixer = sources.mixer.topic.subscribe();
    let mut stop = service.stop.subscribe();
    let origin = Instant::now();
    let now = || origin.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let mut value = empty("system-tiles");
    let mut awake = core::KeepAwake::new();
    let mut cookie: Option<Cookie> = None;
    let mut retry: Option<u64> = None;
    value.tailscale = tailscale(&runner).await;
    value.voice = voice(voices(&runner).await);
    let mut next_tail = now().saturating_add(30_000);
    let mut jobs = JoinSet::new();
    loop {
        if *stop.borrow() {
            break;
        }
        let saver = sources.ss.current();
        let picker = sources.kwin.current();
        value.keep_awake = wire::KeepAwakeTile {
            available: saver.owner.is_some() && retry.is_none(),
            detail: if retry.is_some() {
                "release-pending".into()
            } else {
                saver.detail.clone()
            },
            on: saver.owner.is_some().then_some(cookie.is_some()),
            remaining_ms: awake
                .auto_off_at_ms()
                .map(|at| at.saturating_sub(now()) as f64),
        };
        value.output = output(&sources.mixer.current());
        value.network = network(&sources.nm.current());
        value.picker_available = picker.owner.is_some();
        value.available = value.keep_awake.available
            || value.output.available
            || value.network.available
            || value.voice.available
            || value.tailscale.available
            || value.picker_available;
        service.publish(value.clone());
        let wake = awake
            .auto_off_at_ms()
            .into_iter()
            .chain(retry)
            .chain(Some(next_tail))
            .min();
        tokio::select! {
            _=stop.changed()=>break,
            changed=nm.changed()=>{if changed.is_err(){break;}},
            changed=kwin.changed()=>{if changed.is_err(){break;}},
            changed=mixer.changed()=>{if changed.is_err(){break;}},
            changed=ss.changed()=>{
                if changed.is_err(){break;}
                if cookie.as_ref().is_some_and(|c|Some(c.owner.as_str())!=sources.ss.current().owner.as_deref()) {
                    if let Some(held)=cookie.take(){if let Err(error)=held.release().await{tracing::debug!(%error,"Former ScreenSaver owner already gone");}}
                    awake.on_bus_lost();retry=None;
                }
            }
            ()=wait(wake,now())=>{
                let off=matches!(awake.tick(now()),Some(core::Action::UnInhibit{..}));
                if off || retry.is_some_and(|at|now()>=at) {if let Some(held)=&cookie {
                    match held.release().await {Ok(())=>{cookie=None;retry=None;},Err(_)=>{retry=Some(now().saturating_add(1000));}}
                }}
                if now()>=next_tail {value.tailscale=tailscale(&runner).await;next_tail=now().saturating_add(30_000);}
            }
            result=jobs.join_next(),if !jobs.is_empty()=>{match result {
                Some(Ok(Job::Colour(result)))=>{value.picker_busy=false;match result{Ok(colour)=>{value.last_colour=Some(colour);value.action_error=None;},Err(error)=>value.action_error=Some(error.message)}},
                Some(Ok(Job::Screenshot(result)))=>{value.screenshot_busy=false;value.action_error=result.err().map(|e|e.message);},
                _=>{value.picker_busy=false;value.screenshot_busy=false;value.action_error=Some("Controls action worker failed".into());}
            }}
            request=rx.recv()=>{
                let Some(request)=request else{break;};if request.reply.is_closed(){continue;}
                let result=async {match request.op {
                    wire::TilesOp::Refresh=>{value.tailscale=tailscale(&runner).await;value.voice=voice(voices(&runner).await);}
                    wire::TilesOp::KeepAwake{on,duration_ms}=>{
                        if duration_ms.is_some_and(|d|d==0 || d>604_800_000){return Err(IslandError::invalid("Keep awake duration invalid"));}
                        if on {
                            let owner=sources.ss.current().owner.ok_or_else(||IslandError::unavailable("ScreenSaver unavailable"))?;
                            if cookie.is_none() {
                                let address=session.as_deref().ok_or_else(||IslandError::unavailable("No private session bus"))?;let conn=dbus_util::connect(address).await?;
                                if let Some(core::Action::Inhibit{app,reason})=awake.turn_on(now(),"User enabled keep awake") {
                                    let reply=bus::call::<_,u32>(&conn,&owner,SCREENSAVER_PATH,SCREENSAVER,"Inhibit",&(app.as_str(),reason.as_str())).await;
                                    match reply {Ok(id)=>{cookie=Some(Cookie{conn,owner,cookie:id});awake.inhibited(id);},Err(error)=>{awake.failed("inhibit-failed");return Err(error);}}
                                }
                            }
                            awake.set_auto_off(duration_ms.map(u64::from),now());
                        } else {
                            awake.turn_off();if let Some(held)=&cookie {if let Err(error)=held.release().await{retry=Some(now().saturating_add(1000));return Err(error);}}
                            cookie=None;retry=None;
                        }
                    }
                    wire::TilesOp::Output{name}=>{core::set_default_argv(&name).map_err(|_|IslandError::invalid("Output target invalid"))?;let observed=sources.mixer.request(MixerOp::DefaultSink{name}).await?;value.output=output(&observed);}
                    wire::TilesOp::Voice{name}=>{
                        if name.starts_with('-') || !core::is_valid_voice_name(&name){return Err(IslandError::invalid("Voice name invalid"));}
                        let list=voices(&runner).await?;let command=core::switch_argv(&list,&name).map_err(|_|IslandError::invalid("Unknown/reserved voice name"))?;runner.text(&command).await?;
                        let observed=voices(&runner).await?;if observed.active.as_deref()!=Some(name.as_str()){return Err(IslandError::unavailable("Voice switch was not confirmed"));}value.voice=voice(Ok(observed));
                    }
                    wire::TilesOp::PickColour=>{
                        if value.picker_busy{return Err(IslandError::unavailable("Colour picker already open"));}
                        let owner=sources.kwin.current().owner.ok_or_else(||IslandError::unavailable("KWin picker unavailable"))?;let address=session.clone().ok_or_else(||IslandError::unavailable("No private session bus"))?;let clipboard=clipboard.clone();
                        value.picker_busy=true;jobs.spawn(async move{Job::Colour(pick(address,owner,clipboard).await)});
                    }
                    wire::TilesOp::Screenshot=>{
                        if value.screenshot_busy{return Err(IslandError::unavailable("Screenshot already open"));}
                        let runner=runner.clone();value.screenshot_busy=true;jobs.spawn(async move {Job::Screenshot(runner.run(&core::screenshot_argv(),Duration::from_secs(120),64*1024).await.and_then(|o|if o.code==0{Ok(())}else{Err(IslandError::unavailable("Screenshot launcher failed"))}))});
                    }
                } Ok(service.publish(value.clone()))}.await;
                respond(request.reply,result);
            }
        }
    }
    jobs.abort_all();
    while jobs.join_next().await.is_some() {}
    awake.turn_off();
    if let Some(held) = cookie {
        if let Err(error) = held.release().await {
            tracing::warn!(%error,"Keep awake shutdown release failed");
        }
    }
    sources.shutdown().await;
    service.publish(empty("stopped"));
}
