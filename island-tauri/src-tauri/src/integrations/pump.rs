//! Keeps the long-lived event pumps (notification hub, clipboard delivery) alive.
//!
//! A pump used to end for good on its first error: one full actor queue or one failed emit
//! and notifications stopped until the island was restarted, with nothing on screen saying
//! so. Three small tools fix that without unbounded queues or retry storms:
//! - [`Backoff`]: doubling delays with a ceiling;
//! - [`deliver`]: a few bounded retries while the state actor's queue is full (`Busy`);
//! - [`supervise`]: restarts a pump that failed, with backoff and a restart cap, and reports
//!   its health so the service can show `available: false` with a reason.
//!
//! A release build aborts on panic, so a panicking pump still ends the island; the
//! supervisor's panic branch only matters in debug builds and tests.
use crate::error::{ErrorCode, IslandError};
use std::{
    future::Future,
    time::{Duration, Instant},
};
use tokio::sync::watch;

/// Doubling delays from `first` up to `max`.
#[derive(Clone, Copy, Debug)]
pub struct Backoff {
    first: Duration,
    max: Duration,
    next: Duration,
}

impl Backoff {
    pub const fn new(first: Duration, max: Duration) -> Self {
        Self {
            first,
            max,
            next: first,
        }
    }
    /// The delay to wait now; the following one doubles (never past `max`).
    pub fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        self.next = self.next.saturating_mul(2).min(self.max);
        delay
    }
    pub fn reset(&mut self) {
        self.next = self.first;
    }
}

/// Waits `delay` unless a stop is signalled first. Returns `true` when stopped.
pub async fn pause(delay: Duration, stop: &mut watch::Receiver<bool>) -> bool {
    if *stop.borrow() {
        return true;
    }
    tokio::select! { biased;
        changed = stop.changed() => changed.is_err() || *stop.borrow(),
        () = tokio::time::sleep(delay) => false,
    }
}

/// Waits between attempts after a `Busy` answer: ~210 ms in total, then the caller keeps
/// the work for its next round instead of spinning here.
pub const BUSY_RETRIES: [Duration; 3] = [
    Duration::from_millis(10),
    Duration::from_millis(40),
    Duration::from_millis(160),
];

#[derive(Debug)]
pub enum Delivery<T> {
    Done(T),
    /// Still `Busy` after every retry. Nothing was lost: try again later.
    Busy,
    /// A stop was signalled while waiting.
    Stopped,
    /// Any other error. The caller decides whether it means the receiver is gone.
    Failed(IslandError),
}

/// Runs `attempt`, retrying only `Busy` answers, with the short waits in [`BUSY_RETRIES`].
pub async fn deliver<T, F, Fut>(mut attempt: F, stop: &mut watch::Receiver<bool>) -> Delivery<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, IslandError>>,
{
    let waits = std::iter::once(None).chain(BUSY_RETRIES.iter().copied().map(Some));
    for wait in waits {
        if let Some(wait) = wait {
            if pause(wait, stop).await {
                return Delivery::Stopped;
            }
        }
        match attempt().await {
            Ok(value) => return Delivery::Done(value),
            Err(error) if error.code == ErrorCode::Busy => {}
            Err(error) => return Delivery::Failed(error),
        }
    }
    Delivery::Busy
}

/// What a supervised pump is doing, for the service's availability report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PumpHealth {
    Running,
    /// It failed and will be started again after a pause.
    Restarting,
    /// It failed too often in a row and was left stopped.
    Stopped,
}

#[derive(Clone, Copy, Debug)]
pub struct RestartPolicy {
    /// Restarts allowed in a row before the pump is left stopped.
    pub max_restarts: u32,
    pub backoff: Backoff,
    /// A run at least this long counts as healthy and resets the restart count.
    pub healthy_after: Duration,
}

impl RestartPolicy {
    /// Production policy: 1 s, 2 s, 4 s … 30 s between restarts, at most 5 in a row.
    pub const fn standard() -> Self {
        Self {
            max_restarts: 5,
            backoff: Backoff::new(Duration::from_secs(1), Duration::from_secs(30)),
            healthy_after: Duration::from_secs(60),
        }
    }
}

/// Runs `make()` as a task and starts it again when it fails (returns `Err` or panics),
/// until `stop` is signalled or the restart cap is reached. A pump that returns `Ok(())`
/// finished on purpose (stop requested or its receiver is gone) and is not restarted.
pub async fn supervise<F, Fut>(
    name: &'static str,
    policy: RestartPolicy,
    mut stop: watch::Receiver<bool>,
    mut make: F,
    report: impl Fn(PumpHealth),
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), IslandError>> + Send + 'static,
{
    let mut backoff = policy.backoff;
    let mut restarts = 0u32;
    loop {
        report(PumpHealth::Running);
        let started = Instant::now();
        let outcome = tokio::spawn(make()).await;
        if *stop.borrow() {
            return;
        }
        match outcome {
            Ok(Ok(())) => return,
            Ok(Err(error)) => tracing::error!(%error, pump = name, "Event pump failed"),
            Err(error) if error.is_panic() => tracing::error!(pump = name, "Event pump panicked"),
            Err(_) => tracing::warn!(pump = name, "Event pump was cancelled"),
        }
        if started.elapsed() >= policy.healthy_after {
            restarts = 0;
            backoff.reset();
        }
        if restarts >= policy.max_restarts {
            tracing::error!(pump = name, restarts, "Event pump left stopped after repeated failures");
            report(PumpHealth::Stopped);
            return;
        }
        restarts += 1;
        report(PumpHealth::Restarting);
        let delay = backoff.next_delay();
        tracing::warn!(pump = name, attempt = restarts, ?delay, "Restarting event pump");
        if pause(delay, &mut stop).await {
            return;
        }
    }
}
