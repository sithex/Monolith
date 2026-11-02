# system-tiles-core

Pure Rust library backing the Hermes Island Controls system tiles
(SPEC §0.5): **Keep awake** (session-bus ScreenSaver Inhibit/UnInhibit state
machine), **Audio output switch** (pactl sinks + default-sink argv),
**Network / Tailscale** (NetworkManager + `tailscale status --json`),
**colour picker** (KWin ARGB → hex/CSS), and the **Hermes voice switcher**
(`hermes-fish`).

The Tauri actor runs the commands and D-Bus calls; this crate only **models,
parses and builds argv**. It never runs anything.

## Guarantees

- Pure library: no IO, no network, no threads, no clock (time is always an
  injected `now_ms` parameter), no `unsafe` (`#![forbid(unsafe_code)]`),
  no process spawning.
- No panics on any input. Every parser is total: oversize/malformed input
  gives a typed error or a counted warning; unknown values stay `None`.
- Wire types use `camelCase` through serde and accept no aliases.
- Dependencies: `serde` (derive) and `serde_json` only.

## Modules

- `keepawake` — `KeepAwake` state machine: `turn_on` → `Action::Inhibit`,
  `inhibited(cookie)` → `On`, `turn_off` → `UnInhibit` (a cancel-pending
  late reply is always released — cookies never leak), optional timed
  auto-off (`tick`), `failed`, `on_bus_lost`.
- `parse` — total parsers with size caps: `parse_sinks[_report]`
  (4 MiB), `parse_default_sink`, `parse_tailscale_status[_report]`
  (8 MiB), `parse_voice_list[_report]` / `parse_current` (64 KiB),
  `is_valid_voice_name`.
- `cmd` — argv builders: `set_default_argv` (validates name), `switch_argv`
  (rejects reserved words and unknown voices), `screenshot_argv`.
- `color` — `argb_to_hex` (`#rrggbb`, alpha dropped), `argb_to_rgba_css`.
- `tiles` — `output_tile`, `network_tile[_report]`.
- `model` — wire types (`Sink`, `OutputTile`, `NmState`, `NmConnectivity`,
  `NetKind`, `NetworkTile`, `Peer`, `TailscaleTile`, `VoiceList`).
- `error` — `ParseError`, `CmdError`.

## Usage

```rust
use system_tiles_core::*;

let (sinks, skipped) = parse_sinks_report(PACTL_JSON).unwrap();
let tile = output_tile(sinks, "alsa_output.pci-0000_00_1f.3.analog-stereo");
let argv = set_default_argv("alsa_output.pci-0000_00_1f.3.analog-stereo");

let mut ka = KeepAwake::new();
ka.set_auto_off(Some(AUTO_OFF_30M), now_ms);
ka.turn_on(now_ms, "user asked");
let cookie = /* after bus reply */ 7;
ka.inhibited(cookie);
if let Some(Action::UnInhibit { .. }) = ka.turn_off() { /* send it */ }
```

## Testing

```
nice -n 10 cargo fmt --check
nice -n 10 cargo clippy --all-targets --locked -- -D warnings
nice -n 10 cargo test --locked
```

66 tests: 3 unit + 1 never-panic fuzz (LCG over every parser and the state
machine) + 62 integration (parse fixtures, argv validation, serde camelCase
snapshots, state-machine lifecycle). See VERIFICATION.md for gate output and
the mutation table.
