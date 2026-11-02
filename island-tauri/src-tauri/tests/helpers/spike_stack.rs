//! Phase 2.5 private nested-KWin stack. A trimmed copy of `gui_stack.rs` (left untouched: Phase 2
//! acceptance depends on it) plus what the spike needs: arbitrary app arguments, the workspace
//! capture helper and the burst-capture helper. No inherited display, bus or HOME.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::common::{
    atom, prop_bytes, prop_u32, require_tool, stop_group, wait_for, SCREEN_H, SCREEN_W,
};
use crate::spike_analysis::Capture;
use std::{
    collections::BTreeSet,
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread::sleep,
    time::Duration,
};
use x11rb::{connection::Connection, protocol::xproto::AtomEnum, rust_connection::RustConnection};

fn sockets() -> BTreeSet<String> {
    fs::read_dir("/tmp/.X11-unix")
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

pub struct Stack {
    pub root: PathBuf,
    pub display: String,
    before: BTreeSet<String>,
    children: Vec<Child>,
    kwin: u32,
}

impl Stack {
    pub fn new(tag: &str) -> Self {
        for name in ["kwin_wayland", "Xwayland", "dbus-daemon", "python3"] {
            require_tool(name);
        }
        let root = std::env::temp_dir().join(format!("hi-gui-{tag}-{}", std::process::id()));
        assert!(!root.exists(), "stale test sandbox: {}", root.display());
        for d in [
            "home", "config", "data", "cache", "state", "run", "inst", "services",
        ] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("run"), fs::Permissions::from_mode(0o700)).unwrap();
        let mut s = Self {
            root,
            display: String::new(),
            before: sockets(),
            children: vec![],
            kwin: 0,
        };
        fs::write(s.root.join("bus.conf"), format!("<!DOCTYPE busconfig PUBLIC \"-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN\" \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\"><busconfig><type>session</type><listen>unix:path={}/bus</listen><servicedir>{}/services</servicedir><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\" eavesdrop=\"true\"/><allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>", s.root.display(), s.root.display())).unwrap();
        let mut bus = Command::new("dbus-daemon");
        bus.args(["--nofork", "--nosyslog", "--config-file"])
            .arg(s.root.join("bus.conf"));
        s.spawn_with(bus, "bus", &[]);
        wait_for("private spike bus", Duration::from_secs(5), || {
            s.root.join("bus").exists().then_some(())
        });
        let mut kwin = Command::new("kwin_wayland");
        kwin.args([
            "--virtual",
            "--xwayland",
            "--socket",
            "hi-gui-wayland",
            "--width",
            &SCREEN_W.to_string(),
            "--height",
            &SCREEN_H.to_string(),
            "--no-lockscreen",
            "--no-global-shortcuts",
        ]);
        s.kwin = s.spawn_with(kwin, "kwin", &[("QT_QPA_PLATFORM", "offscreen".into())]);
        wait_for("spike KWin wayland", Duration::from_secs(30), || {
            s.root.join("run/hi-gui-wayland").exists().then_some(())
        });
        s.display = wait_for("our KWin Xwayland", Duration::from_secs(40), || {
            for name in sockets().difference(&s.before) {
                let display = format!(":{}", name.trim_start_matches('X'));
                let Ok((c, _)) = RustConnection::connect(Some(&display)) else {
                    continue;
                };
                let root = c.setup().roots[0].root;
                let wm = prop_u32(&c, root, "_NET_SUPPORTING_WM_CHECK", AtomEnum::WINDOW)?;
                if prop_bytes(&c, wm, "_NET_WM_NAME", atom(&c, "UTF8_STRING")).as_deref()
                    != Some(b"KWin")
                {
                    continue;
                }
                if s.xwayland_child_of_kwin(&display) {
                    return Some(display);
                }
            }
            None
        });
        s
    }

    fn xwayland_child_of_kwin(&self, display: &str) -> bool {
        fs::read_dir("/proc")
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| {
                let stat = fs::read_to_string(e.path().join("stat")).unwrap_or_default();
                let parent = stat
                    .rsplit_once(')')
                    .and_then(|(_, r)| r.split_whitespace().nth(1))
                    .and_then(|v| v.parse::<u32>().ok());
                let cmd = fs::read(e.path().join("cmdline")).unwrap_or_default();
                let args: Vec<_> = cmd.split(|b| *b == 0).collect();
                parent == Some(self.kwin)
                    && args.first().is_some_and(|v| v.ends_with(b"Xwayland"))
                    && args.get(1) == Some(&display.as_bytes())
            })
    }

    pub fn env(&self) -> Vec<(String, String)> {
        let mut e = vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("LANG".into(), "C.UTF-8".into()),
            ("TMPDIR".into(), self.root.to_string_lossy().into_owned()),
            (
                "DBUS_SESSION_BUS_ADDRESS".into(),
                format!("unix:path={}/bus", self.root.display()),
            ),
        ];
        for (key, dir) in [
            ("HOME", "home"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_RUNTIME_DIR", "run"),
        ] {
            e.push((key.into(), self.root.join(dir).display().to_string()));
        }
        e
    }

    fn spawn_with(&mut self, mut command: Command, name: &str, extra: &[(&str, String)]) -> u32 {
        let log = fs::File::create(self.root.join(format!("{name}.log"))).unwrap();
        let child = command
            .env_clear()
            .envs(self.env())
            .envs(extra.iter().map(|(k, v)| (*k, v)))
            .process_group(0)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let pid = child.id();
        self.children.push(child);
        pid
    }

    /// The island with the Phase 2.5 spike switches. `HERMES_ISLAND_TEST=1` and
    /// `--test-instance` are what the product requires before it honours `--spike-panel`.
    pub fn start_island(&mut self, spike: &str) -> u32 {
        let mut app = Command::new(env!("CARGO_BIN_EXE_hermes-island"));
        app.args(["--test-instance", "--overlay", "--data-dir"])
            .arg(self.root.join("inst"))
            .args(["--spike-panel", spike]);
        let display = self.display.clone();
        self.spawn_with(
            app,
            "app",
            &[
                ("DISPLAY", display),
                ("GDK_BACKEND", "x11".into()),
                ("HERMES_ISLAND_TEST", "1".into()),
                ("NO_AT_BRIDGE", "1".into()),
                ("GTK_USE_PORTAL", "0".into()),
                ("GIO_USE_PORTAL", "0".into()),
            ],
        )
    }

    /// Stop one child (and its process group) and forget it, e.g. the island between variants.
    pub fn stop_child(&mut self, pid: u32) {
        let index = self.children.iter().position(|c| c.id() == pid).unwrap();
        let mut child = self.children.remove(index);
        stop_group(&mut child);
    }

    pub fn connect(&self) -> RustConnection {
        RustConnection::connect(Some(&self.display)).unwrap().0
    }

    fn line_helper(&mut self, script: &str, name: &str) -> (ChildStdin, mpsc::Receiver<String>) {
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/helpers")
            .join(script);
        let mut child = Command::new("/usr/bin/python3")
            .arg(script)
            .env_clear()
            .envs(self.env())
            .env("HERMES_ISLAND_SCRATCH", &self.root)
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(self.root.join(format!("{name}.log"))).unwrap())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        self.children.push(child);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for l in BufReader::new(output).lines().map_while(Result::ok) {
                if tx.send(l).is_err() {
                    break;
                }
            }
        });
        (input, rx)
    }

    pub fn eis(&mut self) -> Eis {
        let (input, rx) = self.line_helper("eis_input.py", "eis");
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(30)).unwrap(),
            format!("ready 0 0 {SCREEN_W} {SCREEN_H}")
        );
        Eis { input, rx }
    }

    pub fn burst_helper(&mut self) -> Burst {
        let (input, rx) = self.line_helper("kwin_burst.py", "burst");
        assert_eq!(rx.recv_timeout(Duration::from_secs(30)).unwrap(), "ready");
        Burst { input, rx }
    }

    /// One full composited workspace capture (RGB, row-major).
    pub fn capture(&self) -> Capture {
        let raw = self.root.join("capture.raw");
        let output = Command::new("/usr/bin/python3")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/kwin_capture.py"))
            .arg(&raw)
            .env_clear()
            .envs(self.env())
            .env("HERMES_ISLAND_SCRATCH", &self.root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "capture: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let meta: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        Capture::from_raw(
            &fs::read(raw).unwrap(),
            meta["stride"].as_u64().unwrap() as usize,
            meta["format"].as_u64().unwrap(),
            meta["width"].as_u64().unwrap() as usize,
            meta["height"].as_u64().unwrap() as usize,
        )
    }

    pub fn shutdown(&mut self) {
        let groups: Vec<_> = self.children.iter().map(Child::id).collect();
        for child in self.children.iter_mut().rev() {
            stop_group(child);
        }
        self.children.clear();
        for group in groups {
            let alive = fs::read_dir("/proc")
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| {
                    let stat = fs::read_to_string(entry.path().join("stat")).unwrap_or_default();
                    let fields: Vec<_> = stat
                        .rsplit_once(')')
                        .map(|(_, r)| r.split_whitespace().collect())
                        .unwrap_or_default();
                    fields.first().is_some_and(|s| *s != "Z")
                        && fields.get(2).and_then(|s| s.parse::<u32>().ok()) == Some(group)
                });
            assert!(!alive, "private spike process group {group} survived");
        }
        wait_for("spike X sockets removed", Duration::from_secs(5), || {
            (sockets() == self.before).then_some(())
        });
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        if std::thread::panicking() {
            for name in ["app", "kwin", "eis", "burst"] {
                eprintln!(
                    "{name}: {}",
                    fs::read_to_string(self.root.join(format!("{name}.log"))).unwrap_or_default()
                );
            }
        }
        for c in self.children.iter_mut().rev() {
            stop_group(c);
        }
        for _ in 0..30 {
            if fs::remove_dir_all(&self.root).is_ok() || !self.root.exists() {
                break;
            }
            sleep(Duration::from_millis(50));
        }
    }
}

pub struct Eis {
    input: ChildStdin,
    rx: mpsc::Receiver<String>,
}

impl Eis {
    pub fn send(&mut self, line: &str) {
        writeln!(self.input, "{line}").unwrap();
        assert_eq!(self.rx.recv_timeout(Duration::from_secs(10)).unwrap(), "ok");
    }
    /// Absolute pointer motion; the pause lets KWin route the motion before anything follows.
    pub fn move_to(&mut self, x: i16, y: i16) {
        self.send(&format!("move {x} {y}"));
        sleep(Duration::from_millis(250));
    }
    pub fn click(&mut self) {
        self.send("click");
        sleep(Duration::from_millis(250));
    }
    pub fn type_text(&mut self, text: &str) {
        self.send(&format!("type {text}"));
        sleep(Duration::from_millis(250));
    }
}

pub struct Burst {
    input: ChildStdin,
    rx: mpsc::Receiver<String>,
}

impl Burst {
    /// Returns immediately; the helper captures `count` times back to back (about 3 ms each).
    pub fn start(&mut self, area: (i32, i32, u32, u32), count: u32, dir: &Path) {
        writeln!(
            self.input,
            "burst {} {} {} {} {count} {}",
            area.0,
            area.1,
            area.2,
            area.3,
            dir.display()
        )
        .unwrap();
    }
    pub fn finish(&self) -> serde_json::Value {
        let line = self.rx.recv_timeout(Duration::from_secs(120)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(value.get("error").is_none(), "burst helper: {line}");
        value
    }
}
