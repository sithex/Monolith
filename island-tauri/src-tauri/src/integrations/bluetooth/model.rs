//! Pure core behind SPEC §3.5's "Bluetooth" row and §0.5's quick toggles:
//! BlueZ D-Bus values (already converted to `serde_json::Value`) drive a
//! [`BtModel`] that turns `GetManagedObjects` snapshots and `InterfacesAdded`
//! / `InterfacesRemoved` / `PropertiesChanged` signals into a device/adapter
//! model. Platform-free: no IO, no network, no threads, no clock reading, no
//! `unsafe`, and never panics on any input.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

const IFACE_DEVICE: &str = "org.bluez.Device1";
const IFACE_BATTERY: &str = "org.bluez.Battery1";
const IFACE_ADAPTER: &str = "org.bluez.Adapter1";

/// Validate a D-Bus object path. Accepts only D-Bus-valid syntax: starts
/// with '/', every segment is non-empty and made solely of
/// `[A-Za-z0-9_]`, and there is no trailing '/' except on the root path
/// "/".
pub fn valid_object_path(path: &str) -> bool {
    if !path.starts_with('/') {
        return false;
    }
    if path == "/" {
        return true;
    }
    if path.ends_with('/') || path.contains("//") {
        return false;
    }
    path[1..]
        .split('/')
        .all(|seg| !seg.is_empty() && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// Read a JSON string; any other type yields `None`.
fn string_or_none(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Read a JSON boolean; any other type yields `false`.
fn bool_or_false(value: &Value) -> bool {
    matches!(value, Value::Bool(true))
}

/// `org.bluez.Battery1.Percentage` (`y` byte, 0..=100). Exactly the integers
/// 0..=100 are accepted; anything else (101+, negatives, floats, strings,
/// bools, null) yields `None`.
fn parse_battery(value: &Value) -> Option<u8> {
    let n = value.as_u64()?;
    (n <= 100).then_some(n as u8)
}

/// `org.bluez.Device1.RSSI` (`n` int16). Exactly the integers representable
/// in `i16` are accepted; anything else yields `None`.
fn parse_rssi(value: &Value) -> Option<i16> {
    i16::try_from(value.as_i64()?).ok()
}

/// Internal per-path record. A path may hold device state (from
/// `org.bluez.Device1`), battery state (`org.bluez.Battery1`, same path) or
/// both. A record with no valid `address` is hidden from [`BtModel::devices`].
#[derive(Debug, Clone, Default)]
struct DeviceRecord {
    address: Option<String>,
    name: Option<String>,
    alias: Option<String>,
    icon: Option<String>,
    paired: bool,
    bonded: bool,
    connected: bool,
    trusted: bool,
    blocked: bool,
    adapter: Option<String>,
    rssi: Option<i16>,
    services_resolved: bool,
    uuids: Option<Vec<String>>,
    battery_percent: Option<u8>,
}

/// Apply one `org.bluez.Device1` property. Pass `value = None` when the
/// property was invalidated: the field is cleared to its absent value
/// (`None` / `false`) rather than keeping stale data.
fn apply_device_key(rec: &mut DeviceRecord, key: &str, value: Option<&Value>) {
    match key {
        "Address" => rec.address = value.and_then(string_or_none),
        "Name" => rec.name = value.and_then(string_or_none),
        "Alias" => rec.alias = value.and_then(string_or_none),
        "Icon" => rec.icon = value.and_then(string_or_none),
        "Paired" => rec.paired = value.is_some_and(bool_or_false),
        "Bonded" => rec.bonded = value.is_some_and(bool_or_false),
        "Connected" => rec.connected = value.is_some_and(bool_or_false),
        "Trusted" => rec.trusted = value.is_some_and(bool_or_false),
        "Blocked" => rec.blocked = value.is_some_and(bool_or_false),
        "Adapter" => rec.adapter = value.and_then(string_or_none),
        "RSSI" => rec.rssi = value.and_then(parse_rssi),
        "ServicesResolved" => rec.services_resolved = value.is_some_and(bool_or_false),
        "UUIDs" => {
            rec.uuids = match value {
                Some(Value::Array(items)) if items.iter().all(Value::is_string) => Some(
                    items
                        .iter()
                        .filter_map(|i| i.as_str().map(str::to_string))
                        .collect(),
                ),
                _ => None,
            }
        }
        _ => {}
    }
}

/// Apply one `org.bluez.Battery1` property (only `Percentage` is modeled).
fn apply_battery_key(rec: &mut DeviceRecord, key: &str, value: Option<&Value>) {
    if key == "Percentage" {
        rec.battery_percent = value.and_then(parse_battery);
    }
}

/// Internal per-path adapter record.
#[derive(Debug, Clone, Default)]
struct AdapterRecord {
    alias: String,
    powered: bool,
    discovering: bool,
}

/// Apply one `org.bluez.Adapter1` property.
fn apply_adapter_key(rec: &mut AdapterRecord, key: &str, value: Option<&Value>) {
    match key {
        "Powered" => rec.powered = value.is_some_and(bool_or_false),
        "Discovering" => rec.discovering = value.is_some_and(bool_or_false),
        "Alias" => rec.alias = value.and_then(string_or_none).unwrap_or_default(),
        _ => {}
    }
}

/// The displayed name: Alias when non-blank, else Name when non-blank, else
/// the Address. "Blank" means empty or whitespace-only after trimming.
fn display_name(rec: &DeviceRecord) -> String {
    if let Some(alias) = &rec.alias {
        if !alias.trim().is_empty() {
            return alias.clone();
        }
    }
    if let Some(name) = &rec.name {
        if !name.trim().is_empty() {
            return name.clone();
        }
    }
    rec.address.clone().unwrap_or_default()
}

/// Errors from [`BtModel::load_managed_objects`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtLoadError {
    /// The value passed was not a JSON object.
    NotAnObject,
}

/// A device as exposed to the Tauri side. Serde wires camelCase field names
/// (`displayName`, `batteryPercent`, ...) and accepts no aliases.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BtDevice {
    /// D-Bus object path, e.g. `/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF`.
    pub path: String,
    /// `org.bluez.Device1.Address` (always present for a visible device).
    pub address: String,
    /// Alias, else Name, else Address.
    pub display_name: String,
    /// `org.bluez.Device1.Icon`, or `None`.
    pub icon: Option<String>,
    /// `org.bluez.Device1.Paired`.
    pub paired: bool,
    /// `org.bluez.Device1.Connected`.
    pub connected: bool,
    /// `org.bluez.Device1.Trusted`.
    pub trusted: bool,
    /// `org.bluez.Device1.Blocked`.
    pub blocked: bool,
    /// `org.bluez.Battery1.Percentage` (0..=100), or `None`.
    pub battery_percent: Option<u8>,
    /// `org.bluez.Device1.RSSI` (int16), or `None`.
    pub rssi: Option<i16>,
    /// `org.bluez.Device1.Adapter` object path, or `None`.
    pub adapter: Option<String>,
}

/// An adapter as exposed to the Tauri side. Serde wires camelCase field
/// names and accepts no aliases.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BtAdapter {
    /// D-Bus object path, e.g. `/org/bluez/hci0`.
    pub path: String,
    /// `org.bluez.Adapter1.Alias` (empty string when absent/invalid).
    pub alias: String,
    /// `org.bluez.Adapter1.Powered`.
    pub powered: bool,
    /// `org.bluez.Adapter1.Discovering`.
    pub discovering: bool,
}

/// Summary feeding the compact Bluetooth pill. Serde wires camelCase field
/// names and accepts no aliases.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BtSummary {
    /// Whether at least one adapter is powered.
    pub powered_any: bool,
    /// Number of connected devices.
    pub connected_count: usize,
    /// Display names of the connected devices, at most 3, in device order.
    pub connected_names: Vec<String>,
    /// Lowest battery percentage over connected devices, or `None`.
    pub lowest_battery_percent: Option<u8>,
}

/// The BlueZ device/adapter model. Starts empty. Incremental signals
/// (`interfaces_added`, `interfaces_removed`, `properties_changed`) mutate
/// it; [`Self::load_managed_objects`] replaces it wholesale.
#[derive(Debug, Clone, Default)]
pub struct BtModel {
    devices: HashMap<String, DeviceRecord>,
    adapters: HashMap<String, AdapterRecord>,
    ignored: u64,
}

impl BtModel {
    /// Replace the whole model from a `GetManagedObjects` reply: a JSON
    /// object `{ path: { interface: { prop: value } } }`. Returns a typed
    /// error (model unchanged) when the top-level value is not an object.
    /// Invalid path keys, malformed interface maps and devices without a
    /// valid `Address` are skipped and counted in [`Self::ignored_count`].
    pub fn load_managed_objects(&mut self, v: &Value) -> Result<(), BtLoadError> {
        let obj = match v {
            Value::Object(o) => o,
            _ => return Err(BtLoadError::NotAnObject),
        };

        let mut devices: HashMap<String, DeviceRecord> = HashMap::new();
        let mut adapters: HashMap<String, AdapterRecord> = HashMap::new();
        let mut ignored: u64 = 0;

        for (path, ifaces) in obj {
            if !valid_object_path(path) {
                ignored += 1;
                continue;
            }
            let ifaces_obj = match ifaces {
                Value::Object(o) => o,
                _ => {
                    ignored += 1;
                    continue;
                }
            };

            if ifaces_obj.contains_key(IFACE_DEVICE) {
                match ifaces_obj.get(IFACE_DEVICE) {
                    Some(Value::Object(props)) => {
                        let mut rec = DeviceRecord::default();
                        for (key, value) in props {
                            apply_device_key(&mut rec, key, Some(value));
                        }
                        if let Some(Value::Object(battery_props)) = ifaces_obj.get(IFACE_BATTERY) {
                            for (key, value) in battery_props {
                                apply_battery_key(&mut rec, key, Some(value));
                            }
                        }
                        if rec.address.is_some() {
                            devices.insert(path.clone(), rec);
                        } else {
                            ignored += 1;
                        }
                    }
                    _ => ignored += 1,
                }
            } else if ifaces_obj.contains_key(IFACE_BATTERY) {
                match ifaces_obj.get(IFACE_BATTERY) {
                    Some(Value::Object(battery_props)) => {
                        let mut rec = DeviceRecord::default();
                        for (key, value) in battery_props {
                            apply_battery_key(&mut rec, key, Some(value));
                        }
                        devices.insert(path.clone(), rec);
                    }
                    _ => ignored += 1,
                }
            }

            if ifaces_obj.contains_key(IFACE_ADAPTER) {
                match ifaces_obj.get(IFACE_ADAPTER) {
                    Some(Value::Object(adapter_props)) => {
                        let mut rec = AdapterRecord::default();
                        for (key, value) in adapter_props {
                            apply_adapter_key(&mut rec, key, Some(value));
                        }
                        adapters.insert(path.clone(), rec);
                    }
                    _ => ignored += 1,
                }
            }
        }

        self.devices = devices;
        self.adapters = adapters;
        self.ignored = ignored;
        Ok(())
    }

    /// Apply an `InterfacesAdded` signal: `ifaces` is a JSON object
    /// `{ interface: { prop: value } }`. Unrecognised interfaces are ignored
    /// without counting; an invalid path, a non-object `ifaces`, or a
    /// `Device1` that ends up without a valid address counts in
    /// [`Self::ignored_count`].
    pub fn interfaces_added(&mut self, path: &str, ifaces: &Value) {
        if !valid_object_path(path) {
            self.ignored += 1;
            return;
        }
        let obj = match ifaces {
            Value::Object(o) => o,
            _ => {
                self.ignored += 1;
                return;
            }
        };

        if let Some(Value::Object(props)) = obj.get(IFACE_DEVICE) {
            let rec = self.devices.entry(path.to_string()).or_default();
            for (key, value) in props {
                apply_device_key(rec, key, Some(value));
            }
            if rec.address.is_none() {
                self.ignored += 1;
            }
        }

        if let Some(Value::Object(props)) = obj.get(IFACE_BATTERY) {
            let rec = self.devices.entry(path.to_string()).or_default();
            for (key, value) in props {
                apply_battery_key(rec, key, Some(value));
            }
        }

        if let Some(Value::Object(props)) = obj.get(IFACE_ADAPTER) {
            let rec = self.adapters.entry(path.to_string()).or_default();
            for (key, value) in props {
                apply_adapter_key(rec, key, Some(value));
            }
        }
    }

    /// Apply an `InterfacesRemoved` signal. Removing `Device1` removes the
    /// whole device record even if `Battery1` remains; removing only
    /// `Battery1` clears the battery to `None`. An invalid or unknown path
    /// is ignored and counted in [`Self::ignored_count`].
    pub fn interfaces_removed(&mut self, path: &str, names: &[String]) {
        if !valid_object_path(path) {
            self.ignored += 1;
            return;
        }
        let has_device = self.devices.contains_key(path);
        let has_adapter = self.adapters.contains_key(path);
        if !has_device && !has_adapter {
            self.ignored += 1;
            return;
        }

        let mut removed_device = false;
        let mut removed_battery = false;
        let mut removed_adapter = false;
        for name in names {
            match name.as_str() {
                IFACE_DEVICE => removed_device = true,
                IFACE_BATTERY => removed_battery = true,
                IFACE_ADAPTER => removed_adapter = true,
                _ => {}
            }
        }

        if has_device {
            if removed_device {
                self.devices.remove(path);
            } else if removed_battery {
                if let Some(rec) = self.devices.get_mut(path) {
                    rec.battery_percent = None;
                }
            }
        }
        if has_adapter && removed_adapter {
            self.adapters.remove(path);
        }
    }

    /// Apply a `PropertiesChanged` signal. Only keys present in `changed`
    /// change; a key with an invalid-typed value, or a key listed in
    /// `invalidated`, is cleared to its absent value (`None` / `false`)
    /// rather than keeping stale data. Changes for an unknown path or an
    /// unknown interface are ignored and counted in
    /// [`Self::ignored_count`].
    pub fn properties_changed(
        &mut self,
        path: &str,
        interface: &str,
        changed: &Map<String, Value>,
        invalidated: &[String],
    ) {
        match interface {
            IFACE_DEVICE => match self.devices.get_mut(path) {
                Some(rec) => {
                    for (key, value) in changed {
                        apply_device_key(rec, key, Some(value));
                    }
                    for key in invalidated {
                        apply_device_key(rec, key, None);
                    }
                }
                None => self.ignored += 1,
            },
            IFACE_BATTERY => match self.devices.get_mut(path) {
                Some(rec) => {
                    for (key, value) in changed {
                        apply_battery_key(rec, key, Some(value));
                    }
                    for key in invalidated {
                        apply_battery_key(rec, key, None);
                    }
                }
                None => self.ignored += 1,
            },
            IFACE_ADAPTER => match self.adapters.get_mut(path) {
                Some(rec) => {
                    for (key, value) in changed {
                        apply_adapter_key(rec, key, Some(value));
                    }
                    for key in invalidated {
                        apply_adapter_key(rec, key, None);
                    }
                }
                None => self.ignored += 1,
            },
            _ => self.ignored += 1,
        }
    }

    /// The devices, sorted: connected first, then paired, then displayName
    /// case-insensitively, then path. Records without a valid address are
    /// excluded.
    pub fn devices(&self) -> Vec<BtDevice> {
        let mut out = Vec::with_capacity(self.devices.len());
        for (path, rec) in &self.devices {
            let Some(address) = rec.address.as_ref() else {
                continue;
            };
            out.push(BtDevice {
                path: path.clone(),
                address: address.clone(),
                display_name: display_name(rec),
                icon: rec.icon.clone(),
                paired: rec.paired,
                connected: rec.connected,
                trusted: rec.trusted,
                blocked: rec.blocked,
                battery_percent: rec.battery_percent,
                rssi: rec.rssi,
                adapter: rec.adapter.clone(),
            });
        }
        out.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| b.paired.cmp(&a.paired))
                .then_with(|| {
                    a.display_name
                        .to_lowercase()
                        .cmp(&b.display_name.to_lowercase())
                })
                .then_with(|| a.path.cmp(&b.path))
        });
        out
    }

    /// The adapters, sorted by path.
    pub fn adapters(&self) -> Vec<BtAdapter> {
        let mut out: Vec<BtAdapter> = self
            .adapters
            .iter()
            .map(|(path, rec)| BtAdapter {
                path: path.clone(),
                alias: rec.alias.clone(),
                powered: rec.powered,
                discovering: rec.discovering,
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// The default adapter: the first powered adapter (in path order), else
    /// the first adapter, else `None`.
    pub fn default_adapter(&self) -> Option<BtAdapter> {
        let adapters = self.adapters();
        adapters
            .iter()
            .find(|a| a.powered)
            .cloned()
            .or_else(|| adapters.first().cloned())
    }

    /// The compact pill summary, consistent with [`Self::devices`] and
    /// [`Self::adapters`].
    pub fn summary(&self) -> BtSummary {
        let devices = self.devices();
        let adapters = self.adapters();
        let connected: Vec<&BtDevice> = devices.iter().filter(|d| d.connected).collect();
        BtSummary {
            powered_any: adapters.iter().any(|a| a.powered),
            connected_count: connected.len(),
            connected_names: connected
                .iter()
                .take(3)
                .map(|d| d.display_name.clone())
                .collect(),
            lowest_battery_percent: connected.iter().filter_map(|d| d.battery_percent).min(),
        }
    }

    /// Number of input items that were ignored or dropped: invalid path
    /// keys, malformed maps, devices without a valid address, and changes
    /// for unknown paths/interfaces.
    pub fn ignored_count(&self) -> u64 {
        self.ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_path_validation() {
        for valid in [
            "/",
            "/a",
            "/a/b",
            "/org/bluez/hci0",
            "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF",
            "/A_1/x2",
        ] {
            assert!(valid_object_path(valid), "should be valid: {valid}");
        }
        for invalid in [
            "", "dev", "/a//b", "/a/", "/a-b", "/a b", "a/b", "/a.", "//a", "/a/b/", "/a/b-",
        ] {
            assert!(!valid_object_path(invalid), "should be invalid: {invalid}");
        }
    }

    #[test]
    fn battery_parse_boundaries() {
        assert_eq!(parse_battery(&json!(0)), Some(0));
        assert_eq!(parse_battery(&json!(100)), Some(100));
        assert_eq!(parse_battery(&json!(101)), None);
        assert_eq!(parse_battery(&json!(-1)), None);
        assert_eq!(parse_battery(&json!(255)), None);
        assert_eq!(parse_battery(&json!("50")), None);
        assert_eq!(parse_battery(&json!(50.0)), None);
        assert_eq!(parse_battery(&json!(true)), None);
        assert_eq!(parse_battery(&json!(null)), None);
    }

    #[test]
    fn rssi_parse_boundaries() {
        assert_eq!(parse_rssi(&json!(-32768)), Some(-32768));
        assert_eq!(parse_rssi(&json!(32767)), Some(32767));
        assert_eq!(parse_rssi(&json!(0)), Some(0));
        assert_eq!(parse_rssi(&json!(32768)), None);
        assert_eq!(parse_rssi(&json!(-32769)), None);
        assert_eq!(parse_rssi(&json!(40000)), None);
        assert_eq!(parse_rssi(&json!("5")), None);
        assert_eq!(parse_rssi(&json!(5.0)), None);
        assert_eq!(parse_rssi(&json!(null)), None);
    }

    #[test]
    fn display_name_fallback_direct() {
        let base = DeviceRecord {
            address: Some("AA:BB:CC:DD:EE:FF".to_string()),
            ..DeviceRecord::default()
        };
        assert_eq!(display_name(&base), "AA:BB:CC:DD:EE:FF");

        let mut r = base.clone();
        r.name = Some("Kbd".to_string());
        assert_eq!(display_name(&r), "Kbd");

        r.alias = Some(String::new());
        assert_eq!(display_name(&r), "Kbd");

        r.alias = Some("   ".to_string());
        assert_eq!(display_name(&r), "Kbd");

        r.alias = Some("Keyboard".to_string());
        assert_eq!(display_name(&r), "Keyboard");

        r.alias = None;
        r.name = Some("   ".to_string());
        assert_eq!(display_name(&r), "AA:BB:CC:DD:EE:FF");

        r.name = None;
        assert_eq!(display_name(&r), "AA:BB:CC:DD:EE:FF");
    }
}
