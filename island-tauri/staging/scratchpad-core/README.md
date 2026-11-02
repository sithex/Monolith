# scratchpad-core

Pure core for the Hermes Island **Phase 6b Scratchpad** app (SPEC §0.5
"Design direction" Apps bullet, §8 "Phase 6b": "plain-text notes in the data
dir, debounced atomic saves").

The crate decides **what** to write (notes, derived titles, limits, ordering)
and **when** (the debounced save scheduler). The Tauri actor owns the actual
file IO: it feeds injected `u64` monotonic-ms timestamps, polls
`SaveScheduler::next_save_at()` / `due()`, calls `begin_save()` when due,
writes `to_json()` to the data dir, then reports success/failure.

Platform-free: no IO, no network, no threads, no clock, no `unsafe`, never
panics on any input. Dependencies: `serde` (derive) + `serde_json` only.

## Quick tour

```rust
use scratchpad_core::{Scratchpad, SaveScheduler};

let mut pad = Scratchpad::new();
pad.new_note(1_000, "n1".to_string())?;          // id: 1..=64 of [A-Za-z0-9_-]
pad.edit(1_200, "n1", "hello\nworld".to_string())?; // title becomes "hello"
pad.set_pinned(1_300, "n1", true)?;
assert!(pad.is_dirty());

// Drive saves via the scheduler:
let sched = pad.scheduler_mut();
sched.flush_requested();                    // app quit -> due immediately
if let Some(gen) = sched.begin_save(1_400) {
    // ... Tauri writes pad.to_json() atomically into the data dir ...
    sched.saved(1_400, gen);               // or sched.save_failed(now)
}
```

`delete` and `set_pinned` also take a `now` parameter (see Decisions in
VERIFICATION.md).

## Persistence format

    { "version": 1, "notes": [ { "id": "...", "title": "...", "body": "...",
      "createdMs": 0, "updatedMs": 0, "pinned": false }, ... ] }

Field names are camelCase. **Load behavior** (`Scratchpad::from_json`):

- Fatal typed errors (`ScratchpadError::InvalidJson`, `NotAnObject`,
  `InvalidVersion`, `NotesNotArray`): the caller should **keep the file
  aside and start empty** — never overwrite a file you couldn't parse.
- Per-note tolerance: invalid id, duplicate id, oversize body (> 256 KiB),
  wrong-typed fields, or non-object elements skip that note and increment
  `LoadReport::skipped`.
- More than 200 valid notes keep only the newest 200 (`truncatedToMax`).
- Titles are recomputed from the body, never trusted from the file.
- Unknown extra fields are ignored. Missing `createdMs`/`updatedMs`/`pinned`/
  `body` default to `0`/`0`/`false`/`""`.

## Limits

- Body ≤ 256 KiB (bytes); a larger body is a typed error and leaves the note
  unchanged.
- At most 200 notes; creating the 201st is a typed error.
- Id: 1..=64 ASCII `[A-Za-z0-9_-]`, unique.
- Title: first non-blank line, trimmed, cut at 80 characters on a character
  boundary (never mid-UTF-8), `"Untitled"` when the body is blank.
- `list()` order: pinned first, then `updatedMs` descending, ties by `id`
  ascending. Editing with an identical body changes nothing (no `updatedMs`
  bump, no dirty).

## Save scheduler

`SaveScheduler { debounce_ms: 750, max_delay_ms: 5000 }`:

- `mark_dirty(now)` pushes the save to `now + debounce`, but never later than
  `first_dirty + max_delay` (continuous typing still saves ≥ every 5 s).
- `begin_save(now) -> Option<u64>` returns the generation being saved (only
  when due). `saved(now, gen)` clears dirty only if no `mark_dirty` happened
  after that `begin_save`; a stale generation is ignored.
- `save_failed(now)` keeps dirty and retries with backoff 1 s, 2 s, 4 s …
  capped at 30 s; success resets the backoff.
- `flush_requested()` makes a dirty scheduler due immediately (app quit).

## Development

    nice -n 10 cargo generate-lockfile
    nice -n 10 cargo fmt --check
    nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    nice -n 10 cargo test --locked

`unwrap`/`expect` are denied in non-test code (`[lints.clippy]`); tests may
use them (`clippy.toml`). Also run `nice -n 10` because another agent runs
timing-sensitive GUI tests on the same machine.
