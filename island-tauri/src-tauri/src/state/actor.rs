//! A bounded single-owner actor: the ONLY authority for `IslandState` (SPEC §4.1/§8 Phase 3).
//! It is wired to the host by `state::host` (event emission + window effects).
use super::machine::{Clock, Effect, Intent, Machine, Presentation};
use crate::{
    contract::{island::IslandState, settings::Settings},
    error::IslandError,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, watch};

pub const CAPACITY: usize = 256;
pub struct MonotonicClock(Instant);
impl Default for MonotonicClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl Clock for MonotonicClock {
    fn now_ms(&self) -> u64 {
        self.0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub state: IslandState,
    pub presentation: Presentation,
}
impl From<&Machine> for Snapshot {
    fn from(m: &Machine) -> Self {
        Self {
            state: m.state.clone(),
            presentation: m.presentation().clone(),
        }
    }
}
/// Effect delivery is synchronous and must not block. A host should enqueue to
/// its GTK/service executor. Errors terminate the actor rather than vanish.
pub trait Sink: Send + 'static {
    fn apply(&mut self, effects: &[Effect], snapshot: &Snapshot) -> Result<(), IslandError>;
}
struct Request {
    intent: Option<Intent>,
    settings: Option<Settings>,
    tick: bool,
    /// The intent came from the webview DOM, which already gave its own click feedback.
    ui_origin: bool,
    reply: oneshot::Sender<Result<Snapshot, IslandError>>,
}
#[derive(Clone)]
pub struct Handle {
    queue: mpsc::Sender<Request>,
    stop: watch::Sender<bool>,
    /// Latest state, published BEFORE the effect sink runs for the same change, so a reader
    /// that has seen an event's `rev` can never observe an older value here.
    latest: watch::Sender<IslandState>,
}
impl Handle {
    fn enqueue(
        &self,
        intent: Option<Intent>,
        settings: Option<Settings>,
        tick: bool,
        ui_origin: bool,
    ) -> Result<oneshot::Receiver<Result<Snapshot, IslandError>>, IslandError> {
        if *self.stop.borrow() {
            return Err(IslandError::unavailable("State actor stopped"));
        }
        let (reply, receive) = oneshot::channel();
        self.queue
            .try_send(Request {
                intent,
                settings,
                tick,
                ui_origin,
                reply,
            })
            .map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => {
                    IslandError::new(crate::error::ErrorCode::Busy, "State actor queue full")
                }
                mpsc::error::TrySendError::Closed(_) => {
                    IslandError::unavailable("State actor stopped")
                }
            })?;
        Ok(receive)
    }
    async fn request(
        &self,
        intent: Option<Intent>,
        settings: Option<Settings>,
        tick: bool,
        ui_origin: bool,
    ) -> Result<Snapshot, IslandError> {
        self.enqueue(intent, settings, tick, ui_origin)?
            .await
            .map_err(|_| IslandError::unavailable("State actor stopped"))?
    }
    pub async fn dispatch(&self, intent: Intent) -> Result<Snapshot, IslandError> {
        self.request(Some(intent), None, false, false).await
    }
    /// Intent originating in the webview DOM (`island_dispatch`). Identical to `dispatch`
    /// except that `PlayClick` is withheld: the DOM click capture already played the sound,
    /// so the cue would double it. Rust-origin intents (panel, tray) keep their cue.
    pub async fn dispatch_ui(&self, intent: Intent) -> Result<Snapshot, IslandError> {
        self.request(Some(intent), None, false, true).await
    }
    /// Non-blocking, fire-and-forget enqueue for the panel socket thread (never waits on the
    /// actor). `Busy` when the bounded queue is full.
    pub fn try_dispatch(&self, intent: Intent) -> Result<(), IslandError> {
        self.enqueue(Some(intent), None, false, false).map(drop)
    }
    pub async fn snapshot(&self) -> Result<Snapshot, IslandError> {
        self.request(None, None, false, false).await
    }
    pub async fn settings(&self, settings: Settings) -> Result<Snapshot, IslandError> {
        self.request(None, Some(settings), false, false).await
    }
    pub async fn tick(&self) -> Result<Snapshot, IslandError> {
        self.request(None, None, true, false).await
    }
    /// Latest published state (synchronous; for status files and revision checks).
    pub fn state(&self) -> IslandState {
        self.latest.borrow().clone()
    }
    /// Subscribe to every state publication (status-file refresh).
    pub fn watch(&self) -> watch::Receiver<IslandState> {
        self.latest.subscribe()
    }
    pub fn shutdown(&self) {
        self.stop.send_replace(true);
    }
    /// False once the actor was told to stop or its task has ended (its queue is closed).
    /// Lets a caller tell "the authority is gone" from an ordinary refused intent.
    pub fn is_running(&self) -> bool {
        !*self.stop.borrow() && !self.queue.is_closed()
    }
}
/// Publish only a changed state: every publication rewrites the status file and wakes
/// its watchers, and most effects (a refused focus, a cue) leave the state untouched.
fn publish(latest: &watch::Sender<IslandState>, state: &IslandState) {
    latest.send_if_modified(|current| {
        if current == state {
            false
        } else {
            *current = state.clone();
            true
        }
    });
}
pub fn spawn(
    settings: Settings,
    clock: Arc<dyn Clock + Send + Sync>,
    mut sink: impl Sink,
) -> (Handle, tokio::task::JoinHandle<Result<(), IslandError>>) {
    let (queue, mut receive) = mpsc::channel::<Request>(CAPACITY);
    let (stop, mut stopped) = watch::channel(false);
    let mut machine = Machine::new(&settings);
    let (latest, _) = watch::channel(machine.state.clone());
    let handle = Handle {
        queue,
        stop,
        latest: latest.clone(),
    };
    let task = tokio::spawn(async move {
        let mut settings = settings;
        struct Now(u64);
        impl Clock for Now {
            fn now_ms(&self) -> u64 {
                self.0
            }
        }
        loop {
            let wait = machine
                .next_deadline()
                .map(|d| Duration::from_millis(d.saturating_sub(clock.now_ms())));
            let timer = async {
                match wait {
                    Some(wait) => tokio::time::sleep(wait).await,
                    None => std::future::pending::<()>().await,
                }
            };
            tokio::select! { biased;
                changed=stopped.changed()=>{if changed.is_err() || *stopped.borrow() {break;}}
                message=receive.recv()=>{
                    let Some(request)=message else {break;};
                    let now=Now(clock.now_ms());
                    let mut effects=if request.tick {machine.tick(&settings,&now)?} else {Vec::new()};
                    if let Some(next)=request.settings {
                        // Only a CHANGED settings.dnd drives the state; an unrelated patch must not
                        // overwrite a dnd that a typed `SetDnd` intent changed in the meantime.
                        let dnd_changed = next.dnd != settings.dnd;
                        let game_changed = next.game_mode != settings.game_mode || next.game_mode_auto != settings.game_mode_auto || next.game_mode_dnd != settings.game_mode_dnd;
                        settings=next;
                        if dnd_changed { effects.extend(machine.reduce(Intent::SetDnd(settings.dnd),&settings,&now)?); }
                        if game_changed { effects.extend(machine.reduce(Intent::SyncGameMode,&settings,&now)?); }
                    }
                    let result=match request.intent {Some(intent)=>machine.reduce(intent,&settings,&now),None=>Ok(Vec::new())};
                    match result {
                        Ok(next)=>{
                            effects.extend(next);
                            if request.ui_origin { effects.retain(|e| *e != Effect::PlayClick); }
                            publish(&latest,&machine.state);
                            let snapshot=Snapshot::from(&machine);
                            // A failed side effect (cue, clipboard pause, emit) is reported to the
                            // caller but never takes the state authority down: a dead actor would
                            // freeze the whole island with no way back short of a restart.
                            if let Err(error)=sink.apply(&effects,&snapshot) {
                                tracing::warn!(?error,"State effect failed");
                                if request.reply.send(Err(error)).is_err() {tracing::debug!("State caller disconnected during effect failure");}
                                continue;
                            }
                            if request.reply.send(Ok(snapshot)).is_err() {tracing::debug!("State caller disconnected");}
                        }
                        Err(error)=>{if request.reply.send(Err(error)).is_err() {tracing::debug!("State caller disconnected during invalid intent");}}
                    }
                }
                ()=timer=>{
                    let effects=machine.tick(&settings,&Now(clock.now_ms()))?;
                    publish(&latest,&machine.state);
                    if let Err(error)=sink.apply(&effects,&Snapshot::from(&machine)) {
                        tracing::warn!(?error,"State effect failed after a timer tick");
                    }
                }
            }
        }
        receive.close();
        while let Ok(request) = receive.try_recv() {
            if request
                .reply
                .send(Err(IslandError::unavailable("State actor stopped")))
                .is_err()
            {
                tracing::debug!("State caller already disconnected");
            }
        }
        Ok(())
    });
    (handle, task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::island::{IslandMode, IslandView};
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    };
    struct Fake(AtomicU64);
    impl Clock for Fake {
        fn now_ms(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
    }
    struct Log(Arc<Mutex<Vec<Snapshot>>>);
    impl Sink for Log {
        fn apply(&mut self, e: &[Effect], s: &Snapshot) -> Result<(), IslandError> {
            if e.contains(&Effect::EmitState) {
                self.0.lock().unwrap().push(s.clone());
            }
            Ok(())
        }
    }
    #[tokio::test]
    async fn actor_serializes_typed_view_changes_snapshots_and_injected_timer_ticks() {
        let clock = Arc::new(Fake(AtomicU64::new(0)));
        let log = Arc::new(Mutex::new(Vec::new()));
        let (handle, task) = spawn(Settings::default(), clock.clone(), Log(log.clone()));
        handle.dispatch(Intent::PanelEnter).await.unwrap();
        clock.0.store(180, Ordering::SeqCst);
        assert_eq!(handle.tick().await.unwrap().state.mode, IslandMode::Hover);
        let changed = handle
            .dispatch(Intent::SelectView(IslandView::Ai))
            .await
            .unwrap();
        assert_eq!(changed.state.view, IslandView::Ai);
        assert_eq!(changed.state.mode, IslandMode::Large);
        assert_eq!(handle.snapshot().await.unwrap().state, changed.state);
        assert_eq!(log.lock().unwrap().last().unwrap().state, changed.state);
        assert_eq!(handle.state(), changed.state, "published before the sink");
        handle.shutdown();
        task.await.unwrap().unwrap();
        assert!(handle.snapshot().await.is_err());
    }
    #[tokio::test]
    async fn game_effects_settings_checkbox_reduces_immediately_without_persisting_dnd() {
        let mut settings = Settings {
            game_mode: true,
            game_mode_dnd: false,
            ..Settings::default()
        };
        let (handle, task) = spawn(
            settings.clone(),
            Arc::new(Fake(AtomicU64::new(0))),
            Log(Arc::new(Mutex::new(Vec::new()))),
        );
        assert_eq!(
            handle
                .dispatch(Intent::NotificationArrived("visible".into()))
                .await
                .unwrap()
                .state
                .mode,
            IslandMode::Hover
        );
        settings.game_mode_dnd = true;
        let quiet = handle.settings(settings.clone()).await.unwrap();
        assert_eq!(quiet.state.mode, IslandMode::Still);
        assert!(!quiet.state.dnd && !settings.dnd);
        settings.game_mode_dnd = false;
        handle.settings(settings).await.unwrap();
        assert_eq!(
            handle
                .dispatch(Intent::NotificationArrived("resumed".into()))
                .await
                .unwrap()
                .state
                .mode,
            IslandMode::Hover
        );
        handle.shutdown();
        task.await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn settings_dnd_is_actor_owned_and_effect_failure_is_reported() {
        struct Fail;
        impl Sink for Fail {
            fn apply(&mut self, e: &[Effect], _: &Snapshot) -> Result<(), IslandError> {
                if e.contains(&Effect::RequestFocus) {
                    Err(IslandError::unavailable("fixture focus failure"))
                } else {
                    Ok(())
                }
            }
        }
        let (handle, task) = spawn(Settings::default(), Arc::new(Fake(AtomicU64::new(0))), Fail);
        assert!(
            handle
                .settings(Settings {
                    dnd: true,
                    ..Settings::default()
                })
                .await
                .unwrap()
                .state
                .dnd
        );
        assert!(handle.dispatch(Intent::Pin).await.is_err());
        // A failed effect is reported, but the actor keeps serving.
        assert_eq!(
            handle.dispatch(Intent::Collapse).await.unwrap().state.mode,
            IslandMode::Still
        );
        handle.shutdown();
        task.await.unwrap().unwrap();
    }
    #[tokio::test(flavor = "current_thread")]
    async fn actor_queue_is_bounded_and_shutdown_is_not_queued_behind_work() {
        let (handle, task) = spawn(
            Settings::default(),
            Arc::new(Fake(AtomicU64::new(0))),
            Log(Arc::new(Mutex::new(Vec::new()))),
        );
        for _ in 0..CAPACITY {
            let (reply, _receive) = oneshot::channel();
            handle
                .queue
                .try_send(Request {
                    intent: None,
                    settings: None,
                    tick: false,
                    ui_origin: false,
                    reply,
                })
                .unwrap();
        }
        assert_eq!(
            handle.snapshot().await.unwrap_err().code,
            crate::error::ErrorCode::Busy
        );
        assert_eq!(
            handle.try_dispatch(Intent::PanelEnter).unwrap_err().code,
            crate::error::ErrorCode::Busy,
            "the non-blocking panel path is bounded too"
        );
        handle.shutdown();
        task.await.unwrap().unwrap();
    }
    /// A DOM-origin Pin must not produce a second click (the DOM already played it); a
    /// Rust-origin pin (panel/tray) must. An unrelated settings change keeps a typed DND.
    #[tokio::test]
    async fn dom_origin_intents_withhold_the_click_cue_and_settings_keep_typed_dnd() {
        struct Cues(Arc<Mutex<Vec<bool>>>);
        impl Sink for Cues {
            fn apply(&mut self, e: &[Effect], _: &Snapshot) -> Result<(), IslandError> {
                if e.contains(&Effect::EmitState) {
                    self.0.lock().unwrap().push(e.contains(&Effect::PlayClick));
                }
                Ok(())
            }
        }
        let cues = Arc::new(Mutex::new(Vec::new()));
        let settings = Settings {
            click_sounds: true,
            ..Settings::default()
        };
        let (handle, task) = spawn(
            settings.clone(),
            Arc::new(Fake(AtomicU64::new(0))),
            Cues(cues.clone()),
        );
        handle.dispatch_ui(Intent::Pin).await.unwrap();
        handle.dispatch(Intent::PanelPin).await.unwrap(); // large -> still
        handle.dispatch(Intent::PanelPin).await.unwrap(); // still -> large
        assert_eq!(*cues.lock().unwrap(), [false, true, true]);
        assert!(
            handle
                .dispatch(Intent::SetDnd(true))
                .await
                .unwrap()
                .state
                .dnd
        );
        // an unrelated settings change (dnd unchanged: false -> false) must not clear it
        let kept = handle
            .settings(Settings {
                hover_delay: 400.,
                ..settings
            })
            .await
            .unwrap();
        assert!(kept.state.dnd);
        handle.shutdown();
        task.await.unwrap().unwrap();
    }
}
