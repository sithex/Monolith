//! Monolith's own running cost, for the Monitor view's "Monolith cost" card: CPU %, PSS and
//! scheduler wakeups of the island's whole process tree (Tauri main, WebKit processes, helpers,
//! the embedded terminal and whatever runs in it), plus the CPU % of `plasmashell` and
//! `kwin_wayland`, where the notch's redraws land.
//!
//! Sampled only when the view asks (one call per poll while it is visible); there is no
//! background thread. CPU and wakeups are rates over a sliding ~10 s window, built from
//! per-process / per-thread counter deltas so processes that start or exit inside the window
//! neither vanish from nor spike the figure. Linux only; the maths is pure and tested below.

use crate::contract::system::MonolithCost;
use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    time::{Duration, Instant},
};

/// Length of the averaging window.
pub const WINDOW: Duration = Duration::from_secs(10);
/// A pause between samples longer than this (the view was hidden) starts a fresh window, so
/// the first reading after reopening never averages over the time nobody was looking.
const GAP: Duration = Duration::from_secs(15);
/// Shortest span a rate is reported over; two samples closer than this say nothing.
const MIN_SPAN: Duration = Duration::from_millis(500);

/// Turns per-key cumulative counters into one monotonic total that survives keys appearing
/// and disappearing. The first call only records a baseline.
#[derive(Debug)]
pub struct Tally<K: Hash + Eq + Copy> {
    last: HashMap<K, u64>,
    total: u64,
    primed: bool,
}

impl<K: Hash + Eq + Copy> Default for Tally<K> {
    fn default() -> Self {
        Self {
            last: HashMap::new(),
            total: 0,
            primed: false,
        }
    }
}

impl<K: Hash + Eq + Copy> Tally<K> {
    /// Add the growth since the previous call. A key seen for the first time after the
    /// baseline counts in full (it was born inside the interval); a counter that went
    /// backwards (a reused id) restarts from zero.
    pub fn advance(&mut self, current: impl IntoIterator<Item = (K, u64)>) {
        let mut next = HashMap::new();
        for (key, value) in current {
            if self.primed {
                self.total += match self.last.get(&key) {
                    Some(&before) if value >= before => value - before,
                    _ => value,
                };
            }
            next.insert(key, value);
        }
        self.last = next;
        self.primed = true;
    }

    pub fn total(&self) -> u64 {
        self.total
    }
}

/// A [`Tally`] plus the recent history of its total, giving a rate over the last [`WINDOW`].
#[derive(Debug, Default)]
pub struct Meter<K: Hash + Eq + Copy> {
    tally: Tally<K>,
    history: VecDeque<(Instant, u64)>,
}

impl<K: Hash + Eq + Copy> Meter<K> {
    pub fn push(&mut self, now: Instant, current: impl IntoIterator<Item = (K, u64)>) {
        if self
            .history
            .back()
            .is_some_and(|(at, _)| now.saturating_duration_since(*at) > GAP)
        {
            self.tally = Tally::default();
            self.history.clear();
        }
        self.tally.advance(current);
        self.history.push_back((now, self.tally.total()));
        // Keep one anchor at or before the window start; drop everything older than that.
        while self.history.len() > 2 && now.saturating_duration_since(self.history[1].0) >= WINDOW {
            self.history.pop_front();
        }
    }

    /// Counter growth per second over the window, and the span it covers.
    pub fn rate(&self) -> Option<(f64, Duration)> {
        let (now, total) = *self.history.back()?;
        let (then, before) = *self.history.front()?;
        let span = now.saturating_duration_since(then);
        if span < MIN_SPAN {
            return None;
        }
        Some(((total - before) as f64 / span.as_secs_f64(), span))
    }
}

/// What one process of the tree contributes to a sample.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcRead {
    pub pid: u32,
    /// utime + stime, in clock ticks.
    pub own_ticks: u64,
    /// cutime + cstime: children it has already waited for, in clock ticks.
    pub child_ticks: u64,
    pub pss_kb: Option<u64>,
    /// (thread id, voluntary + involuntary context switches) per thread.
    pub threads: Vec<(u32, u64)>,
}

/// Everything one sample reads from the system.
#[derive(Clone, Debug, Default)]
pub struct CostInput {
    pub tree: Vec<ProcRead>,
    /// (pid, utime + stime ticks)
    pub plasmashell: Option<(u32, u64)>,
    pub kwin: Option<(u32, u64)>,
}

#[derive(Debug, Default)]
pub struct CostSampler {
    cpu: Meter<(u32, bool)>,
    wakeups: Meter<(u32, u32)>,
    plasmashell: Meter<u32>,
    kwin: Meter<u32>,
}

impl CostSampler {
    /// `ticks_per_second` is the kernel's USER_HZ (`sysconf(_SC_CLK_TCK)`).
    pub fn sample(
        &mut self,
        now: Instant,
        input: &CostInput,
        ticks_per_second: f64,
    ) -> MonolithCost {
        let hz = if ticks_per_second > 0.0 {
            ticks_per_second
        } else {
            100.0
        };
        self.cpu.push(
            now,
            input.tree.iter().flat_map(|p| {
                [
                    ((p.pid, false), p.own_ticks),
                    ((p.pid, true), p.child_ticks),
                ]
            }),
        );
        self.wakeups.push(
            now,
            input
                .tree
                .iter()
                .flat_map(|p| p.threads.iter().map(move |(tid, n)| ((p.pid, *tid), *n))),
        );
        self.plasmashell.push(now, input.plasmashell);
        self.kwin.push(now, input.kwin);
        let pss: Option<u64> = input
            .tree
            .iter()
            .map(|p| p.pss_kb)
            .try_fold(0u64, |sum, kb| Some(sum + kb?));
        MonolithCost {
            cpu: percent(&self.cpu, !input.tree.is_empty(), hz),
            pss_bytes: pss.map(|kb| kb as f64 * 1024.0),
            wakeups: self
                .wakeups
                .rate()
                .filter(|_| !input.tree.is_empty())
                .map(|(per_second, _)| round_tenth(per_second)),
            processes: input.tree.len() as u32,
            threads: input.tree.iter().map(|p| p.threads.len() as u32).sum(),
            plasmashell_cpu: percent(&self.plasmashell, input.plasmashell.is_some(), hz),
            kwin_cpu: percent(&self.kwin, input.kwin.is_some(), hz),
            window_secs: self
                .cpu
                .rate()
                .map_or(0.0, |(_, span)| (span.as_secs_f64() * 10.0).round() / 10.0),
        }
    }
}

/// Clock-tick growth per second as a percentage of one core; `None` while the window is
/// too short or the process is absent (unavailable, never a fabricated zero).
fn percent<K: Hash + Eq + Copy>(meter: &Meter<K>, present: bool, hz: f64) -> Option<f64> {
    meter
        .rate()
        .filter(|_| present)
        .map(|(ticks, _)| round_tenth(ticks / hz * 100.0))
}

fn round_tenth(n: f64) -> f64 {
    (n * 10.0).round() / 10.0
}

// ---- /proc parsing (pure) -------------------------------------------------------------

/// (utime + stime, cutime + cstime) from the text of `/proc/<pid>/stat`. The command name is
/// parenthesised and may itself contain spaces and parentheses, so fields are counted after
/// the LAST `)`.
pub fn parse_stat_ticks(stat: &str) -> Option<(u64, u64)> {
    let rest = &stat[stat.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    // After the name: state(0) ppid pgrp session tty tpgid flags minflt cminflt majflt
    // cmajflt utime(11) stime(12) cutime(13) cstime(14)
    let n = |i: usize| f.get(i)?.parse::<u64>().ok();
    Some((n(11)?.checked_add(n(12)?)?, n(13)?.checked_add(n(14)?)?))
}

/// voluntary + nonvoluntary context switches from `/proc/<pid>/task/<tid>/status`.
pub fn parse_ctxt_switches(status: &str) -> u64 {
    status
        .lines()
        .filter(|l| {
            l.starts_with("voluntary_ctxt_switches:")
                || l.starts_with("nonvoluntary_ctxt_switches:")
        })
        .filter_map(|l| l.split(':').nth(1)?.trim().parse::<u64>().ok())
        .sum()
}

/// The `Pss:` line of `/proc/<pid>/smaps_rollup`, in kB.
pub fn parse_pss_kb(rollup: &str) -> Option<u64> {
    rollup
        .lines()
        .find_map(|l| l.strip_prefix("Pss:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[cfg(target_os = "linux")]
pub use linux::{read_input, ticks_per_second, ProcFinder};

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{fs, os::unix::fs::MetadataExt};

    pub fn ticks_per_second() -> f64 {
        // SAFETY: sysconf has no preconditions.
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        if hz > 0 {
            hz as f64
        } else {
            100.0
        }
    }

    fn read_proc(pid: u32) -> Option<ProcRead> {
        let (own_ticks, child_ticks) =
            parse_stat_ticks(&fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)?;
        let mut threads = Vec::new();
        for entry in fs::read_dir(format!("/proc/{pid}/task")).ok()?.flatten() {
            let Some(tid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            if let Ok(status) = fs::read_to_string(entry.path().join("status")) {
                threads.push((tid, parse_ctxt_switches(&status)));
            }
        }
        let pss_kb = fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))
            .ok()
            .and_then(|text| parse_pss_kb(&text));
        Some(ProcRead {
            pid,
            own_ticks,
            child_ticks,
            pss_kb,
            threads,
        })
    }

    /// `root` and every descendant, walked through `task/*/children`.
    fn descendants(root: u32) -> Vec<u32> {
        let mut found = vec![root];
        let mut next = 0;
        while let Some(&pid) = found.get(next) {
            next += 1;
            let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) else {
                continue;
            };
            for task in tasks.flatten() {
                let Ok(text) = fs::read_to_string(task.path().join("children")) else {
                    continue;
                };
                for child in text
                    .split_whitespace()
                    .filter_map(|s| s.parse::<u32>().ok())
                {
                    if !found.contains(&child) {
                        found.push(child);
                    }
                }
            }
        }
        found
    }

    /// Finds our own `plasmashell` / `kwin_wayland` by command name, caching the pid and
    /// rescanning `/proc` only when it vanished (and at most every 10 s).
    #[derive(Default)]
    pub struct ProcFinder {
        cached: HashMap<&'static str, u32>,
        scanned: Option<Instant>,
    }

    impl ProcFinder {
        pub fn find(&mut self, name: &'static str) -> Option<u32> {
            let alive = |pid: u32| {
                fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.trim() == name)
            };
            if let Some(&pid) = self.cached.get(name) {
                if alive(pid) {
                    return Some(pid);
                }
                self.cached.remove(name);
            }
            if self
                .scanned
                .is_some_and(|t| t.elapsed() < Duration::from_secs(10))
            {
                return None;
            }
            self.scanned = Some(Instant::now());
            // SAFETY: geteuid has no preconditions.
            let uid = unsafe { libc::geteuid() };
            for entry in fs::read_dir("/proc").ok()?.flatten() {
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|s| s.parse::<u32>().ok())
                else {
                    continue;
                };
                if entry.metadata().is_ok_and(|m| m.uid() == uid) && alive(pid) {
                    self.cached.insert(name, pid);
                    return Some(pid);
                }
            }
            None
        }
    }

    fn ticks_of(pid: u32) -> Option<(u32, u64)> {
        let (own, _) = parse_stat_ticks(&fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)?;
        Some((pid, own))
    }

    pub fn read_input(root: u32, finder: &mut ProcFinder) -> CostInput {
        CostInput {
            tree: descendants(root)
                .into_iter()
                .filter_map(read_proc)
                .collect(),
            plasmashell: finder.find("plasmashell").and_then(ticks_of),
            kwin: finder.find("kwin_wayland").and_then(ticks_of),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HZ: f64 = 100.0;

    fn proc(pid: u32, own: u64, kids: u64, threads: &[(u32, u64)]) -> ProcRead {
        ProcRead {
            pid,
            own_ticks: own,
            child_ticks: kids,
            pss_kb: Some(1000),
            threads: threads.to_vec(),
        }
    }

    fn input(tree: Vec<ProcRead>) -> CostInput {
        CostInput {
            tree,
            ..CostInput::default()
        }
    }

    #[test]
    fn stat_parsing_survives_odd_command_names() {
        // comm contains spaces and a ')' ; utime=7 stime=3 cutime=4 cstime=1
        let stat = "42 (we ird) name) S 1 1 1 0 -1 4194560 100 0 0 0 7 3 4 1 20 0 5 0 99 1000 50";
        assert_eq!(parse_stat_ticks(stat), Some((10, 5)));
        assert_eq!(parse_stat_ticks("garbage"), None);
        assert_eq!(parse_stat_ticks("1 (x) S 1 2"), None);
    }

    #[test]
    fn status_and_rollup_parsing() {
        let status = "Name:\tx\nvoluntary_ctxt_switches:\t120\nnonvoluntary_ctxt_switches:\t5\n";
        assert_eq!(parse_ctxt_switches(status), 125);
        assert_eq!(parse_ctxt_switches("Name:\tx\n"), 0);
        assert_eq!(
            parse_pss_kb("Rss: 9 kB\nPss:   2048 kB\nPss_Anon: 1 kB\n"),
            Some(2048)
        );
        assert_eq!(parse_pss_kb("Rss: 9 kB\n"), None);
    }

    #[test]
    fn tally_baseline_then_growth_with_births_and_deaths() {
        let mut t = Tally::default();
        t.advance([(1u32, 100), (2, 50)]);
        assert_eq!(t.total(), 0, "the first call is only a baseline");
        t.advance([(1, 130), (2, 50)]);
        assert_eq!(t.total(), 30);
        // 2 exits (its past stays counted), 3 is born with 7 ticks already used.
        t.advance([(1, 140), (3, 7)]);
        assert_eq!(t.total(), 30 + 10 + 7);
        // A counter that went backwards is a reused id: it restarts from zero.
        t.advance([(1, 5), (3, 7)]);
        assert_eq!(t.total(), 30 + 10 + 7 + 5);
    }

    #[test]
    fn cpu_percent_is_the_average_over_the_window() {
        let mut s = CostSampler::default();
        let t0 = Instant::now();
        let first = s.sample(t0, &input(vec![proc(1, 1000, 0, &[(1, 10)])]), HZ);
        assert_eq!(
            (first.cpu, first.wakeups),
            (None, None),
            "nothing to average yet"
        );
        // 50 ticks in 5 s at 100 Hz = 10% of a core.
        let r = s.sample(
            t0 + Duration::from_secs(5),
            &input(vec![proc(1, 1050, 0, &[(1, 60)])]),
            HZ,
        );
        assert_eq!(r.cpu, Some(10.0));
        assert_eq!(r.wakeups, Some(10.0));
        assert_eq!(r.window_secs, 5.0);
        assert_eq!(r.processes, 1);
        assert_eq!(r.threads, 1);
    }

    #[test]
    fn old_samples_fall_out_of_the_ten_second_window() {
        let mut s = CostSampler::default();
        let t0 = Instant::now();
        let mut ticks = 0;
        let mut last = None;
        // Busy (100%) for the first 10 s, then idle: the reading must forget the busy part.
        for sec in 0..=30u64 {
            if sec <= 10 {
                ticks += 100;
            }
            last = Some(s.sample(
                t0 + Duration::from_secs(sec),
                &input(vec![proc(1, ticks, 0, &[])]),
                HZ,
            ));
            if sec == 10 {
                assert_eq!(last.as_ref().and_then(|r| r.cpu), Some(100.0));
            }
        }
        let r = last.unwrap();
        assert_eq!(r.cpu, Some(0.0));
        assert!(
            (9.0..=11.0).contains(&r.window_secs),
            "window {}",
            r.window_secs
        );
    }

    #[test]
    fn processes_joining_and_leaving_do_not_spike_or_hide_cost() {
        let mut s = CostSampler::default();
        let t0 = Instant::now();
        s.sample(
            t0,
            &input(vec![proc(1, 100, 0, &[]), proc(2, 100, 0, &[])]),
            HZ,
        );
        // Process 2 exits; a new helper 3 appears having used 20 ticks; 1 used 30.
        let r = s.sample(
            t0 + Duration::from_secs(2),
            &input(vec![proc(1, 130, 0, &[]), proc(3, 20, 0, &[])]),
            HZ,
        );
        assert_eq!(r.cpu, Some(25.0), "(30 + 20) ticks in 2 s");
        // A reaped child moves into its parent's cutime; only the growth counts.
        let r = s.sample(
            t0 + Duration::from_secs(4),
            &input(vec![proc(1, 130, 8, &[]), proc(3, 20, 0, &[])]),
            HZ,
        );
        assert_eq!(r.cpu, Some(14.5), "(30 + 20 + 8) ticks in 4 s");
    }

    #[test]
    fn a_long_pause_starts_a_fresh_window() {
        let mut s = CostSampler::default();
        let t0 = Instant::now();
        s.sample(t0, &input(vec![proc(1, 0, 0, &[])]), HZ);
        s.sample(
            t0 + Duration::from_secs(2),
            &input(vec![proc(1, 200, 0, &[])]),
            HZ,
        );
        // The view was hidden for a minute (a lot of ticks passed): no average over the gap.
        let back = s.sample(
            t0 + Duration::from_secs(62),
            &input(vec![proc(1, 5000, 0, &[])]),
            HZ,
        );
        assert_eq!(back.cpu, None);
        let next = s.sample(
            t0 + Duration::from_secs(64),
            &input(vec![proc(1, 5020, 0, &[])]),
            HZ,
        );
        assert_eq!(next.cpu, Some(10.0));
        assert_eq!(next.window_secs, 2.0);
    }

    #[test]
    fn pss_sums_the_tree_and_needs_every_member() {
        let mut s = CostSampler::default();
        let t0 = Instant::now();
        let r = s.sample(t0, &input(vec![proc(1, 0, 0, &[]), proc(2, 0, 0, &[])]), HZ);
        assert_eq!(r.pss_bytes, Some(2000.0 * 1024.0));
        let mut hidden = proc(2, 0, 0, &[]);
        hidden.pss_kb = None;
        let r = s.sample(
            t0 + Duration::from_secs(1),
            &input(vec![proc(1, 0, 0, &[]), hidden]),
            HZ,
        );
        assert_eq!(
            r.pss_bytes, None,
            "a partial sum would under-report, so report nothing"
        );
    }

    #[test]
    fn plasma_and_kwin_are_independent_and_optional() {
        let mut s = CostSampler::default();
        let t0 = Instant::now();
        let mk = |p: Option<u64>, k: Option<u64>| CostInput {
            tree: vec![proc(1, 0, 0, &[])],
            plasmashell: p.map(|t| (10, t)),
            kwin: k.map(|t| (11, t)),
        };
        s.sample(t0, &mk(Some(0), None), HZ);
        let r = s.sample(t0 + Duration::from_secs(4), &mk(Some(8), None), HZ);
        assert_eq!(r.plasmashell_cpu, Some(2.0));
        assert_eq!(r.kwin_cpu, None, "not running: unavailable, not zero");
    }

    #[test]
    fn an_empty_tree_reports_nothing() {
        let mut s = CostSampler::default();
        let t0 = Instant::now();
        s.sample(t0, &input(vec![]), HZ);
        let r = s.sample(t0 + Duration::from_secs(2), &input(vec![]), HZ);
        assert_eq!((r.cpu, r.wakeups, r.processes), (None, None, 0));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reads_this_very_process_from_proc() {
        let mut finder = ProcFinder::default();
        let read = read_input(std::process::id(), &mut finder);
        let me = read.tree.first().expect("own process is readable");
        assert_eq!(me.pid, std::process::id());
        assert!(!me.threads.is_empty());
        assert!(
            me.pss_kb.unwrap_or(0) > 0,
            "smaps_rollup of our own process"
        );
        assert!(ticks_per_second() >= 1.0);
    }
}
