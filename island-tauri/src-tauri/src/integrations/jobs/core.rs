//! Pure decoding of KDE job-progress traffic seen by a passive bus monitor.
//!
//! A KDE app that copies files asks Plasma (the owner of `org.kde.JobViewServer`) for a job
//! view with `requestView`, gets an object path back, then calls `update(a{sv})` on that path
//! about five times per second and `terminate` at the end (interface XML: kjobwidgets'
//! `kf6_org.kde.JobViewV2.xml`, and the V3 interface embedded in libnotificationmanager).
//! This module correlates the call with its reply, merges the updates into one row per job and
//! decides when the closed notch should be told. Nothing here does I/O.
use crate::contract::apps::TransferRow;
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};
use zbus::{message::Type, Message, OwnedVariant as OwnedValue};

pub const SERVER_NAME: &str = "org.kde.JobViewServer";
pub const SERVER_PATH: &str = "/JobViewServer";
pub const SERVER_V1: &str = "org.kde.JobViewServer";
pub const SERVER_V2: &str = "org.kde.JobViewServerV2";
pub const VIEW_V2: &str = "org.kde.JobViewV2";
pub const VIEW_V3: &str = "org.kde.JobViewV3";
/// At most this many jobs are tracked; a new one past the cap is ignored.
pub const MAX_JOBS: usize = 16;
/// Unanswered `requestView` calls kept for correlation.
pub const MAX_PENDING: usize = 32;
pub const PENDING_TTL: Duration = Duration::from_secs(5);
/// A job with no message for this long is dropped (a crashed client). A job waiting on an
/// overwrite dialog goes quiet too, so this is long; a client's disconnect is the main cleanup.
pub const JOB_TTL: Duration = Duration::from_secs(120);
/// Larger messages are rejected before decoding (an update is a few hundred bytes).
pub const MAX_MESSAGE: usize = 64 * 1024;
/// Longer strings are cut to this many bytes (on a char boundary).
pub const MAX_TEXT: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq)]
struct Job {
    client: String,
    app: String,
    title: String,
    percent: Option<f64>,
    processed_bytes: f64,
    total_bytes: f64,
    processed_files: f64,
    total_files: f64,
    speed: f64,
    suspended: bool,
    dest_url: String,
    seen: Option<Instant>,
}

/// Correlates job views with their clients and keeps one row per running job.
#[derive(Default)]
pub struct Tracker {
    pending: VecDeque<Pending>,
    jobs: BTreeMap<String, Job>,
    /// Jobs ignored because `MAX_JOBS` were already running.
    pub overflow: u32,
}

struct Pending {
    client: String,
    serial: u32,
    job: Job,
    at: Instant,
}

/// Display text: control characters (a newline in a file name, say) become spaces, and the
/// result is cut to `MAX_TEXT` bytes on a char boundary.
fn text(value: &str) -> String {
    let mut end = value.len().min(MAX_TEXT);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end]
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Any D-Bus integer or double as a finite, non-negative number.
fn number(value: &OwnedValue) -> Option<f64> {
    let n = if let Ok(v) = u64::try_from(value) {
        v as f64
    } else if let Ok(v) = i64::try_from(value) {
        v as f64
    } else if let Ok(v) = u32::try_from(value) {
        f64::from(v)
    } else if let Ok(v) = i32::try_from(value) {
        f64::from(v)
    } else if let Ok(v) = f64::try_from(value) {
        v
    } else {
        return None;
    };
    (n.is_finite() && n >= 0.).then_some(n)
}

fn string(value: &OwnedValue) -> Option<String> {
    <&str>::try_from(value).ok().map(text)
}

impl Job {
    fn merge(&mut self, properties: &BTreeMap<String, OwnedValue>) {
        for (key, value) in properties {
            match key.as_str() {
                "percent" => self.percent = number(value).map(|p| p.min(100.)),
                "processedBytes" => self.processed_bytes = number(value).unwrap_or(0.),
                "totalBytes" => self.total_bytes = number(value).unwrap_or(0.),
                "processedFiles" => self.processed_files = number(value).unwrap_or(0.),
                "totalFiles" => self.total_files = number(value).unwrap_or(0.),
                "speed" => self.speed = number(value).unwrap_or(0.),
                "suspended" => self.suspended = bool::try_from(value).unwrap_or(false),
                "title" => self.title = string(value).unwrap_or_default(),
                "destUrl" => self.dest_url = string(value).unwrap_or_default(),
                _ => {}
            }
        }
    }

    /// V2 setters (`setPercent`, `setProcessedAmount(t, s unit)`, ...) mapped onto the row.
    fn apply_v2(&mut self, member: &str, message: &Message) -> bool {
        let body = message.body();
        match member {
            "setPercent" => body
                .deserialize::<u32>()
                .map(|p| self.percent = Some(f64::from(p.min(100))))
                .is_ok(),
            "setSpeed" => body
                .deserialize::<u64>()
                .map(|s| self.speed = s as f64)
                .is_ok(),
            "setSuspended" => body
                .deserialize::<bool>()
                .map(|s| self.suspended = s)
                .is_ok(),
            "setInfoMessage" => body
                .deserialize::<String>()
                .map(|m| self.title = text(&m))
                .is_ok(),
            "setTotalAmount" | "setProcessedAmount" => {
                let Ok((amount, unit)) = body.deserialize::<(u64, String)>() else {
                    return false;
                };
                let total = member == "setTotalAmount";
                let slot = match (unit.as_str(), total) {
                    ("bytes", true) => &mut self.total_bytes,
                    ("bytes", false) => &mut self.processed_bytes,
                    ("files", true) => &mut self.total_files,
                    ("files", false) => &mut self.processed_files,
                    _ => return true,
                };
                *slot = amount as f64;
                true
            }
            "setDestUrl" => body
                .deserialize::<OwnedValue>()
                .map(|v| self.dest_url = string(&v).unwrap_or_default())
                .is_ok(),
            _ => true,
        }
    }

    /// 0..=100: the reported percent, else processed/total bytes, else files.
    fn progress(&self) -> Option<f64> {
        self.percent
            .or_else(|| {
                (self.total_bytes > 0.).then(|| self.processed_bytes / self.total_bytes * 100.)
            })
            .or_else(|| {
                (self.total_files > 0.).then(|| self.processed_files / self.total_files * 100.)
            })
            .map(|p| p.clamp(0., 100.))
    }
}

/// The local folder a "Show in folder" button opens: the destination's parent when the
/// destination is a file being written, the destination itself when it is a folder.
pub fn local_path(dest_url: &str) -> Option<std::path::PathBuf> {
    let rest = dest_url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    if out.contains(&0) {
        return None;
    }
    use std::os::unix::ffi::OsStringExt;
    let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(out));
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(path)
}

impl Tracker {
    pub fn reset(&mut self) {
        self.pending.clear();
        self.jobs.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Feed one monitored message. Returns whether the visible job list changed.
    /// `owner` is the unique name currently owning `org.kde.JobViewServer`.
    pub fn observe(&mut self, message: &Message, owner: &str, now: Instant) -> bool {
        if message.data().len() > MAX_MESSAGE || owner.is_empty() {
            return false;
        }
        let header = message.header();
        let sender = header.sender().map(|s| s.as_str()).unwrap_or("");
        let destination = header.destination().map(|s| s.as_str()).unwrap_or("");
        let interface = header.interface().map(|s| s.as_str()).unwrap_or("");
        let member = header.member().map(|s| s.as_str()).unwrap_or("");
        let path = header.path().map(|s| s.as_str()).unwrap_or("");
        let to_server = destination == SERVER_NAME || destination == owner;
        match message.message_type() {
            Type::MethodCall
                if to_server
                    && path == SERVER_PATH
                    && member == "requestView"
                    && (interface == SERVER_V2 || interface == SERVER_V1) =>
            {
                let mut job = Job {
                    client: sender.into(),
                    ..Default::default()
                };
                let body = message.body();
                let ok = if interface == SERVER_V2 {
                    body.deserialize::<(String, i32, BTreeMap<String, OwnedValue>)>()
                        .map(|(app, _, hints)| {
                            job.app = text(&app);
                            job.merge(&hints);
                        })
                        .is_ok()
                } else {
                    body.deserialize::<(String, String, i32)>()
                        .map(|(app, _, _)| job.app = text(&app))
                        .is_ok()
                };
                if !ok || sender.is_empty() {
                    return false;
                }
                let serial = header.primary().serial_num().get();
                self.pending
                    .retain(|p| !(p.client == sender && p.serial == serial));
                if self.pending.len() >= MAX_PENDING {
                    self.pending.pop_front();
                }
                self.pending.push_back(Pending {
                    client: sender.into(),
                    serial,
                    job,
                    at: now,
                });
                false
            }
            Type::MethodReturn if sender == owner => {
                let Some(serial) = header.reply_serial().map(|n| n.get()) else {
                    return false;
                };
                let Some(index) = self
                    .pending
                    .iter()
                    .position(|p| p.serial == serial && p.client == destination)
                else {
                    return false;
                };
                let Ok(path) = message
                    .body()
                    .deserialize::<zbus::OwnedObjectPath>()
                else {
                    return false;
                };
                let Some(pending) = self.pending.remove(index) else {
                    return false;
                };
                if !self.jobs.contains_key(path.as_str()) && self.jobs.len() >= MAX_JOBS {
                    self.overflow = self.overflow.wrapping_add(1);
                    return false;
                }
                let mut job = pending.job;
                job.seen = Some(now);
                self.jobs.insert(path.as_str().to_owned(), job);
                true
            }
            Type::MethodCall if interface == VIEW_V3 || interface == VIEW_V2 => {
                // Only the client that requested this view may update or end it.
                let Some(job) = self.jobs.get_mut(path).filter(|j| j.client == sender) else {
                    return false;
                };
                job.seen = Some(now);
                let before = job.clone();
                if member == "terminate" {
                    self.jobs.remove(path);
                    return true;
                }
                if interface == VIEW_V3 {
                    if member != "update" {
                        return false;
                    }
                    let Ok(properties) = message
                        .body()
                        .deserialize::<BTreeMap<String, OwnedValue>>()
                    else {
                        return false;
                    };
                    job.merge(&properties);
                } else if !job.apply_v2(member, message) {
                    return false;
                }
                let mut after = job.clone();
                after.seen = before.seen;
                after != before
            }
            _ => false,
        }
    }

    /// A unique name left the bus: its jobs are over (crash or normal exit).
    pub fn client_gone(&mut self, unique: &str) -> bool {
        self.pending.retain(|p| p.client != unique);
        let before = self.jobs.len();
        self.jobs.retain(|_, j| j.client != unique);
        before != self.jobs.len()
    }

    /// Drops stale correlations and silent jobs. Returns whether a job was removed.
    pub fn expire(&mut self, now: Instant) -> bool {
        self.pending
            .retain(|p| now.saturating_duration_since(p.at) < PENDING_TTL);
        let before = self.jobs.len();
        self.jobs.retain(|_, j| {
            j.seen
                .is_none_or(|t| now.saturating_duration_since(t) < JOB_TTL)
        });
        before != self.jobs.len()
    }

    /// When `expire` next has something to do.
    pub fn next_expiry(&self) -> Option<Instant> {
        let pending = self.pending.iter().map(|p| p.at + PENDING_TTL);
        let jobs = self.jobs.values().filter_map(|j| j.seen.map(|t| t + JOB_TTL));
        pending.chain(jobs).min()
    }

    pub fn rows(&self) -> Vec<TransferRow> {
        self.jobs
            .iter()
            .map(|(path, j)| TransferRow {
                id: path.clone(),
                app: j.app.clone(),
                title: j.title.clone(),
                percent: j.progress().map(f64::round).unwrap_or(-1.),
                processed_bytes: j.processed_bytes,
                total_bytes: j.total_bytes,
                processed_files: j.processed_files,
                total_files: j.total_files,
                speed: j.speed,
                suspended: j.suspended,
                destination: local_path(&j.dest_url)
                    .map(|p| text(&p.display().to_string()))
                    .unwrap_or_default(),
            })
            .collect()
    }

    pub fn dest_url(&self, id: &str) -> Option<&str> {
        self.jobs.get(id).map(|j| j.dest_url.as_str())
    }

    /// One number for the notch: `None` = no job; otherwise 0..=100 over all jobs, weighted by
    /// bytes when every job reports a byte total, else the average of their progress.
    pub fn aggregate(&self) -> Option<u8> {
        if self.jobs.is_empty() {
            return None;
        }
        let total: f64 = self.jobs.values().map(|j| j.total_bytes).sum();
        let p = if self.jobs.len() > 1 && self.jobs.values().all(|j| j.total_bytes > 0.) {
            let done: f64 = self
                .jobs
                .values()
                .map(|j| j.processed_bytes.min(j.total_bytes))
                .sum();
            done / total * 100.
        } else {
            let sum: f64 = self.jobs.values().map(|j| j.progress().unwrap_or(0.)).sum();
            sum / self.jobs.len() as f64
        };
        Some(p.clamp(0., 100.) as u8)
    }
}

/// Notch progress steps. The notch is repainted on every monitor for each step, so it is told
/// at most once per `MIN_GAP`, in whole `STEP`s, plus right away when a copy starts or ends.
pub const STEP: u8 = 10;
pub const MIN_GAP: Duration = Duration::from_secs(3);
/// The wire value meaning "no copy running".
pub const IDLE: i32 = -1;

pub fn wire(progress: Option<u8>) -> i32 {
    progress.map_or(IDLE, |p| i32::from(p.min(100) / STEP * STEP))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Send this value now.
    Send(i32),
    /// Nothing to send now; ask again at this instant.
    Later(Instant),
    Hold,
}

#[derive(Default)]
pub struct NotchSteps {
    sent: Option<(i32, Instant)>,
}

impl NotchSteps {
    pub fn current(&self) -> i32 {
        self.sent.map_or(IDLE, |(v, _)| v)
    }

    /// What to do for the latest aggregate. Call `sent` after a successful send.
    pub fn decide(&self, progress: Option<u8>, now: Instant) -> Step {
        let target = wire(progress);
        let (current, at) = match self.sent {
            Some(sent) => sent,
            None if target == IDLE => return Step::Hold,
            None => return Step::Send(target),
        };
        if target == current {
            Step::Hold
        } else if current == IDLE || target == IDLE {
            // Start and end are shown at once.
            Step::Send(target)
        } else if now.saturating_duration_since(at) >= MIN_GAP {
            Step::Send(target)
        } else {
            Step::Later(at + MIN_GAP)
        }
    }

    pub fn sent(&mut self, value: i32, now: Instant) {
        self.sent = Some((value, now));
    }
}
