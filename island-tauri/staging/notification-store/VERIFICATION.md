# VERIFICATION — notification-store crate

Location: `island-tauri/staging/notification-store/`.
Scope: pure notification store, passive D-Bus correlation/decoded argument
parsing, and Hermes NDJSON protocol/token bucket. Only serde + serde_json;
unsafe forbidden; no IO, clocks, threads, sockets, or platform bindings in the
library. All project edits and generated evidence are confined to this crate.
No other crate, SPEC, skills or memory edited; no commit/push or process kill.

## Gate commands — final state

First lockfile generation preceded compilation/testing:

```text
$ nice -n 10 cargo generate-lockfile
    Updating crates.io index
     Locking 11 packages to highest compatible versions
```

Real output from `python3 scripts/verify.py gates`, AFTER all ten mutation
restorations. 43 passing tests (1 unit + 42 integration), none failed/ignored.

```text
$ nice -n 10 cargo fmt
exit=0

$ nice -n 10 cargo fmt --check
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking notification-store v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/notification-store)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s
exit=0

$ nice -n 10 cargo test --locked
   Compiling notification-store v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/notification-store)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.30s
     Running unittests src/lib.rs (target/debug/deps/notification_store-0a4db736f2562be1)

running 1 test
test store::tests::wrapping_ids_skip_live_collisions_and_revisions_wrap ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/dbus.rs (target/debug/deps/dbus-077bd683e309fd91)

running 10 tests
test correlation_is_per_sender_and_unmatched_returns_never_create_items ... ok
test body_is_plain_text_including_malformed_unicode_and_once_decoded_entities ... ok
test duplicate_serial_refreshes_payload_and_reset_forgets_previous_owner ... ok
test invalid_calls_and_zero_server_id_do_not_destroy_pending_match ... ok
test legacy_hint_names_and_precedence_are_explicit_not_serde_aliases ... ok
test image_hint_geometry_and_bytes_are_bounded_without_encoding_or_io ... ok
test notify_rejects_wrong_hint_types_invalid_urgency_actions_timeouts_and_hint_count ... ok
test notify_string_caps_count_utf8_bytes_before_markup_stripping ... ok
test notify_tuple_parses_every_listed_hint_actions_and_markup ... ok
test lru_evicts_least_recent_serial_and_touch_refreshes ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/edge_cases.rs (target/debug/deps/edge_cases-229e97a54d3cec3e)

running 6 tests
test conflicting_replaces_and_returned_ids_never_leave_duplicate_or_stale_ids ... ok
test malformed_policy_is_atomic_for_list_and_all_policy_fields ... ok
test markup_scanner_preserves_comparisons_and_handles_quoted_tag_delimiters ... ok
test privacy_batches_emit_in_revision_order_and_payloads_equal_current_snapshot ... ok
test replacement_lifetime_and_transient_transitions_replace_old_metadata ... ok
test server_reset_removes_only_correlated_system_items_without_close_requests ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/ingest.rs (target/debug/deps/ingest-234767a35301fd88)

running 11 tests
test levels_mapping_filters_and_clear_key_are_source_scoped ... ok
test malformed_attempts_consume_rate_limit_and_connections_are_independent ... ok
test rate_limiter_refills_fractionally_at_twenty_per_second_and_caps_burst ... ok
test progress_updates_same_id_moves_front_and_completion_expires ... ok
test spec_notify_progress_clear_roundtrip_and_typed_replies ... ok
test single_line_framing_rejects_multiple_json_empty_invalid_utf8_and_embedded_newline ... ok
test time_rewinds_and_nonfinite_values_do_not_mint_tokens_or_mutate_store ... ok
test strict_protocol_rejects_unknown_fields_aliases_cross_op_fields_arrays_and_duplicates ... ok
test typed_errors_hide_request_contents_and_output_matches_snapshot ... ok
test invalid_versions_required_fields_ranges_and_types_fail_before_mutation ... ok
test line_cap_exact_boundary_counts_bytes_excluding_lf_or_crlf ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/store.rs (target/debug/deps/store-d91b742185c79031)

running 15 tests
test changed_returned_server_id_closes_superseded_local_identity ... ok
test dedupe_moves_to_front_keeps_id_updates_fields_and_refreshes_ttl ... ok
test dismiss_clear_server_close_are_idempotent_and_request_only_owned_closes ... ok
test dnd_stores_and_suppresses_even_critical_toasts_low_is_always_silent ... ok
test invalid_numeric_and_oversized_inputs_are_atomic ... ok
test loop_guard_and_deny_list_drop_without_deltas_or_id_consumption ... ok
test emitted_events_json_shape_snapshot ... ok
test missing_dedupe_keys_do_not_coalesce_and_key_bytes_are_literal ... ok
test policy_changes_scrub_existing_bodies_filter_apps_and_return_deltas ... ok
test redaction_is_applied_before_storage_and_emission ... ok
test replaces_id_and_server_id_updates_preserve_server_identity ... ok
test ttl_boundaries_zero_resident_and_critical_transient_policy ... ok
test validation_covers_actions_icons_identity_and_exact_string_caps ... ok
test dedupe_at_capacity_does_not_evict_an_unrelated_entry ... ok
test ring_evicts_oldest_and_reports_closed_before_received ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests notification_store

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

Gate/mutation build processes always use `nice -n 10`. One informational
`cargo --version` query was initially run without nice; it did not compile or
change project artifacts and was subsequently repeated with nice. Toolchain:
rustc 1.99.1 (58cf8f9c3 2026-10-13), cargo 1.99.1 (5f94df478 2026-08-27).

## Mutation checks (actual execution)

All ten compiled and failed their pinned runtime tests with exit 101. Every
restoration diff was empty with exit 0. Table names one actual failing test per
mutation; complete failing-name lists and traces are in mutations.json / M*.log.

| ID | Deliberate breakage | Pinned actual failing test | Test exit | Restored diff |
|---|---|---|---|---|
| M1 | Evict newest instead of oldest | `ring_evicts_oldest_and_reports_closed_before_received` | 101 | empty, exit 0 |
| M2 | Append inserts/updates at back | `dedupe_moves_to_front_keeps_id_updates_fields_and_refreshes_ttl` | 101 | empty, exit 0 |
| M3 | Remove own-app guard | `loop_guard_and_deny_list_drop_without_deltas_or_id_consumption` | 101 | empty, exit 0 |
| M4 | Ignore DND for toasts | `dnd_stores_and_suppresses_even_critical_toasts_low_is_always_silent` | 101 | empty, exit 0 |
| M5 | Expire only after boundary | `ttl_boundaries_zero_resident_and_critical_transient_policy` | 101 | empty, exit 0 |
| M6 | Evict newest serial, not LRU | `lru_evicts_least_recent_serial_and_touch_refreshes` | 101 | empty, exit 0 |
| M7 | Reject exactly 16KiB | `line_cap_exact_boundary_counts_bytes_excluding_lf_or_crlf` | 101 | empty, exit 0 |
| M8 | Refill at half rate | `rate_limiter_refills_fractionally_at_twenty_per_second_and_caps_burst` | 101 | empty, exit 0 |
| M9 | Replace Hidden with Visible in redaction | `redaction_is_applied_before_storage_and_emission` | 101 | empty, exit 0 |
| M10 | Ignore caller identity in reply matching | `correlation_is_per_sender_and_unmatched_returns_never_create_items` | 101 | empty, exit 0 |

Actual helper summary:

```text
10 compiling mutations killed; every restoration diff empty.
```

An additional full-tree comparison after final gates and programmatic result
parsing returned:

```text
$ diff -r --exclude=target target/verification/pristine .
(empty output)
passing_test_counts= [1, 10, 6, 11, 15, 0] total= 43
mutations= 10 all_runtime_killed= True all_identical_restorations= True
```

Usage checks ran at start and throughout; last observed codex Session was 84%,
below the 95% stop threshold. A read-only audit identified quoted-delimiter /
comparison-sign markup loss and missing transition/privacy batch coverage; both
were addressed and verified before this mutation belt.

Method: `python3 scripts/verify.py mutations` copies the entire crate except
`target/` to `target/verification/pristine/`. Each mutation runs separately
with `nice -n 10 cargo test --locked --no-fail-fast`; a compile failure or
unnamed failure does NOT count. Source bytes are restored in a finally block,
then touched to avoid Cargo reusing a mutated executable. Each restoration
runs `diff -r --exclude=target <pristine> <crate>` and must have empty output /
exit 0. Raw test/diff logs and mutations.json remain under target/verification.
The final gate rerun must pass on restored source. Only this verification
report is intentionally updated after the pristine full-tree comparisons.

## Decisions

Controller must fold these into SPEC (SPEC was read-only):

1. Identity: global literal Hermes dedupeKey, full replacement rather than
   partial merge, stable h-N on updates, move to front. D-Bus returned ID is
   authoritative d-N; also coalesce the nonzero replaces_id. If returned and
   replaced IDs differ, close the superseded local ID as replaced before
   received. Handle both IDs already being present without duplicates.
2. Revision/event contract: add wrapping rev:u32 to IslandNotification and
   NotificationClosed, with one monotonically wrapping counter for both topics.
   NotificationReceived newtype serializes directly as the SPEC notification
   payload. NotificationClosed is {rev,id,reason}; reasons are expired,
   dismissed, cleared, evicted, filtered, replaced, serverClosed. Delta's
   {type,payload} envelope is Rust-call convenience, not the Tauri topic payload.
3. TTL: this core treats TTL as entry lifetime, separate from actor toast timers.
   Default 6000ms, 0 resident, explicit finite TTL 0..=86400000ms, inclusive expiry.
   Nontransient critical defaults resident; transient critical uses default.
   Explicit TTL overrides urgency. Caller drives expire; invalid/unrepresentable
   deadlines reject atomically. Low urgency never requests a toast; DND suppresses
   even critical. Changing DND does not replay old toasts.
4. Privacy: exact own-name and exact deny-list matches drop without IDs/deltas.
   Body redaction happens before storage/emission; present body becomes Hidden,
   absent remains null. Policy changes retroactively scrub and remove denied
   entries with deltas; scrubbing is irreversible. Transient items stay in the
   bounded memory ring until expiry/close; is_transient exposes that status for
   an opt-in persistence adapter, which must not persist transient records.
5. Close semantics: local dismiss and clear always remove; return best-effort
   server IDs for the adapter to call CloseNotification. Expiry, eviction,
   privacy filtering and observed server close do not close a remote item.
   canInvokeActions=false unconditionally; actions are display metadata only.
6. D-Bus correlation keys MUST include caller identity: (sender,serial) matched
   to (destination,reply_serial), because serials are not globally unique. Cache
   size 256, true recency refresh, consumed-once matches, unmatched returns drop.
   Validate calls before cache mutation; a zero returned server ID errors and
   preserves the pending match. On server owner change reset BOTH correlator
   and store's correlated server entries; host authenticates owner/interface.
7. Decoded hints: strict recognized types, unknown hints ignored but bounded;
   hyphenated image names take precedence over underscore spellings. These map
   keys are not serde aliases. Body scanner is quote-aware, preserves comparison
   signs, strips well-formed tags/comments, decodes entities once, and returns
   aggregate b/i/u flags separately, not HTML or attributed span ranges.
8. Images: no PNG codec or filesystem access here. Caller must bound/downscale
   raw hints to <=128px per side before parse and encode off-thread; RGB/RGBA
   byte layout validated. iconPng checks data:image/png;base64, prefix and 64KiB
   cap only; adapter owns actual codec verification. Image path is opaque data.
9. String/count domains: inclusive UTF-8 bytes: names/source/key/actions 256,
   title/summary 1024, body 8192, hint/app-icon strings 1024, iconPng 64KiB;
   <=32 action pairs and <=32 hints. Reject oversize, not truncate. Required
   title/app/key/source strings cannot be whitespace-only. No normalization.
10. Ingest: strict v=1 and operation-specific camelCase fields; absent optional
    notify fields default, null optional values act absent. Error level maps to
    critical; info/success/warning normal. source labels appName for filtering.
    Progress is a full deduped upsert, resident below 1, default TTL at 1. Clear
    is idempotent by Hermes key; absent key succeeds with no id. Filtered notify
    returns code filtered. Errors never quote request data. All serializable
    shapes avoid u64/i64/usize; raw D-Bus structs are not serialized.
11. Framing/rate: 16KiB is inclusive payload BYTES excluding one LF/CRLF; accept
    already-stripped frames; reject embedded physical line separators. A bucket
    starts full at 20, caps at 20, refills continuously at 20/s, charges every
    attempted frame. Clock rewinds add no credit, nonfinite/negative time errors.
    This permits bursts and is NOT a rolling-window rate guarantee. Caller must
    enforce streaming byte bounds, same-UID access, max 8 connections and 10s idle.
12. Integration: finite nonnegative f64 now_ms in one consistent time domain;
    receivedAt uses that same input. Caller must map monotonic/epoch needs
    consistently and not mix clock domains. Store APIs are synchronous and
    return deltas only; adapters own transport, event queue/backpressure,
    authentication, lifecycle and real UI. No broad Phase 4 completion claim.

## UNVERIFIED

- Real D-Bus connection, server owner validation/rearm, method signatures and
  mock-server integration; transport adapters are deliberately not present.
- Socket framing accumulation bounds, peer credentials, idle/connection limits,
  event queue delivery, tray actions, toast actor interaction and pointer-enter
  cancellation, GUI and packaging. Constants/pure decisions are tested only.
- PNG encoding, image downscaling/decoding, icon content validity and filesystem
  handling; metadata remains inert. Body scanner is conservative, not a full
  standards-compliant HTML/XML parser; callers must render plain text.
- Persistence and notificationHistoryPersist opt-in integration; library never
  writes history, and transient exclusion must be enforced by the adapter.
- Tauri contract/specta bindings integration, TypeScript shape generation, and
  Settings/module-registry policy precedence. No specta dependency is allowed.
- Rust 1.85 execution (manifest follows module-registry); gates use installed
  1.99.1. No fuzzing, allocation-exhaustion campaign, or unbounded stress claim.
