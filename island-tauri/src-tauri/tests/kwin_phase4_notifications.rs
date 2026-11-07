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
use host::*;
use serde_json::{json, Value};
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
    Shot::grab(conn, window, overlay_size().0, overlay_size().1)
        .save_png(&evidence.path("toast-still-argb.png"));
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
    let mut app = launch_ui(&s, "island");
    let pid = app.0.id();
    let window = wait_for("phase4 window", Duration::from_secs(30), || {
        find_window_sized(&conn, root, pid, overlay_size())
    });
    wait_for("test-control", Duration::from_secs(20), || {
        s.root.join("inst/test-control.sock").exists().then_some(())
    });
    // Notifications are popup cards drawn INSIDE the notch (NotchNotifications): a card grows
    // the collapsed island into a notification hover, and the popup queue's lifetime closes it.
    wait_for("island page", Duration::from_secs(20), || {
        (eval(&s, "!!document.querySelector('.island')") == true).then_some(())
    });
    let origin = {
        let p = conn
            .translate_coordinates(window, root, 0, 0)
            .unwrap()
            .reply()
            .unwrap();
        (i32::from(p.dst_x), i32::from(p.dst_y))
    };
    let card = "document.querySelector('[data-testid=notification-toast]')";
    let card_text = |needle: &str| {
        eval(
            &s,
            &format!("{card}?.textContent.includes({})===true", json!(needle)),
        ) == true
    };
    let covers_card = || {
        let r = eval(
            &s,
            &format!("(()=>{{const r={card}.getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]}})()"),
        );
        let (x, y) = (r[0].as_f64().unwrap() as i32, r[1].as_f64().unwrap() as i32);
        input_region(&conn, window).iter().any(|row| {
            (row.y as i32..row.y as i32 + row.height as i32).contains(&y)
                && (row.x as i32..row.x as i32 + row.width as i32).contains(&x)
        })
    };
    let mut eis = Wheel::new(&s);
    eis.move_to(100, 900);
    sleep(Duration::from_millis(1600));
    assert_eq!(actor_state(&s)["mode"], "still");
    // The collapsed island's own input region (native notch) is the baseline: cards add to it
    // while visible and must leave exactly this behind.
    let idle = input_region(&conn, window);
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{toastTtlMs:1500}})",
    );
    send(&s, "Auto-dismiss fixture");
    wait_for("visible real card", Duration::from_secs(8), || {
        (card_text("Auto-dismiss fixture") && covers_card()).then_some(())
    });
    // The notch grew into the card (no separate window), without taking focus.
    assert_eq!(actor_state(&s)["mode"], "hover");
    assert_eq!(actor_state(&s)["hoverCause"]["type"], "notification");
    assert_eq!(
        eval(&s, &format!("!!{card}.closest('.island')&&!{card}.hasAttribute('data-island-extension')")),
        true,
        "the card must be part of the island surface"
    );
    assert_not_focused_on_show(&conn, root, window);
    // Measured alignment (rendered layout, not CSS intent): the close X is centred in its round
    // target, the target sits on the header row's centre line, and its visible glyph is as far
    // from the card's right edge as the app icon is from the left edge.
    // Measure only once the reveal animation has settled (scaled rects skew every offset).
    wait_for("card animation settles", Duration::from_secs(3), || {
        (eval(&s, &format!("Math.abs({card}.querySelector('.notification-close').getBoundingClientRect().width-26)<0.2")) == true)
            .then_some(())
    });
    let geo = eval(
        &s,
        &format!(
            "(()=>{{const c={card},b=c.querySelector('.notification-close'),g=b.querySelector('svg'),\
             i=c.querySelector('.notification-icon'),a=c.querySelector('.notification-app'),\
             R=e=>e.getBoundingClientRect(),cr=R(c),br=R(b),gr=R(g),ir=R(i),ar=R(a),\
             mid=r=>r.y+r.height/2,cx=r=>r.x+r.width/2;\
             return [br.width,br.height,cx(gr)-cx(br),mid(gr)-mid(br),mid(br)-mid(ir),mid(ar)-mid(ir),\
             (cr.right-gr.right)-(ir.x-cr.x)]}})()"
        ),
    );
    let g: Vec<f64> = geo.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!((g[0] - g[1]).abs() < 0.5, "close target is round: {g:?}");
    assert!(g[2].abs() <= 0.5 && g[3].abs() <= 0.5, "X centred in its target: {g:?}");
    assert!(g[4].abs() <= 1.0 && g[5].abs() <= 1.0, "header row shares one centre line: {g:?}");
    assert!(g[6].abs() <= 1.0, "X inset mirrors the icon inset: {g:?}");
    wait_for(
        "card auto-dismisses after its TTL",
        Duration::from_secs(8),
        || (eval(&s, &format!("!{card}")) == true).then_some(()),
    );
    wait_for("card input region released", Duration::from_secs(5), || {
        (input_region(&conn, window) == idle).then_some(())
    });
    wait_for("notch closes with the queue", Duration::from_secs(3), || {
        (actor_state(&s)["mode"] == "still").then_some(())
    });
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{toastTtlMs:6000}})",
    );
    send(&s, "Build finished");
    wait_for("second card", Duration::from_secs(8), || {
        (card_text("Build finished") && covers_card()).then_some(())
    });
    sleep(Duration::from_millis(1700));
    screenshot(&s, &conn, window);
    let pos = eval(&s, &format!("(()=>{{const r={card}.getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]}})()"));
    eis.move_to(
        origin.0 + pos[0].as_f64().unwrap() as i32,
        origin.1 + pos[1].as_f64().unwrap() as i32,
    );
    wait_for("pointer reads card", Duration::from_secs(5), || {
        (eval(&s, "window.__TAURI_INTERNALS__.invoke('notification_popup',{op:{type:'list'}}).then(p=>p.items[0]?.paused===true)") == true).then_some(())
    });
    sleep(Duration::from_millis(6200));
    assert!(
        card_text("Build finished"),
        "hovering the card did not pause its TTL"
    );
    eis.move_to(100, 900);
    wait_for(
        "card expires after the pointer leaves",
        Duration::from_secs(10),
        || (eval(&s, &format!("!{card}")) == true).then_some(()),
    );
    wait_for(
        "card input region released again",
        Duration::from_secs(5),
        || (input_region(&conn, window) == idle).then_some(()),
    );
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('island_dispatch',{intent:{type:'setDnd',on:true}})",
    );
    send(&s, "Stored silently");
    sleep(Duration::from_millis(1800));
    assert_eq!(actor_state(&s)["mode"], "still");
    assert_eq!(
        eval(&s, &format!("!{card}")),
        true,
        "DND must not draw a card"
    );
    assert_eq!(input_region(&conn, window), idle);
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
    // Settings > Notifications sound picker, rendered: dropdown and play button share one row
    // (same vertical centre, button to the right), in the main row and in a per-app rule.
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{dnd:false,notificationAppRules:[{app:'Fixture',popup:true,sound:true,urgency:'app',soundId:''}]}})",
    );
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('island_dispatch',{intent:{type:'selectView',view:'settings'}})",
    );
    wait_for("settings sound row", Duration::from_secs(8), || {
        (eval(&s, "(()=>{const b=[...document.querySelectorAll('.settings-row .link[aria-expanded=false]')].find(b=>b.closest('[data-setting-label=\"Per-app rules\"]'));b?.click();return !!document.querySelector('.settings-row > .sound-picker')&&!!document.querySelector('.app-rule .sound-picker')})()") == true)
            .then_some(())
    });
    std::thread::sleep(Duration::from_millis(600));
    let pickers = eval(
        &s,
        "[...document.querySelectorAll('.sound-picker')].map(p=>{const R=e=>e.getBoundingClientRect(),d=R(p.querySelector('.dropdown-trigger')),b=R(p.querySelector('.sound-preview')),row=R(p.parentElement),l=p.parentElement.querySelector(':scope > .label, :scope > .app-rule-name');\
         return [Math.abs((d.y+d.height/2)-(b.y+b.height/2)),b.x-d.right,Math.abs((row.y+row.height/2)-(d.y+d.height/2)),b.right<=row.right+0.5?1:0,l?R(l).right-d.x:-1,b.width-b.height]})",
    );
    let pickers = pickers.as_array().unwrap();
    assert_eq!(pickers.len(), 2, "global and per-app pickers: {pickers:?}");
    for p in pickers {
        let g: Vec<f64> = p.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!(g[0] <= 1.0, "play button level with the dropdown: {g:?}");
        assert!((2.0..=12.0).contains(&g[1]), "play button just right of the dropdown: {g:?}");
        assert!(g[2] <= 2.0, "picker centred in its row: {g:?}");
        assert_eq!(g[3], 1.0, "play button inside the row: {g:?}");
        assert!(g[4] <= 0.0, "label and dropdown don't overlap: {g:?}");
        assert!(g[5].abs() < 0.5, "play button is round: {g:?}");
    }
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
