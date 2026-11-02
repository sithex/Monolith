//! Integration tests for the pure clipboard history store.
//!
//! These tests exercise the public API end-to-end (SPEC §3.3 rules): skip
//! reasons, dedupe promotion, echo window expiry, eviction, pin semantics,
//! querying, preview truncation, serde JSON shape snapshots (§5.1–5.2) and
//! persistence round-trips.
//!
//! No filesystem IO, clipboard access, sockets or network are used.

use hermes_island_lib::integrations::clipboard::history::{
    ClipContent, ClipInput, ClipKind, ClipPayload, ClipSummary, Error, HistoryConfig, HistoryStore,
    OfferOutcome, SkipReason,
};

const T0: u64 = 1_000_000;

fn default_cfg() -> HistoryConfig {
    HistoryConfig::default()
}

fn store(cfg: HistoryConfig) -> HistoryStore {
    match HistoryStore::new(cfg) {
        Ok(s) => s,
        Err(e) => panic!("valid config was rejected: {e:?}"),
    }
}

fn text_input(s: &str) -> ClipInput {
    ClipInput {
        content: ClipPayload::Text(s.to_string()),
        mime_types: Vec::new(),
        password_hint: None,
        source_app: None,
    }
}

fn text_input_with_source(s: &str, source: &str) -> ClipInput {
    ClipInput {
        content: ClipPayload::Text(s.to_string()),
        mime_types: Vec::new(),
        password_hint: None,
        source_app: Some(source.to_string()),
    }
}

fn image_input(png: Vec<u8>, width: u32, height: u32) -> ClipInput {
    ClipInput {
        content: ClipPayload::Image { png, width, height },
        mime_types: Vec::new(),
        password_hint: None,
        source_app: None,
    }
}

fn files_input(uris: &[&str]) -> ClipInput {
    ClipInput {
        content: ClipPayload::Files(uris.iter().map(|s| s.to_string()).collect()),
        mime_types: Vec::new(),
        password_hint: None,
        source_app: None,
    }
}

fn added(outcome: OfferOutcome) -> String {
    match outcome {
        OfferOutcome::Added { id } => id,
        other => panic!("expected Added, got {other:?}"),
    }
}

fn reordered(outcome: OfferOutcome) -> String {
    match outcome {
        OfferOutcome::Reordered { id } => id,
        other => panic!("expected Reordered, got {other:?}"),
    }
}

fn skipped(outcome: OfferOutcome) -> SkipReason {
    match outcome {
        OfferOutcome::Skipped(reason) => reason,
        other => panic!("expected Skipped, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[test]
fn config_defaults() {
    let cfg = default_cfg();
    assert_eq!(cfg.capacity, 100);
    assert_eq!(cfg.max_text_bytes, 1024 * 1024);
    assert_eq!(cfg.max_image_bytes, 8 * 1024 * 1024);
    assert_eq!(cfg.max_image_side, 4096);
    assert_eq!(cfg.preview_chars, 280);
    assert!(cfg.deny_patterns.is_empty());
}

#[test]
fn capacity_clamped_low() {
    let cfg = HistoryConfig {
        capacity: 3,
        ..default_cfg()
    }
    .validated()
    .unwrap();
    assert_eq!(cfg.capacity, 10);
}

#[test]
fn capacity_clamped_high() {
    let cfg = HistoryConfig {
        capacity: 600,
        ..default_cfg()
    }
    .validated()
    .unwrap();
    assert_eq!(cfg.capacity, 500);
}

#[test]
fn deny_pattern_too_long_errors() {
    let long = "a".repeat(257);
    let cfg = HistoryConfig {
        deny_patterns: vec![long],
        ..default_cfg()
    };
    let err = HistoryStore::new(cfg).unwrap_err();
    assert!(matches!(err, Error::InvalidConfig(_)));
}

#[test]
fn deny_pattern_too_many_errors() {
    let patterns = (0..33).map(|i| format!("p{i}")).collect();
    let cfg = HistoryConfig {
        deny_patterns: patterns,
        ..default_cfg()
    };
    let err = HistoryStore::new(cfg).unwrap_err();
    assert!(matches!(err, Error::InvalidConfig(_)));
}

#[test]
fn deny_pattern_invalid_regex_errors() {
    let cfg = HistoryConfig {
        deny_patterns: vec!["(".to_string()],
        ..default_cfg()
    };
    let err = HistoryStore::new(cfg).unwrap_err();
    assert!(matches!(err, Error::InvalidConfig(_)));
}

#[test]
fn deny_pattern_empty_errors() {
    let cfg = HistoryConfig {
        deny_patterns: vec![String::new()],
        ..default_cfg()
    };
    let err = HistoryStore::new(cfg).unwrap_err();
    assert!(matches!(err, Error::InvalidConfig(_)));
}

#[test]
fn deny_patterns_valid_accepted() {
    let cfg = HistoryConfig {
        deny_patterns: vec!["^secret".to_string(), r"\d{4}".to_string()],
        ..default_cfg()
    };
    let s = store(cfg);
    assert!(s.is_empty());
}

// ---------------------------------------------------------------------------
// Skip rules
// ---------------------------------------------------------------------------

#[test]
fn paused_skips_all() {
    let mut s = store(default_cfg());
    s.set_paused(true);
    assert!(s.is_paused());
    assert_eq!(
        skipped(s.offer(text_input("hello"), T0)),
        SkipReason::Paused
    );
    assert_eq!(
        skipped(s.offer(image_input(vec![1], 1, 1), T0)),
        SkipReason::Paused
    );
    assert_eq!(
        skipped(s.offer(files_input(&["file:///a"]), T0)),
        SkipReason::Paused
    );
    s.set_paused(false);
    assert!(!s.is_paused());
    assert_eq!(added(s.offer(text_input("hello"), T0)), "c-1");
}

#[test]
fn sensitive_password_hint_secret() {
    let mut s = store(default_cfg());
    let mut input = text_input("hunter2");
    input.password_hint = Some("secret".to_string());
    assert_eq!(skipped(s.offer(input, T0)), SkipReason::Sensitive);
    assert!(s.is_empty());
}

#[test]
fn sensitive_mime_concealed() {
    let mut s = store(default_cfg());
    let mut input = text_input("hidden");
    input.mime_types = vec!["org.nspasteboard.ConcealedType".to_string()];
    assert_eq!(skipped(s.offer(input, T0)), SkipReason::Sensitive);
}

#[test]
fn sensitive_mime_transient() {
    let mut s = store(default_cfg());
    let mut input = text_input("x");
    input.mime_types = vec!["org.nspasteboard.TransientType".to_string()];
    assert_eq!(skipped(s.offer(input, T0)), SkipReason::Sensitive);
}

#[test]
fn sensitive_mime_exclude_processing() {
    let mut s = store(default_cfg());
    let mut input = text_input("x");
    input.mime_types = vec!["ExcludeClipboardContentFromMonitorProcessing".to_string()];
    assert_eq!(skipped(s.offer(input, T0)), SkipReason::Sensitive);
}

#[test]
fn sensitive_mime_case_insensitive_trimmed() {
    let mut s = store(default_cfg());
    let mut input = text_input("x");
    input.mime_types = vec!["  ORG.NSPASTEBOARD.CONCEALEDTYPE ".to_string()];
    assert_eq!(skipped(s.offer(input, T0)), SkipReason::Sensitive);
}

#[test]
fn deny_regex_matches_text() {
    let mut s = store(HistoryConfig {
        deny_patterns: vec!["^secret".to_string()],
        ..default_cfg()
    });
    assert_eq!(
        skipped(s.offer(text_input("secret note"), T0)),
        SkipReason::Denied
    );
    assert_eq!(added(s.offer(text_input("notes"), T0)), "c-1");
}

#[test]
fn deny_regex_ignores_files_and_images() {
    let mut s = store(HistoryConfig {
        deny_patterns: vec!["secret".to_string()],
        ..default_cfg()
    });
    assert_eq!(
        added(s.offer(files_input(&["file:///secret.txt"]), T0)),
        "c-1"
    );
    assert_eq!(added(s.offer(image_input(vec![1], 2, 2), T0)), "c-2");
}

#[test]
fn oversize_text_matching_deny_pattern_returns_too_large() {
    // Size limits are checked before the deny regex: an oversize offer is
    // TooLarge even when its text would also match a deny pattern.
    let cfg = HistoryConfig {
        max_text_bytes: 10,
        deny_patterns: vec!["secret".to_string()],
        ..default_cfg()
    };
    let mut s = store(cfg);
    let input = text_input("secret-ABCDEFGHIJKLM"); // > 10 bytes AND matches "secret"
    assert_eq!(skipped(s.offer(input, T0)), SkipReason::TooLarge);
    assert!(s.is_empty());

    // A small text matching the same pattern is still Denied.
    assert_eq!(
        skipped(s.offer(text_input("secret"), T0)),
        SkipReason::Denied
    );
}

#[test]
fn text_too_large() {
    let mut s = store(HistoryConfig {
        max_text_bytes: 10,
        ..default_cfg()
    });
    assert_eq!(
        skipped(s.offer(text_input("12345678901"), T0)),
        SkipReason::TooLarge
    );
    assert_eq!(added(s.offer(text_input("1234567890"), T0)), "c-1");
}

#[test]
fn image_too_large_bytes() {
    let mut s = store(HistoryConfig {
        max_image_bytes: 4,
        ..default_cfg()
    });
    assert_eq!(
        skipped(s.offer(image_input(vec![0; 5], 1, 1), T0)),
        SkipReason::TooLarge
    );
    assert_eq!(added(s.offer(image_input(vec![0; 4], 1, 1), T0)), "c-1");
}

#[test]
fn image_too_large_side() {
    let mut s = store(default_cfg());
    assert_eq!(
        skipped(s.offer(image_input(vec![0; 8], 4097, 100), T0)),
        SkipReason::TooLarge
    );
    assert_eq!(
        skipped(s.offer(image_input(vec![0; 8], 100, 4097), T0)),
        SkipReason::TooLarge
    );
    // Exactly at the limit is fine.
    assert_eq!(
        added(s.offer(image_input(vec![0; 8], 4096, 4096), T0)),
        "c-1"
    );
}

#[test]
fn text_empty_skipped() {
    let mut s = store(default_cfg());
    assert_eq!(skipped(s.offer(text_input(""), T0)), SkipReason::Empty);
}

#[test]
fn text_whitespace_only_skipped() {
    let mut s = store(default_cfg());
    assert_eq!(
        skipped(s.offer(text_input("   \t\n "), T0)),
        SkipReason::Empty
    );
}

#[test]
fn files_empty_skipped() {
    let mut s = store(default_cfg());
    assert_eq!(skipped(s.offer(files_input(&[]), T0)), SkipReason::Empty);
}

// ---------------------------------------------------------------------------
// Echo guard
// ---------------------------------------------------------------------------

#[test]
fn echo_within_window_skips_and_reorders() {
    let mut s = store(default_cfg());
    let id = added(s.offer(text_input("hello"), T0));
    let hash = HistoryStore::hash_of(&text_input("hello"));
    s.expect_echo(hash, T0 + 100);
    // Inside the window: SkippedEcho and the entry moves to the front.
    assert_eq!(
        skipped(s.offer(text_input("hello"), T0 + 1500)),
        SkipReason::Echo
    );
    let list = s.list(None, 10);
    assert_eq!(list[0].id, id);
}

#[test]
fn echo_consumed_once() {
    let mut s = store(default_cfg());
    added(s.offer(text_input("hello"), T0));
    let hash = HistoryStore::hash_of(&text_input("hello"));
    s.expect_echo(hash, T0 + 100);
    assert_eq!(
        skipped(s.offer(text_input("hello"), T0 + 1500)),
        SkipReason::Echo
    );
    // Second identical offer: the expectation was consumed → plain dedupe.
    assert_eq!(reordered(s.offer(text_input("hello"), T0 + 1600)), "c-1");
}

#[test]
fn echo_expires_exactly_at_2000ms() {
    let mut s = store(default_cfg());
    added(s.offer(text_input("hello"), T0));
    let hash = HistoryStore::hash_of(&text_input("hello"));

    s.expect_echo(hash, T0 + 100);
    // 1999 ms after registration: still inside the window.
    assert_eq!(
        skipped(s.offer(text_input("hello"), T0 + 2099)),
        SkipReason::Echo
    );

    s.expect_echo(hash, T0 + 3000);
    // Exactly 2000 ms after registration: expired → normal dedupe reorder.
    assert_eq!(reordered(s.offer(text_input("hello"), T0 + 5000)), "c-1");
}

#[test]
fn echo_after_delete_consumes_without_reorder() {
    let mut s = store(default_cfg());
    let id = added(s.offer(text_input("hello"), T0));
    let hash = HistoryStore::hash_of(&text_input("hello"));
    s.delete(&id).unwrap();
    s.expect_echo(hash, T0);
    // Entry is gone; the expectation is still consumed, offer is skipped.
    assert_eq!(
        skipped(s.offer(text_input("hello"), T0 + 500)),
        SkipReason::Echo
    );
    assert!(s.is_empty());
}

// ---------------------------------------------------------------------------
// Dedupe + ids
// ---------------------------------------------------------------------------

#[test]
fn dedupe_promotes_keeps_id_and_pin() {
    let mut s = store(default_cfg());
    let id1 = added(s.offer(text_input("alpha"), T0));
    added(s.offer(text_input("beta"), T0 + 100));
    s.pin(&id1, true).unwrap();

    let outcome = s.offer(text_input("alpha"), T0 + 200);
    assert_eq!(reordered(outcome), id1);

    let list = s.list(None, 10);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, id1);
    assert!(list[0].pinned);
    assert_eq!(list[0].bytes, 5); // unchanged, not doubled
}

#[test]
fn dedupe_promotes_to_front() {
    let mut s = store(default_cfg());
    added(s.offer(text_input("a"), T0));
    added(s.offer(text_input("b"), T0 + 10));
    added(s.offer(text_input("c"), T0 + 20));
    reordered(s.offer(text_input("a"), T0 + 30));

    let previews: Vec<String> = s.list(None, 10).into_iter().map(|sm| sm.preview).collect();
    assert_eq!(previews, vec!["a", "c", "b"]);
}

#[test]
fn ids_never_reused_after_delete() {
    let mut s = store(default_cfg());
    let id1 = added(s.offer(text_input("a"), T0));
    let id2 = added(s.offer(text_input("b"), T0 + 10));
    let id3 = added(s.offer(text_input("c"), T0 + 20));
    assert_eq!(
        (id1.as_str(), id2.as_str(), id3.as_str()),
        ("c-1", "c-2", "c-3")
    );
    s.delete(&id2).unwrap();
    let id4 = added(s.offer(text_input("d"), T0 + 30));
    assert_eq!(id4, "c-4");
}

// ---------------------------------------------------------------------------
// Capacity / eviction
// ---------------------------------------------------------------------------

fn small_cfg() -> HistoryConfig {
    HistoryConfig {
        capacity: 10,
        ..default_cfg()
    }
}

#[test]
fn eviction_removes_oldest_unpinned() {
    let mut s = store(small_cfg());
    for i in 0..11 {
        added(s.offer(text_input(&format!("item-{i}")), T0 + i));
    }
    assert_eq!(s.len(), 10);
    let previews: Vec<String> = s.list(None, 100).into_iter().map(|sm| sm.preview).collect();
    // item-0 (oldest) is evicted; newest first.
    assert_eq!(previews[0], "item-10");
    assert!(!previews.contains(&"item-0".to_string()));
}

#[test]
fn eviction_skips_pinned() {
    let mut s = store(small_cfg());
    let oldest = added(s.offer(text_input("oldest"), T0));
    for i in 1..10 {
        added(s.offer(text_input(&format!("item-{i}")), T0 + i));
    }
    s.pin(&oldest, true).unwrap();
    // Full (10 entries, one pinned). Offer a new one → unpinned oldest evicted.
    added(s.offer(text_input("newest"), T0 + 100));

    assert_eq!(s.len(), 10);
    let ids: Vec<String> = s.list(None, 100).into_iter().map(|sm| sm.id).collect();
    assert!(ids.contains(&oldest));
    assert!(!ids.contains(&"c-2".to_string())); // oldest unpinned evicted
    let oldest_summary = s.list(Some("oldest"), 10);
    assert_eq!(oldest_summary.len(), 1);
    assert!(oldest_summary[0].pinned);
}

#[test]
fn all_pinned_full_skipped() {
    let mut s = store(small_cfg());
    for i in 0..10 {
        let id = added(s.offer(text_input(&format!("item-{i}")), T0 + i));
        s.pin(&id, true).unwrap();
    }
    assert_eq!(
        skipped(s.offer(text_input("extra"), T0 + 1000)),
        SkipReason::Full
    );
    assert_eq!(s.len(), 10);
}

#[test]
fn pinned_count_toward_capacity() {
    let mut s = store(small_cfg());
    // 9 unpinned + 1 pinned = full (10).
    for i in 0..9 {
        added(s.offer(text_input(&format!("item-{i}")), T0 + i));
    }
    let pinned = added(s.offer(text_input("pinned"), T0 + 100));
    s.pin(&pinned, true).unwrap();
    // All pinned? No — 9 unpinned exist, so eviction is possible.
    added(s.offer(text_input("new"), T0 + 200));
    assert_eq!(s.len(), 10);
    assert!(s.list(Some("pinned"), 10)[0].pinned);
}

// ---------------------------------------------------------------------------
// pin / delete / clear
// ---------------------------------------------------------------------------

#[test]
fn pin_unknown_id_errors() {
    let mut s = store(default_cfg());
    assert_eq!(s.pin("c-999", true), Err(Error::NotFound));
}

#[test]
fn delete_unknown_id_errors() {
    let mut s = store(default_cfg());
    assert_eq!(s.delete("c-999"), Err(Error::NotFound));
}

#[test]
fn unpin_allows_eviction() {
    let mut s = store(small_cfg());
    let mut ids = Vec::new();
    for i in 0..10 {
        ids.push(added(s.offer(text_input(&format!("item-{i}")), T0 + i)));
    }
    s.pin(&ids[0], true).unwrap();
    s.pin(&ids[0], false).unwrap();
    added(s.offer(text_input("new"), T0 + 1000));
    assert!(!s.list(None, 100).iter().any(|sm| sm.id == ids[0]));
}

#[test]
fn delete_removes_entry() {
    let mut s = store(default_cfg());
    let id = added(s.offer(text_input("x"), T0));
    assert!(s.get(&id).is_some());
    s.delete(&id).unwrap();
    assert!(s.get(&id).is_none());
    assert!(s.is_empty());
}

#[test]
fn clear_keeps_pinned() {
    let mut s = store(default_cfg());
    let id1 = added(s.offer(text_input("a"), T0));
    let id2 = added(s.offer(text_input("b"), T0 + 10));
    s.pin(&id1, true).unwrap();
    let removed = s.clear();
    assert_eq!(removed, 1);
    assert_eq!(s.len(), 1);
    assert!(s.get(&id1).is_some());
    assert!(s.get(&id2).is_none());
}

#[test]
fn clear_returns_removed_count() {
    let mut s = store(default_cfg());
    for i in 0..3 {
        added(s.offer(text_input(&format!("item-{i}")), T0 + i));
    }
    assert_eq!(s.clear(), 3);
    assert!(s.is_empty());
    assert_eq!(s.clear(), 0);
}

// ---------------------------------------------------------------------------
// hash_of
// ---------------------------------------------------------------------------

#[test]
fn hash_of_deterministic_and_distinct() {
    let a = text_input("hello");
    let b = text_input("hello");
    assert_eq!(HistoryStore::hash_of(&a), HistoryStore::hash_of(&b));
    assert_ne!(
        HistoryStore::hash_of(&a),
        HistoryStore::hash_of(&text_input("hellp"))
    );
    // Kind participates in the hash.
    assert_ne!(
        HistoryStore::hash_of(&text_input("1")),
        HistoryStore::hash_of(&files_input(&["1"]))
    );
    // File-list framing: NUL separators distinguish concatenations.
    assert_ne!(
        HistoryStore::hash_of(&files_input(&["ab", "c"])),
        HistoryStore::hash_of(&files_input(&["a", "bc"]))
    );
}

// ---------------------------------------------------------------------------
// list / query
// ---------------------------------------------------------------------------

#[test]
fn list_newest_first() {
    let mut s = store(default_cfg());
    added(s.offer(text_input("a"), T0));
    added(s.offer(text_input("b"), T0 + 10));
    added(s.offer(text_input("c"), T0 + 20));
    let previews: Vec<String> = s.list(None, 10).into_iter().map(|sm| sm.preview).collect();
    assert_eq!(previews, vec!["c", "b", "a"]);
}

#[test]
fn list_query_preview_case_insensitive() {
    let mut s = store(default_cfg());
    added(s.offer(text_input("Hello World"), T0));
    added(s.offer(text_input("goodbye"), T0 + 10));
    let hits = s.list(Some("WORLD"), 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].preview, "Hello World");
}

#[test]
fn list_query_source_app_case_insensitive() {
    let mut s = store(default_cfg());
    added(s.offer(text_input_with_source("text", "Firefox"), T0));
    added(s.offer(text_input_with_source("other", "kitty"), T0 + 10));
    let hits = s.list(Some("fire"), 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_app.as_deref(), Some("Firefox"));
}

#[test]
fn list_limit_caps_results() {
    let mut s = store(default_cfg());
    for i in 0..5 {
        added(s.offer(text_input(&format!("item-{i}")), T0 + i));
    }
    assert_eq!(s.list(None, 2).len(), 2);
    assert_eq!(s.list(None, 0).len(), 0);
}

#[test]
fn list_query_no_match_empty() {
    let mut s = store(default_cfg());
    added(s.offer(text_input("alpha"), T0));
    assert!(s.list(Some("zzz"), 10).is_empty());
    assert!(s.list(Some(""), 10).len() == 1); // empty needle matches everything
}

// ---------------------------------------------------------------------------
// get
// ---------------------------------------------------------------------------

#[test]
fn get_text() {
    let mut s = store(default_cfg());
    let id = added(s.offer(text_input("hello"), T0));
    assert_eq!(
        s.get(&id),
        Some(ClipContent::Text {
            text: "hello".to_string()
        })
    );
    assert_eq!(s.get("c-999"), None);
}

#[test]
fn get_image_data_url() {
    let mut s = store(default_cfg());
    let png = vec![1u8, 2, 3];
    let id = added(s.offer(image_input(png.clone(), 640, 480), T0));
    let content = s.get(&id).unwrap();
    match content {
        ClipContent::Image {
            png_data_url,
            width,
            height,
        } => {
            // base64([1,2,3]) == "AQID"
            assert_eq!(png_data_url, "data:image/png;base64,AQID");
            assert_eq!((width, height), (640, 480));
        }
        other => panic!("expected image, got {other:?}"),
    }
}

#[test]
fn get_files() {
    let mut s = store(default_cfg());
    let id = added(s.offer(files_input(&["file:///a.txt", "file:///b.txt"]), T0));
    assert_eq!(
        s.get(&id),
        Some(ClipContent::Files {
            uris: vec!["file:///a.txt".to_string(), "file:///b.txt".to_string()]
        })
    );
}

// ---------------------------------------------------------------------------
// Summary fields
// ---------------------------------------------------------------------------

#[test]
fn summary_fields_text() {
    let mut s = store(default_cfg());
    let _ = added(s.offer(text_input_with_source("héllo", "app"), T0));
    let sum = &s.list(None, 1)[0];
    assert_eq!(sum.kind, ClipKind::Text);
    assert_eq!(sum.copied_at, T0 as f64);
    assert_eq!(sum.source_app.as_deref(), Some("app"));
    // UTF-8 byte length of "héllo" = 1 + 2 + 3 = 6.
    assert_eq!(sum.bytes, 6);
    assert!(!sum.pinned);
}

#[test]
fn summary_bytes_files_sum() {
    let mut s = store(default_cfg());
    let _ = added(s.offer(files_input(&["ab", "cd"]), T0));
    let sum = &s.list(None, 1)[0];
    assert_eq!(sum.kind, ClipKind::Files);
    assert_eq!(sum.bytes, 4);
}

#[test]
fn summary_image_bytes() {
    let mut s = store(default_cfg());
    let _ = added(s.offer(image_input(vec![0; 16], 10, 10), T0));
    let sum = &s.list(None, 1)[0];
    assert_eq!(sum.kind, ClipKind::Image);
    assert_eq!(sum.bytes, 16);
}

// ---------------------------------------------------------------------------
// Serde JSON shape snapshots (§5.1–5.2)
// ---------------------------------------------------------------------------

#[test]
fn clip_summary_json_shape() {
    let summary = ClipSummary {
        id: "c-1".to_string(),
        kind: ClipKind::Text,
        preview: "hi".to_string(),
        bytes: 2,
        pinned: false,
        copied_at: 1234.5,
        source_app: Some("firefox".to_string()),
    };
    let value = serde_json::to_value(&summary).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "id": "c-1",
            "kind": "text",
            "preview": "hi",
            "bytes": 2,
            "pinned": false,
            "copiedAt": 1234.5,
            "sourceApp": "firefox",
        })
    );
}

#[test]
fn clip_summary_json_kinds() {
    assert_eq!(
        serde_json::to_value(ClipKind::Text).unwrap(),
        serde_json::json!("text")
    );
    assert_eq!(
        serde_json::to_value(ClipKind::Image).unwrap(),
        serde_json::json!("image")
    );
    assert_eq!(
        serde_json::to_value(ClipKind::Files).unwrap(),
        serde_json::json!("files")
    );
}

#[test]
fn clip_content_text_json_shape() {
    let content = ClipContent::Text {
        text: "hi".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&content).unwrap(),
        serde_json::json!({ "type": "text", "text": "hi" })
    );
}

#[test]
fn clip_content_image_json_shape() {
    let content = ClipContent::Image {
        png_data_url: "data:image/png;base64,AQID".to_string(),
        width: 640,
        height: 480,
    };
    assert_eq!(
        serde_json::to_value(&content).unwrap(),
        serde_json::json!({
            "type": "image",
            "pngDataUrl": "data:image/png;base64,AQID",
            "width": 640,
            "height": 480,
        })
    );
}

#[test]
fn clip_content_files_json_shape() {
    let content = ClipContent::Files {
        uris: vec!["file:///a".to_string()],
    };
    assert_eq!(
        serde_json::to_value(&content).unwrap(),
        serde_json::json!({ "type": "files", "uris": ["file:///a"] })
    );
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

#[test]
fn persist_round_trip_preserves_all() {
    let mut s = store(default_cfg());
    let id1 = added(s.offer(text_input_with_source("hello", "app1"), T0));
    let id2 = added(s.offer(image_input(vec![9; 8], 32, 32), T0 + 10));
    let id3 = added(s.offer(files_input(&["file:///x.txt"]), T0 + 20));
    s.pin(&id1, true).unwrap();
    s.delete(&id2).unwrap();

    let json = s.to_persisted(true).unwrap();
    let loaded = HistoryStore::from_persisted(default_cfg(), &json).unwrap();

    assert_eq!(loaded.len(), 2);
    // Order preserved (newest first).
    let list = loaded.list(None, 10);
    assert_eq!(list[0].id, id3);
    assert_eq!(list[1].id, id1);
    assert!(list[1].pinned);
    assert_eq!(list[1].source_app.as_deref(), Some("app1"));
    assert_eq!(
        loaded.get(&id1),
        Some(ClipContent::Text {
            text: "hello".to_string()
        })
    );
    assert_eq!(
        loaded.get(&id3),
        Some(ClipContent::Files {
            uris: vec!["file:///x.txt".to_string()]
        })
    );
    assert!(loaded.get(&id2).is_none());
}

#[test]
fn persist_pinned_only_excludes_unpinned() {
    let mut s = store(default_cfg());
    let pinned_id = added(s.offer(text_input("pinned"), T0));
    added(s.offer(text_input("a"), T0 + 10));
    added(s.offer(text_input("b"), T0 + 20));
    s.pin(&pinned_id, true).unwrap();

    // Default (include_unpinned = false): pinned entries only.
    let json = s.to_persisted(false).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let entries = match value["entries"].as_array() {
        Some(a) => a,
        None => panic!("entries array missing"),
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["id"], serde_json::json!("c-1"));
    assert_eq!(entries[0]["pinned"], serde_json::json!(true));

    // Loading the pinned-only snapshot yields exactly the pinned entry.
    let loaded = HistoryStore::from_persisted(default_cfg(), &json).unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(loaded.get(&pinned_id).is_some());
    assert!(loaded.get("c-2").is_none());
}

#[test]
fn persist_include_unpinned_round_trips_everything() {
    let mut s = store(default_cfg());
    let id1 = added(s.offer(text_input_with_source("alpha", "app"), T0));
    let id2 = added(s.offer(image_input(vec![7; 4], 16, 16), T0 + 10));
    let id3 = added(s.offer(files_input(&["file:///z.txt"]), T0 + 20));
    s.pin(&id1, true).unwrap();

    let json = s.to_persisted(true).unwrap();
    let loaded = HistoryStore::from_persisted(default_cfg(), &json).unwrap();
    assert_eq!(loaded.len(), 3);

    let list = loaded.list(None, 10);
    assert_eq!(list[0].id, id3);
    assert_eq!(list[1].id, id2);
    assert_eq!(list[2].id, id1);
    assert!(list[2].pinned);
    assert_eq!(
        loaded.get(&id1),
        Some(ClipContent::Text {
            text: "alpha".to_string()
        })
    );
    assert_eq!(
        loaded.get(&id3),
        Some(ClipContent::Files {
            uris: vec!["file:///z.txt".to_string()]
        })
    );
}

#[test]
fn persist_next_id_resumes() {
    let mut s = store(default_cfg());
    added(s.offer(text_input("a"), T0));
    added(s.offer(text_input("b"), T0 + 10));
    added(s.offer(text_input("c"), T0 + 20));
    let json = s.to_persisted(true).unwrap();
    let mut loaded = HistoryStore::from_persisted(default_cfg(), &json).unwrap();
    let id = added(loaded.offer(text_input("d"), T0 + 30));
    assert_eq!(id, "c-4"); // no id reuse across the round-trip
}

#[test]
fn persist_id_resume_guard_skips_existing_suffix() {
    // The stored file claims nextId=2 but contains entries whose ids consume
    // suffixes up to c-7 — including c-2, which a naive counter would reuse.
    // Loading must resume past the max existing suffix so a new offer gets a
    // collision-free id (c-8, not c-2).
    let json = serde_json::json!({
        "version": 1,
        "nextId": 2,
        "entries": [
            { "id": "c-2", "payload": { "kind": "text", "text": "alpha" }, "pinned": false, "copiedAtMs": 1.0, "sourceApp": null },
            { "id": "c-7", "payload": { "kind": "text", "text": "beta" }, "pinned": false, "copiedAtMs": 2.0, "sourceApp": null },
        ],
    });
    let mut loaded = HistoryStore::from_persisted(default_cfg(), &json.to_string()).unwrap();
    let id = added(loaded.offer(text_input("gamma"), T0 + 30));
    assert_eq!(id, "c-8");

    let ids: Vec<String> = loaded.list(None, 10).into_iter().map(|sm| sm.id).collect();
    assert_eq!(ids.len(), 3);
    let unique: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len()); // no duplicated ids
}

#[test]
fn persist_dedups_unpinned_duplicates() {
    // Two stored entries with identical content (same hash), both unpinned:
    // the newest (first in the file) survives, the duplicate is dropped.
    let json = serde_json::json!({
        "version": 1,
        "nextId": 3,
        "entries": [
            { "id": "c-1", "payload": { "kind": "text", "text": "dup" }, "pinned": false, "copiedAtMs": 2.0, "sourceApp": null },
            { "id": "c-2", "payload": { "kind": "text", "text": "dup" }, "pinned": false, "copiedAtMs": 1.0, "sourceApp": null },
        ],
    });
    let loaded = HistoryStore::from_persisted(default_cfg(), &json.to_string()).unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(loaded.get("c-1").is_some());
    assert!(loaded.get("c-2").is_none());
    let sum = &loaded.list(None, 10)[0];
    assert_eq!(sum.id, "c-1");
    assert_eq!(sum.copied_at, 2.0); // newest kept
    assert!(!sum.pinned);
}

#[test]
fn persist_dedups_carries_pin_from_dropped() {
    // The newest duplicate is unpinned; the older duplicate is pinned. The
    // duplicate is still dropped, but its pin transfers to the kept entry.
    let json = serde_json::json!({
        "version": 1,
        "nextId": 3,
        "entries": [
            { "id": "c-1", "payload": { "kind": "text", "text": "dup" }, "pinned": false, "copiedAtMs": 2.0, "sourceApp": null },
            { "id": "c-2", "payload": { "kind": "text", "text": "dup" }, "pinned": true, "copiedAtMs": 1.0, "sourceApp": null },
        ],
    });
    let loaded = HistoryStore::from_persisted(default_cfg(), &json.to_string()).unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(loaded.get("c-1").is_some());
    assert!(loaded.get("c-2").is_none());
    let sum = &loaded.list(None, 10)[0];
    assert_eq!(sum.id, "c-1");
    assert!(sum.pinned); // pin carried from the dropped duplicate
}

#[test]
fn persist_rejects_unknown_version() {
    let json = serde_json::json!({ "version": 99, "nextId": 1, "entries": [] });
    let err = HistoryStore::from_persisted(default_cfg(), &json.to_string()).unwrap_err();
    assert_eq!(err, Error::UnknownVersion(99));
}

#[test]
fn persist_corrupt_json_errors() {
    let err = HistoryStore::from_persisted(default_cfg(), "not json").unwrap_err();
    assert!(matches!(err, Error::Corrupt(_)));
}

#[test]
fn persist_caps_oversize_text() {
    let mut s = store(default_cfg());
    let big = "x".repeat(1000);
    let id = added(s.offer(text_input(&big), T0));
    let json = s.to_persisted(true).unwrap();

    let cfg = HistoryConfig {
        max_text_bytes: 16,
        ..default_cfg()
    };
    let loaded = HistoryStore::from_persisted(cfg, &json).unwrap();
    let content = loaded.get(&id).unwrap();
    match content {
        ClipContent::Text { text } => {
            assert!(text.len() <= 16);
            assert!(text.is_char_boundary(text.len()));
        }
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn persist_drops_oversize_image() {
    // Build the store with a permissive config so the oversize image is
    // accepted and persisted…
    let permissive = HistoryConfig {
        max_image_side: 10_000,
        ..default_cfg()
    };
    let mut s = store(permissive);
    let id = added(s.offer(image_input(vec![0; 100], 5000, 100), T0));
    let json = s.to_persisted(true).unwrap();

    // …then load with the strict default config: the image must be dropped.
    let loaded = HistoryStore::from_persisted(default_cfg(), &json).unwrap();
    assert!(loaded.get(&id).is_none());
    assert!(loaded.is_empty());
}

#[test]
fn persist_caps_to_capacity() {
    let mut s = store(default_cfg());
    for i in 0..15 {
        added(s.offer(text_input(&format!("item-{i}")), T0 + i));
    }
    let json = s.to_persisted(true).unwrap();
    let loaded = HistoryStore::from_persisted(small_cfg(), &json).unwrap();
    assert_eq!(loaded.len(), 10);
    let previews: Vec<String> = loaded
        .list(None, 100)
        .into_iter()
        .map(|sm| sm.preview)
        .collect();
    // The 5 oldest (item-0..item-4) were evicted; newest first.
    assert_eq!(previews[0], "item-14");
    assert!(!previews.contains(&"item-0".to_string()));
}

#[test]
fn persist_drops_denied_text() {
    let mut s = store(default_cfg());
    let id = added(s.offer(text_input("secret stuff"), T0));
    let json = s.to_persisted(true).unwrap();
    let cfg = HistoryConfig {
        deny_patterns: vec!["secret".to_string()],
        ..default_cfg()
    };
    let loaded = HistoryStore::from_persisted(cfg, &json).unwrap();
    assert!(loaded.get(&id).is_none());
}

#[test]
fn persist_reassigns_duplicate_ids() {
    let json = serde_json::json!({
        "version": 1,
        "nextId": 1,
        "entries": [
            { "id": "c-1", "payload": { "kind": "text", "text": "a" }, "pinned": false, "copiedAtMs": 1.0, "sourceApp": null },
            { "id": "c-1", "payload": { "kind": "text", "text": "b" }, "pinned": false, "copiedAtMs": 2.0, "sourceApp": null },
        ],
    });
    let mut loaded = HistoryStore::from_persisted(default_cfg(), &json.to_string()).unwrap();
    let ids: Vec<String> = loaded.list(None, 10).into_iter().map(|sm| sm.id).collect();
    assert_ne!(ids[0], ids[1]);
    // New id is beyond both ("c-1" existed → next must be ≥ "c-2").
    let next = added(loaded.offer(text_input("c"), T0));
    assert_ne!(next, "c-1");
    assert!(!ids.contains(&next));
}
