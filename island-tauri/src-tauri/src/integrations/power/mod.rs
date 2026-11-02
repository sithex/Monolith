//! Battery/power service (SPEC §3.5 "Battery/power" + "Battery alerts").
//!
//! `model.rs` is the audited pure core ported unchanged from `staging/power-core`. This module
//! owns the mutable state, the revisioned `PowerState` topic, the alert engine wiring and the
//! availability report; `upower.rs` feeds it from UPower's `DisplayDevice` over D-Bus.

pub mod model;
#[cfg(target_os = "linux")]
mod upower;

use crate::{
    bus::Latest,
    contract::{
        settings::Settings,
        system::{PowerState, ServiceStatus},
    },
    integrations::service_status::Reporter,
};
use model::{AlertConfig, AlertEngine, AlertKind, ChargeState, PowerModel, PowerSnapshot};
use serde_json::{Map, Value};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::watch;

/// Receives every fired battery alert (the application turns it into a `NotificationStore`
/// system alert; tests collect them).
pub type AlertSink = Arc<dyn Fn(AlertKind, &PowerSnapshot) + Send + Sync>;

/// Alert thresholds from `Settings` (`batteryAlerts`, `batteryLowPercent`,
/// `batteryCriticalPercent`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlertSettings {
    pub enabled: bool,
    pub low: f64,
    pub critical: f64,
}

impl From<&Settings> for AlertSettings {
    fn from(s: &Settings) -> Self {
        Self {
            enabled: s.battery_alerts,
            low: s.battery_low_percent,
            critical: s.battery_critical_percent,
        }
    }
}

struct Shared {
    model: PowerModel,
    engine: AlertEngine,
    settings: AlertSettings,
    rev: u32,
    available: bool,
    current: PowerState,
}

pub struct PowerService {
    pub status: Arc<Reporter>,
    pub topic: Latest<PowerState>,
    shared: Mutex<Shared>,
    sink: AlertSink,
    stop: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

fn engine_for(settings: AlertSettings) -> AlertEngine {
    // The settings validator bounds each value but not their order; an inconsistent pair
    // falls back to the audited defaults instead of disabling alerts silently.
    AlertEngine::new(
        AlertConfig::new(settings.low, settings.critical, 100.0, 2.0).unwrap_or_default(),
    )
}

fn state_name(state: ChargeState) -> &'static str {
    match state {
        ChargeState::Unknown => "unknown",
        ChargeState::Charging => "charging",
        ChargeState::Discharging => "discharging",
        ChargeState::Empty => "empty",
        ChargeState::FullyCharged => "fullyCharged",
        ChargeState::PendingCharge => "pendingCharge",
        ChargeState::PendingDischarge => "pendingDischarge",
    }
}

fn unavailable_state(rev: u32) -> PowerState {
    PowerState {
        rev,
        available: false,
        present: false,
        percent: None,
        state: state_name(ChargeState::Unknown).into(),
        charging: false,
        on_battery: None,
        time_to_empty: None,
        time_to_full: None,
    }
}

impl PowerService {
    pub fn new(settings: AlertSettings, sink: AlertSink) -> Arc<Self> {
        let current = unavailable_state(0);
        let topic = Latest::new();
        topic.publish(current.clone());
        Arc::new(Self {
            status: Reporter::new("power"),
            topic,
            shared: Mutex::new(Shared {
                model: PowerModel::default(),
                engine: engine_for(settings),
                settings,
                rev: 0,
                available: false,
                current,
            }),
            sink,
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

    pub fn current(&self) -> PowerState {
        self.lock().current.clone()
    }

    /// Settings changed (alerts toggle / thresholds). A threshold change re-arms the engine.
    pub fn configure(&self, settings: AlertSettings) {
        let mut shared = self.lock();
        if shared.settings != settings {
            shared.engine = engine_for(settings);
            shared.settings = settings;
        }
    }

    /// Apply a `PropertiesChanged` map for the UPower `DisplayDevice`.
    pub(crate) fn apply_device(&self, changed: &Map<String, Value>) {
        let mut shared = self.lock();
        shared.model.apply_device_props(changed);
        self.publish(&mut shared, true);
    }

    /// Apply the root object's `OnBattery`.
    pub(crate) fn apply_on_battery(&self, value: &Value) {
        let mut shared = self.lock();
        shared.model.apply_on_battery(value);
        self.publish(&mut shared, true);
    }

    /// Replace the whole model from an initial read (device properties + root OnBattery).
    pub(crate) fn load(&self, device: &Map<String, Value>, on_battery: Option<&Value>) {
        let mut shared = self.lock();
        shared.model = PowerModel::default();
        shared.model.apply_device_props(device);
        if let Some(value) = on_battery {
            shared.model.apply_on_battery(value);
        }
        self.publish(&mut shared, true);
    }

    /// The daemon is gone: every value becomes unknown (`null`), never a stale reading.
    pub(crate) fn lost(&self) {
        let mut shared = self.lock();
        shared.model = PowerModel::default();
        self.publish(&mut shared, false);
    }

    fn publish(&self, shared: &mut Shared, available: bool) {
        shared.available = available;
        let snapshot = shared.model.snapshot();
        let mut next = if available {
            PowerState {
                rev: 0,
                available: true,
                present: snapshot.present(),
                percent: snapshot.percentage(),
                state: state_name(snapshot.state()).into(),
                charging: snapshot.state() == ChargeState::Charging,
                on_battery: snapshot.on_battery(),
                time_to_empty: snapshot.time_to_empty_s().map(|s| s as f64),
                time_to_full: snapshot.time_to_full_s().map(|s| s as f64),
            }
        } else {
            unavailable_state(0)
        };
        let unchanged = PowerState {
            rev: shared.current.rev,
            ..next.clone()
        } == shared.current;
        if !unchanged {
            shared.rev = shared.rev.wrapping_add(1);
            next.rev = shared.rev;
            shared.current = next.clone();
            self.topic.publish(next);
        }
        // Alerts fire from the same snapshot, once per crossing (engine owns hysteresis).
        if available && shared.settings.enabled {
            if let Some(kind) = shared.engine.update(&snapshot) {
                (self.sink)(kind, &snapshot);
            }
        }
    }

    /// Start the Linux D-Bus worker. `address == None` means "no system bus may be used"
    /// (a test instance without an explicit private bus): the service stays unavailable with
    /// that reason and never touches any bus.
    pub fn start(self: &Arc<Self>, address: Option<String>) {
        let service = Arc::clone(self);
        let stopped = self.stop.subscribe();
        let task = tauri::async_runtime::spawn(async move {
            #[cfg(target_os = "linux")]
            upower::run(service, address, stopped).await;
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

    fn map(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(m) => m,
            other => panic!("not an object: {other}"),
        }
    }

    fn service() -> (Arc<PowerService>, Arc<Mutex<Vec<AlertKind>>>) {
        let fired = Arc::new(Mutex::new(Vec::new()));
        let sink_fired = Arc::clone(&fired);
        let svc = PowerService::new(
            AlertSettings {
                enabled: true,
                low: 20.0,
                critical: 10.0,
            },
            Arc::new(move |kind, _| sink_fired.lock().unwrap().push(kind)),
        );
        (svc, fired)
    }

    #[test]
    fn starts_unavailable_with_nulls_never_zero() {
        let (svc, _) = service();
        let state = svc.current();
        assert!(!state.available && !state.present);
        assert_eq!(state.percent, None);
        assert_eq!(state.on_battery, None);
        assert_eq!(state.time_to_empty, None);
        assert_eq!(state.state, "unknown");
    }

    #[test]
    fn snapshot_maps_units_and_revisions_only_real_changes() {
        let (svc, _) = service();
        svc.load(
            &map(json!({"IsPresent": true, "Percentage": 55.5, "State": 2, "TimeToEmpty": 3600})),
            Some(&json!(true)),
        );
        let state = svc.current();
        assert_eq!(state.rev, 1);
        assert_eq!(state.percent, Some(55.5));
        assert_eq!(state.state, "discharging");
        assert!(!state.charging);
        assert_eq!(state.on_battery, Some(true));
        assert_eq!(state.time_to_empty, Some(3600.0));
        assert_eq!(state.time_to_full, None);
        svc.apply_device(&map(json!({"Percentage": 55.5})));
        assert_eq!(
            svc.current().rev,
            1,
            "identical value is not a new revision"
        );
        svc.apply_device(&map(
            json!({"Percentage": 54.0, "State": 1, "TimeToFull": 120}),
        ));
        let state = svc.current();
        assert_eq!(
            (state.rev, state.charging, state.time_to_full),
            (2, true, Some(120.0))
        );
    }

    #[test]
    fn malformed_values_become_null_not_zero() {
        let (svc, _) = service();
        svc.load(
            &map(json!({"IsPresent": true, "Percentage": "high", "State": 99, "TimeToEmpty": -3})),
            Some(&json!("yes")),
        );
        let state = svc.current();
        assert!(state.available && state.present);
        assert_eq!(state.percent, None);
        assert_eq!(state.state, "unknown");
        assert_eq!(state.time_to_empty, None);
        assert_eq!(state.on_battery, None);
        svc.apply_device(&map(json!({"Percentage": 140.0})));
        assert_eq!(
            svc.current().percent,
            None,
            "out of range clears, not clamps"
        );
    }

    #[test]
    fn lost_clears_values_and_alerts_use_settings() {
        let (svc, fired) = service();
        svc.load(
            &map(json!({"IsPresent": true, "Percentage": 40.0, "State": 2})),
            Some(&json!(true)),
        );
        svc.apply_device(&map(json!({"Percentage": 19.0})));
        svc.apply_device(&map(json!({"Percentage": 9.0})));
        assert_eq!(
            *fired.lock().unwrap(),
            vec![AlertKind::Low, AlertKind::Critical]
        );
        svc.lost();
        let state = svc.current();
        assert!(!state.available);
        assert_eq!(state.percent, None);
        // Alerts disabled in settings: crossing again fires nothing.
        svc.configure(AlertSettings {
            enabled: false,
            low: 20.0,
            critical: 10.0,
        });
        svc.load(
            &map(json!({"IsPresent": true, "Percentage": 90.0, "State": 2})),
            None,
        );
        svc.apply_device(&map(json!({"Percentage": 5.0})));
        assert_eq!(fired.lock().unwrap().len(), 2);
    }

    #[test]
    fn inconsistent_thresholds_fall_back_to_defaults() {
        let (svc, fired) = service();
        svc.configure(AlertSettings {
            enabled: true,
            low: 5.0,
            critical: 30.0,
        });
        svc.load(
            &map(json!({"IsPresent": true, "Percentage": 50.0, "State": 2})),
            None,
        );
        svc.apply_device(&map(json!({"Percentage": 19.0})));
        assert_eq!(
            *fired.lock().unwrap(),
            vec![AlertKind::Low],
            "default low=20 applies"
        );
    }
}
