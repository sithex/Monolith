use std::collections::HashSet;

use ai_dashboard_core::time;
use ai_dashboard_core::{build_dashboard, CostLine, DashError, LimitWindow, UsageRow};

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

    fn pick(&mut self, chars: &[char]) -> char {
        chars[self.below(chars.len() as u64) as usize]
    }
}

fn rand_string(lcg: &mut Lcg, allow_empty: bool) -> String {
    let mut len = lcg.below(6) as usize;
    if allow_empty && lcg.coin() {
        len = 0;
    }
    (0..len)
        .map(|_| lcg.pick(&['a', 'b', '0', ' ', 'é', '_']))
        .collect()
}

fn rand_f64(lcg: &mut Lcg) -> f64 {
    match lcg.below(8) {
        0 => f64::NAN,
        1 => f64::INFINITY,
        2 => f64::NEG_INFINITY,
        3 => -(lcg.next() as f64) / 1e6,
        _ => (lcg.next() as f64) / 1e6,
    }
}

fn rand_row(lcg: &mut Lcg) -> UsageRow {
    let mut r = UsageRow {
        session_id: rand_string(lcg, false),
        started_at_s: (lcg.next() as i64) ^ (lcg.next() as i64),
        ended_at_s: None,
        provider: rand_string(lcg, true),
        model: rand_string(lcg, true),
        project: None,
        input_tokens: None,
        output_tokens: None,
        cost_usd: None,
    };
    if lcg.coin() {
        r.ended_at_s = Some(r.started_at_s.wrapping_add(lcg.next() as i64));
    }
    if lcg.coin() {
        r.project = Some(rand_string(lcg, false));
    }
    if lcg.coin() {
        r.input_tokens = Some(lcg.next());
    }
    if lcg.coin() {
        r.output_tokens = Some(lcg.next());
    }
    if lcg.coin() {
        r.cost_usd = Some(rand_f64(lcg));
    }
    r
}

fn cost_rank(o: Option<f64>) -> (bool, f64) {
    match o {
        Some(v) => (true, v),
        None => (false, 0.0),
    }
}

fn token_rank(o: Option<u64>) -> (bool, u64) {
    match o {
        Some(v) => (true, v),
        None => (false, 0),
    }
}

/// Non-decreasing under the canonical cost-line ordering
/// (cost desc, None last; tokens desc, None last; key asc).
fn line_le(a: &CostLine, b: &CostLine) -> bool {
    let (ac, av) = cost_rank(a.cost_usd);
    let (bc, bv) = cost_rank(b.cost_usd);
    if ac != bc {
        return ac;
    }
    if ac && av != bv {
        return av >= bv;
    }
    let (at, atv) = token_rank(a.tokens);
    let (bt, btv) = token_rank(b.tokens);
    if at != bt {
        return at;
    }
    if at && atv != btv {
        return atv >= btv;
    }
    a.key <= b.key
}

fn cost_unknown(c: Option<f64>) -> bool {
    match c {
        None => true,
        Some(v) => !(v.is_finite() && v >= 0.0),
    }
}

#[test]
fn fuzz_never_panics_and_keeps_invariants() {
    let mut lcg = Lcg(0x9E37_79B9_7F4A_7C15);
    for _ in 0..2000 {
        let rows: Vec<UsageRow> = (0..lcg.below(60) as usize)
            .map(|_| rand_row(&mut lcg))
            .collect();
        let limits: Vec<LimitWindow> = (0..lcg.below(10) as usize)
            .map(|_| LimitWindow {
                provider: rand_string(&mut lcg, false),
                label: rand_string(&mut lcg, false),
                used_percent: if lcg.coin() {
                    Some(rand_f64(&mut lcg))
                } else {
                    None
                },
                resets_at_s: if lcg.coin() {
                    Some((lcg.next() as i64) ^ (lcg.next() as i64))
                } else {
                    None
                },
            })
            .collect();
        let now = lcg.next() as i64;
        let offset = (lcg.next() % 200_000) as i32 - 100_000;

        let dash = match build_dashboard(&rows, &limits, now, offset) {
            Ok(d) => d,
            Err(DashError::InvalidUtcOffset(_)) => {
                assert!(!(-50_400..=50_400).contains(&offset));
                continue;
            }
        };

        // Invariant: exactly 24 hourly buckets.
        assert_eq!(dash.hourly.len(), 24);

        // Invariant: <= 21 cost lines per table (20 + possible "Other").
        assert!(dash.by_model.len() <= 21);
        assert!(dash.by_project.len() <= 21);

        // Invariant: heatmap total <= distinct (session, start) rows in 28d.
        let cutoff28 = now.saturating_sub(28 * 86_400);
        let mut pairs: HashSet<(&str, i64)> = HashSet::new();
        for r in &rows {
            if r.started_at_s >= cutoff28 {
                pairs.insert((r.session_id.as_str(), r.started_at_s));
            }
        }
        let hm_total: u32 = dash.heatmap.iter().flatten().sum();
        assert!(hm_total as usize <= pairs.len());

        // Invariant: each hourly bucket's tokens/sessions match an independent
        // recomputation over kept rows (drop empty provider/model and future).
        let future_cutoff = now.saturating_add(60);
        for (k, b) in dash.hourly.iter().enumerate() {
            let lh = time::local_hour_index(now, offset) - 23 + k as i64;
            let mut any_known = false;
            let mut want_tokens: u64 = 0;
            let mut want_sessions: HashSet<&str> = HashSet::new();
            for r in &rows {
                if r.provider.is_empty() || r.model.is_empty() {
                    continue;
                }
                if r.started_at_s > future_cutoff {
                    continue;
                }
                if time::local_hour_index(r.started_at_s, offset) != lh {
                    continue;
                }
                want_sessions.insert(r.session_id.as_str());
                if let Some(t) = r.input_tokens {
                    any_known = true;
                    want_tokens = want_tokens.saturating_add(t);
                }
                if let Some(t) = r.output_tokens {
                    any_known = true;
                    want_tokens = want_tokens.saturating_add(t);
                }
            }
            assert_eq!(b.sessions as usize, want_sessions.len(), "hour {k}");
            if any_known {
                assert_eq!(b.tokens, Some(want_tokens), "hour {k}");
            } else {
                assert_eq!(b.tokens, None, "hour {k}");
            }
        }

        // Invariant: limits sorted by provider then label.
        for w in dash.limits.windows(2) {
            let (p0, l0) = (w[0].provider.as_str(), w[0].label.as_str());
            let (p1, l1) = (w[1].provider.as_str(), w[1].label.as_str());
            assert!(p0 < p1 || (p0 == p1 && l0 <= l1));
        }

        // Invariant: cost lines sorted (cost desc, None last, tokens desc, key).
        for w in dash.by_model.windows(2) {
            assert!(
                line_le(&w[0], &w[1]),
                "by_model sort at {} {:?} {:?}",
                w[0].key,
                w[0],
                w[1]
            );
        }
        for w in dash.by_project.windows(2) {
            assert!(
                line_le(&w[0], &w[1]),
                "by_project sort at {} {:?} {:?}",
                w[0].key,
                w[0],
                w[1]
            );
        }

        // Invariant: no fabricated zeros — if every row everywhere lacks known
        // tokens, every cost line reports tokens None.
        if rows
            .iter()
            .all(|r| r.input_tokens.is_none() && r.output_tokens.is_none())
        {
            assert!(dash.by_model.iter().all(|l| l.tokens.is_none()));
            assert!(dash.by_project.iter().all(|l| l.tokens.is_none()));
            assert!(dash.hourly.iter().all(|b| b.tokens.is_none()));
        }

        // Invariant: if every row everywhere has unknown/invalid cost, every
        // cost line reports cost None, and spendToday is None when there is at
        // least one (kept) row today.
        if rows.iter().all(|r| cost_unknown(r.cost_usd)) {
            assert!(dash.by_model.iter().all(|l| l.cost_usd.is_none()));
            assert!(dash.by_project.iter().all(|l| l.cost_usd.is_none()));
            let today = time::local_day_index(now, offset);
            let any_kept_today = rows.iter().any(|r| {
                !r.provider.is_empty()
                    && !r.model.is_empty()
                    && r.started_at_s <= future_cutoff
                    && time::local_day_index(r.started_at_s, offset) == today
            });
            if any_kept_today {
                assert_eq!(dash.spend_today_usd, None);
            }
        }

        // Invariant: cost lines are capped at 20 when there are >20 groups,
        // and the last line is "Other" exactly then.
        if dash.by_model.len() == 21 {
            assert_eq!(dash.by_model[20].key, "Other");
        }
        if dash.by_project.len() == 21 {
            assert_eq!(dash.by_project[20].key, "Other");
        }
    }
}
