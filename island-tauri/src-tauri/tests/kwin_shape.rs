//! Phase 1.5 gate: the Phase 1 window assertions repeated on a real KWin 6.8 with XWayland.
//!
//! Stack (all private, nothing touches the user's session):
//! * private `dbus-daemon` (no service dirs: nothing can be bus-activated), scratch
//!   `HOME`/`XDG_*`, `XDG_RUNTIME_DIR` mode 0700;
//! * `kwin_wayland --virtual --xwayland --socket hi-kw-<pid>` started with `env_clear()`, so it
//!   cannot inherit `WAYLAND_DISPLAY`, `DISPLAY` or the session bus. It makes its own Xwayland
//!   (X sockets in `/tmp/.X11-unix`); the display is discovered by diffing that directory and is
//!   only accepted if the X server's window manager is `KWin` and the server is a child
//!   `Xwayland` process of *our* kwin PID;
//! * the app is launched with `GDK_BACKEND=x11` against that Xwayland display, exactly like the
//!   production XWayland path.
//!
//! Everything is fail-closed: missing tools fail; every wait is bounded and panics; a click that
//! "must pass through" is only believed after a positive control proved the injected input path
//! delivers clicks in this very run; a click that "must be consumed" additionally requires KWin to
//! activate the island (otherwise a swallowed/lost click would look like consumption).
//!
//! Scenarios (serialised, each its own stack):
//! 1. `kwin_overlay_native_pill` — identity, position, no focus on show, empty ShapeInput before
//!    first show, still/hover/large physical click-through incl. frame area, no focus in hover.
//! 2. `kwin_preview_non_native_pill` — preview identity, ARGB visual, still pill accepts clicks
//!    only inside its rounded region.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use hermes_island_lib::{
    contract::island::{IslandMode, RoundedRect},
    window::shape::normalize_rows,
};
use std::{
    collections::BTreeSet,
    fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread::sleep,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            AtomEnum, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask,
            KeyPressEvent, PropMode, WindowClass,
        },
        Event,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

const X11_DIR: &str = "/tmp/.X11-unix";
/// `background_pixel(0x00ff00ff)` of the underlay window.
const UNDERLAY_RGB: [u8; 3] = [0xff, 0x00, 0xff];

// ---------------------------------------------------------------------------------------------
// sandbox: private bus + nested KWin/XWayland
// ---------------------------------------------------------------------------------------------

struct Stack {
    root: PathBuf,
    display: String,
    kwin_pid: u32,
    x_before: BTreeSet<String>,
    x_created: BTreeSet<String>,
    /// (name, child) in start order; stopped in reverse.
    children: Vec<(String, Child)>,
    stopped: bool,
}

fn x_sockets() -> BTreeSet<String> {
    fs::read_dir(X11_DIR)
        .map(|dir| {
            dir.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn proc_children_cmdline(parent: u32) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let Ok(dir) = fs::read_dir("/proc") else {
        return out;
    };
    for entry in dir.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.parse::<u32>().is_err() {
            continue;
        }
        let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // "pid (comm) S ppid ..." — comm may contain spaces/parens, so split after the last ')'.
        let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else {
            continue;
        };
        let ppid: u32 = rest
            .split_whitespace()
            .nth(1)
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        if ppid != parent {
            continue;
        }
        if let Ok(raw) = fs::read(entry.path().join("cmdline")) {
            out.push(
                raw.split(|b| *b == 0)
                    .filter(|p| !p.is_empty())
                    .map(|p| String::from_utf8_lossy(p).into_owned())
                    .collect(),
            );
        }
    }
    out
}

fn process_group_alive(pgid: u32) -> bool {
    let Ok(dir) = fs::read_dir("/proc") else {
        return false;
    };
    dir.filter_map(Result::ok).any(|entry| {
        let stat = fs::read_to_string(entry.path().join("stat")).unwrap_or_default();
        let after = stat.rsplit_once(')').map(|(_, r)| r).unwrap_or("");
        let mut fields = after.split_whitespace();
        let state = fields.next().unwrap_or("");
        let pgrp: u32 = fields.nth(1).and_then(|p| p.parse().ok()).unwrap_or(0);
        pgrp == pgid && state != "Z"
    })
}

impl Stack {
    fn new(tag: &str) -> Stack {
        for tool in ["kwin_wayland", "Xwayland", "dbus-daemon"] {
            require_tool(tool);
        }
        let root = std::env::temp_dir().join(format!("hi-kwin-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for d in [
            "config", "data", "cache", "state", "run", "home", "inst", "services",
        ] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::set_permissions(
            root.join("run"),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        let bus_path = root.join("bus");
        fs::write(
            root.join("bus.conf"),
            format!(
                "<!DOCTYPE busconfig PUBLIC \"-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN\" \
                 \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\">\n\
                 <busconfig><type>session</type><listen>unix:path={}</listen>\
                 <servicedir>{}</servicedir><auth>EXTERNAL</auth>\
                 <policy context=\"default\"><allow send_destination=\"*\" eavesdrop=\"true\"/>\
                 <allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>\n",
                bus_path.display(),
                root.join("services").display()
            ),
        )
        .unwrap();

        let mut stack = Stack {
            root,
            display: String::new(),
            kwin_pid: 0,
            x_before: x_sockets(),
            x_created: BTreeSet::new(),
            children: Vec::new(),
            stopped: false,
        };
        let log = |s: &Stack, n: &str| fs::File::create(s.root.join(n)).unwrap();
        let dbus = Command::new("dbus-daemon")
            .args(["--nofork", "--nosyslog", "--config-file"])
            .arg(stack.root.join("bus.conf"))
            .env_clear()
            .envs(stack.base_env())
            .process_group(0)
            .stdout(log(&stack, "dbus.log"))
            .stderr(log(&stack, "dbus.err"))
            .spawn()
            .unwrap();
        stack.children.push(("dbus-daemon".into(), dbus));
        wait_for("private bus socket", Duration::from_secs(5), || {
            bus_path.exists().then_some(())
        });

        let socket = format!("hi-kw-{}", std::process::id());
        let mut env = stack.base_env();
        env.push((
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", bus_path.display()),
        ));
        env.push(("QT_QPA_PLATFORM", "offscreen".into()));
        let kwin = Command::new("kwin_wayland")
            .args([
                "--virtual",
                "--xwayland",
                "--socket",
                socket.as_str(),
                "--width",
                &SCREEN_W.to_string(),
                "--height",
                &SCREEN_H.to_string(),
                "--no-lockscreen",
                "--no-global-shortcuts",
            ])
            .env_clear()
            .envs(env)
            .process_group(0)
            .stdout(log(&stack, "kwin.log"))
            .stderr(log(&stack, "kwin.err"))
            .spawn()
            .unwrap();
        stack.kwin_pid = kwin.id();
        stack.children.push(("kwin".into(), kwin));
        // Our compositor's own Wayland socket must appear inside the private runtime dir.
        let wayland_socket = stack.root.join("run").join(&socket);
        wait_for(
            "nested KWin wayland socket",
            Duration::from_secs(30),
            || {
                assert!(
                    stack.kwin_alive(),
                    "kwin exited early:\n{}",
                    stack.tail("kwin.err")
                );
                wayland_socket.exists().then_some(())
            },
        );
        stack.discover_display();
        stack
    }

    /// Start the libei pointer helper against OUR kwin's private bus. Fails closed.
    fn start_eis(&mut self) -> Eis {
        require_tool("python3");
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/eis_pointer.py");
        let mut env = self.base_env();
        env.push((
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", self.root.join("bus").display()),
        ));
        env.push(("HERMES_ISLAND_SCRATCH", self.root.display().to_string()));
        let mut child = Command::new("/usr/bin/python3")
            .arg(script)
            .env_clear()
            .envs(env)
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(self.root.join("eis.err")).unwrap())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        self.children.push(("eis".into(), child));
        let (tx, replies) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
            {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let ready = replies
            .recv_timeout(Duration::from_secs(30))
            .unwrap_or_else(|e| {
                panic!(
                    "pointer helper never became ready: {e}\n{}",
                    self.tail("eis.err")
                )
            });
        assert_eq!(
            ready,
            format!("ready 0 0 {SCREEN_W} {SCREEN_H}"),
            "EIS absolute-pointer region must be the whole virtual screen\n{}",
            self.tail("eis.err")
        );
        Eis { stdin, replies }
    }

    /// Composited capture through KWin's own `org.kde.KWin.ScreenShot2` on the private bus.
    fn capture(&mut self, name: &str) -> Capture {
        require_tool("python3");
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/kwin_capture.py");
        let raw = self.root.join(format!("{name}.raw"));
        let mut env = self.base_env();
        env.push((
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", self.root.join("bus").display()),
        ));
        env.push(("HERMES_ISLAND_SCRATCH", self.root.display().to_string()));
        let out = Command::new("/usr/bin/python3")
            .arg(script)
            .arg(&raw)
            .env_clear()
            .envs(env)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            out.status.success(),
            "KWin capture failed: {stdout} {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let field = |key: &str| -> usize {
            let pat = format!("\"{key}\": ");
            let rest = stdout
                .split(&pat)
                .nth(1)
                .unwrap_or_else(|| panic!("no {key} in {stdout}"));
            rest.split([',', '}'])
                .next()
                .unwrap()
                .trim()
                .parse()
                .unwrap()
        };
        let (width, height, stride, format) = (
            field("width"),
            field("height"),
            field("stride"),
            field("format"),
        );
        // QImage::Format -> byte offsets of r,g,b
        let order = match format {
            4..=6 => [2, 1, 0], // RGB32 / ARGB32 (+premultiplied): B,G,R,A in memory (little endian)
            16..=18 => [0, 1, 2], // RGBX8888 / RGBA8888 (+premultiplied)
            other => panic!("unsupported capture format {other}: {stdout}"),
        };
        let data = fs::read(&raw).unwrap();
        assert_eq!(
            (width, height),
            (usize::from(SCREEN_W), usize::from(SCREEN_H)),
            "capture is not the whole virtual screen: {stdout}"
        );
        assert!(
            data.len() >= stride * height,
            "short capture: {} bytes, {stdout}",
            data.len()
        );
        Capture {
            width,
            height,
            stride,
            order,
            data,
        }
    }

    fn kwin_alive(&mut self) -> bool {
        self.children
            .iter_mut()
            .find(|(n, _)| n == "kwin")
            .is_some_and(|(_, c)| matches!(c.try_wait(), Ok(None)))
    }

    fn tail(&self, name: &str) -> String {
        fs::read_to_string(self.root.join(name))
            .unwrap_or_default()
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The Xwayland display of *our* KWin. Candidates are the X sockets that appeared since the
    /// stack started; one is accepted only if (a) its window manager is named `KWin` and (b) an
    /// `Xwayland :N` process is a direct child of our kwin PID. Xwayland starts lazily on the
    /// first connection, hence the connect attempts.
    fn discover_display(&mut self) {
        let kwin_pid = self.kwin_pid;
        let before = self.x_before.clone();
        let (display, created) =
            wait_for("KWin's Xwayland display", Duration::from_secs(40), || {
                assert!(self.kwin_alive(), "kwin died:\n{}", self.tail("kwin.err"));
                let created: BTreeSet<String> = x_sockets().difference(&before).cloned().collect();
                for name in &created {
                    let display = format!(":{}", name.trim_start_matches('X'));
                    let Ok((conn, _)) = RustConnection::connect(Some(&display)) else {
                        continue;
                    };
                    let root = conn.setup().roots[0].root;
                    let Some(check) =
                        prop_u32(&conn, root, "_NET_SUPPORTING_WM_CHECK", AtomEnum::WINDOW)
                    else {
                        continue;
                    };
                    let utf8 = atom(&conn, "UTF8_STRING");
                    let wm = prop_bytes(&conn, check, "_NET_WM_NAME", utf8).unwrap_or_default();
                    let is_ours_child = proc_children_cmdline(kwin_pid).iter().any(|c| {
                        c.first().is_some_and(|p| p.ends_with("Xwayland"))
                            && c.get(1) == Some(&display)
                    });
                    if wm == b"KWin" && is_ours_child {
                        return Some((display, created.clone()));
                    }
                }
                None
            });
        self.display = display;
        self.x_created = created;
    }

    fn base_env(&self) -> Vec<(&'static str, String)> {
        let path = |d: &str| self.root.join(d).display().to_string();
        vec![
            ("PATH", std::env::var("PATH").unwrap_or_default()),
            ("LANG", "C.UTF-8".into()),
            ("HOME", path("home")),
            ("XDG_CONFIG_HOME", path("config")),
            ("XDG_DATA_HOME", path("data")),
            ("XDG_CACHE_HOME", path("cache")),
            ("XDG_STATE_HOME", path("state")),
            ("XDG_RUNTIME_DIR", path("run")),
        ]
    }

    fn app_env(&self) -> Vec<(&'static str, String)> {
        let mut env = self.base_env();
        env.extend([
            ("GDK_BACKEND", "x11".to_owned()),
            ("NO_AT_BRIDGE", "1".into()),
            ("GTK_USE_PORTAL", "0".into()),
            ("GIO_USE_PORTAL", "0".into()),
            ("HERMES_ISLAND_TEST", "1".into()),
            ("RUST_LOG", "info".into()),
            // GTK3 selects XInput2 and ignores core key events from `XSendEvent`; core device
            // events make the focus-free debug-key driver work. Test-only: production leaves GDK's
            // default (XI2). Pointer/focus behaviour under test is KWin's and does not depend on it.
            ("GDK_CORE_DEVICE_EVENTS", "1".into()),
            ("DISPLAY", self.display.clone()),
            (
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}", self.root.join("bus").display()),
            ),
        ]);
        env
    }

    fn connect(&self) -> RustConnection {
        wait_for("X connection", Duration::from_secs(10), || {
            RustConnection::connect(Some(&self.display))
                .ok()
                .map(|(c, _)| c)
        })
    }

    fn start_app(&mut self, overlay: bool) -> u32 {
        let data = self.root.join("inst");
        let data = data.to_str().unwrap().to_owned();
        let mut args = vec!["--test-instance", "--data-dir", data.as_str()];
        if overlay {
            args.push("--overlay");
        }
        let out = fs::File::create(self.root.join("app.log")).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_hermes-island"))
            .args(&args)
            .env_clear()
            .envs(self.app_env())
            .process_group(0)
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .unwrap();
        let pid = child.id();
        self.children.push(("app".into(), child));
        pid
    }

    fn app_exited(&mut self) -> Option<String> {
        let (_, child) = self.children.iter_mut().find(|(n, _)| n == "app")?;
        child.try_wait().ok().flatten().map(|s| s.to_string())
    }

    /// Stop everything and prove nothing leaked: every process group is gone and the X sockets
    /// this stack created are removed.
    fn shutdown(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        let groups: Vec<u32> = self.children.iter().map(|(_, c)| c.id()).collect();
        for (_, child) in self.children.iter_mut().rev() {
            stop_group(child);
        }
        for pgid in groups {
            assert!(
                !process_group_alive(pgid),
                "process group {pgid} survived shutdown"
            );
        }
        let now = x_sockets();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && !self.x_created.is_disjoint(&x_sockets()) {
            sleep(Duration::from_millis(50));
        }
        let leaked: Vec<_> = x_sockets().difference(&self.x_before).cloned().collect();
        assert!(
            leaked.is_empty(),
            "X sockets leaked in {X11_DIR}: {leaked:?} (before: {:?}, now: {now:?})",
            self.x_before
        );
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        if std::thread::panicking() {
            for log in ["app.log", "kwin.err", "kwin.log", "dbus.err"] {
                eprintln!("---- {log} (last lines) ----\n{}", self.tail(log));
            }
        }
        if !self.stopped {
            for (_, child) in self.children.iter_mut().rev() {
                stop_group(child);
            }
        }
        for _ in 0..20 {
            if fs::remove_dir_all(&self.root).is_ok() || !self.root.exists() {
                break;
            }
            sleep(Duration::from_millis(50));
        }
    }
}

/// A composited screenshot of the whole nested KWin workspace (not a window readback).
struct Capture {
    width: usize,
    height: usize,
    stride: usize,
    /// byte order of one pixel: offsets of (r, g, b) inside its 4 bytes
    order: [usize; 3],
    data: Vec<u8>,
}

impl Capture {
    fn rgb(&self, x: i32, y: i32) -> [u8; 3] {
        assert!(x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height);
        let i = y as usize * self.stride + x as usize * 4;
        [
            self.data[i + self.order[0]],
            self.data[i + self.order[1]],
            self.data[i + self.order[2]],
        ]
    }
}

/// Physical pointer input through KWin's own libei server (see `tests/helpers/eis_pointer.py`).
struct Eis {
    stdin: std::process::ChildStdin,
    replies: std::sync::mpsc::Receiver<String>,
}

impl Eis {
    fn send(&mut self, line: &str) {
        use std::io::Write;
        writeln!(self.stdin, "{line}").expect("pointer helper died (stdin closed)");
        let reply = self
            .replies
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|e| panic!("pointer helper gave no reply to `{line}`: {e}"));
        assert_eq!(reply, "ok", "pointer helper rejected `{line}`");
    }
    fn move_to(&mut self, x: i16, y: i16) {
        self.send(&format!("move {x} {y}"));
    }
    fn click(&mut self) {
        self.send("click");
    }
}

// ---------------------------------------------------------------------------------------------
// input + X helpers specific to this stack
// ---------------------------------------------------------------------------------------------

fn set_atoms(conn: &RustConnection, window: u32, prop: &str, values: &[&str]) {
    let list: Vec<u32> = values.iter().map(|v| atom(conn, v)).collect();
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atom(conn, prop),
        AtomEnum::ATOM,
        &list,
    )
    .unwrap();
}

/// A managed window of the given EWMH type. `desktop` = the underlay (acts like the Plasma
/// desktop below the island), otherwise an ordinary decorated-by-KWin application window.
fn create_managed_window(
    conn: &RustConnection,
    root: u32,
    (x, y, w, h): (i16, i16, u16, u16),
    kind: &str,
    title: &str,
) -> u32 {
    let window = conn.generate_id().unwrap();
    conn.create_window(
        0,
        window,
        root,
        x,
        y,
        w,
        h,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new()
            .background_pixel(0x00ff00ff)
            .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE),
    )
    .unwrap();
    set_atoms(conn, window, "_NET_WM_WINDOW_TYPE", &[kind]);
    conn.change_property8(
        PropMode::REPLACE,
        window,
        atom(conn, "_NET_WM_NAME"),
        atom(conn, "UTF8_STRING"),
        title.as_bytes(),
    )
    .unwrap();
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        format!("{title}\0{title}\0").as_bytes(),
    )
    .unwrap();
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atom(conn, "_NET_WM_PID"),
        AtomEnum::CARDINAL,
        &[std::process::id()],
    )
    .unwrap();
    // Honour the requested position: "user specified" (USPosition|USSize) in WM_NORMAL_HINTS.
    let hints: [u32; 18] = [
        3,
        x as u32,
        y as u32,
        u32::from(w),
        u32::from(h),
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    conn.change_property32(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_NORMAL_HINTS,
        AtomEnum::WM_SIZE_HINTS,
        &hints,
    )
    .unwrap();
    conn.map_window(window).unwrap();
    conn.flush().unwrap();
    window
}

fn active_window(conn: &RustConnection, root: u32) -> u32 {
    prop_u32(conn, root, "_NET_ACTIVE_WINDOW", AtomEnum::WINDOW).unwrap_or(0)
}

/// Root-relative origin of a window (client area).
fn origin_of(conn: &RustConnection, window: u32, root: u32) -> (i16, i16) {
    let r = conn
        .translate_coordinates(window, root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    (r.dst_x, r.dst_y)
}

/// The top-level ancestor of `window` that is a direct child of the root (KWin's frame window).
fn frame_of(conn: &RustConnection, window: u32, root: u32) -> u32 {
    let mut current = window;
    loop {
        let parent = conn.query_tree(current).unwrap().reply().unwrap().parent;
        if parent == root || parent == 0 {
            return current;
        }
        current = parent;
    }
}

fn frame_extents(conn: &RustConnection, window: u32) -> Option<[u32; 4]> {
    let bytes = prop_bytes(conn, window, "_NET_FRAME_EXTENTS", AtomEnum::CARDINAL)?;
    let mut out = [0u32; 4];
    for (slot, chunk) in out.iter_mut().zip(bytes.chunks(4)) {
        *slot = u32::from_ne_bytes(chunk.try_into().ok()?);
    }
    Some(out)
}

/// Deliver a key to the island window with `SendEvent` — no focus involved at all, which is the
/// point: the island must be drivable (and must stay unfocused) while another window is active.
fn send_key(conn: &RustConnection, window: u32, root: u32, ch: char) {
    let keycode = {
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let map = conn
            .get_keyboard_mapping(min, max - min + 1)
            .unwrap()
            .reply()
            .unwrap();
        let per = usize::from(map.keysyms_per_keycode);
        let index = map
            .keysyms
            .chunks(per)
            .position(|c| c.contains(&(ch as u32)))
            .unwrap_or_else(|| panic!("no keycode for {ch:?}"));
        min + index as u8
    };
    for (response_type, mask) in [(2u8, EventMask::KEY_PRESS), (3u8, EventMask::KEY_RELEASE)] {
        let event = KeyPressEvent {
            response_type,
            detail: keycode,
            sequence: 0,
            time: 0,
            root,
            event: window,
            child: 0,
            root_x: 0,
            root_y: 0,
            event_x: 0,
            event_y: 0,
            state: 0u16.into(),
            same_screen: true,
        };
        conn.send_event(false, window, mask, event).unwrap();
    }
    conn.flush().unwrap();
}

/// Ask KWin to activate `window` the way a pager/taskbar would (EWMH source 2).
fn activate(conn: &RustConnection, root: u32, window: u32) {
    let event = ClientMessageEvent::new(
        32,
        window,
        atom(conn, "_NET_ACTIVE_WINDOW"),
        [2u32, 0, 0, 0, 0],
    );
    conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )
    .unwrap();
    conn.flush().unwrap();
}

struct Probe<'a> {
    conn: &'a RustConnection,
    root: u32,
    underlay: u32,
    eis: std::cell::RefCell<Eis>,
}

impl Probe<'_> {
    fn move_to(&self, x: i16, y: i16) {
        self.eis.borrow_mut().move_to(x, y);
    }

    /// Real XTest click at root coordinates. Returns true if the underlay (a different client,
    /// stacked below the island) received a ButtonPress. `wait` bounds how long we look.
    fn underlay_gets(&self, x: i16, y: i16, wait: Duration) -> bool {
        while self.conn.poll_for_event().unwrap().is_some() {}
        self.move_to(x, y);
        sleep(Duration::from_millis(120)); // let KWin route the motion before the button
        self.eis.borrow_mut().click();
        let deadline = Instant::now() + wait;
        loop {
            self.conn.get_input_focus().unwrap().reply().unwrap();
            while let Some(event) = self.conn.poll_for_event().unwrap() {
                if matches!(event, Event::ButtonPress(ref e) if e.event == self.underlay) {
                    return true;
                }
            }
            if Instant::now() >= deadline {
                return false;
            }
            sleep(Duration::from_millis(25));
        }
    }

    /// Must reach the underlay (pass-through); returns as soon as it does.
    fn assert_passes(&self, what: &str, x: i16, y: i16) {
        assert!(
            self.underlay_gets(x, y, Duration::from_secs(3)),
            "{what}: click at ({x},{y}) did NOT pass through to the window below"
        );
    }

    /// Must be consumed by the island: the underlay stays silent AND KWin activates the island
    /// (positive proof the click landed on it rather than being lost).
    fn assert_consumed(&self, what: &str, x: i16, y: i16, island: u32) {
        // Make sure the island is not already active, otherwise activation proves nothing.
        activate(self.conn, self.root, self.underlay);
        wait_for(
            &format!("{what}: underlay active before click"),
            Duration::from_secs(5),
            || (active_window(self.conn, self.root) != island).then_some(()),
        );
        let got = self.underlay_gets(x, y, Duration::from_millis(700));
        assert!(
            !got,
            "{what}: click at ({x},{y}) leaked to the window below"
        );
        wait_for(
            &format!("{what}: island activated by the click (consumed)"),
            Duration::from_secs(5),
            || (active_window(self.conn, self.root) == island).then_some(()),
        );
    }
}

/// The composited screen must be what the window pixels say: where the island window is fully
/// transparent we see the window below (the underlay colour), where it is opaque we see exactly
/// the island's own pixels. Over the whole 1180x820 window area, so a stale pixel, a frame, a
/// shadow or a compositor effect around the island all fail.
fn assert_composited(
    sb: &mut Stack,
    conn: &RustConnection,
    window: u32,
    origin: (i16, i16),
    spec: &Spec,
    tag: &str,
) {
    sleep(Duration::from_millis(400)); // let the Wayland surface commit + a composited frame happen
    let shot = Shot::grab(conn, window, WIN_W, WIN_H);
    let cap = sb.capture(&format!("{tag}-{}", spec.name));
    let bg = cap.rgb(40, 500); // desktop underlay, nowhere near the island
    assert_eq!(
        bg, UNDERLAY_RGB,
        "{tag}/{}: underlay colour in the capture",
        spec.name
    );
    let (mut transparent, mut opaque, mut bad) = (0usize, 0usize, Vec::new());
    for y in 0..i32::from(WIN_H) {
        for x in 0..i32::from(WIN_W) {
            let [r, g, b, a] = shot.px(x, y);
            let got = cap.rgb(i32::from(origin.0) + x, i32::from(origin.1) + y);
            let want = match a {
                0 => {
                    transparent += 1;
                    Some(bg)
                }
                255 => {
                    opaque += 1;
                    Some([r, g, b])
                }
                _ => None, // anti-aliased edge: any blend is acceptable
            };
            if let Some(want) = want {
                let close = got.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 2);
                if !close {
                    bad.push((x, y, a, got, want));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{tag}/{}: {} composited pixels differ from the window (x, y, alpha, got, want), first: {:?}",
        spec.name,
        bad.len(),
        &bad[..bad.len().min(6)]
    );
    assert!(
        transparent > 100_000 && opaque > 2_000,
        "{tag}/{}: implausible capture ({transparent} transparent, {opaque} opaque pixels)",
        spec.name
    );
    // evidence: the composited workspace around the island
    let mut rgba = Vec::new();
    for y in 0..i32::from(SCREEN_H) {
        for x in 0..i32::from(SCREEN_W) {
            let [r, g, b] = cap.rgb(x, y);
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    image::RgbaImage::from_raw(u32::from(SCREEN_W), u32::from(SCREEN_H), rgba)
        .unwrap()
        .save(Evidence::new("phase1_5").path(&format!("{tag}-{}-KWIN-COMPOSITED.png", spec.name)))
        .unwrap();
}

fn wait_region(conn: &RustConnection, window: u32, spec: &Spec, native: bool, tag: &str) {
    let want = expected_region(spec, native);
    let rows = wait_for(
        &format!("{tag}/{}: ShapeInput == expected", spec.name),
        Duration::from_secs(20),
        || {
            let got = normalize_rows(&input_region(conn, window));
            (got == want).then_some(got)
        },
    );
    if native && spec.mode == IslandMode::Still {
        assert!(rows.is_empty(), "native-pill still must be EMPTY");
    } else {
        assert_rounded_region(&rows, spec);
    }
}

fn find_window(sb: &mut Stack, conn: &RustConnection, root: u32, pid: u32) -> u32 {
    wait_for("island window mapped", Duration::from_secs(60), || {
        assert!(
            sb.app_exited().is_none(),
            "app exited early: {:?}\n{}",
            sb.app_exited(),
            sb.tail("app.log")
        );
        find_island_window(conn, root, pid)
    })
}

/// Everything about "the window maps with the frozen identity, where asked, unfocused".
fn assert_mapped_correctly(
    conn: &RustConnection,
    root: u32,
    window: u32,
    pid: u32,
    overlay: bool,
) -> (i16, i16) {
    assert_identity(conn, window, pid, overlay);
    let geo = conn.get_geometry(window).unwrap().reply().unwrap();
    assert_eq!((geo.width, geo.height), (WIN_W, WIN_H));
    assert_eq!(
        geo.depth, 32,
        "island window must keep its 32-bit ARGB visual under KWin"
    );
    let origin = origin_of(conn, window, root);
    assert_eq!(
        origin,
        (((SCREEN_W - WIN_W) / 2) as i16, 0),
        "KWin did not honour the requested window position"
    );
    // KWin must not wrap the island in a decoration: no extents, and the frame IS the client.
    assert!(
        frame_extents(conn, window).is_none_or(|e| e == [0, 0, 0, 0]),
        "KWin added frame extents to the island: {:?}",
        frame_extents(conn, window)
    );
    let frame = frame_of(conn, window, root);
    let fgeo = conn.get_geometry(frame).unwrap().reply().unwrap();
    assert_eq!(
        (fgeo.x, fgeo.y, fgeo.width, fgeo.height),
        (origin.0, origin.1, WIN_W, WIN_H),
        "KWin frame window must coincide with the client (no decoration area)"
    );
    // not focused on show (request signal + observed outcome)
    assert_not_focused_on_show(conn, root, window);
    origin
}

fn assert_never_focused(conn: &RustConnection, root: u32, window: u32, when: &str) {
    assert_ne!(
        active_window(conn, root),
        window,
        "{when}: island is _NET_ACTIVE_WINDOW"
    );
    assert_ne!(
        conn.get_input_focus().unwrap().reply().unwrap().focus,
        window,
        "{when}: island holds X input focus"
    );
    assert_no_focus_requested(conn, window);
}

/// Drive a mode through the page's debug key without any focus, wait for the X server's region.
fn goto(conn: &RustConnection, root: u32, window: u32, spec: &Spec, native: bool, tag: &str) {
    send_key(conn, window, root, spec.key);
    wait_region(conn, window, spec, native, tag);
    // The window's own ARGB pixels (read through KWin's Xwayland) must show the same state:
    // transparent outside the island, no stale pixels after shrinking.
    verify_pixels(conn, window, spec, &Evidence::new("phase1_5"), tag);
}

fn setup_underlay(conn: &RustConnection, root: u32) -> u32 {
    let underlay = create_managed_window(
        conn,
        root,
        (0, 0, SCREEN_W, SCREEN_H),
        "_NET_WM_WINDOW_TYPE_DESKTOP",
        "hi-underlay",
    );
    wait_for("underlay mapped", Duration::from_secs(10), || {
        let attrs = conn.get_window_attributes(underlay).ok()?.reply().ok()?;
        (attrs.map_state == x11rb::protocol::xproto::MapState::VIEWABLE).then_some(())
    });
    underlay
}

// ---------------------------------------------------------------------------------------------
// scenarios
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn kwin_overlay_native_pill() {
    let _g = lock();
    let mut sb = Stack::new("native");
    let conn = sb.connect();
    let root = conn.setup().roots[0].root;
    let underlay = setup_underlay(&conn, root);
    let eis = sb.start_eis();
    let probe = Probe {
        conn: &conn,
        root,
        underlay,
        eis: std::cell::RefCell::new(eis),
    };

    // Positive control for the whole input path, before the island exists: an XTest click at an
    // arbitrary point must reach the underlay. Without this, every "must pass through" below
    // could be a lie in either direction.
    probe.assert_passes("control (no island yet)", 700, 400);

    let pid = sb.start_app(true);
    let window = find_window(&mut sb, &conn, root, pid);

    // identity + position + frame + not focused on show
    let (ox, oy) = assert_mapped_correctly(&conn, root, window, pid, true);
    // the ShapeInput is explicitly EMPTY at the very first observation (applied before show)
    assert!(
        input_region(&conn, window).is_empty(),
        "native-pill still must be an explicitly EMPTY input region before/at first show"
    );

    // Positive control for the focus assertions: KWin DOES activate an ordinary new window.
    let normal = create_managed_window(
        &conn,
        root,
        (1450, 880, 100, 80),
        "_NET_WM_WINDOW_TYPE_NORMAL",
        "hi-normal-control",
    );
    wait_for(
        "KWin activates an ordinary control window",
        Duration::from_secs(10),
        || (active_window(&conn, root) == normal).then_some(()),
    );
    // Same control tells us whether KWin decorates windows in this sandbox at all (the island
    // frame assertions above are only meaningful if it would).
    let control_extents = frame_extents(&conn, normal);
    println!("control window _NET_FRAME_EXTENTS = {control_extents:?}");
    assert!(
        control_extents.is_some_and(|e| e[2] > 0),
        "positive control failed: KWin did not decorate an ordinary window ({control_extents:?}), \
         so the frame/decoration-area assertions would be vacuous"
    );
    conn.destroy_window(normal).unwrap();
    conn.flush().unwrap();
    assert_never_focused(&conn, root, window, "after show");

    let (wx, wy) = (|x: i32| ox + x as i16, |y: i32| oy + y as i16);
    let at = |x: i32, y: i32| (wx(x), wy(y));
    let rect = |s: &Spec| {
        let RoundedRect {
            x,
            y,
            width,
            height,
            ..
        } = s.rect;
        (x as i32, y as i32, width as i32, height as i32)
    };

    assert_composited(&mut sb, &conn, window, (ox, oy), &STILL, "kwin-initial");
    // ---- still + native pill: the whole window is click-through ----
    let (px, py) = at(WIN_W as i32 / 2, 26); // centre of where the pill would be
    probe.assert_passes("still: pill area", px, py);
    let (cx, cy) = at(WIN_W as i32 / 2, WIN_H as i32 / 2);
    probe.assert_passes("still: window centre", cx, cy);
    assert_never_focused(&conn, root, window, "still, after clicks through it");

    // ---- hover ----
    goto(&conn, root, window, &HOVER, true, "kwin");
    assert_composited(&mut sb, &conn, window, (ox, oy), &HOVER, "kwin");
    let (x, y, w, h) = rect(&HOVER);
    // no focus in hover: not on entry, not while the pointer sits inside the card
    let (mx, my) = at(x + w / 2, y + h / 2);
    probe.move_to(mx, my);
    sleep(Duration::from_millis(600));
    assert_never_focused(
        &conn,
        root,
        window,
        "hover (pointer inside the card, no click)",
    );
    let focus_before = conn.get_input_focus().unwrap().reply().unwrap().focus;
    assert_ne!(focus_before, window);

    // accepted inside the rounded region (consumed + activated), passed outside it
    let (ix, iy) = at(x + w / 2, y + h / 2);
    probe.assert_consumed("hover: inside card", ix, iy, window);
    for (what, (px, py)) in [
        ("hover: cut-off rounded corner", at(x + 1, y + 1)),
        (
            "hover: beside the card (right, inside window)",
            at(x + w + 20, y + h / 2),
        ),
        (
            "hover: beside the card (left, inside window)",
            at(x - 20, y + h / 2),
        ),
        ("hover: below the card", at(x + w / 2, y + h + 40)),
        ("hover: above the card (inside window)", at(x + w / 2, 3)),
        (
            "hover: bottom-right of the window",
            at(WIN_W as i32 - 2, WIN_H as i32 - 2),
        ),
    ] {
        probe.assert_passes(what, px, py);
    }
    // KWin frame / decoration area = just outside every edge of the client rectangle.
    for (what, (px, py)) in [
        ("frame: left edge", (ox - 1, wy(h / 2))),
        ("frame: right edge", (ox + WIN_W as i16, wy(h / 2))),
        (
            "frame: below bottom edge",
            (wx(x + w / 2), oy + WIN_H as i16),
        ),
        (
            "frame: below bottom-left corner",
            (ox - 1, oy + WIN_H as i16),
        ),
        (
            "frame: below bottom-right corner",
            (ox + WIN_W as i16, oy + WIN_H as i16),
        ),
        ("frame: left edge, top row", (ox - 1, oy)),
        ("frame: right edge, top row", (ox + WIN_W as i16, oy)),
    ] {
        probe.assert_passes(what, px, py);
    }

    // ---- large ----
    goto(&conn, root, window, &LARGE, true, "kwin");
    assert_composited(&mut sb, &conn, window, (ox, oy), &LARGE, "kwin");
    let (x, y, w, h) = rect(&LARGE);
    let (ix, iy) = at(x + w / 2, y + h / 2);
    probe.assert_consumed("large: inside surface", ix, iy, window);
    for (what, (px, py)) in [
        ("large: cut-off rounded corner", at(x + 1, y + 1)),
        (
            "large: beside the surface (right, inside window)",
            at(x + w + 20, y + h / 2),
        ),
        (
            "large: beside the surface (left, inside window)",
            at(x - 20, y + h / 2),
        ),
        (
            "large: below the surface (inside window)",
            at(x + w / 2, y + h + 40),
        ),
        ("large: bottom-left of the window", at(2, WIN_H as i32 - 2)),
    ] {
        probe.assert_passes(what, px, py);
    }
    for (what, (px, py)) in [
        ("large/frame: left edge", (ox - 1, wy(h / 2))),
        ("large/frame: right edge", (ox + WIN_W as i16, wy(h / 2))),
        (
            "large/frame: below bottom edge",
            (wx(x + w / 2), oy + WIN_H as i16),
        ),
    ] {
        probe.assert_passes(what, px, py);
    }

    // ---- back to still: former surface area passes through again ----
    activate(&conn, root, underlay);
    goto(&conn, root, window, &STILL, true, "kwin");
    assert_composited(&mut sb, &conn, window, (ox, oy), &STILL, "kwin"); // shrink: no stale pixels
    probe.assert_passes("still again: former large centre", ix, iy);
    probe.assert_passes("still again: former hover centre", mx, my);
    assert!(input_region(&conn, window).is_empty());

    // the identity/position/ARGB facts still hold after the whole cycle
    assert_mapped_after_cycle(&conn, root, window, pid, (ox, oy));
    assert!(sb.app_exited().is_none(), "app died during the cycle");
    assert!(sb.kwin_alive(), "KWin died during the test");
    sb.shutdown();
}

fn assert_mapped_after_cycle(
    conn: &RustConnection,
    root: u32,
    window: u32,
    pid: u32,
    origin: (i16, i16),
) {
    assert_identity(conn, window, pid, true);
    assert_eq!(
        origin_of(conn, window, root),
        origin,
        "window moved during the cycle"
    );
}

#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn kwin_preview_non_native_pill() {
    let _g = lock();
    let mut sb = Stack::new("preview");
    let conn = sb.connect();
    let root = conn.setup().roots[0].root;
    let underlay = setup_underlay(&conn, root);
    let eis = sb.start_eis();
    let probe = Probe {
        conn: &conn,
        root,
        underlay,
        eis: std::cell::RefCell::new(eis),
    };
    probe.assert_passes("control (no island yet)", 700, 400);

    let pid = sb.start_app(false);
    let window = find_window(&mut sb, &conn, root, pid);
    let (ox, oy) = assert_mapped_correctly(&conn, root, window, pid, false);
    assert_never_focused(&conn, root, window, "preview after show");
    let at = |x: i32, y: i32| (ox + x as i16, oy + y as i16);

    // non-native: the first geometry report opens the still pill (rounded, 1 px inset per side)
    wait_region(&conn, window, &STILL, false, "kwin-preview");
    verify_pixels(
        &conn,
        window,
        &STILL,
        &Evidence::new("phase1_5"),
        "kwin-preview-initial",
    );

    // Hover, driven without focus, pointer inside the card, no click: still not focused.
    goto(&conn, root, window, &HOVER, false, "kwin-preview");
    assert_composited(&mut sb, &conn, window, (ox, oy), &HOVER, "kwin-preview");
    let (hx, hy) = at(590, 80);
    probe.move_to(hx, hy);
    sleep(Duration::from_millis(600));
    assert_never_focused(
        &conn,
        root,
        window,
        "preview hover (pointer inside, no click)",
    );
    // (From the first real click on, GTK legitimately publishes a user time, so the user-time
    // signal is only meaningful up to here; the observed-outcome checks continue below.)
    probe.assert_consumed("preview hover: inside card", hx, hy, window);
    probe.assert_passes("preview hover: beside card", at(840, 80).0, at(840, 80).1);
    probe.assert_passes("preview hover: cut-off corner", at(381, 9).0, at(381, 9).1);

    goto(&conn, root, window, &LARGE, false, "kwin-preview");
    probe.assert_consumed(
        "preview large: inside",
        at(590, 300).0,
        at(590, 300).1,
        window,
    );
    probe.assert_passes(
        "preview large: beside (right)",
        at(1150, 300).0,
        at(1150, 300).1,
    );
    probe.assert_passes("preview large: below", at(590, 760).0, at(590, 760).1);

    // still pill: accepted only inside its rounded region
    activate(&conn, root, underlay);
    goto(&conn, root, window, &STILL, false, "kwin-preview");
    probe.assert_consumed(
        "preview still: inside pill",
        at(590, 26).0,
        at(590, 26).1,
        window,
    );
    for (what, (px, py)) in [
        ("preview still: left of pill", at(500, 26)),
        ("preview still: right of pill", at(680, 26)),
        ("preview still: below pill", at(590, 60)),
        ("preview still: rounded-off pill corner", at(516, 9)),
        ("preview still: former large centre", at(590, 300)),
    ] {
        probe.assert_passes(what, px, py);
    }
    for (what, (px, py)) in [
        ("preview frame: left edge", (ox - 1, oy + 20)),
        ("preview frame: right edge", (ox + WIN_W as i16, oy + 20)),
        (
            "preview frame: below bottom edge",
            (ox + 590, oy + WIN_H as i16),
        ),
    ] {
        probe.assert_passes(what, px, py);
    }
    assert!(sb.app_exited().is_none(), "app died during the cycle");
    assert!(sb.kwin_alive(), "KWin died during the test");
    sb.shutdown();
}
