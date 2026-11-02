use ai_dashboard_core::{build_dashboard, days_from_civil, DashError, LimitWindow, UsageRow};

const DAY: i64 = 86_400;

fn ts(y: i64, m: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
    days_from_civil(y, m, d) * DAY + (h as i64) * 3600 + (mi as i64) * 60 + s as i64
}

/// 2026-10-30T04:30:00Z (local hour 4 spans 04:00–04:59 UTC).
const NOW: i64 = 1_793_334_600;

fn row(session: &str, start: i64, provider: &str, model: &str) -> UsageRow {
    UsageRow {
        session_id: session.to_string(),
        started_at_s: start,
        ended_at_s: None,
        provider: provider.to_string(),
        model: model.to_string(),
        project: None,
        input_tokens: None,
        output_tokens: None,
        cost_usd: None,
    }
}

fn limit(provider: &str, label: &str, used: Option<f64>, resets: Option<i64>) -> LimitWindow {
    LimitWindow {
        provider: provider.to_string(),
        label: label.to_string(),
        used_percent: used,
        resets_at_s: resets,
    }
}

/// Exact-equality helper for Option<f64> without triggering float_cmp lint
/// (free helper fns in integration tests may not use unwrap/expect).
fn cost_is(o: Option<f64>, want: f64) -> bool {
    match o {
        Some(v) => (v - want).abs() < 1e-9,
        None => false,
    }
}

// ---------------------------------------------------------------------------
// offset validation
// ---------------------------------------------------------------------------

#[test]
fn offset_validation_boundaries() {
    assert!(build_dashboard(&[], &[], 0, 50_400).is_ok());
    assert!(build_dashboard(&[], &[], 0, -50_400).is_ok());
    assert_eq!(
        build_dashboard(&[], &[], 0, 50_401),
        Err(DashError::InvalidUtcOffset(50_401))
    );
    assert_eq!(
        build_dashboard(&[], &[], 0, -50_401),
        Err(DashError::InvalidUtcOffset(-50_401))
    );
}

// ---------------------------------------------------------------------------
// spendToday
// ---------------------------------------------------------------------------

#[test]
fn spend_today_respects_local_day_with_negative_offset() {
    // Offset -4h (EDT): "today" for now=04:00Z is Oct 30; a row at 03:00Z
    // is Oct 29 locally (23:00), so it must NOT count.
    let mut r = row("s1", ts(2026, 10, 30, 3, 0, 0), "p", "m");
    r.cost_usd = Some(5.0);
    let d = build_dashboard(&[r], &[], NOW, -14_400).unwrap();
    assert_eq!(d.spend_today_usd, Some(0.0));
    assert!(!d.spend_today_partial);
}

#[test]
fn spend_today_respects_local_day_with_positive_offset() {
    // Offset +5:30: the same 03:00Z row is Oct 30 locally (08:30) and counts.
    let mut r = row("s1", ts(2026, 10, 30, 3, 0, 0), "p", "m");
    r.cost_usd = Some(5.0);
    let d = build_dashboard(&[r], &[], NOW, 19_800).unwrap();
    assert!(
        cost_is(d.spend_today_usd, 5.0),
        "want 5.0, got {:?}",
        d.spend_today_usd
    );
    assert!(!d.spend_today_partial);
}

#[test]
fn spend_today_none_when_all_today_costs_unknown() {
    let r = row("s1", NOW - 3600, "p", "m");
    let d = build_dashboard(&[r], &[], NOW, 0).unwrap();
    assert_eq!(d.spend_today_usd, None);
    assert!(!d.spend_today_partial);
}

#[test]
fn spend_today_zero_when_no_rows_today() {
    let mut r = row("s1", NOW - 2 * DAY, "p", "m");
    r.cost_usd = Some(9.0);
    let d = build_dashboard(&[r], &[], NOW, 0).unwrap();
    assert_eq!(d.spend_today_usd, Some(0.0));
    assert!(!d.spend_today_partial);
}

#[test]
fn spend_today_zero_is_real_when_known_costs_zero() {
    let mut r = row("s1", NOW - 3600, "p", "m");
    r.cost_usd = Some(0.0);
    let d = build_dashboard(&[r], &[], NOW, 0).unwrap();
    assert_eq!(d.spend_today_usd, Some(0.0));
    assert!(!d.spend_today_partial);
}

#[test]
fn spend_today_partial_when_some_costs_unknown() {
    let mut a = row("s1", NOW - 3600, "p", "m");
    a.cost_usd = Some(2.0);
    let b = row("s2", NOW - 1800, "p", "m"); // no cost
    let d = build_dashboard(&[a, b], &[], NOW, 0).unwrap();
    assert!(cost_is(d.spend_today_usd, 2.0));
    assert!(d.spend_today_partial);
}

#[test]
fn spend_today_sums_only_finite_costs() {
    let mut a = row("s1", NOW - 3600, "p", "m");
    a.cost_usd = Some(3.0);
    let mut b = row("s2", NOW - 1800, "p", "m");
    b.cost_usd = Some(f64::NAN);
    let d = build_dashboard(&[a, b], &[], NOW, 0).unwrap();
    assert!(cost_is(d.spend_today_usd, 3.0));
    assert!(d.spend_today_partial);
    assert_eq!(d.rows_ignored, 1);
}

// ---------------------------------------------------------------------------
// limits / resetText
// ---------------------------------------------------------------------------

#[test]
fn reset_text_boundaries() {
    let cases: &[(i64, &str)] = &[
        (0, "now"),
        (1, "<1m"),
        (59, "<1m"),
        (60, "1m"),
        (3599, "59m"),
        (3600, "1h 0m"),
        (86399, "23h 59m"),
        (86400, "1d 0h"),
        (90061, "1d 1h"),
    ];
    for &(delta, want) in cases {
        let l = limit("p", "s", None, Some(NOW + delta));
        let d = build_dashboard(&[], &[l], NOW, 0).unwrap();
        assert_eq!(d.limits[0].resets_in_s, Some(delta), "delta={delta}");
        assert_eq!(
            d.limits[0].reset_text.as_deref(),
            Some(want),
            "delta={delta}"
        );
    }
}

#[test]
fn reset_text_none_when_resets_at_unknown() {
    let l = limit("p", "s", None, None);
    let d = build_dashboard(&[], &[l], NOW, 0).unwrap();
    assert_eq!(d.limits[0].resets_in_s, None);
    assert_eq!(d.limits[0].reset_text, None);
}

#[test]
fn resets_past_gives_zero_and_now() {
    let l = limit("p", "s", None, Some(NOW - 5));
    let d = build_dashboard(&[], &[l], NOW, 0).unwrap();
    assert_eq!(d.limits[0].resets_in_s, Some(0));
    assert_eq!(d.limits[0].reset_text.as_deref(), Some("now"));
}

#[test]
fn used_percent_clamped_and_cleaned() {
    let cases: &[(Option<f64>, Option<f64>)] = &[
        (Some(100.5), Some(100.0)),
        (Some(-5.0), Some(0.0)),
        (Some(42.0), Some(42.0)),
        (Some(f64::NAN), None),
        (Some(f64::INFINITY), None),
        (Some(f64::NEG_INFINITY), None),
        (None, None),
    ];
    for &(input, want) in cases {
        let l = limit("p", "s", input, None);
        let d = build_dashboard(&[], &[l], NOW, 0).unwrap();
        match (d.limits[0].used_percent, want) {
            (None, None) => {}
            (Some(got), Some(want)) => assert!(cost_is(Some(got), want), "got {got} want {want}"),
            (got, want) => panic!("mismatch: got {got:?} want {want:?}"),
        }
    }
}

#[test]
fn limits_sorted_by_provider_then_label() {
    let l0 = limit("b", "l0", None, None);
    let l1 = limit("a", "l1", None, None);
    let l2 = limit("a", "l0", None, None);
    let d = build_dashboard(&[], &[l0, l1, l2], NOW, 0).unwrap();
    let pairs: Vec<(&str, &str)> = d
        .limits
        .iter()
        .map(|c| (c.provider.as_str(), c.label.as_str()))
        .collect();
    assert_eq!(pairs, vec![("a", "l0"), ("a", "l1"), ("b", "l0")]);
}

// ---------------------------------------------------------------------------
// active session
// ---------------------------------------------------------------------------

#[test]
fn active_6h_boundary_exact_is_active() {
    let r = row("s1", NOW - 21_600, "p", "m");
    let d = build_dashboard(&[r], &[], NOW, 0).unwrap();
    assert!(d.active.is_some());
}

#[test]
fn active_beyond_6h_is_stale() {
    let r = row("s1", NOW - 21_601, "p", "m");
    let d = build_dashboard(&[r], &[], NOW, 0).unwrap();
    assert!(d.active.is_none());
}

#[test]
fn active_requires_open_end() {
    let mut r = row("s1", NOW - 100, "p", "m");
    r.ended_at_s = Some(NOW);
    let d = build_dashboard(&[r], &[], NOW, 0).unwrap();
    assert!(d.active.is_none());
}

#[test]
fn active_picks_most_recent_start() {
    let a = row("sa", NOW - 100, "p", "m");
    let b = row("sb", NOW - 50, "p", "m");
    let d = build_dashboard(&[a, b], &[], NOW, 0).unwrap();
    match d.active {
        Some(act) => {
            assert_eq!(act.session_id, "sb");
            assert_eq!(act.started_at_s, NOW - 50);
            assert_eq!(act.provider, "p");
            assert_eq!(act.model, "m");
        }
        None => panic!("expected an active session"),
    }
}

#[test]
fn active_tie_breaks_by_session_id() {
    let a = row("a", NOW - 100, "p", "m");
    let b = row("b", NOW - 100, "p", "m");
    let d = build_dashboard(&[a, b], &[], NOW, 0).unwrap();
    match d.active {
        Some(act) => assert_eq!(act.session_id, "b"),
        None => panic!("expected an active session"),
    }
}

#[test]
fn active_stale_only_row_is_none() {
    let r = row("s1", NOW - 40_000, "p", "m");
    let d = build_dashboard(&[r], &[], NOW, 0).unwrap();
    assert!(d.active.is_none());
}

// ---------------------------------------------------------------------------
// hourly
// ---------------------------------------------------------------------------

#[test]
fn hourly_always_24_buckets_current_last() {
    let d = build_dashboard(&[], &[], NOW, 0).unwrap();
    assert_eq!(d.hourly.len(), 24);
    let first = d.hourly.first().unwrap();
    let last = d.hourly.last().unwrap();
    assert_eq!(last.start_s, NOW - 1800); // start of the current local hour
    assert_eq!(first.start_s, NOW - 1800 - 23 * 3600);
    for b in &d.hourly {
        assert_eq!(b.tokens, None);
        assert_eq!(b.sessions, 0);
    }
}

#[test]
fn hourly_sums_tokens_and_distinct_sessions() {
    let mut r1 = row("s1", NOW - 100, "p", "m1");
    r1.input_tokens = Some(100);
    r1.output_tokens = Some(50);
    let mut r2 = row("s1", NOW - 200, "p", "m2");
    r2.output_tokens = Some(25);
    let mut r3 = row("s2", NOW - 50, "p", "m1");
    r3.input_tokens = Some(10);
    let d = build_dashboard(&[r1, r2, r3], &[], NOW, 0).unwrap();
    let last = d.hourly.last().unwrap();
    assert_eq!(last.tokens, Some(185));
    assert_eq!(last.sessions, 2);
}

#[test]
fn hourly_tokens_none_when_all_unknown_but_sessions_counted() {
    let a = row("s1", NOW - 100, "p", "m");
    let b = row("s2", NOW - 200, "p", "m");
    let d = build_dashboard(&[a, b], &[], NOW, 0).unwrap();
    let last = d.hourly.last().unwrap();
    assert_eq!(last.tokens, None);
    assert_eq!(last.sessions, 2);
}

#[test]
fn hourly_token_sums_saturate() {
    let mut a = row("s1", NOW - 100, "p", "m");
    a.input_tokens = Some(u64::MAX);
    a.output_tokens = Some(u64::MAX);
    let mut b = row("s2", NOW - 200, "p", "m");
    b.input_tokens = Some(u64::MAX);
    b.output_tokens = Some(u64::MAX);
    let d = build_dashboard(&[a, b], &[], NOW, 0).unwrap();
    assert_eq!(d.hourly.last().unwrap().tokens, Some(u64::MAX));
}

#[test]
fn hourly_model_switching_attributed_per_row() {
    let mut a = row("s1", NOW - 100, "p", "model-a");
    a.input_tokens = Some(30);
    let mut b = row("s1", NOW - 150, "p", "model-b");
    b.input_tokens = Some(70);
    let d = build_dashboard(&[a, b], &[], NOW, 0).unwrap();
    let last = d.hourly.last().unwrap();
    assert_eq!(last.sessions, 1); // one session, two rows
    assert_eq!(last.tokens, Some(100));
    let keys: Vec<&str> = d.by_model.iter().map(|l| l.key.as_str()).collect();
    assert!(keys.contains(&"model-a"));
    assert!(keys.contains(&"model-b"));
}

// ---------------------------------------------------------------------------
// cost tables (byModel / byProject)
// ---------------------------------------------------------------------------

#[test]
fn by_model_21_keys_merge_into_other() {
    let mut rows = Vec::new();
    for i in 0..21 {
        let mut r = row(&format!("s{i}"), NOW - 100, "p", &format!("model-{i:02}"));
        r.cost_usd = Some(i as f64);
        rows.push(r);
    }
    let d = build_dashboard(&rows, &[], NOW, 0).unwrap();
    assert_eq!(d.by_model.len(), 21);
    assert_eq!(d.by_model.last().unwrap().key, "Other");
    assert_eq!(d.by_model.last().unwrap().rows, 1);
    assert_eq!(d.by_model[0].key, "model-20");
    assert!(cost_is(d.by_model[0].cost_usd, 20.0));

    let rows20 = rows[..20].to_vec();
    let d20 = build_dashboard(&rows20, &[], NOW, 0).unwrap();
    assert_eq!(d20.by_model.len(), 20);
    assert!(!d20.by_model.iter().any(|l| l.key == "Other"));
}

#[test]
fn by_model_none_cost_sorts_last() {
    let mut a = row("sA", NOW - 100, "p", "A");
    a.cost_usd = Some(2.0);
    a.input_tokens = Some(1);
    let b = row("sB", NOW - 100, "p", "B"); // cost unknown
    let mut c = row("sC", NOW - 100, "p", "C");
    c.cost_usd = Some(5.0);
    c.input_tokens = Some(1);
    let d = build_dashboard(&[a, b, c], &[], NOW, 0).unwrap();
    let keys: Vec<&str> = d.by_model.iter().map(|l| l.key.as_str()).collect();
    assert_eq!(keys, vec!["C", "A", "B"]);
}

#[test]
fn by_model_tokens_desc_before_key() {
    let mut a = row("sA", NOW - 100, "p", "A");
    a.cost_usd = Some(1.0);
    a.input_tokens = Some(50);
    let mut b = row("sB", NOW - 100, "p", "B");
    b.cost_usd = Some(1.0);
    b.input_tokens = Some(500);
    let d = build_dashboard(&[a, b], &[], NOW, 0).unwrap();
    let keys: Vec<&str> = d.by_model.iter().map(|l| l.key.as_str()).collect();
    assert_eq!(keys, vec!["B", "A"]);
}

#[test]
fn by_project_none_uses_no_project_key() {
    let mut a = row("sA", NOW - 100, "p", "m");
    a.cost_usd = Some(3.0);
    let mut b = row("sB", NOW - 100, "p", "m");
    b.project = Some("alpha".to_string());
    b.cost_usd = Some(7.0);
    let d = build_dashboard(&[a, b], &[], NOW, 0).unwrap();
    let keys: Vec<&str> = d.by_project.iter().map(|l| l.key.as_str()).collect();
    assert_eq!(keys, vec!["alpha", "(no project)"]);
}

#[test]
fn by_model_window_is_30_days() {
    let inside = row("s1", NOW - 30 * DAY, "p", "m");
    let outside = row("s2", NOW - 30 * DAY - 1, "p", "m");
    let d = build_dashboard(&[inside, outside], &[], NOW, 0).unwrap();
    assert_eq!(d.by_model.len(), 1);
    assert_eq!(d.by_model[0].rows, 1);
}

#[test]
fn cost_line_tokens_unknown_stays_none() {
    let mut a = row("s1", NOW - 100, "p", "m");
    a.cost_usd = Some(1.0);
    let d = build_dashboard(&[a], &[], NOW, 0).unwrap();
    assert_eq!(d.by_model[0].tokens, None);
    assert!(cost_is(d.by_model[0].cost_usd, 1.0));
}

// ---------------------------------------------------------------------------
// heatmap
// ---------------------------------------------------------------------------

#[test]
fn heatmap_weekday_for_known_date() {
    let fri = row("s1", ts(2026, 10, 30, 3, 0, 0), "p", "m"); // Friday, hour 3
    let mon = row("s2", ts(2026, 10, 26, 9, 0, 0), "p", "m"); // Monday, hour 9
    let d = build_dashboard(&[fri, mon], &[], NOW, 0).unwrap();
    assert_eq!(d.heatmap[4][3], 1);
    assert_eq!(d.heatmap[0][9], 1);
    assert_eq!(d.heatmap[4][9], 0);
}

#[test]
fn heatmap_counts_distinct_session_start() {
    let a1 = row("s1", NOW - 100, "p", "m");
    let a2 = row("s1", NOW - 100, "p", "m"); // same session + start: dedup
    let a3 = row("s1", NOW - 500, "p", "m"); // same session, different start
    let a4 = row("s2", NOW - 100, "p", "m"); // different session, same start
    let d = build_dashboard(&[a1, a2, a3, a4], &[], NOW, 0).unwrap();
    let total: u32 = d.heatmap.iter().flatten().sum();
    assert_eq!(total, 3);
    // all four rows are Friday (Oct 30) hour 4.
    assert_eq!(d.heatmap[4][4], 3);
}

#[test]
fn heatmap_window_28_days() {
    let inside = row("s1", NOW - 28 * DAY, "p", "m");
    let outside = row("s2", NOW - 28 * DAY - 1, "p", "m");
    let d = build_dashboard(&[inside, outside], &[], NOW, 0).unwrap();
    let total: u32 = d.heatmap.iter().flatten().sum();
    assert_eq!(total, 1);
}

// ---------------------------------------------------------------------------
// rowsIgnored / filtering
// ---------------------------------------------------------------------------

#[test]
fn future_rows_ignored_and_dropped() {
    let fut = row("s1", NOW + 61, "p", "m");
    let tol = row("s2", NOW + 60, "p", "m");
    let d = build_dashboard(&[fut, tol], &[], NOW, 0).unwrap();
    assert_eq!(d.rows_ignored, 1);
    assert_eq!(d.hourly.last().unwrap().sessions, 1);
    assert_eq!(d.by_model.len(), 1);
    assert_eq!(d.by_model[0].rows, 1);
}

#[test]
fn empty_provider_and_model_dropped() {
    let ep = row("s1", NOW - 100, "", "m");
    let em = row("s2", NOW - 100, "p", "");
    let both = row("s3", NOW - 100, "", "");
    let ok = row("s4", NOW - 100, "p", "m");
    let d = build_dashboard(&[ep, em, both, ok], &[], NOW, 0).unwrap();
    assert_eq!(d.rows_ignored, 3);
    assert_eq!(d.hourly.last().unwrap().sessions, 1);
}

#[test]
fn invalid_cost_counted_but_kept() {
    let mut nan = row("s1", NOW - 100, "p", "m");
    nan.cost_usd = Some(f64::NAN);
    let mut neg = row("s2", NOW - 100, "p", "m");
    neg.cost_usd = Some(-1.0);
    let mut inf = row("s3", NOW - 100, "p", "m");
    inf.cost_usd = Some(f64::INFINITY);
    let mut good = row("s4", NOW - 100, "p", "m");
    good.cost_usd = Some(4.0);
    let d = build_dashboard(&[nan, neg, inf, good], &[], NOW, 0).unwrap();
    assert_eq!(d.rows_ignored, 3);
    assert_eq!(d.hourly.last().unwrap().sessions, 4); // all kept
    assert_eq!(d.by_model[0].rows, 4);
    assert!(cost_is(d.by_model[0].cost_usd, 4.0)); // only finite cost
    assert!(cost_is(d.spend_today_usd, 4.0));
    assert!(d.spend_today_partial);
}

// ---------------------------------------------------------------------------
// serde camelCase round-trip
// ---------------------------------------------------------------------------

#[test]
fn serde_camelcase_roundtrip() {
    let mut r = row("s1", NOW - 100, "openai", "gpt-4o");
    r.project = Some("demo".to_string());
    r.input_tokens = Some(120);
    r.output_tokens = Some(80);
    r.cost_usd = Some(0.012);
    let l = limit("openai", "weekly", Some(77.5), Some(NOW + 3600));
    let d = build_dashboard(&[r], &[l], NOW, 0).unwrap();

    let json = serde_json::to_string(&d).unwrap();
    for key in [
        "spendTodayUsd",
        "spendTodayPartial",
        "rowsIgnored",
        "byModel",
        "byProject",
        "startS",
        "startedAtS",
        "usedPercent",
        "resetsInS",
        "resetText",
    ] {
        assert!(
            json.contains(&format!("\"{key}\"")),
            "missing {key} in {json}"
        );
    }
    assert!(!json.contains("spend_today_usd"));

    let back: ai_dashboard_core::Dashboard = serde_json::from_str(&json).unwrap();
    assert_eq!(back, d);
}

// ---------------------------------------------------------------------------
// unknown data never fabricated
// ---------------------------------------------------------------------------

#[test]
fn unknown_data_never_fabricated_as_zero() {
    let r = row("s1", NOW - 100, "p", "m"); // no tokens, no cost
    let d = build_dashboard(&[r], &[], NOW, 0).unwrap();
    assert_eq!(d.spend_today_usd, None);
    assert!(!d.spend_today_partial);
    assert_eq!(d.hourly.last().unwrap().tokens, None);
    assert_eq!(d.by_model[0].tokens, None);
    assert_eq!(d.by_model[0].cost_usd, None);

    let l = limit("p", "s", Some(f64::NAN), None);
    let d2 = build_dashboard(&[], &[l], NOW, 0).unwrap();
    assert_eq!(d2.limits[0].used_percent, None);
    assert_eq!(d2.limits[0].resets_in_s, None);
    assert_eq!(d2.limits[0].reset_text, None);
}
