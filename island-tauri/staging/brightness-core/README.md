# brightness-core

Pure core for the Hermes Island **Phase 6b Controls** app (SPEC §0.5
"Design direction" Apps bullet — "Controls (volume, brightness, quick
toggles)"; §8 "Phase 6b" — "Controls + Volume mixer (volume, brightness,
quick toggles; per-app streams). Pure cores go to `staging/` first").
The Tauri actor talks to KDE PowerDevil over D-Bus (zbus) and passes
DECODED values here; this crate keeps the display model, converts percent
<-> raw values, coalesces slider drags and builds the method-call requests.
**It never does IO.**

Platform-free: no IO, no network, no threads, no clock, no `unsafe`, never
panics on any input. Dependencies: `serde` (derive) + `serde_json` only.
Wire types use camelCase via serde, no aliases. All time is an injected
`now_ms: u64`.

Upstream facts (org.kde.ScreenBrightness + org.kde.ScreenBrightness.Display,
from PowerDevil's daemon/dbus XML, confirmed by introspecting the Plasma 6
session bus) are used verbatim as test fixtures; the fixtures never touch
D-Bus or change brightness.

## Quick tour

```rust
use brightness_core::{
    display_path, validate_display_name, BrightnessModel, Event, Step, step_request,
    Display, percent, raw_for_percent, CONTEXT, SUPPRESS_INDICATOR,
};

assert!(validate_display_name("display0").is_ok());
assert_eq!(display_path("display0").unwrap(), "/org/kde/ScreenBrightness/display0");

let d = Display { name: "display0".into(), label: "DELL".into(),
                  is_internal: false, max_brightness: 10000, brightness: 5000 };
assert_eq!(percent(&d), Some(50));
assert_eq!(raw_for_percent(&d, 100), Ok(10000));

let mut m = BrightnessModel::new();
let a = m.apply(Event::DisplayNames(vec!["display0".into()])); // fetch ["display0"]
let a = m.apply(Event::Props {
    name: "display0".into(), label: "DELL".into(), is_internal: false,
    max_brightness: 10000, brightness: 5000,
}); // fetch cleared

m.drag("display0", 60, 0)?;          // user grabs the slider at 60%
let reqs = m.poll(0);                 // first drag emits immediately
// reqs[0] = SetRequest { name, path, brightness: 6000, flags: 0x1, context: "hermes-island" }
m.drag("display0", 70, 10)?;
assert!(m.poll(49).is_empty());       // throttled (50 ms)
let reqs = m.poll(50);                // latest target wins -> 7000
let flushed = m.release("display0", 60); // flush pending immediately, end drag

let step = step_request(Step::Increase); // stepAction 0, flags 0x1, context CONTEXT
```

## What the model does

- `BrightnessModel::apply(Event)` mutates the display list (kept in
  `DisplaysDBusNames` order, max 16) and returns `Applied { changed, fetch }`.
  `fetch` lists new displays that have no Props yet (each at most once until
  Props arrive or they are removed). Unknown/invalid names and names beyond
  the cap are dropped and counted (`ignored()`); brightness outside `0..=max`
  is stored clamped and counted (`clamped()`). `max <= 0` keeps a display but
  marks it unavailable (`percent` = None).
- Slider coalescing: `drag` records the latest target (one session per
  display); `poll(now_ms)` emits at most one `SetRequest` per display per
  50 ms, always sending the latest target, skipping when it equals the last
  sent value; `release` flushes a pending target immediately and ends the
  drag. A drag target is shown in `DisplayView.percent` while dragging and
  for 500 ms after release; after that the model value shows. A
  `BrightnessChanged` with a foreign context still updates the stored
  brightness during a drag (visible after the settle window).
- `next_wakeup(now_ms)` returns the earliest ms at which poll/settle has
  work (next throttle release, or settle expiry for a UI refresh).
- `step_request(Step)` maps `Step::{Increase, Decrease, IncreaseSmall,
  DecreaseSmall}` to `stepAction` 0/1/2/3.
- `Display` / `DisplayView` / `SetRequest` / `StepRequest` / `Event` are
  serde camelCase wired.

## Development

    nice -n 10 cargo generate-lockfile
    nice -n 10 cargo fmt --check
    nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    nice -n 10 cargo test --locked

`unwrap`/`expect` are denied in non-test code (`[lints.clippy]`); tests may
use them (`clippy.toml`). Run under `nice -n 10` because another agent runs
timing-sensitive GUI tests on the same machine.
