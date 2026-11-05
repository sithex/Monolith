//! Owned private Controls daemon; JSON stdin is test control, not production transport.
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
pub struct ExtraMock {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<String>,
    calls: PathBuf,
}
impl ExtraMock {
    pub fn start(bus: &PrivateBus) -> Self {
        let mut child = Command::new("/usr/bin/python3")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/extra_controls_mock.py"))
            .env_clear()
            .envs(bus.session_env())
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(bus.root.join("extra-mock.log")).unwrap())
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
        let fixture = Self {
            child,
            input,
            output,
            calls: bus.root.join("extra-calls.jsonl"),
        };
        let ready = fixture
            .output
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|e| {
                panic!(
                    "extra mock startup {e}: {}",
                    fs::read_to_string(bus.root.join("extra-mock.log")).unwrap_or_default()
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
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}
impl Drop for ExtraMock {
    fn drop(&mut self) {
        unsafe {
            libc::killpg(self.child.id() as i32, libc::SIGKILL);
        }
        if let Err(error) = self.child.wait() {
            eprintln!("extra mock reap: {error}");
        }
    }
}
