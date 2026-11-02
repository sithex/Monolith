//! Credit-based flow control (SPEC §3.4).
//!
//! Rust sends `Data { seq, data }` chunks; the UI acknowledges a chunk once xterm has *parsed*
//! it. The PTY reader stops reading while unacknowledged bytes exceed `limit`, so a slow
//! renderer back-pressures the child (through the kernel PTY buffer) instead of growing memory.
//!
//! Sequence numbers are `u32` and wrap; comparisons use the signed-difference rule. An
//! acknowledgement for a sequence that was never sent (ahead of everything in flight) or that is
//! older than everything in flight is ignored — it is never an error, because a stale ack from a
//! previous attach/run is expected after a webview reload.

use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex, MutexGuard},
};

/// Unacknowledged bytes at which the PTY reader pauses (SPEC: 1 MiB).
pub const DEFAULT_CREDIT_LIMIT: usize = 1024 * 1024;
/// Largest `Data` chunk.
pub const MAX_CHUNK: usize = 64 * 1024;

/// Is `a` at or before `b` in wrapping `u32` order?
pub fn seq_le(a: u32, b: u32) -> bool {
    (b.wrapping_sub(a) as i32) >= 0
}

#[derive(Default)]
struct Inner {
    in_flight: VecDeque<(u32, usize)>,
    bytes: usize,
    metrics: FlowMetrics,
    /// Set on cancel/exit: waiters never block again (the credit limit no longer applies).
    closed: bool,
}

/// Internal diagnostics; no terminal contents or generated command bindings.
#[derive(Default, Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowMetrics {
    pub peak_unacked_bytes: usize,
    pub paused_reads: usize,
    pub resumed_reads: usize,
    pub read_bytes: usize,
}

pub struct Flow {
    limit: usize,
    inner: Mutex<Inner>,
    credit: Condvar,
}

impl Flow {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            inner: Mutex::new(Inner::default()),
            credit: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // The state is a plain counter set; a panicked holder cannot leave it inconsistent.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A chunk was handed to the attached client.
    pub fn record(&self, seq: u32, bytes: usize) {
        let mut inner = self.lock();
        inner.in_flight.push_back((seq, bytes));
        inner.bytes = inner.bytes.saturating_add(bytes);
        inner.metrics.peak_unacked_bytes = inner.metrics.peak_unacked_bytes.max(inner.bytes);
    }

    /// Release every in-flight chunk with `sequence <= seq`. Returns the bytes released
    /// (0 for unknown, stale or ahead-of-everything sequences).
    pub fn ack(&self, seq: u32) -> usize {
        let mut inner = self.lock();
        let Some(&(newest, _)) = inner.in_flight.back() else {
            return 0;
        };
        if !seq_le(seq, newest) {
            return 0; // ahead of anything we ever sent
        }
        let mut released = 0usize;
        while let Some(&(front, bytes)) = inner.in_flight.front() {
            if !seq_le(front, seq) {
                break;
            }
            inner.in_flight.pop_front();
            released = released.saturating_add(bytes);
        }
        inner.bytes = inner.bytes.saturating_sub(released);
        drop(inner);
        if released > 0 {
            self.credit.notify_all();
        }
        released
    }

    /// New client attached: nothing it has not seen is "in flight" any more.
    pub fn reset(&self) {
        self.replace_snapshot(0, 0);
    }

    /// Atomically replace the old client's credit with replay, without a zero-credit gap.
    pub fn replace_snapshot(&self, seq: u32, bytes: usize) {
        let mut inner = self.lock();
        inner.in_flight.clear();
        inner.bytes = bytes;
        if bytes > 0 {
            inner.in_flight.push_back((seq, bytes));
        }
        inner.metrics.peak_unacked_bytes = inner.metrics.peak_unacked_bytes.max(bytes);
        drop(inner);
        self.credit.notify_all();
    }

    /// Permanently wake and release all waiters (cancel / child exit).
    pub fn close(&self) {
        self.lock().closed = true;
        self.credit.notify_all();
    }

    pub fn in_flight_bytes(&self) -> usize {
        self.lock().bytes
    }

    pub fn metrics(&self) -> FlowMetrics {
        self.lock().metrics
    }

    pub fn read(&self, bytes: usize) {
        let mut inner = self.lock();
        inner.metrics.read_bytes = inner.metrics.read_bytes.saturating_add(bytes);
    }

    /// Block (no polling) until the unacknowledged bytes drop below the limit or the flow is
    /// closed. Returns `false` when it returned because of `close`.
    pub fn wait_for_credit(&self) -> bool {
        let mut inner = self.lock();
        let paused = inner.bytes >= self.limit && !inner.closed;
        if paused {
            inner.metrics.paused_reads += 1;
        }
        while inner.bytes >= self.limit && !inner.closed {
            inner = self.credit.wait(inner).unwrap_or_else(|e| e.into_inner());
        }
        if paused && !inner.closed {
            inner.metrics.resumed_reads += 1;
        }
        !inner.closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, thread, time::Duration};

    #[test]
    fn ack_releases_everything_up_to_and_including_seq() {
        let flow = Flow::new(100);
        for seq in 1..=4 {
            flow.record(seq, 10);
        }
        assert_eq!(flow.in_flight_bytes(), 40);
        assert_eq!(flow.ack(2), 20);
        assert_eq!(flow.in_flight_bytes(), 20);
        assert_eq!(flow.ack(2), 0, "repeated ack releases nothing");
        assert_eq!(flow.ack(1), 0, "stale ack releases nothing");
        assert_eq!(flow.ack(4), 20);
        assert_eq!(flow.in_flight_bytes(), 0);
    }

    #[test]
    fn unknown_and_ahead_sequences_are_ignored_not_errors() {
        let flow = Flow::new(100);
        assert_eq!(flow.ack(7), 0, "nothing in flight");
        flow.record(5, 10);
        flow.record(6, 10);
        assert_eq!(flow.ack(7), 0, "ahead of everything sent");
        assert_eq!(flow.ack(u32::MAX), 0);
        assert_eq!(flow.ack(4), 0, "older than everything in flight");
        assert_eq!(flow.in_flight_bytes(), 20);
    }

    #[test]
    fn wrapping_sequences_compare_correctly() {
        assert!(seq_le(u32::MAX, 0));
        assert!(seq_le(u32::MAX - 1, 1));
        assert!(!seq_le(1, u32::MAX));
        assert!(seq_le(5, 5));
        let flow = Flow::new(100);
        for seq in [u32::MAX - 1, u32::MAX, 0, 1] {
            flow.record(seq, 10);
        }
        assert_eq!(flow.ack(u32::MAX), 20, "acks both pre-wrap chunks only");
        assert_eq!(flow.ack(0), 10);
        assert_eq!(flow.ack(1), 10);
        assert_eq!(flow.in_flight_bytes(), 0);
    }

    #[test]
    fn waiter_blocks_at_the_limit_and_resumes_after_ack() {
        let flow = Arc::new(Flow::new(30));
        flow.record(1, 20);
        assert!(flow.wait_for_credit(), "below the limit does not block");
        flow.record(2, 20);
        let waiter = {
            let flow = flow.clone();
            thread::spawn(move || flow.wait_for_credit())
        };
        thread::sleep(Duration::from_millis(80));
        assert!(
            !waiter.is_finished(),
            "must stall at the limit without acks"
        );
        flow.ack(1);
        assert!(waiter.join().unwrap(), "resumes with credit");
    }

    #[test]
    fn snapshot_credit_blocks_until_parsed_and_metrics_count_pause_resume() {
        let flow = Arc::new(Flow::new(30));
        flow.record(1, 29);
        flow.replace_snapshot(4, 40);
        assert_eq!(
            flow.in_flight_bytes(),
            40,
            "replay is in flight, not free credit"
        );
        let waiter = {
            let flow = flow.clone();
            thread::spawn(move || flow.wait_for_credit())
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while flow.metrics().paused_reads == 0 {
            assert!(std::time::Instant::now() < deadline);
            thread::yield_now();
        }
        assert!(!waiter.is_finished());
        assert_eq!(
            flow.ack(3),
            0,
            "old pre-snapshot ack does not grant replay credit"
        );
        assert_eq!(flow.ack(5), 0, "ahead ack does not grant replay credit");
        assert_eq!(flow.ack(4), 40);
        assert!(waiter.join().unwrap());
        flow.read(123);
        let metrics = flow.metrics();
        assert_eq!(metrics.peak_unacked_bytes, 40);
        assert_eq!(metrics.paused_reads, 1);
        assert_eq!(metrics.resumed_reads, 1);
        assert_eq!(metrics.read_bytes, 123);
    }

    #[test]
    fn reset_and_close_wake_the_waiter() {
        let flow = Arc::new(Flow::new(10));
        flow.record(1, 50);
        let waiter = {
            let flow = flow.clone();
            thread::spawn(move || flow.wait_for_credit())
        };
        thread::sleep(Duration::from_millis(50));
        assert!(!waiter.is_finished());
        flow.reset();
        assert!(waiter.join().unwrap());
        assert_eq!(flow.in_flight_bytes(), 0);

        flow.record(2, 50);
        let waiter = {
            let flow = flow.clone();
            thread::spawn(move || flow.wait_for_credit())
        };
        thread::sleep(Duration::from_millis(50));
        assert!(!waiter.is_finished());
        flow.close();
        assert!(!waiter.join().unwrap(), "close reports closed");
        assert!(!flow.wait_for_credit(), "stays closed");
    }
}
