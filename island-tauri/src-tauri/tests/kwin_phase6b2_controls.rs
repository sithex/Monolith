//! Real IPC/events + module lifecycle. A separate private bus owns PowerDevil; KWin remains
//! isolated too. The renderer itself stays byte-identical: no Phase 7 view is introduced.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "helpers/control_fakes.rs"]
mod control_fakes;
#[path = "helpers/controls_fixture.rs"]
mod controls_fixture;
#[path = "helpers/extra_controls_fixture.rs"]
mod extra_controls_fixture;
#[path = "helpers/gui_stack.rs"]
#[allow(dead_code)]
mod gui_stack;
#[path = "helpers/phase3_host.rs"]
mod host;
#[path = "helpers/private_bus.rs"]
mod private_bus;
use common::*;
use controls_fixture::ControlsMock;
use gui_stack::Stack;
use private_bus::PrivateBus;
use serde_json::{json, Value};
use std::{fs, process::Command, time::Duration};
fn invoke(s: &Stack, name: &str, args: Value) -> Value {
    host::eval(
        s,
        &format!("window.__TAURI_INTERNALS__.invoke({},{args})", json!(name)),
    )
}
#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn remaining_controls_ipc_events_commands_bootstrap_and_module_cleanup_are_private() {
    let _lock = lock();
    let s = Stack::new("p6b2-controls");
    host::prepare(&s);
    let bus = PrivateBus::start("6b2-controls-ipc");
    let mut mock = extra_controls_fixture::ExtraMock::start(&bus);
    let f = control_fakes::Fakes::within(&s.root, "controls-ipc");
    let data = s.root.join("inst");
    let log = fs::File::create(s.root.join("app.log")).unwrap();
    let _app = host::Owned(
        host::command(&s, &data)
            .env("HERMES_ISLAND_CONTROLS_SESSION_BUS", bus.address())
            .env("HERMES_ISLAND_CONTROLS_SYSTEM_BUS", bus.address())
            .env("HERMES_ISLAND_CONTROLS_FIXTURE", &f.root)
            .env("WAYLAND_DISPLAY", "hi-gui-wayland")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    wait_for("6b2 Controls socket", Duration::from_secs(30), || {
        data.join("test-control.sock").exists().then_some(())
    });
    host::until(
        &s,
        "6b2 Controls renderer",
        20,
        "!!document.querySelector('[data-state]')",
    );
    host::eval(&s,"(async()=>{window.__controls={};for(const topic of ['mixer-changed','toggles-changed','tiles-changed','updates-changed']){window.__controls[topic]=[];await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:topic,target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(e=>window.__controls[topic].push(e.payload))});}await window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{clipboardHistory:true}});const settings=await window.__TAURI_INTERNALS__.invoke('settings_get');for(const m of settings.moduleRegistry.modules){if(['system-tiles','update-badge'].includes(m.id))m.enabled=true;}return window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{moduleRegistry:settings.moduleRegistry}})})()");
    host::until(&s,"6b2 all Controls initial snapshots",10,"window.__TAURI_INTERNALS__.invoke('island_bootstrap').then(v=>v.apps.mixer?.available&&v.apps.toggles?.powerAvailable&&v.apps.toggles?.micAvailable&&v.apps.tiles?.voice.available&&v.apps.tiles?.network.available&&v.apps.tiles?.keepAwake.available&&v.apps.updates?.available)");
    invoke(
        &s,
        "mixer_command",
        json!({"op":{"type":"sinkVolume","name":"headset","percent":65}}),
    );
    host::until(
        &s,
        "6b2 mixer typed event",
        5,
        "window.__controls['mixer-changed'].some(v=>v.sinks[0]?.volumePercent===65)",
    );
    invoke(
        &s,
        "toggles_command",
        json!({"op":{"type":"micMute","muted":true}}),
    );
    host::until(
        &s,
        "6b2 mic typed event",
        5,
        "window.__controls['toggles-changed'].some(v=>v.micMuted===true)",
    );
    invoke(
        &s,
        "toggles_command",
        json!({"op":{"type":"powerProfile","profile":"performance"}}),
    );
    assert!(mock
        .calls()
        .iter()
        .any(|c| c[1] == "Set:ActiveProfile" && c[2] == json!(["performance"])));
    invoke(
        &s,
        "tiles_command",
        json!({"op":{"type":"output","name":"hdmi"}}),
    );
    host::until(
        &s,
        "6b2 output typed event",
        5,
        "window.__controls['tiles-changed'].some(v=>v.output.current==='hdmi')",
    );
    host::until(&s,"6b2 private Clipboard connection ready",8,"window.__TAURI_INTERNALS__.invoke('island_bootstrap').then(v=>v.capabilities.clipboardStatus.available)");
    invoke(&s, "tiles_command", json!({"op":{"type":"pickColour"}}));
    host::until(&s,"6b2 colour copied through private Clipboard service",8,"window.__TAURI_INTERNALS__.invoke('tiles_get').then(v=>v.lastColour==='#12ab34'&&!v.pickerBusy&&!v.actionError)");
    invoke(&s, "tiles_command", json!({"op":{"type":"screenshot"}}));
    host::until(
        &s,
        "6b2 screenshot job done",
        5,
        "window.__TAURI_INTERNALS__.invoke('tiles_get').then(v=>!v.screenshotBusy)",
    );
    invoke(
        &s,
        "tiles_command",
        json!({"op":{"type":"voice","name":"fixture-b"}}),
    );
    assert_eq!(
        invoke(&s, "tiles_get", json!({}))["voice"]["active"],
        "fixture-b"
    );
    assert_eq!(invoke(&s, "updates_refresh", json!({}))["count"], 1);
    host::until(
        &s,
        "6b2 updates latest-value event",
        5,
        "window.__controls['updates-changed'].some(v=>v.available&&v.count===1)",
    );
    invoke(
        &s,
        "tiles_command",
        json!({"op":{"type":"keepAwake","on":true,"durationMs":null}}),
    );
    host::until(
        &s,
        "6b2 owned inhibit cookie",
        5,
        "window.__TAURI_INTERNALS__.invoke('tiles_get').then(v=>v.keepAwake.on===true)",
    );
    host::eval(&s,"(async()=>{const settings=await window.__TAURI_INTERNALS__.invoke('settings_get');for(const m of settings.moduleRegistry.modules){if(['mixer','quick-toggles','system-tiles','update-badge'].includes(m.id))m.enabled=false;}return window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{moduleRegistry:settings.moduleRegistry}})})()");
    mock.send(json!({"op":"assert_cookies","count":0}));
    let apps = invoke(&s, "island_bootstrap", json!({}))["apps"].clone();
    for key in ["mixer", "toggles", "tiles", "updates"] {
        assert!(apps[key].is_null());
    }
    for cmd in ["mixer_get", "toggles_get", "tiles_get", "updates_get"] {
        assert_eq!(host::eval(&s,&format!("window.__TAURI_INTERNALS__.invoke({}).then(()=>false,e=>e.code==='unavailable')",json!(cmd))),true);
    }
    assert!(f
        .calls()
        .iter()
        .any(|c| c[1] == json!(["set-source-mute", "@DEFAULT_SOURCE@", "1"])));
    assert!(f
        .calls()
        .iter()
        .any(|c| c[0] == "spectacle" && c[1] == json!(["-r"])));
}
#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn agents_ipc_fixture_poll_event_and_module_cleanup_are_private() {
    let _lock = lock();
    let s = Stack::new("p6b2-agents");
    host::prepare(&s);
    let db = s.root.join("fixture.db");
    let create = Command::new("/usr/bin/python3").args(["-I","-c",
        "import sqlite3,time,sys; c=sqlite3.connect(sys.argv[1]); c.execute('CREATE TABLE sessions (id TEXT, source TEXT, parent_session_id TEXT, model TEXT, billing_provider TEXT, started_at REAL, ended_at REAL, end_reason TEXT, last_activity_at REAL, api_call_count INTEGER, title TEXT)'); n=time.time(); c.execute('INSERT INTO sessions VALUES (?,?,?,?,?,?,?,?,?,?,?)',('gui','cli',None,None,None,n-10,None,None,n,None,'Fixture agent')); c.commit(); c.close()"])
        .arg(&db).env_clear().envs(s.env()).output().unwrap();
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    let original = fs::read(&db).unwrap();
    let data = s.root.join("inst");
    let log = fs::File::create(s.root.join("app.log")).unwrap();
    let _app = host::Owned(
        host::command(&s, &data)
            .env("HERMES_ISLAND_AGENT_DB", &db)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    wait_for("6b2 agent control socket", Duration::from_secs(30), || {
        data.join("test-control.sock").exists().then_some(())
    });
    host::until(
        &s,
        "6b2 agent renderer ready",
        20,
        "!!document.querySelector('[data-state]')",
    );
    assert_eq!(host::eval(&s,"window.__TAURI_INTERNALS__.invoke('agents_get').then(()=>false,e=>e.code==='unavailable')"),true);
    host::eval(&s,"(async()=>{window.__agents=[];await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'agents-changed',target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(e=>window.__agents.push(e.payload))});const settings=await window.__TAURI_INTERNALS__.invoke('settings_get');settings.moduleRegistry.modules.find(m=>m.id==='agent-tracker').enabled=true;return window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{moduleRegistry:settings.moduleRegistry}})})()");
    host::until(
        &s,
        "6b2 fixture agent initial event",
        5,
        "window.__agents.some(v=>v.available&&v.chips.some(c=>c.id==='gui'))",
    );
    assert_eq!(fs::read(&db).unwrap(), original);
    assert!(invoke(&s, "agents_get", json!({}))["chips"][0]["apiCallCount"].is_null());
    assert_eq!(
        invoke(&s, "island_bootstrap", json!({}))["apps"]["agents"]["chips"][0]["id"],
        "gui"
    );
    let finish = Command::new("/usr/bin/python3").args(["-I","-c","import sqlite3,time,sys; c=sqlite3.connect(sys.argv[1]); c.execute('UPDATE sessions SET ended_at=? WHERE id=?',(time.time(),'gui')); c.commit(); c.close()"])
        .arg(&db).env_clear().envs(s.env()).output().unwrap();
    assert!(finish.status.success());
    host::until(&s,"6b2 fixture agent 15-second completion poll",22,"window.__TAURI_INTERNALS__.invoke('notifications_list').then(v=>v.some(n=>n.title==='Agent run finished'))");
    host::until(
        &s,
        "6b2 fixture agent empty chips event",
        5,
        "window.__agents.some(v=>v.available&&v.chips.length===0)",
    );
    host::eval(&s,"(async()=>{const settings=await window.__TAURI_INTERNALS__.invoke('settings_get');settings.moduleRegistry.modules.find(m=>m.id==='agent-tracker').enabled=false;return window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{moduleRegistry:settings.moduleRegistry}})})()");
    assert_eq!(host::eval(&s,"window.__TAURI_INTERNALS__.invoke('agents_get').then(()=>false,e=>e.code==='unavailable')"),true);
    assert!(invoke(&s, "island_bootstrap", json!({}))["apps"]["agents"].is_null());
    assert_eq!(
        invoke(&s, "settings_get", json!({}))["moduleRegistry"]["modules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == "agent-tracker")
            .unwrap()["enabled"],
        false
    );
}

#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn brightness_ipc_event_and_module_disable_reenable_use_only_the_private_daemon() {
    let _lock = lock();
    let s = Stack::new("p6b2");
    host::prepare(&s);
    let bus = PrivateBus::start("6b2-ipc");
    let mock = ControlsMock::start(&bus);
    let data = s.root.join("inst");
    let log = fs::File::create(s.root.join("app.log")).unwrap();
    let _app = host::Owned(
        host::command(&s, &data)
            .env("HERMES_ISLAND_CONTROLS_SESSION_BUS", bus.address())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    wait_for("6b2 control socket", Duration::from_secs(30), || {
        data.join("test-control.sock").exists().then_some(())
    });
    host::until(
        &s,
        "6b2 legacy renderer ready",
        20,
        "!!document.querySelector('[data-state]')",
    );
    host::until(&s,"6b2 private brightness initial snapshot",10,"window.__TAURI_INTERNALS__.invoke('brightness_get').then(v=>v.available&&v.displays[0]?.percent===50)");
    assert_eq!(
        invoke(&s, "island_bootstrap", json!({}))["apps"]["brightness"]["displays"][0]["percent"],
        50
    );
    host::eval(&s,"(async()=>{window.__brightness=[];await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'brightness-changed',target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(e=>window.__brightness.push(e.payload))});return true})()");
    invoke(
        &s,
        "brightness_command",
        json!({"op":{"type":"drag","name":"display0","percent":64}}),
    );
    host::until(
        &s,
        "6b2 real brightness event",
        5,
        "window.__brightness.some(v=>v.available&&v.displays[0]?.percent===64)",
    );
    invoke(
        &s,
        "brightness_command",
        json!({"op":{"type":"release","name":"display0"}}),
    );
    assert_eq!(
        mock.calls(),
        vec![json!([
            "/org/kde/ScreenBrightness/display0",
            "SetBrightnessWithContext",
            [6400, 1, "hermes-island"]
        ])]
    );
    for bad in [
        json!(["drag", "display0", 70]),
        json!({"type":"drag","name":"display0","percent":70,"unknown":true}),
        json!({"type":"drag","name":"display0","percent":101}),
    ] {
        let refused=host::eval(&s,&format!("window.__TAURI_INTERNALS__.invoke('brightness_command',{{op:{bad}}}).then(()=>false,()=>true)"));
        assert_eq!(refused, true);
    }
    assert_eq!(mock.calls().len(), 1);
    host::eval(&s,"(async()=>{const settings=await window.__TAURI_INTERNALS__.invoke('settings_get');settings.moduleRegistry.modules.find(m=>m.id==='brightness').enabled=false;return window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{moduleRegistry:settings.moduleRegistry}})})()");
    assert_eq!(host::eval(&s,"window.__TAURI_INTERNALS__.invoke('brightness_get').then(()=>false,e=>e.code==='unavailable')"),true);
    assert!(invoke(&s, "island_bootstrap", json!({}))["apps"]["brightness"].is_null());
    host::eval(&s,"(async()=>{const settings=await window.__TAURI_INTERNALS__.invoke('settings_get');settings.moduleRegistry.modules.find(m=>m.id==='brightness').enabled=true;return window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{moduleRegistry:settings.moduleRegistry}})})()");
    host::until(&s,"6b2 private brightness reenabled snapshot",10,"window.__TAURI_INTERNALS__.invoke('brightness_get').then(v=>v.available&&v.displays[0]?.percent===64)");
    assert_eq!(
        mock.calls().len(),
        1,
        "module restart must not write brightness"
    );
}
