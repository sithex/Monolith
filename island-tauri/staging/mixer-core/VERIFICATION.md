# VERIFICATION — mixer-core crate

Location: `island-tauri/staging/mixer-core/`.
Scope: pure core behind SPEC §0.5 "Design direction" (Apps bullet) and §8
"Phase 6b" — the Controls + Volume mixer app ("volume, brightness, quick
toggles; per-app streams"). The Tauri actor runs `pactl` (PipeWire's pulse
compatibility) and passes its TEXT output here; this crate parses it into a
mixer model (`Sink`/`Stream`/`MixerModel`/`AppGroup`) and builds the argv
for volume/mute commands. It never runs anything. Platform-free: no IO, no
network, no threads, no clock, no `unsafe`, never panics on any input.
Dependencies: serde (derive) + serde_json only. Wire types use camelCase via
serde, no `#[serde(alias)]`. Upstream facts (index/name/description/mute/
volume/base_volume/state; sink-input index/sink/client/corked/mute/volume/
balance/properties; PA_VOLUME_NORM 65536; event line shapes; command shapes)
are captured from `pactl 17.0` by the controller and used verbatim as test
fixtures.

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
    Checking mixer-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/mixer-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.00s
exit=0

$ nice -n 10 cargo test --locked
   Compiling mixer-core v0.1.0 (.../mixer-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.21s
     Running unittests src/lib.rs (target/debug/deps/mixer_core-465c122b6547622d)

running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-7e96e76b88085071)

running 1 test
test fuzz_mixer_never_panics_and_keeps_invariants ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/mixer_tests.rs (target/debug/deps/mixer_tests-818b3626ebc26056)

running 35 tests
test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests mixer_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total: 44 tests (8 unit + 1 fuzz + 35 integration), all green.

## Mutation checks (all actually run on the current code)

Method: snapshot the pristine tree to scratch (`--exclude=target`), apply
one breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record
failing test names, restore pristine (`rsync -a --delete` + `cargo clean`),
then `diff -r --exclude=target <pristine> .` verified empty for every
mutation (printed `RESTORED IDENTICAL` each time), and the final full gate
re-run on the restored tree is green (above). Every mutation compiled (a
non-compiling mutation would not count). Each fails *named* tests, not only
the fuzz test. Note: the suggested "norm = 65535" mutation was tried first
and survived (every covered boundary value still rounds to the same
number — the ±1 relative change never crosses a rounding threshold), so it
was replaced by the rounding-mode mutation below, which does break a named
test.

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | Percent rounding truncated instead of round-half-up (the `+ ½` term dropped in `value_to_percent`) | `parse::tests::value_to_percent_boundaries` |
| 2 | appName fallback skips `application.process.binary` (only `application.name` → `media.name` → "Unknown") | `model_apps_from_fixture`, `parse_sink_inputs_app_name_fallback_chain`, `parse_sink_inputs_parses_fixture` |
| 3 | Grouping by appName only, pid ignored (`(app, None)` key in `apps()`) | `model_same_app_two_pids_two_groups` |
| 4 | Percent 151 accepted (`validate_pct` bound raised to 1000) | `cmd::tests::percent_bounds`, `set_sink_volume_argv`, `set_stream_volume_argv`, `set_app_volume_argv` |
| 5 | Leading '-' sink name accepted (`starts_with('-')` guard dropped) | `cmd::tests::sink_name_validation`, `cmd::tests::name_is_checked_before_percent`, `invalid_sink_name_rejected_in_all_name_builders` |
| 6 | Server event refreshes `None` instead of `Both` | `needs_refresh_scopes` |

## Decisions

- **Rounding is half-up** (`round(value × 100 / 65536)`, ties away from
  zero): `(value × 100 + 32768) / 65536`. The exact half occurs when
  `value × 100 ≡ 32768 (mod 65536)` — e.g. value 8192 → 12.5 % → 13 %.
  655 ≈ 1.0 % (0.9994 → 1), 327 ≈ 0.499 % → 0, 328 ≈ 0.5005 % → 1.
  Pathological inputs saturate at `u32::MAX` (never panics).
- **`channels` counts valid channels** — channel entries whose object has an
  integer `value` — and is the denominator of the average. An element whose
  `volume` is an object but has no valid channel is kept with
  `volume_percent: None` and `channels: 0` (a *missing/wrong-typed* volume
  still skips the element).
- **"Skipped and counted"** is exposed via `parse_sinks_report` /
  `parse_sink_inputs_report` returning `(items, skipped)`; the spec's
  `parse_sinks`/`parse_sink_inputs` wrap those and drop the count.
- **Element tolerance.** Required per element: sinks `index`/`name`/
  `volume` (object); streams `index`/`sink`/`volume` (object). Missing or
  wrong-typed required field → element skipped and counted. Other fields are
  lenient: missing or wrong-typed `mute`/`corked` → `false`, `description`/
  `state` → `""`, properties → absent.
- **appName fallback**: `application.name` → `application.process.binary` →
  `media.name` → `"Unknown"`. `pid` parses `application.process.id`; a
  non-numeric value is `None` (never skips).
- **Grouping** by exact `(app_name, pid)`; a pid-less stream groups with
  other pid-less streams of the same app name. Sort is by app name
  case-insensitive then pid; this is implemented by pre-sorting the stream
  list (stable) before grouping, so `AppGroup` carries no pid field.
  `volume_percent` = max of members' known volumes (`None` when none had
  one — unknown is shown unavailable, never fabricated as 0); `muted` = all
  members muted; `sink_index` = `Some` iff all members share one sink
  (any disagreement makes it `None` for good, even if later streams agree);
  `idle` = all members corked.
- **Name validation runs before percent validation** in
  `set_sink_volume` (sink name first, then percent). Sink names: non-empty,
  ≤ 256 bytes, no whitespace, no NUL, no leading `-`.
- **Percent is 0..=150 inclusive**; anything above is a typed error, never
  silently clamped (and a `u32` cannot be negative). `MuteOp::Off/On/
  Toggle` → `"0"`/`"1"`/`"toggle"`.
- **Event grammar** is exactly `Event '<kind>' on <facility> #<index>` with
  no extra tokens; the kind must be quoted and index numeric after `#`.
  Unknown facilities map to `PaFacility::Other(String)`.
- **`needs_refresh`**: sink → `Sinks`; sink-input and client → `Streams`;
  server → `Both` (default sink may have changed); everything else → `None`.
- **Size cap** is strictly greater-than 4 MiB (`input.len() > 4 MiB`); the
  boundary itself (exactly 4 MiB) parses.

## UNVERIFIED

- **Tauri actor wiring** — running `pactl` subprocesses, subscribe loop, and
  calling the built argv are outside this crate's scope (it never spawns or
  runs anything).
- **Live pactl versions** — fixtures come from `pactl 17.0` on Nick's
  PipeWire machine; JSON field shapes on other versions are untested.
- **Volume values above 100 % from upstream** — the model can carry a
  percent > 150 if upstream reports one (e.g. 98304 → 150 is the cap in the
  fixture); command builders reject anything > 150 regardless.
- **Deeply nested JSON** — `serde_json`'s recursion limit (128) makes such
  input an `InvalidJson` error rather than a processed file.
- **Concurrency / shared state** — the crate is single-threaded with no
  locks; the actor serializes access.
- **Real media role / icon semantics** — `media.role`, `balance`, and other
  keys are ignored by design.

## Follow-up after controller audit

Controller audit flagged three issues in `src/model.rs` grouping; all fixed
with named tests, and the new behavior is pinned by mutations below.

1. **BUG `sink_index`** — the fold used `None` both for "no stream yet" and
   "mixed sinks", so streams on sinks 1, 2, 1 ended as `Some(1)`. The fold
   now tracks a `mixed` flag alongside each group: any disagreement makes
   `sink_index` `None` for good (even if later streams agree). Pinned by
   `model_sink_index_mixed_stays_none` ([1,2,1]→None, [1,1]→Some(1),
   [2]→Some(2)).
2. **GAP `idle`** — mutating `g.idle = g.idle && s.corked` to
   `g.idle = s.corked` survived all tests. Added the discriminating order to
   `model_idle_flag_corked_only`: [uncorked, corked] → `false` (a final
   corked stream must not override an earlier uncorked one), [corked,
   corked] → `true`.
3. **SPEC unknown volume** — `volumePercent` was `u32` initialised to 0
   (fabricating volume for fully-unknown groups). It is now
   `Option<u32>` = max of members' known volumes, `None` when no member has
   one; serde round-trip pins `null` (not 0). Updated README, tests, and
   this file.

### Gate commands — final state (all green, under `nice -n 10`)

```
$ nice -n 10 cargo fmt --check
(clean)
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking mixer-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/mixer-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s
exit=0

$ nice -n 10 cargo test --locked
     Running unittests src/lib.rs (target/debug/deps/mixer_core-465c122b6547622d)
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-7e96e76b88085071)
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/mixer_tests.rs (target/debug/deps/mixer_tests-818b3626ebc26056)
running 37 tests
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests mixer_core
running 0 tests
exit=0
```

Total: 46 tests (8 unit + 1 fuzz + 37 integration), all green.

### Follow-up mutations (each compiled, fails a named non-fuzz test, reverted
and diff-checked identical — every restore printed `RESTORED_IDENTICAL`)

| # | Breakage applied | Failing named test observed |
|---|---|---|
| 1 | `g.idle = g.idle && s.corked` → `g.idle = s.corked` | `model_idle_flag_corked_only` |
| 2 | Mixed-sink fix reverted to old `None`-means-unset fold | `model_sink_index_mixed_stays_none` |
| 3 | `volume_percent` initialised to `Some(0)` | `model_volume_percent_max_and_none_when_no_known` |

