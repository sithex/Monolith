# VERIFICATION — system-tiles-core crate

Pure library for the Hermes Island Controls system tiles (SPEC §0.5):
Keep awake (ScreenSaver inhibit state machine), Audio output switch
(pactl sinks + default-sink argv), Network / Tailscale (NetworkManager +
tailscale status), colour picker (KWin ARGB -> hex/CSS), Hermes voice
switcher (hermes-fish). Pure: no IO, no network, no threads, no clock,
no `unsafe`, never panics. Deps: serde (derive), serde_json only.

## Gate commands — final state (all green, under `nice -n 10`)

```
$ nice -n 10 cargo generate-lockfile
    Locking 14 packages to highest compatible versions
exit=0

$ nice -n 10 cargo fmt --check
(clean)
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s
(no warnings)
exit=0

$ nice -n 10 cargo test --locked
     Running unittests src/lib.rs (target/debug/deps/system_tiles_core-...)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-...)
test fuzz_system_tiles_never_panics_and_keeps_invariants ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/system_tiles_tests.rs (target/debug/deps/system_tiles_tests-...)
test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests system_tiles_core
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total: 66 tests (3 unit + 1 fuzz + 62 integration), all green. The fuzz
test drives every parser and the KeepAwake state machine with a hand-rolled
LCG and asserts no panic and key invariants (unknown stays None, caps hold,
state machine never leaks a cookie).

## Mutation checks (all actually run on the current code)

Method: snapshotted `src/` + `tests/` to scratch
(`~/.hermes/cache/scratch/system-tiles-pristine/`); for each mutation applied
one breakage, ran `nice -n 10 cargo test --locked --no-fail-fast`, recorded
the named (non-fuzz) failing tests, restored the file from the pristine
snapshot, verified `diff -r <crate src/tests> <pristine src/tests>` empty
(printed `IDENTICAL`), and `touch`ed the restored file before the next step.
Every mutation compiled (a non-compiling mutation does not count). Each fails
*named* tests, not only the fuzz test. The final full gate re-run on the
restored tree is green (above) and the final diff against pristine is empty.

| # | Breakage applied | Failing named tests observed |
|---|---|---|
| 1 | Late inhibit cookie absorbed into `On` instead of released (`inhibited` cancel-pending arm keeps the cookie) | `keepawake_late_cookie_released_after_cancel_pending`, `keepawake_tick_due_while_requesting_marks_cancel_pending` |
| 2 | Auto-off tick `>=` changed to `>` (`tick` boundary off by one) | `keepawake_tick_due_releases_exactly_at_boundary`, `keepawake_tick_due_while_requesting_marks_cancel_pending` |
| 3 | `set_default_argv` leading-dash guard dropped (`starts_with('-')` removed) | `set_default_argv_rejects_invalid_names` |
| 4 | Tailscale zero-time `LastSeen` kept (filter on `0001-01-01T00:00:00Z` removed) | `parse_tailscale_status_fixture` |
| 5 | NM state 70 mapped to `ConnectedLocal` instead of `ConnectedGlobal` | `nm_state_from_u32_all_values`, `network_tile_maps_full_snapshot`, `wire_types_serialize_camel_case` |
| 6 | Voice `[active]` suffix treated as non-active (returned `(name, false)`) | `parse_voice_list_fixture`, `parse_voice_list_multiple_active_is_error`, `parse::tests::voice_line_accepts_active_and_plain` |
| 7 | `remove` removed from reserved `hermes-fish` words (`RESERVED_VOICE_ARGS`) | `switch_argv_rejects_reserved_even_if_in_list`, `switch_argv_ok_and_errors` |
| 8 | ARGB channel order swapped (r/b exchanged in both `argb_to_hex` and `argb_to_rgba_css`) | `argb_to_hex_chanks`, `argb_to_rgba_css_values` |

All 8 mutations killed named tests, were reverted to a byte-identical tree,
and the final gate on the restored tree is green.

## Decisions

- **Keep-awake cookie lifecycle.** `turn_off` from `On` returns `UnInhibit`;
  `turn_off` while `Requesting` marks `cancel_pending` and a late
  `inhibited(cookie)` immediately returns `UnInhibit(cookie)` — a cookie is
  never leaked. A spurious `inhibited` in any other state (`Off`/`On`) is
  also released defensively. `failed(msg)` only fires from `Requesting`.
  `on_bus_lost()` → `Off` with no action (cookies die with the connection).
- **Auto-off boundary is inclusive**: `tick` fires when `now_ms >=
  auto_off_at_ms`; at the exact deadline it releases (or, if still
  `Requesting`, marks a pending cancel). `turn_off`/bus-lost clear the
  deadline.
- **`set_default_argv`** rejects empty names, > 256 bytes, a leading `-`
  (argv-injection guard), and any whitespace/control char.
- **Tailscale** zero-time `LastSeen` (`0001-01-01T00:00:00Z`) → `None`
  (never/unknown). Peers sorted online-first then by host name. Missing
  `Peer` map → empty list (not an error); missing `BackendState` → typed
  error; unknown `BackendState` string kept as `Unknown(raw)`.
- **NetworkManager numeric mapping** follows NM docs: 0 UNKNOWN, 10 ASLEEP,
  20 DISCONNECTED, 30 DISCONNECTING, 40 CONNECTING, 50 CONNECTED_LOCAL,
  60 CONNECTED_SITE, 70 CONNECTED_GLOBAL; unknown numeric → `None` + counted
  warning. `showWifiToggle` only when `has_wireless_device`.
- **Voice list parsing.** A voice line is exactly two leading spaces + a
  valid name (1..=64 chars of `[A-Za-z0-9_.-]`) + nothing/whitespace, or +
  exactly `  [active]` (+ optional trailing whitespace). More than one active
  → typed error; other invalid lines count as warnings and are skipped. The
  header `Fish Audio voices:` and blank lines are skipped silently. Reserved
  subcommand words (`current`, `add`, `remove`, `id`, `list`) are never valid
  switch targets, even if a line of that name existed in the list.
- **Colour.** KWin `pick()` returns `0xAARRGGBB`; hex drops alpha
  (lowercase `#rrggbb`); CSS `rgba(r, g, b, a)` uses `a / 255` as f32.
- **Sinks.** Top level must be a JSON array; an element missing a numeric
  `index` or string `name`/`description` is skipped and counted; other fields
  are lenient (`state` → `""`, `mute` → `false`). Size cap is strict
  greater-than (`input.len() > 4 MiB`); exactly 4 MiB parses. Same strict
  greater-than semantics for tailscale (8 MiB) and voice list (64 KiB).
- **Every parser is total**: oversize/malformed input gives a typed error or
  counted warning; unknown numeric/string values stay `None`; nothing is
  fabricated; no panics (fuzz-verified).

## UNVERIFIED

- **Live system commands were never run** (per rules: no pactl, busctl,
  tailscale, hermes-fish, spectacle). Parser fixtures and argv shapes follow
  SPEC §0.5 and the controller-provided facts; they have not been
  cross-checked against real `pactl -f json list sinks` /
  `tailscale status --json` / `hermes-fish` output on a machine.
- **Wire shape** is verified by serde camelCase serialization snapshots in
  tests only; the actual Tauri-side IPC consumer's expectations were not
  exercised here.
- **KWin ARGB interpretation** (`0xAARRGGBB`) is taken from SPEC and tested
  against that assumption; it was not validated against a live KWin reply.
- The Tauri actor wiring (running commands, D-Bus calls, feeding snapshots)
  lives outside this crate and is not covered here.

## Follow-up after controller audit

Reserved for a future audit pass (mirrors mixer-core layout).
