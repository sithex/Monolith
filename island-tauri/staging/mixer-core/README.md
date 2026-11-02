# mixer-core

Pure core for the Hermes Island **Phase 6b Controls + Volume mixer** app
(SPEC §0.5 "Design direction" Apps bullet, §8 "Phase 6b": "Controls (volume,
brightness, quick toggles) + Volume mixer (per-app streams)"). The Tauri
actor runs `pactl` (PipeWire's pulse compatibility) and passes its TEXT
output here; this crate parses it into a mixer model and builds the argv for
volume/mute commands. **It never runs anything.**

Platform-free: no IO, no network, no threads, no clock, no `unsafe`, never
panics on any input. Dependencies: `serde` (derive) + `serde_json` only.
Wire types use camelCase via serde, no aliases.

## Quick tour

```rust
use mixer_core::{
    parse_sinks, parse_sink_inputs, parse_default_sink, MixerModel,
    set_sink_volume, mute_sink, MuteOp,
};

let sinks = parse_sinks(&pactl_sinks_json)?;          // Vec<Sink>
let streams = parse_sink_inputs(&pactl_streams_json)?; // Vec<Stream>
let default = parse_default_sink("alsa_output...");    // Option<String>
let model = MixerModel::from(sinks, streams, default);

for app in model.apps() {
    // AppGroup { app_name, streams, volume_percent, muted, sink_index, idle }
    let _ = set_app_volume(&app, 60)?;   // one argv per stream
}

let sink = model.default_sink().unwrap();             // by name
let argv = set_sink_volume(sink, 75)?;                // ["pactl","set-sink-volume",name,"75%"]
let argv = mute_sink(sink, MuteOp::Toggle)?;          // ["pactl","set-sink-mute",name,"toggle"]
```

## Parsing

- `parse_sinks` / `parse_sink_inputs` accept `pactl -f json list sinks` /
  `list sink-inputs` output. Top level must be a JSON array (else a typed
  error); input > 4 MiB is rejected. Per-element tolerance: an element
  missing its required fields (`index`/`name`/`volume` for sinks,
  `index`/`sink`/`volume` for streams) or carrying wrong types is skipped
  and counted (see `parse_sinks_report` / `parse_sink_inputs_report` for the
  skip count).
- Volume percent = `round(value × 100 / 65536)`, round half up (PA_VOLUME_
  NORM = 65536 = 100 %). `volumePercent` is the average of the sink's/stream's
  channels; `None` when no channel has a numeric value. `channels` counts the
  valid channels.
- `parse_event` maps `pactl subscribe` lines like `Event 'change' on sink-input
  #2127` to a `PaEvent` (`None` on malformed lines); `needs_refresh` maps it
  to a `RefreshScope` (`Sinks` / `Streams` / `Both` / `None`).

## Model

`MixerModel::from(sinks, streams, default_sink)`:

- `apps()` groups streams by `(appName, pid)` — two windows of the same app
  with the same pid are one group — sorted by appName case-insensitive then
  pid. `volumePercent` = max of members' known volumes (`None` when no member
  has one — unknown is shown unavailable, never fabricated as 0); `muted`
  = all members muted; `sinkIndex` = Some iff all members share one sink
  (`None` whenever they ever disagree, even if later streams agree);
  `idle` = all members corked. Corked-only groups are kept (flagged
  `idle`).
- `default_sink()` returns the sink whose name matches the default sink name,
  or `None`.

## Command builders

Each returns `Result<Vec<String>, CmdError>` (or `Vec<Vec<String>>` for
`set_app_volume`); nothing is executed.

- `set_sink_volume(&Sink, pct)` → `pactl set-sink-volume <name> <pct>%`
- `set_stream_volume(index, pct)` → `pactl set-sink-input-volume <index> <pct>%`
- `set_app_volume(&AppGroup, pct)` → one argv per stream
- `mute_sink(&Sink, MuteOp)` / `mute_stream(index, MuteOp)` →
  `pactl set-sink-mute <name> <0|1|toggle>` / `set-sink-input-mute`
- `set_default_sink(&Sink)` → `pactl set-default-sink <name>`
- `move_stream(index, &Sink)` → `pactl move-sink-input <index> <name>`

Validation (typed errors, never silent clamps): percent must be in 0..=150
(`PercentOutOfRange`); sink names must be non-empty, ≤ 256 bytes, contain no
whitespace or NUL, and not start with `-` (`InvalidSinkName`). In
`set_sink_volume` the name is validated before the percent.

## Development

    nice -n 10 cargo generate-lockfile
    nice -n 10 cargo fmt --check
    nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    nice -n 10 cargo test --locked

`unwrap`/`expect` are denied in non-test code (`[lints.clippy]`); tests may
use them (`clippy.toml`). Run under `nice -n 10` because another agent runs
timing-sensitive GUI tests on the same machine.
