# VERIFICATION — launcher-arrange-core crate

Location: `island-tauri/staging/launcher-arrange-core/`.
Scope: pure core behind SPEC §0.5 "Design direction" (App launcher + Launcher
edit mode bullets) — the phone-style "jiggle" edit session for the Hermes
Island app grid. The crate decides the edit state machine: visible/hidden,
moves, lock rules, cancel/commit semantics, and reconcile against module-
registry changes. The Tauri actor applies the resulting `ArrangeCommit` (a
full `order` permutation plus `enabled_changes`) to the module registry; this
crate never imports it. Platform-free: no IO, no network, no threads, no
clock, no `unsafe`, never panics on any input. Dependencies: serde (derive) +
serde_json only. Wire types use camelCase via serde, no `#[serde(alias)]`.

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
    Checking launcher-arrange-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/launcher-arrange-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.04s
exit=0

$ nice -n 10 cargo test --locked
   Compiling launcher-arrange-core v0.1.0 (.../launcher-arrange-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.10s
     Running unittests src/lib.rs (target/debug/deps/launcher_arrange_core-2f7d4f995366c64e)

running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/arrange_tests.rs (target/debug/deps/arrange_tests-f3aa82b113b7b850)

running 52 tests
test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-4c0922d28518b4d1)

running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests launcher_arrange_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total: 55 tests (2 unit + 52 integration + 1 fuzz), all green under
`nice -n 10`.

## Mutation checks (all actually run on the current code)

Method: snapshot the pristine tree to scratch (`rsync --exclude=target`),
apply one breakage, run `nice -n 10 cargo test --locked --no-fail-fast`,
record failing test names, restore pristine (touching restored sources so
cargo rebuilds), then `diff -r --exclude=target <pristine> .` verified empty
for every mutation. All six restored identical, and the final full gate re-run
on the restored tree is green (above). Every mutation compiled (a non-compiling
mutation would not count). Each mutation fails *named* non-fuzz tests; the
fuzz test also catches most but is not relied upon.

| # | Breakage applied | Failing named non-fuzz tests observed |
|---|---|---|
| 1 | `Locked` guard removed from `disable` (locked items can be disabled) | `disable_locked_returns_locked`, `op_error_leaves_state_unchanged`, `reconcile_locking_a_disabled_survivor_forces_it_enabled`, `reconcile_takes_locked_from_current_ids` |
| 2 | `cancel` ignores the snapshot (keeps working state) | `arrangement::tests::cancel_restores_exact_internal_snapshot`, `arrangement::tests::reconcile_during_editing_keeps_snapshot_parallel`, `cancel_restores_exact_state`, `move_with_hidden_items_interleaved_preserves_hidden_relative_order` |
| 3 | `commit` order built from visible items only (omits hidden ids) | `commit_order_is_full_permutation_including_hidden`, `commit_order_matches_state_after_change`, `move_past_end_clamps_to_last_visible_slot_across_hidden` |
| 4 | `enabled_changes` lists every id (diff filter dropped) | `commit_enabled_changes_only_lists_diffs_in_order`, `commit_order_matches_state_after_change` |
| 5 | Move clamp off by one (`target = (to + 1).min(last)`) | `move_to_zero_places_at_front`, `move_onto_itself_is_noop`, `move_with_hidden_items_interleaved_preserves_hidden_relative_order`, `commit_order_is_full_permutation_including_hidden` |
| 6 | `reconcile` during editing skips the snapshot update | `reconcile_during_editing_updates_snapshot`, `reconcile_during_editing_does_not_resurrect_removed_on_cancel`, `arrangement::tests::reconcile_during_editing_keeps_snapshot_parallel` |

## Decisions

- **Internal model is a single ordered list.** The arrangement stores one
  `Vec<GridItem>`; `visible` = enabled items in that order, `hidden` =
  disabled items in that order. `ArrangeCommit::order` is the full internal
  permutation (visible and hidden interleaved).
- **`move_item` rule (exact).** Positions are visible-grid indices. The
  target is clamped to the last visible slot (`to.min(visible_count - 1)`).
  The moved item is removed, then reinserted immediately before the item that
  currently occupies visible index `target` (or at the end when `target`
  equals the post-removal visible count). This preserves the relative order —
  and absolute positions — of every other item, so hidden items keep their
  relative order among all items by construction. Moving an item onto its own
  (clamped) slot is a no-op. Moving a hidden item is an error
  (`NotVisible`) — hidden tiles have no visible index and the UI never offers
  dragging them.
- **`enable` rule (exact).** A re-enabled item is inserted immediately after
  the last enabled item, so it becomes the last visible item while any
  trailing hidden items keep their positions. Enabling an already-visible
  item is a no-op.
- **Locked ⇒ enabled is enforced at ingestion.** In `new` and `reconcile`,
  an item that is `locked` and `enabled == false` is stored as enabled
  (locked dominates). This makes the "locked items always enabled" invariant
  hold by construction and mirrors the module registry's "never disabled"
  contract for Hermes home / Settings. `disable` on a locked item → `Locked`.
- **`new` error precedence:** empty ids → first id longer than 64 bytes →
  first duplicate id → more than 256 items. Ids are measured in bytes
  (`String::len()`), so multi-byte UTF-8 counts toward the 64-byte cap.
- **`reconcile` survivor semantics.** Surviving ids keep their **old**
  relative order (not the `current_ids` order); their enabled flag is kept as
  it is in the target list; `locked` is always taken from `current_ids`
  (locking a disabled survivor forces it enabled). Missing ids are dropped;
  new ids are appended in `current_ids` order with their given flags;
  duplicates in `current_ids` are deduplicated (first occurrence wins).
  During editing the same transform is applied to the snapshot, so cancel
  never resurrects a removed id nor drops a new one. `reconcile` does not
  end an edit session and may produce an empty arrangement (e.g. an empty
  `current_ids`).
- **`cancel` / `commit` are edit-session ops**: outside a session they return
  `NotEditing`. `commit` ends the session in both cases (Some/None); the
  working state becomes the base for the next `begin_edit`.
- **`enabled_changes` order** follows the commit `order` (internal order),
  not the snapshot order.
- **No serde aliases.** snake_case keys such as `enabled_changes` are
  rejected (tested); camelCase is the only accepted shape.

## UNVERIFIED

- **Tauri actor wiring** — applying `ArrangeCommit` to the module registry,
  the begin/commit lifecycle around the UI, and hidden-apps-row rendering are
  outside this crate's scope.
- **Concurrency** — the crate is single-threaded with no locks; the actor
  serializes access.
- **Real-world id alphabet** — the core constrains id *length* and
  uniqueness only; any UTF-8 id ≤ 64 bytes is accepted (tested with
  multi-byte ids), but the registry's slug rules are not enforced here.
- **Very large `current_ids` slices** — `reconcile` dedup expense is
  O(n·m) via linear scans; bounded in practice by the 256-item cap but not
  enforced on the reconcile input itself.
- **Commit applied vs. registry rejection** — if the registry rejects an
  `order` permutation, the core has no rollback story; that is the actor's
  decision.
