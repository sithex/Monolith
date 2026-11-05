//! Phase 6a: the Bluetooth service against a MOCK BlueZ on a private, scratch-only bus.
//! Read only; nothing here (or in the service) connects, pairs or powers a device.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::private_bus;

use hermes_island_lib::{contract::system::BtDevice, integrations::bluetooth::BluetoothService};
use private_bus::{wait_until, Mock, PrivateBus};
use serde_json::json;
use std::{sync::Arc, time::Duration};

const DEVICE: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";
const MOUSE: &str = "/org/bluez/hci0/dev_11_22_33_44_55_66";
const NEW: &str = "/org/bluez/hci0/dev_DE_AD_BE_EF_00_01";
const T: Duration = Duration::from_secs(10);

struct Rig {
    svc: Arc<BluetoothService>,
    bus: PrivateBus,
    mock: Option<Mock>,
}

fn rig(tag: &str) -> Rig {
    let bus = PrivateBus::start(tag);
    let mock = Mock::start(&bus.env(), &bus.root, "bluez");
    let svc = BluetoothService::new();
    svc.start(Some(bus.address()));
    Rig {
        svc,
        bus,
        mock: Some(mock),
    }
}

impl Rig {
    fn mock(&mut self) -> &mut Mock {
        self.mock.as_mut().unwrap()
    }
    fn device(&self, address: &str) -> Option<BtDevice> {
        self.svc
            .current()
            .devices
            .into_iter()
            .find(|d| d.address == address)
    }
    fn ready(&self) {
        wait_until("BlueZ snapshot", T, || {
            (self.svc.status().available && self.svc.current().devices.len() == 2).then_some(())
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
fn bluetooth_initial_snapshot_property_changes_added_and_removed_reach_the_topic() {
    let mut r = rig("bt-basic");
    r.ready();
    let status = r.svc.status();
    assert_eq!((status.detail.as_str(), status.connects), ("bluez", 1));
    let snapshot = r.svc.current();
    // Connected first, then by name: the audited core's order.
    assert_eq!(snapshot.devices[0].name, "Mock Headset");
    let headset = r.device("AA:BB:CC:DD:EE:FF").unwrap();
    assert_eq!(headset.icon.as_deref(), Some("audio-headset"));
    assert_eq!(headset.battery_percent, Some(80));
    assert!(headset.connected && headset.paired);
    assert_eq!(headset.path, DEVICE);
    let mouse = r.device("11:22:33:44:55:66").unwrap();
    assert_eq!(
        (mouse.icon, mouse.battery_percent, mouse.connected),
        (None, None, false),
        "absent values stay null, never defaulted"
    );

    let mut topic = r.svc.topic.subscribe();
    topic.borrow_and_update();
    r.mock().send(json!({"op":"props","path":DEVICE,"iface":"org.bluez.Battery1","changed":{"Percentage":["y",65]}}));
    wait_until("battery 65", T, || {
        (r.device("AA:BB:CC:DD:EE:FF")?.battery_percent == Some(65)).then_some(())
    });
    assert!(topic.has_changed().unwrap());
    r.mock().send(json!({"op":"props","path":MOUSE,"iface":"org.bluez.Device1","changed":{"Connected":["b",true]}}));
    wait_until("mouse connected", T, || {
        r.device("11:22:33:44:55:66")?.connected.then_some(())
    });
    r.mock().send(json!({"op":"added","path":NEW,"ifaces":{"org.bluez.Device1":{
        "Address":["s","DE:AD:BE:EF:00:01"],"Alias":["s","New Pad"],"Paired":["b",false],"Connected":["b",false]}}}));
    wait_until("added device", T, || {
        r.device("DE:AD:BE:EF:00:01").map(|_| ())
    });
    assert_eq!(r.device("DE:AD:BE:EF:00:01").unwrap().name, "New Pad");
    r.mock()
        .send(json!({"op":"removed","path":NEW,"ifaces":["org.bluez.Device1"]}));
    wait_until("removed device", T, || {
        r.device("DE:AD:BE:EF:00:01").is_none().then_some(())
    });
    assert_eq!(r.svc.current().devices.len(), 2);
}

#[test]
fn bluetooth_name_lost_reports_unavailable_then_restart_recovers_without_restarting_the_service() {
    let mut r = rig("bt-name");
    r.ready();
    r.mock.take().unwrap().kill();
    wait_until("name lost is reported", T, || {
        let s = r.svc.status();
        (!s.available && s.detail == "name-lost").then_some(())
    });
    assert!(
        r.svc.current().devices.is_empty(),
        "no stale device survives its daemon"
    );
    r.mock = Some(Mock::start(&r.bus.env(), &r.bus.root, "bluez"));
    wait_until("recovery after restart", T, || {
        let s = r.svc.status();
        (s.available && s.connects >= 2 && r.svc.current().devices.len() == 2).then_some(())
    });
}

#[test]
fn bluetooth_ignores_signals_from_senders_that_are_not_the_bluez_owner() {
    let mut r = rig("bt-sender");
    r.ready();
    let base = r.svc.current();
    // A different bus connection forges a battery value, a property change and a new device;
    // a legitimate battery change afterwards is the ordering sentinel.
    r.mock().send(json!({"op":"rogue_props","path":DEVICE,"iface":"org.bluez.Battery1","changed":{"Percentage":["y",1]}}));
    r.mock().send(json!({"op":"rogue_props","path":MOUSE,"iface":"org.bluez.Device1","changed":{"Connected":["b",true]}}));
    r.mock().send(json!({"op":"rogue_added","path":NEW,"ifaces":{"org.bluez.Device1":{"Address":["s","66:66:66:66:66:66"],"Alias":["s","Forged"]}}}));
    r.mock().send(json!({"op":"props","path":DEVICE,"iface":"org.bluez.Battery1","changed":{"Percentage":["y",70]}}));
    wait_until("sentinel battery 70", T, || {
        (r.device("AA:BB:CC:DD:EE:FF")?.battery_percent == Some(70)).then_some(())
    });
    let after = r.svc.current();
    assert_eq!(
        after.rev,
        base.rev + 1,
        "forged signals produced no revision"
    );
    assert_eq!(after.devices.len(), 2, "{:?}", after.devices);
    assert!(!r.device("11:22:33:44:55:66").unwrap().connected);
}

#[test]
fn bluetooth_ignores_objects_outside_the_org_bluez_tree() {
    let mut r = rig("bt-path");
    r.ready();
    let base = r.svc.current();
    // The REAL owner announces a device under a foreign object-path prefix. BlueZ objects live
    // under /org/bluez, so it is not ours; the sentinel proves the signals were processed.
    r.mock().send(json!({"op":"added","path":"/com/evil/hci0/dev_66_66_66_66_66_67","ifaces":{"org.bluez.Device1":{"Address":["s","66:66:66:66:66:67"],"Alias":["s","Foreign path"]}}}));
    r.mock().send(json!({"op":"props","path":DEVICE,"iface":"org.bluez.Battery1","changed":{"Percentage":["y",70]}}));
    wait_until("sentinel battery 70", T, || {
        (r.device("AA:BB:CC:DD:EE:FF")?.battery_percent == Some(70)).then_some(())
    });
    let after = r.svc.current();
    assert_eq!(after.rev, base.rev + 1, "{:?}", after.devices);
    assert_eq!(after.devices.len(), 2, "{:?}", after.devices);
}

#[test]
fn bluetooth_object_manager_signals_are_accepted_only_from_the_root_object() {
    let mut r = rig("bt-root");
    r.ready();
    let base = r.svc.current();
    // The real owner emits InterfacesAdded from a NON-root object (not the ObjectManager).
    r.mock().send(json!({"op":"added","signal_path":"/org/bluez/hci0","path":"/org/bluez/hci0/dev_66_66_66_66_66_68","ifaces":{"org.bluez.Device1":{"Address":["s","66:66:66:66:66:68"],"Alias":["s","Wrong emitter"]}}}));
    r.mock().send(json!({"op":"props","path":DEVICE,"iface":"org.bluez.Battery1","changed":{"Percentage":["y",70]}}));
    wait_until("sentinel battery 70", T, || {
        (r.device("AA:BB:CC:DD:EE:FF")?.battery_percent == Some(70)).then_some(())
    });
    let after = r.svc.current();
    assert_eq!(after.rev, base.rev + 1, "{:?}", after.devices);
    assert_eq!(after.devices.len(), 2, "{:?}", after.devices);
}

#[test]
fn bluetooth_malformed_values_never_become_fabricated_values() {
    let mut r = rig("bt-bad");
    r.ready();
    r.mock().send(json!({"op":"emit_props","path":DEVICE,"iface":"org.bluez.Battery1","changed":{"Percentage":["q",250]}}));
    wait_until("out-of-range battery cleared", T, || {
        r.device("AA:BB:CC:DD:EE:FF")?
            .battery_percent
            .is_none()
            .then_some(())
    });
    r.mock().send(json!({"op":"emit_props","path":DEVICE,"iface":"org.bluez.Device1","changed":{"Connected":["s","yes"],"Icon":["u",7]}}));
    wait_until("wrongly typed values cleared", T, || {
        let d = r.device("AA:BB:CC:DD:EE:FF")?;
        (!d.connected && d.icon.is_none()).then_some(())
    });
    assert!(r.svc.status().available);
}

#[test]
fn bluetooth_without_a_configured_system_bus_stays_unavailable_and_idle() {
    let svc = BluetoothService::new();
    svc.start(None);
    wait_until("explicit reason", T, || {
        (svc.status().detail == "no-system-bus-configured").then_some(())
    });
    assert!(!svc.status().available && svc.current().devices.is_empty());
    assert_eq!(svc.status().connects, 0);
    svc.shutdown();
}

#[test]
fn bluetooth_ignores_a_spoofed_name_owner_changed_from_a_rogue_peer() {
    let mut r = rig("bt-noc");
    r.ready();
    let base = r.svc.current();
    r.mock()
        .send(json!({"op":"rogue_noc","name":"org.bluez","new":""}));
    r.mock()
        .send(json!({"op":"rogue_noc","name":"org.bluez","new":""}));
    r.mock().send(json!({"op":"props","path":DEVICE,"iface":"org.bluez.Battery1","changed":{"Percentage":["y",70]}}));
    wait_until("sentinel battery 70", T, || {
        (r.device("AA:BB:CC:DD:EE:FF")?.battery_percent == Some(70)).then_some(())
    });
    std::thread::sleep(Duration::from_millis(400));
    let status = r.svc.status();
    assert!(
        status.available && status.detail == "bluez" && status.connects == 1,
        "spoofed owner loss must not be believed: {status:?}"
    );
    let after = r.svc.current();
    assert_eq!(after.devices.len(), 2, "no device was dropped");
    assert_eq!(after.rev, base.rev + 1, "only the genuine change applied");
}

#[test]
fn bluetooth_interfaces_removed_is_accepted_only_from_the_root_object() {
    let mut r = rig("bt-removed-root");
    r.ready();
    let base = r.svc.current();
    // The REAL owner emits InterfacesRemoved for the connected headset from a NON-root path.
    r.mock().send(json!({"op":"removed","signal_path":"/org/bluez/hci0","path":DEVICE,"ifaces":["org.bluez.Device1"]}));
    r.mock().send(json!({"op":"props","path":MOUSE,"iface":"org.bluez.Device1","changed":{"Connected":["b",true]}}));
    wait_until("sentinel mouse connected", T, || {
        r.device("11:22:33:44:55:66")?.connected.then_some(())
    });
    let after = r.svc.current();
    assert_eq!(after.rev, base.rev + 1, "{:?}", after.devices);
    assert_eq!(after.devices.len(), 2, "{:?}", after.devices);
    assert!(r.device("AA:BB:CC:DD:EE:FF").is_some());
}
