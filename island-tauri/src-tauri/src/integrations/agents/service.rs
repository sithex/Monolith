//! Read-only SQLite metadata polling, read in process (`reader.rs`); the pure tracker owns
//! classification and transitions, never SQL or filesystem effects.
use super::core::{self, Event, Tracker};
use super::reader;
use crate::{
    bus::Latest,
    contract::controls::{AgentChip, AgentsChanged},
    error::IslandError,
};
use std::{
    collections::{BTreeSet, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub const POLL: Duration = Duration::from_secs(15);
/// Unchanged database files reuse the last rows this long before a forced re-read.
const REUSE_ROWS: Duration = Duration::from_secs(300);
/// Size and modification time of the database and its WAL: if neither moved, the rows did not.
type Fingerprint = [Option<(u64, SystemTime)>; 2];
fn fingerprint(path: &std::path::Path) -> Fingerprint {
    let stamp = |p: &std::path::Path| {
        std::fs::metadata(p)
            .ok()
            .and_then(|m| Some((m.len(), m.modified().ok()?)))
    };
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    [stamp(path), stamp(std::path::Path::new(&wal))]
}
#[derive(Clone)]
pub struct AgentConfig {
    database: Option<PathBuf>,
}
impl AgentConfig {
    /// Production reads only its user's metadata DB. Test instances never infer that path.
    pub fn for_instance(test: bool, home: Option<PathBuf>, fixture: Option<PathBuf>) -> Self {
        let database = if test {
            fixture.and_then(|p| {
                if !p.is_absolute()
                    || !std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_file() && !m.is_symlink())
                {
                    return None;
                }
                let root = std::env::temp_dir().canonicalize().ok()?;
                let path = p.canonicalize().ok()?;
                path.starts_with(root).then_some(path)
            })
        } else {
            home.map(|p| p.join(".hermes/state.db"))
        };
        Self { database }
    }
}
pub trait AgentEvents: Send + Sync {
    fn deliver(&self, event: &Event) -> Result<(), IslandError>;
    fn clear(&self) -> Result<(), IslandError>;
}
struct State {
    tracker: Tracker,
    outbox: VecDeque<Event>,
    current: AgentsChanged,
}
pub struct AgentService {
    game_pause: crate::integrations::plasma::game_effects::PollPause,
    config: AgentConfig,
    sink: Arc<dyn AgentEvents>,
    state: Mutex<State>,
    serial: Mutex<()>,
    stop: Mutex<bool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    /// Last rows read, so an untouched database costs a `stat` instead of a SQLite read.
    rows: Mutex<Option<(Fingerprint, std::time::Instant, Vec<core::SessionRow>)>>,
    pub topic: Latest<AgentsChanged>,
}
impl AgentService {
    pub fn new(config: AgentConfig, sink: Arc<dyn AgentEvents>) -> Arc<Self> {
        Self::new_gated(config, sink, Default::default())
    }
    pub fn new_gated(
        config: AgentConfig,
        sink: Arc<dyn AgentEvents>,
        game_pause: crate::integrations::plasma::game_effects::PollPause,
    ) -> Arc<Self> {
        let current = AgentsChanged {
            rev: 0,
            available: false,
            detail: "starting".into(),
            chips: vec![],
            overflow: 0,
        };
        let topic = Latest::new();
        topic.publish(current.clone());
        Arc::new(Self {
            game_pause,
            config,
            sink,
            state: Mutex::new(State {
                tracker: Tracker::new(),
                outbox: VecDeque::new(),
                current,
            }),
            serial: Mutex::new(()),
            stop: Mutex::new(false),
            worker: Mutex::new(None),
            rows: Mutex::new(None),
            topic,
        })
    }
    pub fn current(&self) -> Result<AgentsChanged, IslandError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| IslandError::internal("Agent state poisoned"))?
            .current
            .clone())
    }
    fn publish(
        &self,
        state: &mut State,
        available: bool,
        detail: &str,
        chips: Vec<AgentChip>,
        overflow: u32,
    ) -> AgentsChanged {
        let mut next = AgentsChanged {
            rev: state.current.rev,
            available,
            detail: detail.into(),
            chips,
            overflow,
        };
        if next != state.current {
            next.rev = next.rev.wrapping_add(1);
            state.current = next.clone();
            self.topic.publish(next.clone());
        }
        next
    }
    /// The production worker supplies epoch seconds; tests inject the same clock as SQL and
    /// the pure core. Each refresh is serialized and the rows read are bounded.
    pub fn poll_at(&self, now: f64) -> Result<AgentsChanged, IslandError> {
        if !now.is_finite() || now < 0. {
            return Err(IslandError::invalid("Agent clock invalid"));
        }
        let _serial = self
            .serial
            .lock()
            .map_err(|_| IslandError::internal("Agent reader poisoned"))?;
        if *self
            .stop
            .lock()
            .map_err(|_| IslandError::internal("Agent stop poisoned"))?
        {
            return Err(IslandError::unavailable("Agent tracker stopped"));
        }
        self.flush_outbox()?;
        let rows = self.read(now);
        let mut state = self
            .state
            .lock()
            .map_err(|_| IslandError::internal("Agent state poisoned"))?;
        let rows = match rows {
            Ok(rows) => rows,
            Err(error) => {
                self.publish(
                    &mut state,
                    false,
                    if self.config.database.is_none() {
                        "no-fixture-database"
                    } else {
                        "database-unavailable"
                    },
                    vec![],
                    0,
                );
                return Err(error);
            }
        };
        let ids: BTreeSet<String> = rows.iter().map(|r| r.id.clone()).collect();
        let mut candidate = state.tracker.clone();
        let update = candidate.update(rows, now);
        if update.events.len() > 128 {
            self.publish(&mut state, false, "event-outbox-capacity", vec![], 0);
            return Err(IslandError::unavailable(
                "Agent transition batch exceeds bounded outbox",
            ));
        }
        candidate.retain_history(&ids);
        state.tracker = candidate;
        state.outbox.extend(update.events);
        // The core's diagnostics are counters/inspection, not an unbounded lifetime log.
        drop(state.tracker.take_warnings());
        let chips = update
            .chips
            .into_iter()
            .map(|c| AgentChip {
                id: c.id,
                label: c.label,
                model: c.model,
                provider: c.provider,
                elapsed_secs: c.elapsed_secs,
                api_call_count: c.api_call_count.map(|n| n as f64),
                is_delegated: c.is_delegated,
                children: c.children,
            })
            .collect();
        drop(state);
        self.flush_outbox()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| IslandError::internal("Agent state poisoned"))?;
        Ok(self.publish(
            &mut state,
            true,
            "read-only-metadata",
            chips,
            update.overflow,
        ))
    }
    fn flush_outbox(&self) -> Result<(), IslandError> {
        loop {
            let event = self
                .state
                .lock()
                .map_err(|_| IslandError::internal("Agent state poisoned"))?
                .outbox
                .front()
                .cloned();
            let Some(event) = event else {
                return Ok(());
            };
            if let Err(error) = self.sink.deliver(&event) {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| IslandError::internal("Agent state poisoned"))?;
                self.publish(&mut state, false, "event-delivery-failed", vec![], 0);
                return Err(error);
            }
            self.state
                .lock()
                .map_err(|_| IslandError::internal("Agent state poisoned"))?
                .outbox
                .pop_front();
        }
    }
    fn read(&self, now: f64) -> Result<Vec<core::SessionRow>, IslandError> {
        let path = self
            .config
            .database
            .as_ref()
            .ok_or_else(|| IslandError::unavailable("Agent fixture database not configured"))?;
        // Refuse missing/non-regular/symlinked files before SQLite can open any database.
        if !path.is_absolute()
            || !std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !m.is_symlink())
        {
            return Err(IslandError::unavailable("Agent database unavailable"));
        }
        // The tracker re-classifies cached rows against the new `now`, so staleness still
        // advances; only the SQL read is skipped. Its 24 h window is wider than the tracker's
        // lookback, which filters the reused rows again.
        let print = fingerprint(path);
        if let Ok(cache) = self.rows.lock() {
            if let Some((seen, at, rows)) = cache.as_ref() {
                if *seen == print && at.elapsed() < REUSE_ROWS {
                    return Ok(rows.clone());
                }
            }
        }
        let rows = reader::read_sessions(path, now)?;
        if rows.len() > reader::MAX_ROWS {
            return Err(IslandError::unavailable("Agent metadata row limit"));
        }
        if let Ok(mut cache) = self.rows.lock() {
            *cache = Some((print, std::time::Instant::now(), rows.clone()));
        }
        Ok(rows)
    }
    pub fn start(self: &Arc<Self>) -> Result<(), IslandError> {
        let mut slot = self
            .worker
            .lock()
            .map_err(|_| IslandError::internal("Agent worker poisoned"))?;
        if slot.is_some() {
            return Err(IslandError::invalid("Agent worker already running"));
        }
        let service = self.clone();
        *slot = Some(
            std::thread::Builder::new()
                .name("hi-agents".into())
                .spawn(move || loop {
                    match service.stop.lock() {
                        Ok(stop) if !*stop => {}
                        _ => return,
                    }
                    let generation = service.game_pause.generation();
                    let stopped = || service.stop.lock().map_or(true, |s| *s);
                    if service.game_pause.paused() {
                        service.game_pause.wait_blocking(
                            generation,
                            Duration::from_secs(86400),
                            stopped,
                        );
                        continue;
                    }
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs_f64());
                    match now {
                        Ok(now) => {
                            if let Err(error) = service.poll_at(now) {
                                tracing::debug!(%error,"Agent refresh unavailable");
                            }
                        }
                        Err(_) => return,
                    }
                    service.game_pause.wait_blocking(generation, POLL, stopped);
                })
                .map_err(|_| IslandError::io("Agent worker unavailable"))?,
        );
        Ok(())
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        *self
            .stop
            .lock()
            .map_err(|_| IslandError::internal("Agent stop poisoned"))? = true;
        self.game_pause.wake_stop();
        if let Some(task) = self
            .worker
            .lock()
            .map_err(|_| IslandError::internal("Agent worker poisoned"))?
            .take()
        {
            task.join()
                .map_err(|_| IslandError::internal("Agent worker panicked"))?;
        }
        let _serial = self
            .serial
            .lock()
            .map_err(|_| IslandError::internal("Agent reader poisoned"))?;
        self.sink.clear()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| IslandError::internal("Agent state poisoned"))?;
        state.outbox.clear();
        self.publish(&mut state, false, "stopped", vec![], 0);
        Ok(())
    }
}
