use hermes_island_lib::integrations::toggles::core::{
    dnd_tile, next_profile, power_tile, PowerTile, PpdHold, PpdProfile, PpdSnapshot, Profile,
};

fn profile(name: &str) -> PpdProfile {
    PpdProfile {
        profile: name.to_string(),
        driver: Some("platform".to_string()),
    }
}

fn hold(reason: &str) -> PpdHold {
    PpdHold {
        profile: "performance".to_string(),
        reason: reason.to_string(),
        application_id: "org.example.app".to_string(),
    }
}

fn snapshot(
    active: &str,
    profiles: Vec<PpdProfile>,
    degraded: &str,
    holds: Vec<PpdHold>,
) -> PpdSnapshot {
    PpdSnapshot {
        active: active.to_string(),
        profiles,
        degraded: degraded.to_string(),
        holds,
    }
}

#[test]
fn tile_full_three_profiles_with_unknown_ignored() {
    let snap = snapshot(
        "balanced",
        vec![
            profile("power-saver"),
            profile("balanced"),
            profile("performance"),
            profile("turbo"),
        ],
        "lap-detected",
        vec![hold("lap-detected"), hold("high-operating-temperature")],
    );
    let tile = power_tile(Some(&snap));
    assert!(tile.available);
    assert_eq!(tile.unavailable_reason, None);
    assert_eq!(tile.active, Some(Profile::Balanced));
    assert_eq!(
        tile.choices,
        vec![Profile::PowerSaver, Profile::Balanced, Profile::Performance]
    );
    assert_eq!(tile.degraded.as_deref(), Some("lap-detected"));
    assert_eq!(
        tile.held_by,
        vec![
            "lap-detected".to_string(),
            "high-operating-temperature".to_string()
        ]
    );
    assert_eq!(tile.ignored, 1);
}

#[test]
fn tile_unknown_active_gives_none_available() {
    let snap = snapshot(
        "turbo",
        vec![profile("power-saver"), profile("balanced")],
        "",
        vec![],
    );
    let tile = power_tile(Some(&snap));
    assert!(tile.available);
    assert_eq!(tile.active, None);
    assert_eq!(tile.choices, vec![Profile::PowerSaver, Profile::Balanced]);
}

#[test]
fn tile_unknown_only_profiles_empty_choices() {
    let snap = snapshot(
        "turbo",
        vec![profile("turbo"), profile("hyper")],
        "",
        vec![],
    );
    let tile = power_tile(Some(&snap));
    assert!(tile.available);
    assert!(tile.choices.is_empty());
    assert_eq!(tile.active, None);
    assert_eq!(tile.ignored, 2);
}

#[test]
fn tile_degraded_reason_mapped_empty_to_none() {
    let snap = snapshot(
        "performance",
        vec![profile("power-saver"), profile("performance")],
        "high-operating-temperature",
        vec![],
    );
    assert_eq!(
        power_tile(Some(&snap)).degraded.as_deref(),
        Some("high-operating-temperature")
    );
    let snap_clean = snapshot(
        "performance",
        vec![profile("power-saver"), profile("performance")],
        "",
        vec![],
    );
    assert_eq!(power_tile(Some(&snap_clean)).degraded, None);
}

#[test]
fn tile_holds_listed_in_order_empty_reason_filtered() {
    let mut empty = hold("ignored");
    empty.reason = String::new();
    let snap = snapshot(
        "performance",
        vec![profile("performance")],
        "",
        vec![
            hold("lap-detected"),
            empty,
            hold("high-operating-temperature"),
        ],
    );
    assert_eq!(
        power_tile(Some(&snap)).held_by,
        vec![
            "lap-detected".to_string(),
            "high-operating-temperature".to_string()
        ]
    );
}

#[test]
fn tile_none_snapshot_unavailable() {
    let tile = power_tile(None);
    assert!(!tile.available);
    assert_eq!(
        tile.unavailable_reason.as_deref(),
        Some("power-profiles-daemon not running")
    );
    assert_eq!(tile.active, None);
    assert!(tile.choices.is_empty());
    assert_eq!(tile.degraded, None);
    assert!(tile.held_by.is_empty());
    assert_eq!(tile.ignored, 0);
}

#[test]
fn next_profile_missing_performance_two_choices_cycles() {
    let snap = snapshot(
        "power-saver",
        vec![profile("power-saver"), profile("balanced")],
        "",
        vec![],
    );
    let tile = power_tile(Some(&snap));
    assert_eq!(tile.choices, vec![Profile::PowerSaver, Profile::Balanced]);
    assert_eq!(next_profile(&tile), Some(Profile::Balanced));

    let snap_balanced = snapshot(
        "balanced",
        vec![profile("power-saver"), profile("balanced")],
        "",
        vec![],
    );
    let tile_balanced = power_tile(Some(&snap_balanced));
    assert_eq!(next_profile(&tile_balanced), Some(Profile::PowerSaver)); // wraps
}

#[test]
fn next_profile_three_profiles_cycles_all_ways() {
    let snap = snapshot(
        "performance",
        vec![
            profile("power-saver"),
            profile("balanced"),
            profile("performance"),
        ],
        "",
        vec![],
    );
    let tile = power_tile(Some(&snap));
    assert_eq!(next_profile(&tile), Some(Profile::PowerSaver)); // wraps from last
}

#[test]
fn next_profile_single_choice_none() {
    let snap = snapshot("balanced", vec![profile("balanced")], "", vec![]);
    let tile = power_tile(Some(&snap));
    assert_eq!(tile.choices, vec![Profile::Balanced]);
    assert_eq!(next_profile(&tile), None);
}

#[test]
fn next_profile_unknown_active_none() {
    let snap = snapshot(
        "turbo",
        vec![
            profile("power-saver"),
            profile("balanced"),
            profile("performance"),
        ],
        "",
        vec![],
    );
    let tile = power_tile(Some(&snap));
    assert_eq!(next_profile(&tile), None);
}

#[test]
fn next_profile_active_not_in_choices_none() {
    let snap = snapshot(
        "balanced",
        vec![profile("power-saver"), profile("performance")],
        "",
        vec![],
    );
    let tile = power_tile(Some(&snap));
    assert_eq!(tile.active, Some(Profile::Balanced));
    assert_eq!(
        tile.choices,
        vec![Profile::PowerSaver, Profile::Performance]
    );
    assert_eq!(next_profile(&tile), None);
}

#[test]
fn dnd_tile_mirrors_enabled() {
    assert!(dnd_tile(true).on);
    assert!(!dnd_tile(false).on);
}

#[test]
fn serde_profile_kebab_case() {
    assert_eq!(
        serde_json::to_string(&Profile::PowerSaver).unwrap(),
        "\"power-saver\""
    );
    assert_eq!(
        serde_json::to_string(&Profile::Balanced).unwrap(),
        "\"balanced\""
    );
    assert_eq!(
        serde_json::to_string(&Profile::Performance).unwrap(),
        "\"performance\""
    );
    assert_eq!(
        serde_json::from_str::<Profile>("\"balanced\"").unwrap(),
        Profile::Balanced
    );
    assert_eq!(
        serde_json::from_str::<Profile>("\"performance\"").unwrap(),
        Profile::Performance
    );
    assert!(serde_json::from_str::<Profile>("\"turbo\"").is_err());
}

#[test]
fn serde_snapshot_camel_case_roundtrip() {
    let snap = snapshot(
        "power-saver",
        vec![profile("balanced")],
        "",
        vec![hold("x")],
    );
    let json = serde_json::to_string(&snap).unwrap();
    assert!(json.contains("\"applicationId\""));
    assert!(!json.contains("application_id"));
    let back: PpdSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snap);
}

#[test]
fn serde_power_tile_roundtrip() {
    let snap = snapshot(
        "balanced",
        vec![profile("power-saver"), profile("performance")],
        "lap-detected",
        vec![],
    );
    let tile = power_tile(Some(&snap));
    let json = serde_json::to_string(&tile).unwrap();
    let back: PowerTile = serde_json::from_str(&json).unwrap();
    assert_eq!(back, tile);
}
