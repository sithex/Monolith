//! Quitting cannot hang: one aggregate deadline for every teardown step.
//!
//! Each step runs on its own short-lived thread, in order, and the caller (the GTK main
//! thread on `RunEvent::Exit`) waits at most for that step's own cap and never past the
//! shared deadline. A step that overruns is logged as `timed out` and left behind (the
//! process is exiting), a step that returns an error as `failed`, a step that panics as
//! `panicked`, so a hung quit and a clean quit no longer look the same in the log.
use std::{
    sync::mpsc,
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// How a teardown step ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    Done,
    Failed(String),
    TimedOut,
    Panicked,
    /// The shared deadline was already spent; the step was not started.
    Skipped,
}

pub struct Step {
    name: &'static str,
    cap: Duration,
    run: Box<dyn FnOnce() -> Result<(), String> + Send + 'static>,
}

impl Step {
    pub fn new(
        name: &'static str,
        cap: Duration,
        run: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) -> Self {
        Self {
            name,
            cap,
            run: Box::new(run),
        }
    }
}

/// Runs `steps` in order within `deadline` overall. Returns each step's outcome, logged.
pub fn run(steps: Vec<Step>, deadline: Duration) -> Vec<(&'static str, StepOutcome)> {
    let end = Instant::now() + deadline;
    let mut outcomes = Vec::with_capacity(steps.len());
    for step in steps {
        let remaining = end.saturating_duration_since(Instant::now());
        let outcome = if remaining.is_zero() {
            StepOutcome::Skipped
        } else {
            run_step(step.name, step.run, step.cap.min(remaining))
        };
        match &outcome {
            StepOutcome::Done => tracing::debug!(step = step.name, "Shutdown step done"),
            StepOutcome::Failed(error) => {
                tracing::warn!(step = step.name, %error, "Shutdown step failed")
            }
            StepOutcome::TimedOut => {
                tracing::warn!(step = step.name, cap = ?step.cap.min(remaining), "Shutdown step timed out")
            }
            StepOutcome::Panicked => tracing::error!(step = step.name, "Shutdown step panicked"),
            StepOutcome::Skipped => {
                tracing::warn!(step = step.name, "Shutdown step skipped: deadline spent")
            }
        }
        outcomes.push((step.name, outcome));
    }
    outcomes
}

fn run_step(
    name: &'static str,
    work: Box<dyn FnOnce() -> Result<(), String> + Send>,
    wait: Duration,
) -> StepOutcome {
    let (done, finished) = mpsc::sync_channel(1);
    let spawned = std::thread::Builder::new()
        .name(format!("hi-quit-{name}"))
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
            let _ = done.send(result);
        });
    if let Err(error) = spawned {
        return StepOutcome::Failed(format!("could not start its thread: {error}"));
    }
    match finished.recv_timeout(wait) {
        Ok(Ok(Ok(()))) => StepOutcome::Done,
        Ok(Ok(Err(error))) => StepOutcome::Failed(error),
        Ok(Err(_)) => StepOutcome::Panicked,
        Err(mpsc::RecvTimeoutError::Timeout) => StepOutcome::TimedOut,
        // The sender dropped without a result: the work panicked past catch_unwind.
        Err(mpsc::RecvTimeoutError::Disconnected) => StepOutcome::Panicked,
    }
}

/// How a bounded join ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinOutcome {
    Joined,
    Panicked,
    TimedOut,
}

/// A thread whose join can be bounded: it signals its own end (normally or by unwinding),
/// so the joiner waits on a channel with a timeout instead of an unbounded `join()`.
pub struct BoundedThread {
    handle: JoinHandle<()>,
    ended: mpsc::Receiver<()>,
}

impl BoundedThread {
    pub fn spawn(name: &str, work: impl FnOnce() + Send + 'static) -> std::io::Result<Self> {
        let (signal, ended) = mpsc::sync_channel::<()>(1);
        let handle = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                // Dropped when `work` returns or unwinds: either way the joiner wakes.
                let _signal = signal;
                work();
            })?;
        Ok(Self { handle, ended })
    }

    /// Joins within `timeout`. On a timeout the thread is left running (detached).
    pub fn join_within(self, timeout: Duration) -> JoinOutcome {
        match self.ended.recv_timeout(timeout) {
            Err(mpsc::RecvTimeoutError::Timeout) => JoinOutcome::TimedOut,
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => match self.handle.join() {
                Ok(()) => JoinOutcome::Joined,
                Err(_) => JoinOutcome::Panicked,
            },
        }
    }
}
