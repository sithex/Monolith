# island-tauri

The Monolith app: a Tauri 2 window (Rust in `src-tauri/`, React in `ui/`).

Setup, building, running and the development commands are in the repository's
[`instructions.md`](../instructions.md). Short version, from this folder:

```sh
npm ci
./build.sh               # UI, then the release binary (Tauri embeds dist/ at compile time)
./run.sh --overlay       # start the island
npm run typecheck && npm run test:ui
cd src-tauri && cargo test --lib
```

## Deploying changes

A release island serves its UI from `island-tauri/dist` on disk when that folder verifies, and
falls back to the copy embedded in the binary otherwise. That makes two deploy paths:

| Change | Command | Restart? |
|---|---|---|
| UI only (`ui/**`, CSS, TSX, `*.html`) | `scripts/deploy-ui.sh` | No: the webview reloads in place (a few seconds, no Rust build) |
| Anything in Rust, or a changed IPC contract (`ui/ipc/bindings.ts`) | `./build.sh`, then restart the island | Yes |

`scripts/deploy-ui.sh` builds the UI into a staging folder (a failed build leaves the working
`dist/` alone), swaps it in, then sends `reload-ui` on the island's panel socket. The island
re-verifies `dist/` and reloads only its webview; it is never restarted or signalled. It prints
`needs ./build.sh` and exits 3 when the UI was written against a different contract than the
installed binary (the binary would refuse it anyway). `--no-reload` only builds and swaps.

**What "verifies" means** (`src-tauri/src/ui_assets.rs`): `dist/.monolith-manifest.json`, written by
every `npm run build:ui`, lists each file with its SHA-256 and the hash of `ui/ipc/bindings.ts`.
The binary serves from disk only if every hash matches, `dist/`, its subfolders and files are real
(non-symlink) entries owned by the current user and not group/world-writable, the contract hash
equals the one compiled into the binary, and no HTML page needs compile-time CSP rewriting. The
verified bytes are held in memory, so editing `dist/` afterwards changes nothing until the next
verified reload. Otherwise the embedded UI is used and the log (`View log` in the tray) says why:
`UI: embedded assets (disk UI rejected: …)`. Force the embedded UI with `--embedded-ui` or
`HERMES_ISLAND_EMBEDDED_UI=1`. Debug builds always use the embedded UI.

`./build.sh` is unchanged: it builds the UI (manifest included), then the binary, so a fresh build
serves its own `dist/` straight away.

### Automatic rollback

`run.sh --overlay` supervises the island. A binary that stays healthy for ~30 s (it has written a
`status.json` carrying its own pid) is copied to `target/release/hermes-island.last-good`. If a later
build exits with an error within ~10 s, or runs 20 s without ever publishing its status, `run.sh` runs
`hermes-island.last-good` once instead and logs the reason in `.island-data-overlay/monolith.log`.
It never loops: one attempt per binary. Not supervised: launches with `--test-instance` or
`--data-dir`, launches while another island is alive, and `HERMES_ISLAND_NO_SUPERVISOR=1`.
Tests: `tests/run_rollback_test.sh` (fake binaries).

### Before pushing

```sh
scripts/precheck.sh            # Rust tests + clippy, bindings, tsc, test:ui, QML, launcher tests, secrets
scripts/precheck.sh --fast     # skips the Rust steps
scripts/install-pre-push-hook.sh   # optional; installs a pre-push hook that runs it (--remove to undo)
```

It runs at low CPU/IO priority, prints a PASS/FAIL table and exits non-zero on any failure; the log
of each failing step is kept (path printed). The nested-compositor GUI suites are not included.

`tests/disk_ui_e2e.py` proves the disk path end to end in an isolated instance (see its docstring).

## Terminal launch command

**Settings → General → Terminal** sets the command the embedded terminal runs. Left blank, it runs
`hermes chat --tui` from your `PATH`. It is an executable plus arguments, **not a shell**: quotes group
arguments, but aliases, pipes, redirects, `$HOME`, `~` and command substitution are not expanded, so use
absolute paths. Saving never replaces a running terminal; the new command applies the next time one
starts (tray → Restart terminal). Don't put API keys in it.

## Layout

| Path | What |
|---|---|
| `src-tauri/src/contract/` | Typed IPC contract; `cargo test export_bindings -- --ignored` regenerates `ui/ipc/bindings.ts` |
| `src-tauri/src/state/` | The island state machine and the actor that owns it |
| `src-tauri/src/integrations/` | Media, timer, calendar, clipboard, mixer, Plasma notch and the rest; pure parsers and models sit in each one's `core` module |
| `src-tauri/tests/` | Integration tests; the `kwin_*` ones opt into a private virtual KWin compositor |
| `ui/island/` | The island UI |
| `ui/legacy/`, `legacy.html`, `spike.html` | Test-harness pages used by the GUI tests |
| `tests/*.test.mjs` | Front-end tests (`npm run test:ui`) |
| `native/vblank-fix/` | `LD_PRELOAD` shim so WebKitGTK animates at the monitor's refresh rate |

Older specs and verification logs are in [`docs/history/`](../docs/history/).

## Testing

From this folder, with the existing cached dependencies:

```sh
nice -n 15 ionice -c3 npm run typecheck
nice -n 15 ionice -c3 npm run test:ui
nice -n 15 ionice -c3 npm run build:ui -- --configLoader runner
cd src-tauri
nice -n 15 ionice -c3 cargo test --locked --offline --no-fail-fast
```

Build the UI first: Tauri embeds `dist/` even for tests. The runner config loader avoids
writing Vite's config bundle through a shared `node_modules` symlink.

Two kinds of test binary live under `src-tauri/`:

- Pure and service tests (`--lib`, `--test units`, `--test services`): real private
  Unix/TCP sockets, D-Bus mock servers and PTYs, no windows. This is the quick gate for
  most changes: `cargo test --locked --lib --test units --test services` (about 20 s once
  built). A sandbox denying socket creation with `EPERM` cannot run it.
- Private-compositor GUI suites (`tests/kwin_*.rs`, `notification_monitor.rs`,
  `xvfb_shape.rs`): they start a nested `kwin_wayland --virtual` + Xwayland (or Xvfb +
  xfwm4), a private D-Bus, HOME and data directories, and drive the real app with
  virtual input and screenshots. The fixtures clear the inherited display, bus and HOME,
  so they never touch the live desktop; no `WAYLAND_DISPLAY`/`DISPLAY` is needed. They are
  ordinary tests (not `#[ignore]`d), so a bare `cargo test` runs them (about 11 minutes)
  and they fail closed when `kwin_wayland`, `Xwayland`, `Xvfb`, `xfwm4`, `dbus-daemon`,
  Python GObject bindings or the capture/input helpers are missing.

Run the GUI suites one binary at a time, at low priority, with a check that no compositor
survives them:

```sh
scripts/run-gui-tests.sh [LOG_DIR] [suite ...]   # no suite = all of them, logs in LOG_DIR
GUI_TEST_ARGS="launcher_apps_layout" scripts/run-gui-tests.sh /tmp/logs kwin_phase7b_launcher
```

Or by hand from `src-tauri/`: `nice -n 15 cargo test --locked --test kwin_phase4_tray -- --test-threads=1`.
Each suite serialises its own tests; do not run two GUI binaries at once (they share the
machine-wide X socket directory and are timing-sensitive). Leave the GUI binaries out with
`cargo test --locked --lib --test units --test services`.

Still `#[ignore]`d on purpose: `clipboard_source_helper` (a subprocess fixture run by its
parent test; it fails when run directly), `real_lrclib_smoke` and the Open-Meteo smoke test
(they contact real servers; run by hand with `-- --ignored`) and `export_bindings`.

GUI test inventory (names are Rust test filters):

| Target | GUI tests |
|---|---|
| `kwin_gui` | `kwin_gui_preview`, `kwin_gui_overlay` |
| `kwin_phase3_actor` | `actor_owns_hover_tabs_focus_cues_and_geometry` |
| `kwin_phase3_host` | `native_anchor_client_pin_coexistence_and_sigterm_cleanup` |
| `kwin_phase4_notifications` | `cli_notify_toast_ttl_reading_and_dnd_on_private_kwin` |
| `kwin_phase4_tray` | `private_watcher_registers_real_tray_and_menu_dispatches_actor_intents` |
| `kwin_phase5_clipboard` | 11 `clipboard_*` tests (real data-control watcher: text/PNG/URI list, pause/resume, sensitive hints, deny patterns, caps, echo, pinned persistence, X11-only) |
| `kwin_phase6a_services` | `services_report_capabilities_snapshots_events_alerts_and_recover_in_the_real_app`, `a_test_instance_without_an_explicit_system_bus_never_connects_to_any_bus` |
| `kwin_phase6b1_apps` | `apps_commands_events_and_secret_never_cross_ipc_logs_or_status` |
| `kwin_phase6b2_controls` | `remaining_controls_ipc_events_commands_bootstrap_and_module_cleanup_are_private`, `agents_ipc_fixture_poll_event_and_module_cleanup_are_private`, `brightness_ipc_event_and_module_disable_reenable_use_only_the_private_daemon` |
| `kwin_phase6m_media` | `media_lyrics_and_equalizer_work_end_to_end_in_the_real_app`, `a_test_instance_without_a_mock_bus_has_an_empty_media_state_and_stays_honest` |
| `kwin_phase7a_island` | `island_page_real_input_geometry_retained_draft_appearance_and_screenshots` |
| `kwin_phase7b_launcher` | `launcher_apps_layout_and_draft_use_real_input_in_private_instance` and 8 `acceptance::*` (drag/cancel/hide badge, locked tiles, timer activity, frozen terminal, native pill + side union, media capture lifetime) |
| `kwin_phase9a_release` | `anchor_client_complete_status_embedded_island_and_sigterm_use_real_eis` |
| `kwin_shape` | `kwin_overlay_native_pill`, `kwin_preview_non_native_pill` |
| `kwin_spike_panel` | `kwin_spike_panel` (plus its pure `spike_analysis` unit tests) |
| `notification_monitor` | 6 private-bus monitor tests |
| `xvfb_shape` | `overlay_cycle_with_window_manager`, `preview_cycle_non_native_pill`, `click_through_is_physical_without_window_manager` |

Island-page (`--ui island`) tests find their window by the product's own overlap size
(`phase3_host::overlay_size()` = `over_panel(...)` for the fixture panel), not the 1180x820
size of the Legacy page; use `find_window_sized` for new ones.
