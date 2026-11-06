//! Pending Hermes approvals for the notch: what the TUI in the island's terminal is asking,
//! and a guarded way to answer it with the TUI's own quick-pick key.
//!
//! Reading: the island hosts a loopback WebSocket sink ([`ws`]) and hands its URL (with a
//! per-run token) to the PTY child as `HERMES_TUI_SIDECAR_URL`; the TUI mirrors every
//! gateway event there and [`frame`] keeps only approval/clarify prompts.
//!
//! Writing: a digit is typed into the PTY, never Enter and never Esc-for-approve, and only
//! when every guard in [`evaluate`] holds (see `docs`/the report for the list). Anything that
//! fails a guard stays visible but read-only with a note ("open the terminal").
//! Sudo/secret/vault prompts are never published as events, so they are never answerable;
//! the terminal title alone then reports `waiting_unknown`.

pub mod frame;
pub mod host;
pub mod ws;

use crate::contract::approvals::{
    ApprovalChanged, ApprovalChoiceView, ApprovalKind, ApprovalPrompt,
};
use crate::error::IslandError;
use frame::{Incoming, Request};
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

/// The title turns `⚠` a moment after the sink reads the request (the TUI renders first), but
/// the two threads stamp them independently. A title that turned `⚠` up to this much BEFORE
/// the request's stamp still counts as caused by it.
pub const TITLE_SLACK: Duration = Duration::from_millis(250);
/// ...and one that turned `⚠` later than this AFTER the request was not caused by it (a
/// background session's prompt followed by an unrelated sudo/secret box must never bind).
pub const TITLE_WINDOW: Duration = Duration::from_millis(2000);
/// A request the terminal never showed (no `⚠` title) is dropped after this long.
pub const UNSHOWN_MAX_AGE: Duration = Duration::from_secs(60);
/// Grace after the gateway's own timeout before a request is dropped.
const TIMEOUT_GRACE: Duration = Duration::from_secs(10);
const MAX_PENDING: usize = 8;

/// Receives what the service wants shown. Called outside the service lock.
pub trait ApprovalSink: Send + Sync {
    fn changed(&self, change: &ApprovalChanged);
    /// A request appeared: the notch may pop its hover notice.
    fn arrived(&self, id: &str);
    /// A request went away (answered anywhere, settled, cancelled, terminal gone).
    fn cleared(&self, id: &str);
}

/// What the answer path needs from the terminal.
pub trait TerminalKeys {
    /// Pid of the running PTY child, `None` when not running.
    fn pid(&self) -> Option<u32>;
    /// Type `key` into the PTY. Must not count as the user typing.
    fn send_key(&self, key: &str) -> Result<(), IslandError>;
}

/// One terminal run's sidecar registration.
#[derive(Clone, Debug)]
pub struct RunTicket {
    pub id: u64,
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Disabled,
    NoRun,
    NotMirrored,
    Ambiguous,
    ReadOnly(&'static str),
    AlreadyAnswered,
    NotShown,
    Typing,
    PtyChanged,
    Expired,
    Stale,
    BadChoice,
}

impl Refusal {
    fn note(self) -> String {
        match self {
            Self::Disabled => "Approvals in the notch are off".into(),
            Self::NoRun | Self::NotMirrored => {
                "Hermes is not reporting to the island: answer in the terminal".into()
            }
            Self::Ambiguous => "Several requests are waiting: answer in the terminal".into(),
            Self::ReadOnly(why) => why.into(),
            Self::AlreadyAnswered => "Answer sent: waiting for Hermes".into(),
            Self::NotShown => "Open the terminal to answer this".into(),
            Self::Typing => "You are typing in the terminal: answer there".into(),
            Self::PtyChanged => "The terminal restarted: answer there".into(),
            Self::Expired => "This request expired".into(),
            Self::Stale => "That request is no longer pending".into(),
            Self::BadChoice => "That choice is not offered".into(),
        }
    }

    fn error(self) -> IslandError {
        match self {
            Self::Stale | Self::BadChoice => IslandError::invalid(self.note()),
            _ => IslandError::unavailable(self.note()),
        }
    }
}

#[derive(Clone, Debug)]
struct Pending {
    /// The island's id (what the UI carries back).
    id: String,
    request: Request,
    arrived: Instant,
    /// Keystrokes the user had typed when it arrived (clarify guard).
    input_epoch: u64,
    /// The terminal title turned `⚠` because of this request.
    bound: bool,
    answered: bool,
    announced: bool,
}

#[derive(Debug)]
struct Run {
    id: u64,
    token: String,
    pid: Option<u32>,
    conns: usize,
    pending: Vec<Pending>,
    waiting: bool,
    wait_since: Option<Instant>,
    input_epoch: u64,
}

#[derive(Default)]
struct Inner {
    port: Option<u16>,
    run: Option<Run>,
    next_run: u64,
    next_prompt: u64,
    rev: u32,
    last: (Option<ApprovalPrompt>, bool),
    cleared: Vec<String>,
}

#[derive(Default)]
struct Effects {
    changed: Option<ApprovalChanged>,
    arrived: Vec<String>,
    cleared: Vec<String>,
}

pub struct ApprovalService {
    inner: Mutex<Inner>,
    sink: Mutex<Option<Arc<dyn ApprovalSink>>>,
    enabled: Arc<dyn Fn() -> bool + Send + Sync>,
    /// Tests drive the server themselves; production binds lazily on the first run.
    serve: bool,
}

/// Does a title that turned `⚠` at `wait_since` count as caused by a request stamped `arrived`?
fn binds(wait_since: Instant, arrived: Instant) -> bool {
    wait_since + TITLE_SLACK >= arrived && wait_since <= arrived + TITLE_WINDOW
}

/// Every guard on answering. `pty` is `Some(pid)` on the answer path (the PTY must still be
/// the run's child) and `None` when only computing what to show.
fn evaluate(
    run: &Run,
    p: &Pending,
    enabled: bool,
    now: Instant,
    pty: Option<Option<u32>>,
) -> Result<(), Refusal> {
    if !enabled {
        return Err(Refusal::Disabled);
    }
    if let Some(why) = p.request.read_only {
        return Err(Refusal::ReadOnly(why));
    }
    if p.answered {
        return Err(Refusal::AlreadyAnswered);
    }
    if expired(p, now) {
        return Err(Refusal::Expired);
    }
    // Exactly one TUI is mirroring (a second would make "the" foreground prompt ambiguous).
    if run.conns != 1 {
        return Err(Refusal::NotMirrored);
    }
    // Exactly one structured prompt is pending: the TUI shows one card by its own priority.
    if run.pending.len() != 1 {
        return Err(Refusal::Ambiguous);
    }
    // The title must say Hermes waits, and it must have turned so because of THIS request;
    // otherwise the card on screen may be a sudo/secret box or another session's prompt.
    if !(run.waiting && p.bound) {
        return Err(Refusal::NotShown);
    }
    if p.request.kind == ApprovalKind::Clarify && run.input_epoch != p.input_epoch {
        return Err(Refusal::Typing);
    }
    if let Some(pid) = pty {
        if run.pid.is_none() || pid != run.pid {
            return Err(Refusal::PtyChanged);
        }
    }
    Ok(())
}

fn expired(p: &Pending, now: Instant) -> bool {
    let age = now.saturating_duration_since(p.arrived);
    match p.request.timeout_s {
        Some(s) if age > Duration::from_secs(u64::from(s)) + TIMEOUT_GRACE => true,
        _ => !p.bound && age > UNSHOWN_MAX_AGE,
    }
}

fn token_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn random_token() -> Option<String> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Terminal bytes that are the user's own typing. Mouse reports, focus events and the
/// terminal's replies to queries (cursor position, device attributes, OSC colours) are not.
pub fn is_user_keystroke(data: &str) -> bool {
    if data.is_empty()
        || data.starts_with("\x1b[<")
        || data.starts_with("\x1b[M")
        || data.starts_with("\x1b]")
        || data.starts_with("\x1bP")
        || data == "\x1b[I"
        || data == "\x1b[O"
    {
        return false;
    }
    // CSI replies: `ESC [ row ; col R` (cursor position) and `ESC [ ? … c` (attributes).
    let csi_reply = data
        .strip_prefix("\x1b[")
        .is_some_and(|rest| {
            (rest.ends_with('R') && rest.bytes().all(|b| b.is_ascii_digit() || b == b';' || b == b'R'))
                || (rest.starts_with(['?', '>']) && rest.ends_with('c'))
        });
    !csi_reply
}

impl ApprovalService {
    pub fn new(enabled: Arc<dyn Fn() -> bool + Send + Sync>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            sink: Mutex::new(None),
            enabled,
            serve: true,
        })
    }

    /// Test constructor: no lazily bound server.
    #[cfg(test)]
    pub fn for_tests(enabled: Arc<dyn Fn() -> bool + Send + Sync>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            sink: Mutex::new(None),
            enabled,
            serve: false,
        })
    }

    pub fn set_sink(&self, sink: Arc<dyn ApprovalSink>) {
        *self.sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink);
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Run `f` under the lock, recompute what is shown, and tell the sink afterwards.
    fn mutate<R>(&self, now: Instant, f: impl FnOnce(&mut Inner) -> R) -> R {
        let enabled = (self.enabled)();
        let (result, effects) = {
            let mut inner = self.lock();
            let result = f(&mut inner);
            let effects = Self::settle(&mut inner, enabled, now);
            (result, effects)
        };
        self.dispatch(effects);
        result
    }

    fn dispatch(&self, effects: Effects) {
        let sink = self.sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(sink) = sink else { return };
        if let Some(change) = &effects.changed {
            sink.changed(change);
        }
        for id in &effects.arrived {
            sink.arrived(id);
        }
        for id in &effects.cleared {
            sink.cleared(id);
        }
    }

    fn settle(inner: &mut Inner, enabled: bool, now: Instant) -> Effects {
        let mut effects = Effects::default();
        if let Some(run) = inner.run.as_mut() {
            let (keep, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut run.pending)
                .into_iter()
                .partition(|p| !expired(p, now));
            run.pending = keep;
            inner.cleared.extend(gone.into_iter().map(|p| p.id));
            for p in run.pending.iter_mut().filter(|p| !p.announced) {
                p.announced = true;
                if enabled {
                    effects.arrived.push(p.id.clone());
                }
            }
        }
        effects.cleared = std::mem::take(&mut inner.cleared);
        let view = Self::view_of(inner.run.as_ref(), enabled, now);
        if view != inner.last {
            inner.rev = inner.rev.wrapping_add(1);
            inner.last = view.clone();
            effects.changed = Some(ApprovalChanged {
                rev: inner.rev,
                prompt: view.0,
                waiting_unknown: view.1,
            });
        }
        effects
    }

    fn view_of(run: Option<&Run>, enabled: bool, now: Instant) -> (Option<ApprovalPrompt>, bool) {
        let Some(run) = run.filter(|_| enabled) else {
            return (None, false);
        };
        let shown = run
            .pending
            .iter()
            .find(|p| p.request.kind == ApprovalKind::Approval)
            .or_else(|| run.pending.first());
        let Some(p) = shown else {
            return (None, run.waiting);
        };
        let verdict = evaluate(run, p, enabled, now, None);
        let request = &p.request;
        let prompt = ApprovalPrompt {
            id: p.id.clone(),
            kind: request.kind,
            text: request.text.clone(),
            text_truncated: request.truncated,
            description: request.description.clone(),
            choices: request
                .choices
                .iter()
                .map(|c| ApprovalChoiceView {
                    id: c.id.clone(),
                    label: c.label.clone(),
                    style: c.style,
                })
                .collect(),
            answerable: verdict.is_ok(),
            answered: p.answered,
            note: verdict.err().map(Refusal::note),
            more: u32::try_from(run.pending.len().saturating_sub(1)).unwrap_or(u32::MAX),
        };
        (Some(prompt), false)
    }

    /// What the UI should show now (command `approval_get`).
    pub fn snapshot(&self) -> ApprovalChanged {
        self.snapshot_at(Instant::now())
    }

    pub fn snapshot_at(&self, now: Instant) -> ApprovalChanged {
        // Settling first drops what has run out, so a stale prompt is never handed out.
        self.mutate(now, |_| ());
        let inner = self.lock();
        ApprovalChanged {
            rev: inner.rev,
            prompt: inner.last.0.clone(),
            waiting_unknown: inner.last.1,
        }
    }

    // ---- terminal run lifecycle -------------------------------------------------------

    /// A terminal child is about to start: new token, previous run and its prompts forgotten.
    /// `None` = no sidecar (the child then simply gets no `HERMES_TUI_SIDECAR_URL`).
    pub fn begin_run(self: &Arc<Self>) -> Option<RunTicket> {
        let port = self.ensure_server()?;
        let token = random_token()?;
        let id = self.mutate(Instant::now(), |inner| {
            Self::replace_run(inner, token.clone())
        });
        Some(RunTicket {
            id,
            url: format!("ws://127.0.0.1:{port}/sidecar?token={token}"),
        })
    }

    /// Tests: register a run for a server the test started itself.
    #[cfg(test)]
    pub fn begin_run_for_tests(&self, port: u16) -> RunTicket {
        let token = random_token().unwrap_or_else(|| "t".repeat(32));
        let id = self.mutate(Instant::now(), |inner| {
            Self::replace_run(inner, token.clone())
        });
        RunTicket {
            id,
            url: format!("ws://127.0.0.1:{port}/sidecar?token={token}"),
        }
    }

    fn replace_run(inner: &mut Inner, token: String) -> u64 {
        if let Some(old) = inner.run.take() {
            inner.cleared.extend(old.pending.into_iter().map(|p| p.id));
        }
        inner.next_run += 1;
        let id = inner.next_run;
        inner.run = Some(Run {
            id,
            token,
            pid: None,
            conns: 0,
            pending: Vec::new(),
            waiting: false,
            wait_since: None,
            input_epoch: 0,
        });
        id
    }

    fn ensure_server(self: &Arc<Self>) -> Option<u16> {
        if let Some(port) = self.lock().port {
            return Some(port);
        }
        if !self.serve {
            return None;
        }
        let listener = match std::net::TcpListener::bind(("127.0.0.1", 0)) {
            Ok(l) => l,
            Err(error) => {
                tracing::warn!(%error, "approval sidecar could not bind");
                return None;
            }
        };
        let port = listener.local_addr().ok()?.port();
        listener.set_nonblocking(true).ok()?;
        self.lock().port = Some(port);
        let service = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            if let Err(error) = ws::serve(listener, service).await {
                tracing::warn!(%error, "approval sidecar stopped");
            }
        });
        Some(port)
    }

    pub fn run_started(&self, run: u64, pid: Option<u32>) {
        self.mutate(Instant::now(), |inner| {
            if let Some(r) = inner.run.as_mut().filter(|r| r.id == run) {
                r.pid = pid;
            }
        });
    }

    /// The child exited or was replaced: nothing it asked can be answered any more.
    pub fn end_run(&self, run: u64) {
        self.mutate(Instant::now(), |inner| {
            if inner.run.as_ref().is_some_and(|r| r.id == run) {
                if let Some(old) = inner.run.take() {
                    inner.cleared.extend(old.pending.into_iter().map(|p| p.id));
                }
            }
        });
    }

    /// The PTY's title marker changed to (or away from) `⚠`.
    pub fn title(&self, run: u64, waiting: bool) {
        self.title_at(run, waiting, Instant::now());
    }

    pub fn title_at(&self, run: u64, waiting: bool, now: Instant) {
        self.mutate(now, |inner| {
            let Some(r) = inner.run.as_mut().filter(|r| r.id == run) else {
                return;
            };
            if waiting == r.waiting {
                return;
            }
            r.waiting = waiting;
            if waiting {
                r.wait_since = Some(now);
                for p in r.pending.iter_mut().filter(|p| !p.bound) {
                    p.bound = binds(now, p.arrived);
                }
            } else {
                // Hermes stopped waiting: whatever it asked has been dealt with (answered in
                // the terminal, timed out, cancelled).
                r.wait_since = None;
                let (gone, keep): (Vec<_>, Vec<_>) =
                    std::mem::take(&mut r.pending).into_iter().partition(|p| p.bound);
                r.pending = keep;
                inner.cleared.extend(gone.into_iter().map(|p| p.id));
            }
        });
    }

    /// Bytes the user typed into the terminal (not the island's own injected key).
    pub fn user_input(&self, run: u64, data: &str) {
        if !is_user_keystroke(data) {
            return;
        }
        let watched = {
            let mut inner = self.lock();
            match inner.run.as_mut().filter(|r| r.id == run) {
                Some(r) => {
                    r.input_epoch = r.input_epoch.wrapping_add(1);
                    !r.pending.is_empty()
                }
                None => false,
            }
        };
        // Typing only changes what is shown while a prompt waits; no recompute per keystroke.
        if watched {
            self.mutate(Instant::now(), |_| ());
        }
    }

    // ---- sidecar connection ------------------------------------------------------------

    /// A client finished the handshake with `token`; returns the run it belongs to.
    pub fn connection_open(&self, token: &str) -> Option<u64> {
        self.mutate(Instant::now(), |inner| {
            let r = inner
                .run
                .as_mut()
                .filter(|r| token_eq(&r.token, token))?;
            r.conns += 1;
            Some(r.id)
        })
    }

    pub fn connection_closed(&self, run: u64) {
        self.mutate(Instant::now(), |inner| {
            if let Some(r) = inner.run.as_mut().filter(|r| r.id == run) {
                r.conns = r.conns.saturating_sub(1);
            }
        });
    }

    /// Is `run` still the current run (connections of replaced runs get closed)?
    pub fn run_is_current(&self, run: u64) -> bool {
        self.lock().run.as_ref().is_some_and(|r| r.id == run)
    }

    /// One mirrored text frame from the TUI.
    pub fn ingest(&self, run: u64, text: &str) {
        self.ingest_at(run, text, Instant::now());
    }

    pub fn ingest_at(&self, run: u64, text: &str, now: Instant) {
        let Some(incoming) = frame::parse(text) else {
            return;
        };
        self.mutate(now, |inner| {
            let next_prompt = &mut inner.next_prompt;
            let Some(r) = inner.run.as_mut().filter(|r| r.id == run) else {
                return;
            };
            match incoming {
                Incoming::Request(request) => {
                    if let Some(p) = r
                        .pending
                        .iter_mut()
                        .find(|p| p.request.prompt_id == request.prompt_id)
                    {
                        // The TUI re-publishes prompts on resume: same prompt, same island id.
                        p.request = *request;
                        return;
                    }
                    if r.pending.len() >= MAX_PENDING {
                        return;
                    }
                    *next_prompt += 1;
                    let bound = r.waiting && r.wait_since.is_some_and(|w| binds(w, now));
                    r.pending.push(Pending {
                        id: format!("ap{}-{}", r.id, *next_prompt),
                        request: *request,
                        arrived: now,
                        input_epoch: r.input_epoch,
                        bound,
                        answered: false,
                        announced: false,
                    });
                }
                Incoming::Settled { prompt_id } | Incoming::Cancel { id: prompt_id } => {
                    let (gone, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut r.pending)
                        .into_iter()
                        .partition(|p| p.request.prompt_id == prompt_id);
                    r.pending = keep;
                    inner.cleared.extend(gone.into_iter().map(|p| p.id));
                }
            }
        });
    }

    // ---- answering -----------------------------------------------------------------------

    /// Answer the prompt the UI saw. Every guard is re-checked here; on success the choice's
    /// quick-pick digit has been typed into the PTY.
    pub fn answer(
        &self,
        prompt_id: &str,
        choice: &str,
        terminal: &dyn TerminalKeys,
    ) -> Result<ApprovalChanged, IslandError> {
        self.answer_at(prompt_id, choice, terminal, Instant::now())
    }

    pub fn answer_at(
        &self,
        prompt_id: &str,
        choice: &str,
        terminal: &dyn TerminalKeys,
        now: Instant,
    ) -> Result<ApprovalChanged, IslandError> {
        let enabled = (self.enabled)();
        let pty = terminal.pid();
        let authorised: Result<u8, Refusal> = self.mutate(now, |inner| {
            let run = inner.run.as_mut().ok_or(Refusal::NoRun)?;
            let index = run
                .pending
                .iter()
                .position(|p| p.id == prompt_id)
                .ok_or(Refusal::Stale)?;
            evaluate(run, &run.pending[index], enabled, now, Some(pty))?;
            let digit = run.pending[index]
                .request
                .choices
                .iter()
                .find(|c| c.id == choice)
                .map(|c| c.digit)
                .ok_or(Refusal::BadChoice)?;
            // Single shot: set before the lock is released, so a double click sends one key.
            run.pending[index].answered = true;
            Ok(digit)
        });
        let digit = authorised.map_err(Refusal::error)?;
        if let Err(error) = terminal.send_key(&digit.to_string()) {
            self.mutate(now, |inner| {
                if let Some(p) = inner
                    .run
                    .as_mut()
                    .and_then(|r| r.pending.iter_mut().find(|p| p.id == prompt_id))
                {
                    p.answered = false;
                }
            });
            return Err(error);
        }
        Ok(self.snapshot_at(now))
    }
}

#[cfg(test)]
mod tests;
