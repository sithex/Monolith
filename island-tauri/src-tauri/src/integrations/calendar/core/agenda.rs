//! Agenda expansion over a time window. See the crate docs.

use crate::integrations::calendar::core::civil::{days_in_month, naive_to_unix, NaiveDateTime};
use crate::integrations::calendar::core::error::{IcsError, Warning};
use crate::integrations::calendar::core::ics::{Event, Freq, Weekday};
use crate::integrations::calendar::core::value::DateTimeKind;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_WINDOW_SECS: i64 = 400 * 86_400;
const MAX_INSTANCES: usize = 2000;
const MAX_CANDIDATES_PER_RULE: usize = 10_000;

/// One expanded occurrence overlapping the requested window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub uid: String,
    pub summary: String,
    pub location: Option<String>,
    pub start_s: i64,
    pub end_s: i64,
    pub all_day: bool,
    /// When this instance is a RECURRENCE-ID override, the UTC start of the
    /// original instance it replaced.
    pub recurrence_id: Option<i64>,
}

struct Ctx<'a> {
    local_offset: i32,
    /// Phase 6b2: DST-aware local zone for floating/date values (falls back to `local_offset`).
    local: Option<&'a dyn Fn(NaiveDateTime) -> Option<i32>>,
    tz: &'a dyn Fn(&str, NaiveDateTime) -> Option<i32>,
    warnings: Vec<Warning>,
    warned_tzids: HashSet<String>,
}

impl Ctx<'_> {
    fn local_at(&self, naive: NaiveDateTime) -> i32 {
        self.local
            .and_then(|f| f(naive))
            .unwrap_or(self.local_offset)
    }
    fn offset(&mut self, kind: &DateTimeKind, naive: NaiveDateTime) -> i32 {
        match kind {
            DateTimeKind::Utc => 0,
            DateTimeKind::Floating | DateTimeKind::Date => self.local_at(naive),
            DateTimeKind::Zoned { tzid } => match (self.tz)(tzid, naive) {
                Some(off) => off,
                None => {
                    if self.warned_tzids.insert(tzid.clone()) {
                        self.warnings.push(Warning::new(format!(
                            "unknown TZID `{tzid}`; treated as floating (local offset)"
                        )));
                    }
                    self.local_at(naive)
                }
            },
        }
    }
}

struct Computed {
    start_s: i64,
    end_s: i64,
    all_day: bool,
}

fn compute(ev: &Event, ctx: &mut Ctx) -> Computed {
    let start_kind = &ev.dtstart.kind;
    let start_naive = ev.dtstart.naive;
    let start_off = ctx.offset(start_kind, start_naive);
    let start_s = naive_to_unix(start_naive) - start_off as i64;
    let all_day = matches!(start_kind, DateTimeKind::Date);
    let end_s = if let Some(de) = &ev.dtend {
        let end_off = ctx.offset(&de.kind, de.naive);
        naive_to_unix(de.naive) - end_off as i64
    } else if let Some(dur) = ev.duration_s {
        start_s.saturating_add(dur)
    } else if all_day {
        // Phase 6b2: next local midnight (a DST day is 23 or 25 hours long).
        let next = start_naive.add_days(1);
        naive_to_unix(next) - ctx.offset(start_kind, next) as i64
    } else {
        start_s
    };
    Computed {
        start_s,
        end_s,
        all_day,
    }
}

/// Generate candidate naive start-times for an event's recurrence rule
/// (or just DTSTART for a non-recurring event). Excludes nothing; EXDATE
/// filtering happens in the caller. Stops after `MAX_CANDIDATES_PER_RULE`
/// generated candidates per rule.
fn expand_rule(ev: &Event) -> Vec<NaiveDateTime> {
    let base = ev.dtstart.naive;
    let Some(rr) = &ev.rrule else {
        return vec![base];
    };
    let mut out: Vec<NaiveDateTime> = Vec::new();
    let until = rr.until;
    if until.is_some_and(|u| base > u) {
        return out;
    }
    out.push(base);
    let count = rr.count.map(|c| c as usize).unwrap_or(usize::MAX);
    if out.len() >= count {
        return out;
    }
    let mut tried = 0usize;
    match rr.freq {
        Freq::Daily => {
            let mut cur = base;
            while tried < MAX_CANDIDATES_PER_RULE && out.len() < count {
                cur = cur.add_days(rr.interval as i64);
                tried += 1;
                if until.is_some_and(|u| cur > u) {
                    break;
                }
                out.push(cur);
            }
        }
        Freq::Weekly => {
            let base_wd = base.weekday_number();
            let anchor = base.add_days(-(base_wd as i64));
            let byday = rr
                .byday
                .clone()
                .unwrap_or_else(|| vec![Weekday::from_number(base_wd)]);
            let mut days: Vec<Weekday> = byday;
            days.sort_by_key(|d| d.idx());
            let mut last = base;
            let mut week = 0i64;
            while tried < MAX_CANDIDATES_PER_RULE && out.len() < count {
                let week_start = anchor.add_days(week * (rr.interval as i64) * 7);
                for d in &days {
                    tried += 1;
                    if tried > MAX_CANDIDATES_PER_RULE {
                        break;
                    }
                    let cand = week_start.add_days(d.idx());
                    if cand <= last {
                        continue;
                    }
                    if until.is_some_and(|u| cand > u) {
                        return out;
                    }
                    out.push(cand);
                    last = cand;
                    if out.len() >= count {
                        return out;
                    }
                }
                week += 1;
            }
        }
        Freq::Monthly | Freq::Yearly => {
            let years = matches!(rr.freq, Freq::Yearly);
            let mut cur = base;
            while tried < MAX_CANDIDATES_PER_RULE && out.len() < count {
                let months = if years {
                    rr.interval as i64 * 12
                } else {
                    rr.interval as i64
                };
                let (y, m) = cur.months_later(months);
                tried += 1;
                if base.day <= days_in_month(y, m) {
                    let cand = NaiveDateTime {
                        year: y,
                        month: m,
                        day: base.day,
                        hour: base.hour,
                        minute: base.minute,
                        second: base.second,
                    };
                    if until.is_some_and(|u| cand > u) {
                        break;
                    }
                    out.push(cand);
                }
                cur = NaiveDateTime {
                    year: y,
                    month: m,
                    day: 1,
                    hour: base.hour,
                    minute: base.minute,
                    second: base.second,
                };
            }
        }
    }
    out
}

/// Checked agenda expansion; also returns the warnings collected while
/// expanding (e.g. unknown TZIDs treated as floating).
pub fn agenda_checked_with_warnings(
    events: &[Event],
    window_start_s: i64,
    window_end_s: i64,
    local_offset_s: i32,
    tz: &dyn Fn(&str, NaiveDateTime) -> Option<i32>,
) -> Result<(Vec<Instance>, Vec<Warning>), IcsError> {
    agenda_zoned(
        events,
        window_start_s,
        window_end_s,
        local_offset_s,
        None,
        tz,
    )
}

/// Like [`agenda_checked_with_warnings`] but floating/date values resolve their offset through
/// `local` per wall-clock time (DST-aware); `local_offset_s` is the fallback when it yields `None`.
pub fn agenda_zoned(
    events: &[Event],
    window_start_s: i64,
    window_end_s: i64,
    local_offset_s: i32,
    local: Option<&dyn Fn(NaiveDateTime) -> Option<i32>>,
    tz: &dyn Fn(&str, NaiveDateTime) -> Option<i32>,
) -> Result<(Vec<Instance>, Vec<Warning>), IcsError> {
    if window_end_s <= window_start_s {
        return Ok((Vec::new(), Vec::new()));
    }
    if (window_end_s as i128 - window_start_s as i128) > MAX_WINDOW_SECS as i128 {
        return Err(IcsError::WindowTooLong);
    }
    let mut ctx = Ctx {
        local_offset: local_offset_s,
        local,
        tz,
        warnings: Vec::new(),
        warned_tzids: HashSet::new(),
    };
    let mut out: Vec<Instance> = Vec::new();

    // Map (uid, recurrence-id naive) -> index of the override event.
    let mut omap: HashMap<(String, NaiveDateTime), usize> = HashMap::new();
    for (i, ev) in events.iter().enumerate() {
        if let Some(rid) = &ev.recurrence_id {
            omap.insert((ev.uid.clone(), rid.naive), i);
        }
    }
    let mut used_overrides: HashSet<usize> = HashSet::new();

    for ev in events.iter() {
        if ev.recurrence_id.is_some() || ev.cancelled {
            continue;
        }
        let exset: HashSet<NaiveDateTime> = ev.exdates.iter().map(|x| x.naive).collect();
        // Baseline instance (DTSTART) gives the shape: all-day-ness and the
        // fixed duration every recurrence shares (an absolute DTEND shifts by
        // the same delta as DTSTART).
        let base = compute(ev, &mut ctx);
        let start_kind = &ev.dtstart.kind;
        for cand in expand_rule(ev) {
            let oi = omap.get(&(ev.uid.clone(), cand)).copied();
            if let Some(oi) = oi {
                let eff = &events[oi];
                if eff.cancelled {
                    used_overrides.insert(oi);
                    continue;
                }
                let comp = compute(eff, &mut ctx);
                if comp.start_s >= window_end_s || comp.end_s <= window_start_s {
                    continue;
                }
                used_overrides.insert(oi);
                let recurrence_id = match &eff.recurrence_id {
                    Some(rid) => {
                        let rid_off = ctx.offset(&rid.kind, rid.naive);
                        naive_to_unix(rid.naive) - rid_off as i64
                    }
                    None => base.start_s,
                };
                out.push(Instance {
                    uid: eff.uid.clone(),
                    summary: eff.summary.clone(),
                    location: eff.location.clone(),
                    start_s: comp.start_s,
                    end_s: comp.end_s,
                    all_day: comp.all_day,
                    recurrence_id: Some(recurrence_id),
                });
            } else {
                if exset.contains(&cand) {
                    continue;
                }
                // Overlap must be judged per candidate, not on DTSTART: a
                // rule whose DTSTART precedes the window still contributes
                // later occurrences.
                let cand_off = ctx.offset(start_kind, cand);
                let cs = naive_to_unix(cand) - cand_off as i64;
                let ce = if let Some(end) = &ev.dtend {
                    // DTSTART/DTEND describe wall times. Preserve their civil displacement,
                    // resolving the end of EACH occurrence in its own zone (not a fixed
                    // elapsed duration copied from the master's DST offset).
                    let shift_days = cand.days() - ev.dtstart.naive.days();
                    let end_naive = end.naive.add_days(shift_days);
                    naive_to_unix(end_naive) - ctx.offset(&end.kind, end_naive) as i64
                } else if let Some(duration) = ev.duration_s {
                    cs.saturating_add(duration)
                } else if base.all_day {
                    let next = cand.add_days(1);
                    naive_to_unix(next) - ctx.offset(start_kind, next) as i64
                } else {
                    cs
                };
                if cs >= window_end_s || ce <= window_start_s {
                    continue;
                }
                out.push(Instance {
                    uid: ev.uid.clone(),
                    summary: ev.summary.clone(),
                    location: ev.location.clone(),
                    start_s: cs,
                    end_s: ce,
                    all_day: base.all_day,
                    recurrence_id: None,
                });
            }
        }
    }

    // Orphan overrides (no matching master instance) behave as standalone.
    for (i, ev) in events.iter().enumerate() {
        let rid = match ev.recurrence_id.as_ref() {
            Some(r) => r,
            None => continue,
        };
        if used_overrides.contains(&i) || ev.cancelled {
            continue;
        }
        let comp = compute(ev, &mut ctx);
        if comp.start_s >= window_end_s || comp.end_s <= window_start_s {
            continue;
        }
        let rid_off = ctx.offset(&rid.kind, rid.naive);
        out.push(Instance {
            uid: ev.uid.clone(),
            summary: ev.summary.clone(),
            location: ev.location.clone(),
            start_s: comp.start_s,
            end_s: comp.end_s,
            all_day: comp.all_day,
            recurrence_id: Some(naive_to_unix(rid.naive) - rid_off as i64),
        });
    }

    out.sort_by(|a, b| {
        a.start_s
            .cmp(&b.start_s)
            .then_with(|| b.all_day.cmp(&a.all_day))
            .then_with(|| a.summary.cmp(&b.summary))
            .then_with(|| a.uid.cmp(&b.uid))
    });
    out.truncate(MAX_INSTANCES);
    Ok((out, ctx.warnings))
}

/// Checked agenda expansion (warnings discarded).
pub fn agenda_checked(
    events: &[Event],
    window_start_s: i64,
    window_end_s: i64,
    local_offset_s: i32,
    tz: &dyn Fn(&str, NaiveDateTime) -> Option<i32>,
) -> Result<Vec<Instance>, IcsError> {
    agenda_checked_with_warnings(events, window_start_s, window_end_s, local_offset_s, tz)
        .map(|(instances, _)| instances)
}

/// Convenience wrapper matching the public contract signature. A window
/// longer than 400 days is a typed error in [`agenda_checked`]; this
/// convenience returns an empty agenda for such windows.
pub fn agenda(
    events: &[Event],
    window_start_s: i64,
    window_end_s: i64,
    local_offset_s: i32,
    tz: &dyn Fn(&str, NaiveDateTime) -> Option<i32>,
) -> Vec<Instance> {
    agenda_checked(events, window_start_s, window_end_s, local_offset_s, tz).unwrap_or_default()
}
