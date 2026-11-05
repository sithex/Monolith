//! Phase 2 real legacy renderer, private nested KWin, production XInput2 + EIS.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "helpers/gui_stack.rs"]
mod gui_stack;
use common::*;
use gui_stack::{Eis, Stack};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::Path,
    process::Command,
    thread::sleep,
    time::Duration,
};
use x11rb::{
    connection::Connection,
    protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask},
    rust_connection::RustConnection,
};

fn request(s: &Stack, value: Value) -> Value {
    let mut stream = UnixStream::connect(s.root.join("inst/test-control.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(65)))
        .unwrap();
    writeln!(stream, "{value}").unwrap();
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).unwrap();
    let reply: Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply["ok"], true, "request {value}: {reply}");
    reply
}
fn eval(s: &Stack, js: &str) -> Value {
    request(s, json!({"op":"eval","js":js}))["value"].clone()
}
fn until(s: &Stack, name: &str, js: &str) {
    wait_for(name, Duration::from_secs(45), || {
        (eval(s, js) == true).then_some(())
    });
}
fn click(s: &Stack, eis: &mut Eis, origin: (i16, i16), selector: &str) {
    eval(
        s,
        &format!(
            "document.querySelector({}).scrollIntoView({{block:'nearest'}})",
            json!(selector)
        ),
    );
    sleep(Duration::from_millis(200));
    let expr=format!("(()=>{{const e=document.querySelector({});if(!e)throw Error('missing click target');const r=e.getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]}})()",json!(selector));
    let pos = eval(s, &expr);
    eis.click(
        origin.0 + pos[0].as_f64().unwrap() as i16,
        origin.1 + pos[1].as_f64().unwrap() as i16,
    );
    sleep(Duration::from_millis(1600));
}
fn screenshot(s: &Stack, conn: &RustConnection, window: u32, name: &str) {
    // Controller-authorized content settle: do not capture an async pane update
    // halfway through painting. This is not a retry of the baseline comparison.
    let mut previous = None;
    wait_for(
        "view text and ARGB content stable",
        Duration::from_secs(5),
        || {
            let text = eval(s, "document.querySelector('.view-pane').innerText");
            let pixels = Shot::grab(conn, window, WIN_W, WIN_H).bgra;
            let current = (text, pixels);
            let stable = previous.as_ref() == Some(&current);
            previous = Some(current);
            if stable {
                Some(())
            } else {
                sleep(Duration::from_millis(250));
                None
            }
        },
    );
    Shot::grab(conn, window, WIN_W, WIN_H)
        .save_png(&Evidence::new("phase2").path(&format!("{name}-argb.png")));
    let raw = s.root.join("capture.raw");
    let output = Command::new("/usr/bin/python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/kwin_capture.py"))
        .arg(&raw)
        .env_clear()
        .envs(s.env())
        .env("HERMES_ISLAND_SCRATCH", &s.root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "capture: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let meta: Value = serde_json::from_slice(&output.stdout).unwrap();
    let width = meta["width"].as_u64().unwrap() as u32;
    let height = meta["height"].as_u64().unwrap() as u32;
    let stride = meta["stride"].as_u64().unwrap() as usize;
    let order = match meta["format"].as_u64().unwrap() {
        4..=6 => [2, 1, 0],
        16..=18 => [0, 1, 2],
        v => panic!("capture format {v}"),
    };
    let data = fs::read(raw).unwrap();
    let mut rgba = Vec::new();
    for y in 0..height as usize {
        for x in 0..width as usize {
            let i = y * stride + x * 4;
            rgba.extend_from_slice(&[
                data[i + order[0]],
                data[i + order[1]],
                data[i + order[2]],
                255,
            ]);
        }
    }
    image::RgbaImage::from_raw(width, height, rgba)
        .unwrap()
        .save(Evidence::new("phase2").path(&format!("{name}-KWIN-COMPOSITED.png")))
        .unwrap();
}
fn scenario(overlay: bool) {
    let _guard = lock();
    let tag = if overlay { "overlay" } else { "preview" };
    let mut s = Stack::new(tag);
    let conn = s.connect();
    let root = conn.setup().roots[0].root;
    let app = s.start_app(overlay);
    let window = wait_for("legacy island mapped", Duration::from_secs(60), || {
        find_island_window(&conn, root, app)
    });
    assert_identity(&conn, window, app, overlay);
    assert_not_focused_on_show(&conn, root, window);
    let translated = conn
        .translate_coordinates(window, root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    let origin = (translated.dst_x, translated.dst_y);
    wait_for("test socket", Duration::from_secs(20), || {
        s.root.join("inst/test-control.sock").exists().then_some(())
    });
    until(
        &s,
        "legacy rendered",
        "!!document.querySelector('[data-state]') && !!window.island",
    );
    let environ = fs::read(format!("/proc/{app}/environ")).unwrap();
    assert!(!environ
        .split(|b| *b == 0)
        .any(|p| p.starts_with(b"GDK_CORE_DEVICE_EVENTS=")));
    let mut eis = s.eis();
    sleep(Duration::from_millis(1600));
    click(&s, &mut eis, origin, "[data-testid=clock]");
    until(&s,"pinned terminal","document.querySelector('[data-state]')?.dataset.state==='pinned' && !!document.querySelector('.xterm')");
    until(
        &s,
        "real xterm fixture prompt",
        "window.__islandTerminalTest?.text()?.includes('island-test>')===true",
    );
    let pty = request(&s, json!({"op":"terminal"}))["pid"]
        .as_u64()
        .unwrap();
    // Real command round trip, ONLY to the guarded bash fixture. Split sentinel prevents echo from satisfying output assertion.
    eval(
        &s,
        "window.island.input(\"printf 'ROUND_%s\\n' TRIP_OK\\r\")",
    );
    until(
        &s,
        "actual xterm PTY output",
        "window.__islandTerminalTest.text().includes('ROUND_TRIP_OK')",
    );
    click(&s, &mut eis, origin, ".terminal");
    eis.send("type UNSUBMITTED_DRAFT");
    until(
        &s,
        "EIS draft visible",
        "window.__islandTerminalTest.text().includes('UNSUBMITTED_DRAFT')",
    );
    click(&s, &mut eis, origin, "[data-testid=collapse]");
    until(
        &s,
        "minimized",
        "document.querySelector('[data-state]').dataset.state==='idle'",
    );
    assert_eq!(request(&s, json!({"op":"terminal"}))["pid"], pty);
    click(&s, &mut eis, origin, "[data-testid=clock]");
    until(&s,"reopened draft","document.querySelector('[data-state]').dataset.state==='pinned' && window.__islandTerminalTest.text().includes('UNSUBMITTED_DRAFT')");
    assert_eq!(request(&s, json!({"op":"terminal"}))["pid"], pty);
    click(&s, &mut eis, origin, ".terminal");
    eis.send("chord 29 22"); // Ctrl+U, never submit the draft
    screenshot(&s, &conn, window, &format!("{tag}-terminal"));
    for (view, labels) in [
        ("monitor", ["CPU", "Memory"]),
        ("ai", ["AI stats", "Connect a provider"]),
        ("settings", ["GENERAL", "APPEARANCE"]),
    ] {
        click(&s, &mut eis, origin, &format!("[data-testid=view-{view}]"));
        until(&s,&format!("{view} pane"),&format!("document.querySelector('[data-state]').dataset.view==={} && Number(getComputedStyle(document.querySelector('.view-pane')).opacity)===1",json!(view)));
        until(&s, "view spring settled", "Math.abs(new DOMMatrix(getComputedStyle(document.querySelector('.view-pane')).transform).m41)<0.1");
        // Actual semantic content, not just the selected tab or a placeholder shell.
        let text = eval(&s, "document.querySelector('.view-pane').innerText");
        for label in labels {
            assert!(
                text.as_str().unwrap().contains(label),
                "missing {label} in {view}: {text}"
            );
        }
        screenshot(&s, &conn, window, &format!("{tag}-{view}"));
    }
    until(&s,"OpenRunde loaded","document.documentElement.dataset.font==='openrunde' && [...document.fonts].some(f=>f.family.includes('OpenRunde')&&f.status==='loaded') && getComputedStyle(document.querySelector('.clock')).fontFamily.includes('OpenRunde')");
    let before = fs::read(s.root.join("inst/settings.json")).unwrap();
    assert_eq!(
        eval(
            &s,
            "window.island.setSettings({accent:'red',stealth:true}).then(()=>false,()=>true)"
        ),
        true
    );
    assert_eq!(
        fs::read(s.root.join("inst/settings.json")).unwrap(),
        before,
        "invalid patch wrote settings"
    );
    click(&s, &mut eis, origin, "[aria-label='Show date under time']");
    let persisted: Value =
        serde_json::from_slice(&fs::read(s.root.join("inst/settings.json")).unwrap()).unwrap();
    assert_eq!(persisted["showDate"], false);
    assert_eq!(
        eval(&s, "window.island.settings().then(s=>s.showDate)"),
        false
    );
    if overlay {
        assert_eq!(eval(&s,"document.body.getBoundingClientRect().height===innerHeight && getComputedStyle(document.body).backgroundColor==='rgba(0, 0, 0, 0)'"),true);
        let shot = Shot::grab(&conn, window, WIN_W, WIN_H);
        assert_eq!(shot.px(5, 800)[3], 0, "overlay corner is opaque");
    }
    click(&s, &mut eis, origin, "[data-testid=view-terminal]");
    until(&s,"terminal reattached on view return","!!document.querySelector('.xterm') && window.__islandTerminalTest.text().includes('ROUND_TRIP_OK')");
    assert_eq!(request(&s, json!({"op":"terminal"}))["pid"], pty);
    // The paused-ack interval proves the Rust read barrier, then parser callbacks restore progress.
    eval(&s, "window.__islandTerminalTest.setAckPaused(true)");
    let baseline = request(&s, json!({"op":"terminal"}));
    eval(&s,"window.island.input(\"python3 -c \\\"import sys; sys.stdout.write('F'*8388608+'FLOOD_'+ 'SENTINEL\\\\n');sys.stdout.flush()\\\"\\r\")");
    wait_for(
        "PTY pauses at credit boundary",
        Duration::from_secs(20),
        || {
            let m = request(&s, json!({"op":"terminal"}));
            (m["pausedReads"].as_u64().unwrap() > baseline["pausedReads"].as_u64().unwrap())
                .then_some(())
        },
    );
    let stalled = request(&s, json!({"op":"terminal"}));
    sleep(Duration::from_millis(500));
    let still = request(&s, json!({"op":"terminal"}));
    assert_eq!(
        stalled["readBytes"], still["readBytes"],
        "PTY never stopped reading"
    );
    assert!(
        still["peakUnackedBytes"].as_u64().unwrap() <= 1024 * 1024 + 64 * 1024,
        "unbounded credit: {still}"
    );
    eval(&s, "window.__islandTerminalTest.setAckPaused(false)");
    let mut seen = false;
    for attempt in 0..300 {
        let tail = eval(&s, "window.__islandTerminalTest.text().slice(-1000)");
        if tail
            .as_str()
            .unwrap()
            .replace('\n', "")
            .contains("FLOOD_SENTINEL")
        {
            seen = true;
            break;
        }
        if attempt % 50 == 49 {
            println!(
                "flood waiting: {} tail={tail}",
                request(&s, json!({"op":"terminal"}))
            );
        }
        sleep(Duration::from_millis(100));
    }
    assert!(seen, "8 MiB parsed flood sentinel missing");
    let metrics = request(&s, json!({"op":"terminal"}));
    assert!(
        metrics["readBytes"].as_u64().unwrap() - baseline["readBytes"].as_u64().unwrap()
            >= 8 * 1024 * 1024
    );
    assert!(metrics["resumedReads"].as_u64().unwrap() > baseline["resumedReads"].as_u64().unwrap());
    assert!(
        metrics["peakUnackedBytes"].as_u64().unwrap() <= 1024 * 1024 + 64 * 1024,
        "{metrics}"
    );
    println!("{tag} flood metrics: {metrics}");
    // Real webview reload must replace the output channel, replay and retain the same PTY.
    eval(&s, "(setTimeout(()=>location.reload(),1000),true)");
    sleep(Duration::from_millis(4000));
    until(
        &s,
        "reload shim ready",
        "!!window.island && !!document.querySelector('[data-state]')",
    );
    click(&s, &mut eis, origin, "[data-testid=clock]");
    until(
        &s,
        "reload replay parsed",
        "window.__islandTerminalTest?.text()?.replaceAll('\\n','').includes('FLOOD_SENTINEL')===true",
    );
    assert_eq!(request(&s, json!({"op":"terminal"}))["pid"], pty);
    // WM_DELETE only our test window; services.shutdown must reap the owned fixture PTY.
    let delete = atom(&conn, "WM_DELETE_WINDOW");
    let protocols = atom(&conn, "WM_PROTOCOLS");
    conn.send_event(
        false,
        window,
        EventMask::NO_EVENT,
        ClientMessageEvent::new(32, window, protocols, [delete, 0, 0, 0, 0]),
    )
    .unwrap();
    conn.flush().unwrap();
    wait_for("test app and PTY exited", Duration::from_secs(10), || {
        (s.exited(app) && !Path::new(&format!("/proc/{pty}")).exists()).then_some(())
    });
    s.shutdown();
    let scratch = s.root.clone();
    drop(s);
    assert!(!scratch.exists(), "GUI scratch leaked");
}
#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn kwin_gui_preview() {
    scenario(false);
}
#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn kwin_gui_overlay() {
    scenario(true);
}
