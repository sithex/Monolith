//! Phase 3 native host acceptance on a private nested KWin: placement, identity, the real
//! `anchor-client.py` pin through the state ACTOR, coexistence/refusal and stale-artifact
//! reclaim as real processes, and SIGTERM cleanup. Hover/tab/focus authority is in
//! `kwin_phase3_actor.rs`.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "helpers/gui_stack.rs"]
#[allow(dead_code)] // Shared Phase 2 stack also exposes controls unused by this scenario.
mod gui_stack;
#[path = "helpers/phase3_host.rs"]
mod host;
use common::*;
use gui_stack::Stack;
use hermes_island_lib::contract::island::IslandMode;
use host::*;
use serde_json::json;
use std::{
    fs,
    os::unix::{
        fs::{symlink, MetadataExt, PermissionsExt},
        net::UnixListener,
    },
    path::Path,
    process::Stdio,
    thread::sleep,
    time::Duration,
};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as _;

/// Content + inode of every non-lock artifact, to prove a refusal touched nothing.
fn artifacts(dir: &Path) -> Vec<(String, u64, Vec<u8>)> {
    let mut rows: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| !e.file_name().to_string_lossy().ends_with(".lock"))
        .map(|e| {
            let meta = fs::symlink_metadata(e.path()).unwrap();
            let bytes = if meta.is_file() {
                fs::read(e.path()).unwrap()
            } else {
                Vec::new()
            };
            (
                e.file_name().to_string_lossy().into_owned(),
                meta.ino(),
                bytes,
            )
        })
        .collect();
    rows.sort();
    rows
}

fn private_dir(s: &Stack, name: &str) -> std::path::PathBuf {
    let dir = s.root.join(name);
    fs::create_dir(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

/// Run the REAL binary against `dir` and require a refusal that leaves `dir` untouched.
fn real_process_refuses(s: &Stack, dir: &Path, reason: &str) {
    let before = artifacts(dir);
    let result = command(s, dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success(), "{reason}: takeover accepted");
    assert!(
        stderr.contains("refusing takeover"),
        "{reason}: wrong refusal: {stderr}"
    );
    assert_eq!(
        artifacts(dir),
        before,
        "{reason}: a refusal must leave every artifact untouched"
    );
}

#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn native_anchor_client_pin_coexistence_and_sigterm_cleanup() {
    let _guard = lock();
    let mut s = Stack::new("phase3-host");
    prepare(&s);
    // A crashed previous run: status of a dead PID plus a socket file nobody listens on. The
    // REAL binary must reclaim exactly this pair (both locks held) and become the owner.
    let planted_status = json!({"pid":dead_pid(),"mode":"pinned","stale":true});
    fs::write(s.root.join("inst/status.json"), planted_status.to_string()).unwrap();
    fs::set_permissions(
        s.root.join("inst/status.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    stale_socket(&s.root.join("inst/panel.sock"));
    let stale_inode = fs::symlink_metadata(s.root.join("inst/panel.sock"))
        .unwrap()
        .ino();
    let mut app = launch(&s);
    let conn = s.connect();
    let root = conn.setup().roots[0].root;
    let pid = app.0.id();
    let window = wait_for("Phase3 host window", Duration::from_secs(30), || {
        find_island_window(&conn, root, pid)
    });
    assert_identity(&conn, window, pid, true);
    assert_not_focused_on_show(&conn, root, window);
    let xy = conn
        .translate_coordinates(window, root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(
        (xy.dst_x, xy.dst_y),
        (210, 44),
        "placement stays BELOW panel"
    );
    wait_for(
        "private native host sockets and status",
        Duration::from_secs(20),
        || {
            (s.root.join("inst/test-control.sock").exists()
                && s.root.join("inst/status.json").exists())
            .then_some(())
        },
    );
    // Stale reclaim happened for real: the status is OURS now (regular file, our PID), and the
    // socket is a different inode that accepts connections.
    assert_eq!(status(&s)["pid"], pid);
    assert_ne!(status(&s)["stale"], true);
    let socket_meta = fs::symlink_metadata(s.root.join("inst/panel.sock")).unwrap();
    assert_ne!(
        socket_meta.ino(),
        stale_inode,
        "stale socket inode must be gone"
    );
    assert_eq!(socket_meta.permissions().mode() & 0o777, 0o600);
    assert!(std::os::unix::net::UnixStream::connect(s.root.join("inst/panel.sock")).is_ok());
    let meta = fs::symlink_metadata(s.root.join("inst/status.json")).unwrap();
    assert!(meta.is_file() && meta.permissions().mode() & 0o777 == 0o600);
    wait_for("legacy shim ready", Duration::from_secs(20), || {
        (eval(
            &s,
            "!!window.island && !!document.querySelector('[data-state]')",
        ) == true)
            .then_some(())
    });
    assert_eq!(eval(&s, "window.island.panelAnchor"), true);
    assert!(input_region(&conn, window).is_empty());
    assert_eq!(client(&s, "status")["anchor"]["native"], true);
    sleep(Duration::from_millis(1600));
    let mut eis = s.eis();
    eis.send("move 800 22");
    // The unchanged `anchor-client.py` pin drives the ACTOR: pinned DOM, status.json mode,
    // actor bootstrap state and ShapeInput all agree.
    client(&s, "pin");
    wait_mode(&s, &conn, window, "pinned", IslandMode::Large);
    assert_eq!(actor_state(&s)["mode"], "large");
    assert_eq!(actor_state(&s)["terminalStarted"], true);
    wait_for("fixture PTY starts", Duration::from_secs(10), || {
        (status(&s)["pty"].as_u64().unwrap() > 0).then_some(())
    });
    let pty = status(&s)["pty"].as_u64().unwrap();
    client(&s, "pin");
    wait_mode(&s, &conn, window, "idle", IslandMode::Still);
    assert_eq!(actor_state(&s)["mode"], "still");
    assert_eq!(status(&s)["pty"], pty);
    let socket_inode = fs::metadata(s.root.join("inst/panel.sock")).unwrap().ino();
    let result = command(&s, &s.root.join("inst"))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(!result.status.success(), "second instance accepted");
    assert!(String::from_utf8_lossy(&result.stderr).contains("singleton lock"));
    assert_eq!(
        fs::metadata(s.root.join("inst/panel.sock")).unwrap().ino(),
        socket_inode
    );
    assert_eq!(client(&s, "status")["pid"], pid);

    // ---- every refusal case as a REAL process (they refuse before any GTK start) ----------
    // Independent fixture legacy status and socket, NEVER the live overlay artifacts.
    let write_status = |dir: &Path, pid: u32| {
        fs::write(
            dir.join("status.json"),
            json!({"pid":pid,"legacy":true}).to_string(),
        )
        .unwrap();
        fs::set_permissions(dir.join("status.json"), fs::Permissions::from_mode(0o600)).unwrap();
    };
    // (1) live legacy status, no socket (the original rollback case)
    let dir = private_dir(&s, "legacy-status");
    write_status(&dir, std::process::id());
    real_process_refuses(&s, &dir, "live legacy status without socket");
    // (2) a live legacy socket without a status
    let dir = private_dir(&s, "legacy-socket");
    let _live_socket = UnixListener::bind(dir.join("panel.sock")).unwrap();
    real_process_refuses(&s, &dir, "live legacy socket without status");
    // (3) live status PID + stale socket: the owner is alive
    let dir = private_dir(&s, "live-status-stale-socket");
    write_status(&dir, std::process::id());
    stale_socket(&dir.join("panel.sock"));
    real_process_refuses(&s, &dir, "live status owner");
    // (4) dead status PID + LIVE socket (accepts connections)
    let dir = private_dir(&s, "dead-status-live-socket");
    write_status(&dir, dead_pid());
    let _live_listener = UnixListener::bind(dir.join("panel.sock")).unwrap();
    real_process_refuses(&s, &dir, "live socket");
    // (5) dead status + a regular file squatting on the socket name
    let dir = private_dir(&s, "dead-status-file-socket");
    write_status(&dir, dead_pid());
    fs::write(dir.join("panel.sock"), "not a socket").unwrap();
    real_process_refuses(&s, &dir, "non-socket");
    // (6) dead status + a symlink to a stale socket
    let dir = private_dir(&s, "dead-status-symlink-socket");
    write_status(&dir, dead_pid());
    stale_socket(&dir.join("real.sock"));
    symlink("real.sock", dir.join("panel.sock")).unwrap();
    real_process_refuses(&s, &dir, "symlinked socket");
    // (7) dead status and no socket at all: unverifiable
    let dir = private_dir(&s, "dead-status-no-socket");
    write_status(&dir, dead_pid());
    real_process_refuses(&s, &dir, "status without socket");
    // (8) unparsable status + stale socket: unverifiable owner
    let dir = private_dir(&s, "garbage-status");
    fs::write(dir.join("status.json"), "{not json").unwrap();
    stale_socket(&dir.join("panel.sock"));
    real_process_refuses(&s, &dir, "unverifiable status");

    // ---- SIGTERM runs the real cleanup ----------------------------------------------------
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
    wait_for(
        "SIGTERM app + fixture PTY exits",
        Duration::from_secs(15),
        || {
            (app.0.try_wait().unwrap().is_some() && !Path::new(&format!("/proc/{pty}")).exists())
                .then_some(())
        },
    );
    assert!(!s.root.join("inst/panel.sock").exists());
    assert!(fs::symlink_metadata(s.root.join("inst/status.json")).is_err());
    assert!(!s.root.join("inst/test-control.sock").exists());
    let leftovers: Vec<_> = fs::read_dir(s.root.join("inst"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp") || n.starts_with(".ripple-status"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "status temp files leaked: {leftovers:?}"
    );
    drop(app);
    s.shutdown();
    let scratch = s.root.clone();
    drop(s);
    assert!(!scratch.exists());
}
