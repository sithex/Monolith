use power_core::{AlertConfig, AlertEngine, AlertError, AlertKind, PowerModel};
use serde_json::{json, Map};

fn snap(
    percent: Option<f64>,
    state: u64,
    on_battery: Option<bool>,
    present: bool,
) -> power_core::PowerSnapshot {
    let mut model = PowerModel::default();
    let mut map = Map::new();
    if let Some(p) = percent {
        map.insert("Percentage".to_string(), json!(p));
    }
    map.insert("State".to_string(), json!(state));
    map.insert("IsPresent".to_string(), json!(present));
    model.apply_device_props(&map);
    if let Some(b) = on_battery {
        model.apply_on_battery(&json!(b));
    }
    model.snapshot()
}

#[test]
fn default_config_is_low20_critical10_full100_hysteresis2() {
    let c = AlertConfig::default();
    assert_eq!(c.low(), 20.0);
    assert_eq!(c.critical(), 10.0);
    assert_eq!(c.full(), 100.0);
    assert_eq!(c.hysteresis(), 2.0);
}

#[test]
fn config_validation_errors() {
    assert_eq!(
        AlertConfig::new(20.0, 10.0, 100.0, 2.0),
        Ok(AlertConfig::default())
    );
    assert_eq!(
        AlertConfig::new(20.0, 20.0, 100.0, 2.0),
        Err(AlertError::InvalidThresholds)
    );
    assert_eq!(
        AlertConfig::new(10.0, 20.0, 100.0, 2.0),
        Err(AlertError::InvalidThresholds)
    );
    assert_eq!(
        AlertConfig::new(20.0, 10.0, 15.0, 2.0),
        Err(AlertError::InvalidThresholds)
    );
    assert_eq!(
        AlertConfig::new(20.0, 10.0, 101.0, 2.0),
        Err(AlertError::InvalidThresholds)
    );
    assert_eq!(
        AlertConfig::new(-1.0, 10.0, 100.0, 2.0),
        Err(AlertError::InvalidThresholds)
    );
    assert_eq!(
        AlertConfig::new(20.0, 10.0, 100.0, -0.1),
        Err(AlertError::InvalidHysteresis)
    );
    assert_eq!(
        AlertConfig::new(20.0, 10.0, 100.0, 10.1),
        Err(AlertError::InvalidHysteresis)
    );
    match AlertConfig::new(20.0, 10.0, 100.0, 10.0) {
        Ok(c) => assert_eq!(c.hysteresis(), 10.0),
        Err(e) => panic!("expected Ok, got {e:?}"),
    }
    assert_eq!(
        AlertConfig::new(f64::NAN, 10.0, 100.0, 2.0),
        Err(AlertError::InvalidThresholds)
    );
    assert_eq!(
        AlertConfig::new(20.0, f64::NAN, 100.0, 2.0),
        Err(AlertError::InvalidThresholds)
    );
    assert_eq!(
        AlertConfig::new(20.0, 10.0, f64::INFINITY, 2.0),
        Err(AlertError::InvalidThresholds)
    );
    assert_eq!(
        AlertConfig::new(20.0, 10.0, 100.0, f64::NAN),
        Err(AlertError::InvalidHysteresis)
    );
}

#[test]
fn low_fires_once_at_threshold_while_discharging() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(e.update(&snap(Some(25.0), 2, Some(false), true)), None);
    assert_eq!(
        e.update(&snap(Some(20.0), 2, Some(false), true)),
        Some(AlertKind::Low)
    );
    assert_eq!(e.update(&snap(Some(19.0), 2, Some(false), true)), None);
    assert_eq!(
        e.update(&snap(Some(10.0), 2, Some(false), true)),
        Some(AlertKind::Critical)
    );
}

#[test]
fn discharging_rules_include_unknown_with_onbattery_true() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(15.0), 0, Some(true), true)),
        Some(AlertKind::Low)
    );

    let mut e2 = AlertEngine::new(AlertConfig::default());
    assert_eq!(e2.update(&snap(Some(15.0), 0, Some(false), true)), None);

    let mut e3 = AlertEngine::new(AlertConfig::default());
    assert_eq!(e3.update(&snap(Some(15.0), 0, None, true)), None);

    let mut e4 = AlertEngine::new(AlertConfig::default());
    assert_eq!(e4.update(&snap(Some(15.0), 5, Some(true), true)), None); // PendingCharge is not discharging
}

#[test]
fn pending_discharge_counts_as_discharging() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(15.0), 6, None, true)),
        Some(AlertKind::Low)
    );
}

#[test]
fn jump_30_to_8_emits_critical_only_and_low_not_until_rearm() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(e.update(&snap(Some(30.0), 2, None, true)), None);
    assert_eq!(
        e.update(&snap(Some(8.0), 2, None, true)),
        Some(AlertKind::Critical)
    );
    assert_eq!(e.update(&snap(Some(8.0), 2, None, true)), None); // Low consumed; no re-fire
    assert_eq!(e.update(&snap(Some(8.0), 2, None, true)), None);
    // re-arm both at low+hysteresis (22)
    assert_eq!(e.update(&snap(Some(22.0), 2, None, true)), None);
    assert_eq!(
        e.update(&snap(Some(15.0), 2, None, true)),
        Some(AlertKind::Low)
    );
}

#[test]
fn critical_takes_priority_over_low_in_same_update() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(5.0), 2, None, true)),
        Some(AlertKind::Critical)
    );
    assert_eq!(e.update(&snap(Some(5.0), 2, None, true)), None); // Low not fired later either
}

#[test]
fn critical_fires_after_low_independently() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(15.0), 2, None, true)),
        Some(AlertKind::Low)
    );
    assert_eq!(
        e.update(&snap(Some(9.0), 2, None, true)),
        Some(AlertKind::Critical)
    );
}

#[test]
fn critical_fires_at_exactly_threshold() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(10.0), 2, None, true)),
        Some(AlertKind::Critical)
    );
    assert_eq!(e.update(&snap(Some(10.1), 2, None, true)), None);
}

#[test]
fn oscillation_within_hysteresis_fires_once() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(e.update(&snap(Some(25.0), 2, None, true)), None);
    assert_eq!(
        e.update(&snap(Some(19.0), 2, None, true)),
        Some(AlertKind::Low)
    );
    for _ in 0..5 {
        assert_eq!(e.update(&snap(Some(21.0), 2, None, true)), None);
        assert_eq!(e.update(&snap(Some(19.0), 2, None, true)), None);
    }
    // exactly at low + hysteresis re-arms
    assert_eq!(e.update(&snap(Some(22.0), 2, None, true)), None);
    assert_eq!(
        e.update(&snap(Some(19.0), 2, None, true)),
        Some(AlertKind::Low)
    );
}

#[test]
fn critical_rearms_only_at_threshold_plus_hysteresis() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(8.0), 2, None, true)),
        Some(AlertKind::Critical)
    );
    assert_eq!(e.update(&snap(Some(11.0), 2, None, true)), None); // < 12 → no re-arm
    assert_eq!(e.update(&snap(Some(8.0), 2, None, true)), None); // no re-fire
    assert_eq!(e.update(&snap(Some(12.0), 2, None, true)), None); // exactly critical+hyst → re-arm
    assert_eq!(
        e.update(&snap(Some(8.0), 2, None, true)),
        Some(AlertKind::Critical)
    );
}

#[test]
fn charging_rearms_low_and_critical() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(15.0), 2, None, true)),
        Some(AlertKind::Low)
    );
    assert_eq!(e.update(&snap(Some(15.0), 1, None, true)), None); // charging starts → re-arm
    assert_eq!(
        e.update(&snap(Some(15.0), 2, None, true)),
        Some(AlertKind::Low)
    );

    let mut e2 = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e2.update(&snap(Some(8.0), 2, None, true)),
        Some(AlertKind::Critical)
    );
    assert_eq!(e2.update(&snap(Some(8.0), 5, None, true)), None); // PendingCharge re-arms
    assert_eq!(
        e2.update(&snap(Some(8.0), 2, None, true)),
        Some(AlertKind::Critical)
    );
}

#[test]
fn full_fires_on_fully_charged_even_below_100() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(97.0), 4, None, true)),
        Some(AlertKind::Full)
    );
    // persists in FullyCharged: no re-fire
    for _ in 0..5 {
        assert_eq!(e.update(&snap(Some(97.0), 4, None, true)), None);
    }
    // leave full condition, then re-arm via drop to full - hysteresis (98)
    assert_eq!(e.update(&snap(Some(90.0), 1, None, true)), None); // Charging below full → re-arm
    assert_eq!(
        e.update(&snap(Some(100.0), 1, None, true)),
        Some(AlertKind::Full)
    );
}

#[test]
fn full_fires_at_exactly_full_while_charging() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(e.update(&snap(Some(99.9), 1, None, true)), None);
    assert_eq!(
        e.update(&snap(Some(100.0), 1, None, true)),
        Some(AlertKind::Full)
    );

    let mut e2 = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e2.update(&snap(Some(100.0), 5, None, true)),
        Some(AlertKind::Full)
    );
}

#[test]
fn full_rearms_when_discharging_starts() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(100.0), 1, None, true)),
        Some(AlertKind::Full)
    );
    assert_eq!(e.update(&snap(Some(80.0), 2, None, true)), None); // discharging starts → re-arm
    assert_eq!(
        e.update(&snap(Some(100.0), 1, None, true)),
        Some(AlertKind::Full)
    );
}

#[test]
fn full_rearms_only_at_full_minus_hysteresis() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(100.0), 1, None, true)),
        Some(AlertKind::Full)
    );
    assert_eq!(e.update(&snap(Some(98.1), 1, None, true)), None); // > 98 → no re-arm
    assert_eq!(e.update(&snap(Some(100.0), 1, None, true)), None); // no re-fire
    assert_eq!(e.update(&snap(Some(98.0), 1, None, true)), None); // exactly full-hyst → re-arm
    assert_eq!(
        e.update(&snap(Some(100.0), 1, None, true)),
        Some(AlertKind::Full)
    );
}

#[test]
fn nothing_fires_when_absent_or_percentage_none() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(e.update(&snap(Some(5.0), 2, None, false)), None);
    assert_eq!(e.update(&snap(None, 2, None, true)), None);
    // reappearing without re-arm: first ever fire → Critical
    assert_eq!(
        e.update(&snap(Some(5.0), 2, None, true)),
        Some(AlertKind::Critical)
    );
}

#[test]
fn disappear_reappear_does_not_refire_without_rearm() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(15.0), 2, None, true)),
        Some(AlertKind::Low)
    );
    // device disappears (present=false) while still low
    assert_eq!(e.update(&snap(Some(15.0), 2, None, false)), None);
    // reappears still low: must NOT re-fire (no re-arm happened)
    assert_eq!(e.update(&snap(Some(15.0), 2, None, true)), None);
    assert_eq!(e.update(&snap(Some(15.0), 2, None, true)), None);
    // re-arm via charging, then fire again
    assert_eq!(e.update(&snap(Some(15.0), 1, None, true)), None);
    assert_eq!(
        e.update(&snap(Some(15.0), 2, None, true)),
        Some(AlertKind::Low)
    );
}

#[test]
fn percentage_none_leaves_armed_state_untouched() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(15.0), 2, None, true)),
        Some(AlertKind::Low)
    );
    // percentage None → nothing, no re-arm
    assert_eq!(e.update(&snap(None, 1, None, true)), None);
    // still disarmed at 15
    assert_eq!(e.update(&snap(Some(15.0), 2, None, true)), None);
}

#[test]
fn full_rearms_when_discharging_starts_inside_hysteresis_band() {
    let mut e = AlertEngine::new(AlertConfig::default());
    // fully charged at 100% fires Full
    assert_eq!(
        e.update(&snap(Some(100.0), 4, None, true)),
        Some(AlertKind::Full)
    );
    // discharging starts while still inside the hysteresis band (99 > 100 - 2):
    // nothing fires, but discharging re-arms Full
    assert_eq!(e.update(&snap(Some(99.0), 2, None, true)), None);
    // charging at 99%: still inside the band, nothing fires
    assert_eq!(e.update(&snap(Some(99.0), 1, None, true)), None);
    // fully charged again at 100%: discharging re-armed Full → fires again
    assert_eq!(
        e.update(&snap(Some(100.0), 4, None, true)),
        Some(AlertKind::Full)
    );
}

#[test]
fn full_does_not_refire_after_charging_dip_within_hysteresis_band() {
    let mut e = AlertEngine::new(AlertConfig::default());
    assert_eq!(
        e.update(&snap(Some(100.0), 1, None, true)),
        Some(AlertKind::Full)
    );
    // charging dip to 99% (inside the band): no discharge, and percentage did
    // not reach full − hysteresis → no re-arm
    assert_eq!(e.update(&snap(Some(99.0), 1, None, true)), None);
    // returning to fully charged at 100% must NOT fire again
    assert_eq!(e.update(&snap(Some(100.0), 4, None, true)), None);
}
