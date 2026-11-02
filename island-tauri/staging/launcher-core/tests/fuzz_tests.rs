use launcher_core::{build_argv, parse_desktop_entry, AppEntry, ExecContext};

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

const VALUE_CHARS: [char; 10] = ['a', 'é', ' ', ';', '%', '"', '\\', '中', '\n', '\0'];
const WORD_CHARS: [char; 8] = ['a', 'b', '0', '-', '_', 'é', '中', '.'];

fn random_value(lcg: &mut Lcg) -> String {
    let mut s = String::new();
    for _ in 0..lcg.below(12) {
        match lcg.below(8) {
            0 => s.push_str("abc"),
            1 => s.push_str("\\s"),
            2 => s.push_str("\\n"),
            3 => s.push_str("\\t"),
            4 => s.push_str("\\\\"),
            5 => s.push_str("\"%f\""),
            6 => s.push_str("\\;"),
            _ => s.push(lcg.pick(&VALUE_CHARS)),
        }
    }
    s
}

fn random_line(lcg: &mut Lcg) -> String {
    match lcg.below(10) {
        0 => "# comment".to_string(),
        1 => format!("[Desktop Action {}]", lcg.below(9)),
        2 => String::new(),
        3 => "not a key line".to_string(),
        _ => {
            const KEYS: [&str; 17] = [
                "Name",
                "GenericName",
                "Comment",
                "Icon",
                "Exec",
                "TryExec",
                "Terminal",
                "NoDisplay",
                "Hidden",
                "OnlyShowIn",
                "NotShowIn",
                "Categories",
                "Keywords",
                "Type",
                "Unknown",
                "Name[sr_RS@latin]",
                "Name[de]",
            ];
            let key = KEYS[lcg.below(KEYS.len() as u64) as usize];
            format!("{key}={}", random_value(lcg))
        }
    }
}

fn random_desktop_text(lcg: &mut Lcg) -> String {
    let mut out = String::new();
    if lcg.below(20) == 0 {
        for _ in 0..lcg.below(15) {
            out.push_str(&random_line(lcg));
            out.push('\n');
        }
        return out;
    }
    out.push_str("[Desktop Entry]\n");
    for _ in 0..lcg.below(20) {
        out.push_str(&random_line(lcg));
        out.push('\n');
    }
    for _ in 0..lcg.below(3) {
        out.push_str(&format!("[Desktop Action {}]\n", lcg.below(100)));
        for _ in 0..lcg.below(5) {
            out.push_str(&random_line(lcg));
            out.push('\n');
        }
    }
    out
}

fn random_word(lcg: &mut Lcg) -> String {
    let mut s = String::new();
    for _ in 0..1 + lcg.below(6) {
        s.push(lcg.pick(&WORD_CHARS));
    }
    s
}

fn random_exec(lcg: &mut Lcg) -> String {
    let mut s = String::new();
    let n = 1 + lcg.below(6);
    for i in 0..n {
        if i > 0 {
            s.push(' ');
        }
        match lcg.below(13) {
            0 => s.push_str("%f"),
            1 => s.push_str("%F"),
            2 => s.push_str("%u"),
            3 => s.push_str("%U"),
            4 => s.push_str("%i"),
            5 => s.push_str("%c"),
            6 => s.push_str("%k"),
            7 => s.push_str("%%"),
            8 => s.push_str("%d"),
            9 => s.push_str("%x"),
            10 => {
                s.push('"');
                s.push_str(&random_word(lcg));
                s.push('"');
            }
            11 => {
                s.push('"');
                let inner = random_value(lcg).replace('"', "\\\"");
                s.push_str(&inner);
                s.push('"');
            }
            _ => s.push_str(&random_word(lcg)),
        }
    }
    s
}

/// Words without '%' so injected %c/%f/%i/%k values never look like
/// unexpanded field codes in the argv invariant.
fn random_safe_word(lcg: &mut Lcg) -> String {
    let mut s = String::new();
    for _ in 0..1 + lcg.below(5) {
        let c = match lcg.below(5) {
            0 => 'a',
            1 => 'B',
            2 => ' ',
            3 => 'é',
            _ => '中',
        };
        s.push(c);
    }
    s
}

fn random_context(lcg: &mut Lcg) -> ExecContext {
    ExecContext {
        name: random_safe_word(lcg),
        icon: lcg.coin().then(|| random_safe_word(lcg)),
        desktop_file: lcg.coin().then(|| random_safe_word(lcg)),
        files: (0..lcg.below(5)).map(|_| random_safe_word(lcg)).collect(),
    }
}

fn assert_no_unexpanded_code(argv: &[String]) {
    assert!(!argv.is_empty(), "Ok argv must be non-empty");
    for a in argv {
        let bytes = a.as_bytes();
        for i in 0..bytes.len() {
            if bytes[i] == b'%' {
                if let Some(&next) = bytes.get(i + 1) {
                    assert!(
                        !next.is_ascii_alphabetic(),
                        "unexpanded field code in argv element {a:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn fuzz_desktop_text_and_exec_never_panics() {
    let mut lcg = Lcg(0xC0FF_EE42_0000_0001);
    let locales = [
        None,
        Some("en"),
        Some("sr_RS@latin"),
        Some("de_DE.UTF-8"),
        Some("pt"),
    ];
    for _ in 0..15_000 {
        let text = random_desktop_text(&mut lcg);
        let locale = locales[lcg.below(locales.len() as u64) as usize];
        let did = format!("{}.desktop", random_word(&mut lcg));
        if let Ok((entry, _warnings)) = parse_desktop_entry(&text, &did, locale) {
            // serde round-trip must be lossless.
            let json = serde_json::to_string(&entry).unwrap();
            let back: AppEntry = serde_json::from_str(&json).unwrap();
            assert_eq!(back, entry);
            // Exec taken from the parsed entry.
            let ctx = random_context(&mut lcg);
            if let Ok(argv) = build_argv(&entry.exec, &ctx) {
                assert_no_unexpanded_code(&argv);
            }
        }
        // Random standalone Exec string.
        let exec = random_exec(&mut lcg);
        let ctx = random_context(&mut lcg);
        if let Ok(argv) = build_argv(&exec, &ctx) {
            assert_no_unexpanded_code(&argv);
        }
    }
}
