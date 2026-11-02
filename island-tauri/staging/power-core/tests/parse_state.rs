use power_core::{parse_state, ChargeState};
use serde_json::json;

#[test]
fn maps_all_six_states() {
    assert_eq!(parse_state(&json!(0)), Some(ChargeState::Unknown));
    assert_eq!(parse_state(&json!(1)), Some(ChargeState::Charging));
    assert_eq!(parse_state(&json!(2)), Some(ChargeState::Discharging));
    assert_eq!(parse_state(&json!(3)), Some(ChargeState::Empty));
    assert_eq!(parse_state(&json!(4)), Some(ChargeState::FullyCharged));
    assert_eq!(parse_state(&json!(5)), Some(ChargeState::PendingCharge));
    assert_eq!(parse_state(&json!(6)), Some(ChargeState::PendingDischarge));
}

#[test]
fn rejects_out_of_range_and_other_types() {
    assert_eq!(parse_state(&json!(7)), None);
    assert_eq!(parse_state(&json!(-1)), None);
    assert_eq!(parse_state(&json!(255)), None);
    assert_eq!(parse_state(&json!("1")), None);
    assert_eq!(parse_state(&json!(1.0)), None);
    assert_eq!(parse_state(&json!(true)), None);
    assert_eq!(parse_state(&json!(null)), None);
    assert_eq!(parse_state(&json!([2])), None);
    assert_eq!(parse_state(&json!({})), None);
}
