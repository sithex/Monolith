use ics_core::{agenda, parse_calendar, Instance, NaiveDateTime};

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

fn rand_string(lcg: &mut Lcg, max: usize) -> String {
    let len = lcg.below(max as u64) as usize;
    let charset = ['a', 'b', '0', ' ', '_', 'é', ';'];
    (0..len).map(|_| lcg.pick(&charset)).collect()
}

fn rand_num(lcg: &mut Lcg) -> String {
    let len = 1 + lcg.below(8) as usize;
    let mut s = String::new();
    for _ in 0..len {
        s.push((b'0' + lcg.below(10) as u8) as char);
    }
    s
}

fn rand_datetime(lcg: &mut Lcg) -> String {
    let y = 1900 + lcg.below(300);
    let m = 1 + lcg.below(12);
    let d = 1 + lcg.below(28);
    let h = lcg.below(24);
    let mi = lcg.below(60);
    let se = lcg.below(60);
    format!("{y:04}{m:02}{d:02}T{h:02}{mi:02}{se:02}")
}

fn random_event(lcg: &mut Lcg) -> String {
    let mut s = String::from("BEGIN:VEVENT\r\nUID:");
    s.push_str(&rand_string(lcg, 6));
    s.push_str("\r\n");
    if lcg.coin() {
        s.push_str("SUMMARY:");
        s.push_str(&rand_string(lcg, 6));
        s.push_str("\r\n");
    }
    let dt = rand_datetime(lcg);
    match lcg.below(4) {
        0 => {
            s.push_str("DTSTART;VALUE=DATE:");
            s.push_str(&dt[..8]);
            s.push_str("\r\n");
        }
        1 => {
            s.push_str("DTSTART:");
            s.push_str(&dt);
            s.push_str("Z\r\n");
        }
        2 => {
            s.push_str("DTSTART:");
            s.push_str(&dt);
            s.push_str("\r\n");
        }
        _ => {
            s.push_str("DTSTART;TZID=Zone/");
            s.push_str(&rand_string(lcg, 4));
            s.push(':');
            s.push_str(&dt);
            s.push_str("\r\n");
        }
    }
    if lcg.coin() {
        s.push_str("DURATION:P");
        if lcg.coin() {
            s.push('T');
        }
        s.push_str(&rand_num(lcg));
        s.push(match lcg.below(3) {
            0 => 'H',
            1 => 'M',
            _ => 'S',
        });
        s.push_str("\r\n");
    }
    if lcg.coin() {
        s.push_str("RRULE:FREQ=");
        s.push_str(match lcg.below(4) {
            0 => "DAILY",
            1 => "WEEKLY",
            2 => "MONTHLY",
            _ => "YEARLY",
        });
        if lcg.coin() {
            s.push_str(";INTERVAL=");
            s.push_str(&rand_num(lcg));
        }
        match lcg.below(4) {
            0 => {
                s.push_str(";COUNT=");
                s.push_str(&rand_num(lcg));
            }
            1 => {
                s.push_str(";UNTIL=");
                s.push_str(&rand_datetime(lcg));
                s.push('Z');
            }
            _ => {}
        }
        if lcg.coin() {
            s.push_str(";BYDAY=MO,WE,FR");
        }
        s.push_str("\r\n");
    }
    if lcg.coin() {
        s.push_str("EXDATE:");
        s.push_str(&rand_datetime(lcg));
        s.push_str("Z,");
        s.push_str(&rand_datetime(lcg));
        s.push_str("Z\r\n");
    }
    if lcg.coin() {
        s.push_str("STATUS:CANCELLED\r\n");
    }
    s.push_str("END:VEVENT\r\n");
    s
}

fn random_cal(lcg: &mut Lcg) -> String {
    let mut s = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
    let n = lcg.below(4) as usize;
    for _ in 0..n {
        s.push_str(&random_event(lcg));
    }
    s.push_str("END:VCALENDAR\r\n");
    s
}

fn no_tz(_: &str, _: NaiveDateTime) -> Option<i32> {
    None
}

#[test]
fn fuzz_never_panics_and_invariants() {
    let mut lcg = Lcg(0x9E37_79B9_7F4A_7C15);
    for _ in 0..300 {
        let text = random_cal(&mut lcg);
        let events = match parse_calendar(&text) {
            Ok((e, _)) => e,
            Err(_) => continue,
        };
        let base = (lcg.next() as i64) % (365 * 86_400 * 6) - 365 * 86_400 * 3;
        let len = (lcg.below(700) as i64) * 86_400;
        let ws = base;
        let we = base.wrapping_add(len);
        let off = (lcg.next() as i32) % (8 * 3600) - 4 * 3600;
        let inst: Vec<Instance> = agenda(&events, ws, we, off, &no_tz);
        assert!(inst.len() <= 2000);
        for w in inst.windows(2) {
            let a = &w[0];
            let b = &w[1];
            let ka = (a.start_s, !a.all_day, a.summary.as_str(), a.uid.as_str());
            let kb = (b.start_s, !b.all_day, b.summary.as_str(), b.uid.as_str());
            assert!(ka <= kb, "out of order: {:?} after {:?}", a, b);
        }
        for i in &inst {
            assert!(i.end_s >= i.start_s, "negative span: {i:?}");
            assert!(
                i.start_s < we && i.end_s > ws,
                "not overlapping window: {i:?} in [{ws},{we})"
            );
        }
    }
}
