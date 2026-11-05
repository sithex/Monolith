use hermes_island_lib::integrations::agents::core::{
    classify, label_for, short_id, Chip, ConfigError, Event, RunState, SessionRow, Tracker,
    TrackerConfig, WarningKind, MAX_MAX_CHIPS, MAX_STALE_AFTER_SECS, MIN_MAX_CHIPS,
};

fn row(id: &str) -> SessionRow {
    SessionRow {
        id: id.to_string(),
        source: "local".to_string(),
        parent_session_id: None,
        model: None,
        billing_provider: None,
        started_at: 9_500.0, // running at the default `now` of 10_000
        ended_at: None,
        end_reason: None,
        last_activity_at: None,
        api_call_count: None,
        title: None,
    }
}

fn feq(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn cfg(stale: u64, lookback: u64, max_chips: u32) -> TrackerConfig {
    match TrackerConfig::try_new(
        stale,
        lookback,
        max_chips,
        vec!["oneshot".to_string(), "subagent".to_string()],
    ) {
        Ok(c) => c,
        Err(e) => panic!("cfg construction failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// classify
// ---------------------------------------------------------------------------

#[test]
fn classify_running_within_stale() {
    let mut r = row("a");
    r.started_at = 9_500.0;
    r.last_activity_at = Some(9_550.0);
    assert_eq!(
        classify(&r, 10_000.0, &TrackerConfig::default()),
        RunState::Running
    );
}

#[test]
fn classify_running_falls_back_to_started_at() {
    let mut r = row("a");
    r.started_at = 9_550.0;
    assert_eq!(
        classify(&r, 10_000.0, &TrackerConfig::default()),
        RunState::Running
    );
}

#[test]
fn classify_stale_after_boundary() {
    let cfg = TrackerConfig::default(); // staleAfter 900
    let mut r = row("a");
    r.started_at = 10_000.0;
    // exactly at the boundary: still running
    r.last_activity_at = Some(9_100.0);
    assert_eq!(classify(&r, 10_000.0, &cfg), RunState::Running);
    // one second older: stale
    r.last_activity_at = Some(9_099.0);
    assert_eq!(
        classify(&r, 10_000.0, &cfg),
        RunState::Stale { clock_skew: false }
    );
}

#[test]
fn classify_finished_when_ended_at_set() {
    let mut r = row("a");
    r.ended_at = Some(11_000.0);
    r.end_reason = Some("cli_close".to_string());
    assert_eq!(
        classify(&r, 10_000.0, &TrackerConfig::default()),
        RunState::Finished {
            reason: Some("cli_close".to_string())
        }
    );
    r.end_reason = None;
    assert_eq!(
        classify(&r, 10_000.0, &TrackerConfig::default()),
        RunState::Finished { reason: None }
    );
}

#[test]
fn classify_future_skew_boundary() {
    let cfg = TrackerConfig::default();
    let mut r = row("a");
    r.started_at = 10_300.0; // now + 300 exactly: still Running
    assert_eq!(classify(&r, 10_000.0, &cfg), RunState::Running);
    r.started_at = 10_300.5; // now + 300.5: stale (clock skew)
    assert_eq!(
        classify(&r, 10_000.0, &cfg),
        RunState::Stale { clock_skew: true }
    );
}

#[test]
fn classify_future_activity_skew() {
    let cfg = TrackerConfig::default();
    let mut r = row("a");
    r.started_at = 9_000.0;
    r.last_activity_at = Some(10_400.0); // >300s ahead
    assert_eq!(
        classify(&r, 10_000.0, &cfg),
        RunState::Stale { clock_skew: true }
    );
}

#[test]
fn classify_invalid_times_rejected() {
    let cfg = TrackerConfig::default();
    let now = 10_000.0;
    let cases = [
        ("nan-start", f64::NAN, None, None),
        ("inf-start", f64::INFINITY, None, None),
        ("ninf-start", f64::NEG_INFINITY, None, None),
        ("neg-start", -1.0, None, None),
        ("nan-activity", 9_900.0, Some(f64::NAN), None),
        ("inf-activity", 9_900.0, Some(f64::INFINITY), None),
        ("neg-activity", 9_900.0, Some(-3.0), None),
        ("nan-ended", 9_900.0, None, Some(f64::NAN)),
        ("inf-ended", 9_900.0, None, Some(f64::INFINITY)),
        ("neg-ended", 9_900.0, None, Some(-2.0)),
    ];
    for (name, started, activity, ended) in cases {
        let mut r = row(name);
        r.started_at = started;
        r.last_activity_at = activity;
        r.ended_at = ended;
        match classify(&r, now, &cfg) {
            RunState::Rejected { reason } => assert!(!reason.is_empty(), "{name}: empty reason"),
            other => panic!("{name}: expected Rejected, got {other:?}"),
        }
    }
}

#[test]
fn classify_reject_precedence_over_finished() {
    let mut r = row("a");
    r.started_at = f64::NAN;
    r.ended_at = Some(11_000.0);
    assert!(matches!(
        classify(&r, 10_000.0, &TrackerConfig::default()),
        RunState::Rejected { .. }
    ));
}

#[test]
fn classify_zero_times_valid() {
    let cfg = TrackerConfig::default();
    let mut r = row("a");
    r.started_at = 0.0;
    assert_eq!(classify(&r, 0.0, &cfg), RunState::Running);
}

// ---------------------------------------------------------------------------
// config
// ---------------------------------------------------------------------------

#[test]
fn config_defaults() {
    let c = TrackerConfig::default();
    assert_eq!(c.stale_after_secs(), 900);
    assert_eq!(c.lookback_secs(), 86_400);
    assert_eq!(c.max_chips(), 6);
    assert_eq!(c.delegated_sources(), &["oneshot", "subagent"]);
}

#[test]
fn config_try_new_valid_bounds() {
    assert!(TrackerConfig::try_new(60, 1, 1, vec!["x".to_string()]).is_ok());
    assert!(TrackerConfig::try_new(86_400, 1, 32, vec!["x".to_string()]).is_ok());
    assert!(TrackerConfig::try_new(900, 86_400, 6, Vec::new()).is_ok());
}

#[test]
fn config_try_new_rejects() {
    assert_eq!(
        TrackerConfig::try_new(59, 86_400, 6, vec!["x".to_string()]),
        Err(ConfigError::StaleAfterSecsOutOfRange)
    );
    assert_eq!(
        TrackerConfig::try_new(86_401, 86_400, 6, vec!["x".to_string()]),
        Err(ConfigError::StaleAfterSecsOutOfRange)
    );
    assert_eq!(
        TrackerConfig::try_new(900, 0, 6, vec!["x".to_string()]),
        Err(ConfigError::LookbackSecsZero)
    );
    assert_eq!(
        TrackerConfig::try_new(900, 86_400, 0, vec!["x".to_string()]),
        Err(ConfigError::MaxChipsOutOfRange)
    );
    assert_eq!(
        TrackerConfig::try_new(900, 86_400, 33, vec!["x".to_string()]),
        Err(ConfigError::MaxChipsOutOfRange)
    );
    assert_eq!(
        TrackerConfig::try_new(900, 86_400, 6, vec!["".to_string()]),
        Err(ConfigError::EmptyDelegatedSource)
    );
    assert_eq!(MIN_MAX_CHIPS, 1);
    assert_eq!(MAX_MAX_CHIPS, 32);
    assert_eq!(MAX_STALE_AFTER_SECS, 86_400);
}

// ---------------------------------------------------------------------------
// labels
// ---------------------------------------------------------------------------

#[test]
fn label_precedence() {
    let base = row("local-abcdefghij");

    let mut r = base.clone();
    r.title = Some("Hello".to_string());
    assert_eq!(label_for(&r), "Hello");

    r.title = Some(String::new()); // empty title falls through
    assert_eq!(label_for(&r), "abcdefgh"); // short id of local-abcdefghij

    r.title = None;
    r.model = Some("gpt-4o".to_string());
    assert_eq!(label_for(&r), "gpt-4o");

    r.model = Some(String::new());
    assert_eq!(label_for(&r), "abcdefgh");
}

#[test]
fn short_id_rules() {
    assert_eq!(short_id("local-abcdefghij"), "abcdefgh");
    assert_eq!(short_id("abcdefghij"), "abcdefgh"); // no prefix: first 8 of full id
    assert_eq!(short_id("local-abc"), "abc"); // shorter than 8
    assert_eq!(short_id("local-"), ""); // empty after prefix
    assert_eq!(short_id(""), "");
    // char-boundary safe (multibyte)
    assert_eq!(short_id("local-héllo😀world"), "héllo😀wo");
}

// ---------------------------------------------------------------------------
// update: lifecycle events
// ---------------------------------------------------------------------------

#[test]
fn update_first_update_emits_no_history_events() {
    // A finished row never seen Running must not emit Finished.
    let mut fin = row("fin");
    fin.ended_at = Some(2_000.0);
    fin.end_reason = Some("tui_close".to_string());
    // A stale row never seen Running must not emit Stalled.
    let mut stale = row("stale");
    stale.last_activity_at = Some(500.0);

    let mut tr = Tracker::new();
    let upd = tr.update(vec![fin, stale], 10_000.0);
    assert!(upd.events.is_empty(), "events: {:?}", upd.events);
    assert!(upd.chips.is_empty());
}

#[test]
fn update_started_on_first_sight_only() {
    let mut tr = Tracker::new();
    let r = row("a");
    let upd = tr.update(vec![r.clone()], 10_000.0);
    assert_eq!(
        upd.events,
        vec![Event::Started {
            id: "a".to_string()
        }]
    );
    // second update: same row still running, no new event
    let upd2 = tr.update(vec![r], 10_010.0);
    assert!(upd2.events.is_empty(), "events: {:?}", upd2.events);
}

#[test]
fn update_finish_transition() {
    let mut tr = Tracker::new();
    let r = row("a");
    let _ = tr.update(vec![r.clone()], 10_000.0);

    let mut fin = r;
    fin.ended_at = Some(10_600.0);
    fin.end_reason = Some("cli_close".to_string());
    let upd = tr.update(vec![fin], 11_000.0);
    assert_eq!(
        upd.events,
        vec![Event::Finished {
            id: "a".to_string(),
            reason: Some("cli_close".to_string()),
            elapsed_secs: 1_100.0,
        }]
    );
    assert!(upd.chips.is_empty());
}

#[test]
fn update_stall_transition() {
    let mut tr = Tracker::new();
    let mut r = row("a");
    r.started_at = 1_000.0;
    r.last_activity_at = Some(1_050.0);
    let _ = tr.update(vec![r.clone()], 1_100.0); // running

    let upd = tr.update(vec![r], 3_000.0); // activity now >900s old
    assert_eq!(
        upd.events,
        vec![Event::Stalled {
            id: "a".to_string()
        }]
    );
    assert!(upd.chips.is_empty());
}

#[test]
fn update_disappear_transition() {
    let mut tr = Tracker::new();
    let r = row("a");
    let _ = tr.update(vec![r], 10_000.0);
    let upd = tr.update(Vec::new(), 10_100.0);
    assert_eq!(
        upd.events,
        vec![Event::Disappeared {
            id: "a".to_string()
        }]
    );
}

#[test]
fn update_finish_not_also_disappeared() {
    let mut tr = Tracker::new();
    let r = row("a");
    let _ = tr.update(vec![r.clone()], 10_000.0);
    let mut fin = r;
    fin.ended_at = Some(10_500.0);
    let upd = tr.update(vec![fin], 11_000.0);
    assert_eq!(upd.events.len(), 1);
    assert!(matches!(&upd.events[0], Event::Finished { id, .. } if id == "a"));
}

#[test]
fn update_no_double_events_across_updates() {
    let mut tr = Tracker::new();
    let r = row("a");
    let u1 = tr.update(vec![r.clone()], 10_000.0);
    let u2 = tr.update(vec![r.clone()], 10_100.0);
    let u3 = tr.update(vec![r], 10_200.0);
    let total: Vec<Event> = u1
        .events
        .iter()
        .chain(&u2.events)
        .chain(&u3.events)
        .cloned()
        .collect();
    assert_eq!(
        total
            .iter()
            .filter(|e| matches!(e, Event::Started { .. }))
            .count(),
        1
    );
}

// ---------------------------------------------------------------------------
// update: lookback, duplicates
// ---------------------------------------------------------------------------

#[test]
fn update_lookback_ignores_old_rows() {
    let cfg = cfg(900, 86_400, 6);
    let mut tr = Tracker::with_config(cfg);
    let mut r = row("old");
    r.started_at = 10_000.0 - 100_000.0;
    r.last_activity_at = Some(10_000.0); // would be Running if not filtered
    let upd = tr.update(vec![r], 10_000.0);
    assert!(upd.chips.is_empty());
    assert!(upd.events.is_empty());
}

#[test]
fn update_lookback_boundary_inclusive() {
    let cfg = cfg(900, 86_400, 6);
    let mut tr = Tracker::with_config(cfg);
    let now = 200_000.0;
    let mut r = row("edge");
    r.started_at = now - 86_400.0; // exactly at cutoff: kept
    r.last_activity_at = Some(now);
    let upd = tr.update(vec![r], now);
    assert_eq!(upd.chips.len(), 1);
    assert_eq!(upd.chips[0].id, "edge");
}

#[test]
fn update_duplicate_keeps_latest_activity() {
    let mut tr = Tracker::new();
    let mut a = row("a");
    a.started_at = 9_000.0;
    a.api_call_count = Some(1);
    a.last_activity_at = Some(9_100.0);
    let mut b = a.clone();
    b.api_call_count = Some(2);
    b.last_activity_at = Some(9_800.0);

    let upd = tr.update(vec![a, b], 10_000.0);
    assert_eq!(upd.chips.len(), 1);
    assert_eq!(upd.chips[0].api_call_count, Some(2));
    assert_eq!(
        upd.events,
        vec![Event::Started {
            id: "a".to_string()
        }]
    );
    assert_eq!(
        tr.take_warnings(),
        vec![WarningKind::DuplicateId {
            id: "a".to_string()
        }]
    );
}

#[test]
fn update_duplicate_tie_keeps_later_started() {
    let mut tr = Tracker::new();
    let mut a = row("a");
    a.started_at = 4_000.0; // older
    a.last_activity_at = None;
    let mut b = row("a");
    b.started_at = 8_000.0; // newer
    b.last_activity_at = None;

    let upd = tr.update(vec![a, b], 8_500.0);
    assert_eq!(upd.chips.len(), 1);
    assert!(feq(upd.chips[0].elapsed_secs, 500.0)); // startedAt 8000 kept
}

#[test]
fn update_first_seen_duplicate_warns_once() {
    let mut tr = Tracker::new();
    let a = row("a");
    let dup = a.clone();
    let _ = tr.update(vec![a.clone(), dup, a], 10_000.0);
    let warns = tr.take_warnings();
    assert_eq!(
        warns
            .iter()
            .filter(|w| matches!(w, WarningKind::DuplicateId { .. }))
            .count(),
        1
    );
}

// ---------------------------------------------------------------------------
// update: chips
// ---------------------------------------------------------------------------

fn running_row(id: &str, source: &str, started: f64, activity: Option<f64>) -> SessionRow {
    let mut r = row(id);
    r.source = source.to_string();
    r.started_at = started;
    r.last_activity_at = activity;
    r
}

#[test]
fn update_chips_cap_and_overflow() {
    let cfg = cfg(900, 86_400, 3);
    let mut tr = Tracker::with_config(cfg);
    let mut rows = Vec::new();
    for i in 0..12 {
        rows.push(running_row(
            &format!("r{i}"),
            "local",
            9_000.0 + i as f64,
            Some(9_900.0),
        ));
    }
    let upd = tr.update(rows, 10_000.0);
    assert_eq!(upd.chips.len(), 3);
    assert_eq!(upd.overflow, 9);
    // newest three kept, newest first
    assert_eq!(upd.chips[0].id, "r11");
    assert_eq!(upd.chips[2].id, "r9");
}

#[test]
fn update_chips_sorted_newest_first() {
    let mut tr = Tracker::new();
    let rows = vec![
        running_row("old", "local", 8_000.0, Some(9_800.0)),
        running_row("mid", "local", 9_000.0, Some(9_900.0)),
        running_row("new", "local", 9_500.0, Some(9_900.0)),
    ];
    let upd = tr.update(rows, 10_000.0);
    let ids: Vec<&str> = upd.chips.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["new", "mid", "old"]);
}

#[test]
fn update_delegated_flag() {
    let mut tr = Tracker::new();
    let rows = vec![
        running_row("d", "oneshot", 9_000.0, Some(9_900.0)),
        running_row("t", "local", 9_100.0, Some(9_900.0)),
        running_row("s", "subagent", 9_200.0, Some(9_900.0)),
        running_row("c", "cli", 9_300.0, Some(9_900.0)),
        running_row("x", "desktop", 9_400.0, Some(9_900.0)),
        running_row("y", "tui", 9_500.0, Some(9_900.0)),
    ];
    let upd = tr.update(rows, 10_000.0);
    assert_eq!(upd.chips.len(), 6);
    for chip in &upd.chips {
        let expected_delegated = chip.id == "d" || chip.id == "s";
        assert_eq!(chip.is_delegated, expected_delegated, "id {}", chip.id);
    }
}

#[test]
fn update_subagent_folds_into_parent() {
    let mut tr = Tracker::new();
    let parent = running_row("parent", "local", 9_000.0, Some(9_900.0));
    let mut child = running_row("child", "subagent", 9_100.0, Some(9_900.0));
    child.parent_session_id = Some("parent".to_string());
    let upd = tr.update(vec![parent, child], 10_000.0);
    assert_eq!(upd.chips.len(), 1);
    assert_eq!(upd.chips[0].id, "parent");
    assert_eq!(upd.chips[0].children, 1);
}

#[test]
fn update_subagent_orphan_is_own_chip() {
    let mut tr = Tracker::new();
    let mut child = running_row("child", "subagent", 9_000.0, Some(9_900.0));
    child.parent_session_id = Some("ghost".to_string()); // parent not present
    let upd = tr.update(vec![child], 10_000.0);
    assert_eq!(upd.chips.len(), 1);
    assert_eq!(upd.chips[0].id, "child");
    assert!(upd.chips[0].is_delegated);
    assert_eq!(upd.chips[0].children, 0);
}

#[test]
fn update_nested_folding_only_one_level() {
    let mut tr = Tracker::new();
    let gp = running_row("gp", "local", 9_000.0, Some(9_900.0));
    let mut p = running_row("p", "subagent", 9_100.0, Some(9_900.0));
    p.parent_session_id = Some("gp".to_string());
    let mut s = running_row("s", "subagent", 9_200.0, Some(9_900.0));
    s.parent_session_id = Some("p".to_string());
    let upd = tr.update(vec![gp, p, s], 10_000.0);
    let mut ids: Vec<&str> = upd.chips.iter().map(|c| c.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["gp", "s"]);
    let gp_chip = upd.chips.iter().find(|c| c.id == "gp").unwrap();
    assert_eq!(gp_chip.children, 1); // p folded in; s stays own chip
}

#[test]
fn update_a2a_source_is_not_a_chip() {
    let mut tr = Tracker::new();
    let a2a = running_row("peer", "a2a", 9_000.0, Some(9_900.0));
    let mut child = running_row("child", "subagent", 9_100.0, Some(9_900.0));
    child.parent_session_id = Some("peer".to_string());
    let upd = tr.update(vec![a2a, child], 10_000.0);
    assert_eq!(upd.chips.len(), 1);
    assert_eq!(upd.chips[0].id, "child"); // a2a excluded; child stands alone
}

#[test]
fn update_self_parent_not_folded() {
    let mut tr = Tracker::new();
    let mut r = running_row("a", "subagent", 9_000.0, Some(9_900.0));
    r.parent_session_id = Some("a".to_string());
    let upd = tr.update(vec![r], 10_000.0);
    assert_eq!(upd.chips.len(), 1);
    assert_eq!(upd.chips[0].id, "a");
    assert_eq!(upd.chips[0].children, 0);
}

#[test]
fn update_children_multiple() {
    let mut tr = Tracker::new();
    let parent = running_row("parent", "local", 9_000.0, Some(9_900.0));
    let mut rows = vec![parent];
    for i in 0..3 {
        let mut c = running_row(
            &format!("c{i}"),
            "subagent",
            9_100.0 + i as f64,
            Some(9_900.0),
        );
        c.parent_session_id = Some("parent".to_string());
        rows.push(c);
    }
    let upd = tr.update(rows, 10_000.0);
    assert_eq!(upd.chips.len(), 1);
    assert_eq!(upd.chips[0].children, 3);
}

#[test]
fn update_skew_warns_and_not_running() {
    let mut tr = Tracker::new();
    let mut r = running_row("future", "local", 11_000.0, None); // now+1000
    let upd = tr.update(vec![r.clone()], 10_000.0);
    assert!(upd.chips.is_empty());
    assert_eq!(
        tr.take_warnings(),
        vec![WarningKind::ClockSkew {
            id: "future".to_string()
        }]
    );
    r.started_at = 10_100.0; // within skew: running
    let upd2 = tr.update(vec![r], 10_000.0);
    assert_eq!(upd2.chips.len(), 1);
}

#[test]
fn update_rejected_row_warns_and_skips() {
    let mut tr = Tracker::new();
    let mut r = row("bad");
    r.started_at = f64::NAN;
    let upd = tr.update(vec![r], 10_000.0);
    assert!(upd.chips.is_empty());
    assert!(upd.events.is_empty());
    let warns = tr.take_warnings();
    assert_eq!(warns.len(), 1);
    assert!(matches!(&warns[0], WarningKind::InvalidTime { id, .. } if id == "bad"));
}

#[test]
fn update_mixed_scenario() {
    // one run, one finishing, one stale, one missing, one orphan subagent
    let mut tr = Tracker::new();
    let run = running_row("run", "local", 9_000.0, Some(9_900.0));
    let mut fin = running_row("fin", "local", 8_000.0, Some(9_000.0));
    fin.ended_at = Some(9_500.0);
    let stale = running_row("stale", "local", 8_000.0, Some(8_100.0));
    let mut sub = running_row("sub", "subagent", 9_200.0, Some(9_900.0));
    sub.parent_session_id = Some("run".to_string());

    let u1 = tr.update(
        vec![run.clone(), fin.clone(), stale.clone(), sub.clone()],
        10_000.0,
    );
    assert_eq!(u1.chips.len(), 1); // only the running `run` chip
    assert_eq!(u1.chips[0].id, "run");
    assert_eq!(u1.chips[0].children, 1);
    // both running rows are seen for the first time -> Started each
    assert_eq!(
        u1.events,
        vec![
            Event::Started {
                id: "run".to_string()
            },
            Event::Started {
                id: "sub".to_string()
            }
        ]
    );

    // next update: run still there, fin already finished (no event),
    // stale still stale (no event), sub still folds
    let u2 = tr.update(
        vec![run.clone(), fin.clone(), stale.clone(), sub.clone()],
        10_100.0,
    );
    assert!(u2.events.is_empty());

    // now run disappears; sub still present but parent gone -> own chip
    let u3 = tr.update(vec![fin, stale, sub], 10_200.0);
    assert_eq!(
        u3.events,
        vec![Event::Disappeared {
            id: "run".to_string()
        }]
    );
    assert_eq!(u3.chips.len(), 1);
    assert_eq!(u3.chips[0].id, "sub");
}

// ---------------------------------------------------------------------------
// serde wire shape
// ---------------------------------------------------------------------------

#[test]
fn serde_session_row_camel_case() {
    let r = SessionRow {
        id: "s1".to_string(),
        source: "subagent".to_string(),
        parent_session_id: Some("p1".to_string()),
        model: Some("gpt-4o".to_string()),
        billing_provider: Some("openai".to_string()),
        started_at: 1_000.0,
        ended_at: Some(2_000.0),
        end_reason: Some("agent_close".to_string()),
        last_activity_at: Some(1_500.0),
        api_call_count: Some(7),
        title: Some("T".to_string()),
    };
    let json = serde_json::to_string(&r).unwrap();
    for key in [
        "\"parentSessionId\"",
        "\"billingProvider\"",
        "\"startedAt\"",
        "\"endedAt\"",
        "\"endReason\"",
        "\"lastActivityAt\"",
        "\"apiCallCount\"",
    ] {
        assert!(json.contains(key), "missing {key} in {json}");
    }
    assert!(!json.contains("parent_session_id"));
    let back: SessionRow = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);
}

#[test]
fn serde_chip_camel_case() {
    let c = Chip {
        id: "c1".to_string(),
        label: "lbl".to_string(),
        model: Some("m".to_string()),
        provider: Some("p".to_string()),
        elapsed_secs: 12.5,
        api_call_count: Some(3),
        is_delegated: true,
        children: 2,
    };
    let json = serde_json::to_string(&c).unwrap();
    for key in [
        "\"elapsedSecs\"",
        "\"apiCallCount\"",
        "\"isDelegated\"",
        "\"children\"",
    ] {
        assert!(json.contains(key), "missing {key} in {json}");
    }
    assert!(!json.contains("elapsed_secs"));
    assert!(!json.contains("is_delegated"));
    let back: Chip = serde_json::from_str(&json).unwrap();
    assert_eq!(back, c);
}

#[test]
fn serde_event_camel_case_json_shape() {
    let started = Event::Started {
        id: "a".to_string(),
    };
    let j1 = serde_json::to_string(&started).unwrap();
    assert!(j1.contains(r#"{"started":{"id":"a"}}"#), "got {j1}");

    let finished = Event::Finished {
        id: "a".to_string(),
        reason: Some("cli_close".to_string()),
        elapsed_secs: 600.0,
    };
    let j2 = serde_json::to_string(&finished).unwrap();
    assert!(j2.contains("\"elapsedSecs\":600.0"), "got {j2}");
    assert!(!j2.contains("elapsed_secs"));
    let back: Event = serde_json::from_str(&j2).unwrap();
    assert_eq!(back, finished);
}

#[test]
fn serde_config_round_trip() {
    let c = TrackerConfig::try_new(
        1200,
        43_200,
        4,
        vec!["oneshot".to_string(), "subagent".to_string()],
    )
    .unwrap();
    let json = serde_json::to_string(&c).unwrap();
    for key in [
        "\"staleAfterSecs\"",
        "\"lookbackSecs\"",
        "\"maxChips\"",
        "\"delegatedSources\"",
    ] {
        assert!(json.contains(key), "missing {key} in {json}");
    }
    let back: TrackerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back, c);
}

#[test]
fn serde_sequence_form_parses_row() {
    // serde accepted sequence form for structs; pin the quirk: an array of
    // the 11 fields in order parses as a SessionRow.
    let arr = r#"["id1","local",null,null,null,1000.0,null,null,null,null,null]"#;
    let r: SessionRow = serde_json::from_str(arr).unwrap();
    assert_eq!(r.id, "id1");
    assert_eq!(r.source, "local");
    assert_eq!(r.started_at, 1000.0);
    // wrong length errors rather than fabricating
    let bad = r#"["id1","local",null,null,null,1000.0]"#;
    assert!(serde_json::from_str::<SessionRow>(bad).is_err());
}

#[test]
fn serde_no_aliases_snake_case_keys_rejected() {
    let obj = r#"{
        "id":"s","source":"local","parent_session_id":null,"model":null,
        "billing_provider":null,"started_at":0.0,"ended_at":null,
        "end_reason":null,"last_activity_at":null,"api_call_count":null,"title":null
    }"#;
    // no #[serde(alias)]: snake_case keys are unknown -> required camelCase
    // fields missing -> error
    assert!(serde_json::from_str::<SessionRow>(obj).is_err());
}

#[test]
fn update_ignores_rows_started_before_lookback() {
    // Default config: lookbackSecs = 86400.
    let now = 10_000_000.0;
    let mut old = row("old");
    old.started_at = now - 86_401.0; // outside lookback window
    old.last_activity_at = Some(now - 10.0); // recently active
    let mut fresh = row("fresh");
    fresh.started_at = now - 86_399.0; // inside lookback window
    fresh.last_activity_at = Some(now - 10.0);

    let mut tr = Tracker::new();
    let upd = tr.update(vec![old.clone(), fresh.clone()], now);
    assert_eq!(upd.chips.len(), 1, "chips: {:?}", upd.chips);
    assert_eq!(upd.chips[0].id, "fresh");
    assert_eq!(
        upd.events,
        vec![Event::Started {
            id: "fresh".to_string()
        }],
        "events: {:?}",
        upd.events
    );

    // Boundary is inclusive: startedAt == now - lookbackSecs is kept.
    let mut b = row("boundary");
    b.started_at = now - 86_400.0;
    b.last_activity_at = Some(now - 10.0);
    let mut tr2 = Tracker::new();
    let upd2 = tr2.update(vec![b], now);
    assert_eq!(upd2.chips.len(), 1, "chips: {:?}", upd2.chips);
    assert_eq!(upd2.chips[0].id, "boundary");
    assert_eq!(
        upd2.events,
        vec![Event::Started {
            id: "boundary".to_string()
        }]
    );
}
