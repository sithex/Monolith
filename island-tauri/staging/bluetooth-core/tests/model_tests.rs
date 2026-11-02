use bluetooth_core::{BtLoadError, BtModel};
use serde_json::{json, Map, Value};

const DEV_PATH: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";
const ADDR: &str = "AA:BB:CC:DD:EE:FF";
const ADAPTER_PATH: &str = "/org/bluez/hci0";
const IFACE_DEVICE: &str = "org.bluez.Device1";
const IFACE_BATTERY: &str = "org.bluez.Battery1";
const IFACE_ADAPTER: &str = "org.bluez.Adapter1";

fn seed_device(m: &mut BtModel, path: &str, addr: &str) {
    let ifaces = json!({ IFACE_DEVICE: { "Address": addr } });
    m.interfaces_added(path, &ifaces);
}

#[test]
fn starts_empty() {
    let m = BtModel::default();
    assert!(m.devices().is_empty());
    assert!(m.adapters().is_empty());
    assert_eq!(m.default_adapter(), None);
    assert_eq!(m.ignored_count(), 0);
    let s = m.summary();
    assert!(!s.powered_any);
    assert_eq!(s.connected_count, 0);
    assert!(s.connected_names.is_empty());
    assert_eq!(s.lowest_battery_percent, None);
}

#[test]
fn load_full_blob() {
    let blob = json!({
        ADAPTER_PATH: { IFACE_ADAPTER: { "Powered": true, "Discovering": false, "Alias": "Main" } },
        DEV_PATH: {
            IFACE_DEVICE: {
                "Address": ADDR,
                "Name": "Kbd",
                "Alias": "Keyboard",
                "Icon": "input-keyboard",
                "Paired": true,
                "Bonded": true,
                "Connected": true,
                "Trusted": false,
                "Blocked": false,
                "Adapter": ADAPTER_PATH,
                "RSSI": -45,
                "ServicesResolved": true,
                "UUIDs": ["00001124-0000-1000-8000-00805f9b34fb"]
            },
            IFACE_BATTERY: { "Percentage": 82 }
        }
    });
    let mut m = BtModel::default();
    assert!(m.load_managed_objects(&blob).is_ok());

    let devices = m.devices();
    assert_eq!(devices.len(), 1);
    let d = &devices[0];
    assert_eq!(d.path, DEV_PATH);
    assert_eq!(d.address, ADDR);
    assert_eq!(d.display_name, "Keyboard");
    assert_eq!(d.icon.as_deref(), Some("input-keyboard"));
    assert!(d.paired);
    assert!(d.connected);
    assert!(!d.trusted);
    assert!(!d.blocked);
    assert_eq!(d.battery_percent, Some(82));
    assert_eq!(d.rssi, Some(-45));
    assert_eq!(d.adapter.as_deref(), Some(ADAPTER_PATH));

    let adapters = m.adapters();
    assert_eq!(adapters.len(), 1);
    assert_eq!(adapters[0].path, ADAPTER_PATH);
    assert_eq!(adapters[0].alias, "Main");
    assert!(adapters[0].powered);
    assert!(!adapters[0].discovering);

    let d = m.default_adapter().unwrap();
    assert_eq!(d.path, ADAPTER_PATH);
    assert!(d.powered);

    let s = m.summary();
    assert!(s.powered_any);
    assert_eq!(s.connected_count, 1);
    assert_eq!(s.connected_names, vec!["Keyboard".to_string()]);
    assert_eq!(s.lowest_battery_percent, Some(82));
}

#[test]
fn devices_without_valid_address_skipped_and_counted() {
    let blob = json!({
        "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF": { IFACE_DEVICE: { "Address": ADDR } },
        "/org/bluez/hci0/dev_NO_ADDR": { IFACE_DEVICE: { "Name": "NoAddr" } },
        "/org/bluez/hci0/dev_NUM_ADDR": { IFACE_DEVICE: { "Address": 12345 } }
    });
    let mut m = BtModel::default();
    assert!(m.load_managed_objects(&blob).is_ok());
    assert_eq!(m.devices().len(), 1);
    assert_eq!(m.devices()[0].address, ADDR);
    assert_eq!(m.ignored_count(), 2);
}

#[test]
fn load_replaces_old_state_and_resets_ignored() {
    let mut m = BtModel::default();
    let blob1 = json!({
        DEV_PATH: { IFACE_DEVICE: { "Address": ADDR, "Connected": true } },
        ADAPTER_PATH: { IFACE_ADAPTER: { "Powered": true } }
    });
    assert!(m.load_managed_objects(&blob1).is_ok());
    assert_eq!(m.devices().len(), 1);
    assert_eq!(m.ignored_count(), 0);

    let mut changed = Map::new();
    changed.insert("Connected".to_string(), json!(true));
    m.properties_changed("/org/bluez/hci0/dev_UNKNOWN", IFACE_DEVICE, &changed, &[]);
    assert_eq!(m.ignored_count(), 1);

    let blob2 = json!({
        "/org/bluez/hci1/dev_11_22_33_44_55_66": {
            IFACE_DEVICE: { "Address": "11:22:33:44:55:66" }
        }
    });
    assert!(m.load_managed_objects(&blob2).is_ok());
    assert_eq!(m.devices().len(), 1);
    assert_eq!(m.devices()[0].address, "11:22:33:44:55:66");
    assert!(m.adapters().is_empty());
    assert_eq!(m.ignored_count(), 0);
}

#[test]
fn load_non_object_returns_error_and_leaves_model_unchanged() {
    let mut m = BtModel::default();
    seed_device(&mut m, DEV_PATH, ADDR);
    m.interfaces_added(ADAPTER_PATH, &json!({ IFACE_ADAPTER: { "Powered": true } }));

    assert_eq!(
        m.load_managed_objects(&json!([1, 2])),
        Err(BtLoadError::NotAnObject)
    );
    assert_eq!(
        m.load_managed_objects(&json!("x")),
        Err(BtLoadError::NotAnObject)
    );
    assert_eq!(
        m.load_managed_objects(&json!(42)),
        Err(BtLoadError::NotAnObject)
    );
    assert_eq!(m.devices().len(), 1);
    assert_eq!(m.adapters().len(), 1);
    assert_eq!(m.ignored_count(), 0);
}

#[test]
fn invalid_paths_rejected_in_load() {
    let blob = json!({
        "dev": { IFACE_ADAPTER: { "Powered": true } },
        "/a//b": { IFACE_DEVICE: { "Address": ADDR } },
        "/a/": { IFACE_DEVICE: { "Address": ADDR } },
        "/a-b": { IFACE_DEVICE: { "Address": ADDR } },
        ADAPTER_PATH: { IFACE_ADAPTER: { "Powered": true } }
    });
    let mut m = BtModel::default();
    assert!(m.load_managed_objects(&blob).is_ok());
    assert_eq!(m.ignored_count(), 4);
    assert_eq!(m.adapters().len(), 1);
    assert_eq!(m.adapters()[0].path, ADAPTER_PATH);
    assert!(m.devices().is_empty());
}

#[test]
fn display_name_alias_blank_falls_back_to_name() {
    let mut m = BtModel::default();
    let ifaces = json!({
        IFACE_DEVICE: { "Address": ADDR, "Name": "Kbd", "Alias": "" }
    });
    m.interfaces_added(DEV_PATH, &ifaces);
    assert_eq!(m.devices()[0].display_name, "Kbd");

    let mut changed = Map::new();
    changed.insert("Alias".to_string(), json!("   "));
    m.properties_changed(DEV_PATH, IFACE_DEVICE, &changed, &[]);
    assert_eq!(m.devices()[0].display_name, "Kbd");
}

#[test]
fn battery_percent_boundaries() {
    let cases: [(&Value, Option<u8>); 8] = [
        (&json!(100), Some(100)),
        (&json!(0), Some(0)),
        (&json!(101), None),
        (&json!(255), None),
        (&json!(-1), None),
        (&json!("50"), None),
        (&json!(50.0), None),
        (&json!(null), None),
    ];
    for (pct, want) in cases {
        let mut m = BtModel::default();
        let ifaces = json!({
            IFACE_DEVICE: { "Address": ADDR },
            IFACE_BATTERY: { "Percentage": pct }
        });
        m.interfaces_added(DEV_PATH, &ifaces);
        let devices = m.devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].battery_percent, want, "pct={pct}");
    }
}

#[test]
fn rssi_boundaries() {
    let cases: [(&Value, Option<i16>); 7] = [
        (&json!(-32768), Some(-32768)),
        (&json!(32767), Some(32767)),
        (&json!(0), Some(0)),
        (&json!(40000), None),
        (&json!(-40000), None),
        (&json!("5"), None),
        (&json!(5.0), None),
    ];
    for (rssi, want) in cases {
        let mut m = BtModel::default();
        let ifaces = json!({ IFACE_DEVICE: { "Address": ADDR, "RSSI": rssi } });
        m.interfaces_added(DEV_PATH, &ifaces);
        assert_eq!(m.devices()[0].rssi, want, "rssi={rssi}");
    }
}

#[test]
fn properties_changed_only_present_keys_change() {
    let mut m = BtModel::default();
    let ifaces = json!({
        IFACE_DEVICE: { "Address": ADDR, "Connected": true, "Trusted": true }
    });
    m.interfaces_added(DEV_PATH, &ifaces);

    let mut changed = Map::new();
    changed.insert("Connected".to_string(), json!(false));
    m.properties_changed(DEV_PATH, IFACE_DEVICE, &changed, &[]);

    let d = &m.devices()[0];
    assert!(!d.connected);
    assert!(d.trusted);
    assert_eq!(d.address, ADDR);
}

#[test]
fn properties_changed_invalid_clears_field_not_stale() {
    let mut m = BtModel::default();
    let ifaces = json!({
        IFACE_DEVICE: {
            "Address": ADDR, "Connected": true, "Paired": true, "RSSI": -40,
            "Alias": "KB", "Adapter": ADAPTER_PATH
        },
        IFACE_BATTERY: { "Percentage": 77 }
    });
    m.interfaces_added(DEV_PATH, &ifaces);

    let mut changed = Map::new();
    changed.insert("Connected".to_string(), json!("yes"));
    changed.insert("Paired".to_string(), json!(1));
    changed.insert("RSSI".to_string(), json!("x"));
    changed.insert("Alias".to_string(), json!(null));
    changed.insert("Adapter".to_string(), json!(false));
    changed.insert("Junk".to_string(), json!(1));
    m.properties_changed(DEV_PATH, IFACE_DEVICE, &changed, &[]);

    let d = &m.devices()[0];
    assert!(!d.connected);
    assert!(!d.paired);
    assert_eq!(d.rssi, None);
    assert_eq!(d.display_name, ADDR);
    assert_eq!(d.adapter, None);
    assert_eq!(d.battery_percent, Some(77));

    let mut bchanged = Map::new();
    bchanged.insert("Percentage".to_string(), json!(false));
    m.properties_changed(DEV_PATH, IFACE_BATTERY, &bchanged, &[]);
    assert_eq!(m.devices()[0].battery_percent, None);

    let mut b2 = Map::new();
    b2.insert("Percentage".to_string(), json!(60));
    m.properties_changed(DEV_PATH, IFACE_BATTERY, &b2, &[]);
    assert_eq!(m.devices()[0].battery_percent, Some(60));

    m.properties_changed(
        DEV_PATH,
        IFACE_BATTERY,
        &Map::new(),
        &["Percentage".to_string()],
    );
    assert_eq!(m.devices()[0].battery_percent, None);
}

#[test]
fn invalidated_connected_clears_to_false() {
    let mut m = BtModel::default();
    let ifaces = json!({ IFACE_DEVICE: { "Address": ADDR, "Connected": true } });
    m.interfaces_added(DEV_PATH, &ifaces);
    assert!(m.devices()[0].connected);

    m.properties_changed(
        DEV_PATH,
        IFACE_DEVICE,
        &Map::new(),
        &["Connected".to_string()],
    );
    assert!(!m.devices()[0].connected);
}

#[test]
fn removing_battery1_keeps_device() {
    let mut m = BtModel::default();
    let ifaces = json!({
        IFACE_DEVICE: { "Address": ADDR },
        IFACE_BATTERY: { "Percentage": 77 }
    });
    m.interfaces_added(DEV_PATH, &ifaces);
    assert_eq!(m.devices()[0].battery_percent, Some(77));

    m.interfaces_removed(DEV_PATH, &[IFACE_BATTERY.to_string()]);
    assert_eq!(m.devices().len(), 1);
    assert_eq!(m.devices()[0].battery_percent, None);
    assert_eq!(m.devices()[0].address, ADDR);
}

#[test]
fn removing_device1_removes_device_even_if_battery_remains() {
    let mut m = BtModel::default();
    let ifaces = json!({
        IFACE_DEVICE: { "Address": ADDR },
        IFACE_BATTERY: { "Percentage": 77 }
    });
    m.interfaces_added(DEV_PATH, &ifaces);
    assert_eq!(m.devices().len(), 1);

    m.interfaces_removed(DEV_PATH, &[IFACE_DEVICE.to_string()]);
    assert!(m.devices().is_empty());

    m.interfaces_added(DEV_PATH, &ifaces);
    m.interfaces_removed(
        DEV_PATH,
        &[IFACE_DEVICE.to_string(), IFACE_BATTERY.to_string()],
    );
    assert!(m.devices().is_empty());
}

#[test]
fn unknown_path_and_interface_changes_ignored_and_counted() {
    let mut m = BtModel::default();
    seed_device(&mut m, DEV_PATH, ADDR);
    m.interfaces_added(ADAPTER_PATH, &json!({ IFACE_ADAPTER: { "Powered": true } }));
    assert_eq!(m.ignored_count(), 0);

    let mut changed = Map::new();
    changed.insert("Connected".to_string(), json!(true));

    m.properties_changed(
        "/org/bluez/hci0/dev_ZZ_YY_XX_WW_VV_UU",
        IFACE_DEVICE,
        &changed,
        &[],
    );
    assert_eq!(m.ignored_count(), 1);

    m.properties_changed(DEV_PATH, "org.bluez.Media1", &changed, &[]);
    assert_eq!(m.ignored_count(), 2);

    m.properties_changed(DEV_PATH, IFACE_ADAPTER, &changed, &[]);
    assert_eq!(m.ignored_count(), 3);

    m.properties_changed("not/a/path", IFACE_DEVICE, &changed, &[]);
    assert_eq!(m.ignored_count(), 4);

    assert!(!m.devices()[0].connected);
    assert_eq!(m.adapters()[0].path, ADAPTER_PATH);
    assert!(m.adapters()[0].powered);
}

#[test]
fn unknown_property_keys_are_not_counted() {
    let mut m = BtModel::default();
    seed_device(&mut m, DEV_PATH, ADDR);
    let mut changed = Map::new();
    changed.insert("Junk".to_string(), json!(1));
    changed.insert("Nope".to_string(), json!("x"));
    m.properties_changed(DEV_PATH, IFACE_DEVICE, &changed, &[]);
    assert_eq!(m.ignored_count(), 0);
    assert_eq!(m.devices()[0].address, ADDR);
}

#[test]
fn interfaces_added_before_adapter_exists() {
    let mut m = BtModel::default();
    let ifaces = json!({ IFACE_DEVICE: { "Address": ADDR, "Adapter": ADAPTER_PATH } });
    m.interfaces_added(DEV_PATH, &ifaces);
    assert_eq!(m.devices().len(), 1);
    assert_eq!(m.devices()[0].adapter.as_deref(), Some(ADAPTER_PATH));
    assert!(m.adapters().is_empty());
    assert_eq!(m.default_adapter(), None);

    m.interfaces_added(
        ADAPTER_PATH,
        &json!({ IFACE_ADAPTER: { "Powered": true, "Alias": "Main" } }),
    );
    assert_eq!(m.adapters().len(), 1);
    assert_eq!(m.devices().len(), 1);
    let a = m.default_adapter().unwrap();
    assert_eq!(a.path, ADAPTER_PATH);
    assert!(a.powered);
    assert_eq!(a.alias, "Main");
}

#[test]
fn interfaces_added_non_object_or_invalid_path_counted() {
    let mut m = BtModel::default();
    m.interfaces_added("dev", &json!({}));
    assert_eq!(m.ignored_count(), 1);
    m.interfaces_added(DEV_PATH, &json!(42));
    assert_eq!(m.ignored_count(), 2);
    m.interfaces_added(DEV_PATH, &json!({ "junk": 1 }));
    assert_eq!(m.ignored_count(), 2);
    assert!(m.devices().is_empty());
}

#[test]
fn interfaces_added_skips_addressless_device_and_counts() {
    let mut m = BtModel::default();
    m.interfaces_added(DEV_PATH, &json!({ IFACE_DEVICE: { "Name": "NoAddr" } }));
    assert_eq!(m.ignored_count(), 1);
    assert!(m.devices().is_empty());

    m.interfaces_added(DEV_PATH, &json!({ IFACE_DEVICE: { "Address": ADDR } }));
    assert_eq!(m.ignored_count(), 1);
    assert_eq!(m.devices().len(), 1);
}

#[test]
fn interfaces_removed_unknown_or_invalid_path_counted() {
    let mut m = BtModel::default();
    m.interfaces_removed(DEV_PATH, &[IFACE_DEVICE.to_string()]);
    assert_eq!(m.ignored_count(), 1);
    m.interfaces_removed("bad/path", &[IFACE_DEVICE.to_string()]);
    assert_eq!(m.ignored_count(), 2);
}

#[test]
fn ordering_connected_paired_name_path() {
    let mut m = BtModel::default();
    let mut add = |path: &str, addr: &str, name: &str, connected: bool, paired: bool| {
        let ifaces = json!({
            IFACE_DEVICE: {
                "Address": addr, "Name": name, "Connected": connected, "Paired": paired
            }
        });
        m.interfaces_added(path, &ifaces);
    };
    add(
        "/org/bluez/hci0/dev_A",
        "AA:00:00:00:00:01",
        "banana",
        true,
        true,
    );
    add(
        "/org/bluez/hci0/dev_B",
        "AA:00:00:00:00:02",
        "Apple",
        true,
        false,
    );
    add(
        "/org/bluez/hci0/dev_C",
        "AA:00:00:00:00:03",
        "apple",
        false,
        true,
    );
    add(
        "/org/bluez/hci0/dev_E",
        "AA:00:00:00:00:05",
        "apple",
        false,
        false,
    );
    add(
        "/org/bluez/hci0/dev_D",
        "AA:00:00:00:00:04",
        "Cherry",
        false,
        false,
    );

    let devices = m.devices();
    let paths: Vec<&str> = devices.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "/org/bluez/hci0/dev_A",
            "/org/bluez/hci0/dev_B",
            "/org/bluez/hci0/dev_C",
            "/org/bluez/hci0/dev_E",
            "/org/bluez/hci0/dev_D",
        ]
    );
}

#[test]
fn summary_connected_names_capped_at_three() {
    let mut m = BtModel::default();
    for i in 0..5 {
        let path = format!("/org/bluez/hci0/dev_{i}");
        let addr = format!("AA:00:00:00:00:{i:02X}");
        let ifaces = json!({
            IFACE_DEVICE: { "Address": addr, "Name": format!("Dev{i}"), "Connected": true }
        });
        m.interfaces_added(&path, &ifaces);
    }
    let s = m.summary();
    assert_eq!(s.connected_count, 5);
    assert_eq!(s.connected_names.len(), 3);
    assert_eq!(
        s.connected_names,
        vec!["Dev0".to_string(), "Dev1".to_string(), "Dev2".to_string()]
    );
}

#[test]
fn summary_lowest_battery_over_connected() {
    let blob = json!({
        "/org/bluez/hci0/dev_01": {
            IFACE_DEVICE: { "Address": "AA:00:00:00:00:01", "Connected": true },
            IFACE_BATTERY: { "Percentage": 80 }
        },
        "/org/bluez/hci0/dev_02": {
            IFACE_DEVICE: { "Address": "AA:00:00:00:00:02", "Connected": true },
            IFACE_BATTERY: { "Percentage": 55 }
        },
        "/org/bluez/hci0/dev_03": {
            IFACE_DEVICE: { "Address": "AA:00:00:00:00:03", "Connected": true }
        },
        "/org/bluez/hci0/dev_04": {
            IFACE_DEVICE: { "Address": "AA:00:00:00:00:04", "Connected": false },
            IFACE_BATTERY: { "Percentage": 10 }
        }
    });
    let mut m = BtModel::default();
    assert!(m.load_managed_objects(&blob).is_ok());
    assert_eq!(m.summary().lowest_battery_percent, Some(55));

    let mut changed = Map::new();
    changed.insert("Percentage".to_string(), json!(12));
    m.properties_changed("/org/bluez/hci0/dev_01", IFACE_BATTERY, &changed, &[]);
    assert_eq!(m.summary().lowest_battery_percent, Some(12));

    m.interfaces_removed("/org/bluez/hci0/dev_02", &[IFACE_BATTERY.to_string()]);
    assert_eq!(m.summary().lowest_battery_percent, Some(12));

    m.properties_changed(
        "/org/bluez/hci0/dev_01",
        IFACE_BATTERY,
        &Map::new(),
        &["Percentage".to_string()],
    );
    assert_eq!(m.summary().lowest_battery_percent, None);
}

#[test]
fn default_adapter_selection() {
    let mut m = BtModel::default();
    assert_eq!(m.default_adapter(), None);

    m.interfaces_added(
        "/org/bluez/hci0",
        &json!({ IFACE_ADAPTER: { "Alias": "Zero" } }),
    );
    m.interfaces_added(
        "/org/bluez/hci1",
        &json!({ IFACE_ADAPTER: { "Alias": "One" } }),
    );
    assert_eq!(m.default_adapter().unwrap().path, "/org/bluez/hci0");

    m.interfaces_added(
        "/org/bluez/hci1",
        &json!({ IFACE_ADAPTER: { "Powered": true } }),
    );
    assert_eq!(m.default_adapter().unwrap().path, "/org/bluez/hci1");

    m.interfaces_added(
        "/org/bluez/hci0",
        &json!({ IFACE_ADAPTER: { "Powered": true } }),
    );
    assert_eq!(m.default_adapter().unwrap().path, "/org/bluez/hci0");
}

#[test]
fn adapter_props_changed_and_invalid_clear() {
    let mut m = BtModel::default();
    m.interfaces_added(
        ADAPTER_PATH,
        &json!({ IFACE_ADAPTER: { "Powered": true, "Alias": "Main" } }),
    );
    let mut changed = Map::new();
    changed.insert("Alias".to_string(), json!(123));
    changed.insert("Powered".to_string(), json!("yes"));
    changed.insert("Discovering".to_string(), json!(true));
    m.properties_changed(ADAPTER_PATH, IFACE_ADAPTER, &changed, &[]);
    let a = &m.adapters()[0];
    assert_eq!(a.alias, "");
    assert!(!a.powered);
    assert!(a.discovering);
}

#[test]
fn serde_camelcase_roundtrip_and_no_alias() {
    let mut m = BtModel::default();
    let ifaces = json!({
        IFACE_DEVICE: {
            "Address": ADDR, "Name": "Kbd", "Alias": "Keyboard", "Icon": "input-keyboard",
            "Paired": true, "Connected": true, "Trusted": true, "Blocked": false,
            "RSSI": -45, "Adapter": ADAPTER_PATH
        },
        IFACE_BATTERY: { "Percentage": 82 }
    });
    m.interfaces_added(DEV_PATH, &ifaces);
    m.interfaces_added(
        ADAPTER_PATH,
        &json!({ IFACE_ADAPTER: { "Powered": true, "Discovering": false, "Alias": "Main" } }),
    );

    let d = m.devices()[0].clone();
    let v = serde_json::to_value(&d).unwrap();
    let obj = v.as_object().unwrap();
    for k in [
        "path",
        "address",
        "displayName",
        "icon",
        "paired",
        "connected",
        "trusted",
        "blocked",
        "batteryPercent",
        "rssi",
        "adapter",
    ] {
        assert!(obj.contains_key(k), "missing camelCase key {k}");
    }
    assert_eq!(obj.len(), 11);
    let d2: bluetooth_core::BtDevice = serde_json::from_value(v).unwrap();
    assert_eq!(d, d2);

    let from_snake: bluetooth_core::BtDevice = serde_json::from_value(json!({
        "display_name": "hax", "battery_percent": 99
    }))
    .unwrap();
    assert_eq!(from_snake.display_name, "");
    assert_eq!(from_snake.battery_percent, None);
    assert_eq!(from_snake.address, "");

    let a = m.adapters()[0].clone();
    let av = serde_json::to_value(&a).unwrap();
    let aobj = av.as_object().unwrap();
    for k in ["path", "alias", "powered", "discovering"] {
        assert!(aobj.contains_key(k), "missing key {k}");
    }
    let a2: bluetooth_core::BtAdapter = serde_json::from_value(av).unwrap();
    assert_eq!(a, a2);

    let s = m.summary();
    let sv = serde_json::to_value(&s).unwrap();
    let sobj = sv.as_object().unwrap();
    for k in [
        "poweredAny",
        "connectedCount",
        "connectedNames",
        "lowestBatteryPercent",
    ] {
        assert!(sobj.contains_key(k), "missing key {k}");
    }
    let s2: bluetooth_core::BtSummary = serde_json::from_value(sv).unwrap();
    assert_eq!(s, s2);
    let from_snake_summary: bluetooth_core::BtSummary = serde_json::from_value(json!({
        "powered_any": true, "connected_count": 7
    }))
    .unwrap();
    assert!(!from_snake_summary.powered_any);
    assert_eq!(from_snake_summary.connected_count, 0);
}
