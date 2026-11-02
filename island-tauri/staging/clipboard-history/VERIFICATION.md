# VERIFICATION — clipboard-history crate

Location: `island-tauri/staging/clipboard-history/`.
Scope: pure clipboard history store (SPEC §3.3) + IPC boundary serde types
(SPEC §5.1–5.2). Platform-free: no clipboard access, no sockets, no
filesystem IO, no network.

## Gate commands — final state (all green)

```
$ nice -n 10 cargo fmt --check
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.05s
exit=0

$ nice -n 10 cargo test --locked
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.01s
     Running unittests src/lib.rs (target/debug/deps/clipboard_history-04d5def23c83276d)

running 6 tests
test preview::tests::preview_collapses_whitespace ... ok
test preview::tests::preview_files_format ... ok
test preview::tests::preview_files_truncation ... ok
test preview::tests::preview_image_format ... ok
test preview::tests::preview_truncates_unicode_safely ... ok
test preview::tests::preview_zero_chars_returns_empty ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/history.rs (target/debug/deps/history-02ba8ccb5a31e1ff)

running 71 tests
test capacity_clamped_high ... ok
test capacity_clamped_low ... ok
test clear_returns_removed_count ... ok
test all_pinned_full_skipped ... ok
test clear_keeps_pinned ... ok
test clip_content_files_json_shape ... ok
test clip_content_image_json_shape ... ok
test clip_content_text_json_shape ... ok
test clip_summary_json_kinds ... ok
test clip_summary_json_shape ... ok
test config_defaults ... ok
test dedupe_promotes_to_front ... ok
test delete_removes_entry ... ok
test delete_unknown_id_errors ... ok
test deny_pattern_empty_errors ... ok
test dedupe_promotes_keeps_id_and_pin ... ok
test deny_pattern_too_long_errors ... ok
test deny_pattern_too_many_errors ... ok
test deny_pattern_invalid_regex_errors ... ok
test echo_after_delete_consumes_without_reorder ... ok
test echo_consumed_once ... ok
test echo_expires_exactly_at_2000ms ... ok
test echo_within_window_skips_and_reorders ... ok
test eviction_removes_oldest_unpinned ... ok
test eviction_skips_pinned ... ok
test deny_regex_ignores_files_and_images ... ok
test get_files ... ok
test get_image_data_url ... ok
test files_empty_skipped ... ok
test get_text ... ok
test hash_of_deterministic_and_distinct ... ok
test ids_never_reused_after_delete ... ok
test image_too_large_bytes ... ok
test image_too_large_side ... ok
test list_limit_caps_results ... ok
test deny_regex_matches_text ... ok
test list_newest_first ... ok
test list_query_no_match_empty ... ok
test list_query_preview_case_insensitive ... ok
test list_query_source_app_case_insensitive ... ok
test paused_skips_all ... ok
test persist_corrupt_json_errors ... ok
test oversize_text_matching_deny_pattern_returns_too_large ... ok
test persist_caps_oversize_text ... ok
test persist_dedups_carries_pin_from_dropped ... ok
test persist_dedups_unpinned_duplicates ... ok
test persist_caps_to_capacity ... ok
test persist_drops_oversize_image ... ok
test persist_id_resume_guard_skips_existing_suffix ... ok
test persist_next_id_resumes ... ok
test persist_drops_denied_text ... ok
test pin_unknown_id_errors ... ok
test pinned_count_toward_capacity ... ok
test persist_reassigns_duplicate_ids ... ok
test persist_pinned_only_excludes_unpinned ... ok
test persist_rejects_unknown_version ... ok
test persist_include_unpinned_round_trips_everything ... ok
test persist_round_trip_preserves_all ... ok
test sensitive_mime_case_insensitive_trimmed ... ok
test sensitive_mime_concealed ... ok
test summary_bytes_files_sum ... ok
test sensitive_mime_exclude_processing ... ok
test sensitive_mime_transient ... ok
test sensitive_password_hint_secret ... ok
test summary_fields_text ... ok
test summary_image_bytes ... ok
test text_empty_skipped ... ok
test text_too_large ... ok
test text_whitespace_only_skipped ... ok
test unpin_allows_eviction ... ok
test deny_patterns_valid_accepted ... ok

test result: ok. 71 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests clipboard_history

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit=0
```

## Mutation checks (all actually run on the current code)

Method: snapshot pristine tree to scratch (`--exclude=target`), apply one
breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record failing
test names, restore pristine, then `diff -r <pristine> .` verified empty for
every mutation. All eight restored identical.

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | Eviction no longer skips pinned (`rposition(\|_\| true)` in `offer`) | `all_pinned_full_skipped`, `eviction_skips_pinned` |
| 2 | Echo window never expires (`take_echo` retains everything) | `echo_expires_exactly_at_2000ms` |
| 3 | Sensitive guard removed from `offer` | `sensitive_password_hint_secret`, `sensitive_mime_concealed`, `sensitive_mime_transient`, `sensitive_mime_exclude_processing`, `sensitive_mime_case_insensitive_trimmed` |
| 4 | Preview truncation splits UTF-8 (byte slicing in `collapse_whitespace`) | `preview::tests::preview_truncates_unicode_safely`, `preview::tests::preview_collapses_whitespace` |
| 5 | Size/deny order reverted (deny before size in `offer`) | `oversize_text_matching_deny_pattern_returns_too_large` |
| 6 | `to_persisted` filters out the `include_unpinned` gate (always writes everything) | `persist_pinned_only_excludes_unpinned` |
| 7 | Id-resume guard disabled (`if store.next_id <= max_suffix` forced false) in `from_persisted` | `persist_id_resume_guard_skips_existing_suffix` |
| 8 | Load-time content-hash dedup removed (`seen_hashes` check forced false) in `from_persisted` | `persist_dedups_unpinned_duplicates`, `persist_dedups_carries_pin_from_dropped` |

## Decisions

- **Persistence default = pinned-only (SPEC §3.3).** `to_persisted(include_unpinned)`
  writes only pinned entries when `include_unpinned == false`; the internal id
  counter is still persisted so ids never resume into reuse. `include_unpinned
  == true` round-trips everything.
- **Size limits checked before the deny regex** in `offer`: an oversize offer
  is `TooLarge` even when its text would also match a deny pattern.
- Empty deny patterns are rejected (a compiling `""` regex would match every
  text and silently deny everything).
- MIME sensitivity matching is case-insensitive and trimmed
  (`org.nspasteboard.ConcealedType` etc. normalized).
- Files preview shows basenames (last path segment), truncated at
  `preview_chars` with a trailing `…`; text preview truncates with no
  ellipsis.
- `from_persisted` re-validates: deny-matched / empty / oversize-image entries
  are dropped; oversize text is truncated at a char boundary; result capped to
  capacity (oldest unpinned first, then oldest overall).
- Persisted duplicate/empty ids are reassigned; `next_id` resumes past the max
  existing numeric suffix so ids are never reused. The id-resume guard is
  pinned by `persist_id_resume_guard_skips_existing_suffix`: a stored
  `nextId` below an existing id suffix (e.g. entries `c-2`/`c-7` with
  `nextId: 2`) must yield a collision-free next id (`c-8`), not a reuse.
- `from_persisted` de-duplicates by content hash (SPEC §3.3): persisted entries
  are processed newest-first, so the first occurrence of a hash is kept and
  later duplicates are dropped; a dropped pinned duplicate's pin transfers to
  the kept entry. Pinned by `persist_dedups_unpinned_duplicates` and
  `persist_dedups_carries_pin_from_dropped`.
- Echo window active iff `now_ms < expires_at` (exactly +2000 ms ⇒ expired).
- Dedupe/echo promotion preserves the existing id and pinned state.
- Deny regexes apply to text only.
- Hash ignores mime/source_app: `xxh3_64(kind || bytes)`, images by raw PNG
  bytes, file-lists NUL-joined.
- `ClipContent` uses `rename_all_fields = "camelCase"` so the image variant
  serializes `pngDataUrl` (serde enum `rename_all` alone does not rename
  struct-variant fields).
- `CanIncludeInClipboardHistory` is not filtered here (caller's job, per spec).

## UNVERIFIED

- **Platform integration** — real clipboard monitors (Wayland/KWin `ext_data_control`,
  X11, Windows, macOS), `clipboard_copy(id)`, and the Tauri command layer in
  `src-tauri` are NOT built or tested here; this crate is the pure store only.
- **Real file persistence** — the `0600`-mode `<data>/clipboard.json` write and
  the pinned-only default file behavior are implemented by the platform layer;
  this crate only produces/consumes the JSON string. Round-trips here are
  in-memory strings.
- **Real PNG data** — `pngDataUrl` base64 verified against synthetic bytes only;
  no real PNG blobs were decoded/validated.
- **Wall-clock echo timing** — echo expiry is tested with caller-supplied
  `now_ms`; no real-time clock was exercised.
- **`allow-unwrap-in-tests` scope** — the clippy.toml allowance covers
  `#[test]`-annotated functions but NOT `Option::expect` in this test suite
  (observed); tests avoid `.expect()`/`.unwrap()` in helper fns and
  `Option::expect` accordingly.
- **Cross-platform behavior** — nothing here is OS-specific; platform-specific
  semantics (e.g. X11/Windows clipboard formats) remain unverified by design.
