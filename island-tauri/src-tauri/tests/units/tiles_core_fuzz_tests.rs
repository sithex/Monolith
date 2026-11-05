use hermes_island_lib::integrations::tiles::core::{
    argb_to_hex, argb_to_rgba_css, network_tile_report, output_tile, parse_current,
    parse_default_sink, parse_sinks_report, parse_tailscale_status_report, parse_voice_list_report,
    screenshot_argv, set_default_argv, switch_argv, Action, KeepAwake, KeepAwakeState, NmSnapshot,
    VoiceList,
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

fn random_text(lcg: &mut Lcg) -> String {
    let len = lcg.below(400);
    let mut s = String::new();
    for _ in 0..len {
        let c = match lcg.below(16) {
            0 => '{',
            1 => '}',
            2 => '[',
            3 => ']',
            4 => '"',
            5 => ' ',
            6 => ',',
            7 => ':',
            8 => '\\',
            9 => '\n',
            10 => '\t',
            11 => '\r',
            12 => '\0',
            13 => 'x',
            14 => 'é',
            _ => '1',
        };
        s.push(c);
    }
    s
}

fn random_sink_name(lcg: &mut Lcg) -> String {
    let good = ['a', 'b', '0', '-', '_', '.'];
    let bad = [' ', '\t', '\n', '\0', '-', '\u{1}'];
    let len = lcg.below(280);
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

fn random_voice_list(lcg: &mut Lcg) -> VoiceList {
    let n = lcg.below(5);
    let mut voices = Vec::new();
    for _ in 0..n {
        voices.push(random_sink_name(lcg));
    }
    let active = if voices.is_empty() {
        None
    } else {
        Some(voices[lcg.below(voices.len() as u64) as usize].clone())
    };
    VoiceList { voices, active }
}

fn random_snapshot(lcg: &mut Lcg) -> NmSnapshot {
    NmSnapshot {
        state: if lcg.coin() {
            Some(lcg.below(200) as u32)
        } else {
            None
        },
        connectivity: if lcg.coin() {
            Some(lcg.below(20) as u32)
        } else {
            None
        },
        primary_connection_type: if lcg.coin() {
            Some(random_text(lcg))
        } else {
            None
        },
        wireless_enabled: if lcg.coin() { Some(lcg.coin()) } else { None },
        wireless_hardware_enabled: if lcg.coin() { Some(lcg.coin()) } else { None },
        has_wireless_device: lcg.coin(),
    }
}

/// Drive one random keep-awake operation and check the machine invariants.
fn drive_machine(lcg: &mut Lcg, ka: &mut KeepAwake) {
    let op = lcg.below(8);
    match op {
        0 => {
            let before = ka.state();
            let now = lcg.next();
            let action = ka.turn_on(now, random_text(lcg));
            if let Some(Action::Inhibit { app, .. }) = &action {
                assert_eq!(app, "Hermes Island");
                assert_eq!(
                    ka.state(),
                    KeepAwakeState::Requesting {
                        cancel_pending: false,
                        since_ms: now
                    }
                );
            } else {
                assert_ne!(before, KeepAwakeState::Off);
            }
        }
        1 => {
            let cookie = lcg.next() as u32;
            let before = ka.state();
            let action = ka.inhibited(cookie);
            match action {
                Some(Action::UnInhibit { cookie: c }) => {
                    assert_eq!(c, cookie);
                    match before {
                        KeepAwakeState::Requesting {
                            cancel_pending: true,
                            ..
                        }
                        | KeepAwakeState::Off => assert_eq!(ka.state(), KeepAwakeState::Off),
                        KeepAwakeState::On { .. } => assert_eq!(ka.state(), before),
                        KeepAwakeState::Requesting {
                            cancel_pending: false,
                            ..
                        } => {
                            panic!("clean requesting never releases");
                        }
                    }
                }
                None => {
                    assert!(matches!(
                        before,
                        KeepAwakeState::Requesting {
                            cancel_pending: false,
                            ..
                        }
                    ));
                    assert!(matches!(
                        ka.state(),
                        KeepAwakeState::On {
                            cookie: c,
                            ..
                        } if c == cookie
                    ));
                }
                Some(Action::Inhibit { .. }) => panic!("inhibited never inhibits"),
            }
        }
        2 => {
            ka.failed(random_text(lcg));
        }
        3 => {
            let before = ka.state();
            let action = ka.turn_off();
            match action {
                Some(Action::UnInhibit { cookie }) => {
                    assert!(matches!(
                        before,
                        KeepAwakeState::On {
                            cookie: c,
                            ..
                        } if c == cookie
                    ));
                    assert_eq!(ka.state(), KeepAwakeState::Off);
                    assert_eq!(ka.auto_off_at_ms(), None);
                }
                None => {
                    if let KeepAwakeState::Requesting {
                        cancel_pending: false,
                        since_ms,
                    } = before
                    {
                        assert_eq!(
                            ka.state(),
                            KeepAwakeState::Requesting {
                                cancel_pending: true,
                                since_ms
                            }
                        );
                    }
                }
                Some(Action::Inhibit { .. }) => panic!("turn_off never inhibits"),
            }
        }
        4 => {
            let now = lcg.next();
            let before = ka.state();
            let action = ka.tick(now);
            if let Some(Action::UnInhibit { cookie }) = action {
                assert!(matches!(
                    before,
                    KeepAwakeState::On {
                        cookie: c,
                        ..
                    } if c == cookie
                ));
                assert_eq!(ka.state(), KeepAwakeState::Off);
                assert_eq!(ka.auto_off_at_ms(), None);
            }
        }
        5 => {
            ka.on_bus_lost();
            assert_eq!(ka.state(), KeepAwakeState::Off);
            assert_eq!(ka.auto_off_at_ms(), None);
        }
        6 => {
            let d = if lcg.coin() {
                Some(lcg.below(1_000_000))
            } else {
                None
            };
            let now = lcg.next();
            ka.set_auto_off(d, now);
            assert_eq!(ka.auto_off_at_ms(), d.map(|d| now.saturating_add(d)));
        }
        _ => {
            let at = if lcg.coin() { Some(lcg.next()) } else { None };
            ka.set_auto_off_at_ms(at);
            assert_eq!(ka.auto_off_at_ms(), at);
        }
    }
}

#[test]
fn fuzz_system_tiles_never_panics_and_keeps_invariants() {
    let mut lcg = Lcg(0xC0FF_EE42_0000_0002);
    let mut ka = KeepAwake::new();

    for _ in 0..3000 {
        let txt = random_text(&mut lcg);

        let _ = parse_sinks_report(&txt);
        let _ = parse_default_sink(&txt);
        let _ = parse_tailscale_status_report(&txt);
        let _ = parse_voice_list_report(&txt);
        let _ = parse_current(&txt);

        let name = random_sink_name(&mut lcg);
        if let Ok(argv) = set_default_argv(&name) {
            assert_eq!(argv.len(), 3);
            assert_eq!(argv[0], "pactl");
            assert_eq!(argv[1], "set-default-sink");
            assert_eq!(argv[2], name);
        }

        let list = random_voice_list(&mut lcg);
        let vname = random_sink_name(&mut lcg);
        if let Ok(argv) = switch_argv(&list, &vname) {
            assert_eq!(argv.len(), 2);
            assert_eq!(argv[0], "hermes-fish");
            assert_eq!(argv[1], vname);
            assert!(list.voices.iter().any(|v| v == &vname));
        }

        assert_eq!(
            screenshot_argv(),
            vec!["spectacle".to_string(), "-r".to_string()]
        );

        let argb = lcg.next() as u32;
        let hex = argb_to_hex(argb);
        assert_eq!(hex.len(), 7);
        assert!(hex.starts_with('#'));
        let css = argb_to_rgba_css(argb);
        assert!(css.starts_with("rgba("));

        let n_sinks = lcg.below(4);
        let mut sinks = Vec::new();
        for _ in 0..n_sinks {
            let s = hermes_island_lib::integrations::tiles::core::Sink {
                index: lcg.next() as u32,
                name: random_sink_name(&mut lcg),
                description: random_text(&mut lcg),
                state: random_text(&mut lcg),
                muted: lcg.coin(),
            };
            sinks.push(s);
        }
        let default = if lcg.coin() {
            Some(random_sink_name(&mut lcg))
        } else {
            None
        };
        let tile = output_tile(sinks, default);
        if let Some(cur) = &tile.current {
            assert!(tile.choices.contains(cur));
        }

        let _ = network_tile_report(Some(random_snapshot(&mut lcg)));
        let _ = network_tile_report(None);

        drive_machine(&mut lcg, &mut ka);
    }
}
