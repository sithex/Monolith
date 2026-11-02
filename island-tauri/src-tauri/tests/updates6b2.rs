#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]
#[path = "helpers/control_fakes.rs"]
mod control_fakes;
use control_fakes::Fakes;
use hermes_island_lib::{
    contract::{
        controls::{UpdatesChanged, UpdatesOp},
        system::PowerState,
    },
    integrations::updates::UpdatesService,
    state::machine::Clock,
};
use serde_json::json;
use std::{
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
    let s = Service(UpdatesService::start(f.runner(), clock.clone(), rx));
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
    let s = Service(UpdatesService::start(f.runner(), clock, rx));
    let value = wait(&s.0, |v| {
        v.detail == "checks-unavailable" && v.next_check_in_secs > 0.
    })
    .await;
    assert!(!value.available);
    assert_eq!(value.count, None);
    assert_eq!(value.aur_count, None);
    assert!(value.stale);
}
