//! Deterministic pseudo-random never-panic fuzz over the whole public API.
//! Hand-rolled LCG: no extra dependencies, fully reproducible.

use hermes_island_lib::integrations::media::lrclib::{
    cache_key, get_path, parse_response, TrackQuery,
};

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }

    fn rand_str(&mut self, max_len: usize) -> String {
        let n = self.below((max_len + 1) as u64) as usize;
        String::from_utf8_lossy(&self.bytes(n)).into_owned()
    }
}

#[test]
fn never_panics_on_pseudorandom_input() {
    let mut rng = Lcg(0xC0FFEE_F00DBA5E00);

    const STATUSES: [u16; 12] = [200, 404, 429, 503, 0, 100, 201, 301, 418, 500, 599, 65535];
    const CT_POOL: [&str; 7] = [
        "application/json",
        "Application/JSON; charset=utf-8",
        "text/plain",
        "",
        "application/problem+json",
        "application/octet-stream",
        " text/plain ",
    ];
    const RETRY_POOL: [&str; 7] = [
        "5",
        "0",
        "99999",
        "abc",
        "-1",
        "1e3",
        "99999999999999999999999",
    ];

    for _ in 0..3000 {
        let status = STATUSES[rng.below(STATUSES.len() as u64) as usize];
        let content_type = match rng.below(10) {
            0 => Some(rng.rand_str(96)),
            1 => None,
            n => Some(CT_POOL[(n as usize) % CT_POOL.len()].to_string()),
        };
        let retry_after = match rng.below(10) {
            0 => Some(rng.rand_str(48)),
            1 => None,
            n => Some(RETRY_POOL[(n as usize) % RETRY_POOL.len()].to_string()),
        };
        let body_len = match rng.below(24) {
            0 => ONE_MIB + 1 + rng.below(4096) as usize,
            1 => ONE_MIB,
            _ => rng.below(4096) as usize,
        };
        let body = rng.bytes(body_len);
        let _ = parse_response(
            status,
            content_type.as_deref(),
            retry_after.as_deref(),
            &body,
        );
    }

    for _ in 0..3000 {
        let tq = TrackQuery {
            title: rng.rand_str(2000),
            artist: rng.rand_str(2000),
            album: if rng.below(2) == 0 {
                None
            } else {
                Some(rng.rand_str(2000))
            },
            length_ms: if rng.below(2) == 0 {
                None
            } else {
                Some(rng.next())
            },
        };
        let _ = get_path(&tq);
        let _ = cache_key(&tq);
    }
}

const ONE_MIB: usize = 1024 * 1024;
