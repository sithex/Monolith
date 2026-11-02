# layout-core

Pure core for the Hermes Island **Phase 7 side-button Layout editor** (SPEC
§0.5 "Design direction" — Side buttons bullet; §2.4 single input-region
owner). This crate owns the persisted layout model and the geometry math the
shape/input-region owner will use; it renders nothing.

Platform-free: no IO, no network, no threads, no clock, no `unsafe`, and it
never panics on input reached through its API. Dependencies: `serde`
(derive) + `serde_json` only. All geometry is logical px (`f64`, finite).
Wire types use camelCase via serde, no aliases.

## Quick tour

```rust
use layout_core::{ButtonAction, Layout, Rect, Side, SideButton, SizePreset};

let mut layout = Layout::default();          // concept defaults (5 buttons, Compact)
layout.add(SideButton {
    id: "calc".into(),
    side: Side::Left,
    slot: 2,
    action: ButtonAction::OpenApp { app_id: "org.kde.calculator".into() },
    label: Some("Calc".into()),
})?;
layout.move_to("timer", Side::Bottom, 1)?;   // occupied target -> swap
layout.set_preset(SizePreset::Custom { width: 480.0, max_height: 560.0 })?;

// Geometry for the shape/input-region owner:
let island = Rect { x: 100.0, y: 200.0, w: 400.0, h: 80.0 };
let buttons = layout.button_rects(island)?;   // Vec<(String, Rect)> in left/right/bottom order
let region = layout.input_region(island)?;    // island rect + every button rect (one owner)
let win = layout.bounding_box(island)?;       // window sizing

// Persistence (tolerant on invalid pieces, never destructive):
let saved = layout.to_json();
let (reloaded, report) = Layout::from_json(&saved)?;
```

Every editing op validates and returns a typed `LayoutError`, leaving the
layout unchanged on any error.

## Model / limits

- Side slots: Left/Right `0..3`, Bottom `0..5`; at most one button per
  `(side, slot)` pair.
- Ids: `1..=64` chars of `[A-Za-z0-9_.-]`, unique (buttons and embedded
  `appId` / `command id`).
- Labels: optional, ≤ 32 chars.
- Presets: Compact (`360×420`), Spacious (`520×640`), Custom (`width`
  `300..=900`, `maxHeight` `240..=900`, finite).
- Button diameter `28..=56` (default 36), island gap `4..=24` (default 10).

## Geometry

`button_rects` places `d×d` buttons around the island rect (`d` = diameter,
`g` = gap):

- Left: stacked downward from `island.y`, centred at `island.x - g - d/2`;
  consecutive buttons are spaced by `g` (centre pitch `d + g`).
- Right: mirrored at `island.x + island.w + g + d/2`, same `g` spacing.
- Bottom: a row centred on the island centre at `y = island.y + h + g`,
  `count * d + (count - 1) * g` wide, consecutive buttons spaced by `g`.

Within each side, occupied slots are **packed in slot order with no holes**
(the dashed `+` slots come from `empty_slots()`). `input_region` returns the
island plus every button rect — one owner unions them (SPEC §2.4).
`bounding_box` covers everything for window sizing. Non-finite or
negative/zero-size islands are typed errors.

## Persistence format

    { "version": 1,
      "preset": "compact" | "spacious"
             | { "custom": { "width": 480.0, "maxHeight": 560.0 } },
      "buttonDiameter": 36.0, "gap": 10.0,
      "buttons": [ { "id": "...", "side": "left", "slot": 0,
                     "action": "appGrid"
                            | { "openApp": { "appId": "..." } }
                            | { "command": { "id": "..." } },
                     "label": null | "..." } ] }

Fatal typed errors (caller should keep the file aside): invalid JSON,
non-object top level, missing/unknown version. Everything else is tolerant:
- Per-button: invalid id / duplicate id / slot out of range / occupied pair /
  invalid action / invalid label / wrong-typed or non-object element → the
  button is skipped and counted (`LoadReport::skipped`); the first valid
  occurrence of an id wins.
- Invalid `preset` / `buttonDiameter` / `gap` fall back to defaults and are
  flagged (`preset_fallback` / `diameter_fallback` / `gap_fallback`).
- Unknown fields are ignored; missing fields default silently.

## Development

    nice -n 10 cargo generate-lockfile
    nice -n 10 cargo fmt --check
    nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    nice -n 10 cargo test --locked

`unwrap`/`expect` are denied in non-test code (`[lints.clippy]`); tests may
use them (`clippy.toml`). Also run under `nice -n 10` because another agent
runs timing-sensitive GUI tests on the same machine. See `VERIFICATION.md`
for gate output, mutation results and design decisions.
