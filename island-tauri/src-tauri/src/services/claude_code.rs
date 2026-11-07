//! Read-only Claude registry and incremental transcript reader.
//! No credentials, network, transcript text, tool inputs, or free-form registry titles in IPC.
mod scan;
use crate::contract::claude_code::{ClaudeCodeRaw, ClaudeCodeRequest, ClaudeRegEntry};
use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_BUDGET: u64 = 16 * 1024 * 1024;
pub const MAX_LINE: usize = 8 * 1024 * 1024;
pub const MAX_FILES: usize = 5000;
pub const MAX_REGISTRY: usize = 200;
const MAX_REG_JSON: u64 = 1024 * 1024;

pub struct ClaudeCodeService {
    claude_dir: PathBuf,
    proc_root: PathBuf,
    // Metadata only: unchanged unfinished EOF tails must not starve later files' IO budget.
    tails: Mutex<scan::TailCache>,
}
impl ClaudeCodeService {
    pub fn new(claude_dir: PathBuf) -> Self {
        Self {
            claude_dir,
            proc_root: PathBuf::from("/proc"),
            tails: Mutex::new(HashMap::new()),
        }
    }
    pub fn raw(&self, req: &ClaudeCodeRequest) -> ClaudeCodeRaw {
        let dir = &self.claude_dir;
        let mut raw = ClaudeCodeRaw {
            now: now_ms(),
            claude_dir: None,
            folder_missing: false,
            registry: vec![],
            registry_error: None,
            files: vec![],
            more: false,
            error: None,
        };
        match fs::symlink_metadata(dir) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                raw.folder_missing = true;
                return raw;
            }
            Ok(m) if m.file_type().is_dir() => {}
            _ => {
                raw.error = Some("Claude directory unreadable or not a real directory".into());
                return raw;
            }
        }
        raw.claude_dir = Some(dir.to_string_lossy().into_owned());
        (raw.registry, raw.registry_error) = read_registry(&dir.join("sessions"), &self.proc_root);
        let budget = match req.budget_bytes {
            Some(b) if b.is_finite() && b >= 0.0 => (b as u64).min(64 * 1024 * 1024),
            _ => DEFAULT_BUDGET,
        };
        let cursors: HashMap<&str, (&str, f64)> = req
            .cursors
            .iter()
            .map(|c| (c.key.as_str(), (c.ino.as_str(), c.off)))
            .collect();
        let (listed, error) = scan::list(&dir.join("projects"));
        let mut tails = self.tails.lock().unwrap_or_else(|e| e.into_inner());
        (raw.files, raw.more) = scan::read_all(listed, &cursors, budget, &mut tails);
        raw.error = error;
        raw
    }
}

pub fn claude_dir(env: Option<OsString>, home: &Path) -> PathBuf {
    env.filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"))
}
fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}
fn mtime_ms(m: &fs::Metadata) -> f64 {
    m.mtime() as f64 * 1000.0 + (m.mtime_nsec() as f64 / 1e6).floor()
}
fn real_dir(dir: &Path) -> std::io::Result<()> {
    let m = fs::symlink_metadata(dir)?;
    if m.file_type().is_dir() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a real directory",
        ))
    }
}
/// Refuse symlink/nonregular replacements at the final component. No create/write flags.
fn open_read(path: &Path) -> std::io::Result<fs::File> {
    let f = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if f.metadata()?.is_file() {
        Ok(f)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ))
    }
}
fn proc_start(proc_root: &Path, pid: u32) -> Option<String> {
    let stat = fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
    let field = stat.rsplit_once(')')?.1.split_whitespace().nth(19)?;
    (!field.is_empty() && field.bytes().all(|b| b.is_ascii_digit())).then(|| field.to_owned())
}

/// Whitelist in Rust, not merely in the renderer: arbitrary registry fields must not cross IPC.
/// Session titles are deliberately omitted, even though the fork showed them in tooltips.
fn registry_metadata(v: serde_json::Value) -> serde_json::Value {
    use serde_json::{Map, Value};
    let Some(obj) = v.as_object() else {
        return Value::Null;
    };
    let mut out = Map::new();
    for key in [
        "sessionId",
        "cwd",
        "entrypoint",
        "status",
        "kind",
        "procStart",
        "waitingFor",
    ] {
        if let Some(Value::String(s)) = obj.get(key) {
            let valid = match key {
                "sessionId" => {
                    !s.is_empty()
                        && s.len() <= 80
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                }
                "cwd" => s.len() <= 4096 && !s.chars().any(char::is_control),
                "procStart" => {
                    !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit())
                }
                _ => {
                    !s.is_empty()
                        && s.len() <= 40
                        && s.bytes().all(|b| {
                            b.is_ascii_alphanumeric()
                                || matches!(b, b'_' | b' ' | b'.' | b':' | b'-')
                        })
                }
            };
            if valid {
                out.insert(key.into(), Value::String(s.clone()));
            }
        }
    }
    for key in ["startedAt", "updatedAt", "statusUpdatedAt"] {
        if let Some(value) = obj.get(key) {
            // Valid numeric/ISO timestamps only, never a free-form string.
            if scan::to_ms(&Some(value.clone())).is_some() {
                out.insert(key.into(), value.clone());
            }
        }
    }
    Value::Object(out)
}

fn read_registry(dir: &Path, proc_root: &Path) -> (Vec<ClaudeRegEntry>, Option<String>) {
    let entries = match real_dir(dir).and_then(|()| fs::read_dir(dir)) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (vec![], None),
        Err(_) => return (vec![], Some("unreadable".into())),
    };
    let mut found: Vec<(u32, String, f64)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let pid = name.strip_suffix(".json")?;
            if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let pid = pid.parse().ok()?;
            let meta = e.metadata().ok().filter(|m| m.file_type().is_file())?;
            Some((pid, name, mtime_ms(&meta)))
        })
        .collect();
    found.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    let out = found
        .into_iter()
        .take(MAX_REGISTRY)
        .map(|(pid, file, mtime_ms)| {
            let data = open_read(&dir.join(&file))
                .ok()
                .and_then(|f| {
                    let mut b = vec![];
                    f.take(MAX_REG_JSON + 1).read_to_end(&mut b).ok()?;
                    if b.len() as u64 > MAX_REG_JSON {
                        return None;
                    }
                    serde_json::from_slice(&b).ok()
                })
                .map(registry_metadata)
                .unwrap_or(serde_json::Value::Null);
            let alive = proc_root.join(pid.to_string()).exists();
            ClaudeRegEntry {
                file,
                mtime_ms,
                alive,
                proc_start: if alive {
                    proc_start(proc_root, pid)
                } else {
                    None
                },
                data,
            }
        })
        .collect();
    (out, None)
}
#[cfg(test)]
mod tests;
