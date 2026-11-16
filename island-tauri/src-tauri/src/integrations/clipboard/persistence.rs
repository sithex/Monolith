//! Opt-in pinned-only filesystem adapter. Never called by the state actor.
//!
//! Writes go through [`Writer`]: callers capture a cheap [`PersistSnapshot`] under the history
//! lock and hand it over; one ordered thread serialises it, fsyncs and renames it with no lock
//! held. Revisions that are superseded before the writer reaches them are never written.
use super::{
    core_error,
    history::{HistoryConfig, HistoryStore, PersistSnapshot},
};
use crate::error::IslandError;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, Condvar, Mutex, MutexGuard,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
static SERIAL: AtomicU32 = AtomicU32::new(0);
/// How long a snapshot that changes only the id counter may wait for newer ones to replace it.
/// Pin/unpin/delete/clear/reorder of pinned entries and settings changes never wait.
const LAZY_DELAY: Duration = Duration::from_millis(100);

const MAX_FILE: u64 = 64 * 1024 * 1024;

pub(super) fn load(path: &Path, config: HistoryConfig) -> Result<HistoryStore, IslandError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return HistoryStore::new(config).map_err(core_error)
        }
        Err(_) => return Err(IslandError::io("Cannot open clipboard persistence")),
    };
    let metadata = file
        .metadata()
        .map_err(|_| IslandError::io("Cannot inspect clipboard persistence"))?;
    if !metadata.is_file() || metadata.len() > MAX_FILE {
        return Err(IslandError::invalid(
            "Clipboard persistence exceeds file limit",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no preconditions and does not mutate process state.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(IslandError::invalid(
                "Clipboard persistence must be private and owned by this user",
            ));
        }
    }
    let mut json = String::new();
    file.take(MAX_FILE + 1)
        .read_to_string(&mut json)
        .map_err(|_| IslandError::io("Cannot read clipboard persistence"))?;
    if json.len() as u64 > MAX_FILE {
        return Err(IslandError::invalid(
            "Clipboard persistence exceeds file limit",
        ));
    }
    // Validate real image blobs too: the pure core deliberately trusts its caller's dimensions.
    #[cfg(target_os = "linux")]
    {
        use base64::Engine;
        let mut store = HistoryStore::from_persisted(config, &json).map_err(core_error)?;
        for s in store.list(None, 500) {
            if let Some(super::history::ClipContent::Image {
                png_data_url,
                width,
                height,
            }) = store.get(&s.id)
            {
                let valid = png_data_url.strip_prefix("data:image/png;base64,")
                    .and_then(|v| base64::engine::general_purpose::STANDARD.decode(v).ok())
                    .and_then(|v| super::linux_wayland::decode_png(v).ok())
                    .is_some_and(|p| matches!(p, super::ClipPayload::Image {width:w,height:h,..} if w==width && h==height));
                if !valid {
                    store.delete(&s.id).map_err(core_error)?;
                }
            }
        }
        Ok(store)
    }
    #[cfg(not(target_os = "linux"))]
    HistoryStore::from_persisted(config, &json).map_err(core_error)
}
fn save(path: &Path, snapshot: &PersistSnapshot) -> Result<(), IslandError> {
    let json = snapshot.to_json().map_err(core_error)?;
    if json.len() as u64 > MAX_FILE {
        return Err(IslandError::invalid(
            "Pinned clipboard persistence exceeds file limit",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| IslandError::io("Invalid clipboard persistence directory"))?;
    fs::create_dir_all(parent)
        .map_err(|_| IslandError::io("Cannot create clipboard data directory"))?;
    let temporary = parent.join(format!(
        ".clipboard.{}.{}.tmp",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let result = (|| {
        let mut file = options
            .open(&temporary)
            .map_err(|_| IslandError::io("Cannot create private clipboard snapshot"))?;
        file.write_all(json.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|_| IslandError::io("Cannot write clipboard snapshot"))?;
        fs::rename(&temporary, path)
            .map_err(|_| IslandError::io("Cannot atomically replace clipboard snapshot"))?;
        Ok(())
    })();
    if result.is_err() {
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(%e,"Clipboard temporary cleanup failed"),
        }
    }
    result
}
fn remove(path: &Path) -> Result<(), IslandError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(IslandError::io(
            "Cannot remove disabled clipboard persistence",
        )),
    }
}

enum Job {
    Save(PersistSnapshot),
    Remove,
}
struct Pending {
    job: Job,
    /// Must be written without waiting for newer revisions.
    urgent: bool,
    since: Instant,
}
#[derive(Default)]
struct State {
    pending: Option<Pending>,
    writing: bool,
    /// A flush is waiting: write the pending job now.
    flush: bool,
    closed: bool,
    /// The last write failed and nobody has been told yet.
    error: Option<IslandError>,
    /// The last write failed: the next save must not be skipped as "unchanged".
    failed: bool,
}
struct Core {
    state: Mutex<State>,
    cv: Condvar,
    path: PathBuf,
    /// Durable writes (saves and removals) attempted so far.
    writes: AtomicU64,
    /// Test hook: artificial delay inside every write, to model a slow disk.
    #[cfg(test)]
    delay_ms: AtomicU64,
}
fn state(core: &Core) -> MutexGuard<'_, State> {
    core.state.lock().unwrap_or_else(|e| e.into_inner())
}

/// One ordered background writer for `clipboard.json`. The thread starts with the first
/// submission, so a default-off install never has one.
pub(super) struct Writer {
    core: Arc<Core>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl Writer {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            core: Arc::new(Core {
                state: Mutex::default(),
                cv: Condvar::new(),
                path,
                writes: AtomicU64::new(0),
                #[cfg(test)]
                delay_ms: AtomicU64::new(0),
            }),
            thread: Mutex::new(None),
        }
    }
    /// Queue a save of `snapshot`, replacing any older queued job. Cheap: call it under the
    /// history lock so submission order is revision order.
    pub(super) fn save(&self, snapshot: PersistSnapshot, urgent: bool) -> Result<(), IslandError> {
        self.submit(Job::Save(snapshot), urgent)
    }
    /// Queue removal of the file. It replaces any queued save and, being ordered after
    /// everything already submitted, a late save cannot bring the file back.
    pub(super) fn remove(&self) -> Result<(), IslandError> {
        self.submit(Job::Remove, true)
    }
    fn submit(&self, job: Job, urgent: bool) -> Result<(), IslandError> {
        self.ensure_thread()?;
        let mut st = state(&self.core);
        let (urgent, since) = match st.pending.take() {
            // Superseding a queued job keeps its urgency and its age.
            Some(old) => (urgent || old.urgent, old.since),
            None => (urgent, Instant::now()),
        };
        st.pending = Some(Pending { job, urgent, since });
        drop(st);
        self.core.cv.notify_all();
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn writes(&self) -> u64 {
        self.core.writes.load(Ordering::SeqCst)
    }
    #[cfg(test)]
    pub(super) fn set_delay_ms(&self, ms: u64) {
        self.core.delay_ms.store(ms, Ordering::SeqCst);
    }
    /// True once if the last write failed: the caller must queue its state again even if it
    /// looks unchanged.
    pub(super) fn take_failed(&self) -> bool {
        std::mem::take(&mut state(&self.core).failed)
    }
    /// Block until everything submitted so far is on disk; returns the newest write's error,
    /// if it failed. Never call it while holding the history lock.
    pub(super) fn flush(&self) -> Result<(), IslandError> {
        let mut st = state(&self.core);
        st.flush = true;
        self.core.cv.notify_all();
        while st.pending.is_some() || st.writing {
            st = self.core.cv.wait(st).unwrap_or_else(|e| e.into_inner());
        }
        st.flush = false;
        match st.error.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    fn ensure_thread(&self) -> Result<(), IslandError> {
        let mut thread = self.thread.lock().unwrap_or_else(|e| e.into_inner());
        if thread.is_none() {
            let core = self.core.clone();
            *thread = Some(
                std::thread::Builder::new()
                    .name("hi-clip-writer".into())
                    .spawn(move || run(&core))
                    .map_err(|_| IslandError::io("Cannot start clipboard writer"))?,
            );
        }
        Ok(())
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        state(&self.core).closed = true;
        self.core.cv.notify_all();
        if let Some(thread) = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = thread.join();
        }
    }
}
fn run(core: &Core) {
    loop {
        let mut st = state(core);
        let pending = loop {
            match &st.pending {
                None if st.closed => return,
                None => st = core.cv.wait(st).unwrap_or_else(|e| e.into_inner()),
                Some(p) if p.urgent || st.flush || st.closed => break st.pending.take(),
                Some(p) => {
                    let wait = LAZY_DELAY.saturating_sub(p.since.elapsed());
                    if wait.is_zero() {
                        break st.pending.take();
                    }
                    st = core
                        .cv
                        .wait_timeout(st, wait)
                        .unwrap_or_else(|e| e.into_inner())
                        .0;
                }
            }
        };
        let Some(pending) = pending else { continue };
        st.writing = true;
        drop(st);
        core.writes.fetch_add(1, Ordering::SeqCst);
        #[cfg(test)]
        std::thread::sleep(Duration::from_millis(core.delay_ms.load(Ordering::SeqCst)));
        let result = match &pending.job {
            Job::Save(snapshot) => save(&core.path, snapshot),
            Job::Remove => remove(&core.path),
        };
        let mut st = state(core);
        st.writing = false;
        // The file reflects the newest write only: a later success supersedes an earlier
        // failure, so a flush never reports an error the disk has already recovered from.
        match result {
            Ok(()) => {
                st.error = None;
                st.failed = false;
            }
            Err(error) => {
                tracing::warn!(%error, "Clipboard persistence write failed");
                st.error = Some(error);
                st.failed = true;
            }
        }
        drop(st);
        core.cv.notify_all();
    }
}
