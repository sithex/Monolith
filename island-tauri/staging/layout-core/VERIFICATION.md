# VERIFICATION — layout-core crate

Location: `island-tauri/staging/layout-core/`.
Scope: pure core behind SPEC §0.5 "Design direction" (Side buttons bullet) and
§2.4 (one input-region owner) — the Phase 7 side-button Layout editor. The
crate owns the persisted layout model and the geometry math the
shape/input-region owner will use; it renders nothing. Platform-free: no IO,
no network, no threads, no clock, no `unsafe`, never panics on input reached
through its API. All geometry is logical px (`f64`, finite). Wire types use
camelCase via serde, no aliases. Dependencies: serde (derive) + serde_json
only.

## Gate commands — final state (all green, run under `nice -n 10`)

```
$ cargo generate-lockfile
    Updating crates.io index
     Locking 11 packages to highest compatible versions
exit=0

$ cargo fmt --check
(clean)
exit=0

$ cargo clippy --all-targets --locked -- -D warnings
    Checking layout-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/layout-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.15s
exit=0

$ cargo test --locked
   Compiling layout-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/layout-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.45s
     Running unittests src/lib.rs (target/debug/deps/layout_core-84d5f1c6a364206b)

running 3 tests
test model::tests::id_validation_boundaries ... ok
test model::tests::label_validation_boundaries ... ok
test model::tests::side_capacities ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-be14e759d400451f)

running 2 tests
test fuzz_from_json_never_panics ... ok
test fuzz_layout_never_panics_and_keeps_invariants ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s

     Running tests/layout_tests.rs (target/debug/deps/layout_tests-c0f6ac4cc9956fc7)

running 39 tests
test add_rejects_invalid_action_and_label ... ok
test add_rejects_invalid_ids ... ok
test add_rejects_duplicate_id_and_occupied_slot_and_leaves_state ... ok
test bottom_row_no_holes_for_gap_slots ... ok
test bottom_row_centred_on_island_centre ... ok
test bottom_slot_4_accepted_slot_5_rejected ... ok
test bottom_row_packs_slots_zero_and_three ... ok
test bounding_box_contains_every_rect_and_island ... ok
test custom_preset_bounds_accepted ... ok
test custom_preset_bounds_rejected ... ok
test default_layout_exact_contents ... ok
test diameter_boundaries ... ok
test edit_updates_action_and_label ... ok
test empty_slots_order_with_defaults ... ok
test edit_rejects_bad_action_long_label_and_missing_id ... ok
test gap_boundaries ... ok
test geometry_rejects_negative_or_zero_size_island ... ok
test geometry_rejects_non_finite_island ... ok
test input_region_count_is_one_plus_buttons ... ok
test left_slot_2_accepted_slot_3_rejected ... ok
test left_stack_positions_exact ... ok
test load_buttons_non_array_null_tolerated ... ok
test load_fatal_errors ... ok
test load_custom_preset_valid_dimensions_kept ... ok
test load_first_occurrence_wins ... ok
test load_missing_fields_default_without_counts ... ok
test load_invalid_preset_diameter_gap_fall_back_and_count ... ok
test move_into_occupied_slot_swaps_both_buttons ... ok
test load_unknown_fields_ignored ... ok
test remove_and_not_found ... ok
test move_to_out_of_range_slot_rejected_and_missing_id_not_found ... ok
test right_side_positions_exact ... ok
test load_skips_each_invalid_kind_and_counts ... ok
test move_to_same_slot_noop ... ok
test set_preset_compact_spacious_ok ... ok
test round_trip_default_layout_equal ... ok
test serde_camelcase_wire_shape ... ok
test to_json_preset_unit_variants ... ok
test round_trip_after_ops_equal ... ok

test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests layout_core

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit=0
```

Total: **44 tests** (3 unit + 2 fuzz + 39 integration), all green under
`nice -n 10`.

## Mutation checks (all 8 actually run on the current code)

Method: snapshot the pristine tree to scratch (`rsync --exclude=target`),
apply one breakage, run `nice -n 10 cargo test --locked --no-fail-fast`,
record failing named tests, restore pristine (via `rsync --delete
--exclude=target`, then `touch` restored sources so cargo rebuilds), and
verify `diff -r --exclude=target <pristine> .` empty for every mutation. All
eight restored identical, and the final full gate re-run on the restored tree
is green (above). Every mutation compiled (a non-compiling mutation would not
count). Each mutation fails *named* tests, not only the fuzz tests.

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | `move_to` into an occupied slot **overwrites** (occupant not moved) instead of swapping | `move_into_occupied_slot_swaps_both_buttons` |
| 2 | Bottom slot capacity reduced 5 → 3 (`Side::capacity`) | `bottom_slot_4_accepted_slot_5_rejected`, `empty_slots_order_with_defaults`, `bottom_row_packs_slots_zero_and_three`, `model::tests::side_capacities` |
| 3 | Bottom row not packed / not centred (each button anchored at its raw slot offset around the island centre) | `bottom_row_packs_slots_zero_and_three`, `bottom_row_no_holes_for_gap_slots`, `bottom_row_centred_on_island_centre` |
| 4 | Right side misplaced by one diameter (`right_x = island.x + w + g + d`) | `right_side_positions_exact`, `bounding_box_contains_every_rect_and_island` |
| 5 | `set_preset` validation removed (NaN / out-of-range Custom accepted) | `custom_preset_bounds_rejected` |
| 6 | Load scans `buttons` in reverse (last duplicate wins instead of first) | `load_first_occurrence_wins`, `load_skips_each_invalid_kind_and_counts`, `round_trip_default_layout_equal`, `round_trip_after_ops_equal` |
| 7 | Minimum diameter lowered 28 → 27 | `diameter_boundaries` |
| 8 | Maximum gap raised 24 → 25 | `gap_boundaries` |

## Decisions

- **Geometry functions return `Result`.** The API sketch showed
  `button_rects` / `input_region` / `bounding_box` returning plain values, but
  it also mandates "Non-finite or negative-size island → typed error". The
  least-surprising resolution is `Result<_, LayoutError>` for all three; the
  owner handles the error (e.g. skips geometry until the island is sane).
- **Zero-size islands are rejected** (`NegativeSizeGeometry`): "negative-size"
  is interpreted as "not positive", so `w <= 0 || h <= 0` errors. Defensively,
  non-finite `button_diameter` / `gap` also error as `NonFiniteGeometry`.
- **Left/right stacks pack too.** The spec states "occupied slots packed in
  slot order (no holes)" explicitly for the bottom row; the same packing rule
  is applied uniformly to the left/right vertical stacks (they stack from
  `island.y` downward in slot order, no holes), so a free slot never leaves a
  visual gap. `empty_slots()` exposes exactly the free slots for the dashed
  `+` markers.
- **`empty_slots()` order:** Left 0..3, then Right 0..3, then Bottom 0..5,
  ascending within each side.
- **Error precedence** (documented per op): static validation first (id
  syntax → action id validity → label length → slot range), then relational
  checks (duplicate id → occupied pair for `add`; existence for `remove` /
  `edit` / `move_to`). `move_to` checks slot range before existence; `edit`
  checks action/label before existence.
- **Load fallbacks only count when the field is present and invalid.** Missing
  `preset` / `buttonDiameter` / `gap` silently default; invalid *present*
  values fall back and set the matching `LoadReport` flag. `buttons: null` is
  treated as absent (no skip); `buttons` present but non-array is tolerated as
  zero buttons and counted as one skip.
- **First valid occurrence wins among survivors.** An invalid first occurrence
  does not consume its id (mirrors scratchpad-core); only after a button fully
  validates does it reserve its id and `(side, slot)` pair. Duplicates among
  survivors are skipped and counted.
- **Slots must be integer JSON numbers** (`2.0` is not accepted; `2` is).
  Wrong-typed scalar fields skip the button.
- **Labels may be the empty string** (`Some("")` is valid, ≤ 32 chars);
  `None` and `Some("")` are distinct.
- **`button_rects` output order:** left (slot ascending), right (slot
  ascending), bottom (slot ascending); each tuple is `(id, Rect)`.
- **`to_json`** mirrors the sibling crate's convention
  (`serde_json::to_string(...).unwrap_or_else(|_| unreachable!(...))`); the
  model ops guarantee finite values, so serialization cannot fail through the
  public API. A hand-constructed `Layout` holding non-finite floats would
  violate that guarantee (see UNVERIFIED).
- **Compact/Spacious store no numbers**; `SizePreset::width()` /
  `max_height()` canonicalize them. Custom dimensions live only in the
  `Custom` variant and are validated on `set_preset` and on load.

## UNVERIFIED

- **Tauri/window-owner wiring** — window sizing padding, physical-px
  scaling, and rounding of the input-region rows for the backend are outside
  this crate's scope (SPEC §4.4 / §2.4 live in other agents' files).
- **Concurrency** — the crate is single-threaded and pure; the owner
  serializes access.
- **Extremely large / deeply nested JSON** — `from_json` allocates
  proportionally to the input, and `serde_json`'s recursion limit (128) makes
  pathological nesting an `InvalidJson` error rather than a processed file.
- **Sub-pixel rendering choice** — geometry is exact logical px; antialiasing
  and physical-px rounding are the renderer's decision.
- **Hand-constructed layouts bypassing validation** — public fields allow a
  caller to build a `Layout` with non-finite floats; the ops reject those, but
  `to_json` would then hit the unreachable guard. The Tauri owner is expected
  to use the validated ops / `from_json`.

## Follow-up after controller audit

The controller ruled on an ambiguity in the original prompt: **adjacent side
buttons must not touch.** Consecutive buttons on the same side are now spaced
by `gap` (the original draft had them edge-to-edge). Nothing else in the
geometry changed.

### Change

In `src/geometry.rs::button_rects`:

- Left/Right: button `i` sits at `y = island.y + i * (d + g)` (centre pitch
  `d + g`, clear gap `g`) instead of `island.y + i * d`. `x` anchors
  unchanged.
- Bottom: the row is `count * d + (count - 1) * g` wide (was `count * d`),
  still centred on the island centre; button `i` is at
  `x = row_left + i * (d + g)` (pitch `d + g`, clear gap `g`).

Tests updated to the new exact positions: `left_stack_positions_exact`
(y offsets 0/46/92), `right_side_positions_exact` (y offsets 0/46),
`bottom_row_packs_slots_zero_and_three` / `bottom_row_no_holes_for_gap_slots`
(2-button row: width 82, left edge 259, edges 259/305),
`bottom_row_centred_on_island_centre` (3-button row: width 128, left edge
236, edges 236/328). `bounding_box_contains_every_rect_and_island` is
unchanged (the default layout keeps identical extents). README geometry docs
updated to state the `g` spacing and the row-width formula.

New named test `adjacent_button_gaps_equal_gap` (3 d/g combos × 3 sides)
asserts every adjacent same-side pair is separated by exactly `gap`
(within 1e-9). The fuzz invariant was extended: every pair of buttons on the
same side is separated by at least `gap - 1e-9` on that side's stacking axis
(vertical for left/right, horizontal for bottom), which also implies same-side
disjointness.

### Cross-side overlap note (important)

A previous *global* no-overlap fuzz assertion was an accidental property of
the old edge-to-edge geometry, not a documented contract. The spaced geometry
can legitimately produce cross-side overlap for extreme-but-valid parameters:
a wide bottom row (max 5 buttons, d=56, g=24 → width 376) centred on a narrow
island reaches horizontally into the left/right columns while tall stacks
reach vertically down to row height (e.g. d=56 g=24 on a 100×60 island with
3 left + 5 bottom buttons). The fuzz invariant is therefore now **per-side**
(same-side disjointness + `gap` separation), the formal guarantee the
controller asked for; buttons-vs-island disjointness and bounding-box
containment are still asserted globally. The input-region owner already unions
rects, so overlapping buttons are harmless for hit-testing, and
`bounding_box` still covers everything for window sizing.

### Gate commands — final state (all green, run under `nice -n 10`)

```
$ cargo fmt --check
(clean)
exit=0

$ cargo clippy --all-targets --locked -- -D warnings
    Checking layout-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/layout-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.18s
exit=0

$ cargo test --locked
   Compiling layout-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/layout-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.46s
     Running unittests src/lib.rs (target/debug/deps/layout_core-84d5f1c6a364206b)
running 3 tests
test model::tests::id_validation_boundaries ... ok
test model::tests::label_validation_boundaries ... ok
test model::tests::side_capacities ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-be14e759d400451f)
running 2 tests
test fuzz_from_json_never_panics ... ok
test fuzz_layout_never_panics_and_keeps_invariants ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s

     Running tests/layout_tests.rs (target/debug/deps/layout_tests-c0f6ac4cc9956fc7)
running 40 tests
test adjacent_button_gaps_equal_gap ... ok
test add_rejects_invalid_action_and_label ... ok
test add_rejects_invalid_ids ... ok
test add_rejects_duplicate_id_and_occupied_slot_and_leaves_state ... ok
test bottom_row_no_holes_for_gap_slots ... ok
test bottom_row_centred_on_island_centre ... ok
test bottom_slot_4_accepted_slot_5_rejected ... ok
test bottom_row_packs_slots_zero_and_three ... ok
test bounding_box_contains_every_rect_and_island ... ok
test custom_preset_bounds_accepted ... ok
test custom_preset_bounds_rejected ... ok
test default_layout_exact_contents ... ok
test diameter_boundaries ... ok
test edit_updates_action_and_label ... ok
test empty_slots_order_with_defaults ... ok
test edit_rejects_bad_action_long_label_and_missing_id ... ok
test gap_boundaries ... ok
test geometry_rejects_negative_or_zero_size_island ... ok
test geometry_rejects_non_finite_island ... ok
test input_region_count_is_one_plus_buttons ... ok
test left_slot_2_accepted_slot_3_rejected ... ok
test left_stack_positions_exact ... ok
test load_buttons_non_array_null_tolerated ... ok
test load_fatal_errors ... ok
test load_custom_preset_valid_dimensions_kept ... ok
test load_first_occurrence_wins ... ok
test load_missing_fields_default_without_counts ... ok
test load_invalid_preset_diameter_gap_fall_back_and_count ... ok
test move_into_occupied_slot_swaps_both_buttons ... ok
test load_unknown_fields_ignored ... ok
test remove_and_not_found ... ok
test move_to_out_of_range_slot_rejected_and_missing_id_not_found ... ok
test right_side_positions_exact ... ok
test load_skips_each_invalid_kind_and_counts ... ok
test move_to_same_slot_noop ... ok
test set_preset_compact_spacious_ok ... ok
test round_trip_default_layout_equal ... ok
test serde_camelcase_wire_shape ... ok
test to_json_preset_unit_variants ... ok
test round_trip_after_ops_equal ... ok
test result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests layout_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit=0
```

Total: **45 tests** (3 unit + 2 fuzz + 40 integration), all green under
`nice -n 10`. (The intermediate `cargo test` run above was a full re-run on
the final restored tree; the only delta vs. the pristine snapshot is this
VERIFICATION.md section.)

### Follow-up mutations (both actually run on the current code)

Method unchanged: snapshot pristine tree (post-follow-up edits) to scratch,
apply one breakage, run `nice -n 10 cargo test --locked --no-fail-fast`,
record failing *named* non-fuzz tests, restore pristine (`rsync --delete
--exclude=target`, then `touch` restored sources so cargo rebuilds), verify
`diff -r --exclude=target <pristine> .` empty. Both restored identical; the
final full gate above was run on the restored tree.

| # | Breakage applied | Failing named tests observed |
|---|---|---|
| 9 | Bottom row width drops the `(count-1)*gap` term (`row_width = count * d`) | `bottom_row_packs_slots_zero_and_three`, `bottom_row_no_holes_for_gap_slots`, `bottom_row_centred_on_island_centre` |
| 10 | Left stack spacing `d` instead of `d + gap` | `left_stack_positions_exact`, `adjacent_button_gaps_equal_gap` |

Both breakages compile; each fails named non-fuzz tests (mutation 10 also
trips the fuzz same-side invariant, as expected).
