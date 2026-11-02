//! Real app IPC/events and calendar secrecy on a private nested KWin; no Phase 7 UI.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "helpers/calendar_fixture.rs"]
#[allow(dead_code)]
mod calendar_fixture;
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
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};
fn invoke(s: &Stack, command: &str, args: Value) -> Value {
    eval(
        s,
        &format!(
            "window.__TAURI_INTERNALS__.invoke({},{args})",
            json!(command)
        ),
    )
}
fn fixture() -> String {
    "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:remote\r\nSUMMARY:Fixture appointment\r\nDTSTART:20261031T120000Z\r\nDTEND:20261031T130000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n".into()
}
#[test]
fn apps_commands_events_and_secret_never_cross_ipc_logs_or_status() {
    let _guard = lock();
    let mut s = Stack::new("p6b1");
    host::prepare(&s);
    let data = s.root.join("inst");
    fs::create_dir_all(&data).unwrap();
    fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
    let downloads = s.root.join("test-downloads");
    fs::create_dir(&downloads).unwrap();
    let server = calendar_fixture::CalendarFixture::start(fixture());
    let secret = server.url();
    hermes_island_lib::integrations::private_file::write(
        &data.join("calendar-secret-url"),
        secret.as_bytes(),
        8192,
    )
    .unwrap();
    let log = fs::File::create(s.root.join("app.log")).unwrap();
    let app = Owned(
        host::command(&s, &data)
            .env("HERMES_ISLAND_DOWNLOAD_DIR", &downloads)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    wait_for("6b1 control", Duration::from_secs(30), || {
        data.join("test-control.sock").exists().then_some(())
    });
    until(
        &s,
        "6b1 renderer ready",
        20,
        "!!document.querySelector('[data-state]')",
    );
    eval(&s,"(async()=>{window.__apps=[];for(const event of ['timer-changed','scratchpad-changed','downloads-changed','calendar-changed'])await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event,target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(e=>window.__apps.push(e.payload))});return true})()");
    invoke(&s, "calendar_refresh", json!({}));
    let bootstrap = invoke(&s, "island_bootstrap", json!({}));
    assert_eq!(bootstrap["apps"]["calendar"]["configured"], true);
    assert!(!bootstrap.to_string().contains(&secret));
    invoke(
        &s,
        "timer_command",
        json!({"op":{"type":"setCountdown","durationMs":1000}}),
    );
    invoke(
        &s,
        "timer_command",
        json!({"op":{"type":"start","kind":"countdown"}}),
    );
    invoke(
        &s,
        "island_dispatch",
        json!({"intent":{"type":"selectView","view":"monitor"}}),
    );
    invoke(
        &s,
        "scratchpad_command",
        json!({"op":{"type":"create","id":"gui"}}),
    );
    invoke(
        &s,
        "scratchpad_command",
        json!({"op":{"type":"edit","id":"gui","body":"Private fixture note"}}),
    );
    fs::write(downloads.join("fixture.txt"), b"fixture only").unwrap();
    until(&s,"6b1 notes persisted event",10,"window.__apps.some(e=>e.notes&&e.notes.some(n=>n.body==='Private fixture note')&&!e.dirty)");
    until(
        &s,
        "6b1 downloads event",
        10,
        "window.__apps.some(e=>e.items&&e.items.some(i=>i.name==='fixture.txt'))",
    );
    until(
        &s,
        "6b1 timer completion event",
        10,
        "window.__apps.some(e=>e.countdownStatus==='finished')",
    );
    let notifications = invoke(&s, "notifications_list", json!({}));
    assert!(notifications
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["appName"] == "Timer"));
    let remote = invoke(
        &s,
        "calendar_get",
        json!({"query":{"startS":1793404800,"endS":1793577600,"localOffsetS":0}}),
    );
    assert_eq!(remote["agenda"][0]["source"], "subscription");
    assert_eq!(remote["agenda"][0]["readOnly"], true);
    for command in [
        "scratchpad_get",
        "downloads_get",
        "timer_get",
        "island_bootstrap",
    ] {
        assert!(!invoke(&s, command, json!({})).to_string().contains(&secret));
    }
    let rejected=eval(&s,"window.__TAURI_INTERNALS__.invoke('downloads_open',{name:'fixture.txt',op:'open'}).then(()=>false,e=>e.code==='unsupported')");
    assert_eq!(rejected, true);
    let events = eval(&s, "JSON.stringify(window.__apps)");
    assert!(!events.as_str().unwrap().contains(&secret));
    wait_for("6b1 private status", Duration::from_secs(5), || {
        data.join("status.json").exists().then_some(())
    });
    assert!(!fs::read_to_string(data.join("status.json"))
        .unwrap()
        .contains(&secret));
    assert!(!fs::read_to_string(data.join("settings.json"))
        .unwrap_or_default()
        .contains(&secret));
    drop(app);
    assert!(!fs::read_to_string(s.root.join("app.log"))
        .unwrap()
        .contains(&secret));
    for name in [
        "scratchpad.json",
        "calendar-secret-url",
        "calendar-cache.json",
    ] {
        assert_eq!(
            fs::metadata(data.join(name)).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    s.shutdown();
    let root = s.root.clone();
    drop(s);
    assert!(!root.exists());
}
