use notification_store::ingest::*;
use notification_store::*;
use serde_json::json;

#[test]
fn spec_notify_progress_clear_roundtrip_and_typed_replies() {
    let mut session = IngestSession::new(0.0).unwrap();
    let mut store = NotificationStore::default();
    let notify = br#"{"v":1,"op":"notify","title":"Build finished","body":"cargo test: 41 passed","level":"success","ttlMs":6000,"source":"hermes:cron","dedupeKey":"build-41"}"#;
    let result = session.handle(&mut store, notify, 0.0);
    assert!(result.reply.is_ok());
    assert_eq!(result.reply.id(), Some("h-1"));
    assert_eq!(
        result.reply.to_line().unwrap(),
        "{\"ok\":true,\"id\":\"h-1\"}\n"
    );
    assert_eq!(store.list()[0].app_name, "hermes:cron");
    assert_eq!(
        store.list()[0].body.as_deref(),
        Some("cargo test: 41 passed")
    );
    let progress = br#"{"v":1,"op":"progress","dedupeKey":"dl-1","title":"Downloading model","progress":0.42}"#;
    let result = session.handle(&mut store, progress, 1.0);
    assert!(result.reply.is_ok());
    assert_eq!(store.list()[0].progress, Some(0.42));
    assert_eq!(store.len(), 2);
    store.expire(10_000.0).unwrap();
    assert_eq!(store.len(), 1); // in-flight progress is resident
    let clear = br#"{"v":1,"op":"clear","dedupeKey":"dl-1"}"#;
    let result = session.handle(&mut store, clear, 10_000.0);
    assert!(result.reply.is_ok());
    assert_eq!(result.changes.events.len(), 1);
    assert!(!result.changes.toast);
    assert!(store.is_empty());
    let result = session.handle(&mut store, clear, 10_001.0);
    assert_eq!(result.reply.to_line().unwrap(), "{\"ok\":true}\n");
}

#[test]
fn progress_updates_same_id_moves_front_and_completion_expires() {
    let mut store = NotificationStore::default();
    parse_line(br#"{"v":1,"op":"progress","dedupeKey":"x","title":"a","progress":0}"#)
        .unwrap()
        .apply(&mut store, 0.0)
        .unwrap();
    store
        .insert(NotificationInput::hermes("other"), 0.0)
        .unwrap();
    let changes =
        parse_line(br#"{"v":1,"op":"progress","dedupeKey":"x","title":"done","progress":1}"#)
            .unwrap()
            .apply(&mut store, 10.0)
            .unwrap();
    assert_eq!(changes.id.as_deref(), Some("h-1"));
    assert_eq!(store.list()[0].title, "done");
    assert_eq!(store.list()[0].progress, Some(1.0));
    store.expire(6009.0).unwrap();
    assert_eq!(store.len(), 1);
    store.expire(6010.0).unwrap();
    assert!(store.is_empty());
}

#[test]
fn line_cap_exact_boundary_counts_bytes_excluding_lf_or_crlf() {
    let base = r#"{"v":1,"op":"notify","title":"é"}"#;
    let mut exact = base.as_bytes().to_vec();
    exact.resize(MAX_LINE_BYTES, b' ');
    assert_eq!(exact.len(), 16384);
    assert!(parse_line(&exact).is_ok());
    let mut lf = exact.clone();
    lf.push(b'\n');
    assert!(parse_line(&lf).is_ok());
    let mut crlf = exact.clone();
    crlf.extend_from_slice(b"\r\n");
    assert!(parse_line(&crlf).is_ok());
    exact.push(b' ');
    assert!(parse_line(&exact).is_err());
    exact.push(b'\n');
    assert!(parse_line(&exact).is_err());
}

#[test]
fn single_line_framing_rejects_multiple_json_empty_invalid_utf8_and_embedded_newline() {
    for bad in [
        b"".as_slice(),
        b"\n",
        b"{}\n{}\n",
        b"{\n}\n",
        b"{}\r",
        b"\xff",
        br#"{"v":1,"op":"notify","title":"a"} {}"#,
    ] {
        assert!(parse_line(bad).is_err());
    }
    assert!(parse_line(b"{\"v\":1,\"op\":\"notify\",\"title\":\"a\\nb\"}\n").is_ok());
}

#[test]
fn strict_protocol_rejects_unknown_fields_aliases_cross_op_fields_arrays_and_duplicates() {
    for bad in [
        r#"{"v":1,"op":"notify","title":"a","unknown":true}"#,
        r#"{"v":1,"op":"notify","title":"a","ttl_ms":1}"#,
        r#"{"v":1,"op":"notify","title":"a","dedupe_key":"k"}"#,
        r#"{"v":1,"op":"clear","dedupeKey":"a","title":"a"}"#,
        r#"{"v":1,"op":"progress","dedupeKey":"a","title":"a","progress":0.5,"body":"x"}"#,
        r#"{"v":1,"op":"notify","title":"a","progress":0.5}"#,
        r#"{"v":1,"op":"notify","title":"a","title":"b"}"#,
        r#"{"v":1,"v":1,"op":"clear","dedupeKey":"a"}"#,
        r#"{"v":1,"op":"clear","op":"clear","dedupeKey":"a"}"#,
        r#"[1,"notify","a"]"#,
        r#"null"#,
        r#"{"v":1,"op":"Notify","title":"a"}"#,
    ] {
        assert!(parse_line(bad.as_bytes()).is_err(), "{bad}");
    }
}

#[test]
fn invalid_versions_required_fields_ranges_and_types_fail_before_mutation() {
    for value in [
        json!({"v":2,"op":"notify","title":"a"}),
        json!({"v":-1,"op":"notify","title":"a"}),
        json!({"v":1.5,"op":"notify","title":"a"}),
        json!({"v":1,"op":"notify"}),
        json!({"v":1,"op":"notify","title":"  "}),
        json!({"v":1,"op":"notify","title":7}),
        json!({"v":1,"op":"notify","title":"a","level":"bad"}),
        json!({"v":1,"op":"notify","title":"a","ttlMs":-1}),
        json!({"v":1,"op":"notify","title":"a","ttlMs":MAX_TTL_MS+1.0}),
        json!({"v":1,"op":"notify","title":"a","source":""}),
        json!({"v":1,"op":"notify","title":"a","dedupeKey":""}),
        json!({"v":1,"op":"notify","title":"a","body":"x".repeat(MAX_BODY_BYTES+1)}),
        json!({"v":1,"op":"notify","title":"é".repeat(MAX_TITLE_BYTES/2+1)}),
        json!({"v":1,"op":"progress","title":"a","dedupeKey":"k","progress":1.01}),
        json!({"v":1,"op":"progress","title":"a","dedupeKey":"k","progress":-0.01}),
        json!({"v":1,"op":"progress","title":"a","dedupeKey":"k","progress":null}),
        json!({"v":1,"op":"progress","dedupeKey":"k","progress":0.5}),
        json!({"v":1,"op":"clear","dedupeKey":""}),
    ] {
        let mut session = IngestSession::new(0.0).unwrap();
        let mut store = NotificationStore::default();
        let result = session.handle(&mut store, value.to_string().as_bytes(), 0.0);
        assert!(!result.reply.is_ok(), "{value}");
        assert_eq!(result.reply.error().unwrap().code, ErrorCode::Invalid);
        assert_eq!(result.changes, Changes::default());
        assert!(store.is_empty());
    }
}

#[test]
fn typed_errors_hide_request_contents_and_output_matches_snapshot() {
    let mut session = IngestSession::new(0.0).unwrap();
    let result = session.handle(
        &mut NotificationStore::default(),
        br#"{"v":1,"op":"notify","SECRET":"sensitive"}"#,
        0.0,
    );
    assert_eq!(
        serde_json::to_value(&result.reply).unwrap(),
        json!({"ok":false,"error":{"code":"invalid","message":"invalid request shape or JSON"}})
    );
    assert!(!result.reply.to_line().unwrap().contains("SECRET"));
    assert!(result.reply.to_line().unwrap().ends_with('\n'));
}

#[test]
fn rate_limiter_refills_fractionally_at_twenty_per_second_and_caps_burst() {
    let mut limiter = RateLimiter::new(100.0).unwrap();
    for _ in 0..20 {
        assert!(limiter.check(100.0).unwrap());
    }
    assert!(!limiter.check(100.0).unwrap());
    assert!(!limiter.check(149.999).unwrap());
    assert!(limiter.check(150.0).unwrap());
    assert!(!limiter.check(150.0).unwrap());
    for _ in 0..20 {
        assert!(limiter.check(1150.0).unwrap());
    }
    assert!(!limiter.check(1150.0).unwrap());
    for _ in 0..20 {
        assert!(limiter.check(1e12).unwrap());
    }
    assert!(!limiter.check(1e12).unwrap());
}

#[test]
fn time_rewinds_and_nonfinite_values_do_not_mint_tokens_or_mutate_store() {
    let mut limiter = RateLimiter::new(1000.0).unwrap();
    for _ in 0..20 {
        assert!(limiter.check(1000.0).unwrap());
    }
    assert!(!limiter.check(0.0).unwrap());
    assert!(!limiter.check(1000.0).unwrap());
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert!(RateLimiter::new(bad).is_err());
        assert!(limiter.check(bad).is_err());
        let request = Request::Progress {
            v: 1,
            dedupe_key: "k".into(),
            title: "a".into(),
            progress: bad,
        };
        assert!(request
            .apply(&mut NotificationStore::default(), 0.0)
            .is_err());
    }
    assert!(limiter.check(1050.0).unwrap());
}

#[test]
fn malformed_attempts_consume_rate_limit_and_connections_are_independent() {
    let mut session = IngestSession::new(0.0).unwrap();
    let mut store = NotificationStore::default();
    for _ in 0..20 {
        assert_eq!(
            session
                .handle(&mut store, b"invalid", 0.0)
                .reply
                .error()
                .unwrap()
                .code,
            ErrorCode::Invalid
        );
    }
    let valid = br#"{"v":1,"op":"notify","title":"a"}"#;
    let denied = session.handle(&mut store, valid, 0.0);
    assert_eq!(
        serde_json::to_value(denied.reply).unwrap(),
        json!({"ok":false,"error":{"code":"rateLimited","message":"rate limit exceeded"}})
    );
    assert!(store.is_empty());
    let mut independent = IngestSession::new(0.0).unwrap();
    assert!(independent.handle(&mut store, valid, 0.0).reply.is_ok());
    assert!(session.handle(&mut store, valid, 50.0).reply.is_ok());
    assert_eq!(MAX_CONNECTIONS, 8);
    assert_eq!(IDLE_TIMEOUT_MS, 10_000.0);
}

#[test]
fn levels_mapping_filters_and_clear_key_are_source_scoped() {
    let mut store = NotificationStore::default();
    for (level, urgency) in [
        ("info", Urgency::Normal),
        ("success", Urgency::Normal),
        ("warning", Urgency::Normal),
        ("error", Urgency::Critical),
    ] {
        let request = json!({"v":1,"op":"notify","title":"a","level":level});
        parse_line(request.to_string().as_bytes())
            .unwrap()
            .apply(&mut store, 0.0)
            .unwrap();
        assert_eq!(store.list()[0].urgency, urgency);
    }
    let mut session = IngestSession::new(0.0).unwrap();
    let result = session.handle(
        &mut store,
        br#"{"v":1,"op":"notify","title":"a","source":"Hermes Island"}"#,
        0.0,
    );
    assert_eq!(result.reply.error().unwrap().code, ErrorCode::Filtered);
    assert_eq!(store.len(), 4);
    let mut policy = store.policy().clone();
    policy.denied_apps.insert("blocked".into());
    store.set_policy(policy).unwrap();
    assert_eq!(
        session
            .handle(
                &mut store,
                br#"{"v":1,"op":"notify","title":"a","source":"blocked"}"#,
                0.0
            )
            .reply
            .error()
            .unwrap()
            .code,
        ErrorCode::Filtered
    );
    let mut a = NotificationInput::hermes("a");
    a.dedupe_key = Some("key".into());
    let id = store.insert(a, 0.0).unwrap().id.unwrap();
    let result = session.handle(
        &mut store,
        br#"{"v":1,"op":"clear","dedupeKey":"key"}"#,
        0.0,
    );
    assert_eq!(result.reply.id(), Some(id.as_str()));
    assert_eq!(store.len(), 4);
}
