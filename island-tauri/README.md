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
writing Vite's config bundle through a shared `node_modules` symlink. The default Rust
suite does not launch GUI windows. It still exercises real private Unix/TCP sockets,
D-Bus mock servers and PTYs, so it needs an environment that permits those local
operations. A sandbox denying socket creation with `EPERM` cannot run that coverage;
those tests remain enabled.

The window/clipboard/notification acceptance tests below are explicitly ignored because
they launch actual KWin/Xwayland or Xvfb plus WebKit. KWin fixtures deliberately clear
inherited display and bus variables and create a virtual compositor, private D-Bus,
HOME and data directories. They do not connect to the live desktop selection. A live
session variable alone cannot fix a missing tool, blocked socket or graphics backend.
Prerequisites include `kwin_wayland`, `Xwayland`, `Xvfb`, `xfwm4`, `dbus-daemon`, Python
GObject bindings, and the capture/input helper dependencies used by the fixtures.

Run one suite from `src-tauri/`, in a local shell that allows compositor and socket access:

```sh
WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 nice -n 15 ionice -c3 cargo test --locked --offline --test kwin_phase4_tray -- --ignored --test-threads=1
```

Run every GUI suite serially (the clipboard subprocess helper is invoked only by its parent):

```sh
for source in tests/kwin_*.rs tests/notification_monitor.rs tests/xvfb_shape.rs; do
  suite=${source##*/}
  WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 nice -n 15 ionice -c3 cargo test --locked --offline --test "${suite%.rs}" -- --ignored --skip clipboard_source_helper --test-threads=1 || break
done
```

Use the display names from your own session. Do not run an unfiltered `--ignored` across
all targets: it also includes binding generation and subprocess-only helpers. Pure
notification decoder and spike image-analysis tests remain in the default suite.

Ignored GUI test inventory (names are Rust test filters):

| Target | Ignored GUI tests |
|---|---|
| `kwin_gui` | `kwin_gui_preview`<br>`kwin_gui_overlay` |
| `kwin_phase3_actor` | `actor_owns_hover_tabs_focus_cues_and_geometry` |
| `kwin_phase3_host` | `native_anchor_client_pin_coexistence_and_sigterm_cleanup` |
| `kwin_phase4_notifications` | `cli_notify_toast_ttl_reading_and_dnd_on_private_kwin` |
| `kwin_phase4_tray` | `private_watcher_registers_real_tray_and_menu_dispatches_actor_intents` |
| `kwin_phase5_clipboard` | `clipboard_default_off_has_no_watcher_thread_or_connection`<br>`clipboard_real_watcher_reads_text_png_and_uri_list`<br>`clipboard_disable_joins_watcher_and_closes_its_connection`<br>`clipboard_pause_discards_real_offers_and_resume_captures_only_future_changes`<br>`clipboard_sensitive_hint_and_validated_deny_patterns_skip_real_offers`<br>`clipboard_concealed_and_transient_offers_are_skipped_and_counted_as_sensitive`<br>`clipboard_read_caps_drop_oversize_offers_and_a_following_offer_is_still_captured`<br>`clipboard_copy_echo_reorders_only_with_stable_id_and_pin`<br>`clipboard_pinned_survive_clear_and_persistence_is_private_pinned_only`<br>`clipboard_x11_only_environment_reports_unavailable_without_wayland_connection`<br>`clipboard_in_app_pause_and_push_update_the_actual_sibling_ui` |
| `kwin_phase6a_services` | `services_report_capabilities_snapshots_events_alerts_and_recover_in_the_real_app`<br>`a_test_instance_without_an_explicit_system_bus_never_connects_to_any_bus` |
| `kwin_phase6b1_apps` | `apps_commands_events_and_secret_never_cross_ipc_logs_or_status` |
| `kwin_phase6b2_controls` | `remaining_controls_ipc_events_commands_bootstrap_and_module_cleanup_are_private`<br>`agents_ipc_fixture_poll_event_and_module_cleanup_are_private`<br>`brightness_ipc_event_and_module_disable_reenable_use_only_the_private_daemon` |
| `kwin_phase6m_media` | `media_lyrics_and_equalizer_work_end_to_end_in_the_real_app`<br>`a_test_instance_without_a_mock_bus_has_an_empty_media_state_and_stays_honest` |
| `kwin_phase7a_island` | `island_page_real_input_geometry_retained_draft_appearance_and_screenshots` |
| `kwin_phase7b_launcher` | `launcher_apps_layout_and_draft_use_real_input_in_private_instance`<br>`acceptance::launcher_real_drag_done_persists_across_restart`<br>`acceptance::launcher_real_cancel_does_not_persist`<br>`acceptance::launcher_real_disable_badge_hides_unlocked_module`<br>`acceptance::launcher_home_and_settings_are_not_movable_or_disableable`<br>`acceptance::timer_completed_activity_is_dismissable_by_real_input`<br>`acceptance::frozen_prototype_keeps_real_terminal_and_motion_working`<br>`acceptance::native_pill_side_union_and_launcher_app_pacing_use_real_input`<br>`acceptance::media_capture_stops_on_real_navigation_and_collapse` |
| `kwin_phase9a_release` | `anchor_client_complete_status_embedded_island_and_sigterm_use_real_eis` |
| `kwin_shape` | `kwin_overlay_native_pill`<br>`kwin_preview_non_native_pill` |
| `kwin_spike_panel` | `kwin_spike_panel` |
| `notification_monitor` | `passive_notify_monitor_correlates_notify_return_closed_and_actions`<br>`owner_replacement_rearms_monitor_and_bounds_pending`<br>`loop_guard_and_policy_are_enforced_on_private_bus`<br>`privacy_scrub_at_capacity_repairs_renderer_after_delivery_overflow`<br>`monitor_denial_preserves_hermes_ingest_capability`<br>`dismiss_only_requests_remote_close_for_live_same_server_id` |
| `xvfb_shape` | `overlay_cycle_with_window_manager`<br>`preview_cycle_non_native_pill`<br>`click_through_is_physical_without_window_manager` |
