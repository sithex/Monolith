use hermes_island_lib::integrations::notifications::dbus::NotifyArgs;
use hermes_island_lib::integrations::notifications::*;
use std::collections::BTreeMap;

fn dbus_input(id: u32) -> NotificationInput {
    let mut input = NotificationInput::hermes("system");
    input.source = NotificationSource::System;
    input.app_name = "Mail".into();
    input.server_id = Some(id);
    input
}

#[test]
fn conflicting_replaces_and_returned_ids_never_leave_duplicate_or_stale_ids() {
    for order in [[10, 11], [11, 10]] {
        let mut store = NotificationStore::default();
        for id in order {
            store.insert(dbus_input(id), 0.0).unwrap();
        }
        let mut input = dbus_input(11);
        input.replaces_id = 10;
        let changes = store.insert(input, 1.0).unwrap();
        assert_eq!(store.len(), 1);
        assert_eq!(store.list()[0].id, "d-11");
        assert_eq!(changes.events.len(), 2);
        assert!(
            matches!(&changes.events[0], Delta::NotificationClosed(c) if c.id == "d-10" && c.reason == CloseReason::Replaced)
        );
        assert!(matches!(&changes.events[1], Delta::NotificationReceived(n) if n.0.id == "d-11"));
        assert!(changes.close_server_ids.is_empty());
    }
}

#[test]
fn replacement_lifetime_and_transient_transitions_replace_old_metadata() {
    for system in [false, true] {
        let mut store = NotificationStore::default();
        let mut input = if system {
            dbus_input(1)
        } else {
            NotificationInput::hermes("first")
        };
        if !system {
            input.dedupe_key = Some("key".into());
        }
        input.ttl_ms = Some(1.0);
        input.transient = true;
        let id = store.insert(input.clone(), 0.0).unwrap().id.unwrap();
        assert!(store.is_transient(&id));
        input.ttl_ms = Some(0.0);
        input.transient = false;
        assert_eq!(store.insert(input.clone(), 0.5).unwrap().events.len(), 1);
        assert!(!store.is_transient(&id));
        assert!(store.expire(100.0).unwrap().events.is_empty());
        input.ttl_ms = Some(2.0);
        store.insert(input.clone(), 100.0).unwrap();
        assert!(store.expire(101.999).unwrap().events.is_empty());
        assert_eq!(store.expire(102.0).unwrap().events.len(), 1);
        assert!(store.is_empty());
        input.ttl_ms = None;
        input.urgency = Urgency::Critical;
        let id = store.insert(input.clone(), 200.0).unwrap().id.unwrap();
        assert!(store.expire(10_000.0).unwrap().events.is_empty());
        input.urgency = Urgency::Normal;
        store.insert(input, 10_000.0).unwrap();
        assert_eq!(store.list()[0].id, id);
        assert!(store.expire(15999.999).unwrap().events.is_empty());
        assert_eq!(store.expire(16000.0).unwrap().events.len(), 1);
    }
}

#[test]
fn privacy_batches_emit_in_revision_order_and_payloads_equal_current_snapshot() {
    let mut store = NotificationStore::default();
    for name in ["first", "Mail", "third"] {
        let mut input = NotificationInput::hermes(name);
        input.app_name = name.into();
        input.body = Some(format!("{name} secret"));
        store.insert(input, 0.0).unwrap();
    }
    let policy = StorePolicy {
        denied_apps: ["Mail".into()].into_iter().collect(),
        redact_bodies: true,
        ..StorePolicy::default()
    };
    let changes = store.set_policy(policy).unwrap();
    assert_eq!(changes.events.len(), 3);
    let snapshot = store.list();
    let mut previous_rev = 3;
    for delta in changes.events {
        let rev = match delta {
            Delta::NotificationClosed(c) => c.rev,
            Delta::NotificationReceived(n) => {
                assert_eq!(n.0.body.as_deref(), Some("Hidden"));
                assert!(snapshot.contains(&n.0));
                n.0.rev
            }
        };
        assert_eq!(rev, previous_rev + 1);
        previous_rev = rev;
    }
    assert_eq!(snapshot.len(), 2);
}

#[test]
fn markup_scanner_preserves_comparisons_and_handles_quoted_tag_delimiters() {
    let mut args = NotifyArgs {
        app_name: "App".into(),
        replaces_id: 0,
        app_icon: String::new(),
        summary: "Title".into(),
        body: String::new(),
        actions: vec![],
        hints: BTreeMap::new(),
        expire_timeout: -1,
    };
    for (body, expected) in [
        ("Value < 5 > 3", "Value < 5 > 3"),
        ("<a href=\"x>y\">label</a>", "label"),
        ("<b title='x>y'>é</b>", "é"),
        ("a<!-- comment -->b", "ab"),
        ("a <unfinished", "a <unfinished"),
        ("<&>", "<&>"),
    ] {
        args.body = body.into();
        assert_eq!(args.parse(1).unwrap().input.body.as_deref(), Some(expected));
    }
}

#[test]
fn malformed_policy_is_atomic_for_list_and_all_policy_fields() {
    let mut store = NotificationStore::default();
    store
        .insert(NotificationInput::hermes("kept"), 0.0)
        .unwrap();
    let before = store.list();
    let original_policy = store.policy().clone();
    let mut policy = original_policy.clone();
    policy.dnd = true;
    policy.redact_bodies = true;
    policy.denied_apps.insert("x".repeat(MAX_NAME_BYTES + 1));
    assert!(store.set_policy(policy).is_err());
    assert_eq!(store.policy(), &original_policy);
    assert_eq!(store.list(), before);
}

#[test]
fn server_reset_removes_only_correlated_system_items_without_close_requests() {
    let mut store = NotificationStore::default();
    store.insert(dbus_input(1), 0.0).unwrap();
    store
        .insert(NotificationInput::hermes("kept"), 0.0)
        .unwrap();
    let mut alert = NotificationInput::hermes("alert");
    alert.source = NotificationSource::Alert;
    store.insert(alert, 0.0).unwrap();
    let changes = store.reset_server();
    assert_eq!(changes.events.len(), 1);
    assert!(
        matches!(&changes.events[0], Delta::NotificationClosed(c) if c.id == "d-1" && c.reason == CloseReason::ServerClosed)
    );
    assert!(changes.close_server_ids.is_empty());
    assert_eq!(store.len(), 2);
    assert_eq!(store.reset_server(), Changes::default());
}
