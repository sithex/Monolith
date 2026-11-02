# VERIFICATION — scratchpad-core crate

Location: `island-tauri/staging/scratchpad-core/`.
Scope: pure core behind SPEC §0.5 "Design direction" (Apps bullet) and §8
"Phase 6b" — the Scratchpad app ("plain-text notes in the data dir, debounced
atomic saves"). The crate decides what to write (notes, titles, ordering,
limits) and when (the `SaveScheduler`); the Tauri actor does the actual file
IO. Platform-free: no IO, no network, no threads, no clock, no `unsafe`,
never panics on any input. All times are injected `u64` monotonic ms.
Dependencies: serde (derive) + serde_json only. Wire types use camelCase via
serde, no `#[serde(alias)]`.

## Gate commands — final state (all green)

```
$ nice -n 10 cargo generate-lockfile
    Updating crates.io index
     Locking 11 packages to highest compatible versions
exit=0

$ nice -n 10 cargo fmt --check
(clean)
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking scratchpad-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/scratchpad-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.15s
exit=0

$ nice -n 10 cargo test --locked
   Compiling scratchpad-core v0.1.0 (.../scratchpad-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.20s
     Running unittests src/lib.rs (target/debug/deps/scratchpad_core-72ce198ecb0df463)

running 24 tests
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz_tests.rs
running 2 tests
test fuzz_scheduler_never_panics_and_keeps_invariants ... ok
test fuzz_scratchpad_never_panics_and_keeps_invariants ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/scratchpad_tests.rs
running 33 tests
test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests scratchpad_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total: 59 tests (24 unit + 2 fuzz + 33 integration), all green under
`nice -n 10`.

## Mutation checks (all actually run on the current code)

Method: snapshot the pristine tree to scratch (`--exclude=target`), apply
one breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record
failing test names, restore pristine (touching restored sources so cargo
rebuilds), then `diff -r --exclude=target <pristine> .` verified empty for
every mutation. All six restored identical, and the final full gate re-run on
the restored tree is green (above). Every mutation compiled (a non-compiling
mutation would not count). Each mutation fails *named* tests, not only the
fuzz tests.

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | Title cut on a **byte** boundary (lossy `from_utf8_lossy`) instead of a char boundary in `derive_title` | `note::tests::title_cut_never_splits_multibyte_char`, `note::tests::title_cut_with_emoji_straddling_boundary` |
| 2 | Identical edit always marks dirty and bumps `updatedMs` (identical-body early return removed in `edit`) | `edit_identical_body_does_not_mark_dirty`, `edit_identical_body_does_not_touch_updated_ms` |
| 3 | Max-delay cap ignored: `next_save_at` = `last_mark + debounce` only (the `.min(first + max_delay)` dropped) | `save_scheduler::tests::first_dirty_anchors_the_max_delay`, `save_scheduler::tests::max_delay_cap_holds_under_continuous_edits` |
| 4 | `saved()` clears dirty even when a `mark_dirty` happened mid-flight (version guard removed) | `save_scheduler::tests::edit_during_inflight_save_keeps_dirty` |
| 5 | Backoff not capped at 30 s (`.min(MAX_BACKOFF_MS)` dropped in `save_failed`) | `save_scheduler::tests::backoff_sequence_cap_and_reset` |
| 6 | Load trusts the stored `title` instead of recomputing it | `from_json_recomputes_titles_and_ignores_stored_title`, `from_json_skips_each_invalid_kind_and_counts` |

## Decisions

- **`delete` / `set_pinned` take a `now` parameter.** The spec lists
  `delete(id)` and `set_pinned(id, bool)`, but the integration rule "every
  mutating op that changes state calls into the scheduler with its now"
  cannot be honored otherwise. Both take `now` first (like `new_note` /
  `edit`): `delete(now, id)`, `set_pinned(now, id, pinned)`.
- **Missing scalar fields default on load.** `createdMs`/`updatedMs` →
  `0`, `pinned` → `false`, `body` → `""`. Only wrong-typed *present* fields
  skip a note; a missing field never does.
- **Deduplication counts against kept notes.** An invalid note never occupies
  its id, so a later valid note with the same id is accepted; "duplicate id"
  skipping applies among notes that survive validation.
- **Truncation keeps the newest 200 by `updatedMs`** (ties broken by `id`
  ascending); `truncatedToMax` is set when > 200 valid notes existed.
  Truncation is separate from `skipped` (invalid notes never feed it).
- **Titles are recomputed on load and the stored `title` is ignored
  entirely** — even a wrong-typed stored title never fails a note.
- **Unknown extra fields are ignored** at every level (top level and per
  note).
- **Flush/backoff interaction.** `flush_requested()` makes a dirty scheduler
  due immediately (`next_save_at() == Some(0)`); if that flush save fails,
  flush intent is dropped and normal backoff takes over (this avoids a hot
  retry loop at quit while the disk is failing). A backoff deadline may
  exceed `first_dirty + max_delay` by design; the fuzz invariant asserts
  `next_save_at ≤ first_dirty + max_delay` only when not in failure backoff.
- **Internal storage is always in canonical display order** (pinned
  descending, `updatedMs` descending, `id` ascending); `to_json` emits that
  order so `to_json → from_json → to_json` is byte-stable.
- **Fatal load errors keep the file aside.** `InvalidJson`, `NotAnObject`,
  `InvalidVersion`, `NotesNotArray` are typed errors; the caller preserves
  the bad file and starts empty (documented in README). Per-note problems
  never escalate to fatal.
- **Ids are ASCII-only** `[A-Za-z0-9_-]`, 1..=64 characters (length in
  chars == bytes for the allowed alphabet).
- **Error precedence in `new_note`:** invalid id → duplicate id → capacity
  (> 200). In `edit`: missing note → oversize body → identical-body no-op.

## UNVERIFIED

- **Tauri actor wiring** — atomic file writes, data-dir paths, and the
  wakeup loop around `next_save_at()` are outside this crate's scope.
- **Live monotonic source behavior** — e.g. system suspend / clock jumps on
  a real host; synthetic `u64` inputs only.
- **Very large files** — loading allocates proportionally to the input; there
  is no streaming reader or cap on a pathological file's size beyond the
  256 KiB per-note body limit.
- **Deeply nested JSON** — `serde_json`'s recursion limit (128) makes such
  input an `InvalidJson` error rather than a processed file.
- **Concurrency** — the crate is single-threaded with no locks; the actor
  serializes access.
- **No eventual give-up policy** — a permanently failing disk retries every
  backoff deadline forever (up to 30 s apart); whether to surface an error is
  the app's decision.
- **Multi-device / conflict handling** — no merge or last-writer logic
  beyond "keep the newest 200 by updatedMs".
