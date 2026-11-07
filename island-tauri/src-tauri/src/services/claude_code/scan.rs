//! Incremental newline reader. Buffers at most an 8 MiB line plus a 1 MiB chunk.
//! Only reduced facts survive parsing; no handles or raw lines survive a poll.
use super::{mtime_ms, open_read, real_dir, MAX_FILES, MAX_LINE};
use crate::contract::claude_code::{ClaudeCodeFact, ClaudeCodeUsage, ClaudeFile};
use regex::bytes::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::LazyLock,
};
const READ_CHUNK: usize = 1024 * 1024;
const INTERRUPT_HEAD: usize = 8192;
fn needs_parse(line: &[u8]) -> bool {
    static RX: LazyLock<Option<Regex>> = LazyLock::new(|| {
        Regex::new(r#""type"\s*:\s*"(?:assistant|system)"|"compact_boundary"|"turn_duration""#).ok()
    });
    // A failed optimisation must fall back to parsing, never silently lose metrics.
    RX.as_ref().is_none_or(|rx| rx.is_match(line))
}
fn is_user(line: &[u8]) -> bool {
    static RX: LazyLock<Option<Regex>> =
        LazyLock::new(|| Regex::new(r#""type"\s*:\s*"user""#).ok());
    RX.as_ref().is_none_or(|rx| rx.is_match(line))
}
pub(super) struct Listed {
    key: String,
    slug: String,
    session_id: String,
    sub: bool,
    path: PathBuf,
    ino: u64,
    size: u64,
    mtime_ms: f64,
}
fn entries(dir: &Path) -> std::io::Result<Vec<(String, fs::FileType, fs::DirEntry)>> {
    real_dir(dir)?;
    Ok(fs::read_dir(dir)?
        .flatten()
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.file_type().ok()?, e)))
        .collect())
}
fn listed(
    e: &fs::DirEntry,
    key: String,
    slug: &str,
    session_id: &str,
    sub: bool,
) -> Option<Listed> {
    let m = e.metadata().ok().filter(|m| m.file_type().is_file())?;
    Some(Listed {
        key,
        slug: slug.into(),
        session_id: session_id.into(),
        sub,
        path: e.path(),
        ino: m.ino(),
        size: m.len(),
        mtime_ms: mtime_ms(&m),
    })
}
/// Exactly projects/*/*.jsonl and projects/*/*/subagents/*.jsonl, no symlink traversal.
pub(super) fn list(projects: &Path) -> (Vec<Listed>, Option<String>) {
    let slugs = match entries(projects) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (vec![], None),
        Err(_) => return (vec![], Some("projects unreadable".into())),
    };
    let mut out = vec![];
    let mut error = None;
    for (slug, ft, _) in slugs {
        if !ft.is_dir() {
            continue;
        }
        let items = match entries(&projects.join(&slug)) {
            Ok(items) => items,
            Err(_) => {
                error = Some("project unreadable".into());
                continue;
            }
        };
        for (name, ft, e) in items {
            if ft.is_file() {
                if let Some(stem) = name.strip_suffix(".jsonl").filter(|s| !s.is_empty()) {
                    out.extend(listed(&e, format!("{slug}/{name}"), &slug, stem, false));
                }
            } else if ft.is_dir() {
                let subdir = e.path().join("subagents");
                if !fs::symlink_metadata(&subdir).is_ok_and(|m| m.file_type().is_dir()) {
                    continue;
                }
                let subs = match entries(&subdir) {
                    Ok(subs) => subs,
                    Err(_) => {
                        error = Some("subagents unreadable".into());
                        continue;
                    }
                };
                for (sname, sft, se) in subs {
                    if sft.is_file() && sname.len() > ".jsonl".len() && sname.ends_with(".jsonl") {
                        out.extend(listed(
                            &se,
                            format!("{slug}/{name}/subagents/{sname}"),
                            &slug,
                            &name,
                            true,
                        ));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| {
        b.mtime_ms
            .total_cmp(&a.mtime_ms)
            .then_with(|| a.key.cmp(&b.key))
    });
    out.truncate(MAX_FILES);
    (out, error)
}
pub(super) type TailCache = HashMap<String, (String, u64, u64, f64)>;
pub(super) fn read_all(
    listed: Vec<Listed>,
    cursors: &HashMap<&str, (&str, f64)>,
    budget: u64,
    tails: &mut TailCache,
) -> (Vec<ClaudeFile>, bool) {
    let keys: std::collections::HashSet<_> = listed.iter().map(|l| l.key.as_str()).collect();
    tails.retain(|key, _| keys.contains(key.as_str()));
    let mut remaining = budget;
    let mut more = false;
    let files = listed
        .into_iter()
        .map(|l| {
            let ino = l.ino.to_string();
            let off = cursors.get(l.key.as_str()).and_then(|&(cino, off)| {
                let ok = cino == ino && off.is_finite() && off >= 0.0 && off.fract() == 0.0;
                (ok && off as u64 <= l.size).then_some(off as u64)
            });
            let reset = off.is_none();
            let start = off.unwrap_or(0);
            let mut read = Read0 {
                next: start,
                ..Read0::default()
            };
            let stamp = (ino.clone(), l.size, start, l.mtime_ms);
            if start < l.size && tails.get(&l.key) != Some(&stamp) {
                if remaining == 0 {
                    more = true;
                } else {
                    // Recheck identity after opening: replacements must not be labelled with the old inode.
                    read = read_file_checked(&l.path, start, l.size, Some(l.ino), &mut remaining);
                    if read.tail && read.error.is_none() {
                        tails.insert(l.key.clone(), (ino.clone(), l.size, read.next, l.mtime_ms));
                    } else {
                        tails.remove(&l.key);
                    }
                    if read.budget_hit && read.next < l.size {
                        more = true;
                    }
                }
            }
            ClaudeFile {
                key: l.key,
                slug: l.slug,
                session_id: l.session_id,
                sub: l.sub,
                ino,
                size: l.size as f64,
                mtime_ms: l.mtime_ms,
                reset,
                from: start as f64,
                next: read.next as f64,
                facts: read.facts,
                skipped_huge: read.skipped_huge as f64,
                bad_lines: read.bad_lines as f64,
                error: read.error,
            }
        })
        .collect();
    (files, more)
}
#[derive(Default)]
pub(super) struct Read0 {
    pub(super) next: u64,
    pub(super) facts: Vec<ClaudeCodeFact>,
    pub(super) skipped_huge: u32,
    pub(super) bad_lines: u32,
    pub(super) error: Option<String>,
    budget_hit: bool,
    tail: bool,
}
fn read_file_checked(
    path: &Path,
    start: u64,
    size: u64,
    ino: Option<u64>,
    remaining: &mut u64,
) -> Read0 {
    let mut r = Read0 {
        next: start,
        ..Read0::default()
    };
    let mut f = match open_read(path).and_then(|mut f| {
        let m = f.metadata()?;
        if ino.is_some_and(|ino| m.ino() != ino) || m.len() < size {
            return Err(std::io::Error::other("file changed"));
        }
        f.seek(SeekFrom::Start(start))?;
        Ok(f)
    }) {
        Ok(f) => f,
        Err(e) => {
            r.error = Some(io_reason(&e));
            return r;
        }
    };
    // An offset inside a line can only be a bounded checkpoint while discarding a huge line
    // (or an invalid client cursor). Recheck the preceding byte; never parse its tail as JSON.
    let mut skipping = false;
    if start > 0 {
        let previous = (|| -> std::io::Result<u8> {
            f.seek(SeekFrom::Start(start - 1))?;
            let mut byte = [0];
            f.read_exact(&mut byte)?;
            f.seek(SeekFrom::Start(start))?;
            Ok(byte[0])
        })();
        match previous {
            Ok(byte) => skipping = byte != b'\n',
            Err(e) => {
                r.error = Some(io_reason(&e));
                return r;
            }
        }
    }
    // Finishing one valid line may exceed the requested budget, never by more than MAX_LINE
    // plus its newline. Previously an arbitrarily large skipped line could read without bound.
    let hard_end = start
        .saturating_add(*remaining)
        .saturating_add(MAX_LINE as u64 + 1);
    let mut buf = vec![0u8; READ_CHUNK.min(size.saturating_sub(start) as usize).max(1)];
    let mut carry = Vec::new();
    let mut read_at = start;
    'outer: while read_at < size {
        if read_at >= hard_end {
            if skipping {
                r.next = read_at;
            }
            *remaining = 0;
            r.budget_hit = true;
            break;
        }
        let want = buf
            .len()
            .min((size - read_at).min(hard_end - read_at) as usize);
        let n = match f.read(&mut buf[..want]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                r.error = Some(io_reason(&e));
                break;
            }
        };
        *remaining = remaining.saturating_sub(n as u64);
        let chunk = &buf[..n];
        let mut s = 0;
        while let Some(i) = chunk[s..].iter().position(|&c| c == b'\n').map(|i| s + i) {
            if skipping {
                skipping = false;
                r.skipped_huge += 1;
            } else if carry.len() + i - s > MAX_LINE {
                r.skipped_huge += 1;
            } else if carry.is_empty() {
                on_line(&chunk[s..i], &mut r);
            } else {
                carry.extend_from_slice(&chunk[s..i]);
                on_line(&carry, &mut r);
            }
            carry.clear();
            let end = read_at + i as u64 + 1;
            r.next = end;
            s = i + 1;
            if *remaining == 0 {
                r.budget_hit = true;
                break 'outer;
            }
        }
        if !skipping && s < n {
            if carry.len() + n - s > MAX_LINE {
                carry = Vec::new();
                skipping = true;
            } else {
                carry.extend_from_slice(&chunk[s..]);
            }
        }
        read_at += n as u64;
    }
    r.tail = read_at == size && r.next < size && !r.budget_hit;
    r
}
fn io_reason(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => "not found",
        std::io::ErrorKind::PermissionDenied => "permission denied",
        _ => "read error",
    }
    .into()
}
#[derive(Deserialize)]
struct Head {
    #[serde(rename = "type")]
    kind: Option<Value>,
    timestamp: Option<Value>,
    #[serde(rename = "isMeta")]
    is_meta: Option<Value>,
}
#[derive(Deserialize)]
struct Full {
    #[serde(rename = "type")]
    kind: Option<Value>,
    subtype: Option<Value>,
    timestamp: Option<Value>,
    #[serde(rename = "isMeta")]
    is_meta: Option<Value>,
    #[serde(rename = "isSidechain")]
    is_sidechain: Option<Value>,
    entrypoint: Option<Value>,
    cwd: Option<Value>,
    uuid: Option<Value>,
    #[serde(rename = "requestId")]
    request_id: Option<Value>,
    message: Option<Value>,
}
pub(super) fn on_line(line: &[u8], r: &mut Read0) {
    if line.is_empty() {
        return;
    }
    let full = needs_parse(line);
    if !full && !is_user(line) {
        return;
    }
    if line.iter().find(|c| !c.is_ascii_whitespace()) != Some(&b'{') {
        if serde_json::from_slice::<serde::de::IgnoredAny>(line).is_err() {
            r.bad_lines += 1;
        }
        return;
    }
    let fact = if full {
        match serde_json::from_slice::<Full>(line) {
            Ok(o) => full_fact(o, line),
            Err(_) => {
                r.bad_lines += 1;
                return;
            }
        }
    } else {
        match serde_json::from_slice::<Head>(line) {
            Ok(h) if str_of(&h.kind) == Some("user") => {
                Some(user_fact(&h.timestamp, &h.is_meta, line))
            }
            Ok(_) => None,
            Err(_) => {
                r.bad_lines += 1;
                return;
            }
        }
    };
    r.facts.extend(fact);
}
fn user_fact(ts: &Option<Value>, meta: &Option<Value>, line: &[u8]) -> ClaudeCodeFact {
    ClaudeCodeFact::User {
        at: to_ms(ts),
        meta: matches!(meta, Some(Value::Bool(true))),
        interrupt: line[..line.len().min(INTERRUPT_HEAD)]
            .windows(b"[Request interrupted by user".len())
            .any(|w| w == b"[Request interrupted by user"),
    }
}
fn full_fact(o: Full, line: &[u8]) -> Option<ClaudeCodeFact> {
    let at = to_ms(&o.timestamp);
    match str_of(&o.kind)? {
        "assistant" => assistant(o, at),
        "system" => Some(match str_of(&o.subtype) {
            Some("compact_boundary") => ClaudeCodeFact::Compact { at },
            Some("turn_duration") => ClaudeCodeFact::TurnEnd { at },
            _ => ClaudeCodeFact::Sys { at },
        }),
        "user" => Some(user_fact(&o.timestamp, &o.is_meta, line)),
        _ => None,
    }
}
fn assistant(o: Full, at: Option<f64>) -> Option<ClaudeCodeFact> {
    let m = o.message.as_ref()?.as_object()?;
    let model = m
        .get("model")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?;
    if model == "<synthetic>" {
        return None;
    }
    let u = m.get("usage")?.as_object()?;
    let (mut chars, mut sig, mut tool) = (0u64, 0u64, None);
    if let Some(content) = m.get("content").and_then(Value::as_array) {
        for c in content.iter().filter_map(Value::as_object) {
            match c.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(t) = c.get("text").and_then(Value::as_str) {
                        chars += utf16_len(t);
                    }
                }
                Some("thinking") => {
                    if let Some(s) = c.get("signature").and_then(Value::as_str) {
                        sig += utf16_len(s);
                    }
                }
                Some("tool_use") => {
                    chars += match c.get("input") {
                        None | Some(Value::Null) => 2,
                        Some(v) => json_len(v),
                    };
                    if let Some(name) = safe_tool_name(c.get("name")) {
                        tool = Some(name);
                    }
                }
                _ => {}
            }
        }
    }
    let id = m
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| str_of(&o.uuid))
        .unwrap_or("");
    let key = format!("{id}|{}", str_of(&o.request_id).unwrap_or(""));
    let num = |v: Option<&Value>| {
        v.and_then(Value::as_f64)
            .filter(|x| x.is_finite())
            .unwrap_or(0.0)
    };
    Some(ClaudeCodeFact::A {
        at,
        model: model.into(),
        key,
        side: matches!(o.is_sidechain, Some(Value::Bool(true))),
        ep: str_of(&o.entrypoint).map(str::to_owned),
        cwd: str_of(&o.cwd).map(str::to_owned),
        stop: m
            .get("stop_reason")
            .and_then(Value::as_str)
            .map(str::to_owned),
        tool,
        chars: chars as f64,
        sig: sig as f64,
        u: ClaudeCodeUsage {
            input: num(u.get("input_tokens")),
            output: num(u.get("output_tokens")),
            cache_read: num(u.get("cache_read_input_tokens")),
            cache_write: num(u.get("cache_creation_input_tokens")),
            cw1h: num(u
                .get("cache_creation")
                .and_then(Value::as_object)
                .and_then(|cc| cc.get("ephemeral_1h_input_tokens"))),
            fast: u.get("speed").and_then(Value::as_str) == Some("fast"),
        },
    })
}
fn str_of(v: &Option<Value>) -> Option<&str> {
    v.as_ref().and_then(Value::as_str)
}
pub(super) fn utf16_len(s: &str) -> u64 {
    s.encode_utf16().count() as u64
}
pub(super) fn safe_tool_name(v: Option<&Value>) -> Option<String> {
    let s = v?.as_str().filter(|s| !s.is_empty())?;
    let ok = s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'));
    Some(if ok { s.into() } else { "tool".into() })
}
pub(super) fn json_len(v: &Value) -> u64 {
    match v {
        Value::Null => 4,
        Value::Bool(b) => {
            if *b {
                4
            } else {
                5
            }
        }
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                n.to_string().len() as u64
            } else {
                n.as_f64().map_or(4, js_number_len)
            }
        }
        Value::String(s) => json_str_len(s),
        Value::Array(a) => {
            2 + a.iter().map(json_len).sum::<u64>() + a.len().saturating_sub(1) as u64
        }
        Value::Object(o) => {
            2 + o
                .iter()
                .map(|(k, v)| json_str_len(k) + 1 + json_len(v))
                .sum::<u64>()
                + o.len().saturating_sub(1) as u64
        }
    }
}
fn js_number_len(f: f64) -> u64 {
    if !f.is_finite() {
        return 4;
    }
    if f == 0.0 {
        return 1;
    }
    let sign = u64::from(f < 0.0);
    let s = format!("{:e}", f.abs());
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i64 = exp.parse().unwrap_or(0);
    let k = mant.bytes().filter(u8::is_ascii_digit).count() as i64;
    let n = exp + 1;
    let body = if k <= n && n <= 21 {
        n
    } else if 0 < n && n <= 21 {
        k + 1
    } else if -6 < n && n <= 0 {
        2 - n + k
    } else {
        k + i64::from(k > 1) + 2 + (n - 1).unsigned_abs().to_string().len() as i64
    };
    sign + body as u64
}
fn json_str_len(s: &str) -> u64 {
    2 + s
        .chars()
        .map(|c| match c {
            '"' | '\\' | '\u{8}' | '\u{c}' | '\n' | '\r' | '\t' => 2,
            c if (c as u32) < 0x20 => 6,
            c => c.len_utf16() as u64,
        })
        .sum::<u64>()
}
pub(super) fn to_ms(v: &Option<Value>) -> Option<f64> {
    fn num(x: f64) -> Option<f64> {
        x.is_finite().then(|| {
            if x < 1e11 {
                (x * 1000.0).round()
            } else {
                x.round()
            }
        })
    }
    match v.as_ref()? {
        Value::Number(n) => num(n.as_f64()?),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                return None;
            }
            let numeric = t.split_once('.').map_or_else(
                || t.bytes().all(|b| b.is_ascii_digit()),
                |(a, b)| {
                    !a.is_empty()
                        && !b.is_empty()
                        && a.bytes().all(|c| c.is_ascii_digit())
                        && b.bytes().all(|c| c.is_ascii_digit())
                },
            );
            if numeric {
                return num(t.parse().ok()?);
            }
            t.parse::<jiff::Timestamp>()
                .ok()
                .map(|ts| ts.as_millisecond() as f64)
        }
        _ => None,
    }
}
