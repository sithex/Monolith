use hermes_island_lib::integrations::mixer::core::{
    move_stream, mute_sink, mute_stream, needs_refresh, parse_default_sink, parse_event,
    parse_sink_inputs, parse_sinks, set_app_volume, set_default_sink, set_sink_volume,
    set_stream_volume, AppGroup, MixerModel, MuteOp, Sink,
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
    let len = lcg.below(300);
    let mut s = String::new();
    for _ in 0..len {
        let c = match lcg.below(10) {
            0 => '{',
            1 => '}',
            2 => '[',
            3 => ']',
            4 => '"',
            5 => ' ',
            6 => ',',
            7 => ':',
            8 => 'x',
            _ => '1',
        };
        s.push(c);
    }
    s
}

fn random_sink_name(lcg: &mut Lcg) -> String {
    let good = ['a', 'b', '0', '-', '_', '.'];
    let bad = [' ', '\t', '\n', '\0', '-'];
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

fn random_event_line(lcg: &mut Lcg) -> String {
    let facilities = [
        "sink",
        "source",
        "sink-input",
        "source-output",
        "client",
        "module",
        "card",
        "server",
        "garbage",
    ];
    let mut s = String::new();
    if lcg.coin() {
        s.push_str("Event ");
    } else {
        s.push_str("Garbage ");
    }
    if lcg.coin() {
        let k = match lcg.below(4) {
            0 => "new",
            1 => "change",
            2 => "remove",
            _ => "novel",
        };
        s.push_str(&format!("'{k}' "));
    } else {
        s.push_str("x ");
    }
    if lcg.coin() {
        s.push_str("on ");
    }
    s.push_str(facilities[lcg.below(facilities.len() as u64) as usize]);
    s.push(' ');
    if lcg.coin() {
        s.push('#');
    }
    if lcg.coin() {
        s.push_str(&lcg.below(1_000_000).to_string());
    } else {
        s.push_str("abc");
    }
    if lcg.coin() {
        s.push_str(" extra");
    }
    s
}

fn make_sink(name: String) -> Sink {
    Sink {
        index: 0,
        name,
        description: String::new(),
        muted: false,
        volume_percent: None,
        channels: 0,
        state: String::new(),
    }
}

fn assert_valid_argv(argv: &[String]) {
    assert!(!argv.is_empty(), "argv must not be empty");
    assert_eq!(argv[0], "pactl", "argv must start with pactl");
    for a in argv {
        assert!(!a.is_empty(), "argv element must not be empty");
        assert!(
            !a.contains(char::is_whitespace),
            "argv element has whitespace: {a:?}"
        );
        assert!(!a.contains('\0'), "argv element has NUL: {a:?}");
    }
}

#[test]
fn fuzz_mixer_never_panics_and_keeps_invariants() {
    let mut lcg = Lcg(0xC0FF_EE42_0000_0001);
    let mut sinks = Vec::new();
    let mut streams = Vec::new();

    for _ in 0..3000 {
        let txt = random_text(&mut lcg);

        if let Ok(s) = parse_sinks(&txt) {
            sinks = s;
        }
        if let Ok(st) = parse_sink_inputs(&txt) {
            streams = st;
        }
        let _ = parse_default_sink(&txt);

        let line = random_event_line(&mut lcg);
        if let Some(ev) = parse_event(&line) {
            let _ = needs_refresh(&ev);
        }

        if !streams.is_empty() {
            let model = MixerModel::from(
                sinks.clone(),
                streams.clone(),
                Some(random_sink_name(&mut lcg)),
            );
            let _ = model.default_sink();
            let existing: std::collections::HashSet<u32> =
                streams.iter().map(|s| s.index).collect();
            for g in model.apps() {
                for idx in &g.streams {
                    assert!(
                        existing.contains(idx),
                        "app group stream {idx} not in model streams"
                    );
                }
            }
        }

        let pct = lcg.below(400) as u32;
        let idx = lcg.next() as u32;
        let sk = make_sink(random_sink_name(&mut lcg));
        let op = match lcg.below(3) {
            0 => MuteOp::Off,
            1 => MuteOp::On,
            _ => MuteOp::Toggle,
        };
        for a in [
            set_sink_volume(&sk, pct),
            set_stream_volume(idx, pct),
            mute_sink(&sk, op),
            mute_stream(idx, op),
            set_default_sink(&sk),
            move_stream(idx, &sk),
        ]
        .into_iter()
        .flatten()
        {
            assert_valid_argv(&a);
        }

        let g = AppGroup {
            app_name: random_sink_name(&mut lcg),
            streams: vec![idx],
            volume_percent: Some(pct.min(150)),
            muted: lcg.coin(),
            sink_index: None,
            idle: lcg.coin(),
        };
        if let Ok(argvs) = set_app_volume(&g, pct) {
            for a in argvs {
                assert_valid_argv(&a);
            }
        }
    }
}
