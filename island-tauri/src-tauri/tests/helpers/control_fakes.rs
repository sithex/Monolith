//! Scratch-only executable fixture. No fallback PATH, host service or real profile access.
#![allow(dead_code, clippy::unwrap_used)]
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, net::UnixDatagram},
    path::PathBuf,
    time::{Duration, Instant},
};
pub struct Fakes {
    pub root: PathBuf,
}
impl Fakes {
    pub fn new(tag: &str) -> Self {
        Self::within(&std::env::temp_dir(), tag)
    }
    pub fn within(base: &std::path::Path, tag: &str) -> Self {
        let root = base.join(format!("hi-6b2-fakes-{tag}-{}", std::process::id()));
        assert!(!root.exists());
        let fixture = Self { root };
        for dir in ["bin", "home", "run", "config", "data", "cache"] {
            fs::create_dir_all(fixture.root.join(dir)).unwrap();
        }
        for name in [
            "pactl",
            "tailscale",
            "hermes-fish",
            "spectacle",
            "checkupdates",
            "paru",
        ] {
            let path = fixture.root.join("bin").join(name);
            fs::write(&path, include_str!("control_fakes.py")).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fixture.set(json!({
            "sinks":[{"index":1,"name":"headset","description":"Fixture headset","mute":false,"state":"RUNNING","volume":{"mono":{"value":32768}}},{"index":2,"name":"hdmi","description":"Fixture HDMI","mute":false,"state":"SUSPENDED","volume":{}}],
            "streams":[{"index":11,"sink":1,"mute":false,"corked":false,"volume":{"mono":{"value":16384}},"properties":{"application.name":"Fixture player","application.process.id":"42"}},{"index":12,"sink":1,"mute":false,"corked":true,"volume":{},"properties":{"application.name":"Fixture player","application.process.id":"42"}}],
            "default":"headset","mic":false,"voice":"fixture-a","voices":["fixture-a","fixture-b"],
            "tailscale":{"BackendState":"Running","Peer":{"unknown":{"HostName":"fixture-peer","OS":"linux"}}},
            "checkupdates":{"code":0,"stdout":"linux 1 -> 2\n"},"paru":{"code":1,"stdout":"","stderr":""}
        }));
        fixture
    }
    pub fn runner(&self) -> hermes_island_lib::integrations::control_process::Runner {
        hermes_island_lib::integrations::control_process::Runner::for_instance(
            true,
            Some(self.root.clone()),
        )
    }
    pub fn state(&self) -> Value {
        serde_json::from_str(&fs::read_to_string(self.root.join("state.json")).unwrap()).unwrap()
    }
    pub fn set(&self, state: Value) {
        let tmp = self.root.join("new-state.json");
        fs::write(&tmp, state.to_string()).unwrap();
        fs::rename(tmp, self.root.join("state.json")).unwrap();
    }
    pub fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.root.join("calls.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    pub fn event(&self, line: &str) {
        let socket = UnixDatagram::unbound().unwrap();
        let mut sent = 0;
        for file in fs::read_dir(&self.root).unwrap() {
            let path = file.unwrap().path();
            if path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("sub-")
                && socket.send_to(line.as_bytes(), path).is_ok()
            {
                sent += 1;
            }
        }
        assert!(sent > 0, "no live private subscription");
    }
    pub async fn subscriptions(&self, count: usize) {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            let live = fs::read_dir(&self.root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|f| f.file_name().to_string_lossy().starts_with("sub-"))
                .count();
            if live >= count {
                return;
            }
            assert!(Instant::now() < until, "private subscription readiness");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
impl Drop for Fakes {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!("fake cleanup failed: {error}");
        }
    }
}
