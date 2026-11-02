//! Phase 4 acceptance: real CLI -> socket -> store -> actor -> WebKit toast, private KWin only.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "helpers/gui_stack.rs"]
#[allow(dead_code)]
mod gui_stack;
#[path = "helpers/phase3_host.rs"]
mod host;
use common::*;
use gui_stack::Stack;
use hermes_island_lib::contract::island::IslandMode;
use host::*;
use serde_json::Value;
use std::{fs, path::Path, process::Command, thread::sleep, time::Duration};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, CreateWindowAux, EventMask, WindowClass};
use x11rb::wrapper::ConnectionExt as _;
fn send(s: &Stack, title: &str) {
    let result = Command::new(env!("CARGO_BIN_EXE_hermes-island"))
        .args(["notify", "--test-instance", "--data-dir"])
        .arg(s.root.join("inst"))
        .args([
            "--title",
            title,
            "--body",
            "A private fixture, not a desktop notification.",
            "--ttl-ms",
            "0",
        ])
        .env_clear()
        .envs(s.env())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap()["ok"],
        true
    );
}
fn screenshot(s: &Stack, conn: &x11rb::rust_connection::RustConnection, window: u32) {
    let evidence = Evidence::new("phase4");
    Shot::grab(conn, window, WIN_W, WIN_H).save_png(&evidence.path("toast-still-argb.png"));
    let raw = s.root.join("capture.raw");
    let result = Command::new("/usr/bin/python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/kwin_capture.py"))
        .arg(&raw)
        .env_clear()
        .envs(s.env())
        .env("HERMES_ISLAND_SCRATCH", &s.root)
        .output()
        .unwrap();
    assert!(result.status.success());
    let m: Value = serde_json::from_slice(&result.stdout).unwrap();
    let (w, h, stride) = (
        m["width"].as_u64().unwrap() as u32,
        m["height"].as_u64().unwrap() as u32,
        m["stride"].as_u64().unwrap() as usize,
    );
    let order = match m["format"].as_u64().unwrap() {
        4..=6 => [2, 1, 0],
        16..=18 => [0, 1, 2],
        v => panic!("format {v}"),
    };
    let raw = fs::read(raw).unwrap();
    let mut rgba = Vec::new();
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * stride + x * 4;
            rgba.extend_from_slice(&[raw[i + order[0]], raw[i + order[1]], raw[i + order[2]], 255]);
        }
    }
    image::RgbaImage::from_raw(w, h, rgba)
        .unwrap()
        .save(evidence.path("toast-still-KWIN-COMPOSITED.png"))
        .unwrap();
}
#[test]
fn cli_notify_toast_ttl_reading_and_dnd_on_private_kwin() {
    let _guard = lock();
    let mut s = Stack::new("phase4-toast");
    prepare(&s);
    let conn = s.connect();
    let root = conn.setup().roots[0].root;
    // Without another X surface, XWayland keeps a stale pointer at (800,500) when EIS
    // moves onto the empty Wayland desktop. A real X underlay makes outside crossings
    // observable and prevents a shape change from entering at that stale coordinate.
    let underlay = conn.generate_id().unwrap();
    conn.create_window(
        0,
        underlay,
        root,
        0,
        0,
        1600,
        1000,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new()
            .override_redirect(0)
            .background_pixel(0x222222)
            .event_mask(
                EventMask::ENTER_WINDOW | EventMask::LEAVE_WINDOW | EventMask::POINTER_MOTION,
            ),
    )
    .unwrap()
    .check()
    .unwrap();
    conn.change_property32(
        x11rb::protocol::xproto::PropMode::REPLACE,
        underlay,
        atom(&conn, "_NET_WM_WINDOW_TYPE"),
        x11rb::protocol::xproto::AtomEnum::ATOM,
        &[atom(&conn, "_NET_WM_WINDOW_TYPE_DESKTOP")],
    )
    .unwrap()
    .check()
    .unwrap();
    conn.map_window(underlay).unwrap().check().unwrap();
    conn.flush().unwrap();
    let mut app = launch(&s);
    let pid = app.0.id();
    let window = wait_for("phase4 window", Duration::from_secs(30), || {
        find_island_window(&conn, root, pid)
    });
    wait_for("test-control", Duration::from_secs(20), || {
        s.root.join("inst/test-control.sock").exists().then_some(())
    });
    wait_for("legacy page", Duration::from_secs(20), || {
        (eval(&s, "!!document.querySelector('[data-state]')") == true).then_some(())
    });
    let mut eis = Wheel::new(&s);
    eis.move_to(100, 900);
    sleep(Duration::from_millis(1600));
    assert_eq!(actor_state(&s)["mode"], "still");
    eval(&s, "window.island.setSettings({toastTtlMs:1500})");
    eval(&s,"(()=>{window.__mouse=[];for(const t of ['mouseover','mouseout','mouseenter','mouseleave'])document.addEventListener(t,e=>window.__mouse.push(t+'@'+e.clientX+','+e.clientY+'/'+(e.target.className||e.target.tagName)),true);return true})()");
    send(&s, "Auto-dismiss fixture");
    wait_mode(&s, &conn, window, "preview", IslandMode::Hover);
    wait_for("visible real toast", Duration::from_secs(5), || {
        (eval(&s,"document.querySelector('[data-testid=notification-toast]')?.textContent.includes('Auto-dismiss fixture')")==true).then_some(())
    });
    assert_not_focused_on_show(&conn, root, window);
    wait_mode(&s, &conn, window, "idle", IslandMode::Still);
    assert!(input_region(&conn, window).is_empty());
    eval(&s, "window.island.setSettings({toastTtlMs:6000})");
    send(&s, "Build finished");
    wait_mode(&s, &conn, window, "preview", IslandMode::Hover);
    sleep(Duration::from_millis(1700));
    screenshot(&s, &conn, window);
    let pos=eval(&s,"(()=>{const r=document.querySelector('[data-testid=notification-toast]').getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]})()");
    eis.move_to(
        210 + pos[0].as_f64().unwrap() as i32,
        44 + pos[1].as_f64().unwrap() as i32,
    );
    wait_for("pointer reads toast", Duration::from_secs(5), || {
        (actor_state(&s)["hoverCause"]["type"] == "pointer").then_some(())
    });
    sleep(Duration::from_millis(6200));
    assert_eq!(
        actor_state(&s)["mode"],
        "hover",
        "PointerEnter did not cancel toast TTL"
    );
    eis.move_to(100, 900);
    wait_mode(&s, &conn, window, "idle", IslandMode::Still);
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('island_dispatch',{intent:{type:'setDnd',on:true}})",
    );
    send(&s, "Stored silently");
    sleep(Duration::from_millis(1800));
    assert_eq!(actor_state(&s)["mode"], "still");
    assert!(input_region(&conn, window).is_empty());
    let list = eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('notifications_list')",
    );
    assert!(list
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["title"] == "Stored silently"));
    assert!(list
        .as_array()
        .unwrap()
        .iter()
        .all(|n| n["canInvokeActions"] == false));
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
    wait_for("phase4 app exits", Duration::from_secs(15), || {
        app.0.try_wait().unwrap().map(|_| ())
    });
    assert!(!s.root.join("inst/ingest.sock").exists());
    drop(eis);
    drop(app);
    s.shutdown();
    let path = s.root.clone();
    drop(s);
    assert!(!path.exists());
}
