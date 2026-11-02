//! A bounded Rust app actor owns TimerCore. No view owns timer state; the existing
//! StateActor remains the sole IslandState owner and receives activity intents only.
pub mod core;
use crate::{
    bus::Latest,
    contract::apps::{TimerChanged, TimerCompletion, TimerKind, TimerLap, TimerOp},
    error::IslandError,
    state::machine::Clock,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch};
pub trait CompletionSink: Send + Sync + 'static {
    fn complete(&self, event: core::TimerEvent, now: u64) -> Result<(), IslandError>;
    fn clear(&self) -> Result<(), IslandError>;
}
struct Request {
    op: Option<TimerOp>,
    reply: oneshot::Sender<Result<TimerChanged, IslandError>>,
}
pub struct TimerService {
    queue: mpsc::Sender<Request>,
    stop: watch::Sender<bool>,
    pub topic: Latest<TimerChanged>,
    worker: Mutex<Option<tauri::async_runtime::JoinHandle<Result<(), IslandError>>>>,
}
fn snapshot(
    core: &core::TimerCore,
    now: u64,
    rev: u32,
    completed_kind: Option<TimerCompletion>,
) -> TimerChanged {
    let s = core.snapshot(now);
    TimerChanged {
        rev,
        completed_kind,
        available: true,
        detail: "ready".into(),
        sampled_at_ms: now as f64,
        countdown_status: format!("{:?}", s.countdown().status()).to_lowercase(),
        remaining_ms: s.countdown().remaining_ms() as f64,
        pomodoro_status: format!("{:?}", s.pomodoro().status()).to_lowercase(),
        phase: match s.pomodoro().phase() {
            core::Phase::Focus => "focus",
            core::Phase::ShortBreak => "shortBreak",
            core::Phase::LongBreak => "longBreak",
        }
        .into(),
        phase_remaining_ms: s.pomodoro().remaining_ms() as f64,
        round: s.pomodoro().round(),
        stopwatch_status: format!("{:?}", s.stopwatch().status()).to_lowercase(),
        elapsed_ms: s.stopwatch().elapsed_ms() as f64,
        laps: s
            .stopwatch()
            .laps()
            .iter()
            .map(|l| TimerLap {
                number: l.number() as f64,
                elapsed_ms: l.elapsed_ms() as f64,
            })
            .collect(),
    }
}
fn apply(core: &mut core::TimerCore, op: TimerOp, now: u64) -> Result<(), IslandError> {
    use TimerKind::*;
    let result = match op {
        TimerOp::SetCountdown { duration_ms } => core.set_countdown(duration_ms.into()),
        TimerOp::Start { kind } => match kind {
            Countdown => core.start(now),
            Pomodoro => core.pomo_start(now),
            Stopwatch => core.sw_start(now),
        },
        TimerOp::Pause { kind } => match kind {
            Countdown => core.pause(now),
            Pomodoro => core.pomo_pause(now),
            Stopwatch => core.sw_pause(now),
        },
        TimerOp::Resume { kind } => match kind {
            Countdown => core.resume(now),
            Pomodoro => core.pomo_resume(now),
            Stopwatch => core.sw_resume(now),
        },
        TimerOp::Cancel { kind } => {
            match kind {
                Countdown => core.reset(),
                Pomodoro => core.pomo_reset(),
                Stopwatch => core.sw_reset(),
            }
            Ok(())
        }
        TimerOp::AddTime { duration_ms } => core.add_time(now, duration_ms.into()),
        TimerOp::Skip => core.pomo_skip(now),
        TimerOp::Lap => core.sw_lap(now),
        TimerOp::ConfigurePomodoro {
            focus_ms,
            short_break_ms,
            long_break_ms,
            rounds,
            auto_start,
        } => {
            let s = core.snapshot(now);
            if s.countdown().status() != core::CountdownStatusKind::Idle
                || s.pomodoro().status() != core::PomodoroStatusKind::Idle
                || s.stopwatch().status() != core::StopwatchStatusKind::Idle
            {
                return Err(IslandError::invalid(
                    "Configure pomodoro only while all timers are idle",
                ));
            }
            match core::PomodoroConfig::new(
                focus_ms.into(),
                short_break_ms.into(),
                long_break_ms.into(),
                rounds.into(),
                auto_start,
            ) {
                Ok(config) => {
                    *core = core::TimerCore::new(config);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    };
    result.map_err(|_| IslandError::invalid("Invalid timer operation or duration"))
}
impl TimerService {
    pub fn start(
        clock: Arc<dyn Clock + Send + Sync>,
        sink: Arc<dyn CompletionSink>,
    ) -> Result<Arc<Self>, IslandError> {
        let config = core::PomodoroConfig::new(25 * 60_000, 5 * 60_000, 15 * 60_000, 4, false)
            .map_err(|_| IslandError::internal("Invalid default pomodoro"))?;
        let mut core = core::TimerCore::new(config);
        let (queue, mut rx) = mpsc::channel::<Request>(128);
        let (stop, mut stopped) = watch::channel(false);
        let service = Arc::new(Self {
            queue,
            stop,
            topic: Latest::new(),
            worker: Mutex::new(None),
        });
        service
            .topic
            .publish(snapshot(&core, clock.now_ms(), 0, None));
        let s = Arc::clone(&service);
        let task = tauri::async_runtime::spawn(async move {
            let mut rev = 0u32;
            let mut completed_kind = None;
            loop {
                let now = clock.now_ms();
                let deadline = core.next_deadline();
                let wait = async {
                    match deadline {
                        Some(at) => {
                            tokio::time::sleep(Duration::from_millis(at.saturating_sub(now))).await
                        }
                        None => std::future::pending().await,
                    }
                };
                let request = tokio::select! {biased; _=stopped.changed()=>break,request=rx.recv()=>{let Some(request)=request else {break;};Some(request)},()=wait=>None};
                let now = clock.now_ms();
                for event in core.tick(now) {
                    completed_kind = Some(match event {
                        core::TimerEvent::CountdownFinished => TimerCompletion::Countdown,
                        core::TimerEvent::PomodoroPhaseEnded { .. } => TimerCompletion::Pomodoro,
                    });
                    sink.complete(event, now)?;
                }
                let result = match request.as_ref().and_then(|r| r.op.as_ref()) {
                    Some(op) => {
                        let reset =
                            matches!(op, TimerOp::Cancel { .. } | TimerOp::SetCountdown { .. });
                        let result = apply(&mut core, op.clone(), now);
                        if result.is_ok() && reset {
                            sink.clear()?;
                            completed_kind = None;
                        }
                        result
                    }
                    None => Ok(()),
                };
                rev = rev.wrapping_add(1);
                let value = snapshot(&core, now, rev, completed_kind);
                s.topic.publish(value.clone());
                if let Some(request) = request {
                    if request.reply.send(result.map(|()| value)).is_err() {
                        tracing::debug!("Timer caller disconnected");
                    }
                }
            }
            Ok(())
        });
        *service
            .worker
            .lock()
            .map_err(|_| IslandError::internal("Timer worker poisoned"))? = Some(task);
        Ok(service)
    }
    pub async fn request(&self, op: Option<TimerOp>) -> Result<TimerChanged, IslandError> {
        if *self.stop.borrow() {
            return Err(IslandError::unavailable("Timer stopped"));
        }
        let (reply, rx) = oneshot::channel();
        self.queue
            .try_send(Request { op, reply })
            .map_err(|_| IslandError::unavailable("Timer busy or stopped"))?;
        rx.await
            .map_err(|_| IslandError::unavailable("Timer stopped"))?
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        self.stop.send_replace(true);
        let task = self
            .worker
            .lock()
            .map_err(|_| IslandError::internal("Timer worker poisoned"))?
            .take();
        if let Some(task) = task {
            tauri::async_runtime::block_on(async {
                tokio::time::timeout(Duration::from_secs(2), task)
                    .await
                    .map_err(|_| IslandError::timeout("Timer shutdown"))?
                    .map_err(|_| IslandError::internal("Timer worker panicked"))?
            })?;
        }
        Ok(())
    }
}
