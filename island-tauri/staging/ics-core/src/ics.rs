//! iCalendar text parsing (RFC 5545 subset).
//!
//! Content lines end in CRLF (bare LF accepted). Unfolding removes a line
//! break followed by exactly one space or horizontal tab. Property syntax:
//! `NAME;PARAM=value;PARAM2="quoted":VALUE` — names and param names are
//! case-insensitive. TEXT escapes per §3.3.11.

use crate::civil::NaiveDateTime;
use crate::error::{IcsError, Warning};
use crate::value::{DateTimeKind, DateTimeValue};
use serde::{Deserialize, Serialize};

/// Recurrence frequency (RFC 5545 §3.3.10 subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Weekday in a `BYDAY` list (no numeric prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weekday {
    Mo,
    Tu,
    We,
    Th,
    Fr,
    Sa,
    Su,
}

impl Weekday {
    fn from_abbrev(s: &str) -> Option<Self> {
        Some(match s {
            "MO" => Self::Mo,
            "TU" => Self::Tu,
            "WE" => Self::We,
            "TH" => Self::Th,
            "FR" => Self::Fr,
            "SA" => Self::Sa,
            "SU" => Self::Su,
            _ => return None,
        })
    }

    pub(crate) fn from_number(wd: u32) -> Self {
        match wd % 7 {
            0 => Self::Mo,
            1 => Self::Tu,
            2 => Self::We,
            3 => Self::Th,
            4 => Self::Fr,
            5 => Self::Sa,
            _ => Self::Su,
        }
    }

    /// Offset from Monday (0 = Monday … 6 = Sunday).
    pub(crate) fn idx(self) -> i64 {
        match self {
            Self::Mo => 0,
            Self::Tu => 1,
            Self::We => 2,
            Self::Th => 3,
            Self::Fr => 4,
            Self::Sa => 5,
            Self::Su => 6,
        }
    }
}

/// Parsed RRULE subset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rrule {
    pub freq: Freq,
    /// Default 1, must be >= 1.
    pub interval: u32,
    /// `COUNT` (inclusive of DTSTART); mutually exclusive with `until`.
    pub count: Option<u32>,
    /// `UNTIL` (inclusive), compared in naive wall-clock.
    pub until: Option<NaiveDateTime>,
    /// `BYDAY` — only meaningful for WEEKLY; `None` means DTSTART's weekday.
    pub byday: Option<Vec<Weekday>>,
}

/// A parsed VEVENT (the wire type; camelCase via serde).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub uid: String,
    pub summary: String,
    pub location: Option<String>,
    pub description: Option<String>,
    pub dtstart: DateTimeValue,
    pub dtend: Option<DateTimeValue>,
    /// Parsed `DURATION` in seconds, if present and valid.
    pub duration_s: Option<i64>,
    pub rrule: Option<Rrule>,
    pub exdates: Vec<DateTimeValue>,
    pub recurrence_id: Option<DateTimeValue>,
    pub cancelled: bool,
}

struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

fn split_outside_quotes(s: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut inq = false;
    for ch in s.chars() {
        if ch == '"' {
            inq = !inq;
            cur.push(ch);
        } else if ch == sep && !inq {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

fn parse_property_line(line: &str) -> Option<Prop> {
    let mut inq = false;
    let mut colon = None;
    for (i, ch) in line.char_indices() {
        if ch == '"' {
            inq = !inq;
        } else if ch == ':' && !inq {
            colon = Some(i);
            break;
        }
    }
    let ci = colon?;
    let head = &line[..ci];
    let value = line[ci + 1..].to_string();
    if head.is_empty() {
        return None;
    }
    let (name, param_str) = match head.find(';') {
        Some(j) => (&head[..j], Some(&head[j + 1..])),
        None => (head, None),
    };
    if name.is_empty() {
        return None;
    }
    let mut params = Vec::new();
    if let Some(ps) = param_str {
        for seg in split_outside_quotes(ps, ';') {
            if seg.is_empty() {
                continue;
            }
            let eq = seg.find('=')?;
            let k = seg[..eq].trim().to_ascii_uppercase();
            let mut v = seg[eq + 1..].trim().to_string();
            if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
                v = v[1..v.len() - 1].to_string();
            }
            params.push((k, v));
        }
    }
    Some(Prop {
        name: name.to_ascii_uppercase(),
        params,
        value,
    })
}

/// Unfold content lines: a line break followed by exactly one space or tab
/// is removed together with that single whitespace character.
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut has_cur = false;
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')) {
            if has_cur {
                cur.push_str(rest);
            } else if !rest.is_empty() {
                out.push(rest.to_string());
            }
        } else {
            if has_cur {
                out.push(std::mem::take(&mut cur));
                has_cur = false;
            }
            if !line.is_empty() {
                cur.push_str(line);
                has_cur = true;
            }
        }
    }
    if has_cur {
        out.push(cur);
    }
    out
}

fn parse_digits(s: &str) -> Option<u32> {
    let mut v: u32 = 0;
    for &b in s.as_bytes() {
        let d = (b as char).to_digit(10)?;
        v = v.checked_mul(10)?.checked_add(d)?;
    }
    Some(v)
}

fn parse_date_value(raw: &str) -> Option<NaiveDateTime> {
    if raw.len() != 8 {
        return None;
    }
    let y = parse_digits(&raw[0..4])? as i32;
    let m = parse_digits(&raw[4..6])?;
    let d = parse_digits(&raw[6..8])?;
    NaiveDateTime::new(y, m, d, 0, 0, 0)
}

fn parse_datetime_value_raw(raw: &str) -> Option<NaiveDateTime> {
    let s = raw.strip_suffix('Z').unwrap_or(raw);
    if s.len() != 15 {
        return None;
    }
    let b = s.as_bytes();
    if b[8] != b'T' {
        return None;
    }
    let y = parse_digits(&s[0..4])? as i32;
    let m = parse_digits(&s[4..6])?;
    let d = parse_digits(&s[6..8])?;
    let h = parse_digits(&s[9..11])?;
    let mi = parse_digits(&s[11..13])?;
    let se = parse_digits(&s[13..15])?;
    NaiveDateTime::new(y, m, d, h, mi, se)
}

fn parse_datetime_value(params: &[(String, String)], raw: &str) -> Option<DateTimeValue> {
    let mut is_date = false;
    let mut tzid: Option<String> = None;
    for (k, v) in params {
        match k.as_str() {
            "VALUE" => {
                if v.eq_ignore_ascii_case("DATE") {
                    is_date = true;
                }
            }
            "TZID" => tzid = Some(v.clone()),
            _ => {}
        }
    }
    if is_date {
        return Some(DateTimeValue {
            naive: parse_date_value(raw)?,
            kind: DateTimeKind::Date,
        });
    }
    if raw.ends_with('Z') {
        return Some(DateTimeValue {
            naive: parse_datetime_value_raw(raw)?,
            kind: DateTimeKind::Utc,
        });
    }
    if let Some(tz) = tzid {
        return Some(DateTimeValue {
            naive: parse_datetime_value_raw(raw)?,
            kind: DateTimeKind::Zoned { tzid: tz },
        });
    }
    Some(DateTimeValue {
        naive: parse_datetime_value_raw(raw)?,
        kind: DateTimeKind::Floating,
    })
}

/// TEXT unescaping per RFC 5545 §3.3.11.
fn unescape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some(';') => out.push(';'),
                Some(',') => out.push(','),
                Some('n') | Some('N') => out.push('\n'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// DURATION (`P1D`, `PT1H30M`, `P2W`); signs rejected.
fn parse_duration(raw: &str) -> Option<i64> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let b = s.as_bytes();
    if b[0] == b'+' || b[0] == b'-' {
        return None;
    }
    let upper = s.to_ascii_uppercase();
    let rest = upper.strip_prefix('P')?;
    if rest.is_empty() {
        return None;
    }
    if let Some(num) = rest.strip_suffix('W') {
        if num.is_empty() || !num.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        return (parse_digits(num)? as i64).checked_mul(7 * 86_400);
    }
    let mut total: i64 = 0;
    let mut any = false;
    let mut rest = rest;
    let digits_end = rest.bytes().take_while(|c| c.is_ascii_digit()).count();
    if digits_end > 0 {
        if digits_end >= rest.len() {
            return None;
        }
        match rest.as_bytes()[digits_end] as char {
            'D' => {
                let v = parse_digits(&rest[..digits_end])?;
                total = total.checked_add((v as i64).checked_mul(86_400)?)?;
                rest = &rest[digits_end + 1..];
                any = true;
            }
            'T' => return None,
            _ => return None,
        }
    }
    if let Some(tail0) = rest.strip_prefix('T') {
        let mut tail = tail0;
        let mut tany = false;
        while !tail.is_empty() {
            let de = tail.bytes().take_while(|c| c.is_ascii_digit()).count();
            if de == 0 || de >= tail.len() {
                return None;
            }
            let unit = tail.as_bytes()[de] as char;
            let v = parse_digits(&tail[..de])?;
            match unit {
                'H' => total = total.checked_add((v as i64).checked_mul(3600)?)?,
                'M' => total = total.checked_add((v as i64).checked_mul(60)?)?,
                'S' => total = total.checked_add(v as i64)?,
                _ => return None,
            }
            tail = &tail[de + 1..];
            tany = true;
        }
        if !tany {
            return None;
        }
        any = true;
    } else if !rest.is_empty() {
        return None;
    }
    if !any {
        return None;
    }
    Some(total)
}

fn parse_byday(val: &str) -> Result<Vec<Weekday>, String> {
    let mut out = Vec::new();
    for tok in val.split(',') {
        let tok = tok.trim();
        if tok.is_empty() || !tok.bytes().all(|c| c.is_ascii_alphabetic()) {
            return Err(format!("numeric/unsupported BYDAY entry `{tok}`"));
        }
        out.push(
            Weekday::from_abbrev(&tok.to_ascii_uppercase())
                .ok_or_else(|| format!("bad BYDAY entry `{tok}`"))?,
        );
    }
    Ok(out)
}

fn parse_until_value(val: &str) -> Option<NaiveDateTime> {
    if val.len() == 8 && val.bytes().all(|c| c.is_ascii_digit()) {
        return parse_date_value(val);
    }
    parse_datetime_value_raw(val)
}

/// Parse the RRULE subset. Any unsupported part makes the whole rule invalid
/// (the caller then treats the event as non-recurring with a warning).
fn parse_rrule(value: &str) -> Result<Rrule, String> {
    let mut freq: Option<Freq> = None;
    let mut interval: u32 = 1;
    let mut count: Option<u32> = None;
    let mut until: Option<NaiveDateTime> = None;
    let mut byday: Option<Vec<Weekday>> = None;
    for seg in value.split(';') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        let eq = seg
            .find('=')
            .ok_or_else(|| "RRULE part missing '='".to_string())?;
        let key = seg[..eq].trim().to_ascii_uppercase();
        let val = seg[eq + 1..].trim();
        match key.as_str() {
            "FREQ" => {
                if freq.is_some() {
                    return Err("duplicate FREQ".to_string());
                }
                freq = Some(match val {
                    "DAILY" => Freq::Daily,
                    "WEEKLY" => Freq::Weekly,
                    "MONTHLY" => Freq::Monthly,
                    "YEARLY" => Freq::Yearly,
                    _ => return Err(format!("unsupported FREQ `{val}`")),
                });
            }
            "INTERVAL" => {
                if val.is_empty() {
                    return Err("empty INTERVAL".to_string());
                }
                interval = val
                    .parse::<u32>()
                    .map_err(|_| format!("bad INTERVAL `{val}`"))?;
                if interval == 0 {
                    return Err("INTERVAL must be >= 1".to_string());
                }
            }
            "COUNT" => {
                if val.is_empty() {
                    return Err("empty COUNT".to_string());
                }
                let c = val
                    .parse::<u32>()
                    .map_err(|_| format!("bad COUNT `{val}`"))?;
                if c == 0 {
                    return Err("COUNT must be >= 1".to_string());
                }
                count = Some(c);
            }
            "UNTIL" => {
                until = Some(parse_until_value(val).ok_or_else(|| format!("bad UNTIL `{val}`"))?);
            }
            "BYDAY" => {
                byday = Some(parse_byday(val)?);
            }
            "WKST" => {
                // ignored
            }
            other => return Err(format!("unsupported RRULE part `{other}`")),
        }
    }
    let freq = freq.ok_or_else(|| "missing FREQ".to_string())?;
    if count.is_some() && until.is_some() {
        return Err("COUNT and UNTIL together are invalid".to_string());
    }
    if let Some(bd) = &byday {
        if freq != Freq::Weekly {
            return Err("BYDAY is only supported for WEEKLY".to_string());
        }
        if bd.is_empty() {
            return Err("empty BYDAY".to_string());
        }
    }
    Ok(Rrule {
        freq,
        interval,
        count,
        until,
        byday,
    })
}

fn parse_event_block(lines: &[String], events: &mut Vec<Event>, warnings: &mut Vec<Warning>) {
    let mut uid: Option<String> = None;
    let mut summary = String::new();
    let mut location: Option<String> = None;
    let mut description: Option<String> = None;
    let mut dtstart: Option<DateTimeValue> = None;
    let mut dtend: Option<DateTimeValue> = None;
    let mut duration: Option<i64> = None;
    let mut duration_bad = false;
    let mut duration_seen = false;
    let mut rrule: Option<Rrule> = None;
    let mut rrule_warn: Option<String> = None;
    let mut exdates: Vec<DateTimeValue> = Vec::new();
    let mut recurrence_id: Option<DateTimeValue> = None;
    let mut cancelled = false;

    for line in lines {
        let Some(p) = parse_property_line(line) else {
            warnings.push(Warning::new("unparseable line in VEVENT skipped"));
            continue;
        };
        match p.name.as_str() {
            "UID" => {
                if uid.is_none() {
                    uid = Some(p.value);
                }
            }
            "SUMMARY" => summary = unescape_text(&p.value),
            "LOCATION" => {
                if location.is_none() {
                    location = Some(unescape_text(&p.value));
                }
            }
            "DESCRIPTION" => {
                if description.is_none() {
                    description = Some(unescape_text(&p.value));
                }
            }
            "DTSTART" => {
                if dtstart.is_none() {
                    dtstart = parse_datetime_value(&p.params, &p.value);
                }
            }
            "DTEND" => {
                if dtend.is_none() {
                    dtend = parse_datetime_value(&p.params, &p.value);
                }
            }
            "DURATION" => {
                if !duration_seen {
                    duration_seen = true;
                    match parse_duration(&p.value) {
                        Some(d) => duration = Some(d),
                        None => duration_bad = true,
                    }
                }
            }
            "RRULE" => {
                if rrule.is_none() && rrule_warn.is_none() {
                    match parse_rrule(&p.value) {
                        Ok(r) => rrule = Some(r),
                        Err(msg) => rrule_warn = Some(msg),
                    }
                }
            }
            "EXDATE" => {
                for item in p.value.split(',') {
                    let item = item.trim();
                    if item.is_empty() {
                        continue;
                    }
                    match parse_datetime_value(&p.params, item) {
                        Some(v) => exdates.push(v),
                        None => warnings.push(Warning::new(format!(
                            "invalid EXDATE value `{item}` ignored"
                        ))),
                    }
                }
            }
            "STATUS" => {
                if p.value.eq_ignore_ascii_case("CANCELLED") {
                    cancelled = true;
                }
            }
            "RECURRENCE-ID" if recurrence_id.is_none() => {
                recurrence_id = parse_datetime_value(&p.params, &p.value);
            }
            _ => {}
        }
    }

    let Some(uid) = uid else {
        warnings.push(Warning::new("VEVENT skipped: missing UID"));
        return;
    };
    if duration_bad {
        warnings.push(Warning::new(format!(
            "VEVENT {uid} skipped: invalid or signed DURATION"
        )));
        return;
    }
    let Some(dtstart) = dtstart else {
        warnings.push(Warning::new(format!(
            "VEVENT {uid} skipped: missing or invalid DTSTART"
        )));
        return;
    };
    if let Some(de) = &dtend {
        if de.naive < dtstart.naive {
            warnings.push(Warning::new(format!(
                "VEVENT {uid} skipped: DTEND before DTSTART"
            )));
            return;
        }
    }
    if let Some(msg) = rrule_warn {
        warnings.push(Warning::new(format!(
            "VEVENT {uid}: invalid RRULE ({msg}); treated as non-recurring"
        )));
        rrule = None;
    }
    events.push(Event {
        uid,
        summary,
        location,
        description,
        dtstart,
        dtend,
        duration_s: duration,
        rrule,
        exdates,
        recurrence_id,
        cancelled,
    });
}

/// Parse iCalendar text into events and warnings.
///
/// Errors: input > 8 MiB, a NUL byte, or no `VCALENDAR` component. A broken
/// `VEVENT` is skipped with a warning; the rest are kept. `VTODO`,
/// `VALARM` (nested in `VEVENT`) and `VTIMEZONE` bodies are ignored.
pub fn parse_calendar(text: &str) -> Result<(Vec<Event>, Vec<Warning>), IcsError> {
    if text.len() > 8 * 1024 * 1024 {
        return Err(IcsError::TooLarge);
    }
    if text.as_bytes().contains(&0) {
        return Err(IcsError::ContainsNul);
    }
    let lines = unfold(text);
    let mut events: Vec<Event> = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();
    let mut cal_depth = 0usize;
    let mut saw_calendar = false;
    let mut buf: Vec<String> = Vec::new();
    let mut in_event = false;
    for line in lines {
        let Some(p) = parse_property_line(&line) else {
            continue;
        };
        if p.name == "BEGIN" || p.name == "END" {
            let comp = p.value.trim().to_ascii_uppercase();
            if p.name == "BEGIN" {
                if comp == "VCALENDAR" {
                    cal_depth += 1;
                    saw_calendar = true;
                } else if comp == "VEVENT" && cal_depth > 0 {
                    in_event = true;
                    buf.clear();
                }
            } else if comp == "VCALENDAR" {
                cal_depth = cal_depth.saturating_sub(1);
            } else if comp == "VEVENT" && in_event {
                parse_event_block(&buf, &mut events, &mut warnings);
                in_event = false;
                buf.clear();
            }
            continue;
        }
        if in_event {
            buf.push(line);
        }
    }
    if in_event && !buf.is_empty() {
        warnings.push(Warning::new(
            "VEVENT not closed (missing END:VEVENT); skipped",
        ));
    }
    if !saw_calendar {
        return Err(IcsError::MissingCalendar);
    }
    Ok((events, warnings))
}
