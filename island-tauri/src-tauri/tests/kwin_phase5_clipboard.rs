//! Phase 5 acceptance: REAL Wayland owners on a private KWin, never desktop selection.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "helpers/clipboard_source.rs"]
mod clipboard_source;
mod common;
#[path = "helpers/gui_stack.rs"]
#[allow(dead_code)]
mod gui_stack;
#[path = "helpers/phase3_host.rs"]
mod host;
use common::*;
use gui_stack::Stack;
use host::{eval, until, Owned};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::Path,
    process::{Command, Stdio},
    thread::sleep,
    time::{Duration, Instant},
};

#[test]
#[ignore = "subprocess-only real clipboard source; requires sandbox opt-in"]
fn clipboard_source_helper() {
    clipboard_source::serve();
}

fn app(s: &Stack, wayland: bool) -> Owned {
    host::prepare(s);
    let log = fs::File::create(s.root.join("app.log")).unwrap();
    let mut command = host::command(s, &s.root.join("inst"));
    if wayland {
        command.env("WAYLAND_DISPLAY", "hi-gui-wayland");
    }
    let app = Owned(
        command
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    wait_for("phase5 private control", Duration::from_secs(30), || {
        s.root.join("inst/test-control.sock").exists().then_some(())
    });
    until(
        s,
        "phase5 legacy ready",
        20,
        "!!document.querySelector('[data-state]')",
    );
    app
}
fn patch(s: &Stack, value: Value) {
    eval(s, &format!("window.island.setSettings({value})"));
}
fn report(s: &Stack) -> Value {
    eval(s, "window.__TAURI_INTERNALS__.invoke('island_bootstrap').then(b=>b.capabilities.clipboardStatus)")
}
fn list(s: &Stack) -> Value {
    eval(
        s,
        "window.__TAURI_INTERNALS__.invoke('clipboard_list',{query:null,limit:500})",
    )
}
fn invoke(s: &Stack, name: &str, args: Value) -> Value {
    eval(
        s,
        &format!("window.__TAURI_INTERNALS__.invoke({},{args})", json!(name)),
    )
}
fn enable(s: &Stack) {
    patch(s, json!({"clipboardHistory":true}));
    wait_for(
        "phase5 real data-control bound",
        Duration::from_secs(8),
        || (report(s)["available"] == true).then_some(()),
    );
    assert_eq!(report(s)["detail"], "ext-data-control");
}
fn source(s: &Stack, mode: &str, text: &str) -> Owned {
    source_sized(s, mode, text, None)
}
fn source_sized(s: &Stack, mode: &str, text: &str, bytes: Option<usize>) -> Owned {
    let ready = s.root.join("source-ready");
    let served = s.root.join("source-served");
    if served.exists() {
        fs::remove_file(&served).unwrap();
    }
    if ready.exists() {
        fs::remove_file(&ready).unwrap();
    }
    let log = fs::File::create(s.root.join("source.log")).unwrap();
    let mut child = Owned(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "clipboard_source_helper",
                "--ignored",
                "--nocapture",
            ])
            .env_clear()
            .envs(s.env())
            .env("WAYLAND_DISPLAY", "hi-gui-wayland")
            .env("HERMES_ISLAND_SCRATCH", &s.root)
            .env("HERMES_ISLAND_CLIPBOARD_FIXTURE", "1")
            .env("HI_CLIPBOARD_MODE", mode)
            .env("HI_CLIPBOARD_TEXT", text)
            .env("HI_CLIPBOARD_BYTES", bytes.unwrap_or(0).to_string())
            .env("HI_CLIPBOARD_READY", &ready)
            .envs(
                matches!(mode, "once" | "concealed" | "transient")
                    .then_some(("HI_CLIPBOARD_SERVED", &served)),
            )
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .stdin(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap(),
    );
    wait_for("real clipboard owner ready", Duration::from_secs(6), || {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "{}",
            fs::read_to_string(s.root.join("source.log")).unwrap()
        );
        ready.exists().then_some(())
    });
    child
}
fn capture(s: &Stack, text: &str) -> Owned {
    let source = source(s, "text", text);
    wait_for(
        "watcher captured real selection",
        Duration::from_secs(6),
        || (list(s)[0]["preview"] == text).then_some(()),
    );
    source
}
fn no_capture_for(s: &Stack, count: usize, millis: u64) {
    let deadline = Instant::now() + Duration::from_millis(millis);
    while Instant::now() < deadline {
        assert_eq!(list(s).as_array().unwrap().len(), count);
        sleep(Duration::from_millis(50));
    }
}
fn watcher_threads(pid: u32) -> Vec<String> {
    fs::read_dir(format!("/proc/{pid}/task"))
        .unwrap()
        .flatten()
        .filter_map(|e| fs::read_to_string(e.path().join("comm")).ok())
        .filter(|name| name.trim() == "hi-clipboard")
        .collect()
}
fn finish(s: Stack, app: Owned) {
    let pid = app.0.id();
    drop(app);
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    let mut s = s;
    s.shutdown();
    let root = s.root.clone();
    drop(s);
    assert!(!root.exists());
}

#[test]
fn clipboard_default_off_has_no_watcher_thread_or_connection() {
    let _guard = lock();
    let s = Stack::new("c-off");
    let app = app(&s, true);
    let owner = source(&s, "text", "not captured while off");
    no_capture_for(&s, 0, 650);
    let status = report(&s);
    assert_eq!(status["enabled"], false);
    assert_eq!(status["watcherRunning"], false);
    assert_eq!(status["connectionsOpen"], 0);
    assert_eq!(status["connectionAttempts"], 0);
    assert!(watcher_threads(app.0.id()).is_empty());
    assert!(!s.root.join("inst/clipboard.json").exists());
    drop(owner);
    finish(s, app);
}
#[test]
fn clipboard_real_watcher_reads_text_png_and_uri_list() {
    use base64::Engine;
    let _guard = lock();
    let s = Stack::new("c-mime");
    let app = app(&s, true);
    enable(&s);
    eval(&s, "(async()=>{window.__clipEvents=[];await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'clipboard-added',target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(e=>window.__clipEvents.push(e.payload))});return true})()");
    let text = capture(&s, "private text α");
    let id = list(&s)[0]["id"].clone();
    assert_eq!(
        invoke(&s, "clipboard_get", json!({"id":id}))["text"],
        "private text α"
    );
    let png = source(&s, "png", "");
    wait_for("real PNG reaches history", Duration::from_secs(6), || {
        (list(&s)[0]["kind"] == "image").then_some(())
    });
    let content = invoke(&s, "clipboard_get", json!({"id":list(&s)[0]["id"]}));
    assert_eq!(
        (content["width"].as_u64(), content["height"].as_u64()),
        (Some(2), Some(3))
    );
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            content["pngDataUrl"]
                .as_str()
                .unwrap()
                .strip_prefix("data:image/png;base64,")
                .unwrap(),
        )
        .unwrap();
    let image = image::load_from_memory(&bytes).unwrap();
    assert_eq!((image.width(), image.height()), (2, 3));
    let files = source(&s, "files", "");
    wait_for(
        "real URI list reaches history",
        Duration::from_secs(6),
        || (list(&s)[0]["kind"] == "files").then_some(()),
    );
    assert_eq!(
        invoke(&s, "clipboard_get", json!({"id":list(&s)[0]["id"]}))["uris"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    until(
        &s,
        "real Added emitted through WebKit",
        5,
        "window.__clipEvents.length===3&&window.__clipEvents.every(x=>Number.isInteger(x.rev))",
    );
    drop(files);
    drop(png);
    drop(text);
    finish(s, app);
}
#[test]
fn clipboard_disable_joins_watcher_and_closes_its_connection() {
    let _guard = lock();
    let s = Stack::new("c-stop");
    let app = app(&s, true);
    enable(&s);
    let first = capture(&s, "before disable");
    assert_eq!(watcher_threads(app.0.id()).len(), 1);
    patch(&s, json!({"clipboardHistory":false}));
    assert!(watcher_threads(app.0.id()).is_empty());
    let status = report(&s);
    assert_eq!(status["watcherRunning"], false);
    assert_eq!(status["connectionsOpen"], 0);
    let second = source(&s, "text", "after disable");
    no_capture_for(&s, 1, 650);
    assert_eq!(list(&s)[0]["preview"], "before disable");
    drop(second);
    drop(first);
    finish(s, app);
}
#[test]
fn clipboard_pause_discards_real_offers_and_resume_captures_only_future_changes() {
    let _guard = lock();
    let s = Stack::new("c-pause");
    let app = app(&s, true);
    enable(&s);
    let before = capture(&s, "before pause");
    invoke(&s, "clipboard_set_paused", json!({"paused":true}));
    assert_eq!(report(&s)["paused"], true);
    let during = source(&s, "text", "during pause");
    no_capture_for(&s, 1, 700);
    // A second DISTINCT selection after the transition fence separates sustained
    // pause policy from the one-off stale/in-flight-transfer cancellation.
    let later = source(&s, "text", "later while still paused");
    no_capture_for(&s, 1, 700);
    invoke(&s, "clipboard_set_paused", json!({"paused":false}));
    no_capture_for(&s, 1, 400);
    let after = capture(&s, "after resume");
    assert_eq!(list(&s).as_array().unwrap().len(), 2);
    drop(after);
    drop(later);
    drop(during);
    drop(before);
    finish(s, app);
}
#[test]
fn clipboard_sensitive_hint_and_validated_deny_patterns_skip_real_offers() {
    let _guard = lock();
    let s = Stack::new("c-hide");
    let app = app(&s, true);
    enable(&s);
    let secret = source(&s, "sensitive", "must never store");
    wait_for(
        "password hint read and skipped",
        Duration::from_secs(6),
        || (report(&s)["sensitiveSkipped"] == 1).then_some(()),
    );
    assert!(list(&s).as_array().unwrap().is_empty());
    patch(&s, json!({"clipboardDenyPatterns":["^deny-"]}));
    let denied = source(&s, "text", "deny-this");
    wait_for("real offer denied", Duration::from_secs(6), || {
        (report(&s)["denied"] == 1).then_some(())
    });
    let allowed = capture(&s, "allowed sentinel");
    assert_eq!(list(&s).as_array().unwrap().len(), 1);
    drop(allowed);
    drop(denied);
    drop(secret);
    finish(s, app);
}
#[test]
fn clipboard_concealed_and_transient_offers_are_skipped_and_counted_as_sensitive() {
    let _guard = lock();
    let s = Stack::new("c-conceal");
    let app = app(&s, true);
    enable(&s);
    let served = s.root.join("source-served");
    // Positive control: a normal one-request owner IS read by the watcher, which proves
    // the served-marker mechanism (so the skipped offers below were never read at all).
    let control = source(&s, "once", "served-marker control");
    wait_for("control content served", Duration::from_secs(6), || {
        served.exists().then_some(())
    });
    // A one-request owner clears its selection after serving, so whether the control
    // itself lands in history is a race; take the baseline once things have settled.
    sleep(Duration::from_millis(900));
    drop(control);
    let base = list(&s).as_array().unwrap().len();
    let captured = report(&s)["captured"].as_u64().unwrap();
    assert!(base <= 1);
    // Each marker is exercised separately so renaming either match arm is caught.
    // The watcher must skip by MIME alone: the secret content is never requested.
    let concealed = source(&s, "concealed", "concealed-secret-text");
    wait_for(
        "ConcealedType offer counted as sensitive",
        Duration::from_secs(6),
        || (report(&s)["sensitiveSkipped"] == 1).then_some(()),
    );
    no_capture_for(&s, base, 700);
    assert!(!served.exists(), "ConcealedType content was read");
    assert_eq!(report(&s)["captured"], captured);
    // A source whose selection is taken over also returns from `serve()` and would
    // write the marker: end each owner before the next one starts.
    drop(concealed);
    let transient = source(&s, "transient", "transient-secret-text");
    wait_for(
        "TransientType offer counted as sensitive",
        Duration::from_secs(6),
        || (report(&s)["sensitiveSkipped"] == 2).then_some(()),
    );
    no_capture_for(&s, base, 700);
    assert!(!served.exists(), "TransientType content was read");
    assert_eq!(report(&s)["captured"], captured);
    drop(transient);
    // The watcher is still healthy: a normal offer afterwards is captured.
    let normal = capture(&s, "normal after markers");
    let status = report(&s);
    assert_eq!(status["sensitiveSkipped"], 2);
    assert_eq!(status["captured"].as_u64().unwrap(), captured + 1);
    assert_eq!(list(&s).as_array().unwrap().len(), base + 1);
    drop(normal);
    finish(s, app);
}
#[test]
fn clipboard_read_caps_drop_oversize_offers_and_a_following_offer_is_still_captured() {
    let _guard = lock();
    let s = Stack::new("c-caps");
    let app = app(&s, true);
    enable(&s);
    // Exactly the 1 MiB text cap is accepted (boundary is inclusive).
    let exact = source_sized(&s, "huge", "", Some(1024 * 1024));
    wait_for(
        "exactly 1 MiB text captured",
        Duration::from_secs(10),
        || (list(&s).as_array().unwrap().len() == 1).then_some(()),
    );
    assert_eq!(list(&s)[0]["bytes"], 1024 * 1024);
    drop(exact);
    // One byte more than the cap is dropped, never captured.
    let over = source_sized(&s, "huge", "", Some(1024 * 1024 + 1));
    wait_for("oversize text dropped", Duration::from_secs(10), || {
        (report(&s)["dropped"] == 1).then_some(())
    });
    no_capture_for(&s, 1, 500);
    assert_eq!(report(&s)["captured"], 1);
    drop(over);
    // The same `cap` guards the tiny password-manager hint (64 bytes): an oversized
    // hint is dropped as a protocol violation, and its text is NOT captured.
    let hint = source(&s, "bighint", "text behind an oversized hint");
    wait_for("oversize hint dropped", Duration::from_secs(10), || {
        (report(&s)["dropped"] == 2).then_some(())
    });
    no_capture_for(&s, 1, 500);
    assert_eq!(report(&s)["captured"], 1);
    drop(hint);
    // A following normal offer still goes through.
    let next = capture(&s, "normal after oversize");
    assert_eq!(list(&s).as_array().unwrap().len(), 2);
    assert_eq!(report(&s)["dropped"], 2);
    drop(next);
    finish(s, app);
}
#[test]
fn clipboard_copy_echo_reorders_only_with_stable_id_and_pin() {
    let _guard = lock();
    let s = Stack::new("c-echo");
    let app = app(&s, true);
    enable(&s);
    let first = capture(&s, "copy first");
    let id = list(&s)[0]["id"].clone();
    invoke(&s, "clipboard_pin", json!({"id":id,"pinned":true}));
    let second = capture(&s, "copy second");
    let captured = report(&s)["captured"].clone();
    invoke(&s, "clipboard_copy", json!({"id":id}));
    wait_for(
        "actual compositor self echo consumed",
        Duration::from_secs(6),
        || (report(&s)["echoes"] == 1).then_some(()),
    );
    assert_eq!(list(&s)[0]["id"], id);
    assert_eq!(list(&s)[0]["pinned"], true);
    assert_eq!(list(&s).as_array().unwrap().len(), 2);
    assert_eq!(report(&s)["captured"], captured);
    drop(second);
    drop(first);
    finish(s, app);
}
#[test]
fn clipboard_pinned_survive_clear_and_persistence_is_private_pinned_only() {
    let _guard = lock();
    let s = Stack::new("c-save");
    let app = app(&s, true);
    enable(&s);
    let pin = capture(&s, "saved pin");
    let id = list(&s)[0]["id"].clone();
    invoke(&s, "clipboard_pin", json!({"id":id,"pinned":true}));
    let other = capture(&s, "unsaved private text");
    patch(&s, json!({"clipboardPersist":true}));
    let file = s.root.join("inst/clipboard.json");
    let raw: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(raw["entries"].as_array().unwrap().len(), 1);
    assert_eq!(raw["entries"][0]["id"], id);
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    invoke(&s, "clipboard_clear", json!({}));
    assert_eq!(list(&s).as_array().unwrap().len(), 1);
    assert_eq!(list(&s)[0]["id"], id);
    patch(&s, json!({"clipboardHistory":false}));
    drop(other);
    drop(pin);
    finish(s, app);
}
#[test]
fn clipboard_x11_only_environment_reports_unavailable_without_wayland_connection() {
    let _guard = lock();
    let s = Stack::new("c-x11");
    let app = app(&s, false);
    patch(&s, json!({"clipboardHistory":true}));
    wait_for("X11-only has honest reason", Duration::from_secs(5), || {
        let status = report(&s);
        (status["enabled"] == true
            && status["available"] == false
            && status["detail"]
                .as_str()
                .is_some_and(|v| v.contains("Wayland") || v.contains("WAYLAND")))
        .then_some(())
    });
    assert_eq!(report(&s)["connectionsOpen"], 0);
    finish(s, app);
}

fn tray_probe(s: &Stack, action: &str) -> Value {
    let output = Command::new("/usr/bin/python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/tray_probe.py"))
        .arg(action)
        .env_clear()
        .envs(s.env())
        .env("HERMES_ISLAND_SCRATCH", &s.root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    if action == "snapshot" {
        serde_json::from_slice(&output.stdout).unwrap()
    } else {
        Value::Null
    }
}

#[test]
fn clipboard_tray_pause_intent_and_push_update_the_actual_sibling_ui() {
    let _guard = lock();
    let s = Stack::new("c-tray");
    let log = fs::File::create(s.root.join("tray.log")).unwrap();
    let watcher = Owned(
        Command::new("/usr/bin/python3")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/tray_probe.py"))
            .arg("watch")
            .env_clear()
            .envs(s.env())
            .env("HERMES_ISLAND_SCRATCH", &s.root)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .process_group(0)
            .spawn()
            .unwrap(),
    );
    wait_for("phase5 private tray host", Duration::from_secs(6), || {
        s.root.join("tray-watcher-ready").exists().then_some(())
    });
    let app = app(&s, true);
    wait_for(
        "phase5 actual native tray registration",
        Duration::from_secs(6),
        || s.root.join("tray-items.json").exists().then_some(()),
    );
    let snapshot = tray_probe(&s, "snapshot");
    assert_eq!(snapshot["ownerPid"], app.0.id());
    let row = snapshot["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["label"] == "Clipboard paused")
        .unwrap();
    assert_eq!(row["properties"]["enabled"], false);
    enable(&s);
    tray_probe(&s, "Clipboard history");
    until(&s,"clipboard sibling mounts inside the existing shape",6,"document.querySelector('[data-testid=clipboard-surface]')?.textContent.includes('Capturing clipboard changes')");
    tray_probe(&s, "Clipboard paused");
    until(
        &s,
        "tray pause pushes to actual sibling root",
        6,
        "document.querySelector('[data-testid=clipboard-status]')?.textContent==='Paused'",
    );
    assert_eq!(report(&s)["paused"], true);
    invoke(&s, "clipboard_set_paused", json!({"paused":false}));
    wait_for(
        "actor pause effect refreshes native check property",
        Duration::from_secs(6),
        || {
            let snapshot = tray_probe(&s, "snapshot");
            snapshot["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["label"] == "Clipboard paused")
                .filter(|r| r["properties"]["toggle-state"] == 0)
                .map(|_| ())
        },
    );
    until(&s,"resume pushes to actual sibling root",6,"document.querySelector('[data-testid=clipboard-status]')?.textContent==='Capturing clipboard changes'");
    drop(watcher);
    finish(s, app);
}
