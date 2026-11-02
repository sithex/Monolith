# launcher-core

Pure core for the Hermes Island **Phase 7 app-grid launcher** (SPEC §0.5
"Design direction" Apps bullet). It parses `.desktop` file text into launcher
entries and turns an entry's `Exec` line into an argv, implementing the
freedesktop.org Desktop Entry Specification (latest): §3–§6 summarised
(groups, keys, values, lists, localized keys, booleans) and §7 "The Exec key"
copied exactly (quoting, escaping, field codes).

The Tauri side reads `.desktop` files from the XDG application dirs and spawns
the processes; this crate only decides **what** an entry is and **how** to
invoke it.

Platform-free: no IO, no network, no threads, no clock, no process spawning, no
`unsafe`, never panics on any input. Dependencies: `serde` (derive) +
`serde_json` only.

## Quick tour

```rust
use launcher_core::{parse_desktop_entry, build_argv, ExecContext, Catalog};

// 1. Parse .desktop file text into an entry.
let text = "[Desktop Entry]\nType=Application\nName=Foo Bar\nIcon=foo\nExec=foo %U\n";
let (entry, warnings) = parse_desktop_entry(text, "org.example.foo.desktop", Some("en_US"))?;
// warnings: Vec<Warning> for non-boolean values etc.

// 2. Turn the Exec line into an argv.
let ctx = ExecContext {
    name: entry.name.clone(),
    icon: (!entry.icon.is_empty()).then(|| entry.icon.clone()),
    desktop_file: Some("/usr/share/applications/org.example.foo.desktop".into()),
    files: vec!["/tmp/a.txt".into()],
};
let argv = build_argv(&entry.exec, &ctx)?; // ["foo", "/tmp/a.txt"]

// 3. Merge dirs and search.
let mut cat = Catalog::new();
cat.add_dir_entries(0, vec![entry]); // lower rank = earlier dir = wins
let hits = cat.search("foo", &["KDE"]);
```

## Behavior notes

- `parse_desktop_entry(text, desktop_id, locale) -> Result<(AppEntry, Vec<Warning>), ParseError>`.
  Errors: no `[Desktop Entry]` group, `Type` missing or not `Application`,
  missing `Name`/`Exec`, input > 64 KiB, NUL in input. Only the first
  `[Desktop Entry]` group counts; duplicate keys: first wins; localized key
  fallback chain is `lang_COUNTRY@MODIFIER` → `lang_COUNTRY` →
  `lang@MODIFIER` → `lang` → unlocalized.
- `build_argv(exec, &ExecContext) -> Result<Vec<String>, ExecError>`: string
  escapes before quoting; `%f %F %u %U` (at most one of them; `%F`/`%U` and
  `%i` must be a whole argument; `%f`/`%u` use only the first file), `%i`
  (→ `--icon` + icon when present), `%c` (name, one argument), `%k`
  (desktop-file path or removed when unknown), `%%` literal, deprecated
  `%d %D %n %N %v %m` removed. Rejects unknown field codes, field codes inside
  quotes, unterminated quotes, empty commands, and an executable containing
  `=`.
- `visible_in(entry, desktops)`: hidden/noDisplay hide; `OnlyShowIn`
  non-empty must intersect; `NotShowIn` intersecting hides. Case-sensitive.
- `Catalog::add_dir_entries(rank, entries)` (lower rank wins; a `Hidden`
  entry in a higher-priority dir masks lower dirs) and
  `Catalog::search(query, desktops)`: case-insensitive substring over name,
  genericName, keywords, desktopId; rank name prefix > name substring > other
  fields; ties by name then desktopId; only visible entries.
- Wire shape is camelCase (serde), no aliases. `AppEntry` fields default
  sensibly on missing JSON (`genericName`, `comment`, `icon`, `tryExec` →
  `""`; booleans → `false`; lists → empty).

## Limits

- Input ≤ 64 KiB (bytes) for parsing; larger is a typed error.
- `Exec` argv: first element must not contain `=`; command must not expand to
  empty.
- No filesystem, env, network, threads, clock, or `unsafe`.

## Development

    nice -n 10 cargo generate-lockfile
    nice -n 10 cargo fmt --check
    nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    nice -n 10 cargo test --locked

`unwrap`/`expect` are denied in non-test code (`[lints.clippy]`); tests may
use them (`clippy.toml`). Run under `nice -n 10` because another agent runs
timing-sensitive GUI tests on the same machine. See VERIFICATION.md for the
gate outputs, mutation checks, and decisions.
