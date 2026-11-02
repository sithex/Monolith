use timer_core::{Phase, PomodoroConfig, TimerCore, MAX_COUNTDOWN_MS, MAX_EVENTS_PER_TICK};

/// Hand-rolled LCG (MMIX constants), fixed seed, no external deps.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }

    fn coin(&mut self) -> bool {
        self.below(2) == 0
    }
}

#[test]
fn fuzz_invariants() {
    let mut rng = Lcg(0x243F_6A88_85A3_08D3);

    for _ in 0..400 {
        let rounds = 1 + rng.below(12);
        let auto = rng.coin();
        let cfg = PomodoroConfig::new(60_000, 60_000, 60_000, rounds, auto).unwrap();
        let focus = cfg.focus_ms();
        let short = cfg.short_break_ms();
        let long = cfg.long_break_ms();

        let mut core = TimerCore::new(cfg);
        // last_seen: the core's clamped clock (only mutating ops advance it).
        let mut last_seen: u64 = 0;
        let mut prev_elapsed: u64 = 0;
        let mut observed = false;

        let steps = rng.below(250);
        for _step in 0..steps {
            let base = last_seen.saturating_add(rng.below(60_001));
            let arg_now = if rng.below(40) == 0 {
                u64::MAX
            } else if rng.coin() {
                base.saturating_sub(rng.below(base.saturating_add(1)))
            } else {
                base
            };
            let eff = arg_now.max(last_seen);

            let op = rng.below(17);
            let mut reset_sw = false;
            match op {
                // set_countdown: no now, does not advance last_seen
                0 => {
                    let d = [0, 999, 1_000, 60_000, 86_400_000, 86_400_001, u64::MAX]
                        [rng.below(7) as usize];
                    let _ = core.set_countdown(d);
                }
                1 => {
                    let _ = core.start(eff);
                    last_seen = eff;
                }
                2 => {
                    let _ = core.pause(eff);
                    last_seen = eff;
                }
                3 => {
                    let _ = core.resume(eff);
                    last_seen = eff;
                }
                // reset: no now
                4 => core.reset(),
                5 => {
                    let _ = core.add_time(eff, rng.below(200_000));
                    last_seen = eff;
                }
                6 => {
                    let _ = core.pomo_start(eff);
                    last_seen = eff;
                }
                7 => {
                    let _ = core.pomo_pause(eff);
                    last_seen = eff;
                }
                8 => {
                    let _ = core.pomo_resume(eff);
                    last_seen = eff;
                }
                // pomo_reset: no now
                9 => core.pomo_reset(),
                10 => {
                    let _ = core.pomo_skip(eff);
                    last_seen = eff;
                }
                11 => {
                    let _ = core.sw_start(eff);
                    last_seen = eff;
                }
                12 => {
                    let _ = core.sw_pause(eff);
                    last_seen = eff;
                }
                13 => {
                    let _ = core.sw_resume(eff);
                    last_seen = eff;
                }
                // sw_reset: no now
                14 => {
                    core.sw_reset();
                    reset_sw = true;
                }
                15 => {
                    let _ = core.sw_lap(eff);
                    last_seen = eff;
                }
                // snapshot: pure read, does not advance last_seen
                _ => {
                    let _ = core.snapshot(eff);
                }
            }

            // the actor's cadence: every step ends with a tick at eff
            let events = core.tick(eff);

            // next_deadline is always >= the last processed tick now (eff),
            // unless the 100-event-per-tick cap spilled (leftover phases).
            if events.len() < MAX_EVENTS_PER_TICK {
                if let Some(d) = core.next_deadline() {
                    assert!(
                        d >= eff,
                        "next_deadline {d} < last processed tick now {eff}"
                    );
                }
            }

            let snap = core.snapshot(eff);

            // remaining never exceeds the countdown cap (24 h)
            assert!(
                snap.countdown().remaining_ms() <= MAX_COUNTDOWN_MS,
                "countdown remaining {} > 24 h",
                snap.countdown().remaining_ms()
            );

            // remaining never exceeds the current pomodoro phase duration
            let pdur = match snap.pomodoro().phase() {
                Phase::Focus => focus,
                Phase::ShortBreak => short,
                Phase::LongBreak => long,
            };
            assert!(
                snap.pomodoro().remaining_ms() <= pdur,
                "pomodoro remaining {} > phase duration {}",
                snap.pomodoro().remaining_ms(),
                pdur
            );

            // stopwatch elapsed never decreases except across a reset
            let elapsed = snap.stopwatch().elapsed_ms();
            if reset_sw {
                prev_elapsed = elapsed;
                observed = true;
            } else if observed {
                assert!(
                    elapsed >= prev_elapsed,
                    "elapsed decreased {prev_elapsed} -> {elapsed}"
                );
                prev_elapsed = elapsed;
            } else {
                prev_elapsed = elapsed;
                observed = true;
            }
        }
    }
}
