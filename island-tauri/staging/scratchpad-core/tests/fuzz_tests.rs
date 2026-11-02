use scratchpad_core::{SaveScheduler, Scratchpad};

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

    fn pick(&mut self, chars: &[char]) -> char {
        chars[self.below(chars.len() as u64) as usize]
    }
}

fn random_id(lcg: &mut Lcg) -> String {
    let good = ['a', 'b', 'c', '0', '1', '-', '_', 'Z'];
    let bad = [' ', '.', '/', 'é', '#', '\n'];
    let len = lcg.below(70);
    let mut s = String::new();
    for _ in 0..len {
        let c = if lcg.coin() {
            lcg.pick(&good)
        } else {
            lcg.pick(&bad)
        };
        s.push(c);
    }
    s
}

fn random_body(lcg: &mut Lcg) -> String {
    if lcg.below(25) == 0 {
        let n = 256 * 1024 + 1 + lcg.below(5000) as usize;
        return "z".repeat(n);
    }
    let len = lcg.below(200);
    let mut s = String::new();
    for _ in 0..len {
        let c = match lcg.below(5) {
            0 => 'a',
            1 => 'é',
            2 => ' ',
            3 => '\n',
            _ => '中',
        };
        s.push(c);
    }
    s
}

fn random_json(lcg: &mut Lcg) -> String {
    if lcg.coin() {
        let len = lcg.below(60);
        let mut s = String::new();
        for _ in 0..len {
            let c = match lcg.below(6) {
                0 => '{',
                1 => '}',
                2 => '[',
                3 => ']',
                4 => '"',
                _ => 'x',
            };
            s.push(c);
        }
        return s;
    }
    let mut notes = String::from("[");
    let nn = lcg.below(5);
    for i in 0..nn {
        let body = random_body(lcg).replace('"', "'");
        notes.push_str(&format!(
            r#"{{"id":"n{i}","body":"{body}","createdMs":{},"updatedMs":{},"pinned":{}}}"#,
            lcg.below(10_000),
            lcg.below(10_000),
            lcg.coin()
        ));
        if i + 1 < nn {
            notes.push(',');
        }
    }
    notes.push(']');
    format!(r#"{{"version":1,"notes":{notes}}}"#)
}

fn random_existing_id(lcg: &mut Lcg, pad: &Scratchpad) -> String {
    let list = pad.list();
    if !list.is_empty() && lcg.coin() {
        let idx = lcg.below(list.len() as u64) as usize;
        list[idx].id.clone()
    } else {
        random_id(lcg)
    }
}

fn assert_unique_ids(pad: &Scratchpad) {
    let mut seen = std::collections::HashSet::new();
    for s in pad.list() {
        assert!(seen.insert(s.id.clone()), "duplicate id {}", s.id);
    }
}

fn assert_list_invariants(pad: &Scratchpad) {
    let list = pad.list();
    for w in list.windows(2) {
        let a = &w[0];
        let b = &w[1];
        if a.pinned != b.pinned {
            assert!(a.pinned, "unpinned before pinned: {} before {}", a.id, b.id);
        } else if a.updated_ms != b.updated_ms {
            assert!(
                a.updated_ms > b.updated_ms,
                "updated_ms not descending: {} ({}) before {} ({})",
                a.id,
                a.updated_ms,
                b.id,
                b.updated_ms
            );
        } else {
            assert!(
                a.id <= b.id,
                "tie not id-ascending: {} before {}",
                a.id,
                b.id
            );
        }
    }
}

#[test]
fn fuzz_scratchpad_never_panics_and_keeps_invariants() {
    let mut lcg = Lcg(0xC0FF_EE42_0000_0001);
    let mut pad = Scratchpad::new();
    let mut now = 0u64;

    for _ in 0..4000 {
        now = now.wrapping_add(lcg.below(50));
        match lcg.below(6) {
            0 => {
                let _ = pad.new_note(now, random_id(&mut lcg));
            }
            1 => {
                let id = random_existing_id(&mut lcg, &pad);
                let _ = pad.edit(now, &id, random_body(&mut lcg));
            }
            2 => {
                let id = random_existing_id(&mut lcg, &pad);
                let _ = pad.delete(now, &id);
            }
            3 => {
                let id = random_existing_id(&mut lcg, &pad);
                let _ = pad.set_pinned(now, &id, lcg.coin());
            }
            4 => {
                let json = random_json(&mut lcg);
                if let Ok((loaded, report)) = Scratchpad::from_json(&json) {
                    assert!(loaded.len() <= 200, "loaded len {} > 200", loaded.len());
                    assert!(report.skipped <= 10_000_000);
                    assert_unique_ids(&loaded);
                    assert_list_invariants(&loaded);
                }
            }
            _ => {
                let _ = pad.list();
            }
        }
        assert!(pad.len() <= 200, "len {} > 200", pad.len());
        assert_unique_ids(&pad);
        assert_list_invariants(&pad);
    }
}

#[test]
fn fuzz_scheduler_never_panics_and_keeps_invariants() {
    let mut lcg = Lcg(0x5EED_BEEF_CAFE_F00D);
    let mut sched = SaveScheduler::new();
    let mut epoch_start: Option<u64> = None;
    let mut in_flight: Option<u64> = None;
    let mut now = 0u64;

    for _ in 0..5000 {
        now = now.wrapping_add(lcg.below(200));
        match lcg.below(7) {
            0 => {
                sched.mark_dirty(now);
                if epoch_start.is_none() {
                    epoch_start = Some(now);
                }
            }
            1 => {
                if let Some(g) = sched.begin_save(now) {
                    in_flight = Some(g);
                }
            }
            2 => match in_flight.take() {
                Some(g) => {
                    if lcg.coin() {
                        sched.saved(now, g);
                    } else {
                        sched.save_failed(now);
                    }
                }
                None => {
                    sched.saved(now, lcg.below(1000));
                }
            },
            3 => sched.flush_requested(),
            4 => {
                let _ = sched.due(now);
            }
            5 => {
                let _ = sched.next_save_at();
            }
            _ => {
                let _ = sched.in_backoff();
            }
        }

        match sched.next_save_at() {
            None => assert!(!sched.is_dirty(), "dirty but no next_save_at"),
            Some(deadline) => {
                assert!(sched.is_dirty());
                if !sched.in_backoff() {
                    let cap = epoch_start
                        .unwrap_or(0)
                        .saturating_add(sched.max_delay_ms());
                    assert!(
                        deadline <= cap,
                        "next_save_at {deadline} > first_dirty + max_delay {cap}"
                    );
                }
                assert_eq!(sched.due(now), now >= deadline);
            }
        }
        if !sched.is_dirty() {
            epoch_start = None;
        }
    }
}
