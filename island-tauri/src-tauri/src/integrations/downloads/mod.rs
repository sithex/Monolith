//! Linux directory notifications, never polling. Tests inject both directory and opener.
pub mod core;
use crate::{
    bus::Latest,
    contract::apps::{DownloadRow, DownloadsChanged, OpenDownload},
    error::IslandError,
};
use notify::{
    event::{ModifyKind, RenameMode},
    EventKind, RecursiveMode, Watcher,
};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant, UNIX_EPOCH},
};
pub const SCAN_CAP: usize = 10_000;
pub trait Opener: Send + Sync {
    fn open(&self, argv: &[OsString]) -> Result<(), IslandError>;
}
pub struct DesktopOpener;
impl Opener for DesktopOpener {
    fn open(&self, argv: &[OsString]) -> Result<(), IslandError> {
        let Some(program) = argv.first() else {
            return Err(IslandError::invalid("No opener"));
        };
        let mut cmd = std::process::Command::new(program);
        cmd.args(&argv[1..]);
        crate::services::process::run_bounded(cmd, Duration::from_secs(5), 4096, false).map(drop)
    }
}
struct State {
    model: core::DownloadsModel,
    rev: u32,
    available: bool,
    detail: String,
}
enum Message {
    Event(notify::Result<notify::Event>),
    Stop,
}
pub struct DownloadsService {
    dir: Option<PathBuf>,
    opener: Arc<dyn Opener>,
    state: Mutex<State>,
    pub topic: Latest<DownloadsChanged>,
    tx: mpsc::SyncSender<Message>,
    worker: Mutex<Option<JoinHandle<()>>>,
    origin: Instant,
}
fn stat(path: &Path) -> Option<core::FileStat> {
    let meta = fs::symlink_metadata(path).ok()?;
    Some(core::FileStat {
        name: path.file_name()?.to_str()?.into(),
        size_bytes: meta.len(),
        mtime_ms: meta
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis() as u64,
        is_dir: meta.is_dir(),
        is_symlink: meta.is_symlink(),
    })
}
fn scan(dir: &Path) -> Result<Vec<core::FileStat>, IslandError> {
    let mut result = Vec::new();
    for (index, entry) in fs::read_dir(dir)
        .map_err(|_| IslandError::io("Downloads directory unreadable"))?
        .enumerate()
    {
        if index >= SCAN_CAP {
            return Err(IslandError::unavailable("Downloads scan limit exceeded"));
        }
        let entry = entry.map_err(|_| IslandError::io("Downloads entry unreadable"))?;
        if let Some(stat) = stat(&entry.path()) {
            result.push(stat);
        }
    }
    Ok(result)
}
impl DownloadsService {
    pub fn start(dir: Option<PathBuf>, opener: Arc<dyn Opener>) -> Result<Arc<Self>, IslandError> {
        let (tx, rx) = mpsc::sync_channel(128);
        let model = core::DownloadsModel::new(core::DownloadsConfig::default())
            .map_err(|_| IslandError::invalid("Downloads config invalid"))?;
        let service = Arc::new(Self {
            dir,
            opener,
            state: Mutex::new(State {
                model,
                rev: 0,
                available: false,
                detail: "not-configured".into(),
            }),
            topic: Latest::new(),
            tx,
            worker: Mutex::new(None),
            origin: Instant::now(),
        });
        let s = Arc::clone(&service);
        let task = std::thread::Builder::new()
            .name("hi-downloads".into())
            .spawn(move || s.run(rx))
            .map_err(|_| IslandError::io("Downloads worker unavailable"))?;
        *service
            .worker
            .lock()
            .map_err(|_| IslandError::internal("Downloads worker poisoned"))? = Some(task);
        Ok(service)
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, IslandError> {
        self.state
            .lock()
            .map_err(|_| IslandError::internal("Downloads state poisoned"))
    }
    fn now(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }
    fn snapshot(&self, s: &State) -> DownloadsChanged {
        DownloadsChanged {
            rev: s.rev,
            available: s.available,
            detail: s.detail.clone(),
            items: s
                .model
                .items(self.now())
                .into_iter()
                .map(|i| DownloadRow {
                    name: i.name,
                    size_bytes: i.size_bytes as f64,
                    mtime_ms: i.mtime_ms as f64,
                    kind: format!("{:?}", i.kind).to_lowercase(),
                    state: match i.state {
                        core::ItemState::InProgress => "inProgress",
                        core::ItemState::Settling => "settling",
                        core::ItemState::Ready => "ready",
                    }
                    .into(),
                })
                .collect(),
        }
    }
    pub fn current(&self) -> Result<DownloadsChanged, IslandError> {
        Ok(self.snapshot(&*self.lock()?))
    }
    fn publish(&self, s: &mut State) {
        s.rev = s.rev.wrapping_add(1);
        self.topic.publish(self.snapshot(s));
    }
    fn fail(&self, detail: &str) {
        if let Ok(mut s) = self.lock() {
            s.available = false;
            s.detail = detail.into();
            self.publish(&mut s);
        }
    }
    fn run(self: Arc<Self>, rx: mpsc::Receiver<Message>) {
        let Some(dir) = &self.dir else {
            self.fail("not-configured");
            return;
        };
        if !fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && !m.is_symlink()) {
            self.fail("directory-unavailable");
            return;
        }
        let overflow = Arc::new(AtomicBool::new(false));
        let callback_overflow = Arc::clone(&overflow);
        let tx = self.tx.clone();
        let watcher = notify::recommended_watcher(move |event| {
            if tx.try_send(Message::Event(event)).is_err() {
                callback_overflow.store(true, Ordering::Release);
            }
        });
        let Ok(mut watcher) = watcher else {
            self.fail("watcher-unavailable");
            return;
        };
        if watcher.watch(dir, RecursiveMode::NonRecursive).is_err() {
            self.fail("watch-failed");
            return;
        }
        if let Err(error) = self.rescan(dir) {
            tracing::warn!(%error,"Downloads scan failed");
            self.fail("scan-failed");
        }
        loop {
            let deadline = match self.lock() {
                Ok(s) => s.model.next_settle_at(),
                Err(_) => return,
            };
            let message = match deadline {
                Some(at) => {
                    match rx.recv_timeout(Duration::from_millis(at.saturating_sub(self.now()))) {
                        Ok(m) => Some(m),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(_) => return,
                    }
                }
                None => match rx.recv() {
                    Ok(m) => Some(m),
                    Err(_) => return,
                },
            };
            if matches!(message, Some(Message::Stop)) {
                return;
            }
            if let Some(Message::Event(event)) = message {
                match event {
                    Ok(event) if !matches!(event.kind, EventKind::Access(_)) => {
                        if let Err(error) = self.changed(dir, event) {
                            tracing::warn!(%error,"Downloads event failed");
                            self.fail("scan-failed");
                        }
                    }
                    Err(_) => {
                        self.fail("watch-error");
                    }
                    _ => {}
                }
            }
            if overflow.swap(false, Ordering::AcqRel) {
                if let Err(error) = self.rescan(dir) {
                    tracing::warn!(%error,"Downloads overflow repair failed");
                    self.fail("scan-failed");
                }
            }
            if let Ok(mut s) = self.lock() {
                if s.model.next_settle_at().is_some_and(|at| at <= self.now()) {
                    s.model.tick(self.now());
                    self.publish(&mut s);
                }
            }
        }
    }
    fn rescan(&self, dir: &Path) -> Result<(), IslandError> {
        let entries = scan(dir)?;
        let mut s = self.lock()?;
        s.model.apply(self.now(), core::DirEvent::Rescan(entries));
        s.available = true;
        s.detail = "inotify".into();
        self.publish(&mut s);
        Ok(())
    }
    fn changed(&self, dir: &Path, event: notify::Event) -> Result<(), IslandError> {
        if event.need_rescan() {
            return self.rescan(dir);
        }
        match event.kind {
            EventKind::Remove(_) => {
                let mut s = self.lock()?;
                for path in event.paths {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        s.model
                            .apply(self.now(), core::DirEvent::Removed { name: name.into() });
                    }
                }
                self.publish(&mut s);
                Ok(())
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() == 2 => {
                if let (Some(from), Some(to)) = (
                    event.paths[0].file_name().and_then(|n| n.to_str()),
                    stat(&event.paths[1]),
                ) {
                    let mut s = self.lock()?;
                    s.model.apply(
                        self.now(),
                        core::DirEvent::Renamed {
                            from: from.into(),
                            to,
                        },
                    );
                    self.publish(&mut s);
                    Ok(())
                } else {
                    self.rescan(dir)
                }
            }
            _ => self.rescan(dir),
        }
    }
    pub fn open(&self, name: &str, op: OpenDownload) -> Result<(), IslandError> {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
            return Err(IslandError::invalid("Invalid download name"));
        }
        let dir = self
            .dir
            .as_ref()
            .ok_or_else(|| IslandError::unavailable("No downloads directory"))?;
        let state = self.lock()?;
        if !state.available || !state.model.items(self.now()).iter().any(|i| i.name == name) {
            return Err(IslandError::unavailable("Download no longer available"));
        }
        let file = dir.join(name);
        if !fs::symlink_metadata(&file).is_ok_and(|m| m.is_file() && !m.is_symlink()) {
            return Err(IslandError::invalid("Download is not a regular file"));
        }
        let argv = match op {
            OpenDownload::Open => vec![OsString::from("xdg-open"), file.into_os_string()],
            OpenDownload::Reveal => vec![
                OsString::from("dolphin"),
                OsString::from("--select"),
                file.into_os_string(),
            ],
        };
        drop(state);
        self.opener.open(&argv)
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        let task = self
            .worker
            .lock()
            .map_err(|_| IslandError::internal("Downloads worker poisoned"))?
            .take();
        if let Some(task) = task {
            if self.tx.send(Message::Stop).is_err() && !task.is_finished() {
                return Err(IslandError::unavailable("Downloads worker disconnected"));
            }
            task.join()
                .map_err(|_| IslandError::internal("Downloads worker panicked"))?;
        }
        Ok(())
    }
}
