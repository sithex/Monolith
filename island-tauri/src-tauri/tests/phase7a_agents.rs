#![allow(clippy::unwrap_used)]
//! Phase 7a carried read-only gap. The Rust preflight alone cannot pin SQLite's open mode.
use hermes_island_lib::{
    error::IslandError,
    integrations::agents::{core::Event, AgentConfig, AgentEvents, AgentService},
};
use std::{fs, path::PathBuf, process::Command, sync::Arc};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
struct Sink;
impl AgentEvents for Sink {
    fn deliver(&self, _: &Event) -> Result<(), IslandError> {
        panic!("a missing database must not invent an agent transition");
    }
    fn clear(&self) -> Result<(), IslandError> {
        Ok(())
    }
}
#[test]
fn agents_missing_database_is_unavailable_and_reader_never_creates_it() {
    let dir = Scratch(std::env::temp_dir().join(format!("hi-7a-agents-{}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    let path = dir.0.join("state.db");
    // Accept a regular fixture path, then lose it. This reaches the production unavailable
    // classification rather than only testing the no-fixture configuration branch.
    fs::write(&path, b"").unwrap();
    let service = AgentService::new(
        AgentConfig::for_instance(true, None, Some(path.clone())),
        Arc::new(Sink),
    );
    fs::remove_file(&path).unwrap();
    let error = service.poll_at(1000.).unwrap_err();
    assert_eq!(error.code, hermes_island_lib::error::ErrorCode::Unavailable);
    let snapshot = service.current().unwrap();
    assert!(!snapshot.available);
    assert_eq!(snapshot.detail, "database-unavailable");
    assert!(snapshot.chips.is_empty());
    assert!(!path.exists());
    // Pin the actual reader too: Rust's regular-file preflight otherwise masks mode=rwc.
    let output = Command::new("/usr/bin/python3")
        .args([
            "-I",
            "-c",
            include_str!("../src/integrations/agents/reader.py"),
        ])
        .arg(&path)
        .arg("1000")
        .env_clear()
        .env("HOME", &dir.0)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "missing SQLite database was accepted"
    );
    assert!(
        output.stdout.is_empty(),
        "unavailable must not fabricate an empty/zero snapshot"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("unable to open database file"));
    assert!(
        !path.exists(),
        "read-only metadata reader CREATED a missing state.db"
    );
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
    service.shutdown().unwrap();
}
