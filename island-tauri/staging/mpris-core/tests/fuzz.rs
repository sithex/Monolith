use mpris_core::{parse_metadata, position_at, PlaybackStatus, Players};
use serde_json::{Map, Value};

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

    fn string(&mut self) -> String {
        let alphabet = ['a', 'b', ' ', 'é', '🎵', '\t', '\n'];
        let len = self.below(60);
        let mut s = String::with_capacity(len as usize * 4);
        for _ in 0..len {
            let c = alphabet[self.below(alphabet.len() as u64) as usize];
            s.push(c);
        }
        s
    }

    fn value(&mut self) -> Value {
        match self.below(6) {
            0 => Value::String(self.string()),
            1 => {
                let n = self.below(10);
                let mut arr = Vec::new();
                for _ in 0..n {
                    arr.push(Value::String(self.string()));
                }
                Value::Array(arr)
            }
            2 => Value::Number(serde_json::Number::from(self.below(1 << 40))),
            3 => {
                let f = (self.below(1000) as f64) / 7.0 - 50.0;
                match serde_json::Number::from_f64(f) {
                    Some(n) => Value::Number(n),
                    None => Value::Null,
                }
            }
            4 => Value::Bool(self.coin()),
            _ => Value::Null,
        }
    }

    fn metadata(&mut self) -> Map<String, Value> {
        let keys = [
            "mpris:trackid",
            "xesam:title",
            "xesam:artist",
            "xesam:album",
            "mpris:artUrl",
            "mpris:length",
            "junk",
            "",
        ];
        let mut m = Map::new();
        for _ in 0..self.below(10) {
            let k = keys[self.below(keys.len() as u64) as usize];
            m.insert(k.to_string(), self.value());
        }
        m
    }

    fn bus(&mut self) -> String {
        if self.coin() {
            format!("org.mpris.MediaPlayer2.fuzz{}", self.below(100))
        } else {
            format!("org.invalid.bus{}", self.below(100))
        }
    }

    fn status(&mut self) -> PlaybackStatus {
        match self.below(3) {
            0 => PlaybackStatus::Playing,
            1 => PlaybackStatus::Paused,
            _ => PlaybackStatus::Stopped,
        }
    }

    fn rate(&mut self) -> f64 {
        match self.below(6) {
            0 => 0.0,
            1 => -2.5,
            2 => f64::NAN,
            3 => f64::INFINITY,
            4 => 1.5,
            _ => (self.below(1000) as f64) / 100.0,
        }
    }
}

#[test]
fn never_panics_on_pseudorandom_input() {
    let mut rng = Lcg(0x9E37_79B9_7F4A_7C15);
    for _ in 0..2000 {
        let m = rng.metadata();
        let _ = parse_metadata(&m);

        let mut players = Players::default();
        for _ in 0..80 {
            let now = rng.below(1_000_000);
            match rng.below(8) {
                0 => players.upsert_status(&rng.bus(), rng.status(), now),
                1 => players.set_metadata(&rng.bus(), &rng.metadata(), now),
                2 => players.seeked(&rng.bus(), rng.below(1 << 41) as i64, now),
                3 => players.set_position(&rng.bus(), rng.below(1 << 41) as i64, now),
                4 => players.set_rate(&rng.bus(), rng.rate(), now),
                5 => {
                    players.remove(&rng.bus());
                }
                6 => {
                    let _ = players.preferred();
                }
                _ => {
                    if let Some(p) = players.preferred() {
                        let _ = position_at(p, rng.below(1_000_000));
                    }
                }
            }
        }
        // Always exercise position_at on the preferred player with random clocks,
        // including now < positionAtMs (underflow path).
        for _ in 0..10 {
            if let Some(p) = players.preferred() {
                let _ = position_at(p, rng.below(1 << 40));
            }
        }
    }
}
