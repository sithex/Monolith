use hermes_island_lib::integrations::timer::core::{
    Phase, PomodoroConfig, TimerCore, TimerError, TimerEvent,
};

#[test]
fn config_validation_bounds() {
    // durations: 1 min ok, 4 h ok, outside rejected
    assert!(PomodoroConfig::new(60_000, 60_000, 60_000, 4, false).is_ok());
    assert!(PomodoroConfig::new(14_400_000, 60_000, 60_000, 4, false).is_ok());
    assert_eq!(
        PomodoroConfig::new(59_999, 60_000, 60_000, 4, false),
        Err(TimerError::PomodoroDurationOutOfRange)
    );
    assert_eq!(
        PomodoroConfig::new(60_000, 14_400_001, 60_000, 4, false),
        Err(TimerError::PomodoroDurationOutOfRange)
    );
    assert_eq!(
        PomodoroConfig::new(60_000, 60_000, 0, 4, false),
        Err(TimerError::PomodoroDurationOutOfRange)
    );
    // rounds: 1 ok, 12 ok, 0 / 13 rejected
    assert!(PomodoroConfig::new(60_000, 60_000, 60_000, 1, false).is_ok());
    assert!(PomodoroConfig::new(60_000, 60_000, 60_000, 12, false).is_ok());
    assert_eq!(
        PomodoroConfig::new(60_000, 60_000, 60_000, 0, false),
        Err(TimerError::PomodoroRoundsOutOfRange)
    );
    assert_eq!(
        PomodoroConfig::new(60_000, 60_000, 60_000, 13, false),
        Err(TimerError::PomodoroRoundsOutOfRange)
    );
}

#[test]
fn phases_cycle_with_long_break_after_4th_completed_focus() {
    let cfg = PomodoroConfig::new(60_000, 60_000, 60_000, 4, true).unwrap();
    let mut core = TimerCore::new(cfg);

    core.pomo_start(0).unwrap(); // Focus, deadline 60_000
    assert_eq!(core.next_deadline(), Some(60_000));

    assert_eq!(
        core.tick(60_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::Focus,
            next: Phase::ShortBreak,
            completed_focus: 1,
        }]
    );
    assert_eq!(core.completed_focus_count(), 1);
    assert_eq!(core.snapshot(60_000).pomodoro().round(), 1);
    assert_eq!(core.next_deadline(), Some(120_000));

    assert_eq!(
        core.tick(120_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::ShortBreak,
            next: Phase::Focus,
            completed_focus: 1,
        }]
    );
    assert_eq!(
        core.tick(180_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::Focus,
            next: Phase::ShortBreak,
            completed_focus: 2,
        }]
    );
    assert_eq!(
        core.tick(240_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::ShortBreak,
            next: Phase::Focus,
            completed_focus: 2,
        }]
    );
    assert_eq!(
        core.tick(300_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::Focus,
            next: Phase::ShortBreak,
            completed_focus: 3,
        }]
    );
    assert_eq!(
        core.tick(360_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::ShortBreak,
            next: Phase::Focus,
            completed_focus: 3,
        }]
    );

    // 4th completed focus -> LongBreak
    assert_eq!(
        core.tick(420_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::Focus,
            next: Phase::LongBreak,
            completed_focus: 4,
        }]
    );
    assert_eq!(core.completed_focus_count(), 4);
    assert_eq!(core.snapshot(420_000).pomodoro().round(), 0);
    let s = core.snapshot(450_000);
    assert_eq!(s.pomodoro().phase(), Phase::LongBreak);
    assert_eq!(
        s.pomodoro().status(),
        hermes_island_lib::integrations::timer::core::PomodoroStatusKind::Running
    );
    assert_eq!(s.pomodoro().remaining_ms(), 30_000);
    assert_eq!(core.next_deadline(), Some(480_000));

    // LongBreak -> Focus, cycle restarts
    assert_eq!(
        core.tick(480_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::LongBreak,
            next: Phase::Focus,
            completed_focus: 4,
        }]
    );
    assert_eq!(core.snapshot(480_000).pomodoro().round(), 0);
    assert_eq!(core.next_deadline(), Some(540_000));
}

#[test]
fn skip_ends_phase_without_counting_focus() {
    let cfg = PomodoroConfig::new(60_000, 60_000, 60_000, 4, false).unwrap();
    let mut core = TimerCore::new(cfg);

    core.pomo_start(0).unwrap();
    assert_eq!(core.completed_focus_count(), 0);

    // skip the focus: not counted, next is ShortBreak armed paused at full length
    core.pomo_skip(10_000).unwrap();
    assert_eq!(core.completed_focus_count(), 0);
    let s = core.snapshot(10_000);
    assert_eq!(
        s.pomodoro().status(),
        hermes_island_lib::integrations::timer::core::PomodoroStatusKind::Paused
    );
    assert_eq!(s.pomodoro().phase(), Phase::ShortBreak);
    assert_eq!(s.pomodoro().remaining_ms(), 60_000);
    assert_eq!(s.pomodoro().round(), 0);
    assert_eq!(core.next_deadline(), None);

    // start the break; completing it goes back to Focus, still no count
    core.pomo_start(10_000).unwrap();
    assert_eq!(
        core.tick(70_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::ShortBreak,
            next: Phase::Focus,
            completed_focus: 0,
        }]
    );
    assert_eq!(core.completed_focus_count(), 0);

    // skip the (paused) Focus -> ShortBreak, still no count
    core.pomo_skip(70_000).unwrap();
    let s = core.snapshot(70_000);
    assert_eq!(s.pomodoro().phase(), Phase::ShortBreak);
    assert_eq!(s.pomodoro().remaining_ms(), 60_000);
    assert_eq!(
        s.pomodoro().status(),
        hermes_island_lib::integrations::timer::core::PomodoroStatusKind::Paused
    );
    assert_eq!(core.completed_focus_count(), 0);

    // skip the (paused) ShortBreak -> Focus, still no count
    core.pomo_skip(70_000).unwrap();
    let s = core.snapshot(70_000);
    assert_eq!(s.pomodoro().phase(), Phase::Focus);
    assert_eq!(s.pomodoro().remaining_ms(), 60_000);
    assert_eq!(
        s.pomodoro().status(),
        hermes_island_lib::integrations::timer::core::PomodoroStatusKind::Paused
    );
    assert_eq!(core.completed_focus_count(), 0);
}

#[test]
fn skip_while_idle_is_invalid() {
    let mut core = TimerCore::new(PomodoroConfig::new(60_000, 60_000, 60_000, 4, false).unwrap());
    assert_eq!(core.pomo_skip(0), Err(TimerError::InvalidTransition));
}

#[test]
fn auto_start_next_false_arms_paused_at_full_length() {
    let cfg = PomodoroConfig::new(60_000, 120_000, 180_000, 4, false).unwrap();
    let mut core = TimerCore::new(cfg);

    core.pomo_start(0).unwrap();
    assert_eq!(
        core.tick(60_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::Focus,
            next: Phase::ShortBreak,
            completed_focus: 1,
        }]
    );

    // armed paused at FULL short-break length, waits for Start
    let s = core.snapshot(60_000);
    assert_eq!(
        s.pomodoro().status(),
        hermes_island_lib::integrations::timer::core::PomodoroStatusKind::Paused
    );
    assert_eq!(s.pomodoro().phase(), Phase::ShortBreak);
    assert_eq!(s.pomodoro().remaining_ms(), 120_000);
    assert_eq!(core.next_deadline(), None);

    // nothing fires while paused
    assert!(core.tick(100_000).is_empty());

    core.pomo_start(100_000).unwrap();
    assert_eq!(core.next_deadline(), Some(220_000));
    assert_eq!(
        core.tick(220_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::ShortBreak,
            next: Phase::Focus,
            completed_focus: 1,
        }]
    );
    let s = core.snapshot(220_000);
    assert_eq!(s.pomodoro().phase(), Phase::Focus);
    assert_eq!(s.pomodoro().remaining_ms(), 60_000);
    assert_eq!(
        s.pomodoro().status(),
        hermes_island_lib::integrations::timer::core::PomodoroStatusKind::Paused
    );
}

#[test]
fn auto_start_next_chains_from_deadline_not_now() {
    let cfg = PomodoroConfig::new(60_000, 60_000, 60_000, 4, true).unwrap();
    let mut core = TimerCore::new(cfg);

    core.pomo_start(0).unwrap(); // Focus deadline 60_000
                                 // late tick at 90_000: Focus ended at its 60_000 deadline, so the short
                                 // break started at 60_000 -> deadline 120_000 (NOT 90_000 + 60_000)
    assert_eq!(
        core.tick(90_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::Focus,
            next: Phase::ShortBreak,
            completed_focus: 1,
        }]
    );
    assert_eq!(core.next_deadline(), Some(120_000));
    let s = core.snapshot(90_000);
    assert_eq!(s.pomodoro().phase(), Phase::ShortBreak);
    assert_eq!(s.pomodoro().remaining_ms(), 30_000);
}

#[test]
fn several_phases_elapse_in_one_late_tick() {
    let cfg = PomodoroConfig::new(60_000, 60_000, 60_000, 4, true).unwrap();
    let mut core = TimerCore::new(cfg);

    core.pomo_start(0).unwrap();
    // deadlines 60k, 120k, 180k all <= 180k: three phases end in one tick
    assert_eq!(
        core.tick(180_000),
        vec![
            TimerEvent::PomodoroPhaseEnded {
                ended: Phase::Focus,
                next: Phase::ShortBreak,
                completed_focus: 1,
            },
            TimerEvent::PomodoroPhaseEnded {
                ended: Phase::ShortBreak,
                next: Phase::Focus,
                completed_focus: 1,
            },
            TimerEvent::PomodoroPhaseEnded {
                ended: Phase::Focus,
                next: Phase::ShortBreak,
                completed_focus: 2,
            },
        ]
    );
    assert_eq!(core.next_deadline(), Some(240_000));
    // draining tick emits nothing
    assert!(core.tick(180_000).is_empty());
}

#[test]
fn hundred_event_cap_spills_to_next_tick() {
    let cfg = PomodoroConfig::new(60_000, 60_000, 60_000, 12, true).unwrap();
    let mut core = TimerCore::new(cfg);

    core.pomo_start(0).unwrap();
    // 105 one-minute phases have deadlines <= 105 * 60_000; the cap stops at
    // 100 events (phases 1..=100 end: 50 focuses among odd indices 1..99).
    let first = core.tick(6_300_000);
    assert_eq!(first.len(), 100);
    assert_eq!(core.completed_focus_count(), 50);

    // leftovers spill into the next tick with the same now
    let second = core.tick(6_300_000);
    assert_eq!(second.len(), 5);

    let total = first.len() + second.len();
    assert_eq!(total, 105);

    // phases 101..=105 add 3 more focuses (odd 101, 103, 105)
    assert_eq!(core.completed_focus_count(), 53);

    // now the chain is caught up: phase 106 ends at 6_360_000
    assert_eq!(core.next_deadline(), Some(6_360_000));
    assert!(core.tick(6_300_000).is_empty());
}

#[test]
fn pause_resume_preserve_phase_remaining() {
    let cfg = PomodoroConfig::new(60_000, 60_000, 60_000, 4, true).unwrap();
    let mut core = TimerCore::new(cfg);

    core.pomo_start(0).unwrap();
    core.pomo_pause(10_000).unwrap();
    let s = core.snapshot(10_000);
    assert_eq!(s.pomodoro().remaining_ms(), 50_000);
    assert_eq!(
        s.pomodoro().status(),
        hermes_island_lib::integrations::timer::core::PomodoroStatusKind::Paused
    );
    assert_eq!(core.next_deadline(), None);

    core.pomo_resume(10_000).unwrap();
    assert_eq!(core.next_deadline(), Some(60_000));

    core.pomo_pause(25_000).unwrap();
    assert_eq!(core.snapshot(25_000).pomodoro().remaining_ms(), 35_000);
    core.pomo_resume(25_000).unwrap();
    assert_eq!(core.next_deadline(), Some(60_000));

    // start while running is invalid
    assert_eq!(core.pomo_start(30_000), Err(TimerError::InvalidTransition));
    // pause while paused is invalid
    core.pomo_pause(30_000).unwrap();
    assert_eq!(core.pomo_pause(31_000), Err(TimerError::InvalidTransition));
    // resume while running is invalid
    core.pomo_resume(31_000).unwrap();
    assert_eq!(core.pomo_resume(32_000), Err(TimerError::InvalidTransition));
}

#[test]
fn reset_clears_pomodoro_progress() {
    let cfg = PomodoroConfig::new(60_000, 60_000, 60_000, 4, true).unwrap();
    let mut core = TimerCore::new(cfg);

    core.pomo_start(0).unwrap();
    core.tick(60_000);
    core.tick(120_000);
    core.pomo_skip(130_000).unwrap();
    assert_eq!(core.completed_focus_count(), 1);

    core.pomo_reset();
    let s = core.snapshot(0);
    assert_eq!(
        s.pomodoro().status(),
        hermes_island_lib::integrations::timer::core::PomodoroStatusKind::Idle
    );
    assert_eq!(s.pomodoro().phase(), Phase::Focus);
    assert_eq!(s.pomodoro().round(), 0);
    assert_eq!(core.completed_focus_count(), 0);
    assert_eq!(core.next_deadline(), None);

    // restart from a now at/after the last seen (130_000)
    core.pomo_start(130_000).unwrap();
    assert_eq!(core.next_deadline(), Some(190_000));
}
