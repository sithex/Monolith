//! The [`Tracker`] state machine: keeps the previous snapshot of Running
//! sessions, turns a fresh batch of rows into chips and transition events,
//! and counts warnings. Pure: time is always an injected parameter.

use std::collections::{BTreeMap, BTreeSet};

use crate::classify::classify;
use crate::model::{Chip, Event, RunState, SessionRow, TrackerConfig, Update, WarningKind};

/// Sources that are always shown as top-level chips (non-delegated).
const TOP_LEVEL_SOURCES: [&str; 4] = ["local", "tui", "cli", "desktop"];

/// The tracker state. Create with [`Tracker::new`] (default config) or
/// [`Tracker::with_config`].
#[derive(Debug, Clone)]
pub struct Tracker {
    cfg: TrackerConfig,
    /// Ids that were Running in the previous update.
    prev_running: BTreeSet<String>,
    /// Ids ever seen Running (suppresses startup Finished/Stalled spam and
    /// duplicate Started events).
    seen_running: BTreeSet<String>,
    /// Cumulative warnings from all updates so far.
    warnings: Vec<WarningKind>,
}

impl Tracker {
    /// A tracker with the default config.
    pub fn new() -> Self {
        Self::with_config(TrackerConfig::default())
    }

    /// A tracker with an explicit (already-validated) config.
    pub fn with_config(cfg: TrackerConfig) -> Self {
        Self {
            cfg,
            prev_running: BTreeSet::new(),
            seen_running: BTreeSet::new(),
            warnings: Vec::new(),
        }
    }

    /// The config this tracker runs with.
    pub fn config(&self) -> &TrackerConfig {
        &self.cfg
    }

    /// All warnings accumulated across updates so far.
    pub fn warnings(&self) -> &[WarningKind] {
        &self.warnings
    }

    /// Drain accumulated warnings (resets the warning log).
    pub fn take_warnings(&mut self) -> Vec<WarningKind> {
        std::mem::take(&mut self.warnings)
    }

    /// Process a batch of rows at `now_secs` and produce chips + events.
    pub fn update(&mut self, rows: Vec<SessionRow>, now_secs: f64) -> Update {
        // --- 1. validate, lookback-filter, dedupe (keep latest activity) ---
        let cutoff = now_secs - self.cfg.lookback_secs() as f64;
        let mut kept: Vec<SessionRow> = Vec::new();
        let mut dup_warned: BTreeSet<String> = BTreeSet::new();

        for row in rows {
            if let RunState::Rejected { reason } = classify(&row, now_secs, &self.cfg) {
                self.warnings.push(WarningKind::InvalidTime {
                    id: row.id.clone(),
                    reason,
                });
                continue;
            }
            if row.started_at < cutoff {
                continue; // outside lookback: ignored
            }
            match kept.iter().position(|k| k.id == row.id) {
                Some(idx) => {
                    if !dup_warned.contains(&row.id) {
                        self.warnings
                            .push(WarningKind::DuplicateId { id: row.id.clone() });
                        dup_warned.insert(row.id.clone());
                    }
                    if better_than(&row, &kept[idx]) {
                        kept[idx] = row;
                    }
                }
                None => kept.push(row),
            }
        }

        // --- 2. classify survivors ---
        let mut present: BTreeMap<String, (SessionRow, RunState)> = BTreeMap::new();
        for row in kept {
            let state = classify(&row, now_secs, &self.cfg);
            present.insert(row.id.clone(), (row, state));
        }

        // --- 3. events ---
        let mut events: Vec<Event> = Vec::new();

        // Started on first sight as Running; clock-skew warnings.
        for (id, (_, state)) in &present {
            match state {
                RunState::Running => {
                    if !self.seen_running.contains(id) {
                        self.seen_running.insert(id.clone());
                        events.push(Event::Started { id: id.clone() });
                    }
                }
                RunState::Stale { clock_skew: true } => {
                    self.warnings
                        .push(WarningKind::ClockSkew { id: id.clone() });
                }
                _ => {}
            }
        }

        // Transitions out of the previous Running snapshot.
        for id in &self.prev_running {
            match present.get(id) {
                None => events.push(Event::Disappeared { id: id.clone() }),
                Some((row, state)) => match state {
                    RunState::Running => {}
                    RunState::Stale { .. } => {
                        events.push(Event::Stalled { id: id.clone() });
                    }
                    RunState::Finished { .. } => {
                        let elapsed = match row.ended_at {
                            Some(e) => e - row.started_at,
                            None => 0.0, // unreachable: Finished implies endedAt set
                        };
                        events.push(Event::Finished {
                            id: id.clone(),
                            reason: row.end_reason.clone(),
                            elapsed_secs: elapsed,
                        });
                    }
                    RunState::Rejected { .. } => {}
                },
            }
        }

        let next_prev_running: BTreeSet<String> = present
            .iter()
            .filter(|(_, (_, state))| matches!(state, RunState::Running))
            .map(|(id, _)| id.clone())
            .collect();
        self.prev_running = next_prev_running;

        // --- 4. chips ---
        // Folding rule: a running row folds into its parent chip iff the
        // parent is itself a running chip. Chip membership is the parity of
        // the distance along the parent-links graph: even depth = chip,
        // odd depth = folded into the parent chip. Cycles in that graph are
        // pathological (a session whose parent chain loops); members of a
        // cycle are treated as not chips, deterministically.
        let candidate: BTreeSet<String> = present
            .iter()
            .filter(|(_, (row, state))| {
                matches!(state, RunState::Running) && self.chip_eligible(row)
            })
            .map(|(id, _)| id.clone())
            .collect();

        let mut parent_of: BTreeMap<String, String> = BTreeMap::new();
        for (id, (row, state)) in &present {
            if !candidate.contains(id) || !matches!(state, RunState::Running) {
                continue;
            }
            if let Some(p) = &row.parent_session_id {
                if p != id && candidate.contains(p) {
                    parent_of.insert(id.clone(), p.clone());
                }
            }
        }

        // Cycle detection over the functional parent graph (0=white,1=gray,2=black).
        let mut color: BTreeMap<String, u8> = BTreeMap::new();
        let mut in_cycle: BTreeSet<String> = BTreeSet::new();
        for start in &candidate {
            if color.get(start) == Some(&2) {
                continue;
            }
            let mut path: Vec<String> = Vec::new();
            let mut pos: BTreeMap<String, usize> = BTreeMap::new();
            let mut cur = start.clone();
            loop {
                match color.get(&cur) {
                    Some(&2) => break,
                    Some(&1) => {
                        let start_idx = pos.get(&cur).copied().unwrap_or(0);
                        for id in &path[start_idx..] {
                            in_cycle.insert(id.clone());
                        }
                        break;
                    }
                    _ => {}
                }
                color.insert(cur.clone(), 1);
                pos.insert(cur.clone(), path.len());
                path.push(cur.clone());
                match parent_of.get(&cur) {
                    Some(p) => cur = p.clone(),
                    None => break,
                }
            }
            for id in &path {
                color.insert(id.clone(), 2);
            }
        }

        // Depth from the nearest anchor. Anchors are roots (no candidate
        // parent; they are chips at depth 0) and cycle members (not chips;
        // their children start at depth 0). Every hop away from an anchor
        // adds 1, so chip membership alternates along a parent chain.
        let mut depth: BTreeMap<String, usize> = BTreeMap::new();
        for start in &candidate {
            if in_cycle.contains(start) || depth.contains_key(start) {
                continue;
            }
            let mut chain: Vec<String> = Vec::new();
            let mut cur = start.clone();
            let tail_depth = loop {
                if in_cycle.contains(&cur) {
                    break 0usize; // cycle anchor: child depth 0
                }
                if let Some(d) = depth.get(&cur) {
                    break d + 1;
                }
                match parent_of.get(&cur) {
                    None => {
                        // root anchor: it is a chip at depth 0
                        depth.insert(cur.clone(), 0);
                        break 1usize;
                    }
                    Some(p) => {
                        chain.push(cur.clone());
                        cur = p.clone();
                    }
                }
            };
            // chain[0] is the start; chain[last] is closest to the anchor.
            for (i, cid) in chain.iter().enumerate() {
                depth.insert(cid.clone(), tail_depth + (chain.len() - 1 - i));
            }
        }

        let mut raw_chips: Vec<(f64, Chip)> = Vec::new();
        let mut chip_index: BTreeMap<String, usize> = BTreeMap::new();
        for id in &candidate {
            let even = depth.get(id).is_some_and(|d| d % 2 == 0);
            if !even {
                continue;
            }
            let Some((row, _)) = present.get(id) else {
                continue;
            };
            let idx = raw_chips.len();
            chip_index.insert(id.clone(), idx);
            raw_chips.push((row.started_at, self.make_chip(row, now_secs)));
        }
        for id in &candidate {
            let even = depth.get(id).is_some_and(|d| d % 2 == 0);
            if even {
                continue;
            }
            let Some((row, _)) = present.get(id) else {
                continue;
            };
            if let Some(p) = &row.parent_session_id {
                if let Some(&pi) = chip_index.get(p) {
                    raw_chips[pi].1.children = raw_chips[pi].1.children.saturating_add(1);
                }
            }
        }

        // Newest first (startedAt desc, stable), then cap + overflow.
        raw_chips.sort_by(|a, b| b.0.total_cmp(&a.0));
        let max = self.cfg.max_chips() as usize;
        let overflow = raw_chips.len().saturating_sub(max).min(u32::MAX as usize) as u32;
        let chips: Vec<Chip> = raw_chips
            .into_iter()
            .take(max)
            .map(|(_, chip)| chip)
            .collect();

        Update {
            chips,
            events,
            overflow,
        }
    }

    fn chip_eligible(&self, row: &SessionRow) -> bool {
        self.cfg.is_delegated_source(&row.source)
            || TOP_LEVEL_SOURCES.contains(&row.source.as_str())
    }

    fn make_chip(&self, row: &SessionRow, now_secs: f64) -> Chip {
        Chip {
            id: row.id.clone(),
            label: label_for(row),
            model: row.model.clone(),
            provider: row.billing_provider.clone(),
            elapsed_secs: now_secs - row.started_at,
            api_call_count: row.api_call_count,
            is_delegated: self.cfg.is_delegated_source(&row.source),
            children: 0,
        }
    }
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `a` should replace `b` among duplicate ids: latest
/// `lastActivityAt` wins; ties break toward the latest `startedAt`; full
/// ties keep the first-seen row.
fn better_than(a: &SessionRow, b: &SessionRow) -> bool {
    fn cmp_opt(x: Option<f64>, y: Option<f64>) -> std::cmp::Ordering {
        match (x, y) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(xv), Some(yv)) => xv.total_cmp(&yv),
        }
    }
    let ord = cmp_opt(a.last_activity_at, b.last_activity_at)
        .then_with(|| a.started_at.total_cmp(&b.started_at));
    ord == std::cmp::Ordering::Greater
}

/// Title, else model, else short id (first 8 chars after a `local-`
/// prefix). Empty titles/models fall through to the next candidate.
pub fn label_for(row: &SessionRow) -> String {
    if let Some(title) = &row.title {
        if !title.is_empty() {
            return title.clone();
        }
    }
    if let Some(model) = &row.model {
        if !model.is_empty() {
            return model.clone();
        }
    }
    short_id(&row.id)
}

/// Short id: strip a leading `local-` prefix, then take the first 8 chars
/// (char-boundary safe).
pub fn short_id(id: &str) -> String {
    id.strip_prefix("local-")
        .unwrap_or(id)
        .chars()
        .take(8)
        .collect()
}
