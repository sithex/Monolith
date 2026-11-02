# VERIFICATION — quick-toggles-core crate

Location: `island-tauri/staging/quick-toggles-core/`.
Scope: pure core behind SPEC §0.5 "Design direction" (bullet "Quick
toggles") — the Controls quick toggles (power profile, mic mute, DND).
The Tauri actor owns the D-Bus and process machinery; this crate converts
zbus-derived values into tile states, builds exact `pactl` argv, parses
`pactl get-source-mute` text, and runs the optimistic toggle state
machine. It never runs anything. Platform-free: no IO, no network, no
threads, no clock, no `unsafe`, never panics on any input. Dependencies:
serde (derive) + serde_json only. Wire types use camelCase via serde, no
`#[serde(alias)]`. Verified bus facts (power-profiles-daemon v0.30,
system bus `org.freedesktop.UPower.PowerProfiles`, path
`/org/freedesktop/UPower/PowerProfiles`, interface
`org.freedesktop.UPower.PowerProfiles`; `pactl` 17) were captured
read-only on nick-pc by the controller and are implemented verbatim.

## Gate commands — final state (all green, under `nice -n 10`)

```
$ nice -n 10 cargo generate-lockfile
    Updating crates.io index
     Locking 11 packages to highest compatible versions
exit=0

$ nice -n 10 cargo fmt --check
(clean)
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking quick-toggles-core v0.1.0 (.../quick-toggles-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.16s
exit=0

$ nice -n 10 cargo test --locked
     Running unittests src/lib.rs (target/debug/deps/quick_toggles_core-...)
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-...)
running 1 test
test fuzz_quick_toggles_never_panics_and_keeps_invariants ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/mic_tests.rs (target/debug/deps/mic_tests-...)
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/power_tests.rs (target/debug/deps/power_tests-...)
running 15 tests
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/toggle_tests.rs (target/debug/deps/toggle_tests-...)
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests quick_toggles_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total: 43 tests (11 unit + 1 fuzz + 10 mic + 15 power + 6 toggle), all
green.

## Mutation checks (all actually run on the current code)

Method: snapshot the pristine tree to scratch (`--exclude=target`), apply
one breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record
failing test names, restore the mutated source from the pristine snapshot
(plain copy → fresh mtime, so no stale binary reuse), then
`diff -r --exclude=target <pristine> .` verified empty for every mutation
(`RESTORED_IDENTICAL` printed each time), and the final full gate re-run
on the restored tree is green (above). Every mutation compiled (a
non-compiling mutation would not count) and each fails *named* non-fuzz
tests.

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | `next_profile` skips wrap (returns `None` at the last choice instead of cycling to the first) | `next_profile_missing_performance_two_choices_cycles`, `next_profile_three_profiles_cycles_all_ways` |
| 2 | Unknown `active` treated as `Balanced` (`unwrap_or(Balanced)`) | `next_profile_unknown_active_none`, `tile_unknown_active_gives_none_available`, `tile_unknown_only_profiles_empty_choices` |
| 3 | Unknown profile strings no longer counted in `ignored` (`Err(_) => {}`) | `tile_full_three_profiles_with_unknown_ignored`, `tile_unknown_only_profiles_empty_choices` |
| 4 | `parse_mic_mute` lowercases input (accepts `mute: yes`/`MUTE: YES`) | `parse_rejects_case_insensitive` |
| 5 | `fail` clears any pending regardless of request id (stale fail clears newer pending) | `toggle::tests::fail_stale_id_ignored`, `toggle::tests::new_request_replaces_pending_with_new_id`, `toggle_fail_stale_id_ignored` |
| 6 | Timeout boundary off by one (`> 3000` instead of `>= 3000`) | `toggle::tests::tick_keeps_before_boundary_drops_at_and_after`, `toggle_tick_timeout_drops_pending` |
| 7 | `pactl` argv targets a hard-coded device name instead of `@DEFAULT_SOURCE@` | `mic_set_argv_exact_all_ops`, `mic_get_argv_exact` |
| 8 | `choices` built in reverse canonical order (performance, balanced, power-saver) | `tile_full_three_profiles_with_unknown_ignored`, `tile_unknown_active_gives_none_available`, `next_profile_missing_performance_two_choices_cycles`, `next_profile_three_profiles_cycles_all_ways`, `next_profile_active_not_in_choices_none` |

## Decisions

- **Unknown profiles are ignored and counted.** Profile strings in
  `Profiles` that are not power-saver/balanced/performance are dropped
  from `choices` and counted in a `ignored` field on `PowerTile` (wire:
  `ignored`). They never fail the snapshot; the tile stays available.
  An unknown `active` yields `active: None` but `available: true`.
- **`heldBy` = hold reasons.** `ActiveProfileHolds` entries are mapped to
  their `Reason` strings, in list order, with empty reasons filtered out.
  (Reasons are the human-meaningful explanation of why the active profile
  is held; the `ApplicationId`/`Profile` fields are not surfaced.)
- **`next_profile` semantics.** Cycles `choices` in canonical order
  (power-saver → balanced → performance) and wraps. Returns `None` when
  there are fewer than two choices, when `active` is `None`, or when the
  active profile is not present among `choices` (treated the same as
  unknown). Profiles absent from the daemon's `Profiles` list are simply
  not offered.
- **Timeout boundary** is `elapsed >= 3000 ms` (`tick` at
  `sent_at_ms + 3000` drops the pending value; at `sent_at_ms + 2999` it
  keeps). A `now_ms` before `sent_at_ms` saturates to 0 elapsed and never
  drops. `display()` shows the pending value while one exists, else the
  confirmed value (it does not itself age-check; `tick` does).
- **`confirm` semantics.** An observation equal to the pending value
  clears the pending AND updates `confirmed` to the observed value (so a
  successful optimistic request does not fall back to stale confirmed
  data). A different observation updates `confirmed` and keeps the pending
  until timeout. With no pending, any observation just updates
  `confirmed`. A stale `fail`/`confirm` for an older request id never
  clears a newer pending.
- **`parse_mic_mute` is strict.** Accepts exactly `Mute: yes` or
  `Mute: no`, optionally followed by a single `\n` or `\r\n` (a lone
  `\r` is rejected). Case-sensitive. Anything else — localized text,
  extra lines, empty, whitespace padding — is `ParseError::InvalidOutput`;
  input over 256 bytes is `ParseError::TooLarge`. Never a guess.
- **Mic always targets `@DEFAULT_SOURCE@`** (nick-pc's default source is
  a DeepFilterNet virtual source), never a device name. Argument map:
  `Mute` → `1`, `Unmute` → `0`, `Toggle` → `toggle`.
- **`Profile` serde is kebab-case** (`power-saver`, `balanced`,
  `performance`); wire structs are camelCase (`applicationId`).
- **`ToggleState` ids** are a monotonically increasing internal counter;
  each `request` returns a new id and replaces any previous pending.

## UNVERIFIED

- No live end-to-end test against a running power-profiles-daemon or
  `pactl` (per policy, nothing here reads or changes the real mic/audio/
  power profile; the Tauri actor runs the processes).
- Whether `ActiveProfileHolds` reasons are ever empty in practice on
  nick-pc (empty reasons are filtered out by design).
- Locale/l10n behavior of `pactl get-source-mute` under LC_ALL=C on other
  machines; the parser intentionally accepts only the canonical English
  `Mute: yes|no` shape.
- `DndTile` wire consumption by the settings module (owned elsewhere; not
  exercised end-to-end here). `PENDING_TIMEOUT_MS = 3000` is a chosen
  constant; display timing may need tuning with the real UI.
