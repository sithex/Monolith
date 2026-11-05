//! Additive clipboard privacy settings remain strict and atomic.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use hermes_island_lib::{
    contract::settings::{ObjectOnly, Settings, SettingsPatch},
    services::{autostart::Autostart, settings::SettingsService},
};
use serde_json::{json, Value};
fn patch(value: Value) -> SettingsPatch {
    serde_json::from_value::<ObjectOnly<SettingsPatch>>(value)
        .unwrap()
        .0
}
#[test]
fn clipboard_deny_settings_are_bounded_strict_atomic_and_persisted() {
    let dir = std::env::temp_dir().join(format!("hi-clip-settings-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let make = || {
        SettingsService::new(
            dir.join("data"),
            Autostart::with_default_launcher(dir.clone(), dir.join("app"), true),
        )
    };
    let svc = make();
    let next = svc
        .patch(&patch(
            json!({"clipboardDenyPatterns":["^secret", "日本語"]}),
        ))
        .unwrap();
    assert_eq!(make().get().unwrap(), next);
    let file = dir.join("data/settings.json");
    let bytes = std::fs::read(&file).unwrap();
    for patterns in [
        vec![String::new()],
        vec!["(".into()],
        vec!["x".repeat(257)],
        vec!["a".into(); 33],
    ] {
        assert!(svc
            .patch(&patch(
                json!({"stealth":true,"clipboardDenyPatterns":patterns})
            ))
            .is_err());
        assert_eq!(svc.get().unwrap(), next);
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
    }
    for value in [Value::Null, json!("regex"), json!([null]), json!([12])] {
        assert!(serde_json::from_value::<ObjectOnly<SettingsPatch>>(
            json!({"clipboardDenyPatterns":value})
        )
        .is_err());
    }
    assert!(patch(json!({"clipboardDenyPatterns":["é".repeat(256)]}))
        .apply(&next)
        .is_ok());
    let cleared = svc
        .patch(&patch(json!({"clipboardDenyPatterns":[]})))
        .unwrap();
    assert!(cleared.clipboard_deny_patterns.is_empty());
    let mut old = serde_json::to_value(Settings::default()).unwrap();
    old.as_object_mut().unwrap().remove("clipboardDenyPatterns");
    assert!(serde_json::from_value::<Settings>(old)
        .unwrap()
        .clipboard_deny_patterns
        .is_empty());
    assert!(!dir.join(".config").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
