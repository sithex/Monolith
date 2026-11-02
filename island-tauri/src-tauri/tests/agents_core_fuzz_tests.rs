use hermes_island_lib::integrations::agents::core::{
    classify, Event, SessionRow, Tracker, TrackerConfig, Update,
};

/// Hand-rolled LCG (MMIX constants), fixed seed, no external deps.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }

    fn coin(&mut self) -> bool {
        self.below(2) == 0
    }
}

const SOURCES: [&str; 9] = [
    "a2a", "cli", "desktop", "local", "oneshot", "subagent", "tui", "", "weird",
];

const ID_CHARS: [char; 14] = [
    'a', 'b', '0', '-', '_', 'l', 'o', 'c', 'a', 'l', '-', '😀', 'é', ' ',
];

fn random_id(lcg: &mut Lcg) -> String {
    let len = lcg.below(24) as usize;
    let mut s = String::new();
    for _ in 0..len {
        s.push(ID_CHARS[lcg.below(ID_CHARS.len() as u64) as usize]);
    }
    s
}

fn random_ts(lcg: &mut Lcg, now: f64) -> f64 {
    match lcg.below(10) {
        0 => f64::NAN,
        1 => f64::INFINITY,
        2 => f64::NEG_INFINITY,
        3 => -(lcg.below(1_000_000) as f64),
        4 => now + 100_000.0 + lcg.below(1_000_000) as f64, // far future
        5 => now + lcg.below(400) as f64,                   // near future (incl. skew boundary)
        6 => now - lcg.below(500) as f64,                   // recent
        7 => now - lcg.below(500_000) as f64,               // old / outside lookback
        _ => lcg.below(1_000_000_000) as f64,               // arbitrary past epoch
    }
}

fn random_row(lcg: &mut Lcg, now: f64) -> SessionRow {
    let started = random_ts(lcg, now);
    let has_ended = lcg.coin();
    let ended = if has_ended && lcg.coin() {
        Some(started + lcg.below(100_000) as f64)
    } else if has_ended {
        Some(random_ts(lcg, now))
    } else {
        None
    };
    let activity = if has_ended {
        if lcg.coin() {
            Some(random_ts(lcg, now))
        } else {
            None
        }
    } else if lcg.coin() {
        Some(random_ts(lcg, now))
    } else {
        None
    };
    SessionRow {
        id: random_id(lcg),
        source: SOURCES[lcg.below(SOURCES.len() as u64) as usize].to_string(),
        parent_session_id: if lcg.coin() {
            Some(random_id(lcg))
        } else {
            None
        },
        model: if lcg.coin() {
            Some(format!("m{}", lcg.below(100)))
        } else {
            None
        },
        billing_provider: if lcg.coin() {
            Some("openai".to_string())
        } else {
            None
        },
        started_at: started,
        ended_at: ended,
        end_reason: if has_ended && lcg.coin() {
            Some("tui_close".to_string())
        } else {
            None
        },
        last_activity_at: activity,
        api_call_count: if lcg.coin() {
            Some(lcg.below(10_000))
        } else {
            None
        },
        title: if lcg.coin() {
            Some(format!("t{}", lcg.below(1000)))
        } else {
            None
        },
    }
}

fn random_cfg(lcg: &mut Lcg) -> TrackerConfig {
    let stale = 60 + lcg.below(86_400 - 60 + 1);
    let lookback = 1 + lcg.below(86_400 * 30);
    let max_chips = 1 + lcg.below(32);
    let n_sources = lcg.below(4);
    let mut sources = Vec::new();
    for _ in 0..n_sources {
        sources.push(SOURCES[lcg.below(SOURCES.len() as u64) as usize].to_string());
    }
    if lcg.coin() {
        sources.push("oneshot".to_string());
    }
    TrackerConfig::try_new(stale, lookback, max_chips as u32, sources)
        .unwrap_or_else(|_| TrackerConfig::default())
}

fn event_kind_id(ev: &Event) -> (u8, String) {
    match ev {
        Event::Started { id } => (0, id.clone()),
        Event::Finished { id, .. } => (1, id.clone()),
        Event::Stalled { id } => (2, id.clone()),
        Event::Disappeared { id } => (3, id.clone()),
    }
}

fn assert_update_invariants(upd: &Update, cfg: &TrackerConfig, now_finite: bool) {
    assert!(upd.chips.len() <= cfg.max_chips() as usize);
    assert!(upd.chips.windows(2).all(|w| w[0].id != w[1].id));
    let ids: std::collections::BTreeSet<&str> = upd.chips.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids.len(), upd.chips.len(), "duplicate chip ids");
    if now_finite {
        // newest-first ordering: elapsed (now - startedAt) is non-decreasing
        assert!(
            upd.chips
                .windows(2)
                .all(|w| w[0].elapsed_secs <= w[1].elapsed_secs + 1e-9),
            "chips not sorted newest-first"
        );
    }
    let mut seen: std::collections::BTreeSet<(u8, String)> = std::collections::BTreeSet::new();
    for ev in &upd.events {
        let key = event_kind_id(ev);
        assert!(
            seen.insert(key.clone()),
            "duplicate event kind/id in one update: {key:?}"
        );
    }
}

#[test]
fn fuzz_tracker_never_panics_and_keeps_invariants() {
    let mut lcg = Lcg(0xC0FF_EE42_0000_0003);

    for _ in 0..2500 {
        let now = if lcg.coin() {
            1_700_000_000.0 + lcg.below(10_000_000) as f64
        } else {
            f64::from_bits(lcg.next()) // garbage now (NaN/inf/…) must not panic
        };
        let now_finite = now.is_finite();
        let cfg = random_cfg(&mut lcg);
        let mut tr = Tracker::with_config(cfg.clone());

        let n = lcg.below(15) as usize;
        let mut rows = Vec::new();
        for _ in 0..n {
            rows.push(random_row(&mut lcg, now));
        }

        // direct classify over every parser input: never panics
        for r in &rows {
            let _ = classify(r, now, &cfg);
        }

        // first update on a fresh tracker: no history events
        let upd = tr.update(rows, now);
        assert!(
            !upd.events
                .iter()
                .any(|e| matches!(e, Event::Finished { .. } | Event::Stalled { .. })),
            "first update emitted history events"
        );
        assert_update_invariants(&upd, &cfg, now_finite);

        // keep the tracker alive across a few steps to exercise transitions
        for step in 0..4 {
            let now2 = now + step as f64 * 1000.0;
            let n2 = lcg.below(15) as usize;
            let mut rows2 = Vec::new();
            for _ in 0..n2 {
                rows2.push(random_row(&mut lcg, now2));
            }
            let upd2 = tr.update(rows2, now2);
            assert_update_invariants(&upd2, &cfg, now_finite);
        }

        let _ = tr.take_warnings();
    }
}
