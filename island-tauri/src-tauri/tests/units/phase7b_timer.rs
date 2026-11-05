#![allow(clippy::unwrap_used)]
use hermes_island_lib::{
    contract::apps::{TimerCompletion, TimerKind, TimerOp},
    error::IslandError,
    integrations::timer::{core::TimerEvent, CompletionSink, TimerService},
    state::machine::Clock,
};
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc,
};
struct Fake(AtomicU64);
impl Clock for Fake {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}
#[derive(Default)]
struct Sink(AtomicUsize);
impl CompletionSink for Sink {
    fn complete(&self, _: TimerEvent, _: u64) -> Result<(), IslandError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn clear(&self) -> Result<(), IslandError> {
        self.0.store(0, Ordering::SeqCst);
        Ok(())
    }
}
#[test]
fn authoritative_completion_kind_survives_phase_transition_and_dismisses_both_activities() {
    let clock = Arc::new(Fake(AtomicU64::new(0)));
    let sink = Arc::new(Sink::default());
    let timer = TimerService::start(clock.clone(), sink.clone()).unwrap();
    tauri::async_runtime::block_on(async {
        timer
            .request(Some(TimerOp::SetCountdown { duration_ms: 1000 }))
            .await
            .unwrap();
        timer
            .request(Some(TimerOp::Start {
                kind: TimerKind::Countdown,
            }))
            .await
            .unwrap();
        clock.0.store(1000, Ordering::SeqCst);
        assert_eq!(
            timer.request(None).await.unwrap().completed_kind,
            Some(TimerCompletion::Countdown)
        );
        assert_eq!(
            timer
                .request(Some(TimerOp::Cancel {
                    kind: TimerKind::Countdown
                }))
                .await
                .unwrap()
                .completed_kind,
            None
        );
        assert_eq!(sink.0.load(Ordering::SeqCst), 0);
        timer
            .request(Some(TimerOp::ConfigurePomodoro {
                focus_ms: 60000,
                short_break_ms: 60000,
                long_break_ms: 60000,
                rounds: 1,
                auto_start: true,
            }))
            .await
            .unwrap();
        timer
            .request(Some(TimerOp::Start {
                kind: TimerKind::Pomodoro,
            }))
            .await
            .unwrap();
        clock.0.store(61000, Ordering::SeqCst);
        let completed = timer.request(None).await.unwrap();
        assert_eq!(completed.completed_kind, Some(TimerCompletion::Pomodoro));
        assert_eq!(completed.phase, "longBreak");
        assert_eq!(
            timer.request(None).await.unwrap().completed_kind,
            Some(TimerCompletion::Pomodoro)
        );
        assert_eq!(
            timer
                .request(Some(TimerOp::Cancel {
                    kind: TimerKind::Pomodoro
                }))
                .await
                .unwrap()
                .completed_kind,
            None
        );
        assert_eq!(sink.0.load(Ordering::SeqCst), 0);
    });
    timer.shutdown().unwrap();
}
