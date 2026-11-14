#![allow(clippy::unwrap_used)]
//! The in-process agent metadata reader: read-only open, query, limits and value tolerance.
use hermes_island_lib::{
    error::{ErrorCode, IslandError},
    integrations::agents::{
        core::{Event, SessionRow},
        reader::{self, read_sessions},
        AgentConfig, AgentEvents, AgentService,
    },
    services::read_only_sqlite,
};
use rusqlite::{params_from_iter, types::Value, Connection};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

static SERIAL: AtomicU32 = AtomicU32::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hi-agents-reader-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn db(&self) -> PathBuf {
        self.0.join("state.db")
    }
    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

const SCHEMA: &str = "CREATE TABLE sessions (id TEXT PRIMARY KEY, source TEXT, parent_session_id TEXT, model TEXT, billing_provider TEXT, started_at REAL, ended_at REAL, end_reason TEXT, last_activity_at REAL, api_call_count INTEGER, title TEXT)";

/// Same columns without declared types, so SQLite keeps each value's storage class exactly
/// (a REAL column would turn INTEGER 900 into 900.0, a TEXT column INTEGER 7 into '7').
const UNTYPED: &str = "CREATE TABLE sessions (id PRIMARY KEY, source, parent_session_id, model, billing_provider, started_at, ended_at, end_reason, last_activity_at, api_call_count, title)";

/// Writes one fixture database (closed again before returning, so no journal stays behind).
fn fixture(dir: &Scratch, rows: &[Vec<Value>]) {
    fixture_with(dir, SCHEMA, rows);
}

fn fixture_with(dir: &Scratch, schema: &str, rows: &[Vec<Value>]) {
    let db = Connection::open(dir.db()).unwrap();
    db.execute_batch(schema).unwrap();
    for row in rows {
        db.execute(
            "INSERT INTO sessions VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            params_from_iter(row.iter()),
        )
        .unwrap();
    }
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

/// A valid row started at 900; `edit` replaces column `i` with `v`.
fn row(edit: &[(usize, Value)]) -> Vec<Value> {
    let mut r = vec![
        text("one"),
        text("cli"),
        Value::Null,
        text("m"),
        text("p"),
        Value::Real(900.5),
        Value::Null,
        Value::Null,
        Value::Real(950.),
        Value::Integer(3),
        text("t"),
    ];
    for (i, v) in edit {
        r[*i] = v.clone();
    }
    r
}

fn read_one(edit: &[(usize, Value)]) -> Result<Vec<SessionRow>, IslandError> {
    let dir = Scratch::new();
    fixture_with(&dir, UNTYPED, &[row(edit)]);
    let before = fs::read(dir.db()).unwrap();
    let out = read_sessions(&dir.db(), 1000.);
    assert_eq!(
        fs::read(dir.db()).unwrap(),
        before,
        "read changed the database"
    );
    assert_eq!(dir.files(), ["state.db"], "read left sidecar files");
    out
}

#[test]
fn agents_reader_reads_the_same_columns_newest_first_within_one_day() {
    let dir = Scratch::new();
    fixture(
        &dir,
        &[
            row(&[(0, text("old")), (5, Value::Real(1000. - 86400. - 1.))]),
            row(&[(0, text("edge")), (5, Value::Real(1000. - 86400.))]),
            row(&[
                (0, text("new")),
                (2, text("edge")),
                (5, Value::Integer(999)),
                (6, Value::Integer(999)),
                (7, text("done")),
            ]),
        ],
    );
    let rows = read_sessions(&dir.db(), 1000.).unwrap();
    assert_eq!(
        rows,
        [
            SessionRow {
                id: "new".into(),
                source: "cli".into(),
                parent_session_id: Some("edge".into()),
                model: Some("m".into()),
                billing_provider: Some("p".into()),
                started_at: 999.,
                ended_at: Some(999.),
                end_reason: Some("done".into()),
                last_activity_at: Some(950.),
                api_call_count: Some(3),
                title: Some("t".into()),
            },
            SessionRow {
                id: "edge".into(),
                source: "cli".into(),
                parent_session_id: None,
                model: Some("m".into()),
                billing_provider: Some("p".into()),
                started_at: 1000. - 86400.,
                ended_at: None,
                end_reason: None,
                last_activity_at: Some(950.),
                api_call_count: Some(3),
                title: Some("t".into()),
            },
        ]
    );
}

#[test]
fn agents_reader_accepts_null_integer_and_real_where_the_json_path_did() {
    // Times: INTEGER and REAL both read as seconds; optional times may be NULL.
    let r = read_one(&[(5, Value::Integer(900)), (8, Value::Null)]).unwrap();
    assert_eq!((r[0].started_at, r[0].last_activity_at), (900., None));
    let r = read_one(&[(6, Value::Integer(950)), (8, Value::Integer(951))]).unwrap();
    assert_eq!(
        (r[0].ended_at, r[0].last_activity_at),
        (Some(950.), Some(951.))
    );
    let r = read_one(&[(6, Value::Real(950.25))]).unwrap();
    assert_eq!(r[0].ended_at, Some(950.25));
    // Optional text and the call count may be NULL.
    let r = read_one(&[
        (2, Value::Null),
        (3, Value::Null),
        (4, Value::Null),
        (7, Value::Null),
        (9, Value::Null),
        (10, Value::Null),
    ])
    .unwrap();
    assert_eq!(
        (&r[0].model, &r[0].title, r[0].api_call_count),
        (&None, &None, None)
    );
    let r = read_one(&[(9, Value::Integer(0))]).unwrap();
    assert_eq!(r[0].api_call_count, Some(0));
}

#[test]
fn agents_reader_refuses_values_the_json_path_refused() {
    let refused: [(&str, (usize, Value)); 11] = [
        ("NULL id", (0, Value::Null)),
        ("INTEGER id", (0, Value::Integer(7))),
        ("NULL source", (1, Value::Null)),
        ("REAL title", (10, Value::Real(1.5))),
        ("BLOB model", (3, Value::Blob(b"m".to_vec()))),
        ("TEXT time", (8, text("950"))),
        ("REAL call count", (9, Value::Real(3.))),
        ("negative call count", (9, Value::Integer(-1))),
        ("TEXT call count", (9, text("3"))),
        ("infinite time", (6, Value::Real(f64::INFINITY))),
        ("BLOB time", (8, Value::Blob(vec![1]))),
    ];
    for (what, edit) in refused {
        let error = read_one(&[edit]).expect_err(what);
        assert_eq!(error.code, ErrorCode::Unavailable, "{what}");
    }
    // A NULL started_at is never selected by `started_at >= ?`, so it is skipped, not an error.
    assert!(read_one(&[(5, Value::Null)]).unwrap().is_empty());
}

#[test]
fn agents_reader_text_field_limit_is_4096_bytes() {
    let fits = read_one(&[(10, text(&"é".repeat(2048)))]).unwrap();
    assert_eq!(fits[0].title.as_deref().map(str::len), Some(4096));
    let error = read_one(&[(10, text(&format!("{}x", "é".repeat(2048))))]).unwrap_err();
    assert_eq!(error.message, "Agent metadata invalid");
    assert!(read_one(&[(0, text(&"i".repeat(4097)))]).is_err());
}

#[test]
fn agents_reader_row_limit_is_10000() {
    for (count, ok) in [(10_000, true), (10_001, false)] {
        let dir = Scratch::new();
        let db = Connection::open(dir.db()).unwrap();
        db.execute_batch(SCHEMA).unwrap();
        db.execute("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<?1) INSERT INTO sessions SELECT 'bulk-'||x,'cli',NULL,'m','p',900,NULL,NULL,950,1,'t' FROM n", [count]).unwrap();
        drop(db);
        let result = read_sessions(&dir.db(), 1000.);
        if ok {
            assert_eq!(result.unwrap().len(), reader::MAX_ROWS);
        } else {
            assert_eq!(result.unwrap_err().message, "Agent metadata row limit");
        }
    }
}

#[test]
fn agents_reader_missing_file_is_refused_and_never_created() {
    let dir = Scratch::new();
    assert!(read_sessions(&dir.db(), 1000.).is_err());
    assert!(reader::open(&dir.db()).is_err());
    assert!(
        dir.files().is_empty(),
        "the reader created {:?}",
        dir.files()
    );
}

#[test]
fn agents_reader_never_follows_a_symlink() {
    let dir = Scratch::new();
    fixture(&dir, &[row(&[])]);
    let link = dir.0.join("link.db");
    std::os::unix::fs::symlink(dir.db(), &link).unwrap();
    assert!(read_sessions(&dir.db(), 1000.).is_ok());
    assert!(read_sessions(&link, 1000.).is_err());
    assert_eq!(
        read_only_sqlite::open(&link, reader::BUSY_TIMEOUT).unwrap_err(),
        "state.db unreadable"
    );
    // The service refuses it before SQLite is reached.
    struct Quiet;
    impl AgentEvents for Quiet {
        fn deliver(&self, _: &Event) -> Result<(), IslandError> {
            Ok(())
        }
        fn clear(&self) -> Result<(), IslandError> {
            Ok(())
        }
    }
    let service = AgentService::new(
        AgentConfig::for_instance(true, None, Some(dir.db())),
        Arc::new(Quiet),
    );
    assert!(service.poll_at(1000.).unwrap().available);
    fs::rename(dir.db(), dir.0.join("real.db")).unwrap();
    std::os::unix::fs::symlink(dir.0.join("real.db"), dir.db()).unwrap();
    let error = service.poll_at(1001.).unwrap_err();
    assert_eq!(error.message, "Agent database unavailable");
    assert_eq!(service.current().unwrap().detail, "database-unavailable");
    service.shutdown().unwrap();
}

#[test]
fn agents_reader_table_missing_a_column_is_an_error() {
    let dir = Scratch::new();
    let db = Connection::open(dir.db()).unwrap();
    db.execute_batch("CREATE TABLE sessions (id TEXT, source TEXT, started_at REAL); INSERT INTO sessions VALUES ('a','cli',900);").unwrap();
    drop(db);
    assert!(read_sessions(&dir.db(), 1000.).is_err());
    assert_eq!(dir.files(), ["state.db"]);
}

#[test]
fn agents_reader_connection_is_query_only_and_refuses_writes() {
    let dir = Scratch::new();
    fixture(&dir, &[row(&[])]);
    let before = fs::read(dir.db()).unwrap();
    let db = reader::open(&dir.db()).unwrap();
    let query_only: i64 = db.query_row("PRAGMA query_only", [], |r| r.get(0)).unwrap();
    assert_eq!(query_only, 1);
    let trusted: i64 = db
        .query_row("PRAGMA trusted_schema", [], |r| r.get(0))
        .unwrap();
    assert_eq!(trusted, 0);
    assert!(db.execute("DELETE FROM sessions", []).is_err());
    assert!(db
        .execute("INSERT INTO sessions (id) VALUES ('x')", [])
        .is_err());
    // Even with query_only switched off, the read-only open still refuses the write.
    assert!(db
        .execute_batch("PRAGMA query_only=OFF; CREATE TABLE forbidden (id TEXT);")
        .is_err());
    drop(db);
    assert_eq!(fs::read(dir.db()).unwrap(), before);
    assert_eq!(dir.files(), ["state.db"]);
}

#[test]
fn agents_reader_sees_committed_wal_rows_without_touching_db_wal_or_shm() {
    let dir = Scratch::new();
    fixture(&dir, &[]);
    let writer = Connection::open(dir.db()).unwrap();
    writer
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    writer
        .execute(
            "INSERT INTO sessions VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            params_from_iter(row(&[(0, text("wal-only"))]).iter()),
        )
        .unwrap();
    writer
        .query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
        .unwrap();
    let snapshot = |d: &Scratch| -> Vec<(String, Vec<u8>)> {
        d.files()
            .into_iter()
            .map(|n| (n.clone(), fs::read(d.0.join(&n)).unwrap()))
            .collect()
    };
    let before = snapshot(&dir);
    assert_eq!(before.len(), 3);
    let rows = read_sessions(&dir.db(), 1000.).unwrap();
    assert_eq!(rows[0].id, "wal-only");
    assert_eq!(snapshot(&dir), before);
    drop(writer);
}

#[test]
fn agents_reader_closed_wal_database_without_sidecars_never_creates_them() {
    // Hermes deletes state.db-wal/-shm when its last connection closes. The former reader
    // recreated both files here; the shared read-only open refuses instead of writing.
    let dir = Scratch::new();
    fixture(&dir, &[row(&[])]);
    let writer = Connection::open(dir.db()).unwrap();
    writer.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    drop(writer);
    assert_eq!(dir.files(), ["state.db"]);
    let before = fs::read(dir.db()).unwrap();
    // Ok or Err depends on SQLite; what matters is that nothing was written.
    let _ = read_sessions(&dir.db(), 1000.);
    assert_eq!(dir.files(), ["state.db"], "the reader created WAL sidecars");
    assert_eq!(fs::read(dir.db()).unwrap(), before);
}
