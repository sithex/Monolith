//! Pure core behind SPEC §3.5's "Battery/power" and "Battery alerts" rows:
//! UPower D-Bus values (already converted to `serde_json::Value`) drive a
//! [`PowerModel`], whose snapshots feed an [`AlertEngine`] that emits low,
//! critical and full battery alerts. Platform-free: no IO, no network, no
//! threads, no clock reading, no `unsafe`, and never panics on any input.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// UPower `DisplayDevice.State` values (integers 0..=6), as documented for
/// the `org.freedesktop.UPower` `DisplayDevice` interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ChargeState {
    #[default]
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

/// Parse a UPower `DisplayDevice.State` value. Exactly the integers 0..=6
/// map to [`ChargeState`] variants; any other value or type yields `None`.
pub fn parse_state(v: &Value) -> Option<ChargeState> {
    match v.as_u64()? {
        0 => Some(ChargeState::Unknown),
        1 => Some(ChargeState::Charging),
        2 => Some(ChargeState::Discharging),
        3 => Some(ChargeState::Empty),
        4 => Some(ChargeState::FullyCharged),
        5 => Some(ChargeState::PendingCharge),
        6 => Some(ChargeState::PendingDischarge),
        _ => None,
    }
}

/// An immutable snapshot of the battery display state, built by
/// [`PowerModel::snapshot`].
///
/// Serde wires these fields as camelCase (`percentage`, `state`,
/// `onBattery`, `timeToEmptyS`, `timeToFullS`, `present`); no aliases are
/// accepted. Fields are read through accessors.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PowerSnapshot {
    /// `DisplayDevice.IsPresent` (default false).
    present: bool,
    /// Finite percentage inside 0..=100, or `None`.
    percentage: Option<f64>,
    /// `DisplayDevice.State`.
    state: ChargeState,
    /// Root object `OnBattery`, or `None` until observed.
    on_battery: Option<bool>,
    /// `DisplayDevice.TimeToEmpty` seconds; `None` when 0/unknown.
    time_to_empty_s: Option<u64>,
    /// `DisplayDevice.TimeToFull` seconds; `None` when 0/unknown.
    time_to_full_s: Option<u64>,
}

impl PowerSnapshot {
    /// `DisplayDevice.IsPresent`.
    pub fn present(&self) -> bool {
        self.present
    }

    /// Finite percentage inside 0..=100, or `None`.
    pub fn percentage(&self) -> Option<f64> {
        self.percentage
    }

    /// `DisplayDevice.State`.
    pub fn state(&self) -> ChargeState {
        self.state
    }

    /// Root object `OnBattery`, or `None` until observed.
    pub fn on_battery(&self) -> Option<bool> {
        self.on_battery
    }

    /// `DisplayDevice.TimeToEmpty` seconds; `None` when 0/unknown.
    pub fn time_to_empty_s(&self) -> Option<u64> {
        self.time_to_empty_s
    }

    /// `DisplayDevice.TimeToFull` seconds; `None` when 0/unknown.
    pub fn time_to_full_s(&self) -> Option<u64> {
        self.time_to_full_s
    }
}

/// A finite percentage inside the inclusive 0..=100 range.
fn percentage_from_f64(f: f64) -> Option<f64> {
    if f.is_finite() && (0.0..=100.0).contains(&f) {
        Some(f)
    } else {
        None
    }
}

fn parse_percentage(value: &Value) -> Option<f64> {
    percentage_from_f64(value.as_f64()?)
}

/// Read a JSON number as a non-negative integer, accepting integral floats.
fn value_as_nonneg_integer(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                return Some(u);
            }
            if let Some(i) = n.as_i64() {
                return (i >= 0).then_some(i as u64);
            }
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f >= 0.0 && (f - f.trunc()).abs() < 1e-9 && f <= u64::MAX as f64
                {
                    return Some(f as u64);
                }
            }
            None
        }
        _ => None,
    }
}

/// `TimeToEmpty` / `TimeToFull` (`x` seconds): 0 means unknown, and zero,
/// negative, non-integer and missing values all become `None` (never
/// fabricated).
fn parse_time(value: &Value) -> Option<u64> {
    let t = value_as_nonneg_integer(value)?;
    (t != 0).then_some(t)
}

/// Tracks the UPower `DisplayDevice` state and the root object's
/// `OnBattery` flag. Starts absent: `present = false`, everything
/// `None`/`Unknown`. Only keys present in a `PropertiesChanged` map change;
/// a key whose value is invalid is cleared to its absent value (`None`,
/// `Unknown` or `false`) rather than keeping the stale value.
#[derive(Debug, Clone, Default)]
pub struct PowerModel {
    present: bool,
    percentage: Option<f64>,
    state: ChargeState,
    on_battery: Option<bool>,
    time_to_empty_s: Option<u64>,
    time_to_full_s: Option<u64>,
}

impl PowerModel {
    /// Apply a UPower `DisplayDevice` `PropertiesChanged` map. Only keys
    /// present in the map are touched; a key whose value is invalid is set
    /// to its absent value (`None`, `Unknown` or `false`) rather than kept
    /// stale. `OnBattery` is a root-object property and belongs to
    /// [`Self::apply_on_battery`], not this map.
    pub fn apply_device_props(&mut self, changed: &Map<String, Value>) {
        for (key, value) in changed {
            match key.as_str() {
                "Percentage" => self.percentage = parse_percentage(value),
                "State" => self.state = parse_state(value).unwrap_or(ChargeState::Unknown),
                "TimeToEmpty" => self.time_to_empty_s = parse_time(value),
                "TimeToFull" => self.time_to_full_s = parse_time(value),
                "IsPresent" => self.present = matches!(value, Value::Bool(true)),
                _ => {}
            }
        }
    }

    /// Apply the root object's `OnBattery` property. Any non-boolean value
    /// clears the flag to `None`.
    pub fn apply_on_battery(&mut self, value: &Value) {
        self.on_battery = match value {
            Value::Bool(b) => Some(*b),
            _ => None,
        };
    }

    /// Snapshot the current state.
    pub fn snapshot(&self) -> PowerSnapshot {
        PowerSnapshot {
            present: self.present,
            percentage: self.percentage,
            state: self.state,
            on_battery: self.on_battery,
            time_to_empty_s: self.time_to_empty_s,
            time_to_full_s: self.time_to_full_s,
        }
    }
}

/// Which battery alert fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertKind {
    Low,
    Critical,
    Full,
}

/// Typed configuration validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertError {
    /// low/critical/full not all in 0..=100 or not satisfying
    /// `critical < low < full`.
    InvalidThresholds,
    /// hysteresis outside 0..=10.
    InvalidHysteresis,
}

/// Threshold configuration for the battery alert engine. Defaults (see
/// [`Default`]): low 20, critical 10, full 100, hysteresis 2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlertConfig {
    low: f64,
    critical: f64,
    full: f64,
    hysteresis: f64,
}

impl AlertConfig {
    /// Build a config, validating `critical < low < full` (all inside
    /// 0..=100) and `hysteresis` inside 0..=10.
    pub fn new(low: f64, critical: f64, full: f64, hysteresis: f64) -> Result<Self, AlertError> {
        let thresholds_in_range = (0.0..=100.0).contains(&low)
            && (0.0..=100.0).contains(&critical)
            && (0.0..=100.0).contains(&full);
        if !thresholds_in_range {
            return Err(AlertError::InvalidThresholds);
        }
        if !(0.0..=10.0).contains(&hysteresis) {
            return Err(AlertError::InvalidHysteresis);
        }
        if !(critical < low && low < full) {
            return Err(AlertError::InvalidThresholds);
        }
        Ok(AlertConfig {
            low,
            critical,
            full,
            hysteresis,
        })
    }

    /// Low threshold (default 20).
    pub fn low(&self) -> f64 {
        self.low
    }

    /// Critical threshold (default 10).
    pub fn critical(&self) -> f64 {
        self.critical
    }

    /// Full threshold (default 100).
    pub fn full(&self) -> f64 {
        self.full
    }

    /// Hysteresis band (default 2).
    pub fn hysteresis(&self) -> f64 {
        self.hysteresis
    }
}

impl Default for AlertConfig {
    fn default() -> Self {
        AlertConfig {
            low: 20.0,
            critical: 10.0,
            full: 100.0,
            hysteresis: 2.0,
        }
    }
}

/// Fires low / critical / full battery alerts from a [`PowerSnapshot`],
/// each at most once until re-armed.
///
/// # Firing
///
/// - **Low** fires once while discharging (state `Discharging` /
///   `PendingDischarge`, or `Unknown` with `OnBattery == true`) and
///   `percentage <= low`.
/// - **Critical** fires once under the same discharging rule and
///   `percentage <= critical`, independent of Low. A Critical fire also
///   consumes a pending Low, so crossing straight from above low to below
///   critical emits Critical only.
/// - **Full** fires once when state is `FullyCharged`, or `Charging` /
///   `PendingCharge` with `percentage >= full`.
///
/// At most one alert fires per update; priority is Critical > Low > Full.
/// Nothing fires while `present == false` or `percentage` is `None`, and a
/// device disappearing then reappearing never re-fires an alert whose
/// condition never re-armed.
///
/// # Re-arming
///
/// Low/Critical re-arm only when the fire condition is currently false and
/// either the percentage has climbed back to the threshold plus the
/// hysteresis band, or charging has started. Full re-arms only when its fire
/// condition is false and either discharging has started or the percentage
/// has dropped to full minus the hysteresis band. While the fire condition
/// stays true (e.g. a persistent `FullyCharged` state) the alert stays
/// disarmed.
#[derive(Debug, Clone)]
pub struct AlertEngine {
    cfg: AlertConfig,
    low_armed: bool,
    critical_armed: bool,
    full_armed: bool,
}

impl AlertEngine {
    /// A fresh engine: every alert is armed and ready to fire.
    pub fn new(cfg: AlertConfig) -> Self {
        AlertEngine {
            cfg,
            low_armed: true,
            critical_armed: true,
            full_armed: true,
        }
    }

    /// Whether the device reads as discharging: state Discharging /
    /// PendingDischarge, or Unknown while `OnBattery == true`.
    fn discharging(snap: &PowerSnapshot) -> bool {
        match snap.state {
            ChargeState::Discharging | ChargeState::PendingDischarge => true,
            ChargeState::Unknown => snap.on_battery == Some(true),
            _ => false,
        }
    }

    /// Whether the device is charging (Charging / PendingCharge).
    fn charging(snap: &PowerSnapshot) -> bool {
        matches!(
            snap.state,
            ChargeState::Charging | ChargeState::PendingCharge
        )
    }

    /// The full-alert fire condition: state FullyCharged, or Charging /
    /// PendingCharge at or above the full threshold.
    fn full_condition(snap: &PowerSnapshot, pct: f64, cfg: &AlertConfig) -> bool {
        snap.state == ChargeState::FullyCharged
            || ((snap.state == ChargeState::Charging || snap.state == ChargeState::PendingCharge)
                && pct >= cfg.full)
    }

    /// Feed one snapshot; returns the alert that fires on this update, if
    /// any (at most one, priority Critical > Low > Full).
    pub fn update(&mut self, snap: &PowerSnapshot) -> Option<AlertKind> {
        let pct = snap.percentage?;
        if !snap.present {
            return None;
        }

        let is_discharging = Self::discharging(snap);
        let is_charging = Self::charging(snap);
        let full_cond = Self::full_condition(snap, pct, &self.cfg);

        let fired = if self.critical_armed && is_discharging && pct <= self.cfg.critical {
            self.critical_armed = false;
            self.low_armed = false; // a Critical fire consumes the pending Low
            Some(AlertKind::Critical)
        } else if self.low_armed && is_discharging && pct <= self.cfg.low {
            self.low_armed = false;
            Some(AlertKind::Low)
        } else if self.full_armed && full_cond {
            self.full_armed = false;
            Some(AlertKind::Full)
        } else {
            None
        };

        // Re-arm (each only when its fire condition is currently false).
        if !self.critical_armed {
            let cond = is_discharging && pct <= self.cfg.critical;
            if !cond && (pct >= self.cfg.critical + self.cfg.hysteresis || is_charging) {
                self.critical_armed = true;
            }
        }
        if !self.low_armed {
            let cond = is_discharging && pct <= self.cfg.low;
            if !cond && (pct >= self.cfg.low + self.cfg.hysteresis || is_charging) {
                self.low_armed = true;
            }
        }
        if !self.full_armed
            && !full_cond
            && (pct <= self.cfg.full - self.cfg.hysteresis || is_discharging)
        {
            self.full_armed = true;
        }

        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn percentage_from_f64_rejects_nonfinite_and_out_of_range() {
        assert_eq!(percentage_from_f64(f64::NAN), None);
        assert_eq!(percentage_from_f64(f64::INFINITY), None);
        assert_eq!(percentage_from_f64(f64::NEG_INFINITY), None);
        assert_eq!(percentage_from_f64(-0.1), None);
        assert_eq!(percentage_from_f64(100.1), None);
        assert_eq!(percentage_from_f64(0.0), Some(0.0));
        assert_eq!(percentage_from_f64(100.0), Some(100.0));
        assert_eq!(percentage_from_f64(50.5), Some(50.5));
    }

    #[test]
    fn value_as_nonneg_integer_edge_cases() {
        assert_eq!(value_as_nonneg_integer(&json!(0)), Some(0));
        assert_eq!(value_as_nonneg_integer(&json!(-1)), None);
        assert_eq!(value_as_nonneg_integer(&json!(5.5)), None);
        assert_eq!(value_as_nonneg_integer(&json!(5.0)), Some(5));
        assert_eq!(value_as_nonneg_integer(&json!("5")), None);
        assert_eq!(value_as_nonneg_integer(&json!(u64::MAX)), Some(u64::MAX));
    }
}
