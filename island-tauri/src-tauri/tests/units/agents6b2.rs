#![allow(clippy::unwrap_used)]
use hermes_island_lib::{
    error::IslandError,
    integrations::agents::{
        core::{Event, SessionRow, Tracker},
        AgentConfig, AgentEvents, AgentService,
    },
};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
static SERIAL: AtomicU32 = AtomicU32::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hi-6b2-agents-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn db(&self) -> PathBuf {
        self.0.join("fixture.db")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[derive(Default)]
struct Sink(Mutex<Vec<Event>>);
impl AgentEvents for Sink {
    fn deliver(&self, event: &Event) -> Result<(), IslandError> {
        self.0.lock().unwrap().push(event.clone());
        Ok(())
    }
    fn clear(&self) -> Result<(), IslandError> {
        Ok(())
    }
}
fn sql(dir: &Scratch, statement: &str, args: Value) {
    assert!(dir.db().starts_with(std::env::temp_dir()));
    let mut child=Command::new("/usr/bin/python3").args(["-I","-c","import sqlite3,json,sys; p=sys.argv[1]; c=sqlite3.connect(p); s,a=json.load(sys.stdin); c.execute(s,a); c.commit(); c.close()"])
        .arg(dir.db()).env_clear().env("HOME",&dir.0).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
    writeln!(child.stdin.take().unwrap(), "{}", json!([statement, args])).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
fn schema(dir: &Scratch) {
    sql(dir,"CREATE TABLE sessions (id TEXT PRIMARY KEY, source TEXT, parent_session_id TEXT, model TEXT, billing_provider TEXT, started_at REAL, ended_at REAL, end_reason TEXT, last_activity_at REAL, api_call_count INTEGER, title TEXT)",json!([]));
}
fn insert(
    dir: &Scratch,
    id: &str,
    source: &str,
    start: f64,
    activity: f64,
    end: Option<f64>,
    parent: Option<&str>,
) {
    sql(
        dir,
        "INSERT OR REPLACE INTO sessions VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        json!([
            id,
            source,
            parent,
            "fixture-model",
            "fixture-provider",
            start,
            end,
            null,
            activity,
            3,
            "fixture run"
        ]),
    );
}
fn service(dir: &Scratch, sink: Arc<Sink>) -> Arc<AgentService> {
    AgentService::new(AgentConfig::for_instance(true, None, Some(dir.db())), sink)
}
#[test]
fn agents_fixture_database_is_read_only_orphans_are_stale_and_completion_survives_stalling() {
    let dir = Scratch::new();
    schema(&dir);
    insert(&dir, "parent", "cli", 100., 1000., None, None);
    insert(&dir, "child", "subagent", 500., 1000., None, Some("parent"));
    insert(&dir, "orphan", "cli", 1., 1., None, None);
    insert(&dir, "history", "cli", 1., 1., Some(2.), None);
    let original = fs::read(dir.db()).unwrap();
    let sink = Arc::new(Sink::default());
    let s = service(&dir, sink.clone());
    let initial = s.poll_at(1000.).unwrap();
    assert!(initial.available);
    assert_eq!(initial.chips.len(), 1);
    assert_eq!(initial.chips[0].id, "parent");
    assert_eq!(initial.chips[0].children, 1);
    assert_eq!(
        fs::read(dir.db()).unwrap(),
        original,
        "read changed fixture database bytes"
    );
    assert_eq!(
        fs::read_dir(&dir.0).unwrap().count(),
        1,
        "read left journal/WAL/socket files"
    );
    assert_eq!(
        sink.0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, Event::Started { .. }))
            .count(),
        2
    );
    assert!(!sink
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, Event::Finished { .. })));
    assert!(s.poll_at(2000.).unwrap().chips.is_empty());
    insert(&dir, "parent", "cli", 100., 1000., Some(2100.), None);
    s.poll_at(2100.).unwrap();
    s.poll_at(2101.).unwrap();
    assert_eq!(
        sink.0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e,Event::Finished{id,..} if id=="parent"))
            .count(),
        1,
        "finish after stalling must be delivered exactly once"
    );
    s.shutdown().unwrap();
}
#[test]
fn agents_database_loss_and_recovery_emit_unavailable_without_false_completions() {
    let dir = Scratch::new();
    schema(&dir);
    insert(&dir, "one", "cli", 100., 1000., None, None);
    let sink = Arc::new(Sink::default());
    let s = service(&dir, sink.clone());
    s.poll_at(1000.).unwrap();
    let old = s.current().unwrap().rev;
    fs::rename(dir.db(), dir.0.join("saved.db")).unwrap();
    assert!(s.poll_at(1001.).is_err());
    let lost = s.current().unwrap();
    assert!(!lost.available);
    assert!(lost.chips.is_empty());
    assert!(lost.rev > old);
    fs::rename(dir.0.join("saved.db"), dir.db()).unwrap();
    let restored = s.poll_at(1002.).unwrap();
    assert!(restored.available);
    assert_eq!(restored.chips.len(), 1);
    assert_eq!(
        sink.0.lock().unwrap().len(),
        1,
        "temporary DB loss must not emit Disappeared, another Started or Finished"
    );
    s.shutdown().unwrap();
}
#[test]
fn agents_test_instance_without_fixture_never_infers_a_home_database() {
    let dir = Scratch::new();
    fs::create_dir(dir.0.join(".hermes")).unwrap();
    fs::write(dir.0.join(".hermes/state.db"), b"canary").unwrap();
    let s = AgentService::new(
        AgentConfig::for_instance(true, Some(dir.0.clone()), None),
        Arc::new(Sink::default()),
    );
    assert!(s.poll_at(1000.).is_err());
    assert_eq!(s.current().unwrap().detail, "no-fixture-database");
    assert_eq!(fs::read(dir.0.join(".hermes/state.db")).unwrap(), b"canary");
    s.shutdown().unwrap();
}
#[test]
fn agents_worker_is_bounded_and_shutdown_wakes_the_fifteen_second_wait() {
    let dir = Scratch::new();
    schema(&dir);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    insert(&dir, "one", "cli", now - 10., now, None, None);
    let s = service(&dir, Arc::new(Sink::default()));
    s.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !s.current().unwrap().available {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let rev = s.current().unwrap().rev;
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(s.current().unwrap().rev, rev, "worker must not busy-poll");
    let start = Instant::now();
    s.shutdown().unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(!s.current().unwrap().available);
    assert!(s.poll_at(now).is_err());
}
#[test]
fn agents_failed_finish_delivery_retries_once_without_losing_the_transition() {
    struct Flaky {
        fail: AtomicU32,
        delivered: Mutex<Vec<Event>>,
    }
    impl AgentEvents for Flaky {
        fn deliver(&self, event: &Event) -> Result<(), IslandError> {
            if self.fail.swap(0, Ordering::SeqCst) > 0 {
                return Err(IslandError::unavailable("fixture sink unavailable"));
            }
            self.delivered.lock().unwrap().push(event.clone());
            Ok(())
        }
        fn clear(&self) -> Result<(), IslandError> {
            Ok(())
        }
    }
    let dir = Scratch::new();
    schema(&dir);
    insert(&dir, "one", "cli", 900., 1000., None, None);
    let sink = Arc::new(Flaky {
        fail: AtomicU32::new(0),
        delivered: Mutex::new(vec![]),
    });
    let s = AgentService::new(
        AgentConfig::for_instance(true, None, Some(dir.db())),
        sink.clone(),
    );
    s.poll_at(1000.).unwrap();
    sink.fail.store(1, Ordering::SeqCst);
    insert(&dir, "one", "cli", 900., 1010., Some(1010.), None);
    assert!(s.poll_at(1010.).is_err());
    assert_eq!(s.current().unwrap().detail, "event-delivery-failed");
    assert!(s.poll_at(1011.).unwrap().available);
    s.poll_at(1012.).unwrap();
    let delivered = sink.delivered.lock().unwrap();
    assert_eq!(
        delivered
            .iter()
            .filter(|e| matches!(e, Event::Started { .. }))
            .count(),
        1
    );
    assert_eq!(
        delivered
            .iter()
            .filter(|e| matches!(e, Event::Finished { .. }))
            .count(),
        1
    );
    drop(delivered);
    s.shutdown().unwrap();
}
#[test]
fn agents_outbox_cap_refuses_before_advancing_and_recovers_when_the_batch_shrinks() {
    for count in [128, 129] {
        let dir = Scratch::new();
        schema(&dir);
        sql(&dir,"WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<?) INSERT INTO sessions SELECT 'bulk-'||x,'cli',NULL,'fixture','fixture',900,NULL,NULL,1000,1,'fixture' FROM n",json!([count]));
        let sink = Arc::new(Sink::default());
        let s = service(&dir, sink.clone());
        if count == 128 {
            assert!(s.poll_at(1000.).unwrap().available);
            assert_eq!(sink.0.lock().unwrap().len(), 128);
        } else {
            assert!(s.poll_at(1000.).is_err());
            assert!(sink.0.lock().unwrap().is_empty());
            sql(&dir, "DELETE FROM sessions WHERE id != 'bulk-1'", json!([]));
            assert!(s.poll_at(1001.).unwrap().available);
            assert!(matches!(sink.0.lock().unwrap()[0], Event::Started { .. }));
        }
        s.shutdown().unwrap();
    }
}
#[test]
fn agents_core_finished_after_stale_has_one_completion_and_no_startup_history_spam() {
    let row = |end| SessionRow {
        id: "one".into(),
        source: "cli".into(),
        parent_session_id: None,
        model: None,
        billing_provider: None,
        started_at: 1.,
        ended_at: end,
        end_reason: None,
        last_activity_at: Some(1000.),
        api_call_count: None,
        title: None,
    };
    let mut t = Tracker::new();
    assert!(matches!(
        t.update(vec![row(None)], 1000.).events[0],
        Event::Started { .. }
    ));
    assert!(matches!(
        t.update(vec![row(None)], 2000.).events[0],
        Event::Stalled { .. }
    ));
    assert!(matches!(
        t.update(vec![row(Some(2100.))], 2100.).events[0],
        Event::Finished { .. }
    ));
    assert!(t.update(vec![row(Some(2100.))], 2101.).events.is_empty());
    assert!(Tracker::new()
        .update(vec![row(Some(2100.))], 2101.)
        .events
        .is_empty());
}
