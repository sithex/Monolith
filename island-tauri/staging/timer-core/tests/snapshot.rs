use serde_json::json;
use timer_core::{
    CountdownStatusKind, Phase, PomodoroConfig, PomodoroStatusKind, StopwatchStatusKind, TimerCore,
    TimerEvent, TimerSnapshot,
};

fn cfg() -> PomodoroConfig {
    match PomodoroConfig::new(60_000, 120_000, 180_000, 4, false) {
        Ok(c) => c,
        Err(_) => panic!("unreachable: valid config"),
    }
}

/// A core with all three sub-states in non-trivial states at now = 3_000.
/// Ops use monotonic nows so the shared backwards-clock clamp never kicks in.
fn rich_core() -> TimerCore {
    let mut core = TimerCore::new(cfg());
    let _ = core.set_countdown(10_000);
    let _ = core.start(0); // last_seen 0
    let _ = core.sw_start(0); // last_seen 0
    let _ = core.sw_lap(1_000); // last_seen 1_000
    let _ = core.pomo_start(1_000); // last_seen 1_000
    let _ = core.pomo_pause(2_000); // last_seen 2_000
    let _ = core.sw_lap(2_000); // last_seen 2_000
    core
}

#[test]
fn snapshot_wires_camelcase_keys_exactly() {
    let core = rich_core();
    let v = serde_json::to_value(core.snapshot(3_000)).unwrap();
    let obj = v.as_object().unwrap();

    // top-level keys exactly
    let mut top: Vec<&str> = obj.keys().map(String::as_str).collect();
    top.sort_unstable();
    assert_eq!(top, vec!["countdown", "pomodoro", "stopwatch"]);

    let cd = obj["countdown"].as_object().unwrap();
    let mut cd_keys: Vec<&str> = cd.keys().map(String::as_str).collect();
    cd_keys.sort_unstable();
    assert_eq!(cd_keys, vec!["remainingMs", "status"]);

    let po = obj["pomodoro"].as_object().unwrap();
    let mut po_keys: Vec<&str> = po.keys().map(String::as_str).collect();
    po_keys.sort_unstable();
    assert_eq!(po_keys, vec!["phase", "remainingMs", "round", "status"]);

    let sw = obj["stopwatch"].as_object().unwrap();
    let mut sw_keys: Vec<&str> = sw.keys().map(String::as_str).collect();
    sw_keys.sort_unstable();
    assert_eq!(sw_keys, vec!["elapsedMs", "laps", "status"]);

    let laps = sw["laps"].as_array().unwrap();
    assert_eq!(laps.len(), 2);
    let lap0 = laps[0].as_object().unwrap();
    let mut lap_keys: Vec<&str> = lap0.keys().map(String::as_str).collect();
    lap_keys.sort_unstable();
    assert_eq!(lap_keys, vec!["elapsedMs", "number"]);

    // values
    assert_eq!(cd["status"], json!("running"));
    assert_eq!(cd["remainingMs"], json!(7_000));
    assert_eq!(po["status"], json!("paused"));
    assert_eq!(po["phase"], json!("focus"));
    assert_eq!(po["remainingMs"], json!(59_000));
    assert_eq!(po["round"], json!(0));
    assert_eq!(sw["status"], json!("running"));
    assert_eq!(sw["elapsedMs"], json!(3_000));
    assert_eq!(laps[0]["number"], json!(1));
    assert_eq!(laps[0]["elapsedMs"], json!(1_000));
    assert_eq!(laps[1]["number"], json!(2));
    assert_eq!(laps[1]["elapsedMs"], json!(2_000));
}

#[test]
fn snapshot_serde_roundtrip_preserves_value() {
    let core = rich_core();
    let snap = core.snapshot(3_000);
    let json = serde_json::to_string(&snap).unwrap();
    let back: TimerSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snap);

    // specific values survive
    assert_eq!(back.countdown().status(), CountdownStatusKind::Running);
    assert_eq!(back.countdown().remaining_ms(), 7_000);
    assert_eq!(back.pomodoro().status(), PomodoroStatusKind::Paused);
    assert_eq!(back.pomodoro().phase(), Phase::Focus);
    assert_eq!(back.pomodoro().remaining_ms(), 59_000);
    assert_eq!(back.pomodoro().round(), 0);
    assert_eq!(back.stopwatch().status(), StopwatchStatusKind::Running);
    assert_eq!(back.stopwatch().elapsed_ms(), 3_000);
    assert_eq!(back.stopwatch().laps().len(), 2);
}

#[test]
fn snake_case_keys_are_ignored_no_aliases() {
    // snake_case keys must NOT populate camelCase fields
    let back: TimerSnapshot = serde_json::from_value(json!({
        "countdown": { "status": "idle", "remaining_ms": 5 },
        "pomodoro": { "status": "idle", "phase": "focus", "remaining_ms": 7, "round": 2 },
        "stopwatch": { "status": "idle", "elapsed_ms": 9, "laps": [] },
    }))
    .unwrap();

    let s = &back;
    assert_eq!(s.countdown().remaining_ms(), 0, "remaining_ms ignored");
    assert_eq!(
        s.pomodoro().remaining_ms(),
        0,
        "pomodoro remaining_ms ignored"
    );
    assert_eq!(s.stopwatch().elapsed_ms(), 0, "elapsed_ms ignored");
    assert!(s.stopwatch().laps().is_empty());
}

#[test]
fn missing_keys_fall_back_to_defaults() {
    let back: TimerSnapshot = serde_json::from_value(json!({})).unwrap();
    assert_eq!(back, TimerSnapshot::default());
    assert_eq!(back.countdown().status(), CountdownStatusKind::Idle);
    assert_eq!(back.pomodoro().status(), PomodoroStatusKind::Idle);
    assert_eq!(back.pomodoro().phase(), Phase::Focus);
    assert_eq!(back.stopwatch().status(), StopwatchStatusKind::Idle);
}

#[test]
fn event_serde_uses_camelcase_completed_focus() {
    let ev = TimerEvent::PomodoroPhaseEnded {
        ended: Phase::Focus,
        next: Phase::ShortBreak,
        completed_focus: 3,
    };
    let v = serde_json::to_value(ev).unwrap();
    let obj = v.as_object().unwrap();
    assert!(obj.contains_key("PomodoroPhaseEnded"));
    let inner = obj["PomodoroPhaseEnded"].as_object().unwrap();
    assert!(inner.contains_key("completedFocus"));
    assert!(inner.contains_key("ended"));
    assert!(inner.contains_key("next"));
    assert_eq!(inner["completedFocus"], json!(3));
    assert_eq!(inner["ended"], json!("focus"));
    assert_eq!(inner["next"], json!("shortBreak"));

    let back: TimerEvent = serde_json::from_value(v).unwrap();
    assert_eq!(back, ev);
}

#[test]
fn snapshot_does_not_fire_events_or_change_state() {
    let mut core = TimerCore::new(cfg());
    core.set_countdown(10_000).unwrap();
    core.start(0).unwrap();

    // snapshot far past the deadline: still Running, remaining saturates
    let s = core.snapshot(12_000);
    assert_eq!(s.countdown().status(), CountdownStatusKind::Running);
    assert_eq!(s.countdown().remaining_ms(), 0);

    // the tick afterwards still fires exactly once
    assert_eq!(core.tick(12_000), vec![TimerEvent::CountdownFinished]);
    assert!(core.tick(12_000).is_empty());
}

#[test]
fn snapshot_does_not_advance_the_cores_clock() {
    // pomodoro auto-chains; a pure snapshot at a far-away now must not move
    // last_seen_now, otherwise a later lower tick would over-process.
    let cfg = PomodoroConfig::new(60_000, 60_000, 60_000, 4, true).unwrap();
    let mut core = TimerCore::new(cfg);
    core.pomo_start(0).unwrap(); // Focus deadline 60_000, last_seen = 0

    let s = core.snapshot(120_000); // pure read at a future now
    assert_eq!(s.pomodoro().remaining_ms(), 0);
    assert_eq!(s.pomodoro().phase(), Phase::Focus);

    // a later tick at 90_000 must only see the Focus deadline 60_000 fire,
    // i.e. exactly one event and the short break chained from 60_000.
    assert_eq!(
        core.tick(90_000),
        vec![TimerEvent::PomodoroPhaseEnded {
            ended: Phase::Focus,
            next: Phase::ShortBreak,
            completed_focus: 1,
        }]
    );
    assert_eq!(core.next_deadline(), Some(120_000));
}

#[test]
fn next_deadline_reflects_earliest_pending_deadline() {
    let mut core = TimerCore::new(cfg());
    assert_eq!(core.next_deadline(), None);

    core.set_countdown(10_000).unwrap();
    core.start(0).unwrap();
    assert_eq!(core.next_deadline(), Some(10_000));

    core.pomo_start(0).unwrap(); // focus deadline 60_000
    assert_eq!(core.next_deadline(), Some(10_000));

    core.tick(10_000); // countdown finishes
    assert_eq!(core.next_deadline(), Some(60_000));

    core.pomo_pause(20_000).unwrap(); // nothing running anymore
    assert_eq!(core.next_deadline(), None);

    core.pomo_resume(20_000).unwrap();
    assert_eq!(core.next_deadline(), Some(60_000));

    core.pomo_reset();
    core.reset(); // countdown is Finished; back to Idle before setting again
    core.set_countdown(5_000).unwrap();
    core.start(40_000).unwrap();
    assert_eq!(core.next_deadline(), Some(45_000));
}
