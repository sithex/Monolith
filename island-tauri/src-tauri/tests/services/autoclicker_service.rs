#![allow(clippy::unwrap_used)]
//! Autoclicker runs in a test instance: no /dev/uinput and no KDE shortcuts, clicks are only
//! counted. Checks bursts, hold-down, the click limit, stats and the notch signal.
use hermes_island_lib::{
    contract::apps::{AutoclickerChanged, AutoclickerOp, ClickKind, TriggerMode},
    integrations::autoclicker::Autoclicker,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static SERIAL: AtomicU64 = AtomicU64::new(0);

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hi-autoclicker-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn configure(
    a: &Autoclicker,
    f: impl FnOnce(&mut hermes_island_lib::contract::apps::AutoclickerConfig),
) {
    let mut c = a.current().unwrap().config;
    c.start_delay_s = 0;
    c.interval_ms = 1;
    f(&mut c);
    a.command(AutoclickerOp::Configure { config: c }).unwrap();
}

fn until_idle(a: &Autoclicker) -> AutoclickerChanged {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let m = a.current().unwrap();
        if !m.running {
            return m;
        }
        assert!(Instant::now() < deadline, "run never finished");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_burst_sends_its_count_and_the_totals_survive_a_restart() {
    let dir = Dir::new();
    let a = Autoclicker::new(&dir.0, true);
    configure(&a, |c| {
        c.mode = TriggerMode::Burst;
        c.burst_count = 7;
    });
    let mut notch = a.running();
    assert!(!*notch.borrow_and_update());
    a.command(AutoclickerOp::Start {}).unwrap();
    assert!(notch.has_changed().unwrap(), "the notch hears the start");
    let m = until_idle(&a);
    assert_eq!(m.clicks, 7.0);
    assert_eq!(m.lifetime_clicks, 7.0);
    assert!(!*notch.borrow_and_update(), "and the stop");
    // A click limit below the burst wins.
    configure(&a, |c| c.click_limit = 3);
    a.command(AutoclickerOp::Start {}).unwrap();
    assert_eq!(until_idle(&a).clicks, 3.0);
    a.shutdown();
    drop(a);
    let again = Autoclicker::new(&dir.0, true);
    let m = again.current().unwrap();
    assert_eq!(m.lifetime_clicks, 10.0, "saved with the settings");
    assert_eq!(m.config.mode, TriggerMode::Burst);
    assert_eq!(m.clicks, 0.0, "the last run is not saved");
    let m = again.command(AutoclickerOp::ResetStats {}).unwrap();
    assert_eq!((m.lifetime_clicks, m.lifetime_ms), (0.0, 0.0));
}

#[test]
fn hold_down_counts_one_press_and_runs_until_stopped() {
    let dir = Dir::new();
    let a = Autoclicker::new(&dir.0, true);
    configure(&a, |c| c.kind = ClickKind::Hold);
    a.command(AutoclickerOp::Start {}).unwrap();
    std::thread::sleep(Duration::from_millis(120));
    let m = a.current().unwrap();
    assert!(m.running && !m.starting);
    assert_eq!(m.clicks, 1.0);
    assert!(
        m.session_ms >= 100.0,
        "the run time ticks: {}",
        m.session_ms
    );
    a.command(AutoclickerOp::Stop {}).unwrap();
    let m = until_idle(&a);
    assert_eq!(m.clicks, 1.0);
    assert!(m.session_ms >= 100.0, "the last run's time stays on show");
    let deadline = Instant::now() + Duration::from_secs(2);
    while a.current().unwrap().lifetime_ms == 0.0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        a.current().unwrap().lifetime_ms >= 100.0,
        "the totals follow once the run ends"
    );
}
