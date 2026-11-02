# VERIFICATION — downloads-core crate

Location: `island-tauri/staging/downloads-core/`.
Scope: pure core behind SPEC §0.5 "Design direction" (Apps bullet) and §8
"Phase 6b" — the Downloads app ("recent files in the XDG download dir via
directory notifications; open/reveal"). The crate turns scan results + change
events (produced by the Tauri host's inotify watcher and `stat` calls) into
the ordered, settled list the UI shows; the host does the file IO, watching
and opening. Platform-free: no IO, no network, no threads, no clock, no
`unsafe`, never panics on any input. `now` is injected monotonic `u64` ms;
`FileStat::mtime_ms` is wall-clock from `stat`. Wire types use camelCase via
serde, no `#[serde(alias)]`. Dependencies: serde (derive) + serde_json only.

## Gate commands — final state (all green)

```
$ nice -n 10 cargo generate-lockfile
    Updating crates.io index
     Locking 11 packages to highest compatible versions
exit=0

$ nice -n 10 cargo fmt --check
(clean)
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking downloads-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/downloads-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.16s
exit=0

$ nice -n 10 cargo test --locked
   Compiling downloads-core v0.1.0 (.../downloads-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.23s
     Running unittests src/lib.rs (target/debug/deps/downloads_core-6bb332fa4df59090)

running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz_tests.rs
running 2 tests
test fuzz_default_config_filters_hidden ... ok
test fuzz_downloads_model_never_panics_and_keeps_invariants ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/model_tests.rs
running 36 tests
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests downloads_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total at the original handoff: 43 tests (5 unit + 2 fuzz + 36 integration),
all green under `nice -n 10`. Subsequent test additions are recorded in the
[Follow-up after controller audit](#follow-up-after-controller-audit) section
(current total: 46 tests).

## Mutation checks (all actually run on the current code)

Method: snapshot the pristine tree to scratch (`--exclude=target`), apply one
breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record failing
test names, restore pristine (touching restored sources so cargo rebuilds),
then `diff -r --exclude=target <pristine> .` verified empty for every
mutation. All six restored identical, and the final full gate re-run on the
restored tree is green (above). Every mutation compiled (a non-compiling
mutation would not count). Each mutation fails *named* tests — at least one
named test outside the fuzz suite — listed below.

| # | Breakage applied (src/lib.rs) | Failing tests observed |
|---|---|---|
| 1 | Settle restart on identical stat: same-stat early return removed in `upsert` (a same-stat Upsert now restarts the window) | `identical_stat_upsert_keeps_window` |
| 2 | Exclusive settle expiry: `deadline <= now` → `deadline < now` in both `tick` and `items` (deadline equal to `now` stays Settling) | `settling_to_ready_at_exact_deadline`, `size_change_mid_settle_restarts_window`, `mtime_change_mid_settle_restarts_window`, `next_settle_at_is_earliest_deadline`, `rename_in_progress_to_final_yields_single_settling_item`, `rescan_preserves_settling_deadline_when_unchanged`, `rescan_restarts_settling_for_changed`, `settle_ms_zero_marks_ready_immediately` |
| 3 | Partial suffix case-sensitive: lowercasing dropped in `is_partial` (`.PART`/`.Crdownload` no longer detected) | `tests::is_partial_case_insensitive_suffixes`, `partial_suffixes_case_insensitive` |
| 4 | Rename producing two items: `self.entries.remove(&from)` dropped in the `Renamed` arm (source survives alongside destination) | `rename_in_progress_to_final_yields_single_settling_item`, `rename_final_to_partial_marks_in_progress` |
| 5 | Eviction keeping oldest: mtime ranking flipped in `evict` (keeps lowest mtimes instead of newest) | `max_items_evicts_oldest`, `evicted_file_returns_with_newer_mtime` |
| 6 | `display_size` 1024 boundary: `bytes < 1024` → `bytes <= 1024` (1024 renders as "1024 B") | `tests::display_size_boundaries`, `display_size_boundaries` |

## Decisions

- **`DownloadsModel::new(cfg)` returns `Result<Self, ConfigError>`.** The
  spec lists `DownloadsModel::new(cfg: DownloadsConfig)`; validating the
  config with a typed error is required, so `new` validates and returns
  `Result`. `DownloadsConfig::default()` (50 / 1500 / false) is always valid.
  There is no `DownloadsModel::default()` (Clippy's `derivable_impls` would
  need `Entry: Default`, which is internal and intentionally not derived).
- **Renaming to a final name always restarts the settle window**, even when
  the destination already existed with the same size/mtime. A rename is a
  fresh event; only a same-stat *Upsert* is treated as a no-op. Renaming to a
  partial name replaces the entry with an `InProgress` one.
- **`next_settle_at()` reads stored state** (it takes no `now`). Callers that
  advance time should call `tick(now)` first; otherwise a due-but-unticked
  deadline may still be reported. `items(now)` evaluates state at `now`
  directly, so it never shows a stale `Ready`.
- **Filtering order:** directories/symlinks are ignored first (never counted),
  then invalid names (empty, `.`, `..`, `/`, NUL) are rejected and counted,
  then hidden names are filtered when `show_hidden` is false (never counted).
  `rejected_count()` therefore counts only invalid names.
- **`display_size` floors to 1 decimal** (integer arithmetic: `whole` +
  `tenths` from the remainder) so it never prints "1024.0 KB" — the largest
  pre-boundary value is "1023.9 KB", and the value rolls up to the next unit
  at exactly 1024/1048576/… `u64::MAX` → "16777215.9 TB", no panic.
- **Classification precedence:** multi-part suffixes are matched before single
  extensions. `.pkg.tar.zst` → Package (single `zst` would be Archive), and
  `.tar.gz` / `.tar.xz` / `.tar.bz2` → Archive. All matching is
  case-insensitive (`AppImage`, `appimage`, `APPIMAGE` → Package). A name
  with no dot, or whose only dot is the leading hidden dot (e.g. `.gitignore`),
  is `Other`.
- **Eviction is by (mtime desc, name asc)** — the same rank as the list
  ordering — so older mtimes are dropped first and mtime ties break by name
  ascending (byte order). Eviction runs after every `apply`; an evicted file
  re-Upserted with a newer mtime comes back as a fresh Settling entry.
- **Rescan preserves state only for identical (size, mtime) entries:** Ready
  stays Ready, a Settling deadline is kept, InProgress stays InProgress. Any
  changed stat restarts settling; names absent from the rescan disappear.
- **Serde enums use `rename_all = "camelCase"`** as well: variants serialize
  as `inProgress` / `settling` / `ready` and `image` / `video` / etc.

## UNVERIFIED

- **Tauri actor wiring** — inotify watching, `stat`/open/reveal calls, and
  the wakeup loop around `next_settle_at()` are outside this crate's scope.
- **Live monotonic source behavior** — e.g. system suspend / clock jumps on a
  real host; synthetic `u64` inputs only (a non-monotonic `now` is accepted
  but the settling guarantees are only meaningful for monotonic time).
- **Real-world watcher event sequencing** — e.g. how Chromium/Firefox order
  `Upsert`/`Renamed`/`Removed` during a save; the model handles any order but
  only the listed events exist.
- **Eviction timing** — eviction is eager (after each `apply`), not lazy;
  `items` never evicts on its own.
- **Locale-sensitive byte order** — name ties use UTF-8 byte order, not a
  locale collation.
- **Concurrency** — the crate is single-threaded with no locks; the actor
  serializes access.

## Follow-up after controller audit

The controller's deliberate-breakage audit exposed two test gaps. Both were
closed with tests only — no behaviour change (the re-runs below confirm the
new named tests fail under each mutation, and the tree is reverted
byte-identical after each).

### Gap 1 — `show_hidden` filter guard

Mutation: `if st.name.starts_with('.') && !show_hidden {` →
`if st.name.starts_with('.') {` (hidden names always filtered, ignoring
`show_hidden`). The controller reported only the fuzz test failing; on this
tree the named tests `hidden_names_filtered_unless_show_hidden` and
`kinds_flow_through_items` (both exercise `show_hidden: true`) also fail.

Added named integration test `hidden_pdf_respects_show_hidden`:
- with `show_hidden: true`, an Upsert of ".hidden.pdf" appears in `items()`;
- with `show_hidden: false` it does not;
- neither path contributes to `rejected_count()`.

### Gap 2 — leading-dot-only classification

Mutation: `Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() =>` →
`Some((_stem, ext)) if !ext.is_empty() =>` (empty stem accepted, so a
leading-dot-only name like ".zip" would classify as Archive).

Before: failed **no** test (".zip"/".pdf"/".gitignore" are absent from the
old named classification table, and ".gitignore"/".hidden" map to the `_`
fallback so they masked the gap).

Added named integration tests (through `items()` kind):
- `leading_dot_only_names_classify_as_other`: ".zip", ".pdf", "README" → `Other`;
- `leading_dot_stem_classifies_by_extension`: ".config.zip" → `Archive`
  (non-empty stem ".config" means it classifies normally).

### Final gate — post-tests (all under `nice -n 10`)

```
$ nice -n 10 cargo fmt --check
(clean)
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking downloads-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/downloads-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.15s
exit=0

$ nice -n 10 cargo test --locked
   Compiling downloads-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/downloads-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.24s

     Running unittests src/lib.rs (target/debug/deps/downloads_core-6bb332fa4df59090)
running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-ad93365899da00ad)
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/model_tests.rs (target/debug/deps/model_tests-26d817baeafe6d53)
running 39 tests
test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests downloads_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total now: **46 tests** (5 unit + 2 fuzz + 39 integration), all green.

### Mutation re-runs (each now fails a new named non-fuzz test)

Method (unchanged): snapshot the post-test-add tree to scratch
(`--exclude=target`), apply one breakage, run
`nice -n 10 cargo test --locked --no-fail-fast`, record failing test names,
restore pristine, then `diff -r --exclude=target <snapshot> .` verified empty
for both restores; the final full gate on the restored tree is green (above).

| # | Breakage applied (src/lib.rs) | New named failing test | Other failing tests |
|---|---|---|---|
| 1 | Hidden-filter guard dropped: `if st.name.starts_with('.') && !show_hidden {` → `if st.name.starts_with('.') {` | `hidden_pdf_respects_show_hidden` | `fuzz_downloads_model_never_panics_and_keeps_invariants`, `hidden_names_filtered_unless_show_hidden`, `kinds_flow_through_items`, `leading_dot_only_names_classify_as_other`, `leading_dot_stem_classifies_by_extension` |
| 2 | Empty-stem guard dropped in `classify`: `Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() =>` → `Some((_stem, ext)) if !ext.is_empty() =>` | `leading_dot_only_names_classify_as_other` | (none) |

Both mutations compiled (a non-compiling mutation would not count). Mutation 2,
which previously failed zero tests, now fails exactly the new named
`leading_dot_only_names_classify_as_other`; mutation 1 now fails the new
`hidden_pdf_respects_show_hidden` in addition to the existing named tests that
already exercised `show_hidden: true`.
