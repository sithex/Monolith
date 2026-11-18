//! Autoclicker, pure part: config validation, the saved file, interval jitter, registry.
use hermes_island_lib::contract::apps::{AutoclickerConfig, ClickKind, ClickTarget, TriggerMode};
use hermes_island_lib::integrations::autoclicker::core::*;

fn with(f: impl FnOnce(&mut AutoclickerConfig)) -> AutoclickerConfig {
    let mut c = defaults();
    f(&mut c);
    c
}

#[test]
fn defaults_are_valid_and_ranges_are_enforced() {
    assert!(validate(&defaults()).is_ok());
    assert_eq!(defaults().hotkey, "F8");
    assert!(validate(&with(|c| c.interval_ms = 0)).is_err());
    assert!(validate(&with(|c| c.interval_ms = MAX_INTERVAL_MS + 1)).is_err());
    assert!(validate(&with(|c| c.interval_ms = 1)).is_ok());
    assert!(validate(&with(|c| c.jitter_percent = 51)).is_err());
    assert!(validate(&with(|c| c.start_delay_s = 11)).is_err());
    assert!(validate(&with(|c| c.time_limit_s = MAX_TIME_LIMIT_S + 1)).is_err());
    assert!(
        validate(&with(|c| c.hotkey = String::new())).is_ok(),
        "no hotkey is allowed"
    );
    assert!(validate(&with(|c| c.hotkey = "Ctrl+Alt+K".into())).is_ok());
    assert!(validate(&with(|c| c.hotkey = "Ctrl+K, Ctrl+C".into())).is_err());
    assert!(validate(&with(|c| c.hotkey = "Hyper+Q".into())).is_err());
}

#[test]
fn saved_file_round_trips_and_bad_files_fall_back_to_defaults() {
    let c = with(|c| {
        c.interval_ms = 33;
        c.mode = TriggerMode::Hold;
        c.hotkey = "Shift+F6".into();
    });
    assert_eq!(from_json(&to_json(&c)), c);
    assert_eq!(from_json("not json"), defaults());
    assert_eq!(from_json(r#"{"intervalMs":0}"#), defaults());
    let out_of_range = to_json(&with(|c| c.jitter_percent = 99));
    assert_eq!(from_json(&out_of_range), defaults());
}

#[test]
fn new_fields_are_range_checked_and_keys_never_trigger_the_autoclicker() {
    assert_eq!(defaults().stop_hotkey, "Shift+Esc");
    assert!(validate(&with(|c| c.press_ms = 1001)).is_err());
    assert!(validate(&with(|c| c.burst_count = 0)).is_err());
    assert!(validate(&with(|c| c.burst_count = 1001)).is_err());
    assert!(
        validate(&with(|c| c.stop_hotkey = "F8".into())).is_err(),
        "stop key = hotkey"
    );
    assert!(validate(&with(|c| c.stop_hotkey = String::new())).is_ok());
    let key = |k: &str| {
        with(|c| {
            c.target = ClickTarget::Key;
            c.key = k.into();
        })
    };
    assert!(validate(&key("E")).is_ok());
    assert!(validate(&key("Ctrl+C")).is_ok());
    assert!(
        validate(&key("F8")).is_err(),
        "pressing the hotkey would toggle itself"
    );
    assert!(
        validate(&key("Shift+Esc")).is_err(),
        "pressing the stop key would stop itself"
    );
    assert!(validate(&key("!")).is_err(), "not on the virtual keyboard");
    assert!(validate(&key("")).is_err());
    // A key that is only stored (mouse mode) may be anything bindable.
    assert!(validate(&with(|c| c.key = "F8".into())).is_ok());
}

#[test]
fn the_older_bare_config_file_keeps_its_settings_and_gains_the_new_defaults() {
    let old = r#"{"intervalMs":33,"jitterPercent":5,"button":"right","kind":"double","mode":"hold",
        "clickLimit":0,"timeLimitS":0,"startDelayS":1,"hotkey":"F9"}"#;
    let saved = load(old);
    assert_eq!(saved.config.interval_ms, 33);
    assert_eq!(saved.config.hotkey, "F9");
    assert_eq!(saved.config.kind, ClickKind::Double);
    assert_eq!(saved.config.burst_count, defaults().burst_count);
    assert_eq!(saved.config.stop_hotkey, "Shift+Esc");
    assert_eq!(saved.lifetime_clicks, 0);
    // Unknown fields are dropped rather than failing the whole file.
    assert_eq!(
        load(r#"{"intervalMs":40,"futureThing":1}"#)
            .config
            .interval_ms,
        40
    );
}

#[test]
fn saved_file_with_stats_round_trips_and_ignores_old_presets() {
    let saved = Saved {
        config: with(|c| c.interval_ms = 10),
        lifetime_clicks: 1234,
        lifetime_ms: 5678,
    };
    assert_eq!(load(&store(&saved)), saved);
    let mut raw: serde_json::Value = serde_json::from_str(&store(&saved)).unwrap();
    raw["presets"] = serde_json::json!([{"name": "Fast", "config": {}}]);
    assert_eq!(
        load(&raw.to_string()),
        saved,
        "a file from a build with presets still loads"
    );
}

#[test]
fn jitter_stays_within_the_spread_and_never_below_one_ms() {
    let mut j = Jitter::new(42);
    assert_eq!(j.interval_us(100, 0), 100_000);
    let (mut lo, mut hi) = (u64::MAX, 0);
    for _ in 0..10_000 {
        let v = j.interval_us(100, 20);
        lo = lo.min(v);
        hi = hi.max(v);
    }
    assert!(lo >= 80_000 && hi <= 120_000, "{lo}..{hi}");
    assert!(hi - lo > 30_000, "the spread is actually used: {lo}..{hi}");
    for _ in 0..1000 {
        assert!(j.interval_us(1, 50) >= 1000);
    }
}

#[test]
fn autoclicker_launcher_app_exists_and_routes() {
    use hermes_island_lib::{
        contract::{island::IslandView, modules::Registry},
        services::workspace::module_route,
    };
    assert!(Registry::defaults()
        .modules()
        .iter()
        .any(|m| m.id.as_str() == "autoclicker" && m.enabled));
    assert_eq!(module_route("autoclicker"), Some(IslandView::Autoclicker));
}
