use hermes_island_lib::integrations::power::model::{ChargeState, PowerModel, PowerSnapshot};
use serde_json::{json, Map};

#[test]
fn starts_absent() {
    let s = PowerModel::default().snapshot();
    assert!(!s.present());
    assert_eq!(s.percentage(), None);
    assert_eq!(s.state(), ChargeState::Unknown);
    assert_eq!(s.on_battery(), None);
    assert_eq!(s.time_to_empty_s(), None);
    assert_eq!(s.time_to_full_s(), None);
}

#[test]
fn applies_full_display_device_map() {
    let mut m = PowerModel::default();
    let mut props = Map::new();
    props.insert("Percentage".to_string(), json!(52.5));
    props.insert("State".to_string(), json!(2));
    props.insert("TimeToEmpty".to_string(), json!(3600));
    props.insert("TimeToFull".to_string(), json!(1800));
    props.insert("IsPresent".to_string(), json!(true));
    m.apply_device_props(&props);

    let s = m.snapshot();
    assert!(s.present());
    assert_eq!(s.percentage(), Some(52.5));
    assert_eq!(s.state(), ChargeState::Discharging);
    assert_eq!(s.time_to_empty_s(), Some(3600));
    assert_eq!(s.time_to_full_s(), Some(1800));
    assert_eq!(s.on_battery(), None);
}

#[test]
fn percentage_boundaries() {
    for (raw, want) in [(0, Some(0.0)), (100, Some(100.0)), (-1, None), (101, None)] {
        let mut m = PowerModel::default();
        let mut p = Map::new();
        p.insert("Percentage".to_string(), json!(raw));
        m.apply_device_props(&p);
        assert_eq!(m.snapshot().percentage(), want, "raw={raw}");
    }
}

#[test]
fn fractional_percentage_accepted() {
    let mut m = PowerModel::default();
    let mut p = Map::new();
    p.insert("Percentage".to_string(), json!(0.5));
    m.apply_device_props(&p);
    assert_eq!(m.snapshot().percentage(), Some(0.5));
}

#[test]
fn percentage_rejects_non_numbers() {
    let mut m = PowerModel::default();
    let mut p = Map::new();
    p.insert("Percentage".to_string(), json!("50"));
    m.apply_device_props(&p);
    assert_eq!(m.snapshot().percentage(), None);
}

#[test]
fn time_to_empty_zero_negative_and_noninteger_are_none() {
    for v in [
        json!(0),
        json!(-5),
        json!(5.5),
        json!("30"),
        json!(true),
        json!(null),
    ] {
        let mut m = PowerModel::default();
        let mut p = Map::new();
        p.insert("TimeToEmpty".to_string(), v);
        m.apply_device_props(&p);
        assert_eq!(m.snapshot().time_to_empty_s(), None);
    }
}

#[test]
fn time_to_full_zero_negative_and_noninteger_are_none() {
    for v in [
        json!(0),
        json!(-5),
        json!(5.5),
        json!("30"),
        json!(true),
        json!(null),
    ] {
        let mut m = PowerModel::default();
        let mut p = Map::new();
        p.insert("TimeToFull".to_string(), v);
        m.apply_device_props(&p);
        assert_eq!(m.snapshot().time_to_full_s(), None);
    }
}

#[test]
fn times_accept_integers_and_integral_floats() {
    let mut m = PowerModel::default();
    let mut p = Map::new();
    p.insert("TimeToEmpty".to_string(), json!(1800));
    p.insert("TimeToFull".to_string(), json!(900.0));
    m.apply_device_props(&p);
    let s = m.snapshot();
    assert_eq!(s.time_to_empty_s(), Some(1800));
    assert_eq!(s.time_to_full_s(), Some(900));
}

#[test]
fn partial_changed_keeps_unlisted_fields() {
    let mut m = PowerModel::default();
    let mut p1 = Map::new();
    p1.insert("Percentage".to_string(), json!(73));
    p1.insert("State".to_string(), json!(2));
    p1.insert("TimeToEmpty".to_string(), json!(5000));
    p1.insert("IsPresent".to_string(), json!(true));
    m.apply_device_props(&p1);
    m.apply_on_battery(&json!(true));

    let mut p2 = Map::new();
    p2.insert("State".to_string(), json!(1));
    m.apply_device_props(&p2);

    let s = m.snapshot();
    assert_eq!(s.state(), ChargeState::Charging);
    assert_eq!(s.percentage(), Some(73.0));
    assert_eq!(s.time_to_empty_s(), Some(5000));
    assert!(s.present());
    assert_eq!(s.on_battery(), Some(true));
}

#[test]
fn invalid_value_clears_field_not_stale() {
    let mut m = PowerModel::default();
    let mut p1 = Map::new();
    p1.insert("Percentage".to_string(), json!(55));
    p1.insert("State".to_string(), json!(2));
    p1.insert("TimeToEmpty".to_string(), json!(100));
    p1.insert("IsPresent".to_string(), json!(true));
    m.apply_device_props(&p1);

    let mut p2 = Map::new();
    p2.insert("Percentage".to_string(), json!("bad"));
    p2.insert("State".to_string(), json!(7));
    p2.insert("TimeToEmpty".to_string(), json!("x"));
    p2.insert("IsPresent".to_string(), json!("yes"));
    m.apply_device_props(&p2);

    let s = m.snapshot();
    assert_eq!(s.percentage(), None);
    assert_eq!(s.state(), ChargeState::Unknown);
    assert_eq!(s.time_to_empty_s(), None);
    assert!(!s.present());
}

#[test]
fn invalid_state_value_clears_to_unknown_after_valid() {
    let mut m = PowerModel::default();
    let mut p1 = Map::new();
    p1.insert("State".to_string(), json!(1));
    m.apply_device_props(&p1);
    assert_eq!(m.snapshot().state(), ChargeState::Charging);

    let mut p2 = Map::new();
    p2.insert("State".to_string(), json!("1"));
    m.apply_device_props(&p2);
    assert_eq!(m.snapshot().state(), ChargeState::Unknown);
}

#[test]
fn on_battery_true_false_and_invalid() {
    let mut m = PowerModel::default();
    m.apply_on_battery(&json!(true));
    assert_eq!(m.snapshot().on_battery(), Some(true));
    m.apply_on_battery(&json!(false));
    assert_eq!(m.snapshot().on_battery(), Some(false));
    m.apply_on_battery(&json!("yes"));
    assert_eq!(m.snapshot().on_battery(), None);
    m.apply_on_battery(&json!(1));
    assert_eq!(m.snapshot().on_battery(), None);
}

#[test]
fn snapshot_wires_camelcase_and_no_alias() {
    let mut m = PowerModel::default();
    let mut p = Map::new();
    p.insert("Percentage".to_string(), json!(42));
    p.insert("State".to_string(), json!(2));
    p.insert("TimeToEmpty".to_string(), json!(100));
    p.insert("TimeToFull".to_string(), json!(200));
    p.insert("IsPresent".to_string(), json!(true));
    m.apply_device_props(&p);
    m.apply_on_battery(&json!(false));

    let obj = serde_json::to_value(m.snapshot()).unwrap();
    let obj = obj.as_object().unwrap();
    for k in [
        "onBattery",
        "percentage",
        "present",
        "state",
        "timeToEmptyS",
        "timeToFullS",
    ] {
        assert!(obj.contains_key(k), "missing camelCase key {k}");
    }
    assert_eq!(obj.len(), 6);

    // snake_case keys must NOT populate fields (no aliases)
    let from_snake: PowerSnapshot = serde_json::from_value(json!({
        "on_battery": true, "time_to_empty_s": 5, "time_to_full_s": 6,
    }))
    .unwrap();
    assert_eq!(from_snake.on_battery(), None);
    assert_eq!(from_snake.time_to_empty_s(), None);
    assert_eq!(from_snake.time_to_full_s(), None);

    let from_camel: PowerSnapshot = serde_json::from_value(json!({
        "onBattery": true, "percentage": 42.0, "present": true,
        "state": "Discharging", "timeToEmptyS": 100, "timeToFullS": 200,
    }))
    .unwrap();
    assert_eq!(from_camel.on_battery(), Some(true));
    assert_eq!(from_camel.percentage(), Some(42.0));
    assert!(from_camel.present());
    assert_eq!(from_camel.state(), ChargeState::Discharging);
    assert_eq!(from_camel.time_to_empty_s(), Some(100));
    assert_eq!(from_camel.time_to_full_s(), Some(200));
}
