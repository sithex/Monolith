//! Exercise Tauri's actual libayatana tray via a mock watcher on the private bus.
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
use serde_json::Value;
use std::{
    fs,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    thread::sleep,
    time::Duration,
};
fn probe(s: &Stack, action: &str) -> String {
    let result = Command::new("/usr/bin/python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/tray_probe.py"))
        .arg(action)
        .env_clear()
        .envs(s.env())
        .env("HERMES_ISLAND_SCRATCH", &s.root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "tray probe {}: {}",
        action,
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

fn wait_tray(s: &Stack, pid: u32, unread: u32, dnd: bool) {
    let label = format!("Notifications ({unread})");
    let tooltip = format!("Hermes Island — {unread} unread");
    let snapshot = wait_for(
        &format!("native tray count/tooltip={unread}, DND={dnd}"),
        Duration::from_secs(10),
        || {
            let snapshot: Value = serde_json::from_str(&probe(s, "snapshot")).unwrap();
            assert_eq!(snapshot["ownerPid"], pid, "not our native tray: {snapshot}");
            let rows = snapshot["rows"].as_array().unwrap();
            let notifications: Vec<_> = rows
                .iter()
                .filter(|row| {
                    row["label"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("Notifications ("))
                })
                .collect();
            let checks: Vec<_> = rows
                .iter()
                .filter(|row| row["label"] == "Do Not Disturb")
                .collect();
            let state = actor_state(s);
            (notifications.len() == 1
                && notifications[0]["label"] == label
                && checks.len() == 1
                && checks[0]["properties"]["toggle-type"] == "checkmark"
                && checks[0]["properties"]["toggle-state"] == i32::from(dnd)
                && snapshot["tooltip"]["title"] == tooltip
                && state["unread"] == unread
                && state["dnd"] == dnd)
                .then_some(snapshot)
        },
    );
    assert_eq!(snapshot["tooltip"]["title"], tooltip);
}

fn notify(s: &Stack, title: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_hermes-island"))
        .args(["notify", "--test-instance", "--data-dir"])
        .arg(s.root.join("inst"))
        .args([
            "--title",
            title,
            "--body",
            "Private tray fixture",
            "--ttl-ms",
            "0",
        ])
        .env_clear()
        .envs(s.env())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reply: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reply["ok"], true, "{reply}");
}

#[test]
fn private_watcher_registers_real_tray_and_menu_dispatches_actor_intents() {
    let _guard = lock();
    let mut s = Stack::new("phase4-tray");
    prepare(&s);
    let log = fs::File::create(s.root.join("watcher.log")).unwrap();
    let watcher = Owned(
        Command::new("/usr/bin/python3")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/tray_probe.py"))
            .arg("watch")
            .env_clear()
            .envs(s.env())
            .env("HERMES_ISLAND_SCRATCH", &s.root)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .stdin(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap(),
    );
    wait_for("mock tray watcher", Duration::from_secs(10), || {
        s.root.join("tray-watcher-ready").exists().then_some(())
    });
    let mut app = launch(&s);
    wait_for("tray app ready", Duration::from_secs(30), || {
        s.root.join("inst/test-control.sock").exists().then_some(())
    });
    wait_for("tray legacy page ready", Duration::from_secs(20), || {
        (eval(
            &s,
            "!!window.island && !!document.querySelector('[data-state]')",
        ) == true)
            .then_some(())
    });
    wait_for(
        "actual libayatana registration",
        Duration::from_secs(10),
        || s.root.join("tray-items.json").exists().then_some(()),
    );
    let menu = probe(&s, "dump");
    for label in [
        "Show/Hide island",
        "Do Not Disturb",
        "Restart Monolith",
        "Restart terminal",
        "Restart Plasma panel",
        "Open Monolith folder",
        "Open data folder",
        "View log",
        "Start at login",
        "Quit",
    ] {
        assert!(menu.contains(label), "missing {label}: {menu}");
    }
    let pid = app.0.id();
    wait_tray(&s, pid, 0, false);
    for gone in ["Open terminal", "Notifications (", "Settings…", "Clipboard"] {
        assert!(
            !menu.contains(gone),
            "in-app entry {gone} still in tray: {menu}"
        );
    }
    probe(&s, "Do Not Disturb");
    wait_for("tray DND intent", Duration::from_secs(5), || {
        (actor_state(&s)["dnd"] == true).then_some(())
    });
    wait_tray(&s, pid, 0, true);
    // A native check item toggles itself before its callback. Change DND through the
    // actor from another input adapter too, proving actor -> native refresh rather
    // than a property that merely matches the widget's local click state.
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('island_dispatch',{intent:{type:'setDnd',on:false}})",
    );
    wait_tray(&s, pid, 0, false);
    probe(&s, "Do Not Disturb");
    wait_tray(&s, pid, 0, true);
    // Real CLI -> private ingest -> hub -> actor -> exported DBusMenu/SNI properties.
    // Resident entries make count checks independent of wall-clock expiry.
    notify(&s, "Tray count first");
    wait_tray(&s, pid, 1, true);
    notify(&s, "Tray count second");
    wait_tray(&s, pid, 2, true);
    eval(&s, "(async()=>{const rows=await window.__TAURI_INTERNALS__.invoke('notifications_list');if(rows.length!==2)throw new Error('expected two tray fixtures');await window.__TAURI_INTERNALS__.invoke('notifications_dismiss',{id:rows[0].id});return true})()");
    wait_tray(&s, pid, 1, true);
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('notifications_clear')",
    );
    wait_tray(&s, pid, 0, true);
    probe(&s, "Do Not Disturb");
    wait_tray(&s, pid, 0, false);
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('island_dispatch',{intent:{type:'selectView',view:'terminal'}})",
    );
    wait_for("terminal view selected", Duration::from_secs(10), || {
        (actor_state(&s)["view"] == "terminal" && actor_state(&s)["terminalStarted"] == true)
            .then_some(())
    });
    wait_for("tray fixture prompt", Duration::from_secs(20), || {
        (eval(
            &s,
            "window.__islandTerminalTest?.text()?.includes('island-test>')",
        ) == true)
            .then_some(())
    });
    let old_pty = wait_for("tray fixture PTY pid", Duration::from_secs(10), || {
        status(&s)["pty"].as_u64().filter(|pid| *pid > 0)
    });
    let executable = fs::read_link(format!("/proc/{old_pty}/exe")).unwrap();
    assert_eq!(
        executable.file_name().unwrap(),
        "bash",
        "not the fixture shell"
    );
    let mut eis = Wheel::new(&s);
    // Focus xterm using a real private-KWin click after the view spring settles.
    sleep(Duration::from_millis(1800));
    let position = eval(&s, "(()=>{const r=document.querySelector('.xterm-screen').getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]})()");
    let bounds = status(&s);
    eis.click_at(
        (bounds["bounds"]["x"].as_f64().unwrap() + position[0].as_f64().unwrap()) as i32,
        (bounds["bounds"]["y"].as_f64().unwrap() + position[1].as_f64().unwrap()) as i32,
    );
    eis.send("type x");
    until(
        &s,
        "tray fixture accepts physical keyboard",
        8,
        "window.__islandTerminalTest.text().includes('island-test> x')",
    );
    eis.send("chord 29 22"); // Ctrl+U: clear the unsubmitted fixture draft.
    until(
        &s,
        "tray fixture draft cleared",
        8,
        "!window.__islandTerminalTest.text().includes('island-test> x')",
    );
    eis.send("chord 29 32"); // Ctrl+D exits ONLY the verified scratch bash fixture.
    until(
        &s,
        "tray fixture exited",
        10,
        "window.__islandTerminalTest.text().includes('[Hermes exited:')",
    );
    wait_for(
        "old tray fixture process reaped",
        Duration::from_secs(10),
        || (!Path::new(&format!("/proc/{old_pty}")).exists()).then_some(()),
    );
    // No terminal_restart/attach call in this test: only the real DBusMenu event can
    // dispatch RestartTerminal and run the actor's host effect. A live-PTY restart is
    // intentionally a no-op in the inherited service, so exercise an exited fixture.
    probe(&s, "Restart terminal");
    let restarted_pty = wait_for(
        "tray Restart event respawns fixture via actor",
        Duration::from_secs(15),
        || {
            status(&s)["pty"]
                .as_u64()
                .filter(|pid| *pid > 0 && *pid != old_pty)
        },
    );
    assert_eq!(
        fs::read_link(format!("/proc/{restarted_pty}/exe"))
            .unwrap()
            .file_name()
            .unwrap(),
        "bash"
    );
    let restarted = actor_state(&s);
    assert_eq!(restarted["mode"], "large");
    assert_eq!(restarted["view"], "terminal");
    assert_eq!(restarted["terminalStarted"], true);
    probe(&s, "Show/Hide island");
    wait_for("tray hide intent", Duration::from_secs(5), || {
        (actor_state(&s)["mode"] == "still").then_some(())
    });
    probe(&s, "Quit");
    wait_for("tray quit exits owned app", Duration::from_secs(15), || {
        app.0.try_wait().unwrap().map(|_| ())
    });
    assert!(!s.root.join("inst/ingest.sock").exists());
    wait_for(
        "restarted tray fixture cleaned up on Quit",
        Duration::from_secs(10),
        || (!Path::new(&format!("/proc/{restarted_pty}")).exists()).then_some(()),
    );
    drop(eis);
    drop(app);
    drop(watcher);
    s.shutdown();
    let path = s.root.clone();
    drop(s);
    assert!(!path.exists());
}
