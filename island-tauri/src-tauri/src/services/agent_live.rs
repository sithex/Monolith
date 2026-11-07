//! AI Agents "Live" page bridge (docs/agent-live.md): a dumb, read-only reader of the Hermes
//! home (plugin JSON files, `/proc`, `logs/agent.log`, `state.db`) plus the island-owned
//! 5-hour block history file. Interpretation stays in the TS model (Habby's logic).

mod db;
mod log;

use crate::contract::agent_live::{
    AgentLiveBlock, AgentLiveBlocks, AgentLiveBlocksOp, AgentLiveProc, AgentLiveRaw,
    AgentLiveRequest, AgentLiveSnapshot, HistoryRaw,
};
use crate::error::IslandError;
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// Kept newest entries of the block history.
pub const MAX_BLOCKS: usize = 200;
/// `history` is re-queried at most this often.
pub const HISTORY_EVERY_MS: f64 = 60e3;
const MAX_JSON: u64 = 2 * 1024 * 1024;
const MAX_SNAPSHOTS: usize = 32;
const MAX_DIRECTORY_ENTRIES: usize = 4096;
const MAX_BLOCK_BYTES: u64 = 128 * 1024;

#[derive(Default)]
struct Cache {
    slow: Option<db::SlowCache>,
    history: Option<HistoryRaw>,
    history_at: Option<f64>,
    history_error: Option<String>,
}

pub struct AgentLiveService {
    hermes_home: PathBuf,
    blocks_file: PathBuf,
    cache: Mutex<Cache>,
    blocks_lock: Mutex<()>,
}

impl AgentLiveService {
    /// `hermes_home` = `HERMES_HOME` or `~/.hermes` (see [`super::profile_dir`]); the block
    /// history lives in the island data dir.
    pub fn new(hermes_home: PathBuf, data_dir: &Path) -> Self {
        Self {
            hermes_home,
            blocks_file: data_dir.join("agent-live-blocks.json"),
            cache: Mutex::new(Cache::default()),
            blocks_lock: Mutex::new(()),
        }
    }

    /// One poll of every source; each fails soft into its `*Error` field.
    pub fn raw(&self, req: &AgentLiveRequest, terminal_pid: Option<u32>) -> AgentLiveRaw {
        let home = &self.hermes_home;
        let now = now_ms();
        let metrics = home.join("cache/habby-metrics");
        let (active_sessions, active_sessions_error) =
            read_json(&home.join("runtime/active_sessions.json"), MAX_JSON);
        let (processes, _) = read_json(&home.join("processes.json"), 512 * 1024);
        let (snapshots, snapshot_error) = read_snapshots(&metrics);
        let (plan, plan_error) = read_json(&metrics.join("plan.json"), MAX_JSON);
        let plan_error = plan_error.map(|e| {
            if e == "not found" {
                "not installed".to_owned()
            } else {
                e
            }
        });
        let log = log::read(&home.join("logs/agent.log"), req.log_cursor.as_deref());
        let db_path = home.join("state.db");
        let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        let (mut db, mut db_error, mut db_busy) = (None, None, false);
        if let Some(sid) = req.session_id.as_deref() {
            match db::live(&db_path, sid, now, &mut cache.slow) {
                Ok(d) => db = Some(d),
                Err(e) => {
                    db_busy = db::is_busy_error(&e);
                    db_error = Some(e);
                }
            }
        }
        let (history, history_error) = if req.history {
            if !db_busy
                && cache
                    .history_at
                    .is_none_or(|at| now < at || now - at >= HISTORY_EVERY_MS)
            {
                cache.history_at = Some(now);
                match db::history(&db_path, now) {
                    Ok(h) => {
                        cache.history = Some(h);
                        cache.history_error = None;
                    }
                    Err(e) if db::is_busy_error(&e) => cache.history_error = Some("db busy".into()),
                    Err(e) => cache.history_error = Some(e),
                }
            }
            (cache.history.clone(), cache.history_error.clone())
        } else {
            (None, None)
        };
        drop(cache);
        AgentLiveRaw {
            now,
            hermes_home: home.to_string_lossy().into_owned(),
            island_pid: std::process::id(),
            terminal_pid,
            procs: scan_procs(Path::new("/proc"), current_uid()),
            active_sessions,
            active_sessions_error,
            processes,
            snapshots,
            snapshot_error,
            plan,
            plan_error,
            log,
            db,
            db_error,
            db_busy,
            history,
            history_error,
        }
    }

    pub fn blocks(&self, op: AgentLiveBlocksOp) -> Result<AgentLiveBlocks, IslandError> {
        let _guard = self.blocks_lock.lock().unwrap_or_else(|p| p.into_inner());
        match op {
            AgentLiveBlocksOp::Get {} => Ok(AgentLiveBlocks {
                blocks: read_blocks(&self.blocks_file),
            }),
            AgentLiveBlocksOp::Set { blocks } => {
                let blocks = cap_blocks(blocks);
                self.check_blocks_location()?;
                write_atomic(
                    &self.blocks_file,
                    &serde_json::to_vec(&blocks)
                        .map_err(|e| IslandError::internal(format!("blocks encode failed: {e}")))?,
                )?;
                Ok(AgentLiveBlocks { blocks })
            }
        }
    }

    /// The caller supplies the island data directory, but never allow a misconfiguration
    /// (including an existing symlink) to turn this one writer into a Hermes-home writer.
    fn check_blocks_location(&self) -> Result<(), IslandError> {
        let home = resolved_path(&self.hermes_home)
            .map_err(|e| IslandError::io(format!("Could not resolve Hermes home: {e}")))?;
        let target = resolved_path(&self.blocks_file)
            .map_err(|e| IslandError::io(format!("Could not resolve block history: {e}")))?;
        if target.starts_with(&home) {
            return Err(IslandError::io(
                "Block history must be outside the Hermes home",
            ));
        }
        // A target symlink is replaced by rename, not followed; check the parent too.
        if resolved_path(self.blocks_file.parent().unwrap_or(Path::new(".")))
            .map_err(|e| {
                IslandError::io(format!("Could not resolve block history directory: {e}"))
            })?
            .starts_with(&home)
        {
            return Err(IslandError::io(
                "Block history must be outside the Hermes home",
            ));
        }
        Ok(())
    }
}

/// Resolve existing symlinks as well as `..`, including paths not created yet.
fn resolved_path(path: &Path) -> io::Result<PathBuf> {
    use std::path::Component;
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => {
                out.push(other.as_os_str());
                match fs::canonicalize(&out) {
                    Ok(real) => out = real,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
        }
    }
    Ok(out)
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

fn mtime_ms(m: &fs::Metadata) -> f64 {
    m.mtime() as f64 * 1000.0 + (m.mtime_nsec() as f64 / 1e6).floor()
}

/// Never follow an input-file symlink or block on a FIFO/device. Metadata is inspected on
/// the same descriptor used to read, and every caller must additionally bound its read.
fn open_regular(path: &Path) -> io::Result<fs::File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    Ok(file)
}

fn read_bounded(file: &Path, max: u64) -> io::Result<Vec<u8>> {
    let source = open_regular(file)?;
    if source.metadata()?.len() > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "file too large"));
    }
    let mut bytes = Vec::new();
    source.take(max.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "file too large"));
    }
    Ok(bytes)
}

/// Small JSON file -> value; `Null` + Habby's reason when missing, too big or unparseable.
fn read_json(file: &Path, max: u64) -> (serde_json::Value, Option<String>) {
    let fail = |e: &str| (serde_json::Value::Null, Some(e.to_owned()));
    match read_bounded(file, max) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(v) => (v, None),
            Err(_) => fail("invalid json"),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => fail("not found"),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => fail("file too large"),
        Err(_) => fail("unreadable"),
    }
}

/// Every `snapshot-<pid>.json` that parses, newest mtime first (at most 32 files read).
fn read_snapshots(dir: &Path) -> (Vec<AgentLiveSnapshot>, Option<String>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (Vec::new(), Some("not installed".into()))
        }
        Err(_) => return (Vec::new(), Some("unreadable".into())),
    };
    let mut files: Vec<(u32, f64, PathBuf)> = entries
        .take(MAX_DIRECTORY_ENTRIES)
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let pid = name
                .to_str()?
                .strip_prefix("snapshot-")?
                .strip_suffix(".json")?;
            if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let pid: u32 = pid.parse().ok().filter(|p| *p != 0)?;
            if !e.file_type().ok()?.is_file() {
                return None;
            }
            let mtime = e.metadata().ok().map(|m| mtime_ms(&m)).unwrap_or(0.0);
            Some((pid, mtime, e.path()))
        })
        .collect();
    files.sort_by(|a, b| b.1.total_cmp(&a.1));
    let out = files
        .into_iter()
        .take(MAX_SNAPSHOTS)
        .filter_map(|(pid, mtime_ms, path)| {
            let (data, err) = read_json(&path, MAX_JSON);
            err.is_none().then(|| AgentLiveSnapshot {
                pid,
                alive: Path::new("/proc").join(pid.to_string()).exists(),
                mtime_ms,
                data,
            })
        })
        .collect();
    (out, None)
}

/// This user's processes (`uid` owns `/proc/<pid>`); `None` when `proc` is unreadable.
fn scan_procs(proc: &Path, uid: u32) -> Option<Vec<AgentLiveProc>> {
    let mut out = Vec::new();
    for e in fs::read_dir(proc).ok()?.take(32768).flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let dir = e.path();
        match fs::metadata(&dir) {
            Ok(m) if m.uid() == uid => {}
            _ => continue,
        }
        // ppid is the 2nd field after the parenthesised comm (which may contain spaces)
        let Some(ppid) = read_bounded(&dir.join("stat"), 65536).ok().and_then(|b| {
            let s = String::from_utf8(b).ok()?;
            s.rsplit_once(')')?
                .1
                .split_whitespace()
                .nth(1)?
                .parse::<u32>()
                .ok()
        }) else {
            continue;
        };
        let exe = fs::read_link(dir.join("exe"))
            .ok()
            .and_then(|p| {
                let name = p.file_name()?.to_string_lossy().into_owned();
                // a replaced binary reads as "<name> (deleted)"
                Some(name.trim_end_matches(" (deleted)").to_owned())
            })
            .or_else(|| {
                read_bounded(&dir.join("comm"), 4096)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
                    .map(|c| c.trim_end().to_owned())
            })
            .unwrap_or_default();
        out.push(AgentLiveProc { pid, ppid, exe });
    }
    Some(out)
}

fn valid(b: &AgentLiveBlock) -> bool {
    b.reset_at.is_finite() && b.peak.is_finite() && b.reset_at >= 0.0 && b.peak >= 0.0
}

/// Valid entries only, the newest (`resetAt`) `MAX_BLOCKS`, oldest first.
fn cap_blocks(mut blocks: Vec<AgentLiveBlock>) -> Vec<AgentLiveBlock> {
    blocks.retain(valid);
    blocks.sort_by(|a, b| a.reset_at.total_cmp(&b.reset_at));
    let extra = blocks.len().saturating_sub(MAX_BLOCKS);
    blocks.drain(..extra);
    blocks
}

/// Habby's tolerant read: anything unreadable is an empty history.
fn read_blocks(file: &Path) -> Vec<AgentLiveBlock> {
    let Ok(bytes) = read_bounded(file, MAX_BLOCK_BYTES) else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Array(items)) = serde_json::from_slice(&bytes) else {
        return Vec::new();
    };
    let blocks = items
        .into_iter()
        .filter_map(|v| {
            Some(AgentLiveBlock {
                reset_at: v.get("resetAt")?.as_f64()?,
                peak: v.get("peak")?.as_f64()?,
            })
        })
        .collect();
    cap_blocks(blocks)
}

/// Write to a sibling temp file, fsync, rename over the target.
fn write_atomic(file: &Path, bytes: &[u8]) -> Result<(), IslandError> {
    let io = |e: std::io::Error| IslandError::io(format!("Could not save block history: {e}"));
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(io)?;
    }
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let tmp = file.with_extension(format!(
        "json.tmp-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut created = false;
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        created = true;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, file)?;
        if let Some(parent) = file.parent() {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() && created {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(io)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::services::scratch;
    use serde_json::json;

    fn req(session: Option<&str>, history: bool) -> AgentLiveRequest {
        AgentLiveRequest {
            session_id: session.map(str::to_owned),
            log_cursor: None,
            history,
        }
    }

    #[test]
    fn empty_home_fails_soft_everywhere() {
        let dir = scratch("alive-empty");
        let svc = AgentLiveService::new(dir.join("hermes"), &dir.join("data"));
        let raw = svc.raw(&req(Some("s"), true), Some(42));
        assert_eq!(raw.terminal_pid, Some(42));
        assert_eq!(raw.island_pid, std::process::id());
        assert!(raw.active_sessions.is_null());
        assert_eq!(raw.active_sessions_error.as_deref(), Some("not found"));
        assert!(raw.processes.is_null());
        assert!(raw.snapshots.is_empty());
        assert_eq!(raw.snapshot_error.as_deref(), Some("not installed"));
        assert_eq!(raw.plan_error.as_deref(), Some("not installed"));
        assert_eq!(raw.log.error.as_deref(), Some("agent.log not found"));
        assert!(raw.db.is_none());
        assert_eq!(raw.db_error.as_deref(), Some("state.db not found"));
        assert!(!raw.db_busy);
        assert!(raw.history.is_none());
        assert_eq!(raw.history_error.as_deref(), Some("state.db not found"));
        // the read never creates anything in the Hermes home
        assert!(!dir.join("hermes").exists());
        let none = svc.raw(&req(None, false), None);
        assert!(none.db.is_none() && none.db_error.is_none() && none.history_error.is_none());
        let wire = serde_json::to_value(&none).unwrap();
        for key in [
            "hermesHome",
            "terminalPid",
            "activeSessionsError",
            "dbBusy",
            "historyError",
        ] {
            assert!(wire.get(key).is_some(), "{key}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_plugin_is_a_normal_read_only_poll_of_an_existing_home() {
        let dir = scratch("alive-no-plugin");
        let home = dir.join("hermes");
        fs::create_dir_all(home.join("logs")).unwrap();
        fs::create_dir_all(home.join("runtime")).unwrap();
        fs::write(home.join("logs/agent.log"), "fixture only\n").unwrap();
        fs::write(home.join("runtime/active_sessions.json"), "{}").unwrap();
        let path = home.join("state.db");
        db::tests::make_db(&path)
            .execute("INSERT INTO sessions (id, started_at) VALUES ('s', 1)", [])
            .unwrap();
        let before = fs::read(&path).unwrap();
        let svc = AgentLiveService::new(home.clone(), &dir.join("data"));
        for _ in 0..3 {
            let raw = svc.raw(&req(Some("s"), true), None);
            assert!(raw.snapshots.is_empty() && raw.plan.is_null());
            assert_eq!(raw.snapshot_error.as_deref(), Some("not installed"));
            assert_eq!(raw.plan_error.as_deref(), Some("not installed"));
            assert!(raw.db.is_some() && raw.db_error.is_none() && !raw.db_busy);
            assert!(raw.history.is_some() && raw.history_error.is_none());
            assert!(raw.log.error.is_none() && raw.active_sessions_error.is_none());
        }
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(!home.join("cache").exists());
        for suffix in ["db-journal", "db-wal", "db-shm"] {
            assert!(!path.with_extension(suffix).exists());
        }
        assert!(!dir.join("data").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_history_queries_are_throttled_too() {
        let dir = scratch("alive-history-throttle");
        let home = dir.join("hermes");
        fs::create_dir(&home).unwrap();
        let svc = AgentLiveService::new(home.clone(), &dir.join("data"));
        assert!(svc.raw(&req(None, true), None).history_error.is_some());
        let first_at = svc.cache.lock().unwrap().history_at;
        drop(db::tests::make_db(&home.join("state.db")));
        assert!(svc.raw(&req(None, true), None).history_error.is_some());
        assert_eq!(svc.cache.lock().unwrap().history_at, first_at);
        svc.cache.lock().unwrap().history_at = None;
        assert!(svc.raw(&req(None, true), None).history_error.is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn json_reads_are_bounded_and_reject_nonregular_files_and_symlinks() {
        use std::os::unix::{ffi::OsStrExt, fs::symlink};
        let dir = scratch("alive-json-bounds");
        let file = dir.join("input.json");
        fs::write(&file, "{}").unwrap();
        assert_eq!(read_json(&file, 2), (json!({}), None));
        assert_eq!(read_json(&file, 1).1.as_deref(), Some("file too large"));
        let link = dir.join("link.json");
        symlink(&file, &link).unwrap();
        assert_eq!(read_json(&link, MAX_JSON).1.as_deref(), Some("unreadable"));
        assert_eq!(read_json(&dir, MAX_JSON).1.as_deref(), Some("unreadable"));
        fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .unwrap()
            .set_len(MAX_JSON + 1)
            .unwrap();
        assert_eq!(
            read_json(&file, MAX_JSON).1.as_deref(),
            Some("file too large")
        );
        // A FIFO must not block the polling thread.
        let fifo = dir.join("fifo.json");
        let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: name is a valid NUL-terminated fixture path; mkfifo creates only it.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert_eq!(read_json(&fifo, MAX_JSON).1.as_deref(), Some("unreadable"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn snapshots_are_capped_and_nonfiles_are_not_followed() {
        let dir = scratch("alive-snapshot-cap");
        for pid in 1..=40 {
            fs::write(dir.join(format!("snapshot-{pid}.json")), "{}").unwrap();
        }
        fs::write(dir.join("snapshot-0.json"), "{}").unwrap();
        fs::create_dir(dir.join("snapshot-100.json")).unwrap();
        std::os::unix::fs::symlink(dir.join("snapshot-1.json"), dir.join("snapshot-101.json"))
            .unwrap();
        let (snapshots, error) = read_snapshots(&dir);
        assert!(error.is_none());
        assert_eq!(snapshots.len(), MAX_SNAPSHOTS);
        assert!(snapshots.iter().all(|s| (1..=40).contains(&s.pid)));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn blocks_reject_hermes_destinations_including_symlinks_and_parent_segments() {
        let dir = scratch("alive-block-location");
        let home = dir.join("hermes");
        fs::create_dir(&home).unwrap();
        std::os::unix::fs::symlink(&home, dir.join("data-link")).unwrap();
        for data in [
            home.clone(),
            home.join("new/subdir"),
            dir.join("data-link"),
            home.join("../hermes/nested"),
        ] {
            let svc = AgentLiveService::new(home.clone(), &data);
            assert!(svc
                .blocks(AgentLiveBlocksOp::Set { blocks: vec![] })
                .is_err());
        }
        assert_eq!(fs::read_dir(&home).unwrap().count(), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_blocks_and_oversized_persisted_history_are_dropped() {
        let dir = scratch("alive-block-bounds");
        let blocks = cap_blocks(vec![
            AgentLiveBlock {
                reset_at: f64::NAN,
                peak: 1.0,
            },
            AgentLiveBlock {
                reset_at: 1.0,
                peak: f64::INFINITY,
            },
            AgentLiveBlock {
                reset_at: -1.0,
                peak: 1.0,
            },
            AgentLiveBlock {
                reset_at: 1.0,
                peak: -1.0,
            },
            AgentLiveBlock {
                reset_at: 1.0,
                peak: 2.0,
            },
        ]);
        assert_eq!(
            blocks,
            [AgentLiveBlock {
                reset_at: 1.0,
                peak: 2.0
            }]
        );
        let path = dir.join("agent-live-blocks.json");
        fs::write(&path, "[]").unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_BLOCK_BYTES + 1)
            .unwrap();
        assert!(read_blocks(&path).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_atomic_replace_does_not_remove_existing_data_or_leave_our_tempfile() {
        let dir = scratch("alive-block-failure");
        let destination = dir.join("agent-live-blocks.json");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("marker"), "untouched").unwrap();
        assert!(write_atomic(&destination, b"[]").is_err());
        assert_eq!(fs::read(destination.join("marker")).unwrap(), b"untouched");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn independent_service_writers_use_private_unique_atomic_temporary_files() {
        let dir = scratch("alive-block-concurrent");
        let home = dir.join("hermes");
        let data = dir.join("data");
        let mut writers = Vec::new();
        for i in 0..8 {
            let svc = AgentLiveService::new(home.clone(), &data);
            writers.push(std::thread::spawn(move || {
                for _ in 0..8 {
                    svc.blocks(AgentLiveBlocksOp::Set {
                        blocks: vec![AgentLiveBlock {
                            reset_at: i as f64,
                            peak: 2.0,
                        }],
                    })
                    .unwrap();
                }
            }));
        }
        for writer in writers {
            writer.join().unwrap();
        }
        let file = data.join("agent-live-blocks.json");
        assert_eq!(read_blocks(&file).len(), 1);
        assert_eq!(fs::metadata(file).unwrap().mode() & 0o777, 0o600);
        assert_eq!(fs::read_dir(&data).unwrap().count(), 1);
        assert!(!home.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn plugin_files_snapshots_and_db_are_read() {
        let dir = scratch("alive-files");
        let home = dir.join("hermes");
        let metrics = home.join("cache/habby-metrics");
        fs::create_dir_all(&metrics).unwrap();
        fs::create_dir_all(home.join("runtime")).unwrap();
        fs::write(home.join("runtime/active_sessions.json"), "{\"a\":1}").unwrap();
        fs::write(home.join("processes.json"), "[1,2]").unwrap();
        fs::write(metrics.join("plan.json"), "nope").unwrap();
        let me = std::process::id();
        fs::write(metrics.join(format!("snapshot-{me}.json")), "{\"pid\":1}").unwrap();
        fs::write(metrics.join("snapshot-1x.json"), "{}").unwrap();
        fs::write(metrics.join("snapshot-4000000000.json"), "{bad").unwrap();
        db::tests::make_db(&home.join("state.db"))
            .execute(
                "INSERT INTO sessions (id, started_at) VALUES ('s', 1.0)",
                [],
            )
            .unwrap();
        let svc = AgentLiveService::new(home.clone(), &dir.join("data"));
        let raw = svc.raw(&req(Some("s"), true), None);
        assert_eq!(raw.active_sessions, json!({"a": 1}));
        assert_eq!(raw.processes, json!([1, 2]));
        assert_eq!(raw.plan_error.as_deref(), Some("invalid json"));
        assert_eq!(raw.snapshots.len(), 1);
        assert_eq!(raw.snapshots[0].pid, me);
        assert!(raw.snapshots[0].alive);
        assert!(raw.snapshot_error.is_none());
        assert_eq!(raw.db.as_ref().unwrap().live.as_ref().unwrap().id, "s");
        assert!(raw.history.is_some() && raw.history_error.is_none());
        let procs = raw.procs.unwrap();
        assert!(procs.iter().any(|p| p.pid == me && !p.exe.is_empty()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn blocks_round_trip_atomically_and_keep_the_newest_200() {
        let dir = scratch("alive-blocks");
        let svc = AgentLiveService::new(dir.join("hermes"), &dir.join("data"));
        assert!(svc
            .blocks(AgentLiveBlocksOp::Get {})
            .unwrap()
            .blocks
            .is_empty());
        let many: Vec<AgentLiveBlock> = (0..250)
            .rev()
            .map(|i| AgentLiveBlock {
                reset_at: i as f64,
                peak: 1.0,
            })
            .collect();
        let set = svc.blocks(AgentLiveBlocksOp::Set { blocks: many }).unwrap();
        assert_eq!(set.blocks.len(), MAX_BLOCKS);
        assert_eq!(set.blocks[0].reset_at, 50.0);
        assert_eq!(set.blocks[MAX_BLOCKS - 1].reset_at, 249.0);
        let got = svc.blocks(AgentLiveBlocksOp::Get {}).unwrap();
        assert_eq!(got, set);
        let data = dir.join("data");
        let names: Vec<_> = fs::read_dir(&data)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            ["agent-live-blocks.json"],
            "no temp file left behind"
        );
        // Habby-tolerant read: junk entries dropped, a corrupt file is an empty history
        fs::write(
            data.join("agent-live-blocks.json"),
            "[{\"resetAt\":1,\"peak\":2},{\"resetAt\":\"x\"},3]",
        )
        .unwrap();
        assert_eq!(
            svc.blocks(AgentLiveBlocksOp::Get {}).unwrap().blocks.len(),
            1
        );
        fs::write(data.join("agent-live-blocks.json"), "{oops").unwrap();
        assert!(svc
            .blocks(AgentLiveBlocksOp::Get {})
            .unwrap()
            .blocks
            .is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn proc_scan_filters_by_uid_and_parses_stat() {
        let procs = scan_procs(Path::new("/proc"), current_uid()).unwrap();
        let me = procs.iter().find(|p| p.pid == std::process::id()).unwrap();
        // SAFETY: getppid has no preconditions.
        assert_eq!(me.ppid, unsafe { libc::getppid() } as u32);
        assert!(!procs.iter().any(|p| p.pid == 1) || current_uid() == 0);
        assert!(scan_procs(Path::new("/no/such/proc"), 0).is_none());
    }
}
