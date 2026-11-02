//! Shared fixtures for the Phase 3 nested-KWin scenarios (native host + state actor).
//! Everything runs inside the private `Stack` (own KWin, bus, HOME and data dir); the
//! `anchor-client.py` used is a BYTE-IDENTICAL copy of `island/anchor-client.py` rooted in a
//! scratch replica whose `.island-data-overlay` points at the private test instance.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(dead_code)] // each scenario uses a different subset
use crate::common::{input_region, stop_group, wait_for};
use crate::gui_stack::Stack;
use hermes_island_lib::{
    contract::island::{IslandMode, RoundedRect},
    window::shape::{normalize_rows, region_rows},
};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::{symlink, PermissionsExt},
        net::UnixStream,
        process::CommandExt,
    },
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread::sleep,
    time::Duration,
};

pub struct Owned(pub Child);
impl Drop for Owned {
    fn drop(&mut self) {
        stop_group(&mut self.0);
    }
}

pub fn command(s: &Stack, data: &Path) -> Command {
    let mut app = Command::new(env!("CARGO_BIN_EXE_hermes-island"));
    app.args([
        "--test-instance",
        "--overlay",
        "--fixture-pty",
        "--ui",
        "legacy",
        "--data-dir",
    ])
    .arg(data)
    .env_clear()
    .envs(s.env())
    .env("DISPLAY", &s.display)
    .env("GDK_BACKEND", "x11")
    .env("HERMES_ISLAND_TEST", "1")
    .env("NO_AT_BRIDGE", "1")
    .env("GTK_USE_PORTAL", "0")
    .env("GIO_USE_PORTAL", "0")
    .env(
        "PATH",
        format!("{}:/usr/bin:/bin", s.root.join("bin").display()),
    )
    .process_group(0);
    app
}

/// Evaluate a JS expression in the island page (test-control socket; promises are awaited).
pub fn eval(s: &Stack, js: &str) -> Value {
    let mut stream = UnixStream::connect(s.root.join("inst/test-control.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    writeln!(stream, "{}", json!({"op":"eval","js":js})).unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["ok"], true, "{value}");
    value["value"].clone()
}

/// The REAL `anchor-client.py` (byte-identical copy) talking to the private instance.
pub fn client(s: &Stack, op: &str) -> Value {
    let result = Command::new("/usr/bin/python3")
        .arg(s.root.join("replica/island/anchor-client.py"))
        .arg(op)
        .env_clear()
        .envs(s.env())
        .output()
        .unwrap();
    assert!(result.status.success());
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        value["online"], true,
        "real anchor client failed {op}: {value}"
    );
    value
}

/// `status.json` of the private instance (must always be a regular file).
pub fn status(s: &Stack) -> Value {
    let path = s.root.join("inst/status.json");
    assert!(
        fs::symlink_metadata(&path).unwrap().is_file(),
        "status.json must be a regular file"
    );
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

/// The legacy renderer's `data-state` (idle|preview|pinned).
pub fn dom_state(s: &Stack) -> Value {
    eval(s, "document.querySelector('[data-state]')?.dataset.state")
}

/// The ACTOR's state as the page sees it through `island_bootstrap().state`.
pub fn actor_state(s: &Stack) -> Value {
    eval(
        s,
        "window.__TAURI_INTERNALS__.invoke('island_bootstrap').then(b=>b.state)",
    )
}

/// Fixture executables (no real Plasma), the anchor-client replica, and the scratch layout.
pub fn prepare(s: &Stack) {
    fs::create_dir(s.root.join("bin")).unwrap();
    fs::create_dir_all(s.root.join("replica/island")).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../island/anchor-client.py");
    let copy = s.root.join("replica/island/anchor-client.py");
    fs::copy(&source, &copy).unwrap();
    assert_eq!(fs::read(source).unwrap(), fs::read(copy).unwrap());
    symlink(
        s.root.join("inst"),
        s.root.join("replica/.island-data-overlay"),
    )
    .unwrap();
    let outputs = json!({"outputs":[{"connected":true,"enabled":true,"priority":1,"name":"fixture","pos":{"x":0,"y":0},"size":{"width":1600,"height":1000}}]});
    let panels = json!([{"id":1,"location":"top","height":44,"x":0,"y":0,"widgets":[{"id":2,"type":"local.hermes.island.v3","x":725,"y":4,"width":150,"height":36}]}]);
    for (name, value) in [("kscreen-doctor", outputs), ("qdbus6", panels)] {
        let path = s.root.join("bin").join(name);
        fs::write(
            &path,
            format!("#!/usr/bin/python3\nprint({:?})\n", value.to_string()),
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

pub fn launch(s: &Stack) -> Owned {
    let log = fs::File::create(s.root.join("app.log")).unwrap();
    Owned(
        command(s, &s.root.join("inst"))
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    )
}

/// A PID that belonged to a process that has exited and been reaped.
pub fn dead_pid() -> u32 {
    let mut child = Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    assert!(!Path::new("/proc").join(pid.to_string()).exists());
    pid
}

/// A socket FILE nobody listens on (bind, close, leave the inode behind).
pub fn stale_socket(path: &Path) {
    drop(std::os::unix::net::UnixListener::bind(path).unwrap());
    assert_eq!(
        UnixStream::connect(path).unwrap_err().kind(),
        std::io::ErrorKind::ConnectionRefused
    );
}

/// Mirrors `eis_input_wheel.py`: pointer + keyboard + wheel into the private KWin.
pub struct Wheel {
    input: ChildStdin,
    rx: mpsc::Receiver<String>,
    child: Child,
}
impl Wheel {
    pub fn new(s: &Stack) -> Self {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/eis_input_wheel.py");
        let mut child = Command::new("/usr/bin/python3")
            .arg(script)
            .env_clear()
            .envs(s.env())
            .env("HERMES_ISLAND_SCRATCH", &s.root)
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(s.root.join("eis-wheel.log")).unwrap())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for l in BufReader::new(output).lines().map_while(Result::ok) {
                if tx.send(l).is_err() {
                    break;
                }
            }
        });
        let ready = rx.recv_timeout(Duration::from_secs(30)).unwrap();
        assert!(ready.starts_with("ready 0 0 "), "{ready}");
        Self { input, rx, child }
    }
    pub fn send(&mut self, line: &str) {
        writeln!(self.input, "{line}").unwrap();
        assert_eq!(self.rx.recv_timeout(Duration::from_secs(10)).unwrap(), "ok");
    }
    pub fn move_to(&mut self, x: i32, y: i32) {
        self.send(&format!("move {x} {y}"));
        sleep(Duration::from_millis(120));
    }
    pub fn click_at(&mut self, x: i32, y: i32) {
        self.move_to(x, y);
        self.send("click");
    }
}
impl Drop for Wheel {
    fn drop(&mut self) {
        stop_group(&mut self.child);
    }
}

/// Poll until `probe` is truthy, panicking with `what` after `secs`.
pub fn until(s: &Stack, what: &str, secs: u64, js: &str) {
    wait_for(what, Duration::from_secs(secs), || {
        (eval(s, js) == true).then_some(())
    });
}

pub fn wait_mode(
    s: &Stack,
    conn: &x11rb::rust_connection::RustConnection,
    window: u32,
    legacy: &str,
    mode: IslandMode,
) {
    let mut attempts = 0;
    wait_for(
        &format!("native bridge {legacy} DOM/status/ShapeInput"),
        Duration::from_secs(15),
        || {
            attempts += 1;
            if attempts % 30 == 0 {
                eprintln!(
                    "bridge {legacy}: dom={} status.mode={} rev={} actor={} sent={} rows={}",
                    dom_state(s),
                    status(s)["mode"],
                    status(s)["stateRev"],
                    actor_state(s),
                    eval(s, "JSON.stringify(window.__mouse||null)"),
                    input_region(conn, window).len()
                );
            }
            if dom_state(s) != legacy {
                return None;
            }
            let rect=eval(s,"(()=>{let e=document.querySelector('[data-state]'),r=e.getBoundingClientRect();return {x:Math.max(0,r.x),y:Math.max(0,r.y),width:r.width,height:r.height,radius:parseFloat(getComputedStyle(e).borderRadius)}})()");
            let rect: RoundedRect = serde_json::from_value(rect).unwrap();
            let expected = region_rows(&[rect], mode, 1., true).unwrap();
            let actual = input_region(conn, window);
            if normalize_rows(&actual) != expected {
                return None;
            }
            let file = status(s);
            if file["mode"] != legacy {
                return None;
            }
            let stored: Vec<_> = file["rectangles"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| hermes_island_lib::window::shape::ShapeRow {
                    x: r["x"].as_i64().unwrap() as i16,
                    y: r["y"].as_i64().unwrap() as i16,
                    width: r["width"].as_u64().unwrap() as u16,
                    height: r["height"].as_u64().unwrap() as u16,
                })
                .collect();
            (normalize_rows(&stored) == normalize_rows(&actual)).then_some(())
        },
    );
}
