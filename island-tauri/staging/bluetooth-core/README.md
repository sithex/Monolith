# bluetooth-core

Pure Rust core behind SPEC §3.5's "Bluetooth" row and §0.5's quick
toggles for the Hermes Island widget. BlueZ D-Bus values
(`org.freedesktop.DBus.ObjectManager.GetManagedObjects`, plus
`InterfacesAdded`, `InterfacesRemoved` and `PropertiesChanged`) arrive already
converted to `serde_json::Value`; nothing in this crate does IO, network,
threads or clock reads.

No IO, no network, no threads, no `unsafe`, never panics on any input.

## API

- `BtModel` — `load_managed_objects(&Value) -> Result<(), BtLoadError>`
  (full replacement; non-object top-level → typed error, model unchanged),
  `interfaces_added(path, &Value)`, `interfaces_removed(path, &[String])`,
  `properties_changed(path, interface, &Map, &[String])`,
  `devices() -> Vec<BtDevice>`, `adapters() -> Vec<BtAdapter>`,
  `default_adapter() -> Option<BtAdapter>`, `summary() -> BtSummary`,
  `ignored_count() -> u64`.
- `BtDevice { path, address, displayName, icon, paired, connected, trusted,
  blocked, batteryPercent, rssi, adapter }` — devices without a valid
  `Address` are skipped; ordering is connected first, then paired, then
  displayName case-insensitively, then path. `displayName` = Alias if
  non-blank, else Name if non-blank, else Address.
- `BtAdapter { path, alias, powered, discovering }` — sorted by path.
- `BtSummary { poweredAny, connectedCount, connectedNames (max 3, in device
  order), lowestBatteryPercent (over connected devices) }` — feeds the
  compact pill.
- `BtLoadError::NotAnObject`.

Semantics are documented on the items in `src/lib.rs`; the gate output and
the mutation record live in `VERIFICATION.md`.
