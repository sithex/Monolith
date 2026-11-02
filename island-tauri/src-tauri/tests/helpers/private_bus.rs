//! Private, scratch-only D-Bus fixtures for the power/Bluetooth tests: a throwaway
//! `dbus-daemon` on a socket in a scratch directory plus a python3 mock of UPower or BlueZ.
//! Nothing here ever connects to the machine's real session or system bus; the mock refuses
//! to start unless its bus address is the scratch socket.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread::sleep,
    time::{Duration, Instant},
};

pub fn wait_until<T>(what: &str, timeout: Duration, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = probe() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {timeout:?} waiting for: {what}"
        );
        sleep(Duration::from_millis(25));
    }
}

fn kill_group(child: &mut Child) {
    // SAFETY: killpg on a process group this fixture created (process_group(0)).
    unsafe {
        libc::killpg(child.id() as i32, libc::SIGKILL);
    }
    let _ = child.wait();
}

/// A throwaway bus daemon owning `<root>/sysbus`.
pub struct PrivateBus {
    pub root: PathBuf,
    daemon: Child,
}

impl PrivateBus {
    pub fn start(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("hi-bus-{tag}-{}", std::process::id()));
        assert!(!root.exists(), "stale test sandbox: {}", root.display());
        for dir in ["home", "run"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        let config = format!(
            "<!DOCTYPE busconfig PUBLIC \"-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN\" \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\"><busconfig><type>session</type><listen>unix:path={}/sysbus</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\" eavesdrop=\"true\"/><allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>",
            root.display()
        );
        fs::write(root.join("bus.conf"), config).unwrap();
        let daemon = Command::new("dbus-daemon")
            .args(["--nofork", "--nosyslog", "--config-file"])
            .arg(root.join("bus.conf"))
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .process_group(0)
            .stdout(fs::File::create(root.join("bus.log")).unwrap())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let bus = Self { root, daemon };
        wait_until("private bus socket", Duration::from_secs(5), || {
            bus.root.join("sysbus").exists().then_some(())
        });
        bus
    }

    pub fn address(&self) -> String {
        format!("unix:path={}/sysbus", self.root.display())
    }

    /// Environment for a child that must see ONLY this bus (and nothing of the real home).
    pub fn env(&self) -> Vec<(String, String)> {
        vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("LANG".into(), "C.UTF-8".into()),
            ("HOME".into(), self.root.join("home").display().to_string()),
            (
                "XDG_RUNTIME_DIR".into(),
                self.root.join("run").display().to_string(),
            ),
            (
                "HERMES_ISLAND_SCRATCH".into(),
                self.root.display().to_string(),
            ),
            ("DBUS_SYSTEM_BUS_ADDRESS".into(), self.address()),
        ]
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        kill_group(&mut self.daemon);
        for _ in 0..30 {
            if fs::remove_dir_all(&self.root).is_ok() || !self.root.exists() {
                break;
            }
            sleep(Duration::from_millis(50));
        }
    }
}

/// UPower or BlueZ mock owning its well-known name on the private bus.
pub struct Mock {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<String>,
    pub unique: String,
    pub rogue: String,
}

impl Mock {
    pub fn start(bus_env: &[(String, String)], root: &Path, kind: &str) -> Self {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/system_mock.py");
        let mut child = Command::new("/usr/bin/python3")
            .arg(script)
            .arg(kind)
            .env_clear()
            .envs(bus_env.iter().map(|(k, v)| (k, v)))
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(root.join(format!("{kind}-mock.log"))).unwrap())
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
        let ready = output.recv_timeout(Duration::from_secs(10));
        let line = ready.unwrap_or_else(|e| {
            panic!(
                "{kind} mock did not start ({e}): {}",
                fs::read_to_string(root.join(format!("{kind}-mock.log"))).unwrap_or_default()
            )
        });
        let parts: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(parts.first(), Some(&"ready"), "{line}");
        Self {
            child,
            input,
            output,
            unique: parts[1].into(),
            rogue: parts[2].into(),
        }
    }

    pub fn send(&mut self, command: serde_json::Value) {
        writeln!(self.input, "{command}").unwrap();
        assert_eq!(
            self.output.recv_timeout(Duration::from_secs(5)).unwrap(),
            "ok"
        );
    }

    /// Kill the daemon the way a crash would: its name is lost on the bus.
    pub fn kill(mut self) {
        kill_group(&mut self.child);
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Mock {
    fn drop(&mut self) {
        kill_group(&mut self.child);
    }
}

impl PrivateBus {
    /// Like [`PrivateBus::env`], plus the SESSION bus address (the same scratch daemon). Only
    /// for children that must see this bus as their session bus (MPRIS mocks, the island).
    pub fn session_env(&self) -> Vec<(String, String)> {
        let mut env = self.env();
        env.push(("DBUS_SESSION_BUS_ADDRESS".into(), self.address()));
        env
    }
}

/// An MPRIS player mock (`mpris_mock.py`) owning `org.mpris.MediaPlayer2.<suffix>` on the
/// private bus. Every Player method call a client makes is logged by the mock and readable
/// through [`PlayerMock::calls`].
pub struct PlayerMock {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<String>,
    pub suffix: String,
    pub unique: String,
    pub rogue: String,
    calls: PathBuf,
}

impl PlayerMock {
    pub fn start(bus: &PrivateBus, suffix: &str) -> Self {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/mpris_mock.py");
        let log = bus.root.join(format!("mpris-{suffix}.log"));
        let mut child = Command::new("/usr/bin/python3")
            .arg(script)
            .arg(suffix)
            .env_clear()
            .envs(bus.session_env().iter().map(|(k, v)| (k, v)))
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(&log).unwrap())
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
        let line = output
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|e| {
                panic!(
                    "player mock {suffix} did not start ({e}): {}",
                    fs::read_to_string(&log).unwrap_or_default()
                )
            });
        let parts: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(parts.first(), Some(&"ready"), "{line}");
        Self {
            child,
            input,
            output,
            suffix: suffix.into(),
            unique: parts[1].into(),
            rogue: parts[2].into(),
            calls: bus.root.join(format!("calls-{suffix}.log")),
        }
    }

    pub fn bus_name(&self) -> String {
        format!("org.mpris.MediaPlayer2.{}", self.suffix)
    }

    pub fn send(&mut self, command: serde_json::Value) {
        writeln!(self.input, "{command}").unwrap();
        assert_eq!(
            self.output.recv_timeout(Duration::from_secs(5)).unwrap(),
            "ok"
        );
    }

    /// Method calls the mock received, oldest first (`Member args`).
    pub fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.calls)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The player quits: its name is released like a closed application's.
    pub fn quit(mut self) {
        self.send(serde_json::json!({"op": "quit"}));
        kill_group(&mut self.child);
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for PlayerMock {
    fn drop(&mut self) {
        kill_group(&mut self.child);
    }
}

impl PrivateBus {
    /// Kill the bus daemon itself (every client sees its connection drop). The scratch
    /// directory stays until this value is dropped.
    pub fn stop_daemon(&mut self) {
        kill_group(&mut self.daemon);
    }
}
