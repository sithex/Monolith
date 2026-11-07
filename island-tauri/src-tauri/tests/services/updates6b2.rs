#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]
use crate::control_fakes;
use control_fakes::Fakes;
use hermes_island_lib::{
    contract::{
        controls::{UpdatesChanged, UpdatesOp},
        system::PowerState,
    },
    integrations::{
        control_process::Runner,
        updates::{
            core::{Backend, Unsupported},
            UpdatesService,
        },
    },
    state::machine::Clock,
};
use serde_json::{json, Value};
use std::{
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
struct FakeClock(AtomicU64);
impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst) * 1000
    }
}
struct Service(Arc<UpdatesService>);
impl Drop for Service {
    fn drop(&mut self) {
        self.0.shutdown().unwrap();
    }
}
async fn wait(service: &UpdatesService, probe: impl Fn(&UpdatesChanged) -> bool) -> UpdatesChanged {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let current = service.current();
        if probe(&current) {
            return current;
        }
        assert!(Instant::now() < until, "updates condition {current:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
fn start(
    backend: Backend,
    runner: Runner,
    clock: Arc<FakeClock>,
    power: tokio::sync::watch::Receiver<Option<PowerState>>,
) -> Arc<UpdatesService> {
    UpdatesService::start_with_backend(backend, runner, clock, power, Default::default())
}
fn pacman(
    runner: Runner,
    clock: Arc<FakeClock>,
    power: tokio::sync::watch::Receiver<Option<PowerState>>,
) -> Arc<UpdatesService> {
    start(Backend::Pacman, runner, clock, power)
}
fn power(battery: bool) -> PowerState {
    PowerState {
        rev: 0,
        available: true,
        present: true,
        percent: None,
        state: "unknown".into(),
        charging: false,
        on_battery: Some(battery),
        time_to_empty: None,
        time_to_full: None,
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_private_read_only_argv_exit_codes_schedule_and_recovery() {
    let f = Fakes::new("updates-main");
    let clock = Arc::new(FakeClock(AtomicU64::new(1000)));
    let (power, rx) = tokio::sync::watch::channel(Some(power(false)));
    let s = Service(pacman(f.runner(), clock.clone(), rx));
    let first = wait(&s.0, |v| v.available).await;
    assert_eq!(first.count, Some(1));
    assert!(first.needs_reboot);
    assert_eq!(
        f.calls().iter().map(|v| v[1].clone()).collect::<Vec<_>>(),
        vec![json!([]), json!(["-Qua"])]
    );
    // Nov 2 controller amendment: a user Refresh runs a real read-only check, rate
    // limited to one per 15 s; the hourly schedule and backoff still drive idle checks.
    let count = f.calls().len();
    s.0.request(UpdatesOp::Refresh).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        count,
        f.calls().len(),
        "refresh inside 15 s of a check is a no-op"
    );
    clock.0.store(4599, Ordering::SeqCst);
    s.0.request(UpdatesOp::Refresh).await.unwrap();
    for _ in 0..100 {
        if f.calls().len() >= count + 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        count + 2,
        f.calls().len(),
        "refresh forces exactly one check"
    );
    let mut state = f.state();
    state["checkupdates"] = json!({"code":2,"stdout":""});
    f.set(state);
    clock.0.store(4620, Ordering::SeqCst);
    s.0.request(UpdatesOp::Refresh).await.unwrap();
    wait(&s.0, |v| v.available && v.count == Some(0)).await;
    let mut state = f.state();
    state["paru"] = json!({"code":1,"stderr":"fixture-unavailable","stdout":""});
    f.set(state);
    clock.0.store(8200, Ordering::SeqCst);
    s.0.request(UpdatesOp::Refresh).await.unwrap();
    let bad = wait(&s.0, |v| !v.available).await;
    assert!(bad.error.is_some());
    assert_eq!(bad.count, Some(0), "last-good count retained with error");
    let count = f.calls().len();
    clock.0.store(8205, Ordering::SeqCst);
    s.0.request(UpdatesOp::Refresh).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        count,
        f.calls().len(),
        "rate limit also holds after a failure"
    );
    let mut state = f.state();
    state["paru"] = json!({"code":1,"stdout":"fixture-aur 1 -> 2\n","stderr":""});
    f.set(state);
    clock.0.store(8800, Ordering::SeqCst);
    s.0.request(UpdatesOp::Refresh).await.unwrap();
    wait(&s.0, |v| v.available && v.count == Some(1)).await;
    power.send_replace(Some(self::power(true)));
    clock.0.store(30401, Ordering::SeqCst);
    s.0.request(UpdatesOp::Refresh).await.unwrap();
    wait(&s.0, |v| v.age_secs == Some(0.)).await;
    assert!(f
        .calls()
        .iter()
        .all(|v| (v[0] == "checkupdates" && v[1] == json!([]))
            || (v[0] == "paru" && v[1] == json!(["-Qua"]))));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_unknown_is_null_never_zero_and_missing_aur_is_not_empty() {
    let f = Fakes::new("updates-unknown");
    let mut state = f.state();
    state["checkupdates"] = json!({"code":7});
    state["paru"] = json!({"code":127});
    f.set(state);
    let clock = Arc::new(FakeClock(AtomicU64::new(1000)));
    let (_power, rx) = tokio::sync::watch::channel(Some(power(false)));
    let s = Service(pacman(f.runner(), clock, rx));
    let value = wait(&s.0, |v| {
        v.detail == "checks-unavailable" && v.next_check_in_secs > 0.
    })
    .await;
    assert!(!value.available);
    assert_eq!(value.count, None);
    assert_eq!(value.aur_count, None);
    assert!(value.stale);
}

/// Fake `dnf` and `rpm` for the private fixture: they log calls like the shared fakes, accept only
/// the read-only argv shapes, and answer from `dnf-state.json`. rpm prints one line per installed
/// version of each requested key and "package X is not installed" otherwise, like real rpm.
const DNF_FAKE: &str = r#"#!/usr/bin/python3
import fcntl, json, os, sys
from pathlib import Path
root = Path(os.environ['HERMES_ISLAND_CONTROL_FIXTURE']).resolve()
program, args = Path(sys.argv[0]).name, sys.argv[1:]
with (root / 'calls.jsonl').open('a') as calls:
    fcntl.flock(calls, fcntl.LOCK_EX)
    calls.write(json.dumps([program, args, os.environ.get('LC_ALL'), os.environ['HOME']]) + '\n')
state = json.loads((root / 'dnf-state.json').read_text())
if program == 'dnf':
    assert args in (['-q', 'check-upgrade', '--json'], ['-q', '-C', 'check-upgrade', '--json']), args
    result = state['dnf']
    sys.stdout.write(result.get('stdout', ''))
    sys.stderr.write(result.get('stderr', ''))
    sys.exit(result.get('code', 0))
assert program == 'rpm' and args[:4] == ['-q', '--qf', '%{NAME}.%{ARCH}\t%{EVR}\n', '--'], args
misses = 0
for key in args[4:]:
    found = state['installed'].get(key, [])
    for evr in found:
        sys.stdout.write(f'{key}\t{evr}\n')
    if not found:
        sys.stdout.write(f'package {key} is not installed\n')
        misses += 1
sys.exit(state.get('rpmCode', misses))
"#;
const UPGRADES_27: &str = include_str!("../fixtures/updates/dnf-check-upgrade-27.json");
const RPM_27: &str = include_str!("../fixtures/updates/rpm-qf-27.txt");
const OFFLINE: &str = include_str!("../fixtures/updates/dnf-check-upgrade-offline.stderr");
fn dnf_fakes(tag: &str, dnf: Value) -> Fakes {
    let f = Fakes::new(tag);
    for name in ["dnf", "rpm"] {
        let path = f.root.join("bin").join(name);
        std::fs::write(&path, DNF_FAKE).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut installed = serde_json::Map::new();
    for line in RPM_27.lines() {
        let (key, evr) = line.split_once('\t').unwrap();
        installed.insert(key.into(), json!([evr]));
    }
    set_dnf(&f, json!({"dnf": dnf, "installed": installed}));
    f
}
fn set_dnf(f: &Fakes, state: Value) {
    let tmp = f.root.join("new-dnf-state.json");
    std::fs::write(&tmp, state.to_string()).unwrap();
    std::fs::rename(tmp, f.root.join("dnf-state.json")).unwrap();
}
fn dnf_state(f: &Fakes) -> Value {
    serde_json::from_str(&std::fs::read_to_string(f.root.join("dnf-state.json")).unwrap()).unwrap()
}
fn args_of(f: &Fakes, program: &str) -> Vec<Vec<String>> {
    f.calls()
        .iter()
        .filter(|c| c[0] == program)
        .map(|c| serde_json::from_value(c[1].clone()).unwrap())
        .collect()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_dnf_real_fixture_counts_with_rpm_old_versions_and_no_aur() {
    let f = dnf_fakes("dnf-main", json!({"code":0,"stdout":UPGRADES_27}));
    let clock = Arc::new(FakeClock(AtomicU64::new(1000)));
    let (_power, rx) = tokio::sync::watch::channel(Some(power(false)));
    let s = Service(start(Backend::Dnf, f.runner(), clock, rx));
    let v = wait(&s.0, |v| v.available).await;
    assert_eq!(serde_json::to_value(v.backend).unwrap(), json!("dnf"));
    assert_eq!(
        (v.count, v.repo_count, v.aur_count),
        (Some(27), Some(27), None)
    );
    assert!(
        v.needs_reboot,
        "glibc / mesa-* are on the Fedora reboot list"
    );
    assert_eq!(v.error, None);
    assert_eq!(v.items.len(), 27);
    assert!(v
        .items
        .iter()
        .all(|i| i.source == "dnf" && i.arch.is_some() && !i.old_version.is_empty()));
    let glibc: Vec<_> = v.items.iter().filter(|i| i.name == "glibc").collect();
    assert_eq!(glibc.len(), 2, "multilib: one row per name.arch");
    let xxd = v.items.iter().find(|i| i.name == "xxd").unwrap();
    assert_eq!(
        (xxd.old_version.as_str(), xxd.new_version.as_str()),
        ("2:9.2.1129-1.fc45", "2:9.2.1202-1.fc45")
    );
    // Exactly one dnf call (no -C on AC power) and one batched rpm query; no pacman tools.
    assert_eq!(
        args_of(&f, "dnf"),
        vec![vec!["-q", "check-upgrade", "--json"]]
    );
    let rpm = args_of(&f, "rpm");
    assert_eq!(rpm.len(), 1);
    assert_eq!(rpm[0].len(), 4 + 27);
    assert!(f.calls().iter().all(|c| c[0] == "dnf" || c[0] == "rpm"));
    assert!(
        f.calls().iter().all(|c| c[2] == "C"),
        "LC_ALL=C for parsing"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_dnf_failure_is_an_error_never_zero_and_recovers() {
    let f = dnf_fakes("dnf-fail", json!({"code":1,"stdout":"","stderr":OFFLINE}));
    let clock = Arc::new(FakeClock(AtomicU64::new(1000)));
    let (_power, rx) = tokio::sync::watch::channel(Some(power(false)));
    let s = Service(start(Backend::Dnf, f.runner(), clock.clone(), rx));
    let bad = wait(&s.0, |v| v.next_check_in_secs > 0.).await;
    assert!(!bad.available);
    assert_eq!(bad.detail, "checks-unavailable");
    assert_eq!(
        (bad.count, bad.repo_count, bad.aur_count),
        (None, None, None)
    );
    assert_eq!(
        bad.error.as_deref(),
        Some("dnf: command reported a failure")
    );
    assert!(bad.stale && bad.items.is_empty());
    assert!(
        args_of(&f, "rpm").is_empty(),
        "no rpm query after a failed dnf"
    );
    // `{}` = known zero.
    let mut state = dnf_state(&f);
    state["dnf"] = json!({"code":0,"stdout":"{}\n"});
    set_dnf(&f, state);
    clock.0.store(1100, Ordering::SeqCst);
    s.0.request(UpdatesOp::Refresh).await.unwrap();
    let ok = wait(&s.0, |v| v.available).await;
    assert_eq!(
        (ok.count, ok.repo_count, ok.aur_count, ok.error),
        (Some(0), Some(0), None, None)
    );
    // Exit 0 with garbage is an error, and the last good zero is kept (stale-but-known).
    let mut state = dnf_state(&f);
    state["dnf"] = json!({"code":0,"stdout":"Updating and loading repositories:\n"});
    set_dnf(&f, state);
    clock.0.store(1200, Ordering::SeqCst);
    s.0.request(UpdatesOp::Refresh).await.unwrap();
    let garbage = wait(&s.0, |v| !v.available).await;
    assert_eq!(
        garbage.error.as_deref(),
        Some("dnf: output was not the expected JSON")
    );
    assert_eq!(garbage.count, Some(0));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_dnf_on_battery_reads_the_cache_but_a_user_refresh_does_not() {
    let f = dnf_fakes("dnf-battery", json!({"code":0,"stdout":"{}\n"}));
    let clock = Arc::new(FakeClock(AtomicU64::new(1000)));
    let (_power, rx) = tokio::sync::watch::channel(Some(power(true)));
    let s = Service(start(Backend::Dnf, f.runner(), clock.clone(), rx));
    wait(&s.0, |v| v.available).await;
    assert_eq!(
        args_of(&f, "dnf"),
        vec![vec!["-q", "-C", "check-upgrade", "--json"]]
    );
    clock.0.store(1020, Ordering::SeqCst);
    s.0.request(UpdatesOp::Refresh).await.unwrap();
    wait(&s.0, |v| {
        v.age_secs == Some(0.) && v.next_check_in_secs > 0.
    })
    .await;
    for _ in 0..100 {
        if args_of(&f, "dnf").len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(args_of(&f, "dnf")[1], vec!["-q", "check-upgrade", "--json"]);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_dnf_rpm_problems_leave_old_versions_empty_but_items_listed() {
    // rpm knows only some packages, then rpm is missing altogether.
    let f = dnf_fakes("dnf-rpm", json!({"code":0,"stdout":UPGRADES_27}));
    let mut state = dnf_state(&f);
    state["installed"] = json!({"xxd.x86_64": ["2:9.2.1129-1.fc45"]});
    set_dnf(&f, state);
    let clock = Arc::new(FakeClock(AtomicU64::new(1000)));
    let (_power, rx) = tokio::sync::watch::channel(Some(power(false)));
    let s = Service(start(Backend::Dnf, f.runner(), clock.clone(), rx));
    let v = wait(&s.0, |v| v.available).await;
    assert_eq!(v.count, Some(27));
    assert_eq!(
        v.items.iter().filter(|i| i.old_version.is_empty()).count(),
        26
    );
    std::fs::remove_file(f.root.join("bin/rpm")).unwrap();
    clock.0.store(1100, Ordering::SeqCst);
    s.0.request(UpdatesOp::Refresh).await.unwrap();
    let v = wait(&s.0, |v| {
        v.available && v.age_secs == Some(0.) && v.items.iter().all(|i| i.old_version.is_empty())
    })
    .await;
    assert_eq!((v.count, v.error), (Some(27), None));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_dnf_without_a_runner_fails_and_schedules_a_retry() {
    let clock = Arc::new(FakeClock(AtomicU64::new(1000)));
    let (_power, rx) = tokio::sync::watch::channel(Some(power(false)));
    let s = Service(start(
        Backend::Dnf,
        Runner::for_instance(true, None),
        clock,
        rx,
    ));
    let v = wait(&s.0, |v| v.next_check_in_secs > 0.).await;
    assert!(!v.available && v.count.is_none() && v.error.is_some());
    assert_eq!(v.next_check_in_secs, 600., "first failure backs off 10 min");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_pacman_missing_paru_only_hides_the_aur_half() {
    let f = Fakes::new("updates-noparu");
    std::fs::remove_file(f.root.join("bin/paru")).unwrap();
    let clock = Arc::new(FakeClock(AtomicU64::new(1000)));
    let (_power, rx) = tokio::sync::watch::channel(Some(power(false)));
    let s = Service(pacman(f.runner(), clock, rx));
    let v = wait(&s.0, |v| v.next_check_in_secs > 0.).await;
    assert!(v.available, "checkupdates worked: {v:?}");
    assert_eq!(serde_json::to_value(v.backend).unwrap(), json!("pacman"));
    assert_eq!(
        (v.count, v.repo_count, v.aur_count),
        (Some(1), Some(1), None)
    );
    assert_eq!(v.error, None);
    assert!(v.needs_reboot && v.items[0].name == "linux" && v.items[0].source == "repo");
    assert_eq!(v.items[0].arch, None);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_unsupported_system_runs_nothing_and_never_retries() {
    let f = dnf_fakes("unsupported", json!({"code":0,"stdout":UPGRADES_27}));
    let clock = Arc::new(FakeClock(AtomicU64::new(1000)));
    let (_power, rx) = tokio::sync::watch::channel(Some(power(false)));
    for reason in [Unsupported::OtherDistro, Unsupported::ImageBased] {
        let s = Service(start(
            Backend::Unsupported(reason),
            f.runner(),
            clock.clone(),
            rx.clone(),
        ));
        let first = s.0.current();
        assert_eq!(
            serde_json::to_value(first.backend).unwrap(),
            json!("unsupported")
        );
        assert_eq!(first.detail, "unsupported-system");
        assert!(!first.available && !first.stale && !first.needs_reboot);
        assert_eq!(
            (first.count, first.error.clone(), first.next_check_in_secs),
            (None, None, 0.)
        );
        let answered = s.0.request(UpdatesOp::Refresh).await.unwrap();
        assert_eq!(answered, first);
        clock.0.fetch_add(100_000, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            s.0.current(),
            first,
            "no schedule, no backoff, no new revision"
        );
    }
    assert!(f.calls().is_empty(), "nothing was run: {:?}", f.calls());
}
