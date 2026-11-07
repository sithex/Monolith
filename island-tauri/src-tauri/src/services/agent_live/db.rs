//! Read-only `state.db` queries, ported verbatim from Habby's `metrics-reader.ts` `DbReader`.
//! Open (`mode=ro`, read-only WAL/SHM, 100 ms busy timeout) -> queries -> close; never writes,
//! VACUUMs or checkpoints. A missing optional table/column leaves that field null/empty.

use crate::contract::agent_live::{
    DbData, DbDelegation, DbSession, DbTask, DbToolCount, HistoryDay, HistoryModel, HistoryRaw,
    HistoryTop,
};
use rusqlite::{ffi, params_from_iter, types::Value, Connection, OpenFlags, Row};
use std::{
    collections::HashSet,
    ffi::{c_char, c_int},
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::OnceLock,
    time::Duration,
};

pub const DB_TIMEOUT: Duration = Duration::from_millis(100);
pub const MAX_CHAIN_HOPS: usize = 20;
/// The messages aggregate (tools by name, turns) refreshes at most this often per chain.
pub const SLOW_EVERY_MS: f64 = 10e3;

/// Last messages aggregate, keyed by the complete chain (IDs may themselves contain commas).
#[derive(Debug, Clone, Default)]
pub struct SlowCache {
    key: Vec<String>,
    at: f64,
    tools_by_name: Vec<DbToolCount>,
    turns: Option<f64>,
}

/// SQLITE_BUSY / SQLITE_LOCKED (WAL recovery, checkpoint, migration).
pub fn is_busy_error(e: &str) -> bool {
    static RE: OnceLock<Option<regex::Regex>> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?i)\b(busy|locked)\b").ok())
        .as_ref()
        .is_some_and(|re| re.is_match(e))
}

fn err_str(e: &rusqlite::Error) -> String {
    let s = e.to_string();
    let first = s.lines().next().unwrap_or_default();
    first.chars().take(160).collect()
}

/// Which optional tables/columns this state.db has (older Hermes / test DBs lack some).
struct Schema {
    tables: HashSet<String>,
    session_cols: HashSet<String>,
    usage_cols: HashSet<String>,
}

impl Schema {
    fn read(db: &Connection) -> rusqlite::Result<Self> {
        let names = |sql: &str| -> rusqlite::Result<HashSet<String>> {
            let mut st = db.prepare(sql)?;
            let rows = st.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect()
        };
        Ok(Self {
            tables: names("SELECT name FROM sqlite_master WHERE type = 'table'")?,
            session_cols: names("SELECT name FROM pragma_table_info('sessions')")?,
            usage_cols: names("SELECT name FROM pragma_table_info('session_model_usage')")?,
        })
    }
    fn has(&self, table: &str) -> bool {
        self.tables.contains(table)
    }
    fn model_config(&self) -> bool {
        self.session_cols.contains("model_config")
    }
    fn end_reason(&self) -> bool {
        self.session_cols.contains("end_reason")
    }
    /// SQL: Hermes's delegate-subagent marker (hermes_state_schema.py v16), NULL for main sessions.
    fn deleg(&self, a: &str) -> String {
        if self.model_config() {
            format!("(CASE WHEN json_valid({a}.model_config) THEN json_extract({a}.model_config, '$._delegate_from') END)")
        } else {
            "NULL".into()
        }
    }
    /// SQL: the row's parent ended by context compression (a continuation, not a subagent).
    fn cont_of(&self, a: &str) -> String {
        if self.end_reason() && self.session_cols.contains("parent_session_id") {
            format!("EXISTS (SELECT 1 FROM sessions pp WHERE pp.id = {a}.parent_session_id AND pp.end_reason = 'compression')")
        } else {
            "0".into()
        }
    }
    /// Habby's `SESSION_COLS`; a column this db lacks reads as NULL.
    fn session_cols_sql(&self) -> String {
        const COLS: [&str; 14] = [
            "id",
            "title",
            "model",
            "api_call_count",
            "input_tokens",
            "output_tokens",
            "cache_read_tokens",
            "cache_write_tokens",
            "estimated_cost_usd",
            "tool_call_count",
            "started_at",
            "ended_at",
            "last_activity_at",
            "parent_session_id",
        ];
        let mut out: Vec<String> = COLS
            .iter()
            .map(|c| {
                if self.session_cols.contains(*c) {
                    format!("s.{c}")
                } else {
                    format!("NULL AS {c}")
                }
            })
            .collect();
        out.push(
            if self.usage_cols.contains("session_id") && self.usage_cols.contains("last_seen") {
                "(SELECT max(u.last_seen) FROM session_model_usage u WHERE u.session_id = s.id) AS last_seen".into()
            } else {
                "NULL AS last_seen".into()
            },
        );
        out.join(", ")
    }
}

/// Escape original path bytes rather than a lossy UTF-8 conversion. Absolute paths
/// cannot accidentally become a URI authority.
fn ro_uri(path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| "state.db path unavailable".to_owned())?
            .join(path)
    };
    let mut p = String::new();
    for &byte in absolute.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            p.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(p, "%{byte:02X}");
        }
    }
    // `mode=ro` alone still lets SQLite CREATE/open a WAL read-write and modify/create
    // its SHM index. Never use immutable=1: it silently ignores committed WAL frames.
    Ok(format!("file:{p}?mode=ro&readonly_shm=1"))
}

const RO_VFS_NAME: &str = "agent-live-read-only";
static BASE_VFS: OnceLock<usize> = OnceLock::new();
static BASE_METHODS: OnceLock<ffi::sqlite3_io_methods> = OnceLock::new();
static RO_METHODS: OnceLock<ffi::sqlite3_io_methods> = OnceLock::new();

/// SQLite opens WAL files with READWRITE|CREATE even for mode=ro databases. This private
/// VFS delegates to bundled SQLite's Unix VFS but forces every named open read-only,
/// denies temporary files/deletions, and leaves SHM to readonly_shm=1. Registration
/// lasts for the process lifetime and does not change SQLite's default VFS.
fn register_ro_vfs() -> Result<(), String> {
    static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
    REGISTERED
        .get_or_init(|| {
            // SAFETY: SQLite owns the native VFS for process lifetime. Registration is
            // serialized by OnceLock; the copied structure is leaked for that lifetime.
            // Unchanged function pointers/app data still refer to bundled SQLite's Unix VFS.
            unsafe {
                let base = ffi::sqlite3_vfs_find(c"unix".as_ptr());
                if base.is_null() || (*base).xOpen.is_none() {
                    return Err("read-only SQLite VFS unavailable".to_owned());
                }
                let _ = BASE_VFS.set(base as usize);
                let mut vfs = Box::new(std::ptr::read(base));
                vfs.zName = c"agent-live-read-only".as_ptr();
                vfs.pNext = std::ptr::null_mut();
                vfs.xOpen = Some(ro_vfs_open);
                vfs.xDelete = Some(ro_vfs_delete);
                let vfs = Box::into_raw(vfs);
                if ffi::sqlite3_vfs_register(vfs, 0) != ffi::SQLITE_OK {
                    drop(Box::from_raw(vfs));
                    return Err("read-only SQLite VFS registration failed".to_owned());
                }
                Ok(())
            }
        })
        .clone()
}

unsafe extern "C" fn ro_vfs_open(
    _vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out_flags: *mut c_int,
) -> c_int {
    if name.is_null() || flags & (ffi::SQLITE_OPEN_MAIN_DB | ffi::SQLITE_OPEN_WAL) == 0 {
        return ffi::SQLITE_READONLY;
    }
    let Some(&base) = BASE_VFS.get() else {
        return ffi::SQLITE_CANTOPEN;
    };
    // SAFETY: SQLite supplies valid file/name/out_flags arguments for xOpen. BASE_VFS
    // is set before this callback is registered and the native VFS never expires.
    unsafe {
        let base = base as *mut ffi::sqlite3_vfs;
        let Some(open) = (*base).xOpen else {
            return ffi::SQLITE_CANTOPEN;
        };
        let flags = (flags
            & !(ffi::SQLITE_OPEN_READWRITE
                | ffi::SQLITE_OPEN_CREATE
                | ffi::SQLITE_OPEN_DELETEONCLOSE))
            | ffi::SQLITE_OPEN_READONLY
            | ffi::SQLITE_OPEN_NOFOLLOW;
        let rc = open(base, name, file, flags, out_flags);
        if rc == ffi::SQLITE_OK && flags & ffi::SQLITE_OPEN_MAIN_DB != 0 {
            // The Unix VFS can reuse an SHM mapping opened by another connection in
            // this process. Tell SQLite it is read-only even if that mapping is RW,
            // so this reader never updates read marks or unlinks the shared index.
            let original = BASE_METHODS.get_or_init(|| *(*file).pMethods);
            let methods = RO_METHODS.get_or_init(|| {
                let mut methods = *original;
                methods.xShmMap = Some(ro_shm_map);
                methods.xShmUnmap = Some(ro_shm_unmap);
                methods
            });
            (*file).pMethods = methods;
        }
        rc
    }
}

unsafe extern "C" fn ro_shm_map(
    file: *mut ffi::sqlite3_file,
    page: c_int,
    size: c_int,
    _extend: c_int,
    out: *mut *mut std::ffi::c_void,
) -> c_int {
    let Some(map) = BASE_METHODS.get().and_then(|m| m.xShmMap) else {
        return ffi::SQLITE_READONLY;
    };
    // SAFETY: arguments are the native SQLite xShmMap arguments. Never extend the
    // existing index; a successful mapping is advertised as read-only to the pager.
    let rc = unsafe { map(file, page, size, 0, out) };
    if rc == ffi::SQLITE_OK {
        ffi::SQLITE_READONLY
    } else {
        rc
    }
}

unsafe extern "C" fn ro_shm_unmap(file: *mut ffi::sqlite3_file, _delete: c_int) -> c_int {
    let Some(unmap) = BASE_METHODS.get().and_then(|m| m.xShmUnmap) else {
        return ffi::SQLITE_OK;
    };
    // SAFETY: the native file object is unchanged, and deletion is always disabled.
    unsafe { unmap(file, 0) }
}

unsafe extern "C" fn ro_vfs_delete(
    _vfs: *mut ffi::sqlite3_vfs,
    _name: *const c_char,
    _sync_dir: c_int,
) -> c_int {
    ffi::SQLITE_READONLY
}

fn open(path: &Path) -> Result<Connection, String> {
    super::open_regular(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "state.db not found"
        } else {
            "state.db unreadable"
        }
        .to_owned()
    })?;
    register_ro_vfs()?;
    let db = Connection::open_with_flags_and_vfs(
        ro_uri(path)?,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        RO_VFS_NAME,
    )
    .map_err(|e| err_str(&e))?;
    db.busy_timeout(DB_TIMEOUT).map_err(|e| err_str(&e))?;
    // SAFETY: the handle belongs to this live connection. This connection-local limit
    // bounds a corrupt/unexpected text/blob row without changing the source database.
    unsafe {
        ffi::sqlite3_limit(db.handle(), ffi::SQLITE_LIMIT_LENGTH, 4 * 1024 * 1024);
    }
    db.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA temp_store=MEMORY; BEGIN DEFERRED;")
        .map_err(|e| err_str(&e))?;
    Ok(db)
}

/// Open -> `f` -> close. The connection is dropped (closed) before returning.
fn with_db<T>(
    path: &Path,
    f: impl FnOnce(&Connection, &Schema) -> rusqlite::Result<T>,
) -> Result<T, String> {
    let db = open(path)?;
    let schema = Schema::read(&db).map_err(|e| err_str(&e))?;
    let out = f(&db, &schema).map_err(|e| err_str(&e));
    drop(db);
    out
}

/// Only absent optional schema is soft. Busy, corruption, permission and I/O failures
/// propagate so the UI retains last-good data instead of caching false zeroes.
fn soft<T>(r: rusqlite::Result<T>, fallback: T) -> rusqlite::Result<T> {
    match r {
        Err(rusqlite::Error::SqliteFailure(_, Some(ref message)))
        | Err(rusqlite::Error::SqlInputError {
            msg: ref message, ..
        }) if message.starts_with("no such table:") || message.starts_with("no such column:") => {
            Ok(fallback)
        }
        ok => ok,
    }
}

// --- value helpers (Habby's n0 / toMs / typeof checks) ---------------------------------------

fn val(r: &Row, name: &str) -> Value {
    r.get::<_, Value>(name).unwrap_or(Value::Null)
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Real(f) if f.is_finite() => Some(*f),
        _ => None,
    }
}

fn n0(v: &Value) -> f64 {
    num(v).unwrap_or(0.0)
}

fn text(v: &Value) -> Option<String> {
    match v {
        Value::Text(s) => Some(s.clone()),
        _ => None,
    }
}

/// Habby `toMs`: epoch seconds (< 1e11) or ms -> ms; numeric or date strings parsed.
pub fn to_ms(v: &Value) -> Option<f64> {
    fn from_num(x: f64) -> Option<f64> {
        let ms = if x < 1e11 {
            (x * 1000.0).round()
        } else {
            x.round()
        };
        ms.is_finite().then_some(ms)
    }
    match v {
        Value::Text(s) => {
            let t = s.trim();
            if t.is_empty() {
                return None;
            }
            let numeric =
                t.split_once('.')
                    .map_or(t.bytes().all(|b| b.is_ascii_digit()), |(a, b)| {
                        !a.is_empty()
                            && !b.is_empty()
                            && a.bytes().all(|c| c.is_ascii_digit())
                            && b.bytes().all(|c| c.is_ascii_digit())
                    });
            if numeric {
                return t
                    .parse::<f64>()
                    .ok()
                    .filter(|x| x.is_finite())
                    .and_then(from_num);
            }
            if let Ok(ts) = t.parse::<jiff::Timestamp>() {
                return Some(ts.as_millisecond() as f64);
            }
            let dt = t.parse::<jiff::civil::DateTime>().ok()?;
            let zoned = dt.to_zoned(jiff::tz::TimeZone::system()).ok()?;
            Some(zoned.timestamp().as_millisecond() as f64)
        }
        other => num(other).and_then(from_num),
    }
}

/// Habby `truncate`: control characters -> space, trimmed, `n` chars max with an ellipsis.
pub fn truncate(s: &str, n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    let mut clean = String::with_capacity(s.len());
    let mut in_ctrl = false;
    for c in s.chars() {
        if (c as u32) < 0x20 || c as u32 == 0x7f {
            if !in_ctrl {
                clean.push(' ');
            }
            in_ctrl = true;
        } else {
            clean.push(c);
            in_ctrl = false;
        }
    }
    let clean = clean.trim();
    if clean.chars().count() > n {
        let mut out: String = clean.chars().take(n.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    } else {
        clean.to_owned()
    }
}

/// Walk `task_json` for goal strings (shape not fixed: {goal} | {tasks:[{goal}]} | [...]).
pub fn goals_of(task_json: Option<&str>) -> Vec<String> {
    let Some(data) = task_json
        .filter(|s| !s.is_empty() && s.len() <= 128 * 1024)
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
    else {
        return Vec::new();
    };
    fn walk(x: &serde_json::Value, depth: usize, out: &mut Vec<String>) {
        if depth > 4 || out.len() >= 20 {
            return;
        }
        match x {
            serde_json::Value::Array(items) => {
                for v in items {
                    walk(v, depth + 1, out);
                }
            }
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::String(g)) = map.get("goal") {
                    out.push(truncate(g, 80));
                }
                for (k, v) in map {
                    if k != "goal" && (v.is_object() || v.is_array() || v.is_null()) {
                        walk(v, depth + 1, out);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&data, 0, &mut out);
    out
}

/// API list price per million tokens (input, cache read): Habby `PRICES`, exact-model match.
fn price_for(model: Option<&str>) -> Option<(f64, f64)> {
    static RES: OnceLock<Vec<(regex::Regex, f64, f64)>> = OnceLock::new();
    let res = RES.get_or_init(|| {
        let rows: [(&[&str], f64, f64); 4] = [
            (&["opus-5-5"], 4.0, 0.2),
            (&["fable-5-1"], 10.0, 0.25),
            (
                &["sonnet-5", "sonnet-4-6", "sonnet-4-5", "sonnet-4"],
                3.0,
                0.3,
            ),
            (&["haiku-4-5"], 1.0, 0.1),
        ];
        rows.iter()
            .filter_map(|(ids, input, cr)| {
                let re = format!(
                    r"(?i)(?:^|[^a-z0-9])(?:{})(?:-\d{{8}})?(?:\[[a-z0-9]+\])?$",
                    ids.join("|")
                );
                regex::Regex::new(&re).ok().map(|r| (r, *input, *cr))
            })
            .collect()
    });
    let model = model.filter(|m| !m.is_empty())?;
    res.iter()
        .find(|(re, _, _)| re.is_match(model))
        .map(|(_, i, c)| (*i, *c))
}

fn row_to_session(r: &Row) -> DbSession {
    let cost = num(&val(r, "estimated_cost_usd"));
    let id = match val(r, "id") {
        Value::Text(s) => s,
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        _ => "null".into(),
    };
    DbSession {
        id,
        title: text(&val(r, "title")),
        model: text(&val(r, "model")),
        calls: n0(&val(r, "api_call_count")),
        input: n0(&val(r, "input_tokens")),
        output: n0(&val(r, "output_tokens")),
        cache_read: n0(&val(r, "cache_read_tokens")),
        cache_write: n0(&val(r, "cache_write_tokens")),
        cost,
        tools: n0(&val(r, "tool_call_count")),
        started_at: to_ms(&val(r, "started_at")),
        ended_at: to_ms(&val(r, "ended_at")),
        last_activity_at: to_ms(&val(r, "last_activity_at")),
        last_seen: to_ms(&val(r, "last_seen")),
    }
}

/// Sum a compression chain (root first) into one row: id/title/model/end = newest, start = root.
pub fn sum_chain(rows: &[DbSession]) -> Option<DbSession> {
    let tip = rows.last()?;
    let mut out = tip.clone();
    out.started_at = rows[0].started_at;
    out.title = tip.title.clone().or_else(|| {
        rows.iter()
            .find_map(|r| r.title.clone().filter(|t| !t.is_empty()))
    });
    if rows.len() > 1 {
        out.calls = 0.0;
        out.input = 0.0;
        out.output = 0.0;
        out.cache_read = 0.0;
        out.cache_write = 0.0;
        out.tools = 0.0;
        out.cost = Some(0.0);
        for r in rows {
            out.calls += r.calls;
            out.input += r.input;
            out.output += r.output;
            out.cache_read += r.cache_read;
            out.cache_write += r.cache_write;
            out.tools += r.tools;
            out.cost = Some(out.cost.unwrap_or(0.0) + r.cost.unwrap_or(0.0));
            let m = out.last_seen.unwrap_or(0.0).max(r.last_seen.unwrap_or(0.0));
            out.last_seen = (m != 0.0).then_some(m);
        }
    }
    Some(out)
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

/// Newest not-ended top-level session or compaction continuation.
fn newest_top_level(db: &Connection, s: &Schema) -> rusqlite::Result<Option<String>> {
    let ended = if s.session_cols.contains("ended_at") {
        "s.ended_at IS NULL"
    } else {
        "1"
    };
    let parent = if s.session_cols.contains("parent_session_id") {
        format!("(s.parent_session_id IS NULL OR {})", s.cont_of("s"))
    } else {
        "1".to_owned()
    };
    let order = match (
        s.session_cols.contains("last_activity_at"),
        s.session_cols.contains("started_at"),
    ) {
        (true, true) => "coalesce(s.last_activity_at, s.started_at)",
        (true, false) => "s.last_activity_at",
        (false, true) => "s.started_at",
        (false, false) => "s.id",
    };
    let sql = format!(
        "SELECT s.id FROM sessions s WHERE {ended} AND {} IS NULL AND {parent}
                ORDER BY {order} DESC LIMIT 1",
        s.deleg("s"),
    );
    let mut st = db.prepare(&sql)?;
    let mut rows = st.query([])?;
    Ok(rows
        .next()?
        .and_then(|r| text(&r.get::<_, Value>(0).unwrap_or(Value::Null))))
}

/// The compression continuation chain through `sid`, root first, newest last (bounded).
fn chain_of(db: &Connection, s: &Schema, sid: &str) -> rusqlite::Result<Vec<String>> {
    let mut chain = vec![sid.to_owned()];
    if !s.end_reason() || !s.session_cols.contains("parent_session_id") {
        return Ok(chain);
    }
    let mut back = db.prepare(&format!(
        "SELECT c.parent_session_id AS p, {} AS dlg, p.end_reason AS per
            FROM sessions c LEFT JOIN sessions p ON p.id = c.parent_session_id WHERE c.id = ?",
        s.deleg("c")
    ))?;
    for _ in 0..MAX_CHAIN_HOPS {
        let mut rows = back.query([&chain[0]])?;
        let Some(r) = rows.next()? else { break };
        let p = text(&val(r, "p")).filter(|p| !p.is_empty());
        let per = text(&val(r, "per"));
        let delegated = !matches!(val(r, "dlg"), Value::Null);
        match p {
            Some(p)
                if !delegated && per.as_deref() == Some("compression") && !chain.contains(&p) =>
            {
                chain.insert(0, p)
            }
            _ => break,
        }
    }
    // before the TUI re-anchors to the continuation, `sid` is the ended segment: follow it forward
    let order = if s.session_cols.contains("started_at") {
        "c.started_at"
    } else {
        "c.id"
    };
    let mut fwd = db.prepare(&format!(
        "SELECT c.id FROM sessions c JOIN sessions p ON p.id = c.parent_session_id
            WHERE c.parent_session_id = ? AND p.end_reason = 'compression' AND {} IS NULL
            ORDER BY {order} DESC LIMIT 1",
        s.deleg("c")
    ))?;
    for _ in 0..MAX_CHAIN_HOPS {
        let last = chain[chain.len() - 1].clone();
        let mut rows = fwd.query([&last])?;
        let Some(id) = rows.next()?.and_then(|r| text(&val(r, "id"))) else {
            break;
        };
        if chain.contains(&id) {
            break;
        }
        chain.push(id);
    }
    Ok(chain)
}

fn sessions_where(
    db: &Connection,
    sql: &str,
    params: &[String],
) -> rusqlite::Result<Vec<DbSession>> {
    let mut st = db.prepare(sql)?;
    let rows = st.query_map(params_from_iter(params.iter()), |r| Ok(row_to_session(r)))?;
    rows.collect()
}

/// Habby `DbReader.live` for `sid`, plus `newestTopLevel` (the UI's db-newest fallback).
pub fn live(
    path: &Path,
    sid: &str,
    now: f64,
    slow: &mut Option<SlowCache>,
) -> Result<DbData, String> {
    if sid.is_empty() || sid.len() > 4096 {
        return Err("invalid session id".to_owned());
    }
    with_db(path, |db, s| {
        let has_sessions = s.has("sessions") && s.session_cols.contains("id");
        let newest = if has_sessions {
            soft(newest_top_level(db, s), None)?
        } else {
            None
        };
        if !has_sessions {
            return Ok(DbData {
                live: None,
                chain: vec![sid.to_owned()],
                children: vec![],
                tasks: vec![],
                delegations: vec![],
                newest_top_level: None,
                tools_by_name: vec![],
                turns: None,
            });
        }
        let chain = chain_of(db, s, sid)?;
        let ph = placeholders(chain.len());
        let cols = s.session_cols_sql();
        let segs = sessions_where(
            db,
            &format!("SELECT {cols} FROM sessions s WHERE s.id IN ({ph})"),
            &chain,
        )?;
        let ordered: Vec<DbSession> = chain
            .iter()
            .filter_map(|id| segs.iter().find(|r| &r.id == id).cloned())
            .collect();
        let live = sum_chain(&ordered);
        let sub_only = if s.model_config() {
            format!("AND {} IS NOT NULL", s.deleg("s"))
        } else {
            String::new()
        };
        let both: Vec<String> = chain.iter().chain(chain.iter()).cloned().collect();
        let mut children = soft(
            sessions_where(
                db,
                &format!(
                    "SELECT {cols} FROM sessions s
                WHERE s.parent_session_id IN ({ph}) AND s.id NOT IN ({ph}) {sub_only} LIMIT 256"
                ),
                &both,
            ),
            vec![],
        )?;
        children.sort_by(|a, b| {
            a.started_at
                .unwrap_or(0.0)
                .total_cmp(&b.started_at.unwrap_or(0.0))
        });
        let tasks = soft(
            (|| {
                let mut st = db.prepare(&format!("SELECT task, model, api_call_count, estimated_cost_usd FROM session_model_usage WHERE session_id IN ({ph}) LIMIT 256"))?;
                let rows = st.query_map(params_from_iter(chain.iter()), |t| {
                    Ok(DbTask {
                        task: text(&val(t, "task")).unwrap_or_default(),
                        model: text(&val(t, "model")),
                        calls: n0(&val(t, "api_call_count")),
                        cost: num(&val(t, "estimated_cost_usd")),
                    })
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })(),
            vec![],
        )?;
        let delegations = soft(
            (|| {
                let mut st = db.prepare(&format!("SELECT delegation_id, state, dispatched_at, completed_at, task_json FROM async_delegations WHERE origin_session IN ({ph}) ORDER BY dispatched_at DESC LIMIT 10"))?;
                let rows = st.query_map(params_from_iter(chain.iter()), |d| {
                    let id = match val(d, "delegation_id") {
                        Value::Text(s) => s,
                        Value::Integer(i) => i.to_string(),
                        _ => "null".into(),
                    };
                    Ok(DbDelegation {
                        id,
                        state: text(&val(d, "state")),
                        dispatched_at: to_ms(&val(d, "dispatched_at")),
                        completed_at: to_ms(&val(d, "completed_at")),
                        goals: goals_of(text(&val(d, "task_json")).as_deref()),
                    })
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })(),
            vec![],
        )?;
        // slower aggregate over messages: at most every 10 s per chain
        let key = chain.clone();
        let stale = slow
            .as_ref()
            .is_none_or(|c| c.key != key || now < c.at || now - c.at >= SLOW_EVERY_MS);
        if stale {
            let agg = soft(
                (|| {
                    let mut st = db.prepare(&format!("SELECT tool_name, count(*) AS n FROM messages WHERE session_id IN ({ph}) AND role = 'tool' AND tool_name IS NOT NULL GROUP BY tool_name ORDER BY n DESC LIMIT 5"))?;
                    let tools = st
                        .query_map(params_from_iter(chain.iter()), |r| {
                            let name = match val(r, "tool_name") {
                                Value::Text(s) => s,
                                Value::Integer(i) => i.to_string(),
                                _ => String::new(),
                            };
                            Ok(DbToolCount {
                                name: truncate(&name, 32),
                                n: n0(&val(r, "n")),
                            })
                        })?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    let turns: Value = db.query_row(
                        &format!("SELECT count(*) AS n FROM messages WHERE session_id IN ({ph}) AND role = 'user'"),
                        params_from_iter(chain.iter()),
                        |r| r.get(0),
                    )?;
                    Ok((tools, Some(n0(&turns))))
                })(),
                (vec![], None),
            )?;
            *slow = Some(SlowCache {
                key: key.clone(),
                at: now,
                tools_by_name: agg.0,
                turns: agg.1,
            });
        }
        let cached = slow.as_ref().filter(|c| c.key == key);
        Ok(DbData {
            live,
            chain,
            children,
            tasks,
            delegations,
            newest_top_level: newest,
            tools_by_name: cached.map(|c| c.tools_by_name.clone()).unwrap_or_default(),
            turns: cached.and_then(|c| c.turns),
        })
    })
}

/// Habby `DbReader.history`: 84 days by start day, 14 days of models, top 5 chains this week,
/// lifetime cache savings.
pub fn history(path: &Path, now: f64) -> Result<HistoryRaw, String> {
    with_db(path, |db, s| {
        let dlg = s.deleg("s");
        let is_sub = if s.model_config() {
            format!("{dlg} IS NOT NULL")
        } else {
            format!(
                "(s.parent_session_id IS NOT NULL AND NOT {})",
                s.cont_of("s")
            )
        };
        let since = (now - 84.0 * 86400e3) / 1000.0;
        let days = soft(
            (|| {
                let mut st = db.prepare(&format!(
                    "SELECT date(s.started_at, 'unixepoch', 'localtime') AS d,
                    sum(s.parent_session_id IS NULL AND {dlg} IS NULL) AS n,
                    sum(coalesce(s.input_tokens,0) + coalesce(s.output_tokens,0) + coalesce(s.cache_read_tokens,0) + coalesce(s.cache_write_tokens,0)) AS tok,
                    sum(coalesce(s.estimated_cost_usd,0)) AS cost,
                    sum(CASE WHEN {is_sub} THEN coalesce(s.estimated_cost_usd,0) ELSE 0 END) AS sub
                FROM sessions s WHERE s.started_at >= ? GROUP BY d ORDER BY d DESC LIMIT 128"
                ))?;
                let rows = st.query_map([since], |r| {
                    Ok(HistoryDay {
                        d: str_of(&val(r, "d")),
                        sessions: n0(&val(r, "n")),
                        tokens: n0(&val(r, "tok")),
                        cost: n0(&val(r, "cost")),
                        sub_cost: n0(&val(r, "sub")),
                    })
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })(),
            vec![],
        )?;
        let models = soft(
            (|| {
                let mut st = db.prepare(
                    "SELECT date(first_seen, 'unixepoch', 'localtime') AS d, model, sum(coalesce(estimated_cost_usd,0)) AS c,
                    sum(coalesce(input_tokens,0) + coalesce(output_tokens,0) + coalesce(cache_read_tokens,0) + coalesce(cache_write_tokens,0)) AS t
                FROM session_model_usage WHERE first_seen >= ? GROUP BY d, model ORDER BY d DESC LIMIT 2048",
                )?;
                let rows = st.query_map([(now - 14.0 * 86400e3) / 1000.0], |r| {
                    Ok(HistoryModel {
                        d: str_of(&val(r, "d")),
                        model: text(&val(r, "model")).unwrap_or_default(),
                        cost: n0(&val(r, "c")),
                        tokens: n0(&val(r, "t")),
                    })
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })(),
            vec![],
        )?;
        // top sessions grouped by chain root: every row climbs its parent links (continuations
        // AND subagents), bounded to 20 hops, and is summed into that root
        let top = soft(
            (|| {
                let mut st = db.prepare(&format!(
                    "WITH RECURSIVE up(id, root, hops) AS (
                    SELECT id, id, 0 FROM sessions WHERE started_at >= ?
                    UNION ALL
                    SELECT up.id, c.parent_session_id, up.hops + 1 FROM up JOIN sessions c ON c.id = up.root
                    WHERE c.parent_session_id IS NOT NULL AND up.hops < {MAX_CHAIN_HOPS}
                ), f AS (SELECT id, root, max(hops) AS h FROM up GROUP BY id)
                SELECT f.root AS id, coalesce((SELECT title FROM sessions WHERE id = f.root), max(s.title)) AS title,
                    sum(coalesce(s.estimated_cost_usd,0)) AS cost, sum({dlg} IS NULL) AS segs
                FROM f JOIN sessions s ON s.id = f.id GROUP BY f.root ORDER BY cost DESC LIMIT 5"
                ))?;
                let rows = st.query_map([(now - 7.0 * 86400e3) / 1000.0], |r| {
                    Ok(HistoryTop {
                        id: str_of(&val(r, "id")),
                        title: text(&val(r, "title")),
                        cost: n0(&val(r, "cost")),
                        segments: n0(&val(r, "segs")).max(1.0),
                    })
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })(),
            vec![],
        )?;
        let savings = soft(
            (|| {
                let mut st = db.prepare(
                    "SELECT model, sum(cache_read_tokens) AS cr FROM session_model_usage GROUP BY model",
                )?;
                let rows = st.query_map([], |r| Ok((text(&val(r, "model")), n0(&val(r, "cr")))))?;
                let mut savings = 0.0;
                for row in rows {
                    let (model, cr) = row?;
                    if let Some((input, cache_read)) = price_for(model.as_deref()) {
                        savings += cr * (input - cache_read) / 1e6;
                    }
                }
                Ok(Some(savings))
            })(),
            None,
        )?;
        Ok(HistoryRaw {
            days,
            models,
            top,
            savings,
        })
    })
}

/// JS `String(x)` for the text-ish columns (`d`, root ids).
fn str_of(v: &Value) -> String {
    match v {
        Value::Text(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        _ => "null".into(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use super::*;
    use crate::services::scratch;

    /// Hermes schema subset (the columns Habby's queries touch).
    pub(crate) fn make_db(path: &Path) -> Connection {
        let db = Connection::open(path).unwrap();
        db.execute_batch(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT, model TEXT, model_config TEXT,
                parent_session_id TEXT, started_at REAL, ended_at REAL, end_reason TEXT,
                api_call_count INTEGER, tool_call_count INTEGER, input_tokens INTEGER,
                output_tokens INTEGER, cache_read_tokens INTEGER, cache_write_tokens INTEGER,
                estimated_cost_usd REAL, last_activity_at REAL);
             CREATE TABLE session_model_usage (session_id TEXT, model TEXT, task TEXT,
                api_call_count INTEGER, input_tokens INTEGER, output_tokens INTEGER,
                cache_read_tokens INTEGER, cache_write_tokens INTEGER, estimated_cost_usd REAL,
                first_seen REAL, last_seen REAL);
             CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, tool_name TEXT);
             CREATE TABLE async_delegations (delegation_id TEXT, origin_session TEXT, state TEXT,
                dispatched_at REAL, completed_at REAL, task_json TEXT);",
        )
        .unwrap();
        db
    }

    fn seed(db: &Connection, now_s: f64) {
        let t = now_s - 3600.0;
        db.execute_batch(&format!(
            "INSERT INTO sessions VALUES ('root','Root title','claude-opus-5-5',NULL,NULL,{t},{t2},'compression',3,2,100,10,1000,50,0.5,{t2});
             INSERT INTO sessions VALUES ('cont',NULL,'claude-opus-5-5',NULL,'root',{t2},NULL,NULL,4,1,200,20,2000,60,0.25,{t3});
             INSERT INTO sessions VALUES ('sub','Sub','claude-haiku-4-5','{{\"_delegate_from\":\"cont\"}}','cont',{t3},{t3},NULL,1,1,5,5,0,0,0.01,{t3});
             INSERT INTO session_model_usage VALUES ('root','claude-opus-5-5','main',3,100,10,1000000,50,0.5,{t},{t2});
             INSERT INTO session_model_usage VALUES ('cont','claude-opus-5-5','main',4,200,20,0,60,0.25,{t2},{t3});
             INSERT INTO messages (session_id, role, tool_name) VALUES ('root','user',NULL),('cont','user',NULL),
                ('cont','tool','terminal'),('cont','tool','terminal'),('root','tool','read_file');
             INSERT INTO async_delegations VALUES ('d1','cont','running',{t3},NULL,'{{\"tasks\":[{{\"goal\":\"fix\\nbug\"}}]}}');",
            t2 = t + 60.0,
            t3 = t + 120.0,
        ))
        .unwrap();
    }

    #[test]
    fn live_sums_the_compression_chain_with_children_and_slow_aggregate() {
        let dir = scratch("alive-db");
        let path = dir.join("state.db");
        let now = 1_800_000_000_000.0;
        seed(&make_db(&path), now / 1000.0);
        let mut slow = None;
        // asked for the ended root: the chain follows the continuation forward
        let d = live(&path, "root", now, &mut slow).unwrap();
        assert_eq!(d.chain, ["root", "cont"]);
        let l = d.live.as_ref().unwrap();
        assert_eq!(l.id, "cont");
        assert_eq!(l.title.as_deref(), Some("Root title"));
        assert_eq!((l.calls, l.input, l.tools), (7.0, 300.0, 3.0));
        assert_eq!(l.cost, Some(0.75));
        assert_eq!(
            l.started_at,
            Some(((now / 1000.0 - 3600.0) * 1000.0).round())
        );
        assert_eq!(d.children.len(), 1);
        assert_eq!(d.children[0].id, "sub");
        assert_eq!(d.tasks.len(), 2);
        assert_eq!(d.delegations[0].goals, ["fix bug"]);
        assert_eq!(d.tools_by_name[0].name, "terminal");
        assert_eq!(d.tools_by_name[0].n, 2.0);
        assert_eq!(d.turns, Some(2.0));
        assert_eq!(d.newest_top_level.as_deref(), Some("cont"));
        // the same chain from the tip
        let d2 = live(&path, "cont", now + 1.0, &mut slow).unwrap();
        assert_eq!(d2.chain, ["root", "cont"]);
        let h = history(&path, now).unwrap();
        assert_eq!(h.days.iter().map(|d| d.sessions).sum::<f64>(), 1.0);
        assert!((h.days.iter().map(|d| d.sub_cost).sum::<f64>() - 0.01).abs() < 1e-9);
        assert_eq!(h.top[0].id, "root");
        assert_eq!(h.top[0].segments, 2.0);
        assert!((h.top[0].cost - 0.76).abs() < 1e-9);
        // 1M cache-read tokens of Opus 5.5: (4 - 0.2) $ saved
        assert!((h.savings.unwrap() - 3.8).abs() < 1e-9);
        assert_eq!(h.models.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_tables_and_columns_fail_soft_and_missing_db_is_an_error() {
        let dir = scratch("alive-db-min");
        let path = dir.join("state.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE sessions (id TEXT, parent_session_id TEXT, started_at REAL, ended_at REAL);
             INSERT INTO sessions VALUES ('a', NULL, 1700000000, NULL);",
        )
        .unwrap();
        drop(db);
        let mut slow = None;
        let d = live(&path, "a", 0.0, &mut slow).unwrap();
        assert_eq!(d.live.as_ref().unwrap().id, "a");
        assert_eq!(d.live.as_ref().unwrap().cost, None);
        assert!(d.tasks.is_empty() && d.delegations.is_empty() && d.tools_by_name.is_empty());
        assert_eq!(d.turns, None);
        let h = history(&path, 1_700_000_100_000.0).unwrap();
        assert_eq!(h.savings, None);
        assert!(h.models.is_empty());
        let e = live(&dir.join("nope.db"), "a", 0.0, &mut slow).unwrap_err();
        assert_eq!(e, "state.db not found");
        // the reader never creates journal files next to a rollback-journal db
        assert!(!dir.join("state.db-journal").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn minimal_session_schema_does_not_require_parent_or_optional_columns() {
        let dir = scratch("alive-db-id-only");
        let path = dir.join("state.db");
        let writer = Connection::open(&path).unwrap();
        writer.execute_batch("CREATE TABLE sessions (id TEXT, end_reason TEXT); INSERT INTO sessions VALUES ('s', NULL);").unwrap();
        drop(writer);
        let data = live(&path, "s", 1.0, &mut None).unwrap();
        assert_eq!(data.live.unwrap().id, "s");
        assert_eq!(data.newest_top_level.as_deref(), Some("s"));
        assert!(data.children.is_empty());
        assert!(history(&path, 1.0).unwrap().days.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn session_ids_are_bound_values_and_cache_keys_cannot_collide() {
        let dir = scratch("alive-db-bound-ids");
        let path = dir.join("state.db");
        let writer = make_db(&path);
        let injection = "x'); DROP TABLE sessions; --";
        for id in [injection, "a", "b", "a,b"] {
            writer
                .execute("INSERT INTO sessions (id) VALUES (?)", [id])
                .unwrap();
        }
        writer.execute_batch("UPDATE sessions SET end_reason = 'compression' WHERE id = 'a'; UPDATE sessions SET parent_session_id = 'a' WHERE id = 'b'; INSERT INTO messages (session_id, role) VALUES ('a','user'),('b','user'),('a,b','user');").unwrap();
        let mut cache = None;
        assert_eq!(
            live(&path, injection, 1.0, &mut cache)
                .unwrap()
                .live
                .unwrap()
                .id,
            injection
        );
        let chain = live(&path, "a", 2.0, &mut cache).unwrap();
        assert_eq!(chain.chain, ["a", "b"]);
        assert_eq!(chain.turns, Some(2.0));
        let comma = live(&path, "a,b", 3.0, &mut cache).unwrap();
        assert_eq!(comma.chain, ["a,b"]);
        assert_eq!(comma.turns, Some(1.0));
        assert!(live(&path, &"x".repeat(4097), 4.0, &mut cache).is_err());
        drop(writer);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn compression_cycles_are_bounded_and_clock_rewind_refreshes_aggregates() {
        let dir = scratch("alive-db-cycles");
        let path = dir.join("state.db");
        let writer = make_db(&path);
        writer.execute_batch("INSERT INTO sessions (id, parent_session_id, end_reason, started_at) VALUES ('a','b','compression',1),('b','a','compression',2); INSERT INTO messages (session_id, role) VALUES ('a','user');").unwrap();
        let mut cache = None;
        let first = live(&path, "a", 100.0, &mut cache).unwrap();
        assert_eq!(first.chain.len(), 2);
        assert_eq!(first.turns, Some(1.0));
        writer
            .execute(
                "INSERT INTO messages (session_id, role) VALUES ('a','user')",
                [],
            )
            .unwrap();
        assert_eq!(live(&path, "a", 99.0, &mut cache).unwrap().turns, Some(2.0));
        assert!(!history(&path, 100.0).unwrap().top.is_empty());
        drop(writer);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn delegated_child_of_a_compressed_parent_is_not_a_continuation() {
        let dir = scratch("alive-db-delegate-boundary");
        let path = dir.join("state.db");
        let writer = make_db(&path);
        writer.execute_batch("INSERT INTO sessions (id, end_reason, api_call_count) VALUES ('root', 'compression', 10); INSERT INTO sessions (id, parent_session_id, model_config, api_call_count) VALUES ('sub', 'root', '{\"_delegate_from\":\"root\"}', 2);").unwrap();
        let child = live(&path, "sub", 1.0, &mut None).unwrap();
        assert_eq!(child.chain, ["sub"]);
        assert_eq!(child.live.unwrap().calls, 2.0);
        let root = live(&path, "root", 1.0, &mut None).unwrap();
        assert_eq!(root.chain, ["root"]);
        assert_eq!(root.children[0].id, "sub");
        drop(writer);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn read_only_connection_rejects_writes_and_uri_escaping_preserves_path_bytes() {
        use std::os::unix::ffi::OsStringExt;
        let dir = scratch("alive-db-uri");
        let path = dir.join(std::ffi::OsString::from_vec(b"state ?#%\xff.db".to_vec()));
        drop(make_db(&path));
        let before = std::fs::read(&path).unwrap();
        let reader = open(&path).unwrap();
        assert!(reader.execute("DELETE FROM sessions", []).is_err());
        assert!(reader
            .execute_batch("PRAGMA query_only=OFF; CREATE TABLE forbidden (id TEXT);")
            .is_err());
        drop(reader);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let uri = ro_uri(&path).unwrap();
        assert!(uri.contains("%20%3F%23%25%FF"));
        assert!(uri.ends_with("?mode=ro&readonly_shm=1"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn fixture_files(dir: &Path) -> std::collections::BTreeMap<std::ffi::OsString, Vec<u8>> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (e.file_name(), std::fs::read(e.path()).unwrap())
            })
            .collect()
    }

    #[test]
    fn committed_wal_data_is_visible_without_modifying_db_wal_or_shm() {
        let dir = scratch("alive-db-readonly-wal");
        let path = dir.join("state.db");
        let writer = make_db(&path);
        writer.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; INSERT INTO sessions (id, started_at) VALUES ('wal-only', 1);").unwrap();
        // Establish the writer's latest read mark before taking a byte-for-byte fixture
        // image. The read-only bridge must not allocate/change SHM read marks itself.
        writer
            .query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap();
        let before = fixture_files(&dir);
        assert!(dir.join("state.db-wal").exists() && dir.join("state.db-shm").exists());
        let data = live(&path, "wal-only", 1.0, &mut None).unwrap();
        assert_eq!(data.live.unwrap().id, "wal-only");
        assert!(history(&path, 1.0).is_ok());
        assert_eq!(fixture_files(&dir), before);
        drop(writer);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn wal_header_without_sidecars_never_creates_them() {
        let dir = scratch("alive-db-no-wal-sidecars");
        let path = dir.join("state.db");
        let writer = make_db(&path);
        writer
            .execute_batch("PRAGMA journal_mode=WAL; INSERT INTO sessions (id) VALUES ('s');")
            .unwrap();
        drop(writer);
        let before = fixture_files(&dir);
        assert_eq!(before.len(), 1);
        // SQLite may need WAL recovery. It is preferable to fail soft rather than
        // create sidecars, checkpoint, or pretend immutable=1 is a valid live snapshot.
        let _ = live(&path, "s", 1.0, &mut None);
        let _ = history(&path, 1.0);
        assert_eq!(fixture_files(&dir), before);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn busy_and_corrupt_sources_propagate_instead_of_becoming_empty_successes() {
        let dir = scratch("alive-db-errors");
        let path = dir.join("state.db");
        let writer = make_db(&path);
        writer.execute_batch("BEGIN EXCLUSIVE;").unwrap();
        let error = live(&path, "s", 1.0, &mut None).unwrap_err();
        assert!(is_busy_error(&error), "{error}");
        writer.execute_batch("ROLLBACK;").unwrap();
        drop(writer);
        std::fs::write(&path, "not a SQLite database").unwrap();
        assert!(live(&path, "s", 1.0, &mut None).is_err());
        let error = rusqlite::Error::SqliteFailure(
            ffi::Error::new(ffi::SQLITE_CORRUPT),
            Some("corrupt fixture".into()),
        );
        assert!(soft::<usize>(Err(error), 0).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn value_helpers_match_habby() {
        assert_eq!(to_ms(&Value::Real(1.5)), Some(1500.0));
        assert_eq!(to_ms(&Value::Integer(1_700_000_000_000)), Some(1.7e12));
        assert_eq!(to_ms(&Value::Text("1700000000".into())), Some(1.7e12));
        assert_eq!(
            to_ms(&Value::Text("2026-01-01T00:00:00Z".into())),
            Some(1_767_225_600_000.0)
        );
        assert_eq!(to_ms(&Value::Text("x".into())), None);
        assert_eq!(to_ms(&Value::Null), None);
        assert_eq!(to_ms(&Value::Real(-f64::MAX)), None);
        assert_eq!(truncate("  a\n\tb  ", 80), "a b");
        assert_eq!(truncate("abcdef", 4), "abc\u{2026}");
        assert_eq!(truncate("anything", 0), "");
        assert_eq!(
            goals_of(Some("[{\"goal\":\"g1\"},{\"x\":{\"goal\":\"g2\"}}]")),
            ["g1", "g2"]
        );
        assert!(goals_of(Some("nope")).is_empty());
        assert_eq!(
            price_for(Some("anthropic/claude-opus-5-5[1m]")),
            Some((4.0, 0.2))
        );
        assert_eq!(price_for(Some("claude-opus-5-50")), None);
        assert!(is_busy_error("database is locked"));
        assert!(!is_busy_error("no such table: x"));
    }
}
