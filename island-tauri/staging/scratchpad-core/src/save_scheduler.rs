/// Default debounce window after the last edit (ms).
pub const DEFAULT_DEBOUNCE_MS: u64 = 750;
/// Default maximum delay from the first dirty op (ms) — continuous typing
/// still saves at least every 5 s.
pub const DEFAULT_MAX_DELAY_MS: u64 = 5_000;
/// Initial failure backoff (ms), doubling per failure up to
/// [`MAX_BACKOFF_MS`].
const BASE_BACKOFF_MS: u64 = 1_000;
/// Failure backoff cap (ms).
const MAX_BACKOFF_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InFlight {
    generation: u64,
    version: u64,
}

/// Debounced save scheduler: decides *when* the dirty scratchpad should be
/// written. The Tauri actor polls [`SaveScheduler::next_save_at`] /
/// [`SaveScheduler::due`], calls [`SaveScheduler::begin_save`] when due, then
/// reports success via [`SaveScheduler::saved`] or failure via
/// [`SaveScheduler::save_failed`].
///
/// Semantics:
/// - [`SaveScheduler::mark_dirty`] pushes the save to `now + debounce`, but
///   never later than `first_dirty + max_delay` (continuous typing still
///   saves at least every `max_delay`).
/// - [`SaveScheduler::saved`] clears dirty only if no `mark_dirty` happened
///   after `begin_save` returned that generation (an edit during an in-flight
///   save keeps it dirty and reschedules).
/// - Failures keep dirty and retry with backoff 1 s, 2 s, 4 s … capped at
///   30 s; a successful save resets the backoff.
/// - [`SaveScheduler::flush_requested`] makes a dirty scheduler due
///   immediately (app quit).
#[derive(Debug, Clone)]
pub struct SaveScheduler {
    debounce_ms: u64,
    max_delay_ms: u64,
    dirty: bool,
    first_dirty: Option<u64>,
    last_mark: Option<u64>,
    dirty_version: u64,
    in_flight: Option<InFlight>,
    next_generation: u64,
    backoff_ms: u64,
    retry_at: Option<u64>,
    flush_pending: bool,
}

impl SaveScheduler {
    /// A scheduler with default `debounce_ms` (750) and `max_delay_ms`
    /// (5000).
    pub fn new() -> Self {
        Self {
            debounce_ms: DEFAULT_DEBOUNCE_MS,
            max_delay_ms: DEFAULT_MAX_DELAY_MS,
            dirty: false,
            first_dirty: None,
            last_mark: None,
            dirty_version: 0,
            in_flight: None,
            next_generation: 0,
            backoff_ms: 0,
            retry_at: None,
            flush_pending: false,
        }
    }

    /// Builder: override the debounce window (ms).
    pub fn with_debounce_ms(mut self, debounce_ms: u64) -> Self {
        self.debounce_ms = debounce_ms;
        self
    }

    /// Builder: override the maximum delay from first dirty (ms).
    pub fn with_max_delay_ms(mut self, max_delay_ms: u64) -> Self {
        self.max_delay_ms = max_delay_ms;
        self
    }

    /// The debounce window (ms).
    pub fn debounce_ms(&self) -> u64 {
        self.debounce_ms
    }

    /// The maximum delay from first dirty (ms).
    pub fn max_delay_ms(&self) -> u64 {
        self.max_delay_ms
    }

    /// Whether there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether a previous save failed and the scheduler is waiting out a
    /// backoff before retrying.
    pub fn in_backoff(&self) -> bool {
        self.retry_at.is_some()
    }

    /// Record that the state changed at `now`. Each call pushes the save to
    /// `now + debounce_ms`, but never later than the first dirty time plus
    /// `max_delay_ms`.
    pub fn mark_dirty(&mut self, now: u64) {
        self.dirty = true;
        if self.first_dirty.is_none() {
            self.first_dirty = Some(now);
        }
        self.last_mark = Some(now);
        self.dirty_version = self.dirty_version.wrapping_add(1);
    }

    /// The planned save time, if dirty. During failure backoff this is the
    /// retry deadline (which may exceed `first_dirty + max_delay`);
    /// otherwise it is `min(last_mark + debounce, first_dirty + max_delay)`.
    /// After [`SaveScheduler::flush_requested`] it is `0`.
    pub fn next_save_at(&self) -> Option<u64> {
        if !self.dirty {
            return None;
        }
        if self.flush_pending {
            return Some(0);
        }
        if let Some(retry) = self.retry_at {
            return Some(retry);
        }
        let last = self.last_mark?;
        let first = self.first_dirty?;
        Some(
            last.saturating_add(self.debounce_ms)
                .min(first.saturating_add(self.max_delay_ms)),
        )
    }

    /// Whether a save is due at `now`.
    pub fn due(&self, now: u64) -> bool {
        matches!(self.next_save_at(), Some(deadline) if now >= deadline)
    }

    /// Begin a save: returns the generation being saved, or `None` if not
    /// due. Only one save is tracked as in-flight; starting a new one while
    /// another is in flight supersedes it (the old generation goes stale).
    pub fn begin_save(&mut self, now: u64) -> Option<u64> {
        if !self.due(now) {
            return None;
        }
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        self.in_flight = Some(InFlight {
            generation,
            version: self.dirty_version,
        });
        Some(generation)
    }

    /// Report that the save of `generation` succeeded at `now`. Clears dirty
    /// only if no `mark_dirty` happened after that `begin_save`. A stale
    /// generation (not the current in-flight one) is ignored. Success resets
    /// the failure backoff.
    pub fn saved(&mut self, _now: u64, generation: u64) {
        let Some(in_flight) = self.in_flight else {
            return;
        };
        if in_flight.generation != generation {
            return;
        }
        self.in_flight = None;
        self.backoff_ms = 0;
        self.retry_at = None;
        if in_flight.version == self.dirty_version {
            self.dirty = false;
            self.first_dirty = None;
            self.last_mark = None;
            self.flush_pending = false;
        }
    }

    /// Report that the in-flight save failed at `now`: stays dirty and
    /// schedules a retry `now + backoff` where the backoff doubles per
    /// consecutive failure (1 s, 2 s, 4 s …) capped at 30 s.
    pub fn save_failed(&mut self, now: u64) {
        self.in_flight = None;
        self.flush_pending = false;
        let next_backoff = if self.backoff_ms == 0 {
            BASE_BACKOFF_MS
        } else {
            self.backoff_ms.saturating_mul(2).min(MAX_BACKOFF_MS)
        };
        self.backoff_ms = next_backoff;
        self.retry_at = Some(now.saturating_add(next_backoff));
    }

    /// Make a dirty scheduler due immediately (app quit). No-op when clean.
    pub fn flush_requested(&mut self) {
        if self.dirty {
            self.flush_pending = true;
        }
    }
}

impl Default for SaveScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_scheduler_has_no_next_save_at() {
        let s = SaveScheduler::new();
        assert!(!s.is_dirty());
        assert_eq!(s.next_save_at(), None);
        assert!(!s.due(u64::MAX));
        assert!(!s.in_backoff());
    }

    #[test]
    fn debounce_uses_last_mark_time() {
        let mut s = SaveScheduler::new();
        s.mark_dirty(1000);
        assert_eq!(s.next_save_at(), Some(1750)); // 1000 + 750
        s.mark_dirty(2000);
        assert_eq!(s.next_save_at(), Some(2750)); // pushed by last mark
    }

    #[test]
    fn max_delay_cap_holds_under_continuous_edits() {
        let mut s = SaveScheduler::new();
        let t0 = 1_000;
        s.mark_dirty(t0);
        let mut now = t0;
        while now < t0 + 6_000 {
            now += 100;
            s.mark_dirty(now);
            let next = s.next_save_at().unwrap();
            assert!(
                next <= t0 + s.max_delay_ms(),
                "cap violated: next={next} > {}",
                t0 + s.max_delay_ms()
            );
        }
        // once past the cap window, every mark pins the save at the cap
        assert_eq!(s.next_save_at().unwrap(), t0 + s.max_delay_ms());
    }

    #[test]
    fn first_dirty_anchors_the_max_delay() {
        let mut s = SaveScheduler::new();
        s.mark_dirty(0);
        s.mark_dirty(6000);
        // min(6750, 5000) = 5000
        assert_eq!(s.next_save_at(), Some(5000));
    }

    #[test]
    fn due_respects_deadline() {
        let mut s = SaveScheduler::new();
        s.mark_dirty(1000);
        assert!(!s.due(1749));
        assert!(s.due(1750));
        assert!(s.due(u64::MAX));
    }

    #[test]
    fn begin_save_requires_due_and_increments_generation() {
        let mut s = SaveScheduler::new();
        assert_eq!(s.begin_save(1000), None); // clean
        s.mark_dirty(1000);
        assert_eq!(s.begin_save(1000), None); // not yet due (1750)
        let g1 = s.begin_save(1750).unwrap();
        let g2 = s.begin_save(1750).unwrap();
        assert_ne!(g1, g2);
    }

    #[test]
    fn saved_clears_dirty_when_no_newer_mark() {
        let mut s = SaveScheduler::new();
        s.mark_dirty(1000);
        let g = s.begin_save(1750).unwrap();
        assert!(s.is_dirty());
        s.saved(1800, g);
        assert!(!s.is_dirty());
        assert_eq!(s.next_save_at(), None);
        assert!(!s.in_backoff());
    }

    #[test]
    fn edit_during_inflight_save_keeps_dirty() {
        let mut s = SaveScheduler::new();
        s.mark_dirty(1000);
        let g = s.begin_save(2000).unwrap();
        s.mark_dirty(2100); // edit during in-flight save
        s.saved(2200, g);
        assert!(s.is_dirty(), "edit during in-flight save must keep dirty");
        // rescheduled from the newest mark: 2100 + 750 = 2850
        assert_eq!(s.next_save_at(), Some(2850));
        let g2 = s.begin_save(2850).unwrap();
        assert_ne!(g2, g);
        s.saved(2850, g2);
        assert!(!s.is_dirty());
    }

    #[test]
    fn stale_generation_ignored() {
        let mut s = SaveScheduler::new();
        s.mark_dirty(1000);
        let g = s.begin_save(2000).unwrap();
        s.saved(2500, g + 1); // stale generation
        assert!(s.is_dirty(), "stale generation must not clear dirty");
        // superseded in-flight: a second begin replaces the first
        let g2 = s.begin_save(3000).unwrap();
        s.saved(3000, g); // old generation, now stale
        assert!(s.is_dirty());
        s.saved(3000, g2);
        assert!(!s.is_dirty());
    }

    #[test]
    fn saved_without_begin_is_noop() {
        let mut s = SaveScheduler::new();
        s.mark_dirty(1000);
        s.saved(2000, 7);
        assert!(s.is_dirty());
    }

    #[test]
    fn backoff_sequence_cap_and_reset() {
        let mut s = SaveScheduler::new();
        s.mark_dirty(1000);
        assert!(!s.in_backoff());

        s.save_failed(1000);
        assert!(s.in_backoff());
        assert_eq!(s.next_save_at(), Some(2000)); // +1 s

        s.save_failed(2000);
        assert_eq!(s.next_save_at(), Some(4000)); // +2 s

        s.save_failed(4000);
        assert_eq!(s.next_save_at(), Some(8000)); // +4 s

        s.save_failed(8000);
        assert_eq!(s.next_save_at(), Some(16_000)); // +8 s

        s.save_failed(16_000);
        assert_eq!(s.next_save_at(), Some(32_000)); // +16 s

        s.save_failed(32_000);
        assert_eq!(s.next_save_at(), Some(62_000)); // +30 s (capped)

        s.save_failed(62_000);
        assert_eq!(s.next_save_at(), Some(92_000)); // +30 s again

        // success resets the backoff
        let g = s.begin_save(92_000).unwrap();
        s.saved(92_000, g);
        assert!(!s.in_backoff());
        assert!(!s.is_dirty());

        // the next failure starts again at 1 s
        s.mark_dirty(100_000);
        s.save_failed(100_000);
        assert_eq!(s.next_save_at(), Some(101_000));
    }

    #[test]
    fn mark_dirty_during_backoff_keeps_retry_deadline() {
        let mut s = SaveScheduler::new();
        s.mark_dirty(1000);
        s.save_failed(1000);
        s.mark_dirty(1500);
        assert_eq!(s.next_save_at(), Some(2000));
    }

    #[test]
    fn flush_makes_dirty_due_immediately_and_clears_on_success() {
        let mut s = SaveScheduler::new();
        s.flush_requested(); // clean: no-op
        assert_eq!(s.next_save_at(), None);

        s.mark_dirty(5000);
        s.flush_requested();
        assert_eq!(s.next_save_at(), Some(0));
        assert!(s.due(5000));

        let g = s.begin_save(5000).unwrap();
        s.saved(5000, g);
        assert!(!s.is_dirty());
        assert_eq!(s.next_save_at(), None);
    }

    #[test]
    fn flush_failure_falls_back_to_backoff() {
        let mut s = SaveScheduler::new();
        s.mark_dirty(1000);
        s.flush_requested();
        let g = s.begin_save(1000).unwrap();
        s.save_failed(1000);
        assert!(s.is_dirty());
        assert!(s.in_backoff());
        assert_eq!(s.next_save_at(), Some(2000));
        let _ = g;
    }
}
