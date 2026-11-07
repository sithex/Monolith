use hermes_island_lib::integrations::updates::core::{
    backoff_minutes, is_kernel, parse_checkupdates, parse_paru_qua, Badge, Schedule, Source,
    Update, UpdatesError,
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
}

fn random_text(lcg: &mut Lcg) -> String {
    const CHARS: [char; 27] = [
        'a', 'b', 'c', '0', '1', ' ', '\t', '\n', '\r', '-', '>', '[', ']', '@', '.', '_', '+',
        '~', ':', '=', '*', '(', ')', '/', '\\', '\0', 'Z',
    ];
    let len = lcg.below(200);
    let mut s = String::new();
    for _ in 0..len {
        s.push(CHARS[lcg.below(CHARS.len() as u64) as usize]);
    }
    s
}

fn random_exit(lcg: &mut Lcg) -> i32 {
    const EXITS: [i32; 12] = [0, 0, 0, 1, 1, 1, 2, 2, 3, 127, -1, 1];
    EXITS[lcg.below(EXITS.len() as u64) as usize]
}

fn random_error(lcg: &mut Lcg) -> UpdatesError {
    match lcg.below(4) {
        0 => UpdatesError::StdoutTooLarge,
        1 => UpdatesError::TooManyLines,
        2 => UpdatesError::CommandFailed,
        _ => UpdatesError::AllMalformed,
    }
}

fn random_update(lcg: &mut Lcg) -> Update {
    const NAME_CHARS: [char; 10] = ['a', 'b', '0', '-', '.', '@', '_', '+', 'Z', ' '];
    const VER_CHARS: [char; 12] = ['1', '2', '.', '-', ':', '~', '+', '_', 'a', 'Z', ' ', '='];
    let name_len = lcg.below(140);
    let mut name = String::new();
    for _ in 0..name_len {
        name.push(NAME_CHARS[lcg.below(NAME_CHARS.len() as u64) as usize]);
    }
    let ver_len = lcg.below(140);
    let mut ver = String::new();
    for _ in 0..ver_len {
        ver.push(VER_CHARS[lcg.below(VER_CHARS.len() as u64) as usize]);
    }
    Update {
        name,
        old_version: ver.clone(),
        new_version: ver,
        // `below(3)` (odd modulus) breaks the LCG's strict parity
        // alternation, so `ignored` actually varies (a bare `coin()`
        // would be constant at this fixed call offset).
        ignored: lcg.below(3) == 0,
        source: if lcg.coin() {
            Source::Repo
        } else {
            Source::Aur
        },
        arch: None,
    }
}

fn random_updates(lcg: &mut Lcg) -> Vec<Update> {
    let n = lcg.below(6);
    let mut out = Vec::new();
    for _ in 0..n {
        out.push(random_update(lcg));
    }
    out
}

#[test]
fn fuzz_updates_never_panics() {
    let mut lcg = Lcg(0xC0FF_EE42_0000_0002);

    for _ in 0..3000 {
        let exit = random_exit(&mut lcg);
        let stdout = random_text(&mut lcg);
        let stderr = random_text(&mut lcg);

        let _ = parse_checkupdates(exit, &stdout, &stderr);
        let _ = parse_paru_qua(exit, &stdout, &stderr);
        let _ = is_kernel(&random_text(&mut lcg));
        let _ = backoff_minutes(lcg.below(64) as u32);

        let sched = Schedule {
            on_battery: lcg.coin(),
            consecutive_failures: lcg.below(16) as u32,
        };
        let _ = sched.due(lcg.next(), Some(lcg.next()), lcg.coin().then(|| lcg.next()));

        let repo: Result<Vec<Update>, UpdatesError> = if lcg.coin() {
            Ok(random_updates(&mut lcg))
        } else {
            Err(random_error(&mut lcg))
        };
        let aur: Option<Result<Vec<Update>, UpdatesError>> = if lcg.coin() {
            None
        } else if lcg.coin() {
            Some(Ok(random_updates(&mut lcg)))
        } else {
            Some(Err(random_error(&mut lcg)))
        };
        let prev = if lcg.coin() {
            None
        } else {
            Some(random_updates(&mut lcg))
        };

        let badge = Badge::compute(
            repo,
            aur,
            lcg.next(),
            lcg.coin().then(|| lcg.next()),
            prev.as_deref(),
        );

        // invariants over the computed badge
        match (badge.count, badge.repo_count, badge.aur_count) {
            (Some(c), Some(r), Some(a)) => {
                assert_eq!(c, r + a, "count != repoCount + aurCount");
            }
            (None, None, None) => {}
            other => panic!("badge counts inconsistent: {other:?}"),
        }
        if let Some(c) = badge.count {
            let n = badge.items.iter().filter(|u| !u.ignored).count() as u32;
            assert_eq!(c, n, "count != non-ignored items");
        }
        for w in badge.items.windows(2) {
            let (x, y) = (&w[0], &w[1]);
            assert!(
                x.needs_reboot() || !y.needs_reboot(),
                "reboot items must sort first"
            );
            if x.needs_reboot() == y.needs_reboot() {
                assert!(x.name <= y.name, "same-class items must sort by name");
            }
        }
        if let Ok(js) = serde_json::to_string(&badge) {
            if let Ok(back) = serde_json::from_str::<Badge>(&js) {
                assert_eq!(back, badge, "serde round-trip");
            }
        }
    }
}
