//! Production monitor path on a mock-only private bus + nested KWin. No session inheritance.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "helpers/notification_fixture.rs"]
mod fixture;
#[path = "helpers/gui_stack.rs"]
#[allow(dead_code)]
mod gui_stack;
#[path = "helpers/phase3_host.rs"]
mod host;
use common::*;
use fixture::*;
use gui_stack::Stack;
use hermes_island_lib::integrations::notifications::linux_monitor::{
    Decoder, Observed, NAME, PATH,
};
use host::{eval, until};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Write},
    num::NonZeroU32,
    os::unix::net::UnixStream,
    process::Command,
    time::{Duration, Instant},
};
use zbus::{Message, OwnedVariant};

#[test]
fn passive_notify_monitor_correlates_notify_return_closed_and_actions() {
    let _guard = lock();
    let s = Stack::new("monitor-notify");
    let mut server = Mock::start(&s);
    let app = fixture::app(&s);
    ready(&s);

    notify(&s, "Private caller", "Monitored arrival");
    until(&s, "notify-send reaches store and actual toast", 8, "window.__TAURI_INTERNALS__.invoke('notifications_list').then(ns=>ns.some(n=>n.title==='Monitored arrival'&&n.body==='Private body'&&!n.canInvokeActions)&&document.querySelector('[data-testid=notification-toast]')?.textContent.includes('Monitored arrival'))");
    assert_eq!(host::actor_state(&s)["hoverCause"]["type"], "notification");
    assert_eq!(list(&s)[0]["id"], "d-1");
    server.send("action 1");
    wait_for("observed ActionInvoked", Duration::from_secs(5), || {
        (report(&s)["observedActions"] == 1).then_some(())
    });
    server.send("closed 1");
    wait_for(
        "NotificationClosed removes store item",
        Duration::from_secs(5),
        || list(&s).as_array().unwrap().is_empty().then_some(()),
    );
    until(
        &s,
        "NotificationClosed removes actual toast",
        5,
        "!document.querySelector('[data-testid=notification-toast]')",
    );
    drop(app);
    drop(server);
    drop(s);
}

#[test]
fn owner_replacement_rearms_monitor_and_bounds_pending() {
    let _guard = lock();
    let s = Stack::new("monitor-rearm");
    let mut server = Mock::start(&s);
    let app = fixture::app(&s);
    ready(&s);
    notify(&s, "Old peer", "Old owner");
    wait_for("old entry", Duration::from_secs(5), || {
        (list(&s).as_array().unwrap().len() == 1).then_some(())
    });
    server.quit();
    drop(server);
    wait_for("old owner reset", Duration::from_secs(5), || {
        list(&s).as_array().unwrap().is_empty().then_some(())
    });
    let server = Mock::start(&s);
    ready(&s);
    notify(&s, "New peer", "New owner");
    wait_for(
        "rearmed monitor receives new owner",
        Duration::from_secs(5),
        || (list(&s)[0]["title"] == "New owner").then_some(()),
    );
    assert!(report(&s)["rearms"].as_u64().unwrap() >= 1);
    assert!(report(&s)["pending"].as_u64().unwrap() <= 256);
    drop(app);
    drop(server);
    drop(s);
}

#[test]
fn loop_guard_and_policy_are_enforced_on_private_bus() {
    let _guard = lock();
    let s = Stack::new("monitor-policy");
    let server = Mock::start(&s);
    let app = fixture::app(&s);
    ready(&s);
    notify(&s, "Hermes Island", "Never mirror self");
    wait_for("monitor-layer loop guard", Duration::from_secs(5), || {
        (report(&s)["loopDropped"] == 1).then_some(())
    });
    assert!(list(&s).as_array().unwrap().is_empty());
    notify(&s, "Private app", "Existing private body");
    wait_for("private item", Duration::from_secs(5), || {
        (list(&s)[0]["body"] == "Private body").then_some(())
    });
    eval(&s, "window.island.setSettings({redactBodies:true})");
    wait_for(
        "retroactive scrub delivered to actual UI store",
        Duration::from_secs(5),
        || {
            (list(&s)[0]["body"] == "Hidden" && eval(&s,"document.querySelector('[data-testid=notification-toast]')?.textContent.includes('Hidden')") == true).then_some(())
        },
    );
    eval(&s, "window.island.setSettings({redactBodies:false})");
    assert_eq!(list(&s)[0]["body"], "Hidden", "scrub must be irreversible");
    eval(
        &s,
        "window.island.setSettings({notificationDeniedApps:['Private app']})",
    );
    wait_for(
        "retroactive deny removes UI toast",
        Duration::from_secs(5),
        || {
            (list(&s).as_array().unwrap().is_empty()
                && eval(
                    &s,
                    "!document.querySelector('[data-testid=notification-toast]')",
                ) == true)
                .then_some(())
        },
    );
    notify(&s, "Private app", "Denied new body");
    notify(&s, "Allowed app", "After denied traffic");
    wait_for("allowed following denied", Duration::from_secs(5), || {
        (list(&s)[0]["title"] == "After denied traffic").then_some(())
    });
    assert_eq!(list(&s).as_array().unwrap().len(), 1);
    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(s.root.join("inst/settings.json")).unwrap()).unwrap();
    assert_eq!(
        persisted["notificationDeniedApps"],
        serde_json::json!(["Private app"])
    );
    assert!(!s.root.join("inst/notifications.json").exists());
    drop(app);
    drop(server);
    drop(s);
}

#[test]
fn privacy_scrub_at_capacity_repairs_renderer_after_delivery_overflow() {
    let _guard = lock();
    // Keep KWin's derived EIS AF_UNIX path below the kernel's 108-byte limit.
    let s = Stack::new("privacy-cap");
    let server = Mock::start(&s);
    let app = fixture::app(&s);
    ready(&s);
    let send = |index| {
        let mut socket = UnixStream::connect(s.root.join("inst/ingest.sock")).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        writeln!(socket, "{}", serde_json::json!({"v":1,"op":"notify","source":"Overflow fixture","title":format!("Ring {index}"),"body":"Must be scrubbed","ttlMs":0})).unwrap();
        let mut reply = String::new();
        BufReader::new(socket).read_line(&mut reply).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&reply).unwrap()["ok"],
            true
        );
    };
    for index in 0..200 {
        send(index);
    }
    until(
        &s,
        "ring full",
        5,
        "window.__TAURI_INTERNALS__.invoke('notifications_list').then(n=>n.length===200)",
    );
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('island_dispatch',{intent:{type:'collapse'}})",
    );
    send(200);
    until(&s, "newest resident toast", 5, "document.querySelector('[data-testid=notification-toast]')?.textContent.includes('Ring 200')");
    eval(&s, "window.island.setSettings({redactBodies:true})");
    until(&s, "full-ring scrub repairs newest toast lost from delta queue", 5,
        "window.__TAURI_INTERNALS__.invoke('notifications_list').then(ns=>ns.length===200&&ns.every(n=>n.body==='Hidden')&&document.querySelector('[data-testid=notification-toast]')?.textContent.includes('Hidden'))");
    eval(
        &s,
        "window.island.setSettings({notificationDeniedApps:['Overflow fixture']})",
    );
    until(&s, "full-ring deny repairs missing close deltas", 5,
        "window.__TAURI_INTERNALS__.invoke('notifications_list').then(ns=>ns.length===0&&!document.querySelector('[data-testid=notification-toast]'))");
    drop(app);
    drop(server);
    drop(s);
}

#[test]
fn monitor_denial_preserves_hermes_ingest_capability() {
    let _guard = lock();
    let s = Stack::new("monitor-denial");
    let config = s.root.join("bus.conf");
    let text = fs::read_to_string(&config).unwrap().replace("</policy>", "<deny send_destination='org.freedesktop.DBus' send_interface='org.freedesktop.DBus.Monitoring' send_member='BecomeMonitor'/></policy>");
    fs::write(config, text).unwrap();
    let reloaded = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.ReloadConfig",
        ])
        .env_clear()
        .envs(s.env())
        .output()
        .unwrap();
    assert!(reloaded.status.success(), "{:?}", reloaded);
    let server = Mock::start(&s);
    let app = fixture::app(&s);
    wait_for(
        "permission denial is explicit",
        Duration::from_secs(8),
        || (report(&s)["detail"] == "monitor-permission-denied").then_some(()),
    );
    assert_eq!(report(&s)["available"], false);
    let caps = eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('island_bootstrap').then(b=>b.capabilities)",
    );
    assert_eq!(caps["notifications"], true);
    assert_eq!(caps["systemNotificationMirror"], false);
    assert_eq!(caps["canInvokeActions"], false);
    let sent = Command::new(env!("CARGO_BIN_EXE_hermes-island"))
        .args(["notify", "--test-instance", "--data-dir"])
        .arg(s.root.join("inst"))
        .args(["--title", "Ingest survives denial", "--ttl-ms", "0"])
        .env_clear()
        .envs(s.env())
        .output()
        .unwrap();
    assert!(sent.status.success());
    wait_for(
        "denial does not disable ingest",
        Duration::from_secs(5),
        || (list(&s)[0]["title"] == "Ingest survives denial").then_some(()),
    );
    drop(app);
    drop(server);
    drop(s);
}

#[test]
fn dismiss_only_requests_remote_close_for_live_same_server_id() {
    let _guard = lock();
    let s = Stack::new("monitor-dismiss");
    let mut server = Mock::start(&s);
    let app = fixture::app(&s);
    ready(&s);
    notify(&s, "Close peer", "Old id");
    wait_for("old d1", Duration::from_secs(5), || {
        (list(&s)[0]["id"] == "d-1").then_some(())
    });
    server.quit();
    drop(server);
    wait_for("old correlation reset", Duration::from_secs(5), || {
        list(&s).as_array().unwrap().is_empty().then_some(())
    });
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('notifications_dismiss',{id:'d-1'})",
    );
    let server = Mock::start(&s);
    ready(&s);
    notify(&s, "Close peer", "Reused new id");
    wait_for("new d1", Duration::from_secs(5), || {
        (list(&s)[0]["title"] == "Reused new id").then_some(())
    });
    let mock = || {
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(s.root.join("mock-state.json")).unwrap(),
        )
        .unwrap()
    };
    assert!(mock()["closed"].as_array().unwrap().is_empty());
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('notifications_dismiss',{id:'d-1'})",
    );
    wait_for(
        "real CloseNotification on connection A",
        Duration::from_secs(5),
        || (mock()["closed"] == serde_json::json!([1])).then_some(()),
    );
    assert!(list(&s).as_array().unwrap().is_empty());
    drop(app);
    drop(server);
    drop(s);
}

fn call(sender: &str, serial: u32, app: &str) -> Message {
    call_to(sender, serial, app, NAME)
}
fn call_to(sender: &str, serial: u32, app: &str, destination: &str) -> Message {
    Message::method_call(PATH, "Notify")
        .interface(NAME)
        .destination(destination)
        .sender(sender)
        .serial(NonZeroU32::new(serial).unwrap())
        .build(&(
            app,
            0u32,
            "",
            "Title",
            "Body",
            Vec::<String>::new(),
            BTreeMap::<String, OwnedVariant>::new(),
            0i32,
        ))
        .unwrap()
}
#[test]
fn monitor_reply_requires_authenticated_owner_and_caller_not_only_serial() {
    let mut decoder = Decoder::default();
    let a = call(":1.11", 7, "Caller A");
    decoder.observe(&a, ":1.9").unwrap();
    let wrong = Message::method_return(&a.header())
        .sender(":1.9")
        .destination(":1.12")
        .build(&41u32)
        .unwrap();
    assert!(matches!(
        decoder.observe(&wrong, ":1.9").unwrap(),
        Observed::Ignored
    ));
    let forged = Message::method_return(&a.header())
        .sender(":1.8")
        .build(&42u32)
        .unwrap();
    assert!(matches!(
        decoder.observe(&forged, ":1.9").unwrap(),
        Observed::Ignored
    ));
    let correct = Message::method_return(&a.header())
        .sender(":1.9")
        .build(&43u32)
        .unwrap();
    match decoder.observe(&correct, ":1.9").unwrap() {
        Observed::Notification(n) => {
            assert_eq!(n.input.app_name, "Caller A");
            assert_eq!(n.input.server_id, Some(43));
        }
        _ => panic!("missing authenticated reply"),
    }
}
#[test]
fn monitor_pending_lru_overflow_and_timeout_are_bounded() {
    let mut decoder = Decoder::default();
    for n in 1..=257 {
        decoder
            .observe(&call(":1.11", n, "Bounded"), ":1.9")
            .unwrap();
    }
    assert_eq!(decoder.overflow, 1);
    assert_eq!(decoder.expire(Instant::now() + Duration::from_secs(6)), 256);
    let original = call(":1.11", 257, "Bounded");
    let late = Message::method_return(&original.header())
        .sender(":1.9")
        .build(&1u32)
        .unwrap();
    assert!(matches!(
        decoder.observe(&late, ":1.9").unwrap(),
        Observed::Ignored
    ));
}

#[test]
fn monitor_signals_require_current_owner_for_closed_and_action() {
    let mut decoder = Decoder::default();
    for sender in [":1.8", ":1.9"] {
        let closed = Message::signal(PATH, NAME, "NotificationClosed")
            .sender(sender)
            .build(&(41u32, 2u32))
            .unwrap();
        let action = Message::signal(PATH, NAME, "ActionInvoked")
            .sender(sender)
            .build(&(41u32, "default"))
            .unwrap();
        if sender == ":1.9" {
            assert!(matches!(
                decoder.observe(&closed, ":1.9").unwrap(),
                Observed::Closed(41)
            ));
            assert!(matches!(
                decoder.observe(&action, ":1.9").unwrap(),
                Observed::Action
            ));
        } else {
            assert!(matches!(
                decoder.observe(&closed, ":1.9").unwrap(),
                Observed::Ignored
            ));
            assert!(matches!(
                decoder.observe(&action, ":1.9").unwrap(),
                Observed::Ignored
            ));
        }
    }
}

#[test]
fn monitor_notify_destination_must_be_well_known_or_current_unique_owner() {
    for destination in [":1.12", NAME, ":1.9"] {
        let call = call_to(":1.11", 7, "Destination fixture", destination);
        let reply = Message::method_return(&call.header())
            .sender(":1.9")
            .build(&41u32)
            .unwrap();
        // Independently test reply correlation BEFORE expiry, and pending accounting
        // in a fresh decoder. Expiring first would hide a wrong-destination reply bug.
        let mut decoder = Decoder::default();
        assert!(matches!(
            decoder.observe(&call, ":1.9").unwrap(),
            Observed::Ignored
        ));
        let observed = decoder.observe(&reply, ":1.9").unwrap();
        if destination == ":1.12" {
            assert!(matches!(observed, Observed::Ignored));
        } else {
            assert!(matches!(observed, Observed::Notification(n) if n.input.server_id == Some(41)));
        }
        let mut expiry = Decoder::default();
        expiry.observe(&call, ":1.9").unwrap();
        assert_eq!(
            expiry.expire(Instant::now() + Duration::from_secs(6)),
            u32::from(destination != ":1.12"),
            "wrong destination must never occupy the pending LRU"
        );
        assert!(matches!(
            expiry.observe(&reply, ":1.9").unwrap(),
            Observed::Ignored
        ));
    }
}
