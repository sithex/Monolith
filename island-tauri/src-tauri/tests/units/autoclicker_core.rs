//! Autoclicker, pure part: config validation, the saved file, interval jitter, registry.
use hermes_island_lib::contract::apps::{AutoclickerConfig, TriggerMode};
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
    assert!(validate(&with(|c| c.hotkey = String::new())).is_ok(), "no hotkey is allowed");
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
