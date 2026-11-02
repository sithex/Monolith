# VERIFICATION — brightness-core crate

Location: `island-tauri/staging/brightness-core/`.

Scope: pure core behind SPEC §0.5 "Design direction" (Apps bullet —
"Controls (volume, brightness, quick toggles)") and §8 "Phase 6b"
("Controls + Volume mixer ... Pure cores go to `staging/` first") — the
brightness slider for the Controls app. The Tauri actor talks to KDE
PowerDevil over D-Bus (zbus) and passes DECODED values here; this crate
keeps the display model, converts percent <-> raw values, coalesces slider
drags and builds method-call requests. It never does IO.

Platform-free: no IO, no network, no threads, no clock, no `unsafe`, never
panics on any input. Dependencies: serde (derive) + serde_json only. Wire
types use camelCase via serde, no `#[serde(alias)]`. Upstream facts
(org.kde.ScreenBrightness + org.kde.ScreenBrightness.Display service/object/
interface, properties, signals, methods, step actions, flags bit 0x1,
DisplaysDBusNames, MaxBrightness 10000 / IsInternal false on Nick's machine)
were copied from powerdevil daemon/dbus XML and confirmed by introspecting
the Plasma 6 session bus; the fixtures never touch D-Bus or change brightness.

## Gate commands — final state (all green, under `nice -n 10`)

```
$ nice -n 10 cargo fmt --check
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking brightness-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/brightness-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.18s
exit=0

$ nice -n 10 cargo test --locked
   Compiling brightness-core v0.1.0 (.../brightness-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.22s
     Running unittests src/lib.rs (target/debug/deps/brightness_core-232bbcb6a9331a86)

running 6 tests
test cmd::tests::step_mapping ... ok
test name::tests::invalid_names_rejected ... ok
test name::tests::path_construction ... ok
test name::tests::valid_names_accepted ... ok
test percent::tests::percent_of_boundaries ... ok
test percent::tests::raw_for_percent_boundaries ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/brightness_tests.rs (target/debug/deps/brightness_tests-1927f191f882aa3c)

running 20 tests
test display_added_then_props_clears_fetch ... ok
test display_names_order_fetch_once ... ok
test drag_errors ... ok
test foreign_context_change_during_drag ... ok
test fetch_at_most_once_until_removed ... ok
test invalid_names_dropped_and_counted ... ok
test max_16_displays_extra_counted ... ok
test name_validation_and_path ... ok
test next_wakeup ... ok
test props_range_availability_and_clamping ... ok
test public_percent_helper ... ok
test readrag_after_release_starts_fresh_session ... ok
test release_flushes_and_ends_drag ... ok
test removed_during_drag_ends_drag ... ok
test settle_window_499_vs_500 ... ok
test serde_camel_case_round_trips ... ok
test step_mapping ... ok
test throttle_boundaries_latest_wins ... ok
test two_displays_throttled_independently ... ok
test unknown_and_invalid_signals_counted ... ok

test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-5ff556537c7a5c77)

running 1 test
test fuzz_brightness_model_never_panics_and_keeps_invariants ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests brightness_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total: 27 tests (6 unit + 20 integration + 1 fuzz), all green.

## Mutation checks (all actually run on the current code)

Method: snapshot the pristine tree to scratch (`--exclude=target`), apply
one breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record
failing test names, restore pristine (copy back + `touch` restored files so
cargo cannot reuse a stale mutated binary), then `diff -r
--exclude=target <pristine> .` verified empty (`RESTORED IDENTICAL`) for
every mutation, and the final full gate re-run on the restored tree is green
(above). Every mutation compiled (a non-compiling mutation would not count).
Each fails *named* tests, not only the fuzz test.

| # | Breakage applied | Failing named tests observed |
|---|---|---|
| 1 | Percent rounding truncated instead of round-half-up (`+ max/2` term dropped in `percent_of`) | `percent::tests::percent_of_boundaries`, `public_percent_helper` |
| 2 | i32 multiplication instead of i64 in `percent_of` (overflows at `i32::MAX` in debug) | `percent::tests::percent_of_boundaries` |
| 3 | Throttle `THROTTLE_MS` 50 → 0 ms | `throttle_boundaries_latest_wins`, `two_displays_throttled_independently`, `next_wakeup` |
| 4 | Settle window ignored (`view_percent` always shows settle target while released) | `settle_window_499_vs_500`, `foreign_context_change_during_drag` |
| 5 | Foreign-context `BrightnessChanged` dropped during a drag (`e.drag.is_none()` guard) | `foreign_context_change_during_drag` |
| 6 | `-` accepted in display names (`*b == b'-'` added to validation) | `name::tests::invalid_names_rejected`, `name::tests::path_construction`, `invalid_names_dropped_and_counted`, `name_validation_and_path` |
| 7 | `Step::Decrease` mapped to 0 (Increase) instead of 1 | `cmd::tests::step_mapping`, `step_mapping` |

## Decisions

- **Rounding is half-up** in i64 for both directions: `percent` =
  `(clamped×100 + max/2) / max` (clamped to `0..=max` first; `None` when
  `max <= 0`); `raw_for_percent` = `(pct×max + 50) / 100`, rejecting
  `pct > 100` and `max <= 0` as typed errors. Max 10000: 50→1, 49→0,
  9950→100; max 1 and max `i32::MAX` verified without overflow.
- **Name validation** is ASCII `[A-Za-z0-9_]`, 1..=255 bytes. `display_path`
  = `/org/kde/ScreenBrightness/<name>`.
- **`DisplayNames` is the authoritative ordered list**: displays no longer
  listed are removed; order follows the vector; duplicates in one vector are
  a no-op; invalid names and names beyond `MAX_DISPLAYS` (16) are dropped
  and counted via `ignored()`. `Applied.changed` is true only when the
  resulting set/order or a stored field actually differs (a repeat
  `DisplayNames` yields `changed: false`).
- **Fetch-once** is enforced structurally: `fetch` lists names that are new
  and have no Props yet; a name already present in the model is never
  re-listed (even without Props) until it is removed and re-added.
  `Props` clears the fetch only by arrival (RangeChanged does not).
- **Availability** = `max > 0`: `Props`/`RangeChanged` with `max <= 0` keep
  the display but yield `percent: None` and make `drag` return
  `DragError::UnavailableDisplay`. Order of drag errors: unknown display,
  unavailable, percent-out-of-range.
- **Clamping counting**: a brightness outside `0..=max` is stored clamped
  and counted via `clamped()` (exact `0`/`max` boundaries are not counted).
  When `max <= 0` the raw value is stored unclamped and uncounted (the
  display is unavailable; percent is None regardless).
- **BrightnessChanged is always stored** (both `CONTEXT` echoes and foreign
  contexts, clamped when out of range); the *view* hides it while dragging
  and for `SETTLE_MS` (500) after release, showing the drag target instead.
  This satisfies "a BrightnessChanged whose context is NOT CONTEXT during a
  drag still updates the stored brightness (visible after the drag ends)" —
  the stored value is the single source of truth once the settle window
  closes.
- **Drag sessions are per display and independent**: `drag` updates the
  latest target in an existing session (throttle state persists) and starts
  a fresh session (immediate first send on the next `poll`) when none is
  active or after a `release`. `poll` emits at most one request per display
  per 50 ms, always the latest target, skipping when it equals the last sent
  value; `release` flushes a pending unsent target immediately (ignoring the
  throttle — the spec says "flushes ... immediately") and ends the drag.
- **`next_wakeup`** returns the minimum of: next throttle release for an
  unsent target (immediate = `now_ms`), and settle expiry (`released_at +
  500`) for a UI refresh; `None` when idle. It never returns before
  `now_ms`.
- **Step mapping** is verbatim: Increase=0, Decrease=1, IncreaseSmall=2,
  DecreaseSmall=3; `flags = SUPPRESS_INDICATOR (0x1)`, `context = CONTEXT`.
- **Serde** renames camelCase: `isInternal`, `maxBrightness`,
  `sourceClient`, `stepAction`; `Event` variants camelCase too
  (`displayNames`, `displayAdded`, `displayRemoved`, `props`,
  `brightnessChanged`, `rangeChanged`) via `rename_all_fields`.

## UNVERIFIED

- **Tauri actor wiring / zbus** — talking to PowerDevil, parsing D-Bus
  into `Event`s, and invoking the built `SetRequest`/`StepRequest` are
  outside this crate's scope (it never does IO or runs anything).
- **Live PowerDevil semantics** — fixtures come from the controller's
  introspection of Nick's Plasma 6 session bus; other PowerDevil versions'
  property/signal shapes are untested. The `sourceClient` field is captured
  in `BrightnessChanged` but not otherwise interpreted.
- **Settle vs. time source** — `DisplayView.percent` evaluates the 500 ms
  settle window against the last `now_ms` the model has seen (`drag`/
  `poll`/`release`). Calling `displays()` without any timed call keeps the
  previous settle state; the Tauri side is expected to drive time via
  `poll`/`next_wakeup` as designed.
- **Concurrency / shared state** — the crate is single-threaded with no
  locks; the actor serializes access.
- **A drag whose display becomes unavailable mid-drag** (RangeChanged to
  `max <= 0`) is skipped by `poll` (no request can be computed) but the drag
  session remains until released/removed; this path is exercised only by the
  fuzz test's invariants, not a dedicated named test.
