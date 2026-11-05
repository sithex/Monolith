//! Process ownership for the 6b2 mock; only our own group is signalled and reaped.
#![allow(dead_code, clippy::unwrap_used)]
use super::private_bus::PrivateBus;
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
pub struct ControlsMock {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<String>,
    calls: PathBuf,
}
impl ControlsMock {
    pub fn start(bus: &PrivateBus) -> Self {
        let mut child = Command::new("/usr/bin/python3")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/controls_mock.py"))
            .env_clear()
            .envs(bus.session_env())
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(bus.root.join("controls-mock.log")).unwrap())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        // Construct the guard BEFORE asserting readiness, so panic cleanup is owned too.
        let fixture = Self {
            child,
            input,
            output,
            calls: bus.root.join("controls-calls.jsonl"),
        };
        let ready = fixture
            .output
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|e| {
                panic!(
                    "mock did not start: {e}: {}",
                    fs::read_to_string(bus.root.join("controls-mock.log")).unwrap_or_default()
                )
            });
        assert!(ready.starts_with("ready :"), "{ready}");
        fixture
    }
    pub fn send(&mut self, command: Value) {
        writeln!(self.input, "{command}").unwrap();
        assert_eq!(
            self.output.recv_timeout(Duration::from_secs(5)).unwrap(),
            "ok"
        );
    }
    pub fn calls(&self) -> Vec<Value> {
        fs::read_to_string(&self.calls)
            .unwrap_or_default()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect()
    }
}
impl Drop for ControlsMock {
    fn drop(&mut self) {
        // SAFETY: child is our unreaped group leader, created with process_group(0).
        unsafe {
            libc::killpg(self.child.id() as i32, libc::SIGKILL);
        }
        let _wait = self.child.wait();
    }
}
