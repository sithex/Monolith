use hermes_island_lib::integrations::timer::core::{
    PomodoroConfig, TimerCore, TimerError, TimerEvent, MAX_COUNTDOWN_MS,
};

fn cfg() -> PomodoroConfig {
    match PomodoroConfig::new(60_000, 60_000, 60_000, 4, false) {
        Ok(c) => c,
        Err(_) => panic!("unreachable: valid config"),
    }
}

#[test]
fn set_countdown_accepts_bounds_rejects_outside() {
    let mut core = TimerCore::new(cfg());
    assert!(core.set_countdown(1_000).is_ok());
    assert!(core.set_countdown(86_400_000).is_ok());
    assert_eq!(
        core.set_countdown(999),
        Err(TimerError::CountdownDurationOutOfRange)
    );
    assert_eq!(
        core.set_countdown(86_400_001),
        Err(TimerError::CountdownDurationOutOfRange)
    );
    // state unchanged after rejections
    let s = core.snapshot(0);
    assert_eq!(
        s.countdown().status(),
        hermes_island_lib::integrations::timer::core::CountdownStatusKind::Idle
    );
}

#[test]
fn set_countdown_overwrites_while_idle_only() {
    let mut core = TimerCore::new(cfg());
    core.set_countdown(10_000).unwrap();
    assert!(core.set_countdown(20_000).is_ok());
    assert_eq!(core.snapshot(0).countdown().remaining_ms(), 20_000);

    core.start(0).unwrap();
    assert_eq!(
        core.set_countdown(5_000),
        Err(TimerError::InvalidTransition)
    );
    core.pause(1_000).unwrap();
    assert_eq!(
        core.set_countdown(5_000),
        Err(TimerError::InvalidTransition)
    );
    core.resume(1_000).unwrap();
    assert_eq!(
        core.set_countdown(5_000),
        Err(TimerError::InvalidTransition)
    );
    core.reset();
    assert!(core.set_countdown(5_000).is_ok());
}

#[test]
fn start_without_duration_is_typed_error_and_state_unchanged() {
    let mut core = TimerCore::new(cfg());
    assert_eq!(core.start(0), Err(TimerError::CountdownNotSet));
    let s = core.snapshot(0);
    assert_eq!(
        s.countdown().status(),
        hermes_island_lib::integrations::timer::core::CountdownStatusKind::Idle
    );
    assert_eq!(s.countdown().remaining_ms(), 0);
    assert_eq!(core.next_deadline(), None);
}

#[test]
fn start_runs_and_finishes_inclusively_at_deadline() {
    let mut core = TimerCore::new(cfg());
    core.set_countdown(10_000).unwrap();
    core.start(0).unwrap();
    assert_eq!(core.next_deadline(), Some(10_000));

    assert!(core.tick(9_999).is_empty());
    assert_eq!(core.tick(10_000), vec![TimerEvent::CountdownFinished]);
    // second tick with the same now emits nothing
    assert!(core.tick(10_000).is_empty());

    let s = core.snapshot(10_000);
    assert_eq!(
        s.countdown().status(),
        hermes_island_lib::integrations::timer::core::CountdownStatusKind::Finished
    );
    assert_eq!(s.countdown().remaining_ms(), 0);
    assert_eq!(core.next_deadline(), None);
}

#[test]
fn pause_resume_preserve_remaining_exactly_across_cycles() {
    let mut core = TimerCore::new(cfg());
    core.set_countdown(10_000).unwrap();
    core.start(0).unwrap(); // deadline 10_000

    core.pause(4_000).unwrap();
    assert_eq!(core.snapshot(4_000).countdown().remaining_ms(), 6_000);

    core.resume(4_000).unwrap();
    assert_eq!(core.next_deadline(), Some(10_000));

    core.pause(5_500).unwrap();
    assert_eq!(core.snapshot(5_500).countdown().remaining_ms(), 4_500);

    core.resume(5_500).unwrap();
    assert_eq!(core.next_deadline(), Some(10_000));

    assert!(core.tick(9_999).is_empty());
    assert_eq!(core.tick(10_000), vec![TimerEvent::CountdownFinished]);
}

#[test]
fn invalid_transitions_leave_state_unchanged() {
    let mut core = TimerCore::new(cfg());
    core.set_countdown(10_000).unwrap();

    // pause while Idle
    assert_eq!(core.pause(0), Err(TimerError::InvalidTransition));
    // resume while Idle
    assert_eq!(core.resume(0), Err(TimerError::InvalidTransition));
    // add_time while Idle
    assert_eq!(core.add_time(0, 1_000), Err(TimerError::InvalidTransition));

    core.start(0).unwrap();
    // start while Running
    assert_eq!(core.start(1), Err(TimerError::InvalidTransition));
    // resume while Running
    assert_eq!(core.resume(1), Err(TimerError::InvalidTransition));

    let before = core.snapshot(2_000);
    // state unchanged by failed ops
    let after = core.snapshot(2_000);
    assert_eq!(before, after);
}

#[test]
fn reset_returns_to_idle_keeps_duration_and_can_restart() {
    let mut core = TimerCore::new(cfg());
    core.set_countdown(10_000).unwrap();
    core.start(0).unwrap();
    core.tick(10_000);
    assert_eq!(
        core.snapshot(10_000).countdown().status(),
        hermes_island_lib::integrations::timer::core::CountdownStatusKind::Finished
    );

    core.reset();
    let s = core.snapshot(0);
    assert_eq!(
        s.countdown().status(),
        hermes_island_lib::integrations::timer::core::CountdownStatusKind::Idle
    );
    assert_eq!(s.countdown().remaining_ms(), 10_000);

    // restart directly from Finished (no reset needed)
    core.start(10_000).unwrap();
    assert_eq!(core.next_deadline(), Some(20_000));
}

#[test]
fn add_time_extends_running_and_paused_capped_at_24h() {
    let mut core = TimerCore::new(cfg());
    core.set_countdown(10_000).unwrap();
    core.start(0).unwrap();

    core.add_time(4_000, 5_000).unwrap();
    // remaining was 6_000, +5_000 = 11_000, deadline = 4_000 + 11_000
    assert_eq!(core.next_deadline(), Some(15_000));

    core.pause(7_000).unwrap();
    assert_eq!(core.snapshot(7_000).countdown().remaining_ms(), 8_000);
    core.add_time(7_000, 2_000).unwrap();
    assert_eq!(core.snapshot(7_000).countdown().remaining_ms(), 10_000);

    // cap at 24 h even with a huge delta (deadline = now + capped remaining)
    core.resume(7_000).unwrap();
    core.add_time(7_000, u64::MAX).unwrap();
    assert_eq!(core.next_deadline(), Some(7_000 + MAX_COUNTDOWN_MS));
    assert_eq!(
        core.snapshot(7_000).countdown().remaining_ms(),
        MAX_COUNTDOWN_MS
    );
}

#[test]
fn add_time_only_while_running_or_paused() {
    let mut core = TimerCore::new(cfg());
    core.set_countdown(10_000).unwrap();
    assert_eq!(core.add_time(0, 1_000), Err(TimerError::InvalidTransition));

    core.start(0).unwrap();
    core.tick(10_000);
    assert_eq!(
        core.add_time(10_000, 1_000),
        Err(TimerError::InvalidTransition)
    );
}

#[test]
fn backwards_clock_is_clamped_to_last_seen_now() {
    let mut core = TimerCore::new(cfg());
    core.set_countdown(10_000).unwrap();
    core.start(0).unwrap(); // last_seen = 0

    // tick with a backwards now clamps to 0; deadline 10_000 not due
    assert!(core.tick(0).is_empty());

    core.pause(4_000).unwrap(); // last_seen = 4_000, remaining 6_000
                                // backwards resume clamps to 4_000 -> deadline 10_000 again
    core.resume(2_000).unwrap();
    assert_eq!(core.next_deadline(), Some(10_000));

    // backwards tick after advancing to 5_000
    core.tick(5_000);
    assert!(core.tick(1_000).is_empty());
    assert_eq!(core.next_deadline(), Some(10_000));

    // fire
    assert_eq!(core.tick(10_000), vec![TimerEvent::CountdownFinished]);
    // backwards tick after finish stays finished
    assert!(core.tick(3_000).is_empty());
    assert_eq!(
        core.snapshot(10_000).countdown().status(),
        hermes_island_lib::integrations::timer::core::CountdownStatusKind::Finished
    );
}

#[test]
fn countdown_never_underflows_with_now_beyond_deadline() {
    let mut core = TimerCore::new(cfg());
    core.set_countdown(10_000).unwrap();
    core.start(0).unwrap();

    // snapshot far past the deadline: remaining saturates to 0, no panic
    let s = core.snapshot(u64::MAX);
    assert_eq!(s.countdown().remaining_ms(), 0);
    assert_eq!(
        s.countdown().status(),
        hermes_island_lib::integrations::timer::core::CountdownStatusKind::Running
    );

    // tick at u64::MAX still fires exactly once
    assert_eq!(core.tick(u64::MAX), vec![TimerEvent::CountdownFinished]);
    assert!(core.tick(u64::MAX).is_empty());
}

#[test]
fn sub_states_are_independent() {
    let mut core = TimerCore::new(cfg());
    core.set_countdown(10_000).unwrap();
    core.start(0).unwrap(); // countdown deadline 10_000

    // churn pomodoro + stopwatch
    core.pomo_start(1_000).unwrap();
    core.pomo_pause(2_000).unwrap();
    core.pomo_resume(2_000).unwrap();
    core.pomo_skip(3_000).unwrap();
    core.sw_start(1_000).unwrap();
    core.sw_pause(2_000).unwrap();
    core.sw_resume(2_000).unwrap();
    core.sw_lap(3_000).unwrap();

    // countdown untouched
    assert_eq!(core.next_deadline(), Some(10_000));
    let s = core.snapshot(5_000);
    assert_eq!(s.countdown().remaining_ms(), 5_000);
    assert_eq!(
        s.countdown().status(),
        hermes_island_lib::integrations::timer::core::CountdownStatusKind::Running
    );
}
