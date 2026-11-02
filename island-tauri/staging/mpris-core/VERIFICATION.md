# VERIFICATION — mpris-core crate

Location: `island-tauri/staging/mpris-core/`.
Scope: pure core behind SPEC §3.5's "Now playing" row (MPRIS, preferred player =
most recently Playing, position interpolated) and the Phase 6 lyrics timing that
needs an accurate playback position. D-Bus values arrive already converted to
`serde_json::Value`; times are injected `u64` monotonic milliseconds. Platform-free:
no IO, no network, no threads, no clock, no `unsafe`. Dependencies: serde
(derive) + serde_json only. Never panics on any input (pseudo-random fuzz test).

## Gate commands — final state (all green)

```
$ nice -n 10 cargo generate-lockfile
    Updating crates.io index
     Locking 11 packages to highest compatible versions
exit=0

$ nice -n 10 cargo fmt --check
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking mpris-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/mpris-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s
exit=0

$ nice -n 10 cargo test --locked
   Compiling mpris-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/mpris-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.21s
     Running unittests src/lib.rs (target/debug/deps/mpris_core-68a33f7dbf05f82e)

running 3 tests
test tests::eviction_caps_at_32 ... ok
test tests::eviction_oldest_last_playing_first ... ok
test tests::eviction_prefers_nonplaying_then_oldest_then_lexical ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/fuzz.rs (target/debug/deps/fuzz-ab99b6ef0421e191)

running 1 test
test never_panics_on_pseudorandom_input ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s

     Running tests/parse.rs (target/debug/deps/parse-202f4ee78b9fa5a3)

running 10 tests
test metadata_artists_array_and_single_string ... ok
test metadata_drops_blank_artists_and_caps_at_sixteen ... ok
test metadata_length_invalid_values_give_none ... ok
test metadata_length_microseconds_to_ms ... ok
test metadata_missing_or_blank_title_is_none ... ok
test metadata_minimal_title_only ... ok
test parse_status_exact_strings ... ok
test parse_status_rejects_anything_else ... ok
test track_wires_camelcase_and_no_alias ... ok
test metadata_strings_trimmed_and_capped_multibyte_safe ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/players.rs (target/debug/deps/players-e57a6dffbc7bfacd)

running 28 tests
test invalid_bus_names_ignored ... ok
test metadata_same_track_does_not_reset ... ok
test metadata_title_change_resets_even_with_same_trackid ... ok
test metadata_track_change_resets_position ... ok
test non_mpris_buses_ignored_by_all_mutating_methods ... ok
test metadata_trackid_change_resets_even_with_same_title ... ok
test pause_freezes_and_resume_continues ... ok
test playing_counts_from_zero ... ok
test position_at_clamps_to_length ... ok
test position_at_clock_backwards_returns_position ... ok
test position_at_paused_frozen ... ok
test position_at_playing_interpolates ... ok
test position_at_rate_nonfinite_or_nonpositive_treated_as_one ... ok
test position_at_rate_scales ... ok
test position_at_stopped_zero ... ok
test position_at_zero_length_clamps_to_zero ... ok
test preferred_empty_and_no_track_none ... ok
test preferred_never_played_lexical_first_with_track ... ok
test preferred_no_playing_latest_last_playing_wins ... ok
test preferred_playing_beats_later_last_playing ... ok
test preferred_tie_break_lexical ... ok
test rate_change_reanchors ... ok
test seek_and_set_position_negative_one_and_sub_ms_boundaries ... ok
test seek_negative_clamps_to_zero ... ok
test seek_while_paused_updates_frozen_position ... ok
test set_position_absolute ... ok
test spotify_bus_accepted ... ok
test stopped_returns_zero_even_after_playing ... ok

test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests mpris_core

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit=0
```

Total: 42 tests (3 unit + 1 fuzz + 10 parse + 28 players), all green.

## Mutation checks (all actually run on the current code)

Method: snapshot pristine tree to scratch (`--exclude=target`), apply one
breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record failing
test names, restore pristine, then `diff -r --exclude=target <pristine> .`
verified empty for every mutation. All eight restored identical, and a final
full gate re-run on the restored tree is green (above). Every mutation
compiled (a non-compiling mutation would not count).

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | Length clamp dropped in `position_at` (`else if pos > len as f64 { len }` removed → position can exceed `lengthMs`) | `position_at_clamps_to_length`, `position_at_zero_length_clamps_to_zero` |
| 2 | Rate ignored in `position_at` (`let rate = 1.0;`, non-finite/≤0 guard removed) | `position_at_rate_scales`, `rate_change_reanchors` |
| 3 | No re-anchor on rate change (`set_rate` no longer recomputes `position_ms` before changing rate) | `rate_change_reanchors` |
| 4 | `preferred` ignores the Playing tier (Playing players no longer ranked first) | `preferred_playing_beats_later_last_playing` |
| 5 | `mpris:length` treated as milliseconds (µs→ms division removed) | `metadata_length_microseconds_to_ms` |
| 6 | Bus-name filter relaxed to any non-empty name (`valid_bus` = `!bus.is_empty()`) | `non_mpris_buses_ignored_by_all_mutating_methods` |
| 7 | Bus-name prefix shortened to `org.mpris.` (bare `org.mpris.MediaPlayer2` accepted) | `non_mpris_buses_ignored_by_all_mutating_methods` |
| 8 | Negative-position clamp off-by-one in `us_to_ms` (`if position_us < 0` → `< -1`) | `seek_and_set_position_negative_one_and_sub_ms_boundaries` |

## Decisions

- **Title is the only required field.** `parse_metadata` returns `None` for a
  missing, non-string, or blank (after trim) `xesam:title`; a title is never
  fabricated. All other keys are optional; blank optional strings become
  `None`.
- **`mpris:length` is microseconds → milliseconds** (`us / 1000`, truncating).
  It must be a JSON integer (integral floats accepted, e.g. `1e6`); zero,
  negative, and non-integer values yield `length_ms = None`. A sub-millisecond
  positive length (e.g. 999 µs) yields `Some(0)`.
- **String caps are character-based and multibyte-safe** (`chars().take(512)`):
  title, trackId, album, artUrl and each artist are trimmed then capped at 512
  chars. Blank artists are dropped; at most 16 artists are kept (the cap counts
  non-blank artists only).
- **Artists**: `xesam:artist` accepts either an array of strings or a plain
  string (treated as a one-element list). Non-string elements are skipped.
- **Position model**: a player remembers `position_ms` sampled at
  `position_at_ms`; `position_at` interpolates while `Playing` with
  `positionMs + (now - positionAtMs) * rate`, clamped to `[0, lengthMs]` when
  the length is known. A non-finite or ≤ 0 rate is treated as 1.0. Clocks
  never run backwards: `now < positionAtMs` returns the frozen `positionMs`.
  `Paused` freezes `positionMs`; `Stopped` is 0.
- **Transition semantics in `upsert_status`**: entering `Playing` re-anchors
  the position (preserving continuity) and records `last_playing_at_ms =
  Some(now)`; leaving `Playing` freezes the interpolated position; entering
  `Stopped` zeroes the position. `last_playing_at_ms` is never cleared by
  pausing/stopping.
- **`set_metadata` resets position to 0 at `now` only when the track id or
  title changed** (first metadata on a player counts as a change). Unparseable
  metadata is ignored. A track id/title change while paused also resets (the
  frozen position becomes 0).
- **`seeked`/`set_position` take microseconds** and convert to ms (`us / 1000`),
  with negative → 0. They only affect players already known to `Players`.
  Boundary pinned by `seek_and_set_position_negative_one_and_sub_ms_boundaries`:
  exactly −1 µs → 0 (guards the `position_us < 0` edge; a `< -1` guard would
  produce a huge `-1i64 as u64 / 1000`), 999 µs → 0 ms, 1000 µs → 1 ms.
- **Bus-name gating**: every mutating method ignores bus names that do not
  start with `org.mpris.MediaPlayer2.` (case-sensitive). `remove` is exact-name.
  Pinned by `non_mpris_buses_ignored_by_all_mutating_methods`, which asserts
  per-method (and before any `remove`) that `org.freedesktop.Notifications`,
  bare `org.mpris.MediaPlayer2`, and "" store nothing and keep `preferred()`
  None, and by `spotify_bus_accepted` for the positive case. Note the pre-
  existing `invalid_bus_names_ignored` calls `remove` after each bad bus, so an
  over-permissive filter that also accepts those names in `remove` can delete
  the stored players and mask the bug — the new test does not have that hole.
- **Eviction at max 32 players**: when a new player would exceed
  `MAX_PLAYERS`, the evicted player is the one with the lowest
  (is-Playing, `last_playing_at_ms`, bus-name) tuple — non-Playing players
  first, then oldest `last_playing_at_ms` (never-played = oldest), then
  lexical bus name. So a currently-Playing player survives while any
  non-Playing player exists.
- **`preferred` tiers**: 1) currently `Playing` with the latest
  `last_playing_at_ms`; 2) if none playing, the player with the latest
  `last_playing_at_ms`; 3) if no player ever played, the lexically first player
  that has a track. Ties within tiers 1–2 break by lexical bus name.
- **Never panic**: all parsing uses `Option`-returning helpers and checked
  arithmetic; no slicing at non-char boundaries (`chars().take`); string caps
  are char-boundary safe; fuzz pinned by `never_panics_on_pseudorandom_input`
  (2000 deterministic LCG-generated metadata maps × 80-op random players
  sequences, plus random `position_at` clocks including backwards jumps, NaN/
  infinite rates) with no new dependencies.
- **Serde wiring**: `Track` and `PlayerState` serialize with camelCase field
  names (`trackId`, `artUrl`, `lengthMs`, `busName`, `positionAtMs`,
  `lastPlayingAtMs`) via `#[serde(rename_all = "camelCase")]`; no
  `#[serde(alias)]` is used, so snake_case keys do not populate fields
  (pinned by `track_wires_camelcase_and_no_alias`).
- **Initial state of a new player**: status `Stopped`, rate 1.0, position 0 at
  the insertion time, track None, never-played.

## UNVERIFIED

- **Live D-Bus / MPRIS integration** — this crate is the pure core; the wiring
  (GDBus/zbus reads, NameOwnerChanged handling, converting D-Bus variants to
  `serde_json::Value`) lives in `src-tauri` and is out of scope here.
- **Real-world player behavior** — pinned by synthetic inputs; the variety of
  real players' metadata quirks (e.g. `xesam:artist` as an array of non-strings,
  unusual length encodings) is exercised via fuzz but not against a corpus.
- **Clock source** — the caller must inject a monotonic clock
  (`position_at_ms` / `now`); no wall-clock or `Instant::now()` usage exists in
  this crate by design.
- **Multiple simultaneous track changes** — a metadata event that changes both
  trackId and title still counts as one reset (position resets once to 0).
- **`preferred` when >1 player is Playing with identical timestamps and lexically
  identical bus names** — impossible for distinct bus names; ties are fully
  resolved by lexical order.
