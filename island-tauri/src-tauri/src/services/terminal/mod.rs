//! The island terminal (port of island-rust `terminal.rs`, SPEC §3.4).
//!
//! The renderer owns a view, not the process: collapse/unmount/webview reload never closes the
//! PTY. Output is delivered to the *currently attached* sink (a `tauri::ipc::Channel` in
//! production) with credit-based flow control (`flow.rs`); the replay buffer (1 MiB) lets a
//! re-attached client catch up. `attach` swaps the sink and takes the snapshot under the same
//! lock that `append` uses, so a client gets `snapshot.seq` and then only newer chunks.
//!
//! The fixture shell (`bash --noprofile --norc`) exists for tests only and re-checks
//! `HERMES_ISLAND_TEST=1` itself (defense in depth behind the CLI gate).

pub mod activity;
pub mod flow;
pub mod pty;

use crate::{
    contract::terminal::{TerminalEvent, TerminalSnapshot},
    error::IslandError,
};
use flow::{Flow, DEFAULT_CREDIT_LIMIT, MAX_CHUNK};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use pty::{
    checked_fd, decode, kill_owned, process, resolve_hermes, scrub_key, split_chunks, wait_ready,
};
use std::{
    fs,
    io::{Read, Write},
    os::fd::{AsRawFd, OwnedFd},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const REPLAY_LIMIT: usize = 1024 * 1024;
/// SPEC §4.3: UTF-8 bytes, not UTF-16 code units.
pub const INPUT_LIMIT_BYTES: usize = 256 * 1024;
const INPUT_DEADLINE: Duration = Duration::from_millis(25);

/// Where terminal events go. `send` returns `false` when the receiver is gone (the webview was
/// closed); the terminal then detaches it.
pub trait TerminalSink: Send + Sync {
    fn send(&self, event: TerminalEvent) -> bool;
}

impl<F> TerminalSink for F
where
    F: Fn(TerminalEvent) -> bool + Send + Sync,
{
    fn send(&self, event: TerminalEvent) -> bool {
        self(event)
    }
}

#[derive(Clone, Debug)]
pub struct TerminalConfig {
    /// Working directory of the child (the repository root).
    pub root: PathBuf,
    /// Home used to resolve the real Hermes launcher (never read for the fixture).
    pub home: PathBuf,
    /// Spawn the `bash --norc` fixture instead of Hermes (test instances only).
    pub fixture: bool,
    /// Unacknowledged bytes at which the reader pauses.
    pub credit_limit: usize,
    /// PATH used to find plain `hermes` when the private wrapper is absent.
    pub search_path: Option<std::ffi::OsString>,
}

impl TerminalConfig {
    pub fn new(root: PathBuf, home: PathBuf, fixture: bool) -> Self {
        Self {
            root,
            home,
            fixture,
            credit_limit: DEFAULT_CREDIT_LIMIT,
            search_path: std::env::var_os("PATH"),
        }
    }
}

struct State {
    pid: Option<u32>,
    data: String,
    seq: u32,
    started: bool,
    running: bool,
    exit_code: Option<i32>,
    sink: Option<Arc<dyn TerminalSink>>,
    flow: Arc<Flow>,
}

impl State {
    fn new(flow: Arc<Flow>) -> Self {
        Self {
            pid: None,
            data: String::new(),
            seq: 0,
            started: false,
            running: false,
            exit_code: None,
            sink: None,
            flow,
        }
    }

    fn snapshot(&self) -> TerminalSnapshot {
        TerminalSnapshot {
            pid: self.pid,
            running: self.running,
            data: self.data.clone(),
            seq: self.seq,
            exit_code: self.exit_code,
        }
    }

    /// Add one chunk (≤ `MAX_CHUNK`) to the replay buffer and, if a client is attached, record
    /// it as in flight. The caller sends it *after* releasing the lock.
    fn append(&mut self, text: &str) -> (u32, Option<Arc<dyn TerminalSink>>) {
        self.data.push_str(text);
        if self.data.len() > REPLAY_LIMIT {
            let mut cut = self.data.len() - REPLAY_LIMIT;
            while !self.data.is_char_boundary(cut) {
                cut += 1;
            }
            self.data.drain(..cut);
        }
        self.seq = self.seq.wrapping_add(1);
        let sink = self.sink.clone();
        if sink.is_some() {
            self.flow.record(self.seq, text.len());
        }
        (self.seq, sink)
    }
}

struct SharedState {
    state: Mutex<State>,
    /// Serializes sends and attach; ack never takes it, so synchronous sink acks are safe.
    delivery: Mutex<()>,
    /// Hermes' busy/idle title marker, for the notch's "Hermes is working" pulse.
    activity: Mutex<activity::TitleActivity>,
}
impl SharedState {
    fn activity(&self) -> MutexGuard<'_, activity::TitleActivity> {
        self.activity.lock().unwrap_or_else(|e| e.into_inner())
    }
}
type Shared = Arc<SharedState>;

fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    // A panicked holder cannot leave State half-updated in a way later readers can't tolerate.
    shared.state.lock().unwrap_or_else(|e| e.into_inner())
}

/// Deliver one chunk to the sink captured at append time; a dead sink is detached.
fn deliver(shared: &Shared, seq: u32, text: &str, sink: Option<Arc<dyn TerminalSink>>) {
    let Some(sink) = sink else { return };
    let event = TerminalEvent::Data {
        seq,
        data: text.to_owned(),
    };
    if !sink.send(event) {
        let mut state = lock(shared);
        if state.sink.as_ref().is_some_and(|s| Arc::ptr_eq(s, &sink)) {
            state.sink = None;
            state.flow.reset();
        }
    }
}

fn publish(shared: &Shared, text: &str) {
    for piece in split_chunks(text, MAX_CHUNK) {
        let _delivery = shared.delivery.lock().unwrap_or_else(|e| e.into_inner());
        let (seq, sink) = lock(shared).append(piece);
        deliver(shared, seq, piece, sink);
    }
}

#[derive(Default)]
struct Run {
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    worker: Option<JoinHandle<()>>,
    owned: Option<(u32, u64)>,
    cancel: Arc<AtomicBool>,
    wake: Option<Arc<OwnedFd>>,
}

pub struct Terminal {
    cfg: TerminalConfig,
    settings: Option<Arc<super::settings::SettingsService>>,
    shared: Shared,
    run: Mutex<Run>,
}

/// Defense in depth behind the CLI gate: the fixture shell only runs with `HERMES_ISLAND_TEST=1`.
pub fn fixture_allowed(env_value: Option<&str>) -> Result<(), IslandError> {
    if env_value == Some("1") {
        Ok(())
    } else {
        Err(IslandError::unsupported(
            "the fixture shell requires HERMES_ISLAND_TEST=1",
        ))
    }
}

/// Empty `terminalCommand`: the author's private wrapper when it exists (unchanged behaviour
/// on that machine), otherwise plain `hermes chat --tui` resolved like any custom program.
/// A fresh clone therefore works without editing Settings first.
fn default_hermes_command(
    home: &std::path::Path,
    cwd: &std::path::Path,
    path: Option<&std::ffi::OsStr>,
) -> Result<CommandBuilder, IslandError> {
    match resolve_hermes(home) {
        Ok((python, script)) => {
            let mut c = CommandBuilder::new(python);
            c.arg("-I");
            c.arg(script);
            c.args(["chat", "--tui"]);
            Ok(c)
        }
        Err(_) => {
            let hermes = pty::resolve_custom_program(
                "hermes", home, cwd, path,
            )
            .map_err(|_| {
                IslandError::unavailable(
                    "Hermes not found on PATH. Install Hermes Agent or set Settings → General → Terminal",
                )
            })?;
            let mut c = CommandBuilder::new(hermes);
            c.args(["chat", "--tui"]);
            Ok(c)
        }
    }
}

fn io_error(error: impl ToString) -> IslandError {
    IslandError::io(error.to_string())
}

impl Terminal {
    pub fn new(cfg: TerminalConfig) -> Self {
        let flow = Arc::new(Flow::new(cfg.credit_limit));
        Self {
            cfg,
            settings: None,
            shared: Arc::new(SharedState {
                state: Mutex::new(State::new(flow)),
                delivery: Mutex::new(()),
                activity: Mutex::new(activity::TitleActivity::default()),
            }),
            run: Mutex::new(Run::default()),
        }
    }

    /// Share the settings authority; read only when creating a new process, never restart on save.
    pub fn with_settings(
        cfg: TerminalConfig,
        settings: Arc<super::settings::SettingsService>,
    ) -> Self {
        let mut terminal = Self::new(cfg);
        terminal.settings = Some(settings);
        terminal
    }

    fn run(&self) -> Result<MutexGuard<'_, Run>, IslandError> {
        self.run
            .lock()
            .map_err(|_| IslandError::internal("terminal state is poisoned"))
    }

    pub fn pid(&self) -> Option<u32> {
        lock(&self.shared).pid
    }

    /// True while the running session is producing sustained output (see `activity`).
    pub fn working(&self) -> bool {
        if !lock(&self.shared).running {
            return false;
        }
        self.shared.activity().working()
    }

    /// True while Hermes waits for the user's answer (approval/clarify title marker).
    pub fn waiting(&self) -> bool {
        if !lock(&self.shared).running {
            return false;
        }
        self.shared.activity().waiting()
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        lock(&self.shared).snapshot()
    }

    pub fn flow_metrics(&self) -> flow::FlowMetrics {
        lock(&self.shared).flow.metrics()
    }

    /// Spawn on first use; later calls return the current snapshot (idempotent).
    pub fn start(&self) -> Result<TerminalSnapshot, IslandError> {
        let mut run = self.run()?;
        if lock(&self.shared).started {
            return Ok(self.snapshot());
        }
        self.spawn(&mut run)
    }

    /// Start if needed, then make `sink` the one client. The snapshot and the sink swap happen
    /// atomically with respect to `append`; in-flight accounting restarts for the new client.
    pub fn attach(&self, sink: Arc<dyn TerminalSink>) -> Result<TerminalSnapshot, IslandError> {
        let mut run = self.run()?;
        if !lock(&self.shared).started {
            self.spawn(&mut run)?;
        }
        let _delivery = self
            .shared
            .delivery
            .lock()
            .map_err(|_| IslandError::internal("terminal delivery is poisoned"))?;
        let snapshot = {
            let mut state = lock(&self.shared);
            let snapshot = state.snapshot();
            state.sink = Some(sink.clone());
            state
                .flow
                .replace_snapshot(snapshot.seq, snapshot.data.len());
            snapshot
        };
        if !sink.send(TerminalEvent::Snapshot(snapshot.clone())) {
            let mut state = lock(&self.shared);
            state.sink = None;
            state.flow.reset();
        }
        Ok(snapshot)
    }

    /// Respawn after the process exited. A running terminal just returns its snapshot.
    pub fn restart(&self) -> Result<TerminalSnapshot, IslandError> {
        let mut run = self.run()?;
        {
            let state = lock(&self.shared);
            if !state.started {
                return Err(IslandError::unavailable("No terminal"));
            }
            if state.running {
                return Ok(state.snapshot());
            }
        }
        self.release(&mut run);
        self.spawn(&mut run)
    }

    /// Client has parsed everything up to `seq`: grant credit. Unknown/stale/ahead values are
    /// ignored (see `flow.rs`).
    pub fn ack(&self, seq: u32) {
        let flow = lock(&self.shared).flow.clone();
        flow.ack(seq);
    }

    pub fn input(&self, input: &str) -> Result<(), IslandError> {
        let mut run = self.run()?;
        {
            let state = lock(&self.shared);
            if !state.started {
                return Err(IslandError::unavailable("No terminal"));
            }
            if input.len() > INPUT_LIMIT_BYTES {
                return Err(IslandError::invalid("Invalid terminal input"));
            }
            if !state.running {
                return Err(IslandError::unavailable("Terminal is not running"));
            }
        }
        let cancel = run.cancel.clone();
        let writer = run
            .writer
            .as_mut()
            .ok_or_else(|| IslandError::unavailable("Terminal is not running"))?;
        let deadline = Instant::now() + INPUT_DEADLINE;
        let mut accepted = 0;
        while accepted < input.len() {
            if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
                return Err(IslandError::new(
                    crate::error::ErrorCode::Busy,
                    format!(
                        "Terminal input backpressure: {accepted} bytes accepted of {}; remaining bytes were not sent",
                        input.len()
                    ),
                ));
            }
            match writer.write(&input.as_bytes()[accepted..]) {
                Ok(0) => {
                    return Err(IslandError::io(format!(
                        "Terminal input closed: {accepted} bytes accepted"
                    )))
                }
                Ok(n) => accepted += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(e) => return Err(IslandError::io(format!("{e}: {accepted} bytes accepted"))),
            }
        }
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), IslandError> {
        let run = self.run()?;
        let (running, started) = {
            let state = lock(&self.shared);
            (state.running, state.started)
        };
        // The renderer's optional resize is a no-op before the first start.
        if !started {
            return Ok(());
        }
        if !(2..=500).contains(&cols) || !(2..=200).contains(&rows) {
            return Err(IslandError::invalid("Invalid terminal dimensions"));
        }
        if running {
            run.master
                .as_ref()
                .ok_or_else(|| IslandError::unavailable("Terminal is not running"))?
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(io_error)?;
        }
        Ok(())
    }

    /// Kill the owned process tree and join the workers (also runs on drop).
    pub fn shutdown(&self) {
        match self.run.lock() {
            Ok(mut run) => self.release(&mut run),
            Err(poisoned) => self.release(&mut poisoned.into_inner()),
        }
    }

    fn command(&self) -> Result<CommandBuilder, IslandError> {
        let mut command = if self.cfg.fixture {
            fixture_allowed(std::env::var("HERMES_ISLAND_TEST").ok().as_deref())?;
            let mut c = CommandBuilder::new("/bin/bash");
            c.args(["--noprofile", "--norc"]);
            c.env("PS1", "island-test> ");
            c
        } else {
            let value = self
                .settings
                .as_ref()
                .map(|s| s.get())
                .transpose()?
                .map(|s| s.terminal_command)
                .unwrap_or_default();
            let argv = crate::contract::settings::parse_terminal_command(&value)?;
            let mut c = if value.trim().is_empty() {
                default_hermes_command(
                    &self.cfg.home,
                    &self.cfg.root,
                    self.cfg.search_path.as_deref(),
                )?
            } else {
                let program = argv.first().filter(|s| !s.is_empty()).ok_or_else(|| {
                    IslandError::invalid("Invalid terminalCommand: empty executable")
                })?;
                let executable = pty::resolve_custom_program(
                    program,
                    &self.cfg.home,
                    &self.cfg.root,
                    std::env::var_os("PATH").as_deref(),
                )?;
                let mut c = CommandBuilder::new(executable);
                c.args(&argv[1..]);
                c
            };
            c.env("TERM_PROGRAM", "hermes-island");
            c
        };
        command.cwd(&self.cfg.root);
        // Keep HOME/profile selection/TMPDIR, scrub only the original session's hooks.
        for (key, _) in std::env::vars_os() {
            if scrub_key(&key.to_string_lossy()) {
                command.env_remove(key);
            }
        }
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        Ok(command)
    }

    fn spawn(&self, run: &mut Run) -> Result<TerminalSnapshot, IslandError> {
        let command = self.command()?;
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(io_error)?;
        // Duplicated PTY descriptors share O_NONBLOCK: both directions stay cancellable.
        let fd = pair
            .master
            .as_raw_fd()
            .ok_or_else(|| IslandError::io("PTY has no Unix descriptor"))?;
        // SAFETY: fd is the live master descriptor owned by `pair`.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        // SAFETY: as above.
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        // Acquire handles before spawn so errors cannot strand a child.
        // SAFETY: duplicating a live descriptor.
        let reader_fd = checked_fd(unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) })?;
        let mut reader = fs::File::from(reader_fd);
        // SAFETY: plain syscall; ownership is taken by checked_fd.
        let wake = Arc::new(checked_fd(unsafe {
            libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK)
        })?);
        let writer = pair.master.take_writer().map_err(io_error)?;
        let mut child = pair.slave.spawn_command(command).map_err(io_error)?;
        drop(pair.slave);
        let pid = child.process_id();
        // pidfds wake both workers on leader exit, even if an orphan retains the slave.
        let leader_fd = pid
            .and_then(|p| {
                // SAFETY: plain syscall; ownership is taken by checked_fd.
                checked_fd(unsafe { libc::syscall(libc::SYS_pidfd_open, p, 0) as i32 }).ok()
            })
            .map(Arc::new);
        run.owned = pid.and_then(|p| process(p).map(|info| (p, info.birth)));

        // New run: fresh flow accounting, the attached client (if any) stays attached.
        let flow = Arc::new(Flow::new(self.cfg.credit_limit));
        let snapshot = {
            let mut state = lock(&self.shared);
            let sink = state.sink.take();
            *state = State::new(flow.clone());
            state.sink = sink;
            state.pid = pid;
            state.started = true;
            state.running = true;
            state.snapshot()
        };
        run.master = Some(pair.master);
        run.writer = Some(writer);
        run.cancel = Arc::new(AtomicBool::new(false));
        run.wake = Some(wake.clone());
        let cancel = run.cancel.clone();
        let exited = Arc::new(AtomicBool::new(false));

        let reader_worker = {
            let shared = self.shared.clone();
            let cancel = cancel.clone();
            let exited = exited.clone();
            let wake = wake.clone();
            let leader = leader_fd.clone();
            let flow = flow.clone();
            thread::spawn(move || {
                let mut buffer = [0u8; 16384];
                let mut pending = Vec::new();
                shared.activity().reset();
                while !cancel.load(Ordering::Acquire) {
                    // Credit: stop reading while too much is unacknowledged. Blocks on a
                    // condvar (no polling); woken by ack, re-attach, cancel and child exit.
                    flow.wait_for_credit();
                    if cancel.load(Ordering::Acquire) {
                        break;
                    }
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            flow.read(n);
                            shared.activity().output(&buffer[..n]);
                            pending.extend_from_slice(&buffer[..n]);
                            let text = decode(&mut pending, false);
                            publish(&shared, &text);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // Drain available output after leader exit; a setsid orphan need
                            // not close.
                            if exited.load(Ordering::Acquire) {
                                break;
                            }
                            if wait_ready(Some(reader.as_raw_fd()), &wake, leader.as_deref()) {
                                exited.store(true, Ordering::Release);
                            }
                        }
                        // Linux PTYs report EIO rather than EOF when the slave closes.
                        Err(_) => break,
                    }
                }
                let tail = decode(&mut pending, true);
                publish(&shared, &tail);
            })
        };

        let shared = self.shared.clone();
        let owned = run.owned;
        run.worker = Some(thread::spawn(move || {
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Some(status),
                    Err(_) => break None,
                    Ok(None) => {}
                }
                if cancel.load(Ordering::Acquire) {
                    if let Err(error) = child.kill() {
                        tracing::debug!(%error, "terminal child kill");
                    }
                    // Reap normal kills without waiting indefinitely on an uninterruptible child.
                    let deadline = Instant::now() + Duration::from_millis(200);
                    while Instant::now() < deadline {
                        if !matches!(child.try_wait(), Ok(None)) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    break None;
                }
                wait_ready(None, &wake, leader_fd.as_deref());
            };
            exited.store(true, Ordering::Release);
            // The child is gone: the credit limit no longer applies, the reader drains what the
            // kernel still holds (bounded by the PTY buffer) and ends.
            flow.close();
            // Best-effort process cleanup. Reader shutdown does not depend on discovering a
            // setsid descendant after it has already been reparented away from this tree.
            if let Some(owner) = owned {
                kill_owned(owner);
            }
            if reader_worker.join().is_err() {
                tracing::warn!("terminal reader thread panicked");
            }
            let signal = status.as_ref().and_then(|s| s.signal().map(str::to_owned));
            let code = match (&status, &signal) {
                (_, Some(_)) => None,
                (Some(s), None) => Some(i32::try_from(s.exit_code()).unwrap_or(i32::MAX)),
                (None, None) => Some(1),
            };
            let _delivery = shared.delivery.lock().unwrap_or_else(|e| e.into_inner());
            let sink = {
                let mut state = lock(&shared);
                state.running = false;
                state.exit_code = code;
                state.sink.clone()
            };
            // Exit is not subject to credit. A cancelled run (restart/shutdown) says nothing.
            if let Some(sink) = sink {
                if !cancel.load(Ordering::Acquire)
                    && !sink.send(TerminalEvent::Exit { code, signal })
                {
                    let mut state = lock(&shared);
                    if state.sink.as_ref().is_some_and(|s| Arc::ptr_eq(s, &sink)) {
                        state.sink = None;
                    }
                }
            }
        }));
        Ok(snapshot)
    }

    fn release(&self, run: &mut Run) {
        run.cancel.store(true, Ordering::Release);
        lock(&self.shared).flow.close();
        if let Some(wake) = &run.wake {
            let value = 1u64.to_ne_bytes();
            // SAFETY: wake is a live eventfd; the buffer is 8 valid bytes.
            let written =
                unsafe { libc::write(wake.as_raw_fd(), value.as_ptr().cast(), value.len()) };
            if written < 0 {
                tracing::debug!("terminal wake write failed");
            }
        }
        if let Some(owner) = run.owned.take() {
            kill_owned(owner);
        }
        run.writer.take();
        run.master.take();
        if let Some(worker) = run.worker.take() {
            if worker.join().is_err() {
                tracing::warn!("terminal worker thread panicked");
            }
        }
        run.wake.take();
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests;
