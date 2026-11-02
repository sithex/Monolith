//! Bluetooth device service (SPEC §3.5 "Bluetooth").
//!
//! `model.rs` is the audited pure core ported unchanged from `staging/bluetooth-core`. This
//! module owns the mutable model, the revisioned `BluetoothChanged` topic and the availability
//! report; `bluez.rs` feeds it from `org.bluez` (ObjectManager + Device1/Battery1/Adapter1).
//! Read only: nothing here connects, disconnects, pairs or powers anything.

#[cfg(target_os = "linux")]
mod bluez;
pub mod model;

use crate::{
    bus::Latest,
    contract::system::{BluetoothChanged, BtDevice, ServiceStatus},
    integrations::service_status::Reporter,
};
use model::BtModel;
use serde_json::{Map, Value};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::watch;

struct Shared {
    model: BtModel,
    /// `org.bluez` currently has an owner and the model is loaded from it.
    attached: bool,
    current: BluetoothChanged,
}

pub struct BluetoothService {
    pub status: Arc<Reporter>,
    pub topic: Latest<BluetoothChanged>,
    shared: Mutex<Shared>,
    stop: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

fn convert(device: model::BtDevice) -> BtDevice {
    BtDevice {
        address: device.address,
        name: device.display_name,
        icon: device.icon,
        connected: device.connected,
        battery_percent: device.battery_percent,
        path: device.path,
        paired: device.paired,
    }
}

impl BluetoothService {
    pub fn new() -> Arc<Self> {
        let current = BluetoothChanged {
            rev: 0,
            devices: Vec::new(),
        };
        let topic = Latest::new();
        topic.publish(current.clone());
        Arc::new(Self {
            status: Reporter::new("bluetooth"),
            topic,
            shared: Mutex::new(Shared {
                model: BtModel::default(),
                attached: false,
                current,
            }),
            stop: watch::channel(false).0,
            task: Mutex::new(None),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        match self.shared.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn current(&self) -> BluetoothChanged {
        self.lock().current.clone()
    }

    /// Replace the model from a `GetManagedObjects` JSON object (already path-filtered).
    pub(crate) fn load(&self, objects: &Value) -> bool {
        let mut shared = self.lock();
        let ok = shared.model.load_managed_objects(objects).is_ok();
        shared.attached = ok;
        self.publish(&mut shared);
        ok
    }

    pub(crate) fn interfaces_added(&self, path: &str, interfaces: &Value) {
        let mut shared = self.lock();
        shared.model.interfaces_added(path, interfaces);
        self.publish(&mut shared);
    }

    pub(crate) fn interfaces_removed(&self, path: &str, names: &[String]) {
        let mut shared = self.lock();
        shared.model.interfaces_removed(path, names);
        self.publish(&mut shared);
    }

    pub(crate) fn properties_changed(
        &self,
        path: &str,
        interface: &str,
        changed: &Map<String, Value>,
        invalidated: &[String],
    ) {
        let mut shared = self.lock();
        shared
            .model
            .properties_changed(path, interface, changed, invalidated);
        self.publish(&mut shared);
    }

    /// `org.bluez` lost its owner: every device disappears (nothing is kept stale).
    pub(crate) fn lost(&self) {
        let mut shared = self.lock();
        shared.model = BtModel::default();
        shared.attached = false;
        self.publish(&mut shared);
    }

    fn publish(&self, shared: &mut Shared) {
        let devices: Vec<BtDevice> = shared.model.devices().into_iter().map(convert).collect();
        if devices != shared.current.devices {
            let next = BluetoothChanged {
                rev: shared.current.rev.wrapping_add(1),
                devices,
            };
            shared.current = next.clone();
            self.topic.publish(next);
        }
        // Availability follows the daemon and its adapters, with a reason.
        if !shared.attached {
            return;
        }
        if shared.model.adapters().is_empty() {
            self.status.unavailable("no-adapter");
        } else {
            self.status.available("bluez");
        }
    }

    pub fn start(self: &Arc<Self>, address: Option<String>) {
        let service = Arc::clone(self);
        let stopped = self.stop.subscribe();
        let task = tauri::async_runtime::spawn(async move {
            #[cfg(target_os = "linux")]
            bluez::run(service, address, stopped).await;
            #[cfg(not(target_os = "linux"))]
            {
                let _ = (address, stopped);
                service.status.unavailable("unsupported-platform");
            }
        });
        if let Ok(mut slot) = self.task.lock() {
            *slot = Some(task);
        }
    }

    pub fn shutdown(&self) {
        self.stop.send_replace(true);
        let task = self.task.lock().ok().and_then(|mut slot| slot.take());
        if let Some(task) = task {
            let _ = tauri::async_runtime::block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(3), task).await
            });
        }
    }

    pub fn status(&self) -> ServiceStatus {
        self.status.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DEV: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";

    fn managed() -> Value {
        json!({
            "/org/bluez/hci0": {"org.bluez.Adapter1": {"Powered": true, "Alias": "pc"}},
            DEV: {
                "org.bluez.Device1": {"Address": "AA:BB:CC:DD:EE:FF", "Alias": "Headset", "Connected": true, "Paired": true},
                "org.bluez.Battery1": {"Percentage": 70}
            }
        })
    }

    #[test]
    fn load_maps_names_battery_and_revisions_only_real_changes() {
        let svc = BluetoothService::new();
        assert!(svc.load(&managed()));
        let now = svc.current();
        assert_eq!(now.rev, 1);
        assert_eq!(now.devices.len(), 1);
        assert_eq!(now.devices[0].name, "Headset");
        assert_eq!(now.devices[0].battery_percent, Some(70));
        assert_eq!(now.devices[0].icon, None, "absent icon stays null");
        assert!(svc.status().available);
        svc.properties_changed(DEV, "org.bluez.Battery1", &Map::new(), &[]);
        assert_eq!(svc.current().rev, 1);
        let mut changed = Map::new();
        changed.insert("Percentage".into(), json!(65));
        svc.properties_changed(DEV, "org.bluez.Battery1", &changed, &[]);
        assert_eq!(svc.current().rev, 2);
        assert_eq!(svc.current().devices[0].battery_percent, Some(65));
    }

    #[test]
    fn malformed_battery_clears_to_null_and_lost_empties() {
        let svc = BluetoothService::new();
        svc.load(&managed());
        let mut changed = Map::new();
        changed.insert("Percentage".into(), json!(250));
        svc.properties_changed(DEV, "org.bluez.Battery1", &changed, &[]);
        assert_eq!(svc.current().devices[0].battery_percent, None);
        svc.lost();
        assert!(svc.current().devices.is_empty());
    }

    #[test]
    fn no_adapter_is_unavailable_with_a_reason() {
        let svc = BluetoothService::new();
        svc.load(&json!({}));
        let status = svc.status();
        assert!(!status.available);
        assert_eq!(status.detail, "no-adapter");
        svc.interfaces_added(
            "/org/bluez/hci0",
            &json!({"org.bluez.Adapter1": {"Powered": false}}),
        );
        assert!(svc.status().available);
    }
}
