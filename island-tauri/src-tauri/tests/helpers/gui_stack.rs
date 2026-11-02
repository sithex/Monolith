//! Private KWin stack for legacy GUI acceptance. No inherited display, bus or HOME.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::common::{
    atom, prop_bytes, prop_u32, require_tool, stop_group, wait_for, SCREEN_H, SCREEN_W,
};
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
        s.spawn(bus, "bus");
        wait_for("private GUI bus", Duration::from_secs(5), || {
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
        ])
        .env("QT_QPA_PLATFORM", "offscreen");
        // spawn clears environment, so set the special flag after the common sandbox environment.
        s.kwin = s.spawn_with(kwin, "kwin", &[("QT_QPA_PLATFORM", "offscreen".into())]);
        wait_for("GUI KWin wayland", Duration::from_secs(30), || {
            s.root.join("run/hi-gui-wayland").exists().then_some(())
        });
        s.display = wait_for("our KWin Xwayland", Duration::from_secs(40), || {
            for name in sockets().difference(&s.before) {
                let display = format!(":{}", name.trim_start_matches('X'));
                let Ok((c, _)) = RustConnection::connect(Some(&display)) else {
                    continue;
                };
                let root = c.setup().roots[0].root;
                let Some(wm) = prop_u32(&c, root, "_NET_SUPPORTING_WM_CHECK", AtomEnum::WINDOW)
                else {
                    continue;
                };
                if prop_bytes(&c, wm, "_NET_WM_NAME", atom(&c, "UTF8_STRING")).as_deref()
                    != Some(b"KWin")
                {
                    continue;
                }
                let ours = fs::read_dir("/proc")
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
                        parent == Some(s.kwin)
                            && args.first().is_some_and(|v| v.ends_with(b"Xwayland"))
                            && args.get(1) == Some(&display.as_bytes())
                    });
                if ours {
                    return Some(display);
                }
            }
            None
        });
        s.fixture();
        s
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
    fn spawn(&mut self, command: Command, name: &str) -> u32 {
        self.spawn_with(command, name, &[])
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
    fn fixture(&self) {
        let agent = self.root.join("home/.hermes/hermes-agent");
        for dir in [".hermes/bin", "pm", "hermes_cli"] {
            fs::create_dir_all(agent.join(dir)).unwrap();
        }
        fs::create_dir_all(self.root.join("home/.hermes/skins")).unwrap();
        fs::write(
            agent.join(".hermes/bin/hermes"),
            "#!/bin/sh\nexec /usr/bin/python3 fixture.py\n",
        )
        .unwrap();
        fs::write(
            agent.join("pm/environments.py"),
            "def selected_venv(root): return root\ndef site_packages(root): return root\n",
        )
        .unwrap();
        fs::write(
            agent.join("hermes_cli/config.py"),
            "def load_config_readonly(): return {}\n",
        )
        .unwrap();
        fs::write(agent.join("hermes_cli/skin_engine.py"), "def init_skin_from_config(c): pass\nclass Skin:\n name='Phase2 fixture'\n def get_color(self,key,fallback): return {'background':'#111111','ui_accent':'#91c99b','ui_text':'#dddddd','ui_primary':'#eeeeee','banner_dim':'#999999','ui_border':'#444444','status_bar_bg':'#222222','ui_error':'#ff8888'}.get(key,fallback)\ndef get_active_skin(): return Skin()\n").unwrap();
        // AI adapters use an isolated helper executable; no real Hermes invocation is possible.
        fs::write(
            self.root.join("inst/settings.json"),
            "{\"uiFont\":\"openrunde\",\"clickSounds\":false}",
        )
        .unwrap();
    }
    pub fn start_app(&mut self, overlay: bool) -> u32 {
        let mut app = Command::new(env!("CARGO_BIN_EXE_hermes-island"));
        app.args(["--test-instance", "--fixture-pty", "--data-dir"])
            .arg(self.root.join("inst"))
            .args(["--ui", "legacy"]);
        if overlay {
            app.arg("--overlay");
        }
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
    pub fn eis(&mut self) -> Eis {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/eis_input.py");
        let mut child = Command::new("/usr/bin/python3")
            .arg(script)
            .env_clear()
            .envs(self.env())
            .env("HERMES_ISLAND_SCRATCH", &self.root)
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(self.root.join("eis.log")).unwrap())
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
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(30)).unwrap(),
            format!("ready 0 0 {SCREEN_W} {SCREEN_H}")
        );
        Eis { input, rx }
    }
    pub fn connect(&self) -> RustConnection {
        RustConnection::connect(Some(&self.display)).unwrap().0
    }
    pub fn exited(&mut self, pid: u32) -> bool {
        self.children
            .iter_mut()
            .find(|c| c.id() == pid)
            .unwrap()
            .try_wait()
            .unwrap()
            .is_some()
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
            assert!(!alive, "private GUI process group {group} survived");
        }
        wait_for("GUI X sockets removed", Duration::from_secs(5), || {
            (sockets() == self.before).then_some(())
        });
    }
}
impl Drop for Stack {
    fn drop(&mut self) {
        if std::thread::panicking() {
            for name in ["app", "kwin", "eis"] {
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
    pub fn click(&mut self, x: i16, y: i16) {
        self.send(&format!("move {x} {y}"));
        sleep(Duration::from_millis(150));
        self.send("click");
    }
}
