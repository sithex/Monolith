//! Phase 6a: the power service against a MOCK UPower on a private, scratch-only bus.
//! Never touches the machine's real system or session bus.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::private_bus;

use hermes_island_lib::integrations::power::{model::AlertKind, AlertSettings, PowerService};
use private_bus::{wait_until, Mock, PrivateBus};
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

const DISPLAY: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const BAT0: &str = "/org/freedesktop/UPower/devices/battery_BAT0";
const ROOT: &str = "/org/freedesktop/UPower";
const DEVICE: &str = "org.freedesktop.UPower.Device";
const T: Duration = Duration::from_secs(10);

struct Rig {
    svc: Arc<PowerService>,
    fired: Arc<Mutex<Vec<AlertKind>>>,
    bus: PrivateBus,
    mock: Option<Mock>,
}

fn rig(tag: &str) -> Rig {
    let bus = PrivateBus::start(tag);
    let mock = Mock::start(&bus.env(), &bus.root, "upower");
    let fired = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&fired);
    let svc = PowerService::new(
        AlertSettings {
            enabled: true,
            low: 20.0,
            critical: 10.0,
        },
        Arc::new(move |kind, _| sink.lock().unwrap().push(kind)),
    );
    svc.start(Some(bus.address()));
    Rig {
        svc,
        fired,
        bus,
        mock: Some(mock),
    }
}

impl Rig {
    fn mock(&mut self) -> &mut Mock {
        self.mock.as_mut().unwrap()
    }
    fn wait_percent(&self, what: &str, percent: Option<f64>) {
        wait_until(what, T, || {
            (self.svc.current().percent == percent).then_some(())
        });
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.svc.shutdown();
        let _ = &self.bus;
    }
}

#[test]
fn power_initial_snapshot_property_change_event_and_alerts() {
    let mut r = rig("power-basic");
    wait_until("initial snapshot from UPower", T, || {
        r.svc.current().available.then_some(())
    });
    let first = r.svc.current();
    assert_eq!(first.percent, Some(55.0));
    assert_eq!(first.state, "discharging");
    assert_eq!(first.on_battery, Some(true));
    assert_eq!(first.time_to_empty, Some(7200.0));
    assert_eq!(
        first.time_to_full, None,
        "TimeToFull 0 means unknown, not 0"
    );
    assert!(first.present && !first.charging);
    let status = r.svc.status();
    assert!(status.available, "{status:?}");
    assert_eq!((status.detail.as_str(), status.connects), ("upower", 1));

    // A PropertiesChanged signal reaches the latest-value topic (what the event forwards).
    let mut topic = r.svc.topic.subscribe();
    topic.borrow_and_update();
    let before = r.svc.current().rev;
    r.mock().send(
        json!({"op":"props","path":DISPLAY,"iface":DEVICE,"changed":{"Percentage":["d",19.0]}}),
    );
    r.wait_percent("19% reaches the topic", Some(19.0));
    assert!(r.svc.current().rev > before);
    assert_eq!(topic.borrow().as_ref().unwrap().percent, Some(19.0));
    r.mock().send(
        json!({"op":"props","path":DISPLAY,"iface":DEVICE,"changed":{"Percentage":["d",9.0]}}),
    );
    r.wait_percent("9% reaches the topic", Some(9.0));
    r.mock().send(json!({"op":"props","path":ROOT,"iface":"org.freedesktop.UPower","changed":{"OnBattery":["b",false]}}));
    wait_until("OnBattery false", T, || {
        (r.svc.current().on_battery == Some(false)).then_some(())
    });
    r.mock().send(json!({"op":"props","path":DISPLAY,"iface":DEVICE,"changed":{"State":["u",1],"TimeToFull":["x",600]}}));
    wait_until("charging with time to full", T, || {
        let s = r.svc.current();
        (s.charging && s.time_to_full == Some(600.0)).then_some(())
    });
    assert_eq!(
        *r.fired.lock().unwrap(),
        vec![AlertKind::Low, AlertKind::Critical],
        "one Low and one Critical alert for the two crossings"
    );
}

#[test]
fn power_name_lost_reports_unavailable_then_restart_recovers_without_restarting_the_service() {
    let mut r = rig("power-name");
    wait_until("first availability", T, || {
        r.svc.current().available.then_some(())
    });
    r.mock.take().unwrap().kill();
    wait_until("name lost is reported", T, || {
        let s = r.svc.status();
        (!s.available && s.detail == "name-lost").then_some(())
    });
    let lost = r.svc.current();
    assert!(!lost.available && !lost.present);
    assert_eq!(
        (lost.percent, lost.on_battery, lost.time_to_empty),
        (None, None, None)
    );
    // Restart the daemon: the SAME service instance recovers by itself.
    r.mock = Some(Mock::start(&r.bus.env(), &r.bus.root, "upower"));
    wait_until("recovery after restart", T, || {
        let s = r.svc.status();
        (s.available && s.connects >= 2).then_some(())
    });
    assert_eq!(r.svc.current().percent, Some(55.0));
    assert!(r.svc.current().available);
}

#[test]
fn power_ignores_signals_from_senders_that_are_not_the_upower_owner() {
    let mut r = rig("power-sender");
    wait_until("availability", T, || {
        r.svc.current().available.then_some(())
    });
    let base = r.svc.current();
    // A DIFFERENT bus connection (not UPower's owner) forges DisplayDevice and root properties;
    // a genuine DisplayDevice change afterwards is the sentinel that proves ordering.
    r.mock().send(json!({"op":"rogue_props","path":DISPLAY,"iface":DEVICE,"changed":{"Percentage":["d",1.0]}}));
    r.mock().send(json!({"op":"rogue_props","path":ROOT,"iface":"org.freedesktop.UPower","changed":{"OnBattery":["b",false]}}));
    r.mock().send(
        json!({"op":"props","path":DISPLAY,"iface":DEVICE,"changed":{"Percentage":["d",60.0]}}),
    );
    r.wait_percent("sentinel 60%", Some(60.0));
    let after = r.svc.current();
    assert_eq!(
        after.rev,
        base.rev + 1,
        "forged signals produced no revision"
    );
    assert_eq!(after.on_battery, Some(true), "rogue OnBattery ignored");
    assert!(
        r.fired.lock().unwrap().is_empty(),
        "a forged 1% must never raise a battery alert"
    );
}

#[test]
fn power_ignores_the_owners_signals_for_other_object_paths() {
    let mut r = rig("power-path");
    wait_until("availability", T, || {
        r.svc.current().available.then_some(())
    });
    let base = r.svc.current();
    // The REAL owner reports a different battery (not the DisplayDevice) and a Device-interface
    // change on the root object; neither is modeled state.
    r.mock()
        .send(json!({"op":"props","path":BAT0,"iface":DEVICE,"changed":{"Percentage":["d",5.0]}}));
    r.mock().send(
        json!({"op":"emit_props","path":ROOT,"iface":DEVICE,"changed":{"Percentage":["d",4.0]}}),
    );
    r.mock().send(
        json!({"op":"props","path":DISPLAY,"iface":DEVICE,"changed":{"Percentage":["d",60.0]}}),
    );
    r.wait_percent("sentinel 60%", Some(60.0));
    assert_eq!(
        r.svc.current().rev,
        base.rev + 1,
        "other object paths produced no revision"
    );
    assert!(
        r.fired.lock().unwrap().is_empty(),
        "a foreign 5% battery must never raise a battery alert"
    );
}

#[test]
fn power_malformed_values_never_become_fabricated_numbers() {
    let mut r = rig("power-bad");
    wait_until("availability", T, || {
        r.svc.current().available.then_some(())
    });
    r.mock().send(json!({"op":"emit_props","path":DISPLAY,"iface":DEVICE,"changed":{
        "Percentage":["s","lots"],"State":["u",99],"TimeToEmpty":["x",-5],"IsPresent":["s","yes"]}}));
    wait_until("malformed values cleared", T, || {
        let s = r.svc.current();
        (s.percent.is_none() && !s.present).then_some(())
    });
    let s = r.svc.current();
    assert!(
        s.available,
        "the daemon is fine; only the values are unknown"
    );
    assert_eq!((s.state.as_str(), s.time_to_empty), ("unknown", None));
    r.mock().send(json!({"op":"emit_props","path":DISPLAY,"iface":DEVICE,"changed":{"Percentage":["d",250.0]}}));
    r.mock().send(json!({"op":"emit_props","path":DISPLAY,"iface":DEVICE,"changed":{"Percentage":["d",42.0],"IsPresent":["b",true]}}));
    r.wait_percent("valid value accepted again", Some(42.0));
    assert!(r.fired.lock().unwrap().is_empty());
}

#[test]
fn power_without_a_configured_system_bus_stays_unavailable_and_idle() {
    let svc = PowerService::new(
        AlertSettings {
            enabled: true,
            low: 20.0,
            critical: 10.0,
        },
        Arc::new(|_, _| {}),
    );
    svc.start(None);
    wait_until("explicit reason", T, || {
        (svc.status().detail == "no-system-bus-configured").then_some(())
    });
    assert!(!svc.status().available && !svc.current().available);
    assert_eq!(svc.status().connects, 0);
    svc.shutdown();
}

#[test]
fn power_ignores_a_spoofed_name_owner_changed_from_a_rogue_peer() {
    let mut r = rig("power-noc");
    wait_until("availability", T, || {
        r.svc.current().available.then_some(())
    });
    let base = r.svc.current();
    // A rogue peer unicasts NameOwnerChanged("org.freedesktop.UPower", _, "") straight to the
    // island's connection (a well-known sender cannot be client-filtered by zbus). Only the
    // in-code sender check keeps the service available.
    r.mock()
        .send(json!({"op":"rogue_noc","name":"org.freedesktop.UPower","new":""}));
    r.mock()
        .send(json!({"op":"rogue_noc","name":"org.freedesktop.UPower","new":""}));
    r.mock().send(
        json!({"op":"props","path":DISPLAY,"iface":DEVICE,"changed":{"Percentage":["d",60.0]}}),
    );
    r.wait_percent("sentinel 60%", Some(60.0));
    std::thread::sleep(Duration::from_millis(400));
    let status = r.svc.status();
    assert!(
        status.available && status.detail == "upower" && status.connects == 1,
        "spoofed owner loss must not be believed: {status:?}"
    );
    let after = r.svc.current();
    assert!(after.available && after.present, "{after:?}");
    assert_eq!(
        after.rev,
        base.rev + 1,
        "only the genuine change was applied"
    );
}
