//! pactl-subscribe driven mixer. No idle polling; reconnect timers only after loss.
use super::core as cmd;
use super::core as events;
use super::core::{self, MixerModel, RefreshScope, Sink, Stream};
use crate::{
    contract::controls::*,
    error::IslandError,
    integrations::{
        control_actor::{respond, ControlActor, Request},
        control_process::{self as process, argv, Lines, Runner},
    },
};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;
pub type MixerService = ControlActor<MixerChanged, MixerOp>;
/// Quiet gap that ends an event burst, and the longest a burst may delay one refresh.
const COALESCE: Duration = Duration::from_millis(40);
const COALESCE_MAX: Duration = Duration::from_millis(200);
fn merge(a: RefreshScope, b: RefreshScope) -> RefreshScope {
    match (a, b) {
        (RefreshScope::None, x) | (x, RefreshScope::None) => x,
        (RefreshScope::Sinks, RefreshScope::Sinks) => RefreshScope::Sinks,
        (RefreshScope::Streams, RefreshScope::Streams) => RefreshScope::Streams,
        _ => RefreshScope::Both,
    }
}
fn empty(detail: &str) -> MixerChanged {
    MixerChanged {
        rev: 0,
        available: false,
        detail: detail.into(),
        sinks: vec![],
        streams: vec![],
        apps: vec![],
        default_sink: None,
    }
}
impl MixerService {
    pub fn start(runner: Runner) -> Arc<Self> {
        let (service, rx) = Self::new(empty("starting"));
        let owned = service.clone();
        service.own(tauri::async_runtime::spawn(async move {
            worker(owned, runner, rx).await
        }));
        service
    }
}
async fn worker(
    service: Arc<MixerService>,
    runner: Runner,
    mut rx: mpsc::Receiver<Request<MixerChanged, MixerOp>>,
) {
    let mut stop = service.stop.subscribe();
    let mut backoff = 1;
    while !*stop.borrow() {
        let result = tokio::select! {_=stop.changed()=>break,result=session(&service,&runner,&mut rx)=>result};
        service.publish(empty("pactl-unavailable"));
        if let Err(error) = result {
            tracing::debug!(%error,"Mixer subscription lost");
        }
        tokio::select! {_=stop.changed()=>break,()=tokio::time::sleep(Duration::from_secs(backoff))=>{}}
        backoff = (backoff * 2).min(60);
    }
    service.publish(empty("stopped"));
}
#[derive(Default)]
struct Model {
    sinks: Vec<Sink>,
    streams: Vec<Stream>,
    default: Option<String>,
}
impl Model {
    fn mixer(&self) -> MixerModel {
        MixerModel::from(
            self.sinks.clone(),
            self.streams.clone(),
            self.default.clone(),
        )
    }
    fn wire(&self) -> MixerChanged {
        MixerChanged {
            rev: 0,
            available: true,
            detail: "pactl-subscribe".into(),
            sinks: self
                .sinks
                .iter()
                .map(|s| MixerSink {
                    index: s.index,
                    name: s.name.clone(),
                    description: s.description.clone(),
                    muted: s.muted,
                    volume_percent: s.volume_percent,
                    channels: s.channels,
                    state: s.state.clone(),
                })
                .collect(),
            streams: self
                .streams
                .iter()
                .map(|s| MixerStream {
                    index: s.index,
                    sink_index: s.sink_index,
                    app_name: s.app_name.clone(),
                    binary: s.binary.clone(),
                    pid: s.pid,
                    media_name: s.media_name.clone(),
                    icon_name: s.icon_name.clone(),
                    muted: s.muted,
                    corked: s.corked,
                    volume_percent: s.volume_percent,
                })
                .collect(),
            apps: self
                .mixer()
                .apps()
                .into_iter()
                .map(|a| MixerApp {
                    app_name: a.app_name,
                    streams: a.streams,
                    volume_percent: a.volume_percent,
                    muted: a.muted,
                    sink_index: a.sink_index,
                    idle: a.idle,
                })
                .collect(),
            default_sink: self.mixer().default_sink().map(|s| s.name.clone()),
        }
    }
    async fn load(&mut self, runner: &Runner, scope: RefreshScope) -> Result<(), IslandError> {
        // Parse a complete candidate before changing the last-good model.
        let mut next = Self {
            sinks: self.sinks.clone(),
            streams: self.streams.clone(),
            default: self.default.clone(),
        };
        if matches!(scope, RefreshScope::Sinks | RefreshScope::Both) {
            let text = runner
                .text(&argv(&["pactl", "-f", "json", "list", "sinks"]))
                .await?;
            validate_observations(&text, false)?;
            let (sinks, skipped) = core::parse_sinks_report(&text).map_err(|_| bad())?;
            if skipped != 0 || sinks.len() > 512 {
                return Err(bad());
            }
            next.sinks = sinks;
            next.default = core::parse_default_sink(
                &runner.text(&argv(&["pactl", "get-default-sink"])).await?,
            );
        }
        if matches!(scope, RefreshScope::Streams | RefreshScope::Both) {
            let text = runner
                .text(&argv(&["pactl", "-f", "json", "list", "sink-inputs"]))
                .await?;
            validate_observations(&text, true)?;
            let (streams, skipped) = core::parse_sink_inputs_report(&text).map_err(|_| bad())?;
            if skipped != 0 || streams.len() > 2048 {
                return Err(bad());
            }
            next.streams = streams;
        }
        *self = next;
        Ok(())
    }
    fn commands(&self, op: MixerOp) -> Result<Vec<Vec<String>>, IslandError> {
        let sink = |name: &str| {
            self.sinks
                .iter()
                .find(|s| s.name == name)
                .ok_or_else(|| IslandError::invalid("Sink not in current snapshot"))
        };
        let stream = |index: u32| {
            if self.streams.iter().any(|s| s.index == index) {
                Ok(())
            } else {
                Err(IslandError::invalid("Stream not in current snapshot"))
            }
        };
        let mute = |b| if b { cmd::MuteOp::On } else { cmd::MuteOp::Off };
        let result = match op {
            MixerOp::SinkVolume { name, percent } => cmd::set_sink_volume(sink(&name)?, percent),
            MixerOp::SinkMute { name, muted } => cmd::mute_sink(sink(&name)?, mute(muted)),
            MixerOp::StreamVolume { index, percent } => {
                stream(index)?;
                cmd::set_stream_volume(index, percent)
            }
            MixerOp::StreamMute { index, muted } => {
                stream(index)?;
                cmd::mute_stream(index, mute(muted))
            }
            MixerOp::DefaultSink { name } => cmd::set_default_sink(sink(&name)?),
            MixerOp::MoveStream { index, sink: name } => {
                stream(index)?;
                cmd::move_stream(index, sink(&name)?)
            }
            MixerOp::AppVolume {
                stream: index,
                percent,
            } => {
                let group = self
                    .mixer()
                    .apps()
                    .into_iter()
                    .find(|a| a.streams.contains(&index))
                    .ok_or_else(|| IslandError::invalid("App stream not in current snapshot"))?;
                return cmd::set_app_volume(&group, percent)
                    .map_err(|_| IslandError::invalid("Mixer command invalid"));
            }
        };
        result
            .map(|v| vec![v])
            .map_err(|_| IslandError::invalid("Mixer command invalid"))
    }
}
fn bad() -> IslandError {
    IslandError::unavailable("Mixer observations invalid or incomplete")
}
/// The staged parser intentionally tolerates missing booleans; observations may not fabricate them.
fn validate_observations(text: &str, streams: bool) -> Result<(), IslandError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| bad())?;
    let rows = value.as_array().ok_or_else(bad)?;
    if rows.iter().any(|r| {
        r.get("mute").and_then(|v| v.as_bool()).is_none()
            || (streams && r.get("corked").and_then(|v| v.as_bool()).is_none())
            || r.get("volume")
                .and_then(|v| v.as_object())
                .is_some_and(|v| v.len() > 255)
    }) {
        return Err(bad());
    }
    Ok(())
}
async fn session(
    service: &MixerService,
    runner: &Runner,
    rx: &mut mpsc::Receiver<Request<MixerChanged, MixerOp>>,
) -> Result<(), IslandError> {
    // Child exists before the initial read; queued events repair any read/subscribe race.
    let mut child = runner.spawn(&argv(&["pactl", "subscribe"]))?;
    let mut lines = Lines::new(child.stdout()?);
    let errors = process::drain(child.stderr()?);
    tokio::pin!(errors);
    let result=async {
        let mut model=Model::default();model.load(runner,RefreshScope::Both).await?;service.publish(model.wire());
        loop {tokio::select! {
            line=lines.next()=>{
                let line=line?;
                if let Some(event)=events::parse_event(&line) {let mut scope=events::needs_refresh(&event);
                    if scope!=RefreshScope::None {
                        // Coalesce a burst (a new stream emits several events at once) into one read,
                        // bounded so a continuous event stream can never starve the refresh.
                        let deadline=tokio::time::Instant::now()+COALESCE_MAX;
                        while let Ok(more)=tokio::time::timeout_at(deadline.min(tokio::time::Instant::now()+COALESCE),lines.next()).await {
                            if let Some(event)=events::parse_event(&more?) {scope=merge(scope,events::needs_refresh(&event));}
                        }
                        model.load(runner,scope).await?;service.publish(model.wire());}
                }
            }
            result=&mut errors=>{result?;return Err(IslandError::unavailable("pactl subscription closed"));}
            request=rx.recv()=>{
                let Some(request)=request else{return Ok(());};if request.reply.is_closed(){continue;}
                let commands=match model.commands(request.op){Ok(c)=>c,Err(e)=>{respond(request.reply,Err(e));continue;}};
                let result=async {for command in commands {runner.text(&command).await?;} model.load(runner,RefreshScope::Both).await?;Ok(service.publish(model.wire()))}.await;
                let failed=result.is_err();respond(request.reply,result);
                if failed {return Err(IslandError::unavailable("Mixer command/readback failed; group writes may be partial"));}
            }
        }}
    }.await;
    child.finish().await?;
    result
}
