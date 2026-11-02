use timer_core::{PomodoroConfig, TimerCore, TimerError};

fn cfg() -> PomodoroConfig {
    match PomodoroConfig::new(60_000, 60_000, 60_000, 4, false) {
        Ok(c) => c,
        Err(_) => panic!("unreachable: valid config"),
    }
}

#[test]
fn elapsed_accumulates_and_never_decreases_across_pause_resume() {
    let mut core = TimerCore::new(cfg());
    core.sw_start(0).unwrap();
    assert_eq!(core.snapshot(5_000).stopwatch().elapsed_ms(), 5_000);

    core.sw_pause(7_000).unwrap();
    // paused: elapsed frozen
    assert_eq!(core.snapshot(7_000).stopwatch().elapsed_ms(), 7_000);
    assert_eq!(core.snapshot(20_000).stopwatch().elapsed_ms(), 7_000);

    core.sw_resume(20_000).unwrap();
    assert_eq!(core.snapshot(23_000).stopwatch().elapsed_ms(), 10_000);

    core.sw_pause(25_000).unwrap();
    // accumulated 7_000 + (25_000 - 20_000) = 12_000
    assert_eq!(core.snapshot(25_000).stopwatch().elapsed_ms(), 12_000);
    core.sw_resume(25_000).unwrap();
    assert_eq!(core.snapshot(30_000).stopwatch().elapsed_ms(), 17_000);

    // monotonic across every observation
    let mut last = 0;
    for now in [
        0, 1_000, 5_000, 7_000, 20_000, 23_000, 25_000, 30_000, 100_000,
    ] {
        let e = core.snapshot(now).stopwatch().elapsed_ms();
        assert!(e >= last, "elapsed decreased at {now}: {e} < {last}");
        last = e;
    }
}

#[test]
fn start_resumes_and_double_start_is_invalid() {
    let mut core = TimerCore::new(cfg());
    core.sw_start(0).unwrap();
    assert_eq!(core.sw_start(1), Err(TimerError::InvalidTransition));

    core.sw_pause(5_000).unwrap();
    assert_eq!(core.sw_pause(6_000), Err(TimerError::InvalidTransition));

    // start from Paused resumes (keeps accumulated)
    core.sw_start(6_000).unwrap();
    core.sw_pause(8_000).unwrap();
    // accumulated 5_000 + (8_000 - 6_000) = 7_000
    assert_eq!(core.snapshot(8_000).stopwatch().elapsed_ms(), 7_000);

    // resume from Idle is invalid
    core.sw_reset();
    assert_eq!(core.sw_resume(0), Err(TimerError::InvalidTransition));
    // resume while Running is invalid
    core.sw_start(0).unwrap();
    assert_eq!(core.sw_resume(1), Err(TimerError::InvalidTransition));
}

#[test]
fn lap_requires_running() {
    let mut core = TimerCore::new(cfg());
    assert_eq!(core.sw_lap(0), Err(TimerError::InvalidTransition));

    core.sw_start(0).unwrap();
    core.sw_lap(1_000).unwrap();
    core.sw_pause(2_000).unwrap();
    assert_eq!(core.sw_lap(3_000), Err(TimerError::InvalidTransition));

    core.sw_resume(3_000).unwrap();
    assert!(core.sw_lap(4_000).is_ok());
}

#[test]
fn hundredth_lap_drops_lap_1_but_numbering_keeps_counting() {
    let mut core = TimerCore::new(cfg());
    core.sw_start(0).unwrap();
    for i in 1..=100u64 {
        core.sw_lap(i * 1_000).unwrap();
    }

    let snap = core.snapshot(100_000);
    let laps = snap.stopwatch().laps();
    assert_eq!(laps.len(), 99);
    assert_eq!(laps[0].number(), 2); // lap 1 dropped
    assert_eq!(laps[0].elapsed_ms(), 2_000);
    assert_eq!(laps[98].number(), 100); // numbering kept counting
    assert_eq!(laps[98].elapsed_ms(), 100_000);
}

#[test]
fn reset_clears_elapsed_and_laps_and_numbering_restarts() {
    let mut core = TimerCore::new(cfg());
    core.sw_start(0).unwrap();
    core.sw_lap(1_000).unwrap();
    core.sw_lap(2_000).unwrap();

    core.sw_reset();
    let s = core.snapshot(0);
    assert_eq!(
        s.stopwatch().status(),
        timer_core::StopwatchStatusKind::Idle
    );
    assert_eq!(s.stopwatch().elapsed_ms(), 0);
    assert!(s.stopwatch().laps().is_empty());

    // numbering restarts at 1
    core.sw_start(0).unwrap();
    core.sw_lap(1_000).unwrap();
    let snap = core.snapshot(1_000);
    let laps = snap.stopwatch().laps();
    assert_eq!(laps.len(), 1);
    assert_eq!(laps[0].number(), 1);
}

#[test]
fn lap_records_elapsed_at_lap_time() {
    let mut core = TimerCore::new(cfg());
    core.sw_start(0).unwrap();
    core.sw_lap(3_000).unwrap();
    core.sw_pause(4_000).unwrap();
    core.sw_resume(10_000).unwrap();
    core.sw_lap(12_000).unwrap(); // accumulated 4_000 + (12_000 - 10_000)

    let snap = core.snapshot(12_000);
    let laps = snap.stopwatch().laps();
    assert_eq!(laps.len(), 2);
    assert_eq!(laps[0].number(), 1);
    assert_eq!(laps[0].elapsed_ms(), 3_000);
    assert_eq!(laps[1].number(), 2);
    assert_eq!(laps[1].elapsed_ms(), 6_000);
}

#[test]
fn backwards_clock_clamps_stopwatch() {
    let mut core = TimerCore::new(cfg());
    core.sw_start(0).unwrap();
    core.sw_pause(4_000).unwrap();
    assert_eq!(core.snapshot(4_000).stopwatch().elapsed_ms(), 4_000);

    // backwards resume clamps to 4_000
    core.sw_resume(2_000).unwrap();
    core.sw_pause(10_000).unwrap();
    assert_eq!(core.snapshot(10_000).stopwatch().elapsed_ms(), 10_000);
}

#[test]
fn stopwatch_never_underflows_at_huge_now() {
    let mut core = TimerCore::new(cfg());
    core.sw_start(0).unwrap();
    let s = core.snapshot(u64::MAX);
    assert_eq!(s.stopwatch().elapsed_ms(), u64::MAX);
    assert_eq!(
        s.stopwatch().status(),
        timer_core::StopwatchStatusKind::Running
    );

    core.sw_pause(u64::MAX).unwrap();
    assert_eq!(core.snapshot(u64::MAX).stopwatch().elapsed_ms(), u64::MAX);
}
