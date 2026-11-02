# downloads-core

Pure core for the Hermes Island Phase 6b Downloads app
(SPEC §0.5 "Design direction" — Apps bullet; §8 "Phase 6b").

Scope: "recent files in the XDG download dir via directory notifications;
open/reveal". This crate decides **what the UI shows**: it consumes scan
results and change events (the "what happened" facts produced by the Tauri
host's inotify watcher and `stat` calls) and produces the ordered list of
`DownloadItem`s. The host does all filesystem work (inotify watching, stat,
opening/revealing); this crate does none.

## Properties

- Pure library: no IO, no network, no threads, no clock, no `unsafe`,
  never panics on any input.
- All times are injected `u64` milliseconds: `now` is monotonic (settling);
  `FileStat::mtime_ms` is wall-clock from `stat`.
- Wire types are camelCase via serde (no `#[serde(alias)]`).
- Dependencies: serde (derive) + serde_json only.

## API

```rust
// Host feeds events; model emits the list.
let mut m = DownloadsModel::new(DownloadsConfig::default())?;
m.apply(now, DirEvent::Upsert(st));          // or Removed / Renamed / Rescan
m.tick(now);                                 // advance settling
let list: Vec<DownloadItem> = m.items(now);  // sorted mtime desc, name asc
let next: Option<u64> = m.next_settle_at();
```

Behavior summary:

- Directories and symlinks are ignored. Hidden names (leading `.`) are
  ignored unless `show_hidden`. Names that are empty, `.`, `..`, or contain
  `/` or NUL are rejected and counted by `rejected_count()`.
- In-progress suffixes `.part`, `.crdownload`, `.download`, `.partial`,
  `.opdownload` (case-insensitive) => `ItemState::InProgress`. Renaming an
  in-progress file to its final name yields **one** entry that enters settling.
- A final file is `Settling` until its size and mtime have been unchanged for
  `settle_ms` (same-stat Upserts do not restart the window; different stat
  does), then `Ready`. Expiry is inclusive.
- Only the newest `max_items` by (mtime desc, name asc) are kept; older ones
  are evicted (a re-Upsert with a newer mtime brings one back).

## FileKind table

Matched case-insensitively; multi-part extensions (`pkg.tar.zst`,
`tar.gz`, `tar.xz`, `tar.bz2`) are classified before single extensions.

| Kind | Extensions |
|---|---|
| Archive | zip tar gz tgz xz zst 7z rar bz2 |
| Image | png jpg jpeg gif webp svg avif heic |
| Video | mp4 mkv webm mov avi |
| Audio | mp3 flac ogg opus wav m4a |
| Document | pdf doc docx odt txt md rtf epub |
| Package | deb rpm pkg.tar.zst AppImage flatpakref |
| Code | rs py js ts json toml yaml sh |
| Other | everything else (no dot, leading-dot-only name, unknown extension) |

`display_size(bytes)` formats sizes in 1024 steps with 1 decimal:
"1.0 KB" at 1024, "1023 B" at 1023, never "1024.0 KB" (rolls up at the
boundary), no panic at `u64::MAX`.

## Gate

```
cargo generate-lockfile
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

See VERIFICATION.md for the gate log and the mutation table. `unwrap_used` /
`expect_used` are denied outside test code (`allow-unwrap-in-tests` in
clippy.toml).
