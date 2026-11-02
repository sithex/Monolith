//! The aggregation core: turns decoded rows + limit windows into a
//! [`Dashboard`].

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::error::DashError;
use crate::time;
use crate::types::{
    ActiveSession, CostLine, Dashboard, HourBucket, LimitCard, LimitWindow, UsageRow,
};

const MAX_OFFSET_S: i32 = 14 * 3600; // ±14h
const FUTURE_TOLERANCE_S: i64 = 60;
const ACTIVE_WINDOW_S: i64 = 6 * 3600;
const DAY_S: i64 = 86_400;
const THIRTY_DAYS_S: i64 = 30 * DAY_S;
const TWENTY_EIGHT_DAYS_S: i64 = 28 * DAY_S;
const MAX_COST_LINES: usize = 20;
const OTHER_KEY: &str = "Other";
const NO_PROJECT_KEY: &str = "(no project)";

/// Build the full dashboard. `utc_offset_s` must be within −14h..=+14h.
pub fn build_dashboard(
    rows: &[UsageRow],
    limits: &[LimitWindow],
    now_s: i64,
    utc_offset_s: i32,
) -> Result<Dashboard, DashError> {
    if !(-MAX_OFFSET_S..=MAX_OFFSET_S).contains(&utc_offset_s) {
        return Err(DashError::InvalidUtcOffset(utc_offset_s));
    }
    let (kept, rows_ignored) = classify_rows(rows, now_s);
    let (spend_today_usd, spend_today_partial) = spend_today(&kept, now_s, utc_offset_s);
    let active = active_session(&kept, now_s);
    let hourly = hourly_buckets(&kept, now_s, utc_offset_s);
    let by_model = cost_table(&kept, now_s, |r| r.model.clone());
    let by_project = cost_table(&kept, now_s, |r| {
        r.project
            .clone()
            .unwrap_or_else(|| NO_PROJECT_KEY.to_string())
    });
    let heatmap = activity_heatmap(&kept, now_s, utc_offset_s);

    Ok(Dashboard {
        limits: limit_cards(limits, now_s),
        spend_today_usd,
        spend_today_partial,
        active,
        hourly,
        by_model,
        by_project,
        heatmap,
        rows_ignored,
    })
}

/// Drop future / empty-provider rows and count them; keep rows with invalid
/// cost (their cost is treated as unknown) but still count them.
fn classify_rows(rows: &[UsageRow], now_s: i64) -> (Vec<&UsageRow>, u32) {
    let mut kept = Vec::with_capacity(rows.len());
    let mut ignored = 0u32;
    let future_cutoff = now_s.saturating_add(FUTURE_TOLERANCE_S);
    for r in rows {
        if r.provider.is_empty() || r.model.is_empty() || r.started_at_s > future_cutoff {
            ignored += 1;
        } else {
            match r.cost_usd {
                Some(c) if !(c.is_finite() && c >= 0.0) => ignored += 1,
                _ => {}
            }
            kept.push(r);
        }
    }
    (kept, ignored)
}

/// Finite, non-negative cost of a kept row, or None when unknown/invalid.
fn normalized_cost(r: &UsageRow) -> Option<f64> {
    match r.cost_usd {
        Some(c) if c.is_finite() && c >= 0.0 => Some(c),
        _ => None,
    }
}

/// Saturating sum for non-negative finite f64 (avoids +inf overflow).
fn add_cost(sum: f64, v: f64) -> f64 {
    if v > f64::MAX - sum {
        f64::MAX
    } else {
        sum + v
    }
}

fn format_reset_text(s: i64) -> String {
    if s == 0 {
        return "now".to_string();
    }
    if s <= 59 {
        return "<1m".to_string();
    }
    if s < 3600 {
        return format!("{}m", s / 60);
    }
    if s < 86400 {
        return format!("{}h {}m", s / 3600, (s % 3600) / 60);
    }
    format!("{}d {}h", s / 86400, (s % 86400) / 3600)
}

fn limit_cards(limits: &[LimitWindow], now_s: i64) -> Vec<LimitCard> {
    let mut cards: Vec<LimitCard> = limits
        .iter()
        .map(|l| {
            let used_percent = l.used_percent.and_then(|v| {
                if v.is_finite() {
                    Some(v.clamp(0.0, 100.0))
                } else {
                    None
                }
            });
            let resets_in_s = l.resets_at_s.map(|r| r.saturating_sub(now_s).max(0));
            let reset_text = resets_in_s.map(format_reset_text);
            LimitCard {
                provider: l.provider.clone(),
                label: l.label.clone(),
                used_percent,
                resets_in_s,
                reset_text,
            }
        })
        .collect();
    cards.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.label.cmp(&b.label)));
    cards
}

/// `(spendTodayUsd, spendTodayPartial)`.
fn spend_today(kept: &[&UsageRow], now_s: i64, offset: i32) -> (Option<f64>, bool) {
    let today = time::local_day_index(now_s, offset);
    let today_rows: Vec<&&UsageRow> = kept
        .iter()
        .filter(|r| time::local_day_index(r.started_at_s, offset) == today)
        .collect();
    let mut sum = 0.0f64;
    let mut any_cost = false;
    let mut any_missing = false;
    for r in &today_rows {
        match normalized_cost(r) {
            Some(c) => {
                any_cost = true;
                sum = add_cost(sum, c);
            }
            None => any_missing = true,
        }
    }
    if today_rows.is_empty() {
        (Some(0.0), false)
    } else if !any_cost {
        (None, false)
    } else {
        (Some(sum), any_missing)
    }
}

fn active_session(kept: &[&UsageRow], now_s: i64) -> Option<ActiveSession> {
    let cutoff = now_s.saturating_sub(ACTIVE_WINDOW_S);
    let mut best: Option<&UsageRow> = None;
    for r in kept {
        if r.ended_at_s.is_none() && r.started_at_s >= cutoff {
            match best {
                None => best = Some(r),
                Some(b) => {
                    let ord = r
                        .started_at_s
                        .cmp(&b.started_at_s)
                        .then_with(|| r.session_id.cmp(&b.session_id));
                    if ord == Ordering::Greater {
                        best = Some(r);
                    }
                }
            }
        }
    }
    best.map(|r| ActiveSession {
        session_id: r.session_id.clone(),
        provider: r.provider.clone(),
        model: r.model.clone(),
        started_at_s: r.started_at_s,
    })
}

fn hourly_buckets(kept: &[&UsageRow], now_s: i64, offset: i32) -> Vec<HourBucket> {
    let cur_hour = time::local_hour_index(now_s, offset);
    let mut buckets = Vec::with_capacity(24);
    for k in 0..24usize {
        let lh = cur_hour - 23 + k as i64;
        let start = time::hour_start_utc(lh, offset);
        let mut tokens_sum = 0u64;
        let mut any_known_token = false;
        let mut sessions: HashSet<&str> = HashSet::new();
        for r in kept {
            if time::local_hour_index(r.started_at_s, offset) == lh {
                if let Some(t) = r.input_tokens {
                    tokens_sum = tokens_sum.saturating_add(t);
                    any_known_token = true;
                }
                if let Some(t) = r.output_tokens {
                    tokens_sum = tokens_sum.saturating_add(t);
                    any_known_token = true;
                }
                sessions.insert(r.session_id.as_str());
            }
        }
        buckets.push(HourBucket {
            start_s: start,
            tokens: if any_known_token {
                Some(tokens_sum)
            } else {
                None
            },
            sessions: sessions.len() as u32,
        });
    }
    buckets
}

struct GroupAcc {
    tokens: u64,
    any_token: bool,
    cost: f64,
    any_cost: bool,
    rows: u32,
}

fn cost_table<F>(kept: &[&UsageRow], now_s: i64, key_of: F) -> Vec<CostLine>
where
    F: Fn(&UsageRow) -> String,
{
    let cutoff = now_s.saturating_sub(THIRTY_DAYS_S);
    let mut order: Vec<String> = Vec::new();
    let mut acc: HashMap<String, GroupAcc> = HashMap::new();
    for r in kept {
        if r.started_at_s < cutoff {
            continue;
        }
        let key = key_of(r);
        if !acc.contains_key(&key) {
            order.push(key.clone());
        }
        let g = acc.entry(key).or_insert(GroupAcc {
            tokens: 0,
            any_token: false,
            cost: 0.0,
            any_cost: false,
            rows: 0,
        });
        if let Some(t) = r.input_tokens {
            g.tokens = g.tokens.saturating_add(t);
            g.any_token = true;
        }
        if let Some(t) = r.output_tokens {
            g.tokens = g.tokens.saturating_add(t);
            g.any_token = true;
        }
        if let Some(c) = normalized_cost(r) {
            g.cost = add_cost(g.cost, c);
            g.any_cost = true;
        }
        g.rows += 1;
    }

    let mut lines: Vec<CostLine> = order
        .iter()
        .map(|k| {
            let g = &acc[k];
            CostLine {
                key: k.clone(),
                tokens: if g.any_token { Some(g.tokens) } else { None },
                cost_usd: if g.any_cost { Some(g.cost) } else { None },
                rows: g.rows,
            }
        })
        .collect();
    lines.sort_by(cmp_cost_line);

    if lines.len() > MAX_COST_LINES {
        let tail = lines.split_off(MAX_COST_LINES);
        let other = tail.into_iter().fold(
            GroupAcc {
                tokens: 0,
                any_token: false,
                cost: 0.0,
                any_cost: false,
                rows: 0,
            },
            |mut a, l| {
                if let Some(t) = l.tokens {
                    a.tokens = a.tokens.saturating_add(t);
                    a.any_token = true;
                }
                if let Some(c) = l.cost_usd {
                    a.cost = add_cost(a.cost, c);
                    a.any_cost = true;
                }
                a.rows += l.rows;
                a
            },
        );
        lines.push(CostLine {
            key: OTHER_KEY.to_string(),
            tokens: if other.any_token {
                Some(other.tokens)
            } else {
                None
            },
            cost_usd: if other.any_cost {
                Some(other.cost)
            } else {
                None
            },
            rows: other.rows,
        });
    }
    lines
}

fn cmp_cost_line(a: &CostLine, b: &CostLine) -> Ordering {
    opt_f64_desc(a.cost_usd, b.cost_usd)
        .then_with(|| opt_u64_desc(a.tokens, b.tokens))
        .then_with(|| a.key.cmp(&b.key))
}

/// Descending, `None` sorts last.
fn opt_f64_desc(a: Option<f64>, b: Option<f64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Descending, `None` sorts last.
fn opt_u64_desc(a: Option<u64>, b: Option<u64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn activity_heatmap(kept: &[&UsageRow], now_s: i64, offset: i32) -> [[u32; 24]; 7] {
    let cutoff = now_s.saturating_sub(TWENTY_EIGHT_DAYS_S);
    let mut hm = [[0u32; 24]; 7];
    let mut seen: HashSet<(&str, i64)> = HashSet::new();
    for r in kept {
        if r.started_at_s < cutoff {
            continue;
        }
        if !seen.insert((r.session_id.as_str(), r.started_at_s)) {
            continue;
        }
        let (wd, hour) = time::local_weekday_and_hour(r.started_at_s, offset);
        hm[wd as usize][hour as usize] += 1;
    }
    hm
}
