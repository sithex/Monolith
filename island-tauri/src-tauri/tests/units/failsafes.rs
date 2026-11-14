//! Failsafes: pumps that survive a full actor queue and failing emits, supervised restarts,
//! a startup that degrades instead of aborting, and a quit with one deadline.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use hermes_island_lib::{
    commands::{island::optional_parts, managed},
    contract::settings::Settings,
    error::{ErrorCode, IslandError},
    integrations::{
        notifications::{
            hub::{HubOutlet, Notifications, Outbound},
            NotificationInput, NotificationSource, Urgency,
        },
        pump::{self, Backoff, Delivery, PumpHealth, RestartPolicy},
    },
    services::{
        autostart::Autostart,
        degraded::{self, Degraded},
        settings::SettingsService,
        shutdown::{self, BoundedThread, JoinOutcome, Step, StepOutcome},
    },
    state::{
        actor,
        machine::{Clock, Effect, Intent},
    },
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::watch;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "hi-failsafes-{name}-{}-{:?}",
        std::process::id(),
        Instant::now()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wait_for(what: &str, limit: Duration, mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + limit;
    while !ready() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// No restarts: a pump that ends on an error stays down, so these tests see the pump's
/// own resilience rather than the supervisor papering over it.
fn no_restarts() -> RestartPolicy {
    RestartPolicy {
        max_restarts: 0,
        backoff: Backoff::new(Duration::from_millis(10), Duration::from_millis(10)),
        healthy_after: Duration::from_secs(60),
    }
}

// ---------------------------------------------------------------------------------------
// pump::deliver and pump::supervise

#[test]
fn busy_answers_are_retried_a_few_times_then_handed_back_without_spinning() {
    let (_stop, mut stopped) = watch::channel(false);
    let calls = AtomicU32::new(0);
    let started = Instant::now();
    let outcome = tauri::async_runtime::block_on(pump::deliver(
        || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(IslandError::new(ErrorCode::Busy, "full")) }
        },
        &mut stopped,
    ));
    assert!(matches!(outcome, Delivery::Busy));
    assert_eq!(calls.load(Ordering::SeqCst), 4, "one try plus three bounded retries");
    let took = started.elapsed();
    assert!(took >= Duration::from_millis(200), "retries wait between attempts: {took:?}");
    assert!(took < Duration::from_secs(2), "bounded: {took:?}");

    let calls = AtomicU32::new(0);
    let outcome = tauri::async_runtime::block_on(pump::deliver(
        || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            async move {
                if n < 2 {
                    Err(IslandError::new(ErrorCode::Busy, "full"))
                } else {
                    Ok(n)
                }
            }
        },
        &mut stopped,
    ));
    assert!(matches!(outcome, Delivery::Done(2)));

    let calls = AtomicU32::new(0);
    let outcome = tauri::async_runtime::block_on(pump::deliver(
        || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(IslandError::internal("effect failed")) }
        },
        &mut stopped,
    ));
    assert!(matches!(outcome, Delivery::Failed(e) if e.code == ErrorCode::Internal));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "only Busy is retried");
}

#[test]
fn supervisor_restarts_a_failing_pump_with_backoff_and_leaves_it_stopped_at_the_cap() {
    let (_stop, stopped) = watch::channel(false);
    let runs = Arc::new(AtomicU32::new(0));
    let health = Arc::new(Mutex::new(Vec::new()));
    let (r, h) = (runs.clone(), health.clone());
    let policy = RestartPolicy {
        max_restarts: 2,
        backoff: Backoff::new(Duration::from_millis(20), Duration::from_millis(40)),
        healthy_after: Duration::from_secs(60),
    };
    let started = Instant::now();
    tauri::async_runtime::block_on(pump::supervise(
        "test",
        policy,
        stopped,
        move || {
            r.fetch_add(1, Ordering::SeqCst);
            async { Err(IslandError::internal("pump broke")) }
        },
        move |state| h.lock().unwrap().push(state),
    ));
    use PumpHealth::*;
    assert_eq!(runs.load(Ordering::SeqCst), 3, "first run plus two restarts");
    assert_eq!(
        *health.lock().unwrap(),
        [Running, Restarting, Running, Restarting, Running, Stopped]
    );
    assert!(started.elapsed() >= Duration::from_millis(60), "20 ms then 40 ms between runs");
}

#[test]
fn supervisor_restarts_after_a_panic_and_ends_on_a_stop_request() {
    let (stop, stopped) = watch::channel(false);
    let runs = Arc::new(AtomicU32::new(0));
    let health = Arc::new(Mutex::new(Vec::new()));
    let (r, h, inner_stop) = (runs.clone(), health.clone(), stop.subscribe());
    let policy = RestartPolicy {
        max_restarts: 3,
        backoff: Backoff::new(Duration::from_millis(10), Duration::from_millis(10)),
        healthy_after: Duration::from_secs(60),
    };
    let task = std::thread::spawn(move || {
        tauri::async_runtime::block_on(pump::supervise(
            "test",
            policy,
            stopped,
            move || {
                let first = r.fetch_add(1, Ordering::SeqCst) == 0;
                let mut stop = inner_stop.clone();
                async move {
                    assert!(!first, "first run panics");
                    let _ = stop.wait_for(|s| *s).await;
                    Ok(())
                }
            },
            move |state| h.lock().unwrap().push(state),
        ))
    });
    wait_for("the restarted pump", Duration::from_secs(5), || runs.load(Ordering::SeqCst) == 2);
    stop.send_replace(true);
    task.join().unwrap();
    use PumpHealth::*;
    assert_eq!(*health.lock().unwrap(), [Running, Restarting, Running]);
}

#[test]
fn supervisor_does_not_restart_a_pump_that_finished_cleanly() {
    let (_stop, stopped) = watch::channel(false);
    let runs = Arc::new(AtomicU32::new(0));
    let r = runs.clone();
    tauri::async_runtime::block_on(pump::supervise(
        "test",
        RestartPolicy::standard(),
        stopped,
        move || {
            r.fetch_add(1, Ordering::SeqCst);
            async { Ok(()) }
        },
        |_| {},
    ));
    assert_eq!(runs.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------------------------------
// The notification hub under fault injection

struct Fixed;
impl Clock for Fixed {
    fn now_ms(&self) -> u64 {
        1000
    }
}

/// An effect sink that can hold the actor inside `apply`, so its bounded queue fills up.
#[derive(Clone, Default)]
struct Gate(Arc<(Mutex<bool>, Condvar)>);
impl Gate {
    fn close(&self) {
        *self.0 .0.lock().unwrap() = true;
    }
    fn open(&self) {
        *self.0 .0.lock().unwrap() = false;
        self.0 .1.notify_all();
    }
}
impl actor::Sink for Gate {
    fn apply(&mut self, _: &[Effect], _: &actor::Snapshot) -> Result<(), IslandError> {
        let (closed, wake) = &*self.0;
        let mut closed = closed.lock().unwrap();
        while *closed {
            closed = wake.wait(closed).unwrap();
        }
        Ok(())
    }
}

/// Records what the hub sends; can be told to fail the next N page events.
#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<Outbound>>,
    fail_next: AtomicU32,
    /// Fails the next N single-notification emits only (not popups or snapshots).
    fail_received: AtomicU32,
}
impl HubOutlet for Recorder {
    fn send(&self, event: Outbound) -> Result<(), IslandError> {
        if matches!(event, Outbound::Received(_)) {
            let left = self.fail_received.load(Ordering::SeqCst);
            if left > 0 {
                self.fail_received.store(left - 1, Ordering::SeqCst);
                return Err(IslandError::internal("injected emit failure"));
            }
        }
        if !matches!(event, Outbound::Status(_))
            && self
                .fail_next
                .load(Ordering::SeqCst)
                .checked_sub(1)
                .is_some_and(|left| {
                    self.fail_next.store(left, Ordering::SeqCst);
                    true
                })
        {
            return Err(IslandError::internal("injected emit failure"));
        }
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}
impl Recorder {
    fn received(&self, title: &str) -> bool {
        self.events.lock().unwrap().iter().any(|e| match e {
            Outbound::Received(n) => n.0.title == title,
            _ => false,
        })
    }
    fn snapshot_has(&self, title: &str) -> bool {
        self.events.lock().unwrap().iter().any(|e| match e {
            Outbound::Snapshot(s) => s.items.iter().any(|n| n.title == title),
            _ => false,
        })
    }
    fn last_status(&self) -> Option<(bool, String)> {
        self.events.lock().unwrap().iter().rev().find_map(|e| match e {
            Outbound::Status(s) => Some((s.available, s.detail.clone())),
            _ => None,
        })
    }
}

fn note(title: &str) -> NotificationInput {
    NotificationInput {
        source: NotificationSource::Hermes,
        app_name: "Test".into(),
        title: title.into(),
        body: None,
        icon_png: None,
        urgency: Urgency::Normal,
        actions: Vec::new(),
        progress: None,
        dedupe_key: None,
        server_id: None,
        replaces_id: 0,
        transient: false,
        ttl_ms: None,
    }
}

fn hub_fixture(
    name: &str,
    sink: impl actor::Sink,
) -> (actor::Handle, Arc<Recorder>, Notifications, PathBuf) {
    let root = scratch(name);
    let (handle, _task) = tauri::async_runtime::block_on(async {
        actor::spawn(Settings::default(), Arc::new(Fixed), sink)
    });
    let settings = Arc::new(SettingsService::new(
        root.clone(),
        Autostart::with_default_launcher(root.clone(), root.join("app"), true),
    ));
    let out = Arc::new(Recorder::default());
    let notifications = Notifications::start_with(
        out.clone(),
        handle.clone(),
        settings,
        &root,
        // Never the desktop's bus: the monitor and takeover must find nothing.
        "unix:path=/nonexistent/hi-failsafes-bus".into(),
        no_restarts(),
    )
    .unwrap();
    (handle, out, notifications, root)
}

#[test]
fn notification_pump_survives_a_full_actor_queue_and_delivers_afterwards() {
    let gate = Gate::default();
    let (handle, out, notifications, root) = hub_fixture("busy", gate.clone());
    wait_for("pump running", Duration::from_secs(5), || {
        out.last_status() == Some((true, "ready".into()))
    });
    // Hold the actor inside an effect and fill its bounded queue.
    gate.close();
    handle.try_dispatch(Intent::NotificationsSync(0)).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    while handle.try_dispatch(Intent::NotificationsSync(0)).is_ok() {}
    assert_eq!(
        handle.try_dispatch(Intent::NotificationsSync(0)).unwrap_err().code,
        ErrorCode::Busy
    );
    notifications.hub.insert(note("while busy")).unwrap();
    wait_for("delivery while busy", Duration::from_secs(5), || out.received("while busy"));
    // Long enough for every bounded retry to answer Busy at least once.
    std::thread::sleep(Duration::from_millis(600));
    gate.open();
    notifications.hub.insert(note("after busy")).unwrap();
    wait_for("delivery after the queue drained", Duration::from_secs(5), || {
        out.received("after busy")
    });
    wait_for("unread count reached the actor", Duration::from_secs(5), || {
        handle.state().unread == 2
    });
    assert_eq!(out.last_status(), Some((true, "ready".into())), "never reported down");
    notifications.shutdown().unwrap();
    handle.shutdown();
    let _ = std::fs::remove_dir_all(root);
}

struct Open;
impl actor::Sink for Open {
    fn apply(&mut self, _: &[Effect], _: &actor::Snapshot) -> Result<(), IslandError> {
        Ok(())
    }
}

#[test]
fn notification_pump_survives_failing_emits_and_resyncs_from_a_snapshot() {
    let (handle, out, notifications, root) = hub_fixture("emit", Open);
    wait_for("pump running", Duration::from_secs(5), || {
        out.last_status() == Some((true, "ready".into()))
    });
    out.fail_next.store(3, Ordering::SeqCst);
    notifications.hub.insert(note("lost emit")).unwrap();
    wait_for("the missed one arrives in a snapshot", Duration::from_secs(5), || {
        out.snapshot_has("lost emit")
    });
    assert_eq!(out.fail_next.load(Ordering::SeqCst), 0, "all injected failures were hit");
    // A failure in the per-notification emit itself (popups went through).
    out.fail_received.store(1, Ordering::SeqCst);
    notifications.hub.insert(note("lost received")).unwrap();
    wait_for("a failed single emit is resent in a snapshot", Duration::from_secs(5), || {
        out.snapshot_has("lost received")
    });
    assert_eq!(out.fail_received.load(Ordering::SeqCst), 0);
    notifications.hub.insert(note("next one")).unwrap();
    wait_for("the next one is delivered normally", Duration::from_secs(5), || {
        out.received("next one")
    });
    assert_eq!(out.last_status(), Some((true, "ready".into())));
    notifications.shutdown().unwrap();
    handle.shutdown();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn notifications_start_without_the_hermes_socket_and_say_so() {
    let root = scratch("ingest");
    // A directory where the socket should be: the bind fails, the island must not.
    std::fs::create_dir(root.join("ingest.sock")).unwrap();
    let (handle, _task) = tauri::async_runtime::block_on(async {
        actor::spawn(Settings::default(), Arc::new(Fixed), Open)
    });
    let settings = Arc::new(SettingsService::new(
        root.clone(),
        Autostart::with_default_launcher(root.clone(), root.join("app"), true),
    ));
    let out = Arc::new(Recorder::default());
    let notifications = Notifications::start_with(
        out.clone(),
        handle.clone(),
        settings,
        &root,
        "unix:path=/nonexistent/hi-failsafes-bus".into(),
        no_restarts(),
    )
    .expect("a missing ingest socket must not fail the start");
    wait_for("status says why", Duration::from_secs(5), || {
        out.last_status() == Some((false, "ingest-unavailable".into()))
    });
    notifications.hub.insert(note("desktop still works")).unwrap();
    wait_for("delivery", Duration::from_secs(5), || out.received("desktop still works"));
    notifications.shutdown().unwrap();
    handle.shutdown();
    let _ = std::fs::remove_dir_all(root);
}

// ---------------------------------------------------------------------------------------
// Startup degrades

#[test]
fn a_failing_optional_constructor_is_recorded_not_fatal() {
    let degraded = Degraded::default();
    let ok: Option<u32> = degraded::optional("ok", Ok::<_, IslandError>(7), &degraded);
    assert_eq!(ok, Some(7));
    let failed: Option<u32> = degraded::optional(
        "system-services",
        Err(IslandError::internal("Lyrics HTTP client unavailable")),
        &degraded,
    );
    assert_eq!(failed, None);
    assert_eq!(degraded.failed(), ["system-services"]);
    let status = degraded::start_failed("power");
    assert_eq!((status.available, status.detail.as_str()), (false, "start-failed"));
}

#[test]
fn missing_subsystems_boot_as_unavailable_and_their_commands_say_so() {
    let parts = optional_parts(None, None, None).unwrap();
    let caps = &parts.capabilities;
    assert!(!caps.notifications && !caps.clipboard && !caps.power && !caps.media);
    for status in [
        &caps.notifications_status,
        &caps.power_status,
        &caps.weather_status,
        &caps.bluetooth_status,
        &caps.media_status,
        &caps.lyrics_status,
        &caps.equalizer_status,
    ] {
        assert_eq!((status.available, status.detail.as_str()), (false, "start-failed"));
    }
    assert_eq!(caps.clipboard_status.detail, "start-failed");
    assert!(parts.notifications.is_empty() && parts.bluetooth.is_empty());
    assert!(parts.power.is_none() && parts.weather.is_none() && parts.media.is_none());
    // Phase-2 capabilities stay on: the terminal and settings never depend on them.
    assert!(caps.terminal && caps.settings);

    let error = managed::missing::<Notifications>();
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(error.message.starts_with("Notifications are unavailable"));
    let error = managed::missing::<
        Arc<hermes_island_lib::integrations::system_services::SystemServices>,
    >();
    assert_eq!(error.code, ErrorCode::Unavailable);
}

// ---------------------------------------------------------------------------------------
// Shutdown cannot hang

#[test]
fn a_worker_that_never_exits_is_left_behind_within_the_bound() {
    let (release, hold) = std::sync::mpsc::channel::<()>();
    let worker = BoundedThread::spawn("hi-test-stuck", move || {
        let _ = hold.recv();
    })
    .unwrap();
    let started = Instant::now();
    assert_eq!(worker.join_within(Duration::from_millis(150)), JoinOutcome::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(release);

    let quick = BoundedThread::spawn("hi-test-quick", || {}).unwrap();
    assert_eq!(quick.join_within(Duration::from_secs(5)), JoinOutcome::Joined);
    let panics = BoundedThread::spawn("hi-test-panics", || panic!("worker broke")).unwrap();
    assert_eq!(panics.join_within(Duration::from_secs(5)), JoinOutcome::Panicked);
}

#[test]
fn quit_runs_every_step_in_order_within_one_deadline_and_names_each_outcome() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let log = |name: &'static str| {
        let order = order.clone();
        move || order.lock().unwrap().push(name)
    };
    let (first, after_hang, last) = (log("first"), log("after-hang"), log("last"));
    let started = Instant::now();
    let outcomes = shutdown::run(
        vec![
            Step::new("first", Duration::from_secs(1), move || {
                first();
                Ok(())
            }),
            Step::new("fails", Duration::from_secs(1), || Err("socket gone".into())),
            Step::new("hangs", Duration::from_millis(200), || {
                std::thread::sleep(Duration::from_secs(30));
                Ok(())
            }),
            Step::new("panics", Duration::from_secs(1), || panic!("step broke")),
            Step::new("after-hang", Duration::from_secs(1), move || {
                after_hang();
                Ok(())
            }),
            Step::new("eats-the-rest", Duration::from_secs(30), || {
                std::thread::sleep(Duration::from_secs(30));
                Ok(())
            }),
            Step::new("last", Duration::from_secs(1), move || {
                last();
                Ok(())
            }),
        ],
        Duration::from_millis(1500),
    );
    let took = started.elapsed();
    assert!(took < Duration::from_millis(2500), "one aggregate deadline: {took:?}");
    let outcome = |name| outcomes.iter().find(|(n, _)| *n == name).unwrap().1.clone();
    assert_eq!(outcome("first"), StepOutcome::Done);
    assert_eq!(outcome("fails"), StepOutcome::Failed("socket gone".into()));
    assert_eq!(outcome("hangs"), StepOutcome::TimedOut);
    assert_eq!(outcome("panics"), StepOutcome::Panicked);
    assert_eq!(outcome("after-hang"), StepOutcome::Done, "a hung step does not block the rest");
    assert_eq!(outcome("eats-the-rest"), StepOutcome::TimedOut);
    assert_eq!(outcome("last"), StepOutcome::Skipped, "deadline spent");
    assert_eq!(*order.lock().unwrap(), ["first", "after-hang"]);
}
