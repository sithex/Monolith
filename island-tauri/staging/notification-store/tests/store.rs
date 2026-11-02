use notification_store::*;
use serde_json::json;

fn keyed(title: &str, key: &str) -> NotificationInput {
    let mut input = NotificationInput::hermes(title);
    input.dedupe_key = Some(key.into());
    input
}
fn system(server_id: u32) -> NotificationInput {
    let mut input = NotificationInput::hermes("System");
    input.source = NotificationSource::System;
    input.app_name = "Mail".into();
    input.server_id = Some(server_id);
    input
}
fn assert_closed(changes: &Changes, id: &str, reason: CloseReason) {
    assert!(changes.events.iter().any(|event| matches!(event,
        Delta::NotificationClosed(closed) if closed.id == id && closed.reason == reason)));
}

#[test]
fn ring_evicts_oldest_and_reports_closed_before_received() {
    let mut store = NotificationStore::default();
    for index in 0..STORE_CAPACITY {
        store
            .insert(NotificationInput::hermes(format!("n{index}")), 0.0)
            .unwrap();
    }
    let original = store.list();
    let oldest = original[STORE_CAPACITY - 1].id.clone();
    let newest = original[0].id.clone();
    let changes = store
        .insert(NotificationInput::hermes("overflow"), 1.0)
        .unwrap();
    assert_eq!(store.len(), 200);
    assert_closed(&changes, &oldest, CloseReason::Evicted);
    assert!(matches!(&changes.events[0], Delta::NotificationClosed(_)));
    assert!(matches!(&changes.events[1], Delta::NotificationReceived(_)));
    assert_eq!(store.list()[0].title, "overflow");
    assert_eq!(store.list()[1].id, newest);
    assert_eq!(store.list().last().unwrap().title, "n1");
    assert!(changes.close_server_ids.is_empty());
}

#[test]
fn dedupe_moves_to_front_keeps_id_updates_fields_and_refreshes_ttl() {
    let mut store = NotificationStore::default();
    let first = store
        .insert(keyed("first", "key"), 0.0)
        .unwrap()
        .id
        .unwrap();
    store.insert(keyed("second", "other"), 1.0).unwrap();
    let mut update = keyed("updated", "key");
    update.body = Some("body".into());
    update.progress = Some(0.75);
    let changes = store.insert(update, 5000.0).unwrap();
    assert_eq!(changes.id.as_deref(), Some(first.as_str()));
    assert_eq!(changes.events.len(), 1);
    assert_eq!(store.len(), 2);
    assert_eq!(store.list()[0].id, first);
    assert_eq!(store.list()[0].title, "updated");
    assert_eq!(store.list()[0].progress, Some(0.75));
    assert_eq!(store.list()[0].received_at, 5000.0);
    store.expire(6001.0).unwrap();
    assert_eq!(store.len(), 1);
    assert!(store.expire(10999.99).unwrap().events.is_empty());
    assert_closed(
        &store.expire(11000.0).unwrap(),
        &first,
        CloseReason::Expired,
    );
}

#[test]
fn dedupe_at_capacity_does_not_evict_an_unrelated_entry() {
    let mut store = NotificationStore::default();
    for i in 0..200 {
        store
            .insert(keyed(&format!("n{i}"), &format!("k{i}")), 0.0)
            .unwrap();
    }
    let changes = store.insert(keyed("update", "k0"), 1.0).unwrap();
    assert_eq!(store.len(), 200);
    assert_eq!(changes.events.len(), 1);
    assert_eq!(store.list()[0].title, "update");
    assert_eq!(store.list().last().unwrap().title, "n1");
}

#[test]
fn missing_dedupe_keys_do_not_coalesce_and_key_bytes_are_literal() {
    let mut store = NotificationStore::default();
    store.insert(NotificationInput::hermes("a"), 0.0).unwrap();
    store.insert(NotificationInput::hermes("a"), 0.0).unwrap();
    store.insert(keyed("a", "Key"), 0.0).unwrap();
    store.insert(keyed("a", "key"), 0.0).unwrap();
    assert_eq!(store.len(), 4);
}

#[test]
fn replaces_id_and_server_id_updates_preserve_server_identity() {
    let mut store = NotificationStore::default();
    store.insert(system(42), 0.0).unwrap();
    store.insert(system(43), 1.0).unwrap();
    let mut update = system(42);
    update.replaces_id = 42;
    update.title = "replacement".into();
    let changes = store.insert(update, 2.0).unwrap();
    assert_eq!(changes.id.as_deref(), Some("d-42"));
    assert_eq!(store.len(), 2);
    assert_eq!(store.list()[0].id, "d-42");
    assert_eq!(store.list()[0].title, "replacement");
    assert_eq!(changes.events.len(), 1);
    // Same returned server id dedupes even when the producer omitted replaces_id.
    store.insert(system(42), 3.0).unwrap();
    assert_eq!(store.len(), 2);
}

#[test]
fn changed_returned_server_id_closes_superseded_local_identity() {
    let mut store = NotificationStore::default();
    store.insert(system(10), 0.0).unwrap();
    let mut replacement = system(11);
    replacement.replaces_id = 10;
    let changes = store.insert(replacement, 1.0).unwrap();
    assert_closed(&changes, "d-10", CloseReason::Replaced);
    assert_eq!(store.len(), 1);
    assert_eq!(store.list()[0].id, "d-11");
}

#[test]
fn loop_guard_and_deny_list_drop_without_deltas_or_id_consumption() {
    let mut store = NotificationStore::default();
    let mut own = NotificationInput::hermes("own");
    own.app_name = "Hermes Island".into();
    assert_eq!(store.insert(own, 0.0).unwrap(), Changes::default());
    let mut policy = store.policy().clone();
    policy.denied_apps.insert("Mail".into());
    store.set_policy(policy).unwrap();
    assert_eq!(store.insert(system(4), 0.0).unwrap(), Changes::default());
    assert!(store.is_empty());
    let changes = store
        .insert(NotificationInput::hermes("allowed"), 0.0)
        .unwrap();
    assert_eq!(changes.id.as_deref(), Some("h-1"));
}

#[test]
fn redaction_is_applied_before_storage_and_emission() {
    let mut store = NotificationStore::new(StorePolicy {
        redact_bodies: true,
        ..StorePolicy::default()
    })
    .unwrap();
    let mut input = NotificationInput::hermes("private");
    input.body = Some("SECRET".into());
    let changes = store.insert(input, 0.0).unwrap();
    assert_eq!(store.list()[0].body.as_deref(), Some("Hidden"));
    assert!(!serde_json::to_string(&changes.events)
        .unwrap()
        .contains("SECRET"));
    let mut policy = store.policy().clone();
    policy.redact_bodies = false;
    store.set_policy(policy).unwrap();
    assert_eq!(store.list()[0].body.as_deref(), Some("Hidden"));
}

#[test]
fn policy_changes_scrub_existing_bodies_filter_apps_and_return_deltas() {
    let mut store = NotificationStore::default();
    let mut private = NotificationInput::hermes("private");
    private.body = Some("secret".into());
    store.insert(private, 0.0).unwrap();
    store.insert(system(9), 0.0).unwrap();
    let mut policy = store.policy().clone();
    policy.redact_bodies = true;
    policy.denied_apps.insert("Mail".into());
    let changes = store.set_policy(policy).unwrap();
    assert_eq!(changes.events.len(), 2);
    assert_closed(&changes, "d-9", CloseReason::Filtered);
    assert!(changes.close_server_ids.is_empty());
    assert!(!changes.toast);
    assert_eq!(store.len(), 1);
    assert_eq!(store.list()[0].body.as_deref(), Some("Hidden"));
    assert!(store
        .set_policy(store.policy().clone())
        .unwrap()
        .events
        .is_empty());
}

#[test]
fn dnd_stores_and_suppresses_even_critical_toasts_low_is_always_silent() {
    let mut store = NotificationStore::new(StorePolicy {
        dnd: true,
        ..StorePolicy::default()
    })
    .unwrap();
    let mut critical = NotificationInput::hermes("critical");
    critical.urgency = Urgency::Critical;
    let changes = store.insert(critical, 0.0).unwrap();
    assert!(!changes.toast);
    assert_eq!(changes.events.len(), 1);
    let mut policy = store.policy().clone();
    policy.dnd = false;
    assert!(store.set_policy(policy).unwrap().events.is_empty());
    let mut low = NotificationInput::hermes("low");
    low.urgency = Urgency::Low;
    assert!(!store.insert(low, 0.0).unwrap().toast);
    assert!(
        store
            .insert(NotificationInput::hermes("normal"), 0.0)
            .unwrap()
            .toast
    );
    assert_eq!(store.len(), 3);
}

#[test]
fn ttl_boundaries_zero_resident_and_critical_transient_policy() {
    let mut store = NotificationStore::default();
    let mut input = keyed("fractional", "fractional");
    input.ttl_ms = Some(0.5);
    let id = store.insert(input, 100.25).unwrap().id.unwrap();
    assert!(store.expire(100.749).unwrap().events.is_empty());
    assert_closed(&store.expire(100.75).unwrap(), &id, CloseReason::Expired);
    assert!(store.expire(100.75).unwrap().events.is_empty());
    let mut resident = keyed("resident", "resident");
    resident.ttl_ms = Some(0.0);
    store.insert(resident, 0.0).unwrap();
    let mut critical = keyed("critical", "critical");
    critical.urgency = Urgency::Critical;
    store.insert(critical.clone(), 0.0).unwrap();
    critical.dedupe_key = Some("transient".into());
    critical.transient = true;
    let transient_id = store.insert(critical.clone(), 0.0).unwrap().id.unwrap();
    assert!(store.is_transient(&transient_id));
    assert!(store.expire(5999.99).unwrap().events.is_empty());
    assert_closed(
        &store.expire(6000.0).unwrap(),
        &transient_id,
        CloseReason::Expired,
    );
    assert!(!store.is_transient(&transient_id));
    critical.dedupe_key = Some("explicit".into());
    critical.ttl_ms = Some(1.0);
    let explicit_id = store.insert(critical, 7000.0).unwrap().id.unwrap();
    assert_closed(
        &store.expire(7001.0).unwrap(),
        &explicit_id,
        CloseReason::Expired,
    );
    store.expire(1e12).unwrap();
    assert_eq!(store.len(), 2);
}

#[test]
fn dismiss_clear_server_close_are_idempotent_and_request_only_owned_closes() {
    let mut store = NotificationStore::default();
    store.insert(system(4), 0.0).unwrap();
    let dismissed = store.dismiss("d-4");
    assert_closed(&dismissed, "d-4", CloseReason::Dismissed);
    assert_eq!(dismissed.close_server_ids, vec![4]);
    assert_eq!(store.dismiss("d-4"), Changes::default());
    store.insert(system(5), 0.0).unwrap();
    let closed = store.server_closed(5);
    assert_closed(&closed, "d-5", CloseReason::ServerClosed);
    assert!(closed.close_server_ids.is_empty());
    assert_eq!(store.server_closed(5), Changes::default());
    store.insert(system(6), 0.0).unwrap();
    store.insert(keyed("Hermes", "key"), 0.0).unwrap();
    let cleared = store.clear();
    assert_eq!(cleared.events.len(), 2);
    assert_eq!(cleared.close_server_ids, vec![6]);
    assert!(store.is_empty());
    assert_eq!(store.clear(), Changes::default());
}

#[test]
fn invalid_numeric_and_oversized_inputs_are_atomic() {
    let mut store = NotificationStore::default();
    store.insert(keyed("good", "key"), 0.0).unwrap();
    let before = store.list();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert!(store.insert(keyed("bad", "key"), bad).is_err());
        assert!(store.expire(bad).is_err());
        let mut input = keyed("bad", "key");
        input.ttl_ms = Some(bad);
        assert!(store.insert(input, 0.0).is_err());
        let mut input = keyed("bad", "key");
        input.progress = Some(bad);
        assert!(store.insert(input, 0.0).is_err());
        let policy = StorePolicy {
            default_ttl_ms: bad,
            ..StorePolicy::default()
        };
        assert!(store.set_policy(policy).is_err());
        assert_eq!(store.list(), before);
    }
    for mut input in [
        keyed("bad", "key"),
        keyed("bad", "key"),
        keyed("bad", "key"),
    ] {
        input.title = "é".repeat(MAX_TITLE_BYTES / 2 + 1);
        assert!(store.insert(input, 0.0).is_err());
    }
    let mut input = keyed("bad", "key");
    input.body = Some("x".repeat(MAX_BODY_BYTES + 1));
    assert!(store.insert(input, 0.0).is_err());
    assert!(store.insert(keyed("bad", "key"), f64::MAX).is_err());
    assert_eq!(store.list(), before);
}

#[test]
fn validation_covers_actions_icons_identity_and_exact_string_caps() {
    let mut store = NotificationStore::default();
    let mut exact = NotificationInput::hermes("é".repeat(MAX_TITLE_BYTES / 2));
    exact.app_name = "a".repeat(MAX_NAME_BYTES);
    exact.body = Some("é".repeat(MAX_BODY_BYTES / 2));
    exact.dedupe_key = Some("k".repeat(MAX_NAME_BYTES));
    exact.actions = vec![
        NotificationAction {
            key: "k".repeat(MAX_NAME_BYTES),
            label: "l".repeat(MAX_NAME_BYTES)
        };
        MAX_ACTIONS
    ];
    exact.icon_png = Some(format!(
        "data:image/png;base64,{}",
        "A".repeat(MAX_ICON_BYTES - "data:image/png;base64,".len())
    ));
    exact.ttl_ms = Some(MAX_TTL_MS);
    store.insert(exact.clone(), 0.0).unwrap();
    let mut invalid = vec![];
    let mut x = exact.clone();
    x.actions.push(x.actions[0].clone());
    invalid.push(x);
    let mut x = exact.clone();
    x.actions[0].key.push('x');
    invalid.push(x);
    let mut x = exact.clone();
    x.actions[0].label.push('x');
    invalid.push(x);
    let mut x = exact.clone();
    x.icon_png = Some("file:///private".into());
    invalid.push(x);
    let mut x = exact.clone();
    x.icon_png.as_mut().unwrap().push('A');
    invalid.push(x);
    let mut x = exact.clone();
    x.server_id = Some(1);
    invalid.push(x);
    let mut x = exact.clone();
    x.replaces_id = 1;
    invalid.push(x);
    let mut x = system(0);
    x.title = "bad".into();
    invalid.push(x);
    let mut x = system(1);
    x.dedupe_key = Some("k".into());
    invalid.push(x);
    let mut x = exact.clone();
    x.progress = Some(1.001);
    invalid.push(x);
    let mut x = exact.clone();
    x.ttl_ms = Some(MAX_TTL_MS + 1.0);
    invalid.push(x);
    for candidate in invalid {
        assert!(store.insert(candidate, 0.0).is_err());
    }
    assert_eq!(store.len(), 1);
}

#[test]
fn emitted_events_json_shape_snapshot() {
    let mut store = NotificationStore::default();
    let mut input = NotificationInput::hermes("Build finished");
    input.body = Some("41 passed".into());
    input.actions = vec![NotificationAction {
        key: "open".into(),
        label: "Open".into(),
    }];
    input.progress = Some(1.0);
    let received = store.insert(input, 1234.5).unwrap();
    let closed = store.dismiss("h-1");
    let mut events = received.events;
    events.extend(closed.events);
    assert_eq!(
        serde_json::to_value(&events).unwrap(),
        json!([
            {"type":"notificationReceived","payload":{
                "rev":1,"id":"h-1","source":"hermes","appName":"Hermes",
                "title":"Build finished","body":"41 passed","iconPng":null,
                "urgency":"normal","receivedAt":1234.5,
                "actions":[{"key":"open","label":"Open"}],"canInvokeActions":false,"progress":1.0
            }},
            {"type":"notificationClosed","payload":{"rev":2,"id":"h-1","reason":"dismissed"}}
        ])
    );
    if let Delta::NotificationReceived(payload) = &events[0] {
        assert_eq!(
            serde_json::to_value(payload).unwrap(),
            serde_json::to_value(&events).unwrap()[0]["payload"]
        );
    } else {
        panic!("expected received");
    }
}
