# clipboard-history

Pure, platform-free clipboard history store for Hermes Island (SPEC §3.3).

This crate never touches the system clipboard, never opens sockets, never does
filesystem IO and never touches the network. It is a self-contained data
structure that the Tauri integration layer (`integrations/clipboard/`) drives.

## Safety / lint contract

- `#![forbid(unsafe_code)]`
- `unwrap_used` / `expect_used` denied for non-test code (`Cargo.toml`
  `[lints.clippy]`); allowed in tests via `clippy.toml`
  (`allow-unwrap-in-tests = true`).

## Gates

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

## Public API

- `HistoryConfig` (validated: capacity clamped to 10..=500, default 100;
  deny patterns ≤32, each ≤256 chars, must compile — invalid is an error).
- `HistoryStore::new`, `offer` (`OfferOutcome`), `pin`, `delete`, `clear`,
  `set_paused`/`is_paused`, `hash_of`, `expect_echo`, `list`, `get`,
  `to_persisted(include_unpinned)` / `from_persisted` (SPEC §3.3 default:
  pinned entries only).
- IPC boundary serde types: `ClipSummary`, `ClipContent` (camelCase, no
  u64/i64/usize, epoch ms as `f64`, ids as `String`, tagged `type`).

## Decisions

- Empty deny patterns are rejected (a compile-able `""` regex would match
  every text and silently deny everything).
- MIME sensitivity matching is case-insensitive and trimmed.
- Files preview shows basenames (last path segment), not full URIs.
- `from_persisted` re-validates: deny-matched / empty / oversize-image
  entries are dropped; oversize text is truncated at a char boundary; the
  result is capped to capacity (oldest unpinned first, then oldest overall).
- Persisted ids are de-duplicated; `next_id` is resumed past any existing
  numeric suffix so ids are never reused.
- Loaded entries are de-duplicated by content hash (newest kept); a dropped
  pinned duplicate's pin transfers to the kept entry.
