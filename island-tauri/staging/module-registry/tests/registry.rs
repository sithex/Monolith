use module_registry::{
    migrate_from_flat, upgrade, JsonValue, ModuleConfig, ModuleEntry, ModuleId, Registry,
    RegistryError, CURRENT_VERSION, KNOWN_IDS, MAX_JSON_BYTES,
};
use serde_json::{json, Value};

fn id(value: &str) -> ModuleId {
    match ModuleId::new(value) {
        Ok(id) => id,
        Err(e) => panic!("bad test ID: {e}"),
    }
}
fn entry(registry: &Registry, name: &str) -> ModuleEntry {
    match registry.get(&id(name)) {
        Some(entry) => entry.clone(),
        None => panic!("missing {name}"),
    }
}
fn value(registry: &Registry) -> Value {
    match serde_json::to_value(registry) {
        Ok(value) => value,
        Err(e) => panic!("serialization: {e}"),
    }
}
fn with_unknown(enabled: bool) -> Value {
    let mut raw = value(&Registry::defaults());
    match raw["modules"].as_array_mut() {
        Some(modules) => modules.push(json!({"id":"future-widget","enabled":enabled,"order":99,"config":{"type":"futureWidget","future_flag":true,"nested":[1.5,null,"keep"]}})),
        None => panic!("expected modules array"),
    }
    raw
}

#[test]
fn json_shape_snapshot_all_modules_camel_case() {
    assert_eq!(
        value(&Registry::defaults()),
        json!({
            "version": 1,
            "modules": [
                {"id":"terminal","enabled":true,"order":0,"config":{"type":"terminal","fontSize":13.0}},
                {"id":"monitor","enabled":true,"order":1,"config":{"type":"monitor"}},
                {"id":"ai","enabled":true,"order":2,"config":{"type":"ai"}},
                {"id":"settings","enabled":true,"order":3,"config":{"type":"settings"}},
                {"id":"notifications","enabled":true,"order":4,"config":{"type":"notifications","systemNotificationMirror":false,"toastTtlMs":6000.0,"redactBodies":false,"dnd":false}},
                {"id":"clipboard","enabled":false,"order":5,"config":{"type":"clipboard","historySize":100.0,"persist":false}},
                {"id":"music","enabled":false,"order":6,"config":{"type":"music","lyrics":true,"equalizer":true,"equalizerBands":16}}
            ]
        })
    );
    let json = Registry::defaults().export_json().unwrap();
    assert!(!json.contains('_'));
    assert_eq!(Registry::default(), Registry::defaults());
    assert_eq!(Registry::defaults().version(), CURRENT_VERSION);
}

#[test]
fn validated_slug_boundaries_without_normalization() {
    for good in [
        "a",
        "z0-",
        "future-widget",
        "abcdefghijklmnopqrstuvwxyz012345",
    ] {
        let parsed = ModuleId::new(good).unwrap();
        assert_eq!(parsed.as_str(), good);
        assert_eq!(serde_json::to_value(parsed).unwrap(), good);
    }
    for bad in [
        "",
        "A",
        "0a",
        "-a",
        "a_b",
        "a b",
        "é",
        "a\n",
        "abcdefghijklmnopqrstuvwxyz0123456",
    ] {
        assert!(
            matches!(ModuleId::new(bad), Err(RegistryError::InvalidId(_))),
            "{bad:?}"
        );
        assert!(serde_json::from_value::<ModuleId>(json!(bad)).is_err());
    }
    for known in KNOWN_IDS {
        assert!(id(known).is_known());
    }
    assert!(!id("future-widget").is_known());
}

#[test]
fn export_import_roundtrip_is_deterministic_and_reports_unknowns() {
    let mut registry = Registry::defaults();
    registry
        .reorder(&KNOWN_IDS.into_iter().rev().map(id).collect::<Vec<_>>())
        .unwrap();
    registry.set_enabled(&id("clipboard"), true).unwrap();
    let exported = registry.export_json().unwrap();
    let (loaded, report) = Registry::from_json(&exported).unwrap();
    assert_eq!(loaded, registry);
    assert!(report.unknown_ids.is_empty());
    assert_eq!(report.upgraded_from, None);
    assert_eq!(loaded.export_json().unwrap(), exported);
}

#[test]
fn import_atomic_on_syntax_semantic_version_and_size_errors() {
    let mut registry = Registry::defaults();
    registry.set_enabled(&id("terminal"), false).unwrap();
    let before = registry.clone();
    let mut invalid_config = value(&Registry::defaults());
    invalid_config["modules"][6]["config"]["equalizerBands"] = json!(7);
    let mut duplicate = value(&Registry::defaults());
    duplicate["modules"][6] = duplicate["modules"][0].clone();
    let bads = [
        "{".to_owned(),
        invalid_config.to_string(),
        duplicate.to_string(),
        json!({"version":2,"modules":[]}).to_string(),
        " ".repeat(MAX_JSON_BYTES + 1),
    ];
    for bad in bads {
        assert!(registry.import_json(&bad).is_err());
        assert_eq!(registry, before);
        assert_eq!(value(&registry), value(&before));
    }
}

#[test]
fn import_success_replaces_instead_of_merging() {
    let mut registry = Registry::defaults();
    let report = registry
        .import_json(r#"{"version":1,"modules":[]}"#)
        .unwrap();
    assert!(registry.modules().is_empty());
    assert!(report.unknown_ids.is_empty());
}

#[test]
fn reorder_rejects_every_non_permutation_atomically() {
    let mut registry = Registry::defaults();
    let before = registry.clone();
    let ids: Vec<_> = registry.modules().iter().map(|m| m.id.clone()).collect();
    let mut duplicate = ids.clone();
    duplicate[1] = duplicate[0].clone();
    let mut foreign = ids.clone();
    foreign[1] = id("missing");
    let mut extra = ids.clone();
    extra.push(id("extra"));
    for bad in [vec![], ids[..6].to_vec(), duplicate, foreign, extra] {
        assert_eq!(
            registry.reorder(&bad),
            Err(RegistryError::InvalidPermutation)
        );
        assert_eq!(registry, before);
    }
}

#[test]
fn reorder_includes_unknown_ids_and_sets_contiguous_orders() {
    let (mut registry, _) = Registry::from_json(&with_unknown(false).to_string()).unwrap();
    let before = registry.clone();
    let known_only: Vec<_> = KNOWN_IDS.into_iter().map(id).collect();
    assert_eq!(
        registry.reorder(&known_only),
        Err(RegistryError::InvalidPermutation)
    );
    assert_eq!(registry, before);
    let ids: Vec<_> = registry
        .modules()
        .iter()
        .rev()
        .map(|m| m.id.clone())
        .collect();
    registry.reorder(&ids).unwrap();
    for ((module, expected_id), order) in registry.modules().iter().zip(ids).zip(0u32..) {
        assert_eq!(module.id, expected_id);
        assert_eq!(module.order, order);
        assert_eq!(module.config, entry(&before, module.id.as_str()).config);
    }
}

#[test]
fn normalization_orders_by_previous_order_then_id_and_is_idempotent() {
    let defaults = Registry::defaults();
    let modules = ["settings", "monitor", "ai", "terminal"]
        .into_iter()
        .map(|name| {
            let mut m = entry(&defaults, name);
            m.order = if name == "terminal" { 2 } else { u32::MAX };
            m
        })
        .collect();
    let registry = Registry::new(1, modules).unwrap();
    let normalized = registry.normalized();
    assert_eq!(
        normalized
            .modules()
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["terminal", "ai", "monitor", "settings"]
    );
    assert_eq!(
        normalized
            .modules()
            .iter()
            .map(|m| m.order)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    assert_eq!(normalized.normalized(), normalized);
    assert_eq!(registry.modules()[0].order, u32::MAX);
    assert_eq!(
        Registry::from_json(&registry.export_json().unwrap())
            .unwrap()
            .0,
        normalized
    );
}

#[test]
fn unknown_modules_preserved_verbatim_and_inert() {
    let raw = with_unknown(true);
    let (mut registry, report) = Registry::from_json(&raw.to_string()).unwrap();
    assert_eq!(report.unknown_ids, vec![id("future-widget")]);
    assert_eq!(registry.unknown_ids(), report.unknown_ids);
    let unknown = entry(&registry, "future-widget");
    assert!(unknown.enabled); // stored bit is preserved, not interpreted as service activation
    assert!(!unknown.is_active());
    assert_eq!(
        value(&registry)["modules"][7]["config"],
        raw["modules"][7]["config"]
    );
    assert_eq!(value(&registry)["modules"][7]["id"], "future-widget");
    let before = registry.clone();
    assert_eq!(
        registry.set_enabled(&unknown.id, true),
        Err(RegistryError::UnknownModule(unknown.id.clone()))
    );
    assert_eq!(registry, before);
    registry.set_enabled(&unknown.id, false).unwrap();
    assert!(!entry(&registry, "future-widget").enabled);
    let exported = registry.export_json().unwrap();
    assert_eq!(Registry::from_json(&exported).unwrap().0, registry);
}

#[test]
fn unknown_payload_can_reuse_known_tag_without_interpretation() {
    for config in [
        json!({"type":"terminal","fontSize":99.0,"future_field":true}),
        json!(null),
        json!([true, "opaque"]),
    ] {
        let mut raw = with_unknown(false);
        raw["modules"][7]["config"] = config.clone();
        let (registry, _) = Registry::from_json(&raw.to_string()).unwrap();
        assert!(matches!(
            entry(&registry, "future-widget").config,
            ModuleConfig::Unknown(_)
        ));
        assert_eq!(value(&registry)["modules"][7]["config"], config);
    }
}

#[test]
fn set_enabled_known_and_missing_errors_are_atomic() {
    let mut registry = Registry::defaults();
    registry.set_enabled(&id("music"), true).unwrap();
    assert!(entry(&registry, "music").is_active());
    let before = registry.clone();
    assert_eq!(
        registry.set_enabled(&id("absent"), false),
        Err(RegistryError::MissingModule(id("absent")))
    );
    assert_eq!(registry, before);
}

#[test]
fn music_bands_range_and_config_mismatch_are_atomic() {
    let mut registry = Registry::defaults();
    for bands in [8, 16, 32] {
        let config = ModuleConfig::Music {
            lyrics: false,
            equalizer: true,
            equalizer_bands: bands,
        };
        registry.set_config(&id("music"), config.clone()).unwrap();
        assert_eq!(entry(&registry, "music").config, config);
    }
    let before = registry.clone();
    for bands in [0, 7, 33, u32::MAX] {
        assert_eq!(
            registry.set_config(
                &id("music"),
                ModuleConfig::Music {
                    lyrics: true,
                    equalizer: false,
                    equalizer_bands: bands
                }
            ),
            Err(RegistryError::InvalidConfig(id("music")))
        );
        assert_eq!(registry, before);
    }
    assert_eq!(
        registry.set_config(&id("music"), ModuleConfig::Monitor {}),
        Err(RegistryError::ConfigMismatch(id("music")))
    );
    assert_eq!(
        registry.set_config(&id("absent"), ModuleConfig::Monitor {}),
        Err(RegistryError::MissingModule(id("absent")))
    );
    assert_eq!(registry, before);
}

#[test]
fn numeric_ranges_reject_nonfinite_and_preserve_fractional_legacy_values() {
    let mut registry = Registry::defaults();
    for n in [8.0, 14.5, 24.0] {
        registry
            .set_config(&id("terminal"), ModuleConfig::Terminal { font_size: n })
            .unwrap();
    }
    let before = registry.clone();
    for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 7.99, 24.01] {
        assert!(registry
            .set_config(&id("terminal"), ModuleConfig::Terminal { font_size: n })
            .is_err());
        assert_eq!(registry, before);
    }
    for n in [9.99, 500.01, f64::NAN] {
        assert!(registry
            .set_config(
                &id("clipboard"),
                ModuleConfig::Clipboard {
                    history_size: n,
                    persist: true
                }
            )
            .is_err());
    }
    for n in [1499.99, 20000.01, f64::INFINITY] {
        assert!(registry
            .set_config(
                &id("notifications"),
                ModuleConfig::Notifications {
                    system_notification_mirror: true,
                    toast_ttl_ms: n,
                    redact_bodies: true,
                    dnd: true
                }
            )
            .is_err());
    }
    assert_eq!(registry, before);
}

#[test]
fn unknown_nonfinite_payload_rejected_recursively() {
    let (mut registry, _) = Registry::from_json(&with_unknown(false).to_string()).unwrap();
    let before = registry.clone();
    let raw = JsonValue::Object(std::collections::BTreeMap::from([(
        "nested".to_owned(),
        JsonValue::Array(vec![JsonValue::Number(f64::NAN)]),
    )]));
    assert_eq!(
        registry.set_config(&id("future-widget"), ModuleConfig::Unknown(raw)),
        Err(RegistryError::InvalidConfig(id("future-widget")))
    );
    assert_eq!(registry, before);
}

#[test]
fn strict_input_rejects_unknown_fields_aliases_arrays_and_wrong_numeric_types() {
    let base = value(&Registry::defaults());
    let mut bads = vec![json!([]), json!(null), json!([1, []])];
    let mut raw = base.clone();
    raw["extra"] = json!(true);
    bads.push(raw);
    let mut raw = base.clone();
    raw["modules"][0]["extra"] = json!(true);
    bads.push(raw);
    let mut raw = base.clone();
    raw["modules"][0] = json!(["terminal",true,0,{"type":"terminal","fontSize":13}]);
    bads.push(raw);
    let mut raw = base.clone();
    raw["modules"][0]["config"]["extra"] = json!(true);
    bads.push(raw);
    let mut raw = base.clone();
    raw["modules"][0]["config"] = json!({"type":"terminal","font_size":13});
    bads.push(raw);
    let mut raw = base.clone();
    raw["modules"][0]["config"]["type"] = json!("monitor");
    bads.push(raw);
    let mut raw = base.clone();
    raw["modules"][0]["id"] = json!("Terminal");
    bads.push(raw);
    for wrong in [
        json!(8.5),
        json!(-1),
        json!(4294967296.0),
        json!("16"),
        json!(null),
    ] {
        let mut raw = base.clone();
        raw["modules"][6]["config"]["equalizerBands"] = wrong;
        bads.push(raw);
    }
    for wrong in [json!(1.5), json!(-1), json!(4294967296.0), json!("1")] {
        let mut raw = base.clone();
        raw["version"] = wrong.clone();
        bads.push(raw);
        let mut raw = base.clone();
        raw["modules"][0]["order"] = wrong;
        bads.push(raw);
    }
    for raw in bads {
        assert!(
            Registry::from_json(&raw.to_string()).is_err(),
            "accepted {raw}"
        );
        assert!(
            serde_json::from_value::<Registry>(raw.clone()).is_err(),
            "serde accepted {raw}"
        );
    }
}

#[test]
fn duplicate_envelope_fields_are_rejected() {
    for raw in [
        r#"{"version":1,"version":1,"modules":[]}"#,
        r#"{"version":1,"modules":[],"modules":[]}"#,
        r#"{"version":1,"modules":[{"id":"monitor","id":"monitor","enabled":true,"order":0,"config":{"type":"monitor"}}]}"#,
    ] {
        assert!(Registry::from_json(raw).is_err());
    }
}

#[test]
fn duplicate_ids_and_config_mismatch_rejected_by_constructor() {
    let defaults = Registry::defaults();
    let m = entry(&defaults, "terminal");
    assert!(matches!(
        Registry::new(1, vec![m.clone(), m.clone()]),
        Err(RegistryError::DuplicateId(_))
    ));
    let mut bad = m;
    bad.config = ModuleConfig::Unknown(JsonValue::Null);
    assert!(matches!(
        Registry::new(1, vec![bad]),
        Err(RegistryError::ConfigMismatch(_))
    ));
    assert_eq!(
        Registry::new(2, vec![]),
        Err(RegistryError::UnsupportedVersion(2))
    );
}

#[test]
fn import_size_cap_is_inclusive_in_bytes() {
    let minimal = r#"{"version":1,"modules":[]}"#;
    let exact = format!("{minimal}{}", " ".repeat(MAX_JSON_BYTES - minimal.len()));
    assert!(Registry::from_json(&exact).is_ok());
    assert_eq!(
        Registry::from_json(&(exact + " ")),
        Err(RegistryError::TooLarge)
    );
    let huge_utf8 = format!(
        "{{\"version\":1,\"modules\":[],\"x\":\"{}\"}}",
        "é".repeat(MAX_JSON_BYTES / 2)
    );
    assert_eq!(
        Registry::from_json(&huge_utf8),
        Err(RegistryError::TooLarge)
    );
}

#[test]
fn export_size_cap_rejects_unimportable_document() {
    let module = ModuleEntry {
        id: id("future-widget"),
        enabled: false,
        order: 0,
        config: ModuleConfig::Unknown(JsonValue::String("x".repeat(MAX_JSON_BYTES))),
    };
    let registry = Registry::new(1, vec![module]).unwrap();
    assert_eq!(registry.export_json(), Err(RegistryError::TooLarge));
}

#[test]
fn realistic_flat_fixture_migrates_all_corresponding_keys_without_loss() {
    let flat: Value = serde_json::from_str(include_str!("fixtures/flat-settings.json")).unwrap();
    let untouched_before = flat.clone();
    let (registry, report) = migrate_from_flat(&flat);
    assert_eq!(flat, untouched_before);
    assert_eq!(
        report.mapped_keys,
        [
            "clipboardHistory",
            "clipboardHistorySize",
            "clipboardPersist",
            "dnd",
            "hardwareMonitor",
            "notificationsEnabled",
            "redactBodies",
            "systemNotificationMirror",
            "terminalFontSize",
            "toastTtlMs"
        ]
    );
    assert!(report.invalid_keys.is_empty());
    assert_eq!(report.input_problem, None);
    assert!(!entry(&registry, "monitor").enabled);
    assert!(!entry(&registry, "notifications").enabled);
    assert!(entry(&registry, "clipboard").enabled);
    assert_eq!(
        entry(&registry, "terminal").config,
        ModuleConfig::Terminal { font_size: 14.5 }
    );
    assert_eq!(
        entry(&registry, "notifications").config,
        ModuleConfig::Notifications {
            system_notification_mirror: true,
            toast_ttl_ms: 3250.5,
            redact_bodies: true,
            dnd: true
        }
    );
    assert_eq!(
        entry(&registry, "clipboard").config,
        ModuleConfig::Clipboard {
            history_size: 123.5,
            persist: true
        }
    );
    for (key, original) in flat.as_object().unwrap() {
        if !report.mapped_keys.contains(key) {
            assert_eq!(&report.untouched[key], original);
        }
    }
    assert_eq!(
        report.untouched.len() + report.mapped_keys.len(),
        flat.as_object().unwrap().len()
    );
    assert!(registry.unknown_ids().is_empty());
    assert!(registry.get(&id("future-widget")).is_none());
    assert_eq!(report.untouched["future-widget"], flat["future-widget"]);
    assert!(!entry(&registry, "music").enabled);
    assert_eq!(
        Registry::from_json(&registry.export_json().unwrap())
            .unwrap()
            .0,
        registry
    );
}

#[test]
fn migration_unexpected_types_and_invalid_ranges_keep_defaults_and_original_values() {
    let flat = json!({"hardwareMonitor":"yes","terminalFontSize":999,"notificationsEnabled":null,"systemNotificationMirror":[],"toastTtlMs":1,"redactBodies":{},"dnd":1,"clipboardHistory":"true","clipboardHistorySize":-5,"clipboardPersist":null,"music":{"enabled":true},"moduleRegistry":{"modules":[{"id":"untrusted","enabled":true}]}});
    let (registry, report) = migrate_from_flat(&flat);
    assert_eq!(registry, Registry::defaults());
    assert_eq!(report.invalid_keys.len(), 10);
    assert!(report.mapped_keys.is_empty());
    assert_eq!(serde_json::to_value(&report.untouched).unwrap(), flat);
    assert!(registry.unknown_ids().is_empty());
}

#[test]
fn migration_non_objects_never_fail() {
    for flat in [json!(null), json!(true), json!(5), json!("text"), json!([])] {
        let (registry, report) = migrate_from_flat(&flat);
        assert_eq!(registry, Registry::defaults());
        assert!(report.input_problem.is_some());
        assert!(report.invalid_keys.is_empty());
    }
    let (registry, report) = migrate_from_flat(&json!({}));
    assert_eq!(registry, Registry::defaults());
    assert_eq!(report, Default::default());
}

#[test]
fn version_zero_upgrade_adds_disabled_music_preserves_values_and_reports_version() {
    let text = include_str!("fixtures/version-0.json");
    let raw: Value = serde_json::from_str(text).unwrap();
    let upgraded = upgrade(raw).unwrap();
    assert_eq!(upgraded.version(), 1);
    assert_eq!(upgraded.modules().len(), 3);
    assert_eq!(
        entry(&upgraded, "terminal").config,
        ModuleConfig::Terminal { font_size: 15.5 }
    );
    assert!(!entry(&upgraded, "music").enabled);
    assert_eq!(
        entry(&upgraded, "music").config,
        entry(&Registry::defaults(), "music").config
    );
    assert_eq!(
        value(&upgraded)["modules"][0]["config"],
        json!({"future_flag":true,"nested":{"mode":"opaque"}})
    );
    let (loaded, report) = Registry::from_json(text).unwrap();
    assert_eq!(loaded, upgraded.normalized());
    assert_eq!(report.upgraded_from, Some(0));
    assert_eq!(report.unknown_ids, vec![id("future-widget")]);
    assert_eq!(upgrade(value(&loaded)).unwrap(), loaded);
}

#[test]
fn upgrade_never_enables_unknown_modules() {
    let text = include_str!("fixtures/version-0.json");
    let upgraded = upgrade(serde_json::from_str(text).unwrap()).unwrap();
    let unknown = entry(&upgraded, "future-widget");
    assert!(!unknown.enabled);
    assert!(!unknown.is_active());
    assert!(entry(&upgraded, "terminal").enabled);
}

#[test]
fn upgrade_rejects_future_versions_and_invalid_v0_before_editing() {
    for version in [2, u32::MAX] {
        assert_eq!(
            upgrade(json!({"version":version,"modules":[]})),
            Err(RegistryError::UnsupportedVersion(version))
        );
    }
    let mut raw = value(&Registry::defaults());
    raw["version"] = json!(0);
    let upgraded = upgrade(raw.clone()).unwrap();
    assert_eq!(upgraded.modules().len(), 7); // no duplicate music when already present
    raw["modules"][0]["config"]["fontSize"] = json!(100);
    assert!(upgrade(raw).is_err());
}
