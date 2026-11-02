# launcher-arrange-core

Pure core for the Hermes Island **Launcher edit mode** (SPEC §0.5 "Design
direction" — App launcher + Launcher edit mode bullets): the phone-style
"jiggle" edit session for the app grid.

The crate decides the whole edit state machine: which tiles are visible/hidden
(`visible`/`hidden`), where a moved tile lands (`move_item`), lock rules
(`disable` rejects locked tiles), cancel/commit semantics, and how
`reconcile` merges module-registry changes. The Tauri actor applies the
resulting `ArrangeCommit` (a full `order` permutation plus `enabled_changes`)
to the module registry; this crate never imports it.

Platform-free: no IO, no network, no threads, no clock, no `unsafe`, never
panics on any input. Dependencies: `serde` (derive) + `serde_json` only. Wire
types use camelCase (`enabledChanges`) and accept no aliases.

## Quick tour

```rust
use launcher_arrange_core::{Arrangement, GridItem};

let mut arr = Arrangement::new(vec![
    GridItem { id: "home".into(), enabled: true, locked: true },
    GridItem { id: "apps".into(), enabled: true, locked: false },
    GridItem { id: "settings".into(), enabled: true, locked: true },
])?;

arr.begin_edit()?;
arr.move_item("home", 2)?;          // visible-grid index, clamped to last slot
arr.disable("apps")?;               // Locked error for locked tiles
if let Some(commit) = arr.commit()? {
    // Tauri applies commit.order (full permutation) + commit.enabled_changes
}
```

## Rules worth knowing

- Ids: non-empty, each ≤ 64 bytes, unique; at most 256 items. Errors are
  typed (`ArrangeError`).
- A locked item is always enabled (the core forces it at ingestion, see
  Decisions) and cannot be disabled (`Locked`).
- `move_item` positions are visible-grid indices; the target is clamped to the
  last visible slot; moving an item onto its own slot is a no-op; hidden items
  keep their relative order (and positions).
- `enable` puts a re-enabled item at the end of the visible grid.
- Ops outside an edit session → `NotEditing`; unknown ids → `UnknownId`; a
  failed op leaves the state unchanged.
- `reconcile` keeps surviving ids' relative order, drops missing ids, appends
  new ids, always takes `locked` from the newer list, and — during editing —
  applies the same transform to the snapshot so cancel never resurrects a
  removed id.

## Development

    nice -n 10 cargo generate-lockfile
    nice -n 10 cargo fmt --check
    nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    nice -n 10 cargo test --locked

`unwrap`/`expect` are denied in non-test code (`[lints.clippy]`); tests may
use them (`clippy.toml`). Also run `nice -n 10` because another agent runs
timing-sensitive GUI tests on the same machine.
