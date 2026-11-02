# updates-core

Pure core for the Hermes Island **package-update badge** (SPEC §0.5
"More tiles/features Nick picked", item 12). The Tauri actor runs
`checkupdates` (pacman-contrib) and `paru -Qua`; this crate parses their
exit code / stdout / stderr into an update list and computes the badge
state (counts, reboot hint, staleness, error) and the check schedule.
**It never runs anything** — no IO, no network, no threads, no clock, no
`unsafe`, never panics on any input. Dependencies: `serde` (derive) +
`serde_json` only. Wire types use camelCase via serde, no aliases.

## Verified command facts (controller, nick-pc)

- `checkupdates` (pacman-contrib 1.13.1): exit 0 = updates (stdout
  lines), 1 = unknown failure, 2 = no updates. Each stdout line is
  `<pkgname> <oldver> -> <newver>`, optionally suffixed ` [ignored]`.
- `paru -Qua`: exit 0 with lines = updates; exit 1 with **empty** stdout
  + stderr = none; exit 1 with stderr text or any other code = failure.
  Lines use the same `<pkgname> <oldver> -> <newver>` form.

## Quick tour

```rust
use updates_core::{
    parse_checkupdates, parse_paru_qua, Badge, Schedule, Update, CHECKUPDATES_ARGV,
    PARU_QUA_ARGV,
};

// read-only argv (never -S/-U, never sudo, never --noconfirm)
let argv = CHECKUPDATES_ARGV; // ["checkupdates"]
let argv = PARU_QUA_ARGV;     // ["paru", "-Qua"]

// parse command output
let updates: Vec<Update> = parse_checkupdates(0, &stdout, &stderr)?;
let aur: Vec<Update> = parse_paru_qua(1, "", "")?; // exit 1 + empty => none

// badge state (prev_ok = last successful full list, None if never had one)
let badge = Badge::compute(
    Ok(updates), Some(Ok(aur)), now_secs, last_ok_secs, prev_ok.as_deref(),
);
// badge.count / repoCount / aurCount are Option — None = unknown (never
// fabricated as 0). badge.items sorted needsReboot first, then name.

// schedule (injected time only)
let mut sched = Schedule::new(on_battery);
sched.note_failure(); // after a failed check; note_success() resets
let due = sched.due(now_secs, last_attempt_secs, last_ok_secs);
```

## Parsing

- `parse_checkupdates(exit, stdout, stderr)`: exit 0 -> parse lines
  (source `Repo`); exit 2 -> none (`Ok(vec![])`); anything else ->
  `CommandFailed`. `stderr` is ignored.
- `parse_paru_qua(exit, stdout, stderr)`: exit 0 -> parse lines (source
  `Aur`); exit 1 with empty stdout+stderr -> none; exit 1 with non-empty
  stderr -> `CommandFailed`; exit 1 with only stdout non-empty -> parse
  lines; any other code -> `CommandFailed`.
- Caps: stdout > 1 MiB -> `StdoutTooLarge`; > 10 000 non-blank lines ->
  `TooManyLines` (boundaries parse).
- A malformed non-blank line is skipped and counted. If **every**
  non-blank line is malformed -> `AllMalformed` (never a fabricated "0
  updates"). Blank lines are ignored.
- Package names: `[a-z0-9@._+-]`, 1..=128 bytes, no leading `-`/`.`.
  Versions: non-empty, ≤ 128 bytes, no whitespace, chars from
  `[0-9A-Za-z.:+~_-]`.
- ` [ignored]` trailing marker -> `ignored: true` (excluded from counts,
  kept in `items`).

## Badge (`Badge::compute`)

- `count` / `repoCount` / `aurCount`: non-ignored counts; `None` when any
  source is unknown (failed and no previous OK result), never fabricated
  as 0. `aur = None` (not run) counts as known-empty.
- `prev_ok`: last successful full update list. When a source fails, its
  counts/items fall back to `prev_ok`'s entries for that source; the
  `error` field still records the current failure.
- `stale`: no successful repo check within the last 6 h (`last_ok_secs
  = None` counts as stale).
- `needsReboot`: any shown item is a kernel/graphics package (`linux`,
  `linux-*`, `nvidia*`, `mesa` — see Decisions in VERIFICATION.md).
- `items`: all known updates (ignored included), sorted needsReboot
  first, then by name.

## Schedule (`Schedule`)

- `due(now, last_attempt, last_ok)`: after a success, at most one check
  per hour; after a failure, exponential backoff 10 → 20 → 40 → 60 min
  (cap). Never on battery unless stale (> 6 h since last OK). Time is
  always injected; `note_success()` / `note_failure()` maintain the
  consecutive-failure counter.

## Development

    nice -n 10 cargo generate-lockfile
    nice -n 10 cargo fmt --check
    nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    nice -n 10 cargo test --locked

`unwrap`/`expect` are denied in non-test code (`[lints.clippy]`); tests
may use them (`clippy.toml`). Run under `nice -n 10` because another
agent runs timing-sensitive GUI tests on the same machine.
