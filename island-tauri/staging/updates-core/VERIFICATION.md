# VERIFICATION — updates-core crate

Location: `island-tauri/staging/updates-core/`.
Scope: pure core behind SPEC §0.5 "More tiles/features Nick picked", item
(12) — the read-only **package update badge** for CachyOS/Arch. The Tauri
actor runs `checkupdates` (pacman-contrib) and `paru -Qua` and passes
exit code / stdout / stderr here; this crate parses them into an update
list, classifies reboot-needing packages, computes the badge state and
the check schedule. It never runs anything. Platform-free: no IO, no
network, no threads, no clock, no `unsafe`, never panics on any input.
Dependencies: serde (derive) + serde_json only. Wire types use camelCase
via serde, no `#[serde(alias)]`. argv constants only: `["checkupdates"]`
and `["paru","-Qua"]` — never `-S`/`-U`, never `sudo`, never
`--noconfirm` (pinned by `argv_never_sudo_or_upgrade`).

## Verified upstream facts (controller, nick-pc)

- `checkupdates` (pacman-contrib 1.13.1) man page: exit 0 = updates
  (stdout lines), 1 = unknown failure, 2 = no updates. Each stdout line
  is `<pkgname> <oldver> -> <newver>`, optionally suffixed ` [ignored]`
  (pacman IgnorePkg).
- `paru -Qua` lists AUR updates in the same form. Observed: with no
  updates it exits 1 with EMPTY stdout and stderr. So: exit 0 with lines
  = updates; exit 1 with empty stdout+stderr = none; exit 1 with stderr
  text or any other code = failure.

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
    Checking updates-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/updates-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s
exit=0

$ nice -n 10 cargo test --locked
     Running unittests src/lib.rs (.../updates_core-dbabb758ef39628a)
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/fuzz_tests.rs (.../fuzz_tests-8327ab58dff12c6f)
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/updates_tests.rs (.../updates_tests-10e94e8984546fa2)
running 55 tests
test result: ok. 55 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total: 59 tests (3 unit + 1 fuzz + 55 integration), all green.

## Mutation checks (all actually run on the current code)

Method: snapshot the pristine tree to scratch (`--exclude=target`), apply
one breakage, run `nice -n 10 cargo test --locked --no-fail-fast`,
record failing test names, restore pristine (`rsync -a --delete` +
`touch` of restored sources so cargo cannot reuse a stale binary), then
`diff -r --exclude=target <pristine> .` verified empty for every
mutation (printed `RESTORED_IDENTICAL` each time), and the final full
gate on the restored tree is green (above). Every mutation compiles (a
non-compiling mutation would not count). Each fails *named* tests.

| # | Breakage applied | Failing named tests observed |
|---|---|---|
| 1 | `checkupdates` exit 2 treated as failure (`2 => Err(CommandFailed)`) | `checkupdates_exit2_no_updates` |
| 2 | `paru` exit 1 stderr treated as none (`if stdout.is_empty()` dropped the `&& stderr.is_empty()` condition) | `paru_exit1_stderr_is_failure` |
| 3 | Ignored packages counted (`non_ignored` returns `len()` instead of filtering) | `badge_count_excludes_ignored`, `badge_repo_aur_sum_invariant`, `badge_serde_camel_case`, plus fuzz invariant (count == non-ignored items) |
| 4 | Repo failure reported as count 0 (Err arm falls back to `Some(vec![])` instead of `None`) | `badge_repo_failure_unknown_not_zero`, `badge_repo_failure_with_prev_fallback` |
| 5 | Backoff not doubling (constant 10 min) | `backoff_sequence`, `due_backoff_grows`, `due_backoff_capped_at_60`, `due_battery_overdue_still_backoff_capped` |
| 6 | Kernel detection misses `linux-*` (`linux-` prefix clause dropped) | `is_kernel_linux_cachyos`, `is_kernel_linux_headers`, `badge_needs_reboot_true_for_kernel` |
| 7 | All-malformed input returns empty instead of `AllMalformed` (check removed) | `all_malformed_is_error`, `ignored_marker_must_be_exact`, `name_validation_edges`, `version_validation_edges`, `stdout_cap_strict_boundary` |
| 8 | Line cap lowered 10 000 → 5 000 (cap lowered, not removed) | `lines_cap_exact_boundary` |

Note on cap tests: `lines_cap_exact_boundary` pins a **literal** 10 000
(independent of `MAX_LINES`); an earlier version derived its expectation
from `MAX_LINES`, which made mutation 8 invisible — fixed so a lowered
cap is caught.

### Fuzz generator note

The hand-rolled LCG (MMIX constants) strictly alternates the parity of
successive outputs, so a `coin()` sampled at a fixed call offset is
constant. The fuzz's `ignored` flag originally used `coin()` and was
therefore always `false`, which made the count-vs-non-ignored invariant
vacuous. It now uses `below(3) == 0` (odd modulus, breaks the parity
lock-step). After the fix, mutation 3 is also caught by the fuzz, so the
invariant is live.

## Decisions

- **Reboot-needing names (`is_kernel`)** — exactly: `linux` (exact),
  `linux-*` prefix (linux-cachyos, linux-cachyos-headers, linux-lts,
  linux-zen, …), `nvidia*` prefix, `mesa` (exact). This literally matches
  the spec's `linux-*` rule, so `linux-firmware` / `linux-firmware-*`
  are also flagged (a known over-approximation: firmware does not need a
  reboot, but the written rule includes it; preferring a false-positive
  reboot hint over a missed one). Revisit only if Nick objects.
- **Counts** — `count` / `repoCount` / `aurCount` are non-ignored counts
  (`count == repoCount + aurCount` when all are `Some`). `items` keeps
  ignored updates too (annotated), so the UI can show why the count
  differs. If any source is unknown (failed and no previous OK result),
  all three counts are `None` — never fabricated as 0.
- **`Badge::compute` extra parameter** — the spec says "a repo failure
  keeps count: None unless a previous OK result is supplied", but the
  listed signature had nowhere to supply it. Resolved least-surprisingly:
  a `prev_ok: Option<&[Update]>` parameter (last successfully parsed full
  list; `None` = never had one). On failure of a source, its counts/items
  fall back to `prev_ok`'s entries for that source; `error` still records
  the current failure. `prev_ok = Some([])` is a *known zero*, not
  unknown.
- **`last_ok_secs: Option<u64>`** — the spec lists it bare, but "no
  successful repo check" (never succeeded) needs `None`; `None` counts as
  stale for both `Badge::stale` and `Schedule::due`.
- **`paru` exit 1 with only stdout non-empty** — treated as updates (the
  lines are in the same form); exit 1 with stderr text or any other exit
  code is `CommandFailed`. `checkupdates` stderr is never consulted (exit
  code decides).
- **`checkupdates` exit 2** ignores stdout entirely (none by definition).
- **Caps** — stdout cap is strictly greater-than 1 MiB (`>`), so exactly
  1 MiB parses; the line cap counts **non-blank** lines (a trailing
  newline adds no phantom line) and is also strictly greater-than 10 000.
- **Blank lines** are skipped silently (they are not malformed warnings).
  A malformed non-blank line is skipped and counted; if *every* non-blank
  line is malformed → `AllMalformed` (never "0 updates").
- **Package names** — `[a-z0-9@._+-]`, 1..=128 bytes, no leading `-` or
  `.`. **Versions** — non-empty, ≤ 128 bytes, no whitespace, chars from
  `[0-9A-Za-z.:+~_-]`. The stricter `[epoch:]pkgver-pkgrel` shape is not
  enforced beyond these facets (avoids rejecting odd-but-real upstream
  versions).
- **`[ignored]` marker** must be the exact trailing token; anything else
  (extra tokens, wrong marker) makes the line malformed.
- **Schedule** — after a success the wait is 1 h; after a failure the
  wait is the backoff delay 10 → 20 → 40 → 60 min (cap; `0` failures
  defensively behaves like 1). Since the backoff is ≤ 1 h it only
  *shortens* the post-failure wait relative to the hourly cadence
  ("at most one check per hour" describes the nominal rate). Battery
  blocks checks unless stale (> 6 h since last OK); the battery override
  lifts only the battery block — the cadence/backoff caps still apply.
- **Sorting** — items sort needsReboot first, then by name (byte-wise
  `String` order; stable so ties keep source order).
- **`error` field** — joins each failed source as `"repo: <msg>"` /
  `"aur: <msg>"` with `"; "`; `None` when every source succeeded.

## UNVERIFIED

- **Tauri actor wiring** — spawning `checkupdates` / `paru`, reading
  exit/stdout/stderr, and driving `Schedule` (`note_success` /
  `note_failure`, battery state, persisted timestamps) are outside this
  crate's scope (it never spawns or runs anything) and are untested here.
- **Live command versions** — facts were verified against pacman-contrib
  1.13.1 and the observed `paru` behavior on nick-pc; other versions are
  untested.
- **AUR `[ignored]` markers** — `paru -Qua` output is assumed to use the
  same line shape as `checkupdates` (including the optional marker); not
  observed end-to-end.
- **Battery/last-OK timestamps** — where the actor gets `on_battery`
  (UPower) and the persisted `last_ok_secs` is not part of this crate.
- **Real-world pathological stdout** — the 1 MiB / 10 000-line caps and
  the version-character allowlist are reasonable guards, but extreme real
  `checkupdates` output was not observed.
- **Version grammar depth** — `[epoch:]pkgver-pkgrel` is not validated
  beyond length/charset (see Decisions); a weird-but-printable upstream
  version passes through as-is.
