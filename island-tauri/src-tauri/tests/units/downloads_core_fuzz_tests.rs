use std::collections::{HashMap, HashSet};

use hermes_island_lib::integrations::downloads::core::{
    DirEvent, DownloadsConfig, DownloadsModel, FileStat, ItemState,
};

/// Hand-rolled LCG (Numerical Recipes constants); no external rand dep.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed)
    }

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

    fn pick<'a>(&mut self, xs: &'a [&'a str]) -> &'a str {
        xs[self.below(xs.len() as u64) as usize]
    }
}

/// Pool mixing valid names, hidden names, partial suffixes and invalid names
/// (empty, `.`, `..`, `/`-containing, NUL-containing).
const NAMES: &[&str] = &[
    "a",
    "b",
    "c",
    "report.pdf",
    "photo.PNG",
    "song.mp3",
    "movie.mp4",
    "arch.tar.gz",
    "pkg.pkg.tar.zst",
    "app.AppImage",
    "code.rs",
    "notes.md",
    "download.part",
    "dl.crdownload",
    "dl2.download",
    "dl3.partial",
    "dl4.opdownload",
    ".hidden",
    ".secret.txt",
    "bad/name",
    "bad\x00name",
    "",
    ".",
    "..",
];

fn mk_stat(name: &str, rng: &mut Lcg) -> FileStat {
    FileStat {
        name: name.to_string(),
        size_bytes: rng.below(1 << 40),
        mtime_ms: rng.below(100_000),
        is_dir: false,
        is_symlink: false,
    }
}

fn invalid(name: &str) -> bool {
    name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0')
}

/// Mirror the model's `accept` filter for generated stats (no dirs/symlinks
/// are generated in the fuzz harness).
fn shadow_accept(name: &str, show_hidden: bool) -> bool {
    !invalid(name) && (!name.starts_with('.') || show_hidden)
}

fn is_partial(name: &str) -> bool {
    const SUFFIXES: [&str; 5] = [
        ".part",
        ".crdownload",
        ".download",
        ".partial",
        ".opdownload",
    ];
    let lower = name.to_lowercase();
    SUFFIXES.iter().any(|s| lower.ends_with(s))
}

/// Shadow map mirroring the model: name -> (size, mtime, last_stat_change_time).
type Shadow = HashMap<String, (u64, u64, u64)>;

/// Mirror the model's `upsert` (including the same-stat no-restart rule and
/// the rename-force rule). `force` = rename semantics: always restart.
fn shadow_upsert(shadow: &mut Shadow, st: &FileStat, now: u64, force: bool, show_hidden: bool) {
    if !shadow_accept(&st.name, show_hidden) {
        return;
    }
    let same_stat = shadow.get(&st.name).is_some_and(|(s, m, _)| {
        *s == st.size_bytes && *m == st.mtime_ms && !force && !is_partial(&st.name)
    });
    if !same_stat {
        shadow.insert(st.name.clone(), (st.size_bytes, st.mtime_ms, now));
    }
}

/// Mirror the model's eviction: keep newest `max_items` by (mtime desc, name
/// asc). Must match the model's tie-break exactly.
fn shadow_evict(shadow: &mut Shadow, max_items: usize) {
    if shadow.len() <= max_items {
        return;
    }
    let mut ranked: Vec<(String, u64)> =
        shadow.iter().map(|(name, v)| (name.clone(), v.1)).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let keep: HashSet<&str> = ranked
        .iter()
        .take(max_items)
        .map(|(n, _)| n.as_str())
        .collect();
    shadow.retain(|name, _| keep.contains(name.as_str()));
}

#[test]
fn fuzz_downloads_model_never_panics_and_keeps_invariants() {
    let settle_ms = 137u64;
    let cfg = DownloadsConfig {
        max_items: 8, // small enough to exercise eviction
        settle_ms,
        show_hidden: true,
    };
    let mut model = match DownloadsModel::new(cfg) {
        Ok(m) => m,
        Err(e) => panic!("config should be valid: {e:?}"),
    };
    let mut rng = Lcg::new(0x9E37_79B9_7F4A_7C15);
    let mut now = 0u64;
    let mut shadow: Shadow = HashMap::new();

    for step in 0..4000u64 {
        now = now.saturating_add(rng.below(30));

        match rng.below(100) {
            0..=54 => {
                let name = rng.pick(NAMES);
                let st = mk_stat(name, &mut rng);
                model.apply(now, DirEvent::Upsert(st.clone()));
                shadow_upsert(&mut shadow, &st, now, false, cfg.show_hidden);
            }
            55..=68 => {
                let name = rng.pick(NAMES);
                model.apply(
                    now,
                    DirEvent::Removed {
                        name: name.to_string(),
                    },
                );
                shadow.remove(name);
            }
            69..=88 => {
                let from = rng.pick(NAMES);
                let name = rng.pick(NAMES);
                let st = mk_stat(name, &mut rng);
                model.apply(
                    now,
                    DirEvent::Renamed {
                        from: from.to_string(),
                        to: st.clone(),
                    },
                );
                shadow.remove(from);
                shadow_upsert(&mut shadow, &st, now, true, cfg.show_hidden);
            }
            _ => {
                let mut list: Vec<FileStat> = Vec::new();
                let mut seen: HashSet<&str> = HashSet::new();
                for _ in 0..rng.below(12) {
                    let name = rng.pick(NAMES);
                    if !seen.insert(name) {
                        continue;
                    }
                    list.push(mk_stat(name, &mut rng));
                }
                model.apply(now, DirEvent::Rescan(list.clone()));
                let old = std::mem::take(&mut shadow);
                for st in &list {
                    if !shadow_accept(&st.name, cfg.show_hidden) {
                        continue;
                    }
                    match old.get(&st.name) {
                        Some((s, m, lc))
                            if *s == st.size_bytes
                                && *m == st.mtime_ms
                                && !is_partial(&st.name) =>
                        {
                            shadow.insert(st.name.clone(), (*s, *m, *lc));
                        }
                        _ => {
                            shadow.insert(st.name.clone(), (st.size_bytes, st.mtime_ms, now));
                        }
                    }
                }
            }
        }

        shadow_evict(&mut shadow, cfg.max_items as usize);

        let items = model.items(now);

        assert!(
            items.len() <= cfg.max_items as usize,
            "step {step}: len {} exceeds max_items {}",
            items.len(),
            cfg.max_items
        );

        let mut seen = HashSet::new();
        for it in &items {
            assert!(
                seen.insert(it.name.clone()),
                "step {step}: duplicate name {}",
                it.name
            );
        }

        for pair in items.windows(2) {
            assert!(
                pair[0].mtime_ms > pair[1].mtime_ms
                    || (pair[0].mtime_ms == pair[1].mtime_ms && pair[0].name < pair[1].name),
                "step {step}: out of order {} ({}) then {} ({})",
                pair[0].name,
                pair[0].mtime_ms,
                pair[1].name,
                pair[1].mtime_ms
            );
        }

        for it in &items {
            if it.state == ItemState::Ready {
                let (_, _, lc) = match shadow.get(&it.name) {
                    Some(v) => v,
                    None => panic!("step {step}: ready item {} missing from shadow", it.name),
                };
                let elapsed = now.saturating_sub(*lc);
                assert!(
                    elapsed >= settle_ms,
                    "step {step}: item {} Ready after only {}ms (settle={}ms)",
                    it.name,
                    elapsed,
                    settle_ms
                );
            }
        }

        if step % 7 == 0 {
            model.tick(now);
        }
    }
}

#[test]
fn fuzz_default_config_filters_hidden() {
    let cfg = DownloadsConfig::default();
    let mut model = match DownloadsModel::new(cfg) {
        Ok(m) => m,
        Err(e) => panic!("config should be valid: {e:?}"),
    };
    let mut rng = Lcg::new(0xDEAD_BEEF_CAFE_BABE);
    let mut now = 0u64;
    let mut shadow: Shadow = HashMap::new();

    for _ in 0..2000u64 {
        now = now.saturating_add(rng.below(50));
        let name = rng.pick(NAMES);
        let st = mk_stat(name, &mut rng);
        match rng.below(10) {
            0..=5 => {
                model.apply(now, DirEvent::Upsert(st.clone()));
                shadow_upsert(&mut shadow, &st, now, false, cfg.show_hidden);
            }
            6..=7 => {
                model.apply(
                    now,
                    DirEvent::Removed {
                        name: name.to_string(),
                    },
                );
                shadow.remove(name);
            }
            _ => {
                model.apply(
                    now,
                    DirEvent::Renamed {
                        from: name.to_string(),
                        to: st.clone(),
                    },
                );
                shadow.remove(name);
                shadow_upsert(&mut shadow, &st, now, true, cfg.show_hidden);
            }
        }
        shadow_evict(&mut shadow, cfg.max_items as usize);

        let items = model.items(now);
        assert!(items.len() <= 500, "len {} exceeds 500", items.len());
        for it in &items {
            assert!(!it.name.starts_with('.'), "hidden name leaked: {}", it.name);
        }
        if now % 5 == 0 {
            model.tick(now);
        }
    }
}
