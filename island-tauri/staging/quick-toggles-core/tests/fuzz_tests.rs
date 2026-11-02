use quick_toggles_core::{
    next_profile, parse_mic_mute, power_tile, PpdHold, PpdProfile, PpdSnapshot, Profile,
    ToggleState, PENDING_TIMEOUT_MS,
};

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

const TEXT_POOL: &[char] = &[
    'a', 'b', 'M', 'u', 't', 'e', ':', ' ', 'y', 's', 'n', 'o', '\n', '\r', '\0', '-', '_', '1',
    '2', 'Z', '\t', '@', '#',
];

fn random_string(lcg: &mut Lcg, max: u64) -> String {
    let len = lcg.below(max);
    let mut out = String::new();
    for _ in 0..len {
        out.push(lcg.pick(TEXT_POOL));
    }
    out
}

fn random_profile_name(lcg: &mut Lcg) -> String {
    match lcg.below(6) {
        0 => "power-saver".to_string(),
        1 => "balanced".to_string(),
        2 => "performance".to_string(),
        3 => "turbo".to_string(),
        4 => String::new(),
        _ => random_string(lcg, 12),
    }
}

fn random_snapshot(lcg: &mut Lcg) -> PpdSnapshot {
    let active = random_profile_name(lcg);
    let nprofiles = lcg.below(6);
    let mut profiles = Vec::new();
    for _ in 0..nprofiles {
        profiles.push(PpdProfile {
            profile: random_profile_name(lcg),
            driver: if lcg.coin() {
                Some(random_string(lcg, 10))
            } else {
                None
            },
        });
    }
    let degraded = if lcg.below(3) == 0 {
        String::new()
    } else {
        random_string(lcg, 24)
    };
    let nholds = lcg.below(4);
    let mut holds = Vec::new();
    for _ in 0..nholds {
        holds.push(PpdHold {
            profile: random_profile_name(lcg),
            reason: random_string(lcg, 20),
            application_id: random_string(lcg, 20),
        });
    }
    PpdSnapshot {
        active,
        profiles,
        degraded,
        holds,
    }
}

#[test]
fn fuzz_quick_toggles_never_panics_and_keeps_invariants() {
    let mut lcg = Lcg(0x9E3779B97F4A7C15);

    for _ in 0..2000 {
        // ---- power tile ----
        let snap = random_snapshot(&mut lcg);
        let tile = power_tile(Some(&snap));
        assert!(tile.available);
        assert_eq!(tile.unavailable_reason, None);
        assert!(tile.choices.len() <= 3);
        for c in &tile.choices {
            assert!(matches!(
                c,
                Profile::PowerSaver | Profile::Balanced | Profile::Performance
            ));
        }
        let mut dedup = tile.choices.clone();
        dedup.sort_by_key(|p| *p as u8);
        dedup.dedup();
        assert_eq!(dedup.len(), tile.choices.len());
        if let Some(d) = &tile.degraded {
            assert!(!d.is_empty());
        }
        let _ = next_profile(&tile);
        let none_tile = power_tile(None);
        assert!(!none_tile.available);
        assert_eq!(
            none_tile.unavailable_reason.as_deref(),
            Some("power-profiles-daemon not running")
        );

        // ---- mic parse ----
        let stdout = random_string(&mut lcg, 300);
        let parsed = parse_mic_mute(&stdout);
        let _ = parsed;

        // ---- toggle state ops ----
        let mut ts = ToggleState::new();
        let mut shadow_pending: Option<(bool, u64, u64)> = None; // (value, id, sent)
        let mut shadow_confirmed: Option<bool> = None;
        for _ in 0..30 {
            match lcg.below(5) {
                0 => {
                    let v = lcg.coin();
                    let now = lcg.next();
                    let id = ts.request(v, now);
                    shadow_pending = Some((v, id, now));
                }
                1 => {
                    let v = lcg.coin();
                    let now = lcg.next();
                    ts.confirm(v, now);
                    if let Some((pv, _, _)) = shadow_pending {
                        if pv == v {
                            shadow_pending = None;
                        }
                    }
                    shadow_confirmed = Some(v);
                }
                2 => {
                    let now = lcg.next();
                    let id = match shadow_pending {
                        Some((_, pid, _)) if lcg.coin() => pid,
                        _ => lcg.next(),
                    };
                    ts.fail(id, now);
                    if let Some((_, pid, _)) = shadow_pending {
                        if pid == id {
                            shadow_pending = None;
                        }
                    }
                }
                3 => {
                    let now = lcg.next();
                    ts.tick(now);
                    if let Some((_, _, sent)) = shadow_pending {
                        if now.saturating_sub(sent) >= PENDING_TIMEOUT_MS {
                            shadow_pending = None;
                        }
                    }
                }
                _ => {
                    let expected = match shadow_pending {
                        Some((v, _, _)) => Some(v),
                        None => shadow_confirmed,
                    };
                    assert_eq!(ts.display(), expected);
                }
            }
        }
    }
}
