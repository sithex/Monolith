#![allow(clippy::unwrap_used)]
use hermes_island_lib::{
    contract::{
        apps::{TimerKind, TimerOp},
        island::IslandView,
        settings::Settings,
    },
    error::IslandError,
    integrations::{
        apps::TimerHubSink,
        notifications::hub::Hub,
        timer::{core::TimerEvent, CompletionSink, TimerService},
    },
    services::{autostart::Autostart, settings::SettingsService},
    state::{
        actor,
        machine::{Clock, Effect, Intent},
    },
};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
struct Fake(AtomicU64);
impl Clock for Fake {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}
struct Sink;
impl actor::Sink for Sink {
    fn apply(&mut self, _: &[Effect], _: &actor::Snapshot) -> Result<(), IslandError> {
        Ok(())
    }
}
struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("hi-6b1-timer-{tag}-{}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _clean = fs::remove_dir_all(&self.0);
    }
}
struct Shutdown(Arc<TimerService>);
impl Drop for Shutdown {
    fn drop(&mut self) {
        let _stopped = self.0.shutdown();
    }
}
#[test]
fn timer_completion_reaches_own_hub_and_activity_after_view_switch() {
    let dir = Scratch::new("hub");
    let clock = Arc::new(Fake(AtomicU64::new(0)));
    let (handle, state_task) = tauri::async_runtime::block_on(async {
        actor::spawn(Settings::default(), clock.clone(), Sink)
    });
    let settings = Arc::new(SettingsService::new(
        dir.0.clone(),
        Autostart::with_default_launcher(dir.0.clone(), dir.0.join("app"), true),
    ));
    let hub = Hub::new(handle.clone(), settings);
    let service = TimerService::start(
        clock.clone(),
        Arc::new(TimerHubSink {
            hub: hub.clone(),
            actor: handle.clone(),
            alarm: None,
        }),
    )
    .unwrap();
    let _stop = Shutdown(service.clone());
    tauri::async_runtime::block_on(async {
        service
            .request(Some(TimerOp::SetCountdown { duration_ms: 1000 }))
            .await
            .unwrap();
        service
            .request(Some(TimerOp::Start {
                kind: TimerKind::Countdown,
            }))
            .await
            .unwrap();
        handle
            .dispatch(Intent::SelectView(IslandView::Monitor))
            .await
            .unwrap();
        clock.0.store(250, Ordering::SeqCst);
        assert_eq!(
            service
                .request(Some(TimerOp::Pause {
                    kind: TimerKind::Countdown
                }))
                .await
                .unwrap()
                .remaining_ms,
            750.
        );
        clock.0.store(5000, Ordering::SeqCst);
        service
            .request(Some(TimerOp::Resume {
                kind: TimerKind::Countdown,
            }))
            .await
            .unwrap();
        handle
            .dispatch(Intent::SelectView(IslandView::Ai))
            .await
            .unwrap();
        clock.0.store(5750, Ordering::SeqCst);
        let finished = service.request(None).await.unwrap();
        assert_eq!(finished.countdown_status, "finished");
        assert_eq!(hub.list().unwrap().len(), 1);
        assert_eq!(hub.list().unwrap()[0].title, "Timer finished");
        let state = handle.snapshot().await.unwrap();
        assert_eq!(state.presentation.compact, vec!["timer-completed"]);
        service.request(None).await.unwrap();
        assert_eq!(hub.list().unwrap().len(), 1, "completion once");
        service
            .request(Some(TimerOp::Cancel {
                kind: TimerKind::Countdown,
            }))
            .await
            .unwrap();
        assert!(handle
            .snapshot()
            .await
            .unwrap()
            .presentation
            .compact
            .is_empty());
        handle.shutdown();
        state_task.await.unwrap().unwrap();
    });
}
#[derive(Default)]
struct Log(Mutex<Vec<TimerEvent>>);
impl CompletionSink for Log {
    fn complete(&self, e: TimerEvent, _: u64) -> Result<(), IslandError> {
        self.0.lock().unwrap().push(e);
        Ok(())
    }
    fn clear(&self) -> Result<(), IslandError> {
        Ok(())
    }
}
#[test]
fn timer_actor_pomodoro_phases_stopwatch_and_cancel_use_only_injected_clock() {
    let clock = Arc::new(Fake(AtomicU64::new(0)));
    let log = Arc::new(Log::default());
    let service = TimerService::start(clock.clone(), log.clone()).unwrap();
    let _stop = Shutdown(service.clone());
    tauri::async_runtime::block_on(async {
        service
            .request(Some(TimerOp::ConfigurePomodoro {
                focus_ms: 60000,
                short_break_ms: 60000,
                long_break_ms: 60000,
                rounds: 1,
                auto_start: true,
            }))
            .await
            .unwrap();
        service
            .request(Some(TimerOp::Start {
                kind: TimerKind::Pomodoro,
            }))
            .await
            .unwrap();
        service
            .request(Some(TimerOp::Start {
                kind: TimerKind::Stopwatch,
            }))
            .await
            .unwrap();
        clock.0.store(60000, Ordering::SeqCst);
        let s = service.request(None).await.unwrap();
        assert_eq!(s.phase, "longBreak");
        assert_eq!(s.elapsed_ms, 60000.);
        assert_eq!(log.0.lock().unwrap().len(), 1);
        service.request(Some(TimerOp::Lap)).await.unwrap();
        service
            .request(Some(TimerOp::Pause {
                kind: TimerKind::Stopwatch,
            }))
            .await
            .unwrap();
        clock.0.store(120000, Ordering::SeqCst);
        let s = service.request(None).await.unwrap();
        assert_eq!(s.phase, "focus");
        assert_eq!(s.elapsed_ms, 60000.);
        assert_eq!(s.laps.len(), 1);
        service
            .request(Some(TimerOp::Cancel {
                kind: TimerKind::Pomodoro,
            }))
            .await
            .unwrap();
        service
            .request(Some(TimerOp::Cancel {
                kind: TimerKind::Stopwatch,
            }))
            .await
            .unwrap();
        clock.0.store(600000, Ordering::SeqCst);
        let s = service.request(None).await.unwrap();
        assert_eq!(s.pomodoro_status, "idle");
        assert_eq!(s.stopwatch_status, "idle");
        assert_eq!(s.elapsed_ms, 0.);
        assert_eq!(log.0.lock().unwrap().len(), 2);
    });
}
