//! Instance-only scratchpad. The accepted core owns notes and debounce policy; IO is serial.
pub mod core;
use crate::{
    bus::Latest,
    contract::apps::{AppNote, ScratchpadChanged, ScratchpadOp},
    error::IslandError,
    integrations::private_file,
};
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};
pub const FILE_CAP: usize = 64 * 1024 * 1024;
struct State {
    store: core::Scratchpad,
    rev: u32,
    stopped: bool,
    readable: bool,
    detail: String,
}
pub struct ScratchpadService {
    path: PathBuf,
    state: Mutex<State>,
    wake: Condvar,
    origin: Instant,
    pub topic: Latest<ScratchpadChanged>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl ScratchpadService {
    pub fn new(data: PathBuf) -> Arc<Self> {
        let path = data.join("scratchpad.json");
        let loaded = private_file::read(&path, FILE_CAP).and_then(|bytes| match bytes {
            None => Ok(core::Scratchpad::new()),
            Some(bytes) => std::str::from_utf8(&bytes)
                .map_err(|_| IslandError::invalid("Invalid notes encoding"))
                .and_then(|text| {
                    core::Scratchpad::from_json(text)
                        .map(|p| p.0)
                        .map_err(|_| IslandError::invalid("Invalid notes file"))
                }),
        });
        let readable = loaded.is_ok();
        let service = Arc::new(Self {
            path,
            state: Mutex::new(State {
                store: loaded.unwrap_or_default(),
                rev: 0,
                stopped: false,
                readable,
                detail: if readable {
                    "ready"
                } else {
                    "load-failed-file-preserved"
                }
                .into(),
            }),
            wake: Condvar::new(),
            origin: Instant::now(),
            topic: Latest::new(),
            worker: Mutex::new(None),
        });
        if let Ok(snapshot) = service.current() {
            service.topic.publish(snapshot);
        }
        service
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, IslandError> {
        self.state
            .lock()
            .map_err(|_| IslandError::internal("Notes state poisoned"))
    }
    fn snapshot(state: &State) -> ScratchpadChanged {
        let notes = state
            .store
            .list()
            .iter()
            .filter_map(|n| state.store.get(&n.id))
            .map(|n| AppNote {
                id: n.id.clone(),
                title: n.title.clone(),
                body: n.body.clone(),
                created_ms: n.created_ms as f64,
                updated_ms: n.updated_ms as f64,
                pinned: n.pinned,
            })
            .collect();
        ScratchpadChanged {
            rev: state.rev,
            available: state.readable && state.detail != "save-failed",
            detail: state.detail.clone(),
            dirty: state.store.is_dirty(),
            notes,
        }
    }
    pub fn current(&self) -> Result<ScratchpadChanged, IslandError> {
        Ok(Self::snapshot(&*self.lock()?))
    }
    pub fn apply(&self, now: u64, op: ScratchpadOp) -> Result<ScratchpadChanged, IslandError> {
        let mut state = self.lock()?;
        if state.stopped || !state.readable {
            return Err(IslandError::unavailable("Notes unavailable"));
        }
        let result = match op {
            ScratchpadOp::Create { id } => state.store.new_note(now, id),
            ScratchpadOp::Edit { id, body } => state.store.edit(now, &id, body),
            ScratchpadOp::Delete { id } => state.store.delete(now, &id),
            ScratchpadOp::Pin { id, pinned } => state.store.set_pinned(now, &id, pinned),
        };
        result.map_err(|e| IslandError::invalid(e.to_string()))?;
        state.rev = state.rev.wrapping_add(1);
        let snapshot = Self::snapshot(&state);
        self.topic.publish(snapshot.clone());
        self.wake.notify_one();
        Ok(snapshot)
    }
    pub fn now(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }
    /// Injected deadline processing; no save before the core says it is due.
    pub fn save_due(&self, now: u64, flush: bool) -> Result<(), IslandError> {
        let mut state = self.lock()?;
        if !state.readable {
            return Err(IslandError::unavailable("Notes file preserved"));
        }
        if flush {
            state.store.scheduler_mut().flush_requested();
        }
        if let Some(generation) = state.store.scheduler_mut().begin_save(now) {
            let bytes = state.store.to_json();
            match private_file::write(&self.path, bytes.as_bytes(), FILE_CAP) {
                Ok(()) => {
                    state.store.scheduler_mut().saved(now, generation);
                    state.detail = "ready".into();
                }
                Err(error) => {
                    state.store.scheduler_mut().save_failed(now);
                    state.detail = "save-failed".into();
                    state.rev = state.rev.wrapping_add(1);
                    self.topic.publish(Self::snapshot(&state));
                    return Err(error);
                }
            }
            state.rev = state.rev.wrapping_add(1);
            self.topic.publish(Self::snapshot(&state));
        }
        Ok(())
    }
    pub fn start(self: &Arc<Self>) -> Result<(), IslandError> {
        let mut slot = self
            .worker
            .lock()
            .map_err(|_| IslandError::internal("Notes worker poisoned"))?;
        if slot.is_some() {
            return Err(IslandError::invalid("Notes worker already running"));
        }
        let service = Arc::clone(self);
        *slot = Some(
            std::thread::Builder::new()
                .name("hi-notes".into())
                .spawn(move || loop {
                    let Ok(state) = service.lock() else {
                        return;
                    };
                    if state.stopped {
                        return;
                    }
                    let wait = state
                        .store
                        .scheduler()
                        .next_save_at()
                        .map(|at| Duration::from_millis(at.saturating_sub(service.now())));
                    let waited = match wait {
                        Some(wait) => service.wake.wait_timeout(state, wait).ok().map(|p| p.0),
                        None => service.wake.wait(state).ok(),
                    };
                    let Some(state) = waited else {
                        return;
                    };
                    if state.stopped {
                        return;
                    }
                    drop(state);
                    if let Err(error) = service.save_due(service.now(), false) {
                        tracing::warn!(%error,"Notes save unavailable");
                    }
                })
                .map_err(|_| IslandError::io("Notes worker could not start"))?,
        );
        Ok(())
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        {
            let mut state = self.lock()?;
            state.stopped = true;
            self.wake.notify_all();
        }
        if let Some(task) = self
            .worker
            .lock()
            .map_err(|_| IslandError::internal("Notes worker poisoned"))?
            .take()
        {
            task.join()
                .map_err(|_| IslandError::internal("Notes worker panicked"))?;
        }
        self.save_due(self.now(), true)
    }
}
