use brightness_core::{BrightnessModel, Event, CONTEXT, SUPPRESS_INDICATOR};
use std::collections::{HashMap, HashSet};

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

fn random_name(lcg: &mut Lcg) -> String {
    let valid = ['a', 'b', 'Z', '0', '5', '_'];
    let invalid = ['-', '/', ' ', '.', 'é', '@', '\0', '\t'];
    let len = lcg.below(300);
    let mut s = String::new();
    for _ in 0..len {
        let c = if lcg.coin() {
            lcg.pick(&valid)
        } else {
            lcg.pick(&invalid)
        };
        s.push(c);
    }
    s
}

fn random_i32(lcg: &mut Lcg) -> i32 {
    lcg.next() as i32
}

#[test]
fn fuzz_brightness_model_never_panics_and_keeps_invariants() {
    let mut lcg = Lcg(0xF00D_BA5E_CAFE_0001);
    let mut model = BrightnessModel::new();
    let mut now = 0u64;
    let mut last_poll_send: HashMap<String, u64> = HashMap::new();
    // Names whose drag session just started/ended: the next poll send for
    // them is the first of a fresh session and is exempt from the throttle
    // (release flush is also immediate by design).
    let mut fresh_sessions: HashSet<String> = HashSet::new();

    for _ in 0..8000 {
        now = now.wrapping_add(lcg.below(200));

        match lcg.below(6) {
            0 => {
                let n = lcg.below(24);
                let names: Vec<String> = (0..n).map(|_| random_name(&mut lcg)).collect();
                let _ = model.apply(Event::DisplayNames(names));
            }
            1 => {
                let _ = model.apply(Event::DisplayAdded(random_name(&mut lcg)));
            }
            2 => {
                let _ = model.apply(Event::DisplayRemoved(random_name(&mut lcg)));
            }
            3 => {
                let _ = model.apply(Event::Props {
                    name: random_name(&mut lcg),
                    label: random_name(&mut lcg),
                    is_internal: lcg.coin(),
                    max_brightness: random_i32(&mut lcg),
                    brightness: random_i32(&mut lcg),
                });
            }
            4 => {
                let _ = model.apply(Event::BrightnessChanged {
                    name: random_name(&mut lcg),
                    brightness: random_i32(&mut lcg),
                    source_client: random_name(&mut lcg),
                    context: random_name(&mut lcg),
                });
            }
            _ => {
                let _ = model.apply(Event::RangeChanged {
                    name: random_name(&mut lcg),
                    max_brightness: random_i32(&mut lcg),
                    brightness: random_i32(&mut lcg),
                });
            }
        }

        if lcg.coin() {
            let name = random_name(&mut lcg);
            let dragging_before = model
                .displays()
                .iter()
                .any(|v| v.name == name && v.dragging);
            let pct = lcg.below(256) as u8;
            let _ = model.drag(&name, pct, now);
            let dragging_after = model
                .displays()
                .iter()
                .any(|v| v.name == name && v.dragging);
            if !dragging_before && dragging_after {
                fresh_sessions.insert(name);
            }
        }

        if lcg.coin() {
            for req in model.poll(now) {
                assert_eq!(req.flags, SUPPRESS_INDICATOR);
                assert_eq!(req.context, CONTEXT);
                assert_eq!(req.path, format!("/org/kde/ScreenBrightness/{}", req.name));
                let max = model.max_brightness(&req.name).unwrap();
                assert!(
                    req.brightness >= 0 && req.brightness <= max,
                    "brightness {} out of 0..={}",
                    req.brightness,
                    max
                );
                if fresh_sessions.remove(&req.name) {
                    last_poll_send.remove(&req.name);
                } else if let Some(prev) = last_poll_send.get(&req.name) {
                    assert!(
                        now.saturating_sub(*prev) >= 50,
                        "throttle violated for {}",
                        req.name
                    );
                }
                last_poll_send.insert(req.name.clone(), now);
            }
        }

        if lcg.coin() {
            let name = random_name(&mut lcg);
            let dragging_before = model
                .displays()
                .iter()
                .any(|v| v.name == name && v.dragging);
            let _ = model.release(&name, now);
            let dragging_after = model
                .displays()
                .iter()
                .any(|v| v.name == name && v.dragging);
            if dragging_before && !dragging_after {
                fresh_sessions.insert(name);
            }
        }

        for v in model.displays() {
            if let Some(p) = v.percent {
                assert!(p <= 100, "percent {p} > 100");
            }
        }

        if let Some(w) = model.next_wakeup(now) {
            assert!(w >= now, "wakeup {w} < now {now}");
        }
    }
}
