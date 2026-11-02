# Hermes Island on Tauri v2 — Architecture Specification

Status: **Phases 0, 1, 1.5 and 2 implemented and verified** (see `VERIFICATION.md`; Phase 2 accepted after controller re-run of all gates + mutations, 2026-10-29). Next: Phase 2.5 spike. Target executor: a code-generation agent working file by file. *(status line revised after Phase 1)*
Written 2026-10-29 against the repository at `/home/nick/Hermes/Ripple` (branch `hermes-island`, HEAD `60d4bbb` plus large uncommitted work).

> **Rules for the implementing agent (read first)**
> 1. Implement one phase at a time (§8). Do not start a phase until the previous phase's gate commands pass from a clean build.
> 2. Never run anything against the live data dir `.island-data-overlay/`, the live `panel.sock`, or the user's real clipboard/notification history during tests. Use `--data-dir <scratch>` + `--test-instance` (§2.6).
> 3. Never send Enter to a real Hermes TUI in a test. Fixture PTYs use `bash --norc` and require `HERMES_ISLAND_TEST=1`.
> 4. No `todo!()`, `unimplemented!()`, `unwrap()`/`expect()` on command paths, or silent `let _ =` on fallible service calls in a phase that claims completion. Platforms not yet implemented return `IslandError { code: "unsupported" }` explicitly.
> 5. Gate on `cargo clippy --all-targets --locked -- -D warnings`, `cargo fmt --check`, `cargo test --locked`, `npm run typecheck`, `npm audit` (clean, dev dependencies included), regenerated-and-checked bindings, and the phase's GUI acceptance. "Exited 0" is not acceptance; assertions must fail closed. *(npm audit + bindings added; revised after Phase 1)*
> 6. Do not delete or edit `island/`, `island-rust/`, `island-native/`, `src/`, or the Plasma applet except where a phase explicitly says *copy from*. Legacy runtimes are rollback paths until §8 Phase 9.
> 7. Pin every pre-1.0 / RC crate with `=` (§1.4). Run `cargo tree -d` after each dependency change; duplicate `gtk`/`glib`/`cairo` major lines are a build-breaking error, not a warning.

---

## 0. Ground truth and scope

### 0.1 What exists today (verified on disk)

| Path | What it is | Disposition in this plan |
|---|---|---|
| `src/` (`main.js`, `Island.jsx` 2830 lines, `preload.js`) | Upstream TopMyster/Ripple Electron app: Still/Quick/Large modes, tabs, Quick Apps, Groq AI chat, weather (weatherapi.com + user key), battery alerts (`navigator.getBattery`), Bluetooth/camera/mic status via shell commands, Spotify AppleScript, tray, mac notch addon | **Feature reference only.** Not ported verbatim. |
| `island/` | Nick's fork: React 19 + Framer Motion + xterm 6 renderer (`renderer.jsx`, `style.css`, `monitor.jsx`, `ai-stats.jsx`), Electron main/preload, settings schema, Plasma QML pill (`plasma-anchor/`), `anchor-client.py`, `panel.py`, Playwright/Node/QML tests | **Renderer is the visual source of truth** (copied, then converted to TS in Phase 7). Electron main stays as rollback. |
| `island-rust/` | GPT's Rust host: GTK3 + Wry 0.57.1 + WebKitGTK, `ripple://` protocol, `bridge.js` reimplementing `window.island`. `terminal.rs` (912 lines, portable-pty, replay, bounded queues, setsid cleanup), `services.rs` (settings validation, palette, voice/usage/auth adapters, autostart w/ rollback), `stats.rs` (`/proc` metrics), `host.rs` (X11 ShapeInput, Plasma anchor discovery, panel socket, flock singleton, status.json). 30 Rust tests, live and in use. | **Primary salvage source.** Its service modules are ported into Tauri nearly unchanged; its window/IPC glue (`host.rs` runtime module, `bridge.js`) is replaced by Tauri. |
| `island-native/` | Iced 0.14 + iced_term native-widget feasibility prototype; verdict PARTIAL (real Hermes composer did not render). | **Shelved.** It is an alternative to a webview, therefore mutually exclusive with this Tauri plan. Do not merge code from it except `palette.rs` ideas. |

Live runtime today: `island-rust/target/release/hermes-island --overlay` via `island-rust/run.sh`, autostart enabled, XWayland, window `1180×820` at `(2612,32)` on DP-1 below Plasma panel 245.

### 0.2 Host facts that drive decisions (verified 2026-10-29 on nick-pc)

- CachyOS, kernel 7.3, **KDE Plasma 6.8.1 / KWin 6.8.1 Wayland session**. KWin advertises `zwlr_layer_shell_v1` v5 and **`ext_data_control_manager_v1` v1** (no `zwlr_data_control`).
- Session bus: `org.freedesktop.Notifications` is owned by plasmashell; `org.mpris.MediaPlayer2.spotify` present; `org.kde.klipper` exists but exports **no `/klipper` object** (do not build on Klipper D-Bus).
- System bus: `org.freedesktop.UPower`, `org.bluez`, `power-profiles-daemon` present.
- Toolchain: rustc/cargo 1.99.1, WebKitGTK 4.1 **2.54.2**, GTK **3.24.52**. `cargo tauri` not installed.

### 0.3 Mode vocabulary (canonical)

The user's names are canonical in Rust, on the IPC wire and in new TS. **Revised after Phase 1: there are no serde aliases.** Legacy names (`idle|preview|pinned`) exist only in (a) the Phase 2–6 legacy shim (`ui/ipc/legacy-shim.ts`, translating both directions) and (b) the explicit Rust mapping function that writes `status.json`/the `panel.sock` `status` reply (external tooling reads legacy names). The Rust contract rejects them on input (`IslandMode` deserialisation test).

| Canonical (`IslandMode`) | Upstream Ripple | Fork renderer `data-state` / status.json | Meaning |
|---|---|---|---|
| `still` | Still | `idle` | Resting pill. With the Plasma pill present, the popup window shows nothing. |
| `hover` | Quick | `preview` | Glanceable card (time, weather, battery, now playing, notification toast). Never takes keyboard focus. |
| `large` | Large | `pinned` | Full surface with views/tabs, terminal, settings. Takes focus only via an explicit pin/click. |

### 0.4 Non-goals

- No native-widget rewrite (that is `island-native`). The webview renderer stays.
- No Wayland layer-shell in v1: Tauri/tao still realize the GTK window before user code can call `gtk_layer_init_for_window` (tao#1315 closed without a merged builder hook; tao CHANGELOG has no layer-shell entry). Linux runs via **XWayland** exactly as today.
- No intercepting other apps' notifications on macOS (no public API).
- No reading provider credentials; no system-volume changes; no Plasma restarts.

### 0.5 Product target (added after Phase 2 review)

Long-term target: smooth Dynamic Island animations and deep customizability. Decisions so far (Nick, 2026-10-29):
- **Panel relationship:** keep the native Plasma pill for hover/click input, but *test first* whether the Tauri window can draw over the panel at the pill's position so the island visibly grows out of the pill (Phase 2.5 spike). If the spike fails, report the evidence before choosing an alternative.
- **Features in scope:** music with lyrics and a live equalizer (Phase 6). **Revised after Nick's concept review (Oct 30):** also Timer, Calendar, Scratchpad + Downloads, Controls + Volume mixer (Phase 6b). Camera mirror, file shelf, recent captures, "Watch" and agent-activity tracking stay out of scope unless Nick adds them.
- **Design direction (revised after Nick's concept review, Oct 30; references: `/home/nick/Hermes/Ripple/concept-pics/*.png`, Nick's own screenshots; do not move or edit them).** Not 1:1, but the Phase 7 UI adopts the concept's structure:
  - **Hermes first:** opening the island lands on the Hermes terminal (home); every other app is secondary and one click back to Hermes.
  - **App launcher:** a grid of rounded app tiles (icon + label; keyboard shortcut hint optional) reached from a side button; each app opens inside the island with a title row (`Title … ⋯`) and a back/grid control. Apps = module-registry modules (enable/order/config there).
  - **Launcher edit mode (Nick, Oct 30):** phone-style "jiggle mode": press-and-hold a tile (or an Edit item in the grid's `⋯` menu, for keyboard/accessibility) makes all tiles wiggle; tiles can then be dragged to a new position or disabled/removed via a small badge; Done commits, Escape/Cancel reverts. Moving = module-registry `order`; disabling a module = `enabled: false` (re-enable from a "Hidden apps" row in the same grid or Settings); the Hermes terminal (home) and Settings tiles are locked (cannot be disabled). Pure core: `staging/launcher-arrange-core`. The wiggle is a CSS/Motion animation in Phase 7 (respect prefers-reduced-motion: show the badges without wiggling).
  - **Quick toggles (Nick, Oct 30):** Controls includes at least Do Not Disturb (existing setting), **power profile** (power-profiles-daemon `org.freedesktop.UPower.PowerProfiles` on the system bus: `ActiveProfile` writable, `Profiles` aa{sv}, `PerformanceDegraded`; verified by read-only introspection on nick-pc, v0.30, profiles power-saver/balanced/performance) and **mic mute** (default source mute via `pactl set-source-mute @DEFAULT_SOURCE@ 0|1|toggle`, read via `pactl get-source-mute` → `Mute: yes|no`; nick-pc's default source is the DeepFilterNet virtual source, so mute the default source, not a hard-coded device). Pure core: `staging/quick-toggles-core`. Further toggles only when Nick picks them.
  - **More tiles/features Nick picked (Oct 30; interfaces verified read-only on nick-pc):** (1) **Keep awake** — session-bus `org.freedesktop.ScreenSaver` `/org/freedesktop/ScreenSaver` `Inhibit(ss)->u` / `UnInhibit(u)` (cookie held while on; released on off and on app exit); whether KDE's auto-suspend also honours it is UNVERIFIED (else add a logind `Inhibit("idle:sleep",…)` fd). (3) **Audio output switch** — `pactl -f json list sinks` (fields index, name, description, state, …; nick-pc has a USB headset, SPDIF and monitor-HDMI sinks) and `pactl set-default-sink <name>`; read the current one with `pactl get-default-sink`. (4) **Network / Tailscale** — NetworkManager system bus `State` u (70 = connected global), `Connectivity` u (4 = full), `PrimaryConnectionType` s, `WirelessEnabled`/`WirelessHardwareEnabled` b (the desktop is wired; Wi-Fi tile only when a wireless device exists) + `tailscale status --json` (v1.104: `BackendState` "Running", `Self`, `Peer` map with `HostName`, `Online`, `OS`, `LastSeen`); read-only, no up/down from the island in the first version. (6) **Screenshot / colour picker** — `spectacle -r` (region) and KWin session-bus `org.kde.KWin` `/ColorPicker` `org.kde.kwin.ColorPicker.pick()->(u)` ARGB → `#rrggbb` copied via the clipboard service. (7) **Hermes voice switcher** — `hermes-fish` (list: one voice per indented line, the active one suffixed `[active]`), `hermes-fish current`, `hermes-fish <name>`; switching changes only the TTS voice. (8) **Agent run tracker** — read-only `~/.hermes/state.db` (`mode=ro`, `query_only`) `sessions` rows (`id, source, parent_session_id, model, billing_provider, started_at, ended_at, end_reason, last_activity_at, api_call_count, title`; epoch-second floats). `ended_at IS NULL` is NOT proof of running (29 orphans on nick-pc) → running = no `ended_at` and `last_activity_at` within a stale window; delegated runs = `source` `oneshot`/`subagent`; usage % comes from the existing AI-usage source; a finish raises an activity + notification. (12) **Package update badge** — `checkupdates` (pacman-contrib 1.13; exit 0 = updates, 2 = none, 1 = failure; lines `pkg old -> new`) and `paru -Qua` (AUR; exit 1 with empty output = none), read-only, at most hourly, never installs. Pure cores: `staging/system-tiles-core` (1, 3, 4, 6 colour format, 7), `staging/agent-tracker-core` (8), `staging/updates-core` (12). They join Controls/the island in Phase 6b/7.
  - **Side buttons:** round floating buttons around the island (left, right, bottom; defaults: apps grid + timer left, settings + volume right, now-playing bottom). Fully customizable: a Layout editor in Settings shows a live miniature of the island with dashed `+` slots to add buttons, drag to move, click to edit; size presets Compact / Spacious / Custom (width and max-height sliders). Side buttons are part of the painted shape AND the input region (one owner, §2), and the morph/blur rules apply to them.
  - **Apps (concept-styled):** Hermes terminal (home); AI stats as a card dashboard (per-provider session/week bars with reset countdowns, spending today, "now" active session, hourly trend, per-model and per-project cost, activity heatmap; unknown data shown unavailable, never fabricated); Timer (Timer / Pomodoro / Stopwatch tabs, tick-ruler minute picker, big countdown, Start); System monitor; Notifications; Clipboard; Now playing (lyrics + equalizer); Calendar; Scratchpad; Downloads; Controls (volume, brightness, quick toggles) + Volume mixer (per-app streams); Settings.
  - **Look:** concept layout (dark rounded cards, generous radius, small caps-free titles, coloured app icons) with colours from the active Hermes skin by default, plus a "Neutral black" appearance option matching the concepts. The Phase 2 screenshot-parity rule applies only until Phase 7; from Phase 7 the concept direction replaces it, judged by settled screenshots against the concept pictures (differences explained) and the existing frame-pacing budget.
  - **Calendar backend (DECIDED by Nick, Oct 31; controller probe of nick-pc found no Akonadi/KOrganizer/Merkuro/EDS/khal and no .ics files):** two sources, both parsed and expanded by the accepted `staging/ics-core`, merged into one agenda: (1) **Google Calendar subscription** via its free "Secret address in iCal format" URL — opt-in, read-only (HTTPS GET only, never writes back), the URL is a secret: stored only in a 0600 file in the island data dir (not in settings.json, not in status.json, never logged, never sent over IPC to the renderer — the UI sees only "configured / not configured" and fetch status), bounded download size and timeout, periodic refresh with backoff, last good copy cached privately so a failed fetch shows stale-with-age rather than empty; (2) **island-local events** stored as an `.ics` file in the island data dir (create/edit/delete in the Calendar app, atomic same-dir temp + rename, 0600). Local events are editable; subscribed events are read-only and visually marked. Tests use fixture .ics bodies and a private loopback HTTP server only, never Nick's real URL or data dir.
  - **Data sources still to verify before their phase (UNVERIFIED):** volume mixer/brightness APIs (PipeWire/wpctl, PowerDevil D-Bus). Tests use private PipeWire/D-Bus instances only and never change the real volume or brightness.
- **Architecture consequences:** (1) the Phase 3 state machine models *activities* (several live sources with priority, a compact and an expanded presentation, and a two-bubble split when two are live) on top of the existing `Still/Hover/Large` modes, not instead of them; (2) customizability is a versioned module registry (per-module `enabled`, `order`, typed config; settings export/import), introduced as an additive `Settings` field with a migration from the flat keys, never by renaming existing keys — **implemented and audited as the pure crate `staging/module-registry`** (added after module-registry review): additive `moduleRegistry` field; valid unknown module ids are preserved verbatim but always inert (enabling them is rejected; invalid slugs rejected, never repaired); imports replace rather than merge, are atomic and capped at 256 KiB; reorder must be a full permutation of present ids (unknown included); clipboard and music default disabled, music defaults lyrics/equalizer on with 16 bands (8..=32); migration maps ten real flat keys and reports the rest; the version-0 upgrade fixture is the pattern for later versions; (3) blur-behind uses KWin's `_KDE_NET_WM_BLUR_BEHIND_REGION` kept in sync with the input region; (4) GUI gates for animation work add a measured frame-pacing check in WebKitGTK (requestAnimationFrame deltas during a spring: report p50/p95/max and dropped frames; budget set from the Phase 2.5 baseline).

---

## 1. Cargo and project layout

### 1.1 Directory tree (all new files live under `island-tauri/`)

```
island-tauri/
  SPEC.md                       ← this document
  README.md                     ← Phase 1: build/run/test commands only
  package.json                  ← UI workspace (Vite 8, @vitejs/plugin-react 6, React 19, TS 5, @tauri-apps/api ^2) (revised after Phase 1: Vite ≥ 6 required; `npm audit` must be clean)
  tsconfig.json  vite.config.ts  index.html
  scripts/
    check-bindings.mjs          ← fails CI if ui/ipc/bindings.ts is stale
    run.sh                      ← production launcher (env hygiene, GDK_BACKEND=x11), port of island-rust/run.sh
  ui/
    main.tsx
    ipc/bindings.ts             ← GENERATED by tauri-specta (config: §5.1). Never hand-edit.
    ipc/client.ts               ← unwrap Result, typed channel helpers, revision guards
    ipc/legacy-shim.ts          ← Phase 2–6 only: implements window.island on top of bindings
    state/store.ts              ← useSyncExternalStore over IslandSnapshot + topic stores
    island/Island.tsx           ← (Phase 7) port of renderer.jsx shell
    island/geometry.ts          ← GeometryReporter (§4.4)
    views/{Terminal,Monitor,AiStats,Settings,Notifications,Clipboard}View.tsx
    widgets/{QuickInfo,NowPlaying,Toast}.tsx
    legacy/                     ← Phase 2: verbatim copies of island/*.jsx|*.cjs|style.css
    styles/style.css            ← verbatim copy of island/style.css (Phase 2), then owned here
    assets/fonts/               ← copy of src/assets/fonts/OpenRunde-*.woff
  src-tauri/
    Cargo.toml  build.rs  tauri.conf.json  tauri.linux.conf.json
    tauri.windows.conf.json  tauri.macos.conf.json
    capabilities/island.json
    permissions/island.toml
    icons/                      ← generated by `cargo tauri icon src/assets/icons/icon.png`
    src/
      main.rs                   ← 10 lines: parse CLI subcommand, call lib::run()
      lib.rs                    ← builder wiring only (plugins, specta, setup, run)
      cli.rs                    ← `--overlay`, `--test-instance`, `--data-dir`, `notify` subcommand
      error.rs                  ← IslandError
      paths.rs                  ← data dir resolution, socket/status/lock paths
      identity.rs               ← frozen window title / WM_CLASS / prgname contract (Phase 1 readback)
      singleton.rs              ← flock + live-status check (port of host.rs lines ~245–320)
      contract/                 ← ALL types that cross the IPC boundary (§5)
        mod.rs island.rs settings.rs terminal.rs notify.rs clipboard.rs system.rs ai.rs events.rs
      state/
        machine.rs              ← pure reducer + timers (§4.2), no Tauri imports
        actor.rs                ← owns IslandState, applies intents, emits snapshots
      bus.rs                    ← typed emit helpers, coalescing for latest-value topics
      window/
        mod.rs                  ← create_island_window(), show/hide, focus policy
        placement.rs            ← monitor/work-area math (generic)
        shape.rs                ← rounded-rect → row decomposition (port of host.rs shape_rows + tests)
        input_region/{mod.rs, linux.rs, windows.rs, macos.rs}
      commands/
        mod.rs island.rs settings.rs terminal.rs notifications.rs clipboard.rs
        system.rs media.rs ai.rs voice.rs diagnostics.rs
      services/
        settings.rs             ← port of island-rust services.rs validate/atomic_write/defaults
        palette.rs              ← port: Hermes skin read + notify watcher + debounce
        terminal/{mod.rs, pty.rs, flow.rs}   ← port of island-rust terminal.rs + credit flow (§3.4)
        stats/{mod.rs, linux_proc.rs, sysinfo.rs}
        ai_usage.rs voice.rs autostart.rs   ← ports of the bounded-subprocess adapters
      integrations/
        plasma/{anchor.rs, panel_socket.rs, kwin_place.rs, status_file.rs}
        notifications/{mod.rs, store.rs, linux_monitor.rs, windows_listener.rs, hermes_ingest.rs}
        clipboard/{mod.rs, history.rs, linux_wayland.rs, linux_x11.rs, windows.rs, macos.rs}
        power/{mod.rs, upower.rs, portable.rs}
        bluetooth/{mod.rs, bluez.rs, windows.rs}
        media/{mod.rs, mpris.rs, windows_gsmtc.rs}
        weather/{mod.rs, open_meteo.rs}
      tray.rs
    tests/                      ← Rust integration tests (shape readback, ingest socket, reducer)
      common/mod.rs             ← shared GUI-test harness pieces (X11 readback, pixel checks, identity/focus asserts)  (revised after Phase 1)
      xvfb_shape.rs             ← Phase 1: Xvfb + xfwm4 / no-WM
      kwin_shape.rs             ← Phase 1.5: nested headless KWin + XWayland, physical input via KWin's EIS
      helpers/eis_pointer.py    ← libei pointer sender for the private KWin (XTest cannot move KWin's pointer)
  tests/
    gui_smoke.py                ← port of island-rust/tests/gui_smoke.py targeting the Tauri binary
    test_launcher.py
```

### 1.2 `src-tauri/Cargo.toml`

Versions below were read from crates.io on 2026-10-29. `gtk`, `gdk`, `glib`, `cairo-rs` **must stay on the 0.18 line** because `tauri 2.12.5` / `tao 0.37.2` / `wry 0.57.1` depend on `gtk ^0.18`; newer 0.19–0.22 releases produce incompatible types.

```toml
[package]
name = "hermes-island"
version = "5.0.0"
edition = "2021"
rust-version = "1.85"

[lib]
name = "hermes_island_lib"
crate-type = ["rlib"]

[build-dependencies]
tauri-build = { version = "2.7", features = [] }

[dependencies]
tauri = { version = "2.12", features = ["tray-icon", "image-png"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
specta = { version = "=2.0.0-rc.26", features = ["derive", "serde_json"] }
specta-typescript = "=0.0.13"
tauri-specta = { version = "=2.0.0-rc.26", features = ["derive", "typescript"] }
tokio = { version = "1", features = ["sync", "time", "macros", "rt-multi-thread", "net", "io-util", "fs", "signal"] }
tokio-util = "0.7"                              # CancellationToken for service shutdown
thiserror = "2"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
portable-pty = "0.9"
notify = "8"
libc = "0.2"
reqwest = { version = "0.13", default-features = false, features = ["json", "rustls", "http2"] }
xxhash-rust = { version = "0.8", features = ["xxh3"] }
image = { version = "0.25", default-features = false, features = ["png"] }

[target.'cfg(target_os = "linux")'.dependencies]
gtk = "0.18"
gdk = "0.18"
glib = "0.18"
cairo-rs = "0.18"
zbus = { version = "6", default-features = false, features = ["tokio"] }
x11rb = { version = "0.14", features = ["shape", "xfixes"] }   # readback tests + X11 clipboard fallback
wl-clipboard-rs = "0.9"                        # ext-data-control/wlr-data-control read/write
wayland-client = "0.31"
wayland-protocols = { version = "0.32", features = ["client", "staging"] }  # ext_data_control_v1 watcher
wayland-protocols-wlr = { version = "0.3", features = ["client"] }

[target.'cfg(not(target_os = "linux"))'.dependencies]
sysinfo = "0.40"
starship-battery = "0.12"
arboard = { version = "3.7", features = ["image-data"] }
clipboard-master = "4.1"

[target.'cfg(windows)'.dependencies]
windows = { version = "0.62", features = [
  "Win32_UI_WindowsAndMessaging", "Win32_Foundation", "Win32_Graphics_Gdi",
  "UI_Notifications_Management", "UI_Notifications", "Media_Control",
  "Devices_Enumeration", "Devices_Bluetooth", "Foundation_Collections" ] }

[target.'cfg(target_os = "macos")'.dependencies]
tauri-nspanel = "2.1"
objc2 = "0.6"

[profile.release]
lto = "thin"
codegen-units = 1
strip = "debuginfo"
panic = "abort"
```

**Revised after Phase 1: `keyring` is removed from the plan.** Weather uses Open-Meteo, which needs no key, so there is no secret to store; the `weatherapi` provider (upstream parity) is dropped. `keyring 4` pulled `secret-service 5` → `zbus 5` next to the SPEC's `zbus 6` (verified in Phase 0 with `cargo tree -d`). The tree must contain **a single `zbus` (6.x)**; `cargo tree -i keyring` must fail. If a secret is ever needed again it needs a new, explicitly approved SPEC revision first.

Phase 0 must verify: `cargo tree -i gtk` shows exactly one `gtk v0.18.x`; `cargo tree -i specta` shows exactly `2.0.0-rc.26`; zbus 6's `tokio` feature name compiles (adjust only the feature list, never the major version, if it does not). Dependencies for later phases may be added in their phase rather than up front; the table is the target state.

### 1.3 `build.rs` — permissioned app commands

```rust
fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&crate_commands::ALL), // literal list, see §5.5
        ),
    )
    .expect("tauri build");
}
```
Declaring the app manifest makes every app command require an explicit `allow-<command>` permission, so `capabilities/island.json` is a real allow-list. The terminal-input command is powerful; it must be granted only to the `island` window.

### 1.4 Pinned / pre-1.0 crates

| Crate | Pin | Why |
|---|---|---|
| `tauri-specta` | `=2.0.0-rc.26` | Only Tauri-v2-compatible line; stable 1.0.2 targets Tauri 1. Upstream docs require `=`. |
| `specta` | `=2.0.0-rc.26` | Must equal tauri-specta's exact requirement. |
| `specta-typescript` | `=0.0.13` | 0.0.13 depends on `specta =2.0.0-rc.26`; 0.0.14 may move to rc.27. |
| `wry`/`tao` | not direct deps | Let `tauri` choose; never add direct `wry` (island-rust pinned `=0.57.1`; remove that pin in the new crate). |

Fallback if tauri-specta RC breaks: replace the generator with `ts-rs 12` on the same `contract/` types plus a hand-written `ui/ipc/commands.ts`. The contract module is designed so this is a mechanical swap (§5.6). **Revised after Phase 1: the fallback was evaluated and not needed** — the two Phase 1 generator defects were fixed by configuration (§5.1), so the generator stays tauri-specta/specta rc.26.

---

## 2. Window architecture (Tauri v2)

### 2.1 Principle

`tauri.conf.json` can only express **static** window properties. "Click-through in Still, interactive in Hover/Large" is **dynamic** and cannot be configured; it is owned by a Rust `InputRegionController` (§2.4) that is the *only* code allowed to change pointer pass-through. Static config makes the window: borderless, transparent, shadowless, non-resizable, skip-taskbar, always-on-top, visible on all workspaces, focusable-but-not-focused-on-show, created hidden.

### 2.2 `src-tauri/tauri.conf.json`

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Hermes Island",
  "version": "5.0.0",
  "identifier": "local.hermes.island",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:5173",
    "beforeDevCommand": "npm run dev:ui",
    "beforeBuildCommand": "npm run build:ui"
  },
  "app": {
    "withGlobalTauri": false,
    "enableGTKAppId": true,
    "windows": [
      {
        "label": "island",
        "create": false,
        "url": "index.html",
        "title": "Hermes Island",
        "width": 1180,
        "height": 820,
        "resizable": false,
        "maximizable": false,
        "minimizable": false,
        "decorations": false,
        "transparent": true,
        "backgroundColor": "#00000000",
        "shadow": false,
        "alwaysOnTop": true,
        "visibleOnAllWorkspaces": true,
        "skipTaskbar": true,
        "focus": false,
        "focusable": true,
        "visible": false,
        "dragDropEnabled": false,
        "acceptFirstMouse": true,
        "backgroundThrottling": "disabled",
        "zoomHotkeysEnabled": false,
        "preventOverflow": false
      }
    ],
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; frame-src 'none'; form-action 'none'",
      "freezePrototype": false,
      "capabilities": ["island-main"]
    }
  },
  "bundle": {
    "active": true,
    "targets": ["deb", "rpm", "appimage", "nsis", "dmg"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/icon.icns", "icons/icon.ico"],
    "category": "Utility",
    "shortDescription": "Hermes Dynamic Island",
    "license": "GPL-3.0-or-later"
  }
}
```

Notes the implementer must respect:
- `create: false` — the window is built in `setup()` via `WebviewWindowBuilder::from_config(app, cfg)` so placement, platform hooks, and the input region are applied **before first show**. Never let Tauri auto-create it.
- `focusable: true` + `focus: false` — lesson carried from Electron: an unfocusable overlay becomes override-redirect and can never receive keyboard focus for the terminal. Focus is requested explicitly only on pin (§2.5).
- `backgroundThrottling: "disabled"` — the window is "hidden" from the compositor's perspective most of the time; throttling would stall PTY channel delivery.
- `'unsafe-inline'` is only in `style-src` (Framer Motion writes style attributes). No remote origins: weather/HTTP happen in Rust.
- `freezePrototype` starts `false`; Phase 7 flips it to `true` after xterm + Framer Motion pass the GUI smoke with it enabled.
- `macOSPrivateApi` is intentionally absent: the Tauri schema documents it as a no-op from 2.12.1 because transparency APIs are always enabled.
- `windowClassname` is **Windows-only** per the schema; Linux identity comes from §2.3.

Platform merge files (Tauri merges `tauri.<platform>.conf.json` as JSON Merge Patch):
- `tauri.linux.conf.json`: `{ "bundle": { "linux": { "deb": { "depends": ["libwebkit2gtk-4.1-0", "libgtk-3-0"] } } } }`
- `tauri.windows.conf.json`: `{ "app": { "windows": [ { "label": "island", "create": false, "noRedirectionBitmap": true, "windowClassname": "HermesIsland" } ] } }` (arrays are replaced wholesale, so repeat the full window object in the real file; the snippet shows only the delta).
- `tauri.macos.conf.json`: nothing window-specific; macOS panel conversion is done in code (§2.3).

### 2.3 Window initialization sequence (`window/mod.rs::create_island_window`)

Executed inside `tauri::Builder::setup`, in this order. Each step is a separate function with its own unit/integration test.

1. **Before `tauri::Builder` (in `lib::run`)**: `cli::parse()`; `paths::resolve(data_dir)`; `singleton::acquire()` (flock on `<data>/island.lock`, then refuse if `status.json` names a live PID whose `/proc/PID/exe` is another island runtime **or** `panel.sock` answers `status`). On Linux also `glib::set_prgname(Some("hermes-island"))` and `glib::set_application_name("Hermes Island")` exactly as island-rust does (`host.rs` ≈ L580–587), and set the title per mode to the existing strings `"Hermes Island — panel overlay"` / `"Hermes Island — isolated preview"`. PID + title + `WM_CLASS` are a shared identity contract with `kwin_place.rs`, the GUI tests, and status tooling: in Phase 1 read the actual `_NET_WM_PID`, `_NET_WM_NAME`, and `WM_CLASS` from X11 (Tauri/tao or `enableGTKAppId` may influence them) and freeze the observed values in one `identity.rs` constant module used by every consumer. Do **not** use `tauri-plugin-single-instance` on Linux: it cannot see legacy Electron/Wry instances.
2. `settings = services::settings::load(&paths)`.
3. `target = placement::resolve(app, &settings)`:
   - Linux + Plasma: `integrations::plasma::anchor::discover()` (port of `host.rs::discover_anchor`/`anchor_from_data`, still using `kscreen-doctor -j` and `qdbus6` as bounded subprocesses, off the main thread). Window origin = `(anchor.center_x − 590, panel.top + panel.height)` clamped to the physical screen, as today.
   - Generic: monitor = `settings.displayId` or primary; `x = monitor.work_area.x + (work_area.width − 1180)/2`, `y = monitor.work_area.y` (notch mode on macOS: `y = monitor.position.y`).
4. `let cfg = app.config().app.windows.iter().find(|w| w.label == "island")`. Build: `WebviewWindowBuilder::from_config(app.handle(), cfg)?.position(target.x, target.y).build()?` (window is created **hidden**).
5. Platform hooks while hidden:
   - **Linux** (`gtk_window()` → `gtk::ApplicationWindow`): `set_type_hint(gdk::WindowTypeHint::Toolbar)`, `set_skip_pager_hint(true)`, `set_focus_on_map(false)`, `set_accept_focus(true)`, `stick()`, `set_keep_above(true)`. These match the proven island-rust host (`host.rs` ≈ L582–602). Type hints must be set before mapping.
   - **Windows**: `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` via `SetWindowLongPtrW`; `SetWindowPos(HWND_TOPMOST)`.
   - **macOS**: convert to `NSPanel` with `tauri-nspanel` (non-activating style mask, level `NSMainMenuWindowLevel + 1`, collection behavior `canJoinAllSpaces | fullScreenAuxiliary | stationary`). `ActivationPolicy::Accessory` (no Dock icon).
6. `InputRegionController::install(window)` and apply the **Still** region for the current anchor mode (empty when the Plasma pill is present). This happens *before* show so there is no frame where an 1180×820 transparent rectangle eats clicks.
7. `window.show()`; Linux only: read back the actual frame position. If KWin did not honor the requested position, fall back to the narrow one-shot KWin script pattern from `island/overlay.cjs` (match exact PID **and** the frozen identity from step 1, set geometry, unload itself; no global rules). Record geometry in `status.json`.
8. Start services (§3), the panel socket, the tray, and mount tauri-specta events.

Tear-down (`RunEvent::ExitRequested` / SIGTERM / SIGINT via `tokio::signal`): stop accepting IPC, drain terminal (port of the bounded owned-process cleanup), remove only this instance's socket inode and status PID, release flock.

### 2.4 Click-through: `InputRegionController`

**Single owner rule:** on Linux nothing may call Tauri's `set_ignore_cursor_events` — tao's Linux event loop handles it by applying a **1×1** input region at (0,0) (or `None` to clear), overwriting the same input shape this controller owns (verified in tao `src/platform_impl/linux/event_loop.rs` ≈ L459; corrected after Phase 1 review).

```rust
pub struct HitRegion { pub rects: Vec<RoundedRect>, pub mode: IslandMode, pub geometry_rev: u32 }
pub trait InputRegionBackend: Send {
    fn apply(&mut self, region: &[ShapeRow]) -> Result<(), IslandError>; // physical px, window-relative
    fn readback(&self) -> Result<Vec<ShapeRow>, IslandError>;             // tests + status.json
}
```

Algorithm (shared, in `input_region/mod.rs`):
1. Inputs: `HitRegion` from the frontend GeometryReporter (§4.4) and the authoritative `IslandMode` from the state actor.
2. Reject reports whose `mode`/`geometry_rev` do not match the actor's current transition (prevents a stale animation frame from re-opening a collapsed region).
3. Policy (table below) → list of rounded rects → `shape::rows()` (exact port of `host.rs::shape_rows`, including its bounds validation against the window size and 0.5–4.0 scale range, plus its tests).
4. Deduplicate on `(mode, scale, rows)`; apply only on change, on the GTK main thread via `run_on_main_thread`.
5. **Transition rule**: when growing (still→hover, hover→large), immediately apply the *target* rect union the current rect so the pointer never falls outside the region mid-animation; when shrinking, keep the larger region until the frontend reports `settled: true`, then apply the final region. At most one region apply per animation frame is ever requested; the frontend only reports on Motion `onUpdate` coalesced to rAF and on `onAnimationComplete`.

| Mode | Plasma native pill present (nick-pc) | No native pill (other DE / Windows / macOS) | Keyboard focus |
|---|---|---|---|
| `still` | **empty region** — 100% click-through; the QML pill in the panel owns hover/click | still-pill rounded rect only | never |
| `still` + stealth | empty | still-pill rect (rendered at opacity 0.08 so it can still be found) | never |
| `hover` | hover-card rect | hover-card rect | **never** (hover must not steal focus) |
| `hover` caused by notification toast | toast rect | toast rect | never |
| `large` | large-surface rect (includes offsets — offsets move the whole `.island`, never inner contents) | same | requested once on entry **only if** the transition cause is an explicit pin/click/tray action |

Backends:
- **Linux (`linux.rs`)**: build a `cairo::Region` from the rows (`Region::create_rectangles`) and call `gtk_window.input_shape_combine_region(Some(&region))` (widget-level, so GTK reapplies it if the window is re-realized). Under XWayland this produces the same X11 `ShapeInput` region the island-rust host sets with `XShapeCombineRectangles`, and it would also work if GDK ever runs on Wayland. Never touch the bounding shape (visual) — transparency handles visuals. `readback()` uses `x11rb` `shape::get_rectangles(window, SK::INPUT)`.
- **Windows (`windows.rs`)**: Windows has no input-only region separate from visuals (`SetWindowRgn` clips painting). Use event-driven toggling: a dedicated thread installs a `WH_MOUSE_LL` hook; on each move it tests the cursor against the current region (in an `ArcSwap`/`RwLock`) and calls `window.set_ignore_cursor_events(!inside)` via `run_on_main_thread` **only on boundary crossings**. Region empty ⇒ ignore always, hook may be uninstalled.
- **macOS (`macos.rs`)**: same crossing logic using `NSEvent addGlobalMonitorForEventsMatchingMask:NSEventMaskMouseMoved` plus a local monitor; toggle `NSPanel.ignoresMouseEvents`.
- Both non-Linux backends must debounce flapping at the edge with a 1 px hysteresis band.

### 2.5 Focus policy

- `hover` never calls `set_focus()`.
- Entering `large` with cause `Pin | TrayOpen | GlobalShortcut` calls `window.set_focus()` once (Linux: `present()` equivalent, as island-rust does on `pin`). Entering `large` because the user clicked inside the hover card is also a pin.
- Escape outside the terminal → `Collapse` intent (renderer already guards `.terminal`).
- Window blur does **not** collapse (current renderer has no blur collapse). Expose `collapseOnBlur: false` setting later only if requested.

### 2.6 CLI, instances and data directories

`hermes-island [--overlay] [--test-instance] [--data-dir PATH] [--fixture-pty]` and `hermes-island notify --title T [--body B] [--level info|success|warning|error] [--ttl-ms N]`.
- Phase 2–8: default data dir stays `<repo>/.island-data-overlay` for `--overlay` (byte-compatible settings) and `<repo>/.island-data` for preview, exactly as island-rust resolves them today. `--test-instance` requires `--data-dir` and refuses the live directories.
- Migration to `$XDG_DATA_HOME/local.hermes.island/` is a separate, explicitly approved step (Phase 9) with an exact move list presented to the user first.

---

### 2.7 KWin findings (Phase 1.5, revised after Phase 1)

Verified on KWin 6.8.1 (nested `--virtual --xwayland`, see `VERIFICATION.md`); these refine §2.3–2.5, nothing in the architecture changed.

- KWin maps the window **undecorated at the requested position** (`_NET_FRAME_EXTENTS` absent/0, the KWin frame window coincides with the client). Under `decorations: true` KWin adds a 36 px title bar and moves the client down — the tests fail on that. In the sandbox the window origin is `y = 0`, so the frame area *above* it is off-screen and cannot be click-tested; that edge is covered by the static frame-extents/frame-geometry assertion (left, right and bottom edges are click-tested).
- KWin does **not** focus a mapped TOOLBAR window even when `focus_on_map(true)`; the only guard against a focus-on-map regression is GTK's request signal (`_NET_WM_USER_TIME == 0`, present on the toplevel *or* the `_NET_WM_USER_TIME_WINDOW` depending on WM/timing). Keep that assertion; the observed outcome alone proves nothing under KWin.
- KWin honours `ShapeInput` for physical pointer delivery: clicks outside the rounded region pass through to the window below, inside it are consumed and KWin activates the island (click-activation is the *explicit pin* path; hover without a click never activates).
- Test-only: XTest cannot move KWin's Wayland pointer; physical input uses KWin's own EIS endpoint (`org.kde.KWin.EIS.RemoteDesktop` on the private bus). GTK3 ignores core key events sent with `XSendEvent` unless `GDK_CORE_DEVICE_EVENTS=1`; the KWin tests set it for the app so the debug key can drive modes while the island is unfocused. Production leaves GDK's XInput2 default.

## 3. Rust backend system design

### 3.1 Runtime model

- Tauri's async runtime (Tokio) hosts services. Blocking work (PTY reads, subprocesses, file IO bursts) uses dedicated threads or `spawn_blocking`; **no blocking call in an `async` command body**.
- One `StateActor` task owns `IslandState`; everything else sends it `Intent`s over an `mpsc::Sender<Intent>` (bounded 256). The actor is the only emitter of `IslandStateChanged`.
- Services publish into `bus.rs`:
  - **Latest-value topics** (power, weather, bluetooth, media, stats, palette, settings): stored in `tokio::sync::watch`; the forwarder coalesces and emits at most every 100 ms.
  - **Queue topics** (notifications, clipboard-added, alerts): bounded `mpsc` 128; overflow drops oldest and increments a `dropped` counter reported in diagnostics.
- Every payload carries `rev: u32` (wrapping). The frontend ignores stale revisions.
- On every webview (re)load the frontend calls `island_bootstrap()` which returns the full `BootstrapSnapshot` (§5.3). This is the "snapshot on connect/restart" guarantee; events are deltas after that.

### 3.2 Notifications hub

```
 ┌──────────────┐   ┌─────────────────────┐   ┌────────────────┐
 │ DBus monitor │──►│                     │──►│ StateActor     │──► IslandStateChanged (hover toast)
 │ (Linux)      │   │ NotificationStore   │   └────────────────┘
 ├──────────────┤   │ ring 200, dedupe,   │──► NotificationReceived / NotificationClosed (to UI)
 │ Win listener │──►│ privacy filter, DND │
 ├──────────────┤   │                     │──► tray badge / tooltip
 │ Hermes ingest│──►│                     │
 │ socket + CLI │   └─────────────────────┘
 ├──────────────┤
 │ System alerts│ (battery thresholds, BT connect, terminal exited)
 └──────────────┘
```

**Linux: passive D-Bus monitor (`linux_monitor.rs`)** — mirror, do not replace, Plasma's notification server.
1. Connection A (normal session connection): `GetNameOwner("org.freedesktop.Notifications")` → unique name `:1.N`; subscribe to `NameOwnerChanged` to re-arm when plasmashell restarts.
2. Connection B (dedicated, becomes monitor-only and can never send again): call `org.freedesktop.DBus.Monitoring.BecomeMonitor(rules, 0)` with:
   - `type='method_call',interface='org.freedesktop.Notifications',member='Notify'`
   - `type='method_return',sender='<unique owner>'` (to learn the server-assigned ID by `reply_serial`)
   - `type='signal',interface='org.freedesktop.Notifications',member='NotificationClosed'`
   - `type='signal',interface='org.freedesktop.Notifications',member='ActionInvoked'`
   Rebuild connection B whenever the owner changes. (Reference implementations of this exact pattern: `odilia-notify`, `sigrun::monitor`.)
3. Correlate `Notify` serial → method_return `u32` id with a 256-entry LRU; drop unmatched returns.
4. Parse `Notify(app_name s, replaces_id u, app_icon s, summary s, body s, actions as, hints a{sv}, expire_timeout i)`. Hints used: `urgency` (y), `desktop-entry` (s), `category` (s), `transient` (b), `image-path`/`image_path` (s), `image-data`/`icon_data` (iiibiiay → PNG ≤128 px, encoded off-thread), `x-kde-origin-name`. Body markup: strip to plain text (allow `<b><i><u>` → semantic flags only).
5. Filters: drop our own `app_name == "Hermes Island"` (loop guard); per-app deny list; `redactBodies` setting replaces body with "Hidden"; never persist to disk unless `notificationHistoryPersist` is on (default off).
6. Capabilities honestly reported: `canInvokeActions: false` (a monitor cannot trigger `ActionInvoked`; the server owns actions). `dismiss` calls `CloseNotification(id)` on connection A as best effort and always removes the item from our store.
7. Test: `dbus-run-session` + a tiny mock server (python `dbus-next` or a zbus test binary) owning `org.freedesktop.Notifications`, then `notify-send`. Never test against the user's bus.

**Windows (`windows_listener.rs`)**: `UserNotificationListener::Current()`, `RequestAccessAsync()`, `GetNotificationsAsync(Toast)` on start + `NotificationChanged` subscription. This API needs app package identity (MSIX or sparse package). Phase 8 spike must prove it from the packaged build; until proven, return `unsupported` and rely on Hermes ingest.

**macOS**: `unsupported` for third-party interception. Hermes ingest and system alerts work everywhere.

**Hermes ingest (`hermes_ingest.rs`)** — the "Hermes" in Hermes Island:
- Linux/macOS: Unix socket `<data>/ingest.sock`, mode `0600`, owner check via `SO_PEERCRED`/`getpeereid` (same UID only). Windows: named pipe `\\.\pipe\hermes-island-<username>` with a DACL for the current user.
- Protocol: newline-delimited JSON, max 16 KiB per line, max 20 messages/s per connection, max 8 concurrent connections, idle timeout 10 s. One reply line per request.
```json
{"v":1,"op":"notify","title":"Build finished","body":"cargo test: 41 passed","level":"success","ttlMs":6000,"source":"hermes:cron","dedupeKey":"build-41"}
{"v":1,"op":"progress","dedupeKey":"dl-1","title":"Downloading model","progress":0.42}
{"v":1,"op":"clear","dedupeKey":"dl-1"}
→ {"ok":true,"id":"h-17"}   |   {"ok":false,"error":{"code":"invalid","message":"title required"}}
```
- `hermes-island notify ...` is a thin client of the same socket so Hermes skills, cron jobs, and shell hooks can push without any library.
- The existing `panel.sock` protocol stays separate and unchanged (`status|enter|leave|pin` only); never accept notifications or terminal input there.

**Pure core already built (added after notification-store review):** `staging/notification-store` (audited) implements the store, D-Bus correlation helper and ingest parsing/rate limiter; Phase 4 ports it into `src-tauri` and wires the platform parts. Its decisions are binding: TTL is entry lifetime, separate from the actor's toast timer (default 6000 ms; 0 = resident; non-transient critical defaults resident; explicit TTL wins; expiry inclusive); Hermes `dedupeKey` does full replacement with stable id + move-to-front; progress < 1 is resident; D-Bus correlation keys on caller identity + serial, the returned server id is authoritative, and both maps reset on owner change; event payloads carry wrapping u32 revisions; policy changes retroactively scrub bodies / drop denied apps (irreversible) and emit deltas; the 16 KiB line cap excludes the final LF/CRLF; the limiter is a 20-token bucket refilling at 20/s and invalid requests consume tokens. Open test gap for Phase 4: removing the image-width bound in `dbus.rs` fails no test — add one.

**Tray (`tray.rs`)** — `TrayIconBuilder` with menu: *Show/Hide island*, *Open terminal*, *Notifications (n)*, *Do Not Disturb* (check item), *Settings…*, separator, *Restart terminal*, *Quit*. Every item maps to an `Intent` sent to the actor (no UI logic in the tray). Linux needs `libayatana-appindicator` at runtime; Plasma hosts it via StatusNotifierItem. Tooltip shows unread count.

### 3.3 Clipboard history manager

**Assessment of `copypasta` (requested):** not suitable as the core. It is text-only, has no change notification, and its Wayland backend requires the app's own `wl_display` pointer — but this app runs under XWayland, where an X11 client only receives the Wayland selection when one of its windows has focus. It can't build a history on Nick's machine. Use:

| Platform | Change detection (event-driven) | Read / write | Notes |
|---|---|---|---|
| Linux Wayland session (nick-pc) | Dedicated thread with its own `wayland-client` connection binding `ext_data_control_manager_v1` (verified present on KWin 6.8.1) → `data_control_device.selection` events. Fallback `zwlr_data_control_manager_v1` for wlroots compositors. | `wl-clipboard-rs 0.9` (supports ext-/wlr-data-control) | Works without focus and independent of XWayland. |
| Linux X11 session | `x11rb` XFixes `SelectSelectionInput(CLIPBOARD)` | `x11rb` or `arboard` | |
| Windows | `clipboard-master` (`AddClipboardFormatListener`) | `arboard` | Skip if format `ExcludeClipboardContentFromMonitorProcessing` or `CanIncludeInClipboardHistory = 0` is present. |
| macOS | `NSPasteboard.changeCount` checked every 750 ms (no notification API exists) | `arboard` | Skip `org.nspasteboard.ConcealedType` / `TransientType`. |

`history.rs` (pure, unit-tested, no platform code):
- `VecDeque<ClipEntry>`, capacity `clipboardHistorySize` (default 100, max 500). Dedup by `xxh3_64(kind || bytes)`; a duplicate moves to the front.
- Kinds: `Text` (≤1 MiB stored; preview = first 280 chars, whitespace-collapsed), `Image` (PNG, longest side ≤4096 px, ≤8 MiB; 160 px thumbnail generated lazily), `Files` (list of URIs from `text/uri-list`).
- Sensitive skip: MIME `x-kde-passwordManagerHint` = `secret`; platform concealed types above; plus user regex deny list (validated, length-capped). Clipboard monitoring defaults **off** (`clipboardHistory: false`) — this is a privacy-sensitive feature and must be opt-in.
- Pinned entries survive clears. Persistence opt-in (`clipboardPersist`), file `<data>/clipboard.json` mode 0600, pinned entries only by default.
- Self-echo guard: when `clipboard_copy(id)` writes, store the expected hash for 2 s; the resulting change event only reorders.
- Pause: `clipboard_set_paused(bool)`; paused state shown in UI and tray.

### 3.4 Terminal (port, with one upgrade)

Port `island-rust/src/terminal.rs` with its tests: hygiene of inherited env, Hermes resolution, replay buffer (1 MiB), owned-process-tree cleanup, setsid/stalled-consumer cases. Transport changes:
- Output goes through a **`tauri::ipc::Channel<TerminalEvent>`** passed to `terminal_attach`, not global events (channels are the Tauri v2 mechanism for ordered high-throughput streams).
- **Credit-based backpressure** (closes a gap island-rust documents as unsolved): Rust sends `Data { seq, data }` chunks ≤64 KiB; the UI calls xterm `write(data, () => ack(seq))` and batches `terminal_ack(seq)` every ≤16 ms. Rust stops reading the PTY when unacknowledged bytes exceed 1 MiB, so a slow renderer back-pressures the child instead of growing memory. This proves xterm *parsed* the data, not just that WebKit evaluated a script.
- **Phase 2 parse-ack adapter (revised after Phase 2 review).** The byte-identical legacy renderer calls `terminal.write(data)` with no callback, so acks come from `ui/ipc/xterm-ack-adapter.ts` (new file, outside `ui/legacy/`), installed by the shim before the renderer's dynamic `import()`. It imports `Terminal` from the *same specifier* the renderer uses (`@xterm/xterm/lib/xterm.js`, so Vite yields one module instance) and wraps `Terminal.prototype.write(data, cb?)` once: the shim records every delivered `Data{seq,data}` and the attach/`Snapshot` result (`data`, `seq`) in an ordered pending list; when a wrapped write's data is the oldest pending entry's string, the adapter passes xterm a callback that, on parse completion, calls the original `cb` and schedules a cumulative ack of that entry's `seq`, dropping all pending entries with `seq ≤` it (this also covers packets the renderer skips because a snapshot already contained them). Untagged writes (exit banner etc.) pass through unchanged. An empty snapshot string is acked immediately (nothing to parse). Acking on delivery is **not** acceptable. Fail-closed: if the wrapper is not hit, no acks flow and the backpressure GUI test stalls. Phase 7's `TerminalView` passes write callbacks directly and deletes the adapter.
- Re-attach after webview reload replaces the channel and resends `Snapshot { pid, running, replay }`; a minimized island keeps the PTY (no detach on collapse).

### 3.5 OS integration services

Linux uses **one D-Bus stack (zbus)** for notifications, UPower, BlueZ, MPRIS: async, no libdbus C dependency, consistent reconnect logic. Each service implements:

```rust
#[async_trait::async_trait] // or native async fn in trait
pub trait Service: Send + 'static {
    const NAME: &'static str;
    async fn run(self, bus: Bus, shutdown: CancellationToken) -> Result<(), IslandError>;
    fn capabilities() -> ServiceCapabilities; // reported in bootstrap; drives UI hiding
}
```
Services restart with exponential backoff (1 s → 60 s) on bus disconnect and publish `available: false` while down. Unavailable values are `null`, never fabricated zeros (existing stats rule).

| Domain | Linux (nick-pc) | Windows | macOS | Cadence |
|---|---|---|---|---|
| Battery/power | zbus → `org.freedesktop.UPower` `/org/freedesktop/UPower/devices/DisplayDevice` `PropertiesChanged` (Percentage, State, TimeToEmpty/Full, IsPresent); `OnBattery` on the root object | `starship-battery 0.12` | `starship-battery 0.12` | Linux push; others 60 s poll |
| Battery alerts | `power/mod.rs` threshold engine: low (default 20%), critical (10%), full-while-charging (100%); 2% hysteresis; produces `SystemAlert` into NotificationStore | same | same | on change |
| Bluetooth | zbus → `org.bluez` `ObjectManager` (`InterfacesAdded/Removed`) + `org.bluez.Device1` `Connected`, `Name`, `Icon` + `org.bluez.Battery1.Percentage` | `DeviceWatcher` over `BluetoothDevice::GetDeviceSelectorFromConnectionStatus(Connected)` and BLE equivalent | `unsupported` initially | push |
| Now playing | zbus → `org.mpris.MediaPlayer2.*` names (`NameOwnerChanged`), `Player` `PropertiesChanged` (Metadata, PlaybackStatus); control via `PlayPause/Next/Previous`; preferred player = most recently `Playing` | `GlobalSystemMediaTransportControlsSessionManager` | `unsupported` (MediaRemote is private) | push; position interpolated in UI |
| Weather | `reqwest` → Open-Meteo `api.open-meteo.com/v1/forecast?latitude=..&longitude=..&current=temperature_2m,weather_code,is_day&temperature_unit=fahrenheit|celsius`; geocoding via `geocoding-api.open-meteo.com/v1/search?name=..`; no key needed; **no other provider and no stored secret** (revised after Phase 1). | same | same | every 15 min aligned to wall clock + on resume; 10 s timeout; last good value kept with `stale: true` |
| System metrics | port `island-rust/src/stats.rs` (`/proc`, `statfs`, no subprocess) | `sysinfo 0.40` | `sysinfo 0.40` | on demand while Monitor view visible (existing ~1.5 s UI cadence) |
| Palette | port island-rust palette watcher (directory notify + atomic-save debounce + serial 30 s fallback) | n/a (static default skin) | n/a | push |
| Displays | `app.available_monitors()` + Plasma anchor on Linux | same | same | on monitor change event |

Location default for weather: configured in Settings (`weatherLocation`), no IP geolocation. Nick's default should be set by the user, not hardcoded in code.

Camera/microphone indicators (upstream feature) are deferred: Linux requires PipeWire node inspection; spec it in a later revision rather than shelling out to `lsof` as upstream did.

---

## 4. Island state machine and frontend bridging

### 4.1 Ownership

Rust owns *what* mode/view the island is in, timers (hover delay, leave grace, toast TTL), and focus/click-through. The frontend owns *how* it animates, measures geometry, and renders. This removes today's split where the renderer holds hover timers but the native pill holds input.

### 4.2 Reducer (`state/machine.rs`, pure)

```rust
pub struct IslandState {
    pub rev: u32,
    pub mode: IslandMode,                 // still | hover | large
    pub view: IslandView,                 // terminal | monitor | ai | settings | notifications | clipboard
    pub hover_cause: Option<HoverCause>,  // pointer | panelPill | notification{id} | alert{id}
    pub dnd: bool,
    pub unread: u32,
    pub terminal_started: bool,
}
pub enum Timer { HoverDelay, LeaveGrace, ToastTtl(String) }
pub fn reduce(state: &IslandState, intent: Intent, settings: &Settings) -> (IslandState, Vec<Effect>)
pub enum Effect { Schedule(Timer, Duration), Cancel(Timer), RequestFocus, PlayClick, StartTerminal, EmitState }
```

Transitions (must reproduce `island/state.cjs` + `renderer.jsx` L428–474 exactly, then extend):

| From | Intent | To | Effects |
|---|---|---|---|
| still | `PointerEnter` / `PanelEnter` | still | if `hoverDelay > 0`: cancel LeaveGrace, schedule HoverDelay(hoverDelay ms). **hoverDelay == 0 means hover preview disabled**, not "instant". |
| still | `TimerFired(HoverDelay)` | hover(cause pointer/panelPill) | EmitState |
| hover | `PointerLeave` / `PanelLeave` | hover | schedule LeaveGrace(300 ms) |
| hover/still | `PointerEnter` during LeaveGrace | unchanged | cancel LeaveGrace |
| hover | `TimerFired(LeaveGrace)` | still | EmitState |
| still / hover | `Pin` (panel click, island click, tray "Open", shortcut) | large | cancel timers, RequestFocus, StartTerminal (first time), PlayClick if `clickSounds` |
| large | `PanelPin` (second native pill click) | still | cancel timers, PlayClick — PTY retained |
| any | `Collapse` (minus button, bare topline, Escape outside terminal) | still | cancel timers |
| large | `SelectView(v)` | large(view v) | — |
| still / hover | `SelectView(v)` | large(view v) | RequestFocus (tray "Settings…" uses this) |
| still | `NotificationArrived(id)` and `!dnd` and toast enabled | hover(cause notification{id}) | schedule ToastTtl(id, ttl) |
| hover(notification) | `PointerEnter` | hover(cause pointer) | cancel ToastTtl (user is reading it) |
| hover(notification) | `TimerFired(ToastTtl)` | still | — |
| large | `NotificationArrived` | large | unread += 1 (no mode change) |
| any | `SetDnd(b)` | dnd = b | — |

Timers are driven by the actor with a generation counter so a cancelled timer firing late is ignored. Unit tests use an injected clock; no real sleeping in reducer tests.

### 4.3 Commands (`#[tauri::command] #[specta::specta]`)

All return `Result<T, IslandError>`. Argument structs use `camelCase`. Legacy column shows the `window.island` method the Phase-2 shim maps onto it.

| Command | Args → Return | Legacy `window.island` |
|---|---|---|
| `island_bootstrap` | `()` → `BootstrapSnapshot` | `settings()`+`palette()` at mount |
| `island_dispatch` | `intent: UiIntent` → `IslandState` | internal dispatch (`enter/leave/pin/collapse`), tab switch |
| `island_report_geometry` | `report: GeometryReport` → `GeometryAck` | `layout(rect, mode, scale)` |
| `settings_get` | `()` → `Settings` | `settings()` |
| `settings_patch` | `patch: SettingsPatch` → `Settings` (atomic; any invalid key rejects all) | `setSettings(patch)`, `taskbar(show)` (becomes side effect of `showInTaskbar`) |
| `palette_get` | `()` → `Palette` | `palette()` |
| `terminal_attach` | `onEvent: Channel<TerminalEvent>` → `TerminalSnapshot` | `start()` + `onData` + `onExit` |
| `terminal_input` | `data: String` (≤256 KiB per call) → `()` | `input(data)` |
| `terminal_resize` | `cols: u16, rows: u16` → `()` | `resize(cols, rows)` |
| `terminal_restart` | `()` → `TerminalSnapshot` | `restart()` |
| `terminal_ack` | `seq: u32` → `()` | — (new flow control) |
| `system_stats` | `()` → `SystemStats` | `stats()` |
| `ai_usage` | `()` → `AiUsage` (cached 60 s, coalesced) | `aiUsage()` |
| `ai_connect` | `provider: AiProvider` (enum allow-list) → `()` | `aiConnect(provider)` |
| `voice_command` | `op: VoiceOp` (`status|toggle|...` existing allow-list) → `VoiceStatus` | `voice(op)` |
| `notifications_list` | `()` → `Vec<IslandNotification>` | — |
| `notifications_dismiss` | `id: String` → `()` | — |
| `notifications_clear` | `()` → `()` | — |
| `clipboard_list` | `query: Option<String>, limit: u16` → `Vec<ClipSummary>` | — |
| `clipboard_get` | `id: String` → `ClipContent` | — |
| `clipboard_copy` | `id: String` → `()` | — |
| `clipboard_pin` / `clipboard_delete` / `clipboard_clear` / `clipboard_set_paused` | … → `()` | — |
| `media_control` | `cmd: MediaCommand` → `()` | (upstream `controlSystemMedia`) |
| `weather_refresh` | `()` → `WeatherState` | — |
| `weather_geocode` | `query: String` → `Vec<GeoPlace>` | — |
| `displays_list` | `()` → `Vec<DisplayInfo>` | (upstream `getDisplays`) |
| `diagnostics_get` | `()` → `Diagnostics` (versions, capabilities, dropped counters, region readback) | — |

`UiIntent` is the frontend-allowed subset of `Intent` (`PointerEnter`, `PointerLeave`, `Pin`, `Collapse`, `SelectView{view}`, `SetDnd{on}`); panel, timer, and notification intents can only originate in Rust.

### 4.4 Geometry reporting contract (frontend → Rust)

- `GeometryReporter` owns one coalesced measurement per animation frame, triggered only by Motion `onUpdate`/`onAnimationComplete`, `ResizeObserver`, and settings changes affecting offsets/scale. **No perpetual rAF loop** (existing idle-performance rule).
- It measures the outer `.island` element (offsets included), not inner panes.
- Payload: `{ stateRev, mode, rects: [{x,y,width,height,radius}], scale: devicePixelRatio, settled }` in CSS px relative to the viewport. Rust converts to physical px.
- Rust replies `GeometryAck { applied: bool, rows: u16 }`; a rejected stale report is not an error.

### 4.5 Events (Rust → frontend), emitted with `emit_to("island", …)`

Defined with `#[derive(Serialize, Clone, specta::Type, tauri_specta::Event)]`; tauri-specta's `Event` trait provides `emit_to(handle, target)` (verified in rc.26 source).

| Event type | Payload | Kind | Notes |
|---|---|---|---|
| `IslandStateChanged` | `IslandState` | latest | only emitter: StateActor |
| `SettingsChanged` | `Settings` | latest | after successful patch from any origin (UI, tray) |
| `PaletteChanged` | `Palette` | latest | skin watcher |
| `NotificationReceived` | `IslandNotification` | queue | |
| `NotificationClosed` | `{ id, reason }` | queue | |
| `ClipboardAdded` | `ClipSummary` | queue | only when `clipboardHistory` on |
| `PowerChanged` | `PowerState` | latest | |
| `WeatherChanged` | `WeatherState` | latest | |
| `BluetoothChanged` | `Vec<BtDevice>` | latest | |
| `MediaChanged` | `Option<NowPlaying>` | latest | |
| `AnchorSignal` | `"enter" \| "leave" \| "pin"` | queue | **Phase 2–6 only**; feeds the legacy shim's `onAnchor` in Phase 2 only — from Phase 3 the renderer no longer consumes it (revised after Phase 3 review: the actor drives mode); removed in Phase 7 |
| `IslandCue` | `{ kind: "click" }` | queue | added in Phase 3 (revised after Phase 3 review): the actor's `PlayClick` effect |
| `ServiceStatus` | `{ service, available, detail }` | latest | |

Terminal output is **not** an event; it is the `terminal_attach` channel.

### 4.6 The Phase-2 compatibility shim (`ui/ipc/legacy-shim.ts`)

Defines `window.island` with the identical frozen shape of `island/preload.cjs` (`overlay`, `panelAnchor`, `onAnchor`, `layout`, `pin`, `palette`, `onPalette`, `start`, `restart`, `input`, `resize`, `voice`, `settings`, `setSettings`, `taskbar`, `stats`, `aiUsage`, `aiConnect`, `onData`, `onExit`) using generated bindings. Mapping details:
- `start()` calls `terminal_attach` once with a channel that fans out to `onData`/`onExit` subscribers and resolves with the snapshot shape `{ running, pid, data }` the renderer expects; later calls return the cached snapshot plus replay.
- `layout(rect, mode, scale)` → `island_report_geometry` with legacy mode names translated (`idle→still`, `preview→hover`, `pinned→large`). **Revised after Phase 1: this translation is the shim's only job; the Rust side no longer accepts the legacy names.**
- `onAnchor` listens to `AnchorSignal`.
- `overlay`/`panelAnchor` come from `BootstrapSnapshot.runtime`.
This lets the **unchanged** fork renderer run under Tauri in Phase 2 and makes every later phase compile-and-run testable. **Revised after Phase 3 review:** from Phase 3 the renderer carries exactly one pinned ownership amendment (§8 Phase 3, `renderer.ownership.patch`): mode/view come from the actor store and intents go out through `island_dispatch`; the shim keeps every other method.

---

## 5. Type-safe boundary contract

### 5.1 Rules

- Every IPC type lives in `src-tauri/src/contract/` and derives `Serialize, Deserialize (inputs only), Clone, Debug, specta::Type`.
- `#[serde(rename_all = "camelCase")]` on structs; enums `#[serde(rename_all = "camelCase")]` with `#[serde(tag = "type")]` for data-carrying variants.
- **No `u64`/`i64`/`usize` across the boundary**: specta-typescript forbids BigInt-sized integers by default. Use `u32` (wrapping revisions/sequence), `f64` for epoch milliseconds, `String` for IDs.
- Inputs use `#[serde(deny_unknown_fields)]`. Validation happens in Rust regardless of TS types.
- `Option<T>` fields that may be omitted get `#[serde(default)]` + `#[specta(optional)]`.
- `ui/ipc/bindings.ts` is generated in debug builds at startup *and* by `cargo test export_bindings`; `scripts/check-bindings.mjs` fails CI if the committed file differs.
- **Generator configuration (revised after Phase 1)** — `lib.rs::specta_builder()` sets `.disable_serde_phases()` and `.semantic_types(specta_typescript::semantic::Configuration::empty().enable_lossless_floats())`. Result: exactly one TS type per Rust type (no `_Serialize/_Deserialize` splits) and `f64` → `number` (not `number | null`; non-finite floats cannot cross the JSON IPC — serde_json writes them as `null` and Rust then rejects `null` for an `f64` — so `number` is the honest type). `Option<f64>` still correctly becomes `number | null`. The unit test `generated_types_are_clean` fails if `bindings.ts` ever contains `_Serialize`, `_Deserialize` or `number | null` (an intentional `Option<f64>` must then be allowed in that test explicitly), and `bindings_are_current` fails if it is stale. Do **not** reintroduce `#[serde(alias)]` or `#[serde(rename(serialize=…, deserialize=…))]` on contract types: they re-split the types.

### 5.2 Core types (Rust → generated TS)

```rust
// contract/island.rs
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum IslandMode { Still, Hover, Large }   // revised after Phase 1: NO serde aliases (see §0.3)

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum IslandView { Terminal, Monitor, Ai, Settings, Notifications, Clipboard }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum HoverCause { Pointer, PanelPill, Notification { id: String }, Alert { id: String } }

#[derive(Serialize, Clone, Debug, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct IslandState {
    pub rev: u32,
    pub mode: IslandMode,
    pub view: IslandView,
    pub hover_cause: Option<HoverCause>,
    pub dnd: bool,
    pub unread: u32,
    pub terminal_started: bool,
}

#[derive(Deserialize, Clone, Debug, specta::Type)]
#[serde(rename_all = "camelCase", tag = "type", deny_unknown_fields)]
pub enum UiIntent { PointerEnter, PointerLeave, Pin, Collapse, SelectView { view: IslandView }, SetDnd { on: bool } }

#[derive(Deserialize, Clone, Copy, Debug, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoundedRect { pub x: f64, pub y: f64, pub width: f64, pub height: f64, pub radius: f64 }

#[derive(Deserialize, Clone, Debug, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeometryReport { pub state_rev: u32, pub mode: IslandMode, pub rects: Vec<RoundedRect>, pub scale: f64, pub settled: bool }
```

Expected generated TS (shape, not exact formatting; verified identical in kind after the Phase 1.5 generator fix):

```ts
export type IslandMode = "still" | "hover" | "large";
export type IslandView = "terminal" | "monitor" | "ai" | "settings" | "notifications" | "clipboard";
export type HoverCause = { type: "pointer" } | { type: "panelPill" } | { type: "notification"; id: string } | { type: "alert"; id: string };
export type IslandState = { rev: number; mode: IslandMode; view: IslandView; hoverCause: HoverCause | null; dnd: boolean; unread: number; terminalStarted: boolean };
export type UiIntent = { type: "pointerEnter" } | { type: "pointerLeave" } | { type: "pin" } | { type: "collapse" } | { type: "selectView"; view: IslandView } | { type: "setDnd"; on: boolean };
```

```rust
// contract/settings.rs — existing 16 keys keep identical names, types, ranges, defaults (island/settings.cjs)
pub struct Settings {
    pub autostart: bool,             // false
    pub show_in_taskbar: bool,       // false
    pub stealth: bool,               // false
    pub click_sounds: bool,          // false
    pub hover_delay: f64,            // 0..=1200, 180; 0 disables hover preview
    pub offset_x: f64,               // -400..=400, 0
    pub offset_y: f64,               // -200..=200, 0
    pub accent: String,              // "" | #rrggbb  ("" = follow Hermes skin)
    pub background: String,          // "" | #rrggbb
    pub border_enabled: bool,        // true
    pub border_color: String,        // "" | #rrggbb
    pub panel_clock_color: String,   // "" | #rrggbb  — pill BACKGROUND, not digits
    pub ui_font: UiFont,             // "system" (default) | "openrunde"
    pub show_date: bool,             // true
    pub hardware_monitor: bool,      // true
    pub terminal_font_size: f64,     // 8..=24, 13
    // ---- new keys (ignored by legacy loaders, which only read known keys) ----
    pub notifications_enabled: bool, // true  (Hermes ingest + system alerts)
    pub system_notification_mirror: bool, // false (D-Bus monitor opt-in)
    pub toast_ttl_ms: f64,           // 1500..=20000, 6000
    pub redact_bodies: bool,         // false
    pub dnd: bool,                   // false
    pub clipboard_history: bool,     // false (privacy opt-in)
    pub clipboard_history_size: f64, // 10..=500, 100
    pub clipboard_persist: bool,     // false
    pub battery_alerts: bool,        // true
    pub battery_low_percent: f64,    // 5..=50, 20
    pub battery_critical_percent: f64, // 1..=30, 10
    pub weather_enabled: bool,       // false until a location is set
    pub weather_location: Option<GeoPlace>, // null
    pub weather_unit: TempUnit,      // "fahrenheit" | "celsius"
    pub hour_format: HourFormat,     // "h12" | "h24"
    pub display_id: Option<String>,  // null = primary / Plasma anchor
}
```
`SettingsPatch` mirrors this with every field `Option<_>`; the validation table is a port of `island-rust/src/services.rs::validate` (numbers clamped, strings pattern-checked with "" allowed, unknown keys rejected, whole patch atomic, file written via temp + rename). Numbers stay `f64` to preserve the existing JSON exactly.

Remaining contract files (field lists are the implementer's to complete under the rules above):
- `terminal.rs`: `TerminalSnapshot { pid: Option<u32>, running: bool, data: String, seq: u32 }`, `TerminalEvent = Data{seq,data} | Exit{code: Option<i32>, signal: Option<String>} | Snapshot(TerminalSnapshot)`.
- `notify.rs`: `IslandNotification { id, source: NotificationSource (system|hermes|alert), appName, title, body: Option, iconPng: Option<String /* data URL ≤ 64 KiB */>, urgency: low|normal|critical, receivedAt: f64, actions: Vec<{key,label}>, canInvokeActions: bool, progress: Option<f64> }`.
- `clipboard.rs`: `ClipSummary { id, kind: text|image|files, preview, bytes: u32, pinned, copiedAt: f64, sourceApp: Option }`, `ClipContent = Text{text} | Image{pngDataUrl,width,height} | Files{uris}`.
- `system.rs`: `PowerState`, `WeatherState { available, stale, tempC: Option<f64>, code: Option<u16>, isDay, place: Option<GeoPlace>, updatedAt }`, `BtDevice { address, name, icon, connected, batteryPercent: Option<u8> }`, `NowPlaying`, `SystemStats` (exact current JSON shape of `stats.rs` with nulls preserved), `DisplayInfo`, `ServiceCapabilities`.
- `ai.rs`: `AiUsage` (wraps `hermes usage --json` passthrough as `serde_json::Value` **only** for the provider-specific detail object; top-level fields typed), `AiProvider` enum allow-list from `services.rs::auth_args`.

### 5.3 Bootstrap snapshot

```rust
pub struct BootstrapSnapshot {
    pub runtime: RuntimeInfo,          // overlay, panelAnchor, platform, version, testInstance
    pub state: IslandState,
    pub settings: Settings,
    pub palette: Palette,
    pub capabilities: ServiceCapabilities,
    pub power: Option<PowerState>,
    pub weather: Option<WeatherState>,
    pub media: Option<NowPlaying>,
    pub bluetooth: Vec<BtDevice>,
    pub notifications: Vec<IslandNotification>, // most recent 50
}
```

### 5.4 Error contract

```rust
#[derive(Serialize, Clone, Debug, specta::Type, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{code:?}: {message}")]
pub struct IslandError { pub code: ErrorCode, pub message: String }
#[derive(Serialize, Clone, Copy, Debug, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode { Invalid, Unsupported, Unavailable, Busy, Io, Timeout, Internal }
```
Messages are user-presentable; never include file contents, tokens, or full command lines.

### 5.5 Registration (`lib.rs`)

```rust
let specta = tauri_specta::Builder::<tauri::Wry>::new()
    .commands(tauri_specta::collect_commands![ /* every command in §4.3 */ ])
    .events(tauri_specta::collect_events![ /* every event in §4.5 */ ]);
#[cfg(debug_assertions)]
specta.export(specta_typescript::Typescript::default(), "../ui/ipc/bindings.ts")?;
tauri::Builder::default()
    .invoke_handler(specta.invoke_handler())
    .setup(move |app| { specta.mount_events(app); setup::run(app) })
    .run(tauri::generate_context!())?;
```
`capabilities/island.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "island-main",
  "windows": ["island"],
  "permissions": ["core:event:default", "island-commands"]
}
```
`permissions/island.toml` defines set `island-commands` = every `allow-<command>` from §4.3. Start with only `core:event:default`; add a core permission only when a concrete call fails, and record why.

### 5.6 Swap-out plan if tauri-specta is abandoned

`contract/` types additionally derive `ts_rs::TS` behind a cargo feature `ts-rs`; `ui/ipc/client.ts` is the only TS module importing `bindings.ts`, so replacing the generator touches one file. **Revised after Phase 1: not triggered.** Decision recorded in Phase 1.5: tauri-specta/specta rc.26 stays, because `disable_serde_phases()` + lossless-floats semantic config (§5.1) produce clean types; the ts-rs swap remains the escape hatch only if a later specta bump regresses `generated_types_are_clean`.

---

## 6. Frontend structure

- Phase 2 copies `island/renderer.jsx`, `monitor.jsx`, `ai-stats.jsx`, `state.cjs`, `palette.cjs`, `click-sound.cjs`, `style.css` into `ui/legacy/` **byte-for-byte** (revised after Phase 3 review: `renderer.jsx` alone then carries the pinned ownership patch from §8 Phase 3; all other copies stay byte-identical) and boots them with `legacy-shim.ts` installed first. Visual parity is the acceptance criterion; do not "clean up" CSS.
- Phase 7 converts to TypeScript one file at a time behind the same DOM contract (`data-state`, `data-view`, `data-testid="view-*"`, `aria-label="UI font"`, `.island`, `.terminal-wrap`) so the GUI smoke keeps working. `data-state` switches to canonical names in the same commit that updates the smoke tests.
- Component tree target: `Island` (reads `useIslandState()`, sends `dispatch`) → `IslandSurface` (Motion springs, `UI_SPRING = {stiffness:350, damping:32, mass:0.8}` and existing widths/radii: still 150/20, hover 420/28, large per-view/24) → `Topline`, `ViewPager` (swipe/wheel/arrow navigation with existing guards), views, `Toast`.
- Stores: one `useSyncExternalStore` store per topic, each guarded by `rev`; bootstrap fills all stores before first render to avoid a flash of defaults.
- No polling for appearance. Clock aligns to wall-clock minute (existing). Monitor polls `system_stats` only while visible, one request in flight.

---

## 7. Plasma integration (Linux, retained)

- QML pill (`island/plasma-anchor`, `local.hermes.island.v3`) and `anchor-client.py` are unchanged in Phases 1–8. Tauri host serves the same `panel.sock` (0600, bounded framing, ops `status|enter|leave|pin`) via `integrations/plasma/panel_socket.rs` ported from `host.rs` including its framing tests. `status` reply and `status.json` keep their current fields (`pid, updated, mode` **in legacy names**, `anchor, bounds, windowId, rectangles, pty`) because external tooling and tests read them.
- `enter|leave|pin` become `Intent::PanelEnter|PanelLeave|PanelPin` to the actor (and, during Phases 2–6, also `AnchorSignal` events for the shim).
- Appearance push to QML (removing the 15 s request/reply timer) is a **protocol change** requiring a QML consumer change; it is Phase 8 optional work (`subscribe` op returning a long-lived stream), never smuggled into the port.
- `anchor-client.py` and the autostart entry select the Tauri binary only in Phase 9, after acceptance.

---

## 8. Phased roadmap with compile-ready gates

Each phase ends with a green build, tests, and a short `island-tauri/VERIFICATION.md` entry describing what real execution proved and what remains unproven.

### Phase 0 — Safety and toolchain (no app code)
- Verified recovery archive under `/home/nick/Hermes/Backups/Ripple/pre-tauri-<timestamp>/` (tracked + untracked source, `.git`, settings; exclude `node_modules`, `target`, sockets/locks); open it, check membership, record SHA-256 (ripple-island skill gate).
- `cargo install tauri-cli --version "^2.12" --locked`; `npm i -D @tauri-apps/cli@^2` in `island-tauri/`.
- Confirm `libayatana-appindicator` (tray) and `xvfb-run`, ImageMagick `import`, Pillow for GUI tests.
- **Gate:** archive verified; `cargo tauri --version` prints 2.12.x.

### Phase 1 — Skeleton + window + input region (the riskiest seam first)
Files: `Cargo.toml`, `build.rs`, `tauri.conf.json` (+ platform files), `main.rs`, `lib.rs`, `cli.rs`, `paths.rs`, `error.rs`, `window/{mod,placement,shape}.rs`, `window/input_region/{mod,linux}.rs`, minimal `ui/main.tsx` that draws three fixed rounded rects (still/hover/large) and a debug key to cycle modes.
- Port `shape_rows` + tests verbatim.
- Integration test under Xvfb: launch with `--test-instance --data-dir $TMP`, cycle modes, read back ShapeInput via `x11rb`, assert empty in still(native-pill mode) and rounded rows matching the rect otherwise; screenshot shows transparent pixels outside the rect (check known WebKitGTK transparent-repaint issue tauri#12800 on this WebKit 2.54.2; if stale frames appear, add the minimal workaround and a test).
- **Gate:** clippy/fmt/test green; Xvfb shape readback test passes; window never focused on show.

### Phase 1.5 — KWin validation (inserted after Phase 1)
Scope: repeat the Phase 1 GUI assertions on a nested, headless `kwin_wayland --virtual --xwayland` (private `XDG_RUNTIME_DIR`/`HOME`/D-Bus, never the user's session) and apply the four review decisions (no serde aliases; clean generated TS; no `keyring`/single zbus; Vite ≥ 6).
- `tests/kwin_shape.rs` (+ `tests/common`, `tests/helpers/eis_pointer.py`): window maps with the frozen identity, at the requested position, undecorated, not focused on show; still (native pill) passes physical clicks through; hover/large accept clicks inside the rounded region and pass them through outside it **including the KWin frame area**; hover never takes focus.
- If KWin ever breaks one of these: stop, report evidence and the smallest fix options; do not redesign.
- **Gate:** Phase 1 gate still passes; `kwin_shape` passes; `npm audit` clean; bindings regenerated byte-identical and checked (`scripts/phase1_5-gate.sh`).

### Phase 2 — Contract + bindings + legacy renderer running on Tauri
Files: `contract/*`, `commands/{island,settings,terminal,system,ai,voice}.rs`, `services/{settings,palette,terminal/*,stats/linux_proc,ai_usage,voice}.rs` (ports), `bus.rs`, `ui/ipc/*`, `ui/legacy/*` (copies), `scripts/check-bindings.mjs`.
- Terminal via Channel + credit flow; fixture PTY in tests.
- **Gate:** `cargo test` includes ported terminal/services tests (≥ the 30 island-rust tests' coverage); bindings check passes; `tests/gui_smoke.py` (ported) passes in preview and `--overlay-test`: four views, PTY round-trip, same PID + unsubmitted draft after minimize/reopen, OpenRunde loads, invalid settings rejected, body height == viewport in overlay.
- **Contract completeness (revised after Phase 2 review).** Phase 2 owns the whole wire contract so later phases never churn bindings: the full §5.2 `Settings` (every key with default + validation; keys for unbuilt features are stored and validated but inert), the full §5.3 `BootstrapSnapshot` (`capabilities` reports unbuilt services as unavailable; `power/weather/media` `None`, `bluetooth/notifications` empty; `state` from the current `IslandState`), `TerminalEvent::Snapshot` with re-attach after a webview reload, and `terminal_input` capped at **256 KiB of UTF-8 per call** (§4.3) with a typed error — this replaces island-rust's 65536-UTF-16-unit cap; port that test as a 256 KiB boundary test (exactly 256 KiB accepted, +1 byte rejected). The shim does not split oversized input; the renderer's existing `.catch(onError)` surfaces it.
- **Earlier-gate amendment (revised after Phase 2 review).** `scripts/phase1_5-gate.sh` no longer greps for every `number | null` (it was written against the Phase 1 contract and rejected SPEC-required `Option` fields such as `SystemStats` metrics and the terminal `pid`). Split types and `bigint` are still grepped; nullability is enforced field-by-field by `generated_types_are_clean`, whose allowlist each new legitimate `Option` field must be added to with its SPEC reference. Plain `f64` must still emit `number`. This is the only permitted edit to earlier gate scripts.
- **Parse-ack evidence (revised after Phase 2 review):** `node --test` unit tests of `xterm-ack-adapter.ts` with a fake `Terminal` whose write callback fires later (no ack before the callback; cumulative ack; skipped-by-snapshot packets dropped; untagged writes untouched; empty snapshot acked), plus a nested-KWin GUI flood test: the fixture emits ≥ 8 MiB ending in a sentinel, a test hook reports Rust's peak unacknowledged bytes, assert peak ≤ 1 MiB + 64 KiB, PTY reading resumed, and the sentinel is in xterm's buffer. Mutation: switching the adapter to ack-on-delivery must fail the unit tests.

### Phase 2.5 — Panel-overlap morph spike (inserted after Phase 2 review; §0.5)
Isolated experiment on nested KWin only; no product code changes beyond a test-only flag. Questions to answer with evidence: (a) can the island window (TOOLBAR, and if needed `_NET_WM_STATE_ABOVE` or another window type — record each tried) be stacked and drawn above a dock-type panel window at the pill's position under KWin 6.8 XWayland, while its input region stays empty there so the panel (stand-in for the Plasma pill) still receives hover/click; (b) does a spring morph from a pill-sized rounded rect at panel height to the hover size render without tearing or a visible seam (compositor capture frames); (c) `_KDE_NET_WM_BLUR_BEHIND_REGION` applied from the input-region owner blurs only the island shape; (d) WebKitGTK frame pacing during that morph (rAF p50/p95/max, dropped frames). Deliver a short report in VERIFICATION.md with screenshots and a recommendation; the live desktop is never used (Phase 9 rule).
- **Result (accepted after controller re-run, 2026-10-29):** (a) TOOLBAR+ABOVE draws over an X11 dock with the pill still receiving real hover/click through the empty input region, but a *focus-accepting* dock that gets clicked is raised over the island and buries it; NOTIFICATION type stays above in every case, still takes no focus on map, and receives clicks/keys inside its region. (b) Morph from the exact pill rect is seamless and untorn (27 captured frames per direction). (c) `_KDE_NET_WM_BLUR_BEHIND_REGION` blurs only the shape; an EMPTY property blurs the whole window, so delete it for an empty region; drive it from the painted shape. (d) Nested KWin (GLES on the real GPU, virtual 60 Hz output): rAF p50/p95/max 16/17/18 ms, 0 frames > 20 ms with per-frame geometry reports. UNVERIFIED: the real Plasma panel is a Wayland layer-shell surface, not an X11 dock, so stacking/burial against it, fullscreen apps, HiDPI and multi-monitor are untested.
- **Real-panel check (Nick-approved, isolated test instance on the live session, 2026-10-29):** against the real Plasma panel, TOOLBAR+ABOVE is *always* drawn under the panel (0 spike pixels over the pill at start, hover and after a pill click); NOTIFICATION is drawn over it in every state (4010/4010 expected pixels), while the real pill still received hover (live island → preview) and click (→ pinned) through the empty input region. **Decision: Phase 3 switches the island window to NOTIFICATION + keep-above** and updates `identity.rs` and every TOOLBAR assertion in the same change. Still UNVERIFIED: fullscreen apps, HiDPI, multi-monitor placement.

### Phase 3 — State actor, Plasma bridge, placement, singleton, status
Files: `state/{machine,actor}.rs`, `singleton.rs`, `integrations/plasma/*`, `window/input_region` wiring to actor.
- **Activities (added after Phase 2 review, §0.5):** the reducer also tracks live activities (`id`, `source`, `priority`, compact/expanded availability, start time); the highest-priority activity owns the compact presentation, a second one produces the split state; tests cover priority ties, expiry and split/merge transitions with the injected clock. Contract additions are additive.
- **Carried over from Phase 2 (added after Phase 2 review):** (1) the actor becomes the authority for `IslandState`, so legacy tab/view changes and preview-mode state reach Rust and `island_bootstrap().state` reflects them — Phase 2 deliberately ships only the complete wire shape; (2) port these island-rust host tests into the Phase 3 gate by name: `instance_lock_excludes_second_owner_without_unlinking_socket`, `live_socket_without_our_lock_is_never_unlinked`, `live_legacy_status_without_socket_blocks_rust_and_preserves_status`, `cleanup_never_removes_another_instances_status`, `native_panel_roundtrip_emits_only_allowed_events`, `anchor_prefers_native_widget_on_primary_top_panel`, `panel_protocol_is_bounded_and_requires_a_complete_command`, plus any remaining row VERIFICATION.md's Phase 2 mapping marks "Phase 3".
- Reducer unit tests for every row of §4.2 with injected clock.
- (The Phase-1 "repeat against nested KWin" item moved to Phase 1.5 and is done.)
- **Renderer ownership seam (revised after Phase 3 review).** The first Phase 3 run correctly stopped: actor authority (above) contradicted the unchanged-renderer rule (§4.6/§6), because `ui/legacy/renderer.jsx` owns mode (`useReducer`), view (`useState`), hover/leave timers and state-mutating `onAnchor` handlers. A DOM observer would only mirror; that is rejected, and so is a hidden build-time transform. Decision: **Phase 3 pulls Phase 7's "drive mode from `IslandStateChanged`, delete renderer-side hover timers" forward as one explicit, audited amendment of `ui/legacy/renderer.jsx` only.**
  - New `ui/ipc/island-state.ts`: one `useSyncExternalStore` store seeded from `island_bootstrap().state` before first render, updated only by `IslandStateChanged`, rev-guarded (older `rev` dropped); exports `useIslandState()` and `dispatch(intent: UiIntent)` (wraps `island_dispatch`). Unit-tested with `node --test` (seed, rev guard, dispatch error surfaces through the renderer's existing `fail`).
  - Allowed renderer edits, nothing else: mode/view/`started` read from that store (canonical → legacy names via the existing `ui/ipc/legacy-mode.ts`, so `data-state` stays `idle|preview|pinned` until Phase 7); `enter`/`leave` → `PointerEnter`/`PointerLeave`; `pin`, island click → `Pin`; minus/topline/Escape → `Collapse`; tabs, arrows, wheel, swipe and `navigate` → `SelectView{view}` computed from the store's view (`direction` may stay local, it is animation-only); delete `hoverTimer`/`leaveTimer`/`cancelHoverTimers` and the whole state-mutating `onAnchor` subscription (panel enter/leave/pin now go panel socket → actor directly); `api.layout` reports carry the store's `rev` as `stateRev`. DOM structure, classes, data attributes, CSS, Motion springs, xterm wiring and every settings control stay unchanged. Every other `ui/legacy/*` file stays byte-identical to `island/`.
  - The amendment is pinned as `ui/legacy/renderer.ownership.patch` = `diff -u island/renderer.jsx ui/legacy/renderer.jsx` (repo-relative labels). **Earlier-gate amendment (only permitted edit):** in `scripts/phase2-gate.sh` and `scripts/phase2_5-gate.sh` the `cmp` of `renderer.jsx` is replaced by a check that the live diff equals that patch file byte-for-byte; every other file keeps its `cmp`. Nothing else in earlier gates/mutation scripts changes.
  - Effects reach the window from Rust: `RequestFocus` → the input-region/window owner focuses the island (panel pin must give xterm keyboard focus); `PlayClick` → additive event `IslandCue { kind: "click" }` the renderer plays through the existing `click-sound.cjs` (contract additions are additive; regenerate bindings, `check-bindings` must pass). `AnchorSignal` is still emitted but no longer consumed by the renderer; its removal stays in Phase 7.
  - Geometry: the Phase 2 mirror in `commands/island.rs` is replaced. Rust accepts a `GeometryReport` only when `stateRev` equals the actor's current `rev` (otherwise `applied:false`, not an error); several reports per rev are accepted (latest wins — layout-only changes such as view switches inside `large` or offset/scale settings re-report at the same rev); `still` always yields an empty input region regardless of reports.
- **Hover sources (revised after Phase 3 review; refines §4.2).** The real-execution failure (`evidence/phase3/legacy-hover-failure.log`: panel `enter` → preview, then a DOM `mouseleave` the pointer never "entered" collapsed it via the 300 ms timer) is fixed in the reducer, not the renderer: the actor tracks `pointer_inside` and `panel_inside` separately; a `PointerLeave`/`PanelLeave` for a source that is not inside is a no-op; `LeaveGrace` is scheduled only when, after a leave, neither source is inside; any enter during grace cancels it. Reducer tests cover the spurious-leave case; the hover GUI test reproduces the original failure sequence and must pass.
- **Status file and stale artifacts (revised after Phase 3 review).** `status.json` stays a regular 0600 file, written by temp-file-in-same-dir + `rename` while both locks are held (legacy readers, rollback tools and crash recovery expect a regular file; the locks are the ownership contract). The symlink-to-private-dir design is withdrawn. With both locks held, stale artifacts are reclaimed only when the status PID is not alive AND the socket path is a socket owned by our UID whose `connect` fails with `ECONNREFUSED`; anything live, foreign-owned, non-socket or unverifiable is still refused untouched. Test both reclaim and every refusal case.
- **Already passed in the first Phase 3 run (audited by the controller, keep unchanged unless a revision above requires it):** NOTIFICATION + keep-above identity, dual-lock singleton and coexistence refusals, panel socket bounds, anchor parsing, SIGTERM cleanup, the 8 ported host tests, reducer/actor/activities with injected clock, six mutations. UNVERIFIED and deferred to Phase 8: the KWin scripting placement fallback. Controller audit also requires: a reducer test that a current-generation `TimerFired` before its deadline is ignored; a panel-socket test that client `MAX_CLIENTS + 1` waits for a free slot; and a bounded settle-wait before the start-pixel check in `tests/kwin_spike_panel.rs` (flaked 1/5). That wait is the only permitted edit to a Phase 2.5 test.
- **Gate:** against an isolated test instance, real `anchor-client.py` `enter/leave/pin` drive hover/still/large with matching shape readback; second instance refused while first live; refusal also when a legacy Electron status/socket is live; SIGTERM leaves no socket/status/PTY behind. **Added after Phase 3 review:** a real EIS click on each legacy tab (and one arrow-key and one wheel navigation) changes the actor's view, visible in `island_bootstrap().state` and `status.json`; the original hover-failure sequence now stays in preview until a real leave + 300 ms grace, then idle with empty ShapeInput; after a panel pin, a typed unsubmitted `x` reaches xterm (then Ctrl+U), same PTY PID after minimize/reopen; the renderer patch check passes; settled screenshots of preview and all four views match the Phase 2 evidence (differences explained, not ignored); a stale-artifact reclaim and each refusal case run as real processes. Mutations (in addition to the six kept): renderer tab click calling a local `setView` instead of `SelectView`, reintroducing a renderer leave timer, accepting a stale-`rev` geometry report, scheduling `LeaveGrace` on a spurious leave, reclaiming when the socket accepts — each must fail a named test.
- **Accepted after Phase 3b audit (controller, Oct 30).** Clean gates 1→3 and all 13 Phase 3 mutations re-run green by the controller; 5/6 controller mutations killed (6th equivalent: dropping the `ToastTtl` cancel on enter-during-hover is harmless because the enter overwrites `hover_cause` and the TTL only collapses a `Notification` cause). **Controller amendment:** `scripts/phase3-audit.py` discovers the live island by `/proc/PID/exe` instead of a hard-coded PID (the old PID died in a reboot); this is the only edit to that script. Carried into Phase 4: (1) **real pointer exit.** In nested KWin a real EIS exit from the shaped XWayland window produced no DOM `mouseleave`, so the hover GUI test feeds a synthetic `mouseout`. Phase 4 must make pointer leave independent of WebKit: the window owner turns GTK `leave-notify-event` (ignore `NotifyInferior`) / `enter-notify-event` into `PointerLeave`/`PointerEnter` intents (DOM handlers may stay; the actor's per-source tracking makes duplicates harmless), and the hover GUI test must collapse after a REAL EIS exit with no synthetic event. If X itself sends no LeaveNotify, record the xev/xinput evidence and stop. (2) Reclaim stale `.status.json.<pid>.<n>.tmp` files whose PID is dead, under both locks, with a test.

- **Controller audit of the partial Phase 4 run (Oct 30, revised after Phase 4 part 1).** Sol's one failure of `kwin_phase3_actor` (timeout at `tests/common/mod.rs:103`, 12.3 s, label filtered by the gate) did NOT reproduce in 7 controller runs (5 standalone unfiltered + 2 inside `CLEAN=1 phase3-gate.sh`, all ~31 s). The second controller gate run passed in full; the first failed only on screenshot drift (`overlay-ai-argb.png` 5.56 % vs 1 % threshold while its composited twin differed 0.07 %; the next run's worst share was 0.85 %). Both are treated as timing races, not Phase 4 regressions. Permitted edits, the only ones to earlier phases' files: (a) `src-tauri/tests/kwin_gui.rs` view loop: before each `screenshot`, a bounded settle-wait (≤ 5 s) that the `.view-pane` innerText and the argb capture are unchanged across two consecutive samples ≥ 250 ms apart — same assertions, no retry of the comparison, no threshold change; (b) `scripts/phase3-gate.sh` may also print `timed out after … waiting for:` lines from failing GUI tests (diagnostic only; nothing else in that script changes). If `kwin_phase3_actor` fails again, the printed wait label decides the fix (a bounded settle-wait on that same condition, recorded here).
- **Staged pure cores added for Phases 6b/7 (controller-audited with own mutations, Oct 30):** `staging/timer-core`, `scratchpad-core`, `downloads-core`, `bluetooth-core`, `layout-core` (side-button Layout editor model + geometry; known gap: with a short island the left/right columns may overlap the bottom row — Phase 7 must clamp or stack), `launcher-core` (Desktop Entry parsing + Exec argv per spec §7), `ai-dashboard-core` (AI-stats card aggregation), `mixer-core` (pactl JSON/subscribe parsing + argv builders; never runs pactl), `ics-core` (RFC 5545 subset + agenda expansion; Calendar backend still to be chosen in Phase 6b), `brightness-core` (KDE PowerDevil `org.kde.ScreenBrightness` display model, percent↔raw, 50 ms drag throttle + 500 ms settle, SetBrightnessWithContext/AdjustBrightnessStepWithContext requests with SuppressIndicator and context `hermes-island`; settle uses the last injected `now_ms`, so the Tauri side must call `poll` at `next_wakeup`; port with saturating counters), `launcher-arrange-core` (launcher jiggle/edit session: visible-index moves, locked tiles never disabled, re-enabled tile goes last, cancel restores the snapshot, commit = full-permutation `order` + `enabledChanges` diff for module-registry `reorder`/`set_enabled`, reconcile applied to the snapshot too; 55 tests), `quick-toggles-core` (power-profiles model + cycle, strict `pactl` mic-mute parse/argv on `@DEFAULT_SOURCE@`, generic optimistic `ToggleState` with request ids and a 3000 ms pending timeout; 43 tests), `updates-core` (read-only `checkupdates` / `paru -Qua` parsing with exit-code semantics, `[ignored]` marker, kernel/driver needs-reboot flag, unknown-never-zero badge, hourly schedule with 10→60 min failure backoff and battery hold; argv never contains -S/-U/sudo/--noconfirm; 59 tests), `system-tiles-core` (keep-awake ScreenSaver inhibit state machine that never leaks a late cookie + timed auto-off; `pactl -f json list sinks` output switch with safe `set-default-sink` argv; NetworkManager State/Connectivity tile with Wi-Fi toggle only when a wireless device exists; `tailscale status --json` peers online-first; KWin ColorPicker ARGB → hex; `hermes-fish` voice list/current/switch with reserved words refused; 66 tests; port note: a peer missing `Online` currently becomes `false` — make it `None`/unknown when porting), `agent-tracker-core` (Hermes `state.db` session rows → running-agent chips with subagents folded into a running parent, Started/Finished/Stalled/Disappeared events without startup spam; running = no `endedAt` AND recent `lastActivityAt` (orphan rows exist); 300 s clock-skew guard; lookback 24 h; source `a2a` is not a chip; 46 tests). The integrating phase ports them; it does not rewrite them.

### Phase 4 — Notification hub + tray
Files: `integrations/notifications/*`, `tray.rs`, `commands/notifications.rs`, `cli.rs notify`, `widgets/Toast.tsx` (rendered by legacy renderer via a small mount point until Phase 7).
- **Gate:** `hermes-island notify` → toast in still mode, auto-dismiss after TTL, pointer-enter cancels TTL, DND suppresses toast but stores item; ingest socket rejects oversize/rate-exceeded/other-UID; D-Bus monitor test under `dbus-run-session` with a mock server and `notify-send`; loop guard proven; tray actions dispatch intents.
- **Accepted after Phase 4b audit (controller, Oct 30).** Controller re-ran CLEAN gates 1→3 and `scripts/phase4-gate.sh` in full, serialized, without any test runner override: all PASS (worst screenshot share 0.86 %; 13 Phase 4 + 13 Phase 3 mutations killed inside the gate; monitor 8/8). Earlier-file edits = only the two permitted ones. `scripts/phase4-test-capture.py` is an OPTIONAL diagnostic `CARGO_TARGET_*_RUNNER` (tees raw `kwin_phase3_actor` output, forwards argv/exit code, no retry); gates must never depend on it. Controller mutations: owner change without `hub.reset_server`, expiry without `correlation.forget`, repair snapshot `rev: 0` → killed by named tests; **two survived = test gaps carried into Phase 5:** (1) `Decoder::observe` accepts `NotificationClosed`/`ActionInvoked` signals from a NON-owner sender when the `sender == owner` check is removed (the BecomeMonitor signal rules have no sender filter, so this check is the only guard against a spoofed close); (2) a `Notify` method call whose destination is neither `org.freedesktop.Notifications` nor the current owner is correlated when the destination check is removed (pollutes the 256-entry pending LRU). Each needs a named decoder test (observable via `Observed::Ignored` and `expire(now+6 s) == 0`). Open risk: Sol reproduced the 12.4 s `kwin_phase3_actor` timeout once in a CLEAN chain with the label filtered; phase3-gate now prints labels, so the next failure names its wait — then add a bounded settle-wait on that condition only (recorded here).

### Phase 5 — Clipboard history
Files: `integrations/clipboard/*`, `commands/clipboard.rs`.
- `history.rs` unit tests (dedupe, caps, pin survival, sensitive skip, self-echo).
- Wayland watcher test needs a nested compositor (e.g. headless `kwin_wayland --virtual` or `weston --backend=headless`) — never the user's session clipboard.
- **Gate:** watcher sees `wl-copy` text and image in the nested session; default-off verified.
- **Accepted after Phase 5 audit (controller, Oct 30).** Controller re-ran `CLEAN=1 scripts/phase5-gate.sh` (full chain: Phase 3 mutations, CLEAN 1→3, phase4, phase5) inside a 14G scope with no runner override: PASS in 52 min (344 tests, 0 failed; 10 Phase 5 mutations killed; worst screenshot share 0.81 %). `wl-copy` is not installed on nick-pc, so the real Wayland source is the scratch-only `tests/helpers/clipboard_source.rs` (wl-clipboard-rs). Earlier-file edits = the two decoder tests carried from Phase 4 (both carried-over mutations now killed) and ONE permitted bounded settle-wait in `kwin_phase3_actor.rs` (waits ≤ 5 s for the `[data-state]` DOM rect AND native ShapeInput to be stable for 250 ms before the single real EIS entry; the old code aimed at a centre sampled before the second opening). This closes the Phase 3/4 actor-timeout open risk unless it recurs. Controller mutations: hint read skipped → killed; pause-epoch cancel in `progress()` removed → equivalent (`Shared::offer` re-checks `active(epoch)` before and under the lock; only the in-flight read keeps running). **Three survived = test gaps carried into Phase 6 as first items:** (1) an offer advertising `org.nspasteboard.ConcealedType` / `TransientType` / `ExcludeClipboardContentFromMonitorProcessing` must be skipped (renaming the match arm survives); (2) a text/PNG read over its cap must be dropped (`cap * 8` survives); (3) `persistence.rs` load must reject a file with group/other permission bits (`& 0o000` survives). Each needs a named test (real nested source for 1–2, temp file chmod 0644 for 3).

### Phase 6 — Widgets: power, weather, Bluetooth, media
- **Tiered gates (Nick, Oct 31; applies from Phase 6m on, after the 6a audit).** Per phase (agent and controller audit): the QUICK tier = incremental build (no CLEAN), fmt, clippy, the whole `cargo test` suite (it already contains every earlier GUI test binary; any `#[ignore]`d GUI test that an earlier gate runs explicitly must be listed and run too), bindings/typecheck/npm audit, the current phase's mutations, the screenshot comparison and the before/after sandbox audits. Earlier phases' mutation scripts and the CLEAN rebuild chain are NOT re-run per phase. The FULL tier (`CLEAN=1` chain through every earlier gate and every earlier mutation script) runs only at milestones: after 6b, after 7, and before Phase 9, and whenever a QUICK run touches an earlier phase's file. New gate scripts take `TIER=quick|full` (default quick); `full` must remain exactly today's behaviour.
Files: `integrations/{power,weather,bluetooth,media}/*`, `commands/{media,system}.rs`.
- **Controller split (Oct 30, after the Phase 5 audit):** Phase 6 runs as two sessions in one crate, serially. **6a** = the three Phase 5 clipboard test gaps + power (port `staging/power-core`), weather (port `staging/weather-core`, incl. `weather_geocode`) and Bluetooth (port `staging/bluetooth-core`); its gate is the Phase 6 gate restricted to those three services and runs as `scripts/phase6a-gate.sh` (chaining `scripts/phase5-gate.sh`). **6m** = media: MPRIS (port `staging/mpris-core`), the music extras below (port `staging/lrclib-core`, `staging/lrc-lyrics`, `staging/spectrum-bands`), `commands/media.rs`; gate `scripts/phase6m-gate.sh` chaining phase6a.
- **Accepted after the Phase 6a controller audit (Oct 31).** Controller `CLEAN=1 scripts/phase6a-gate.sh` in a 14G scope: PASS (68 min, 493 tests, 15 gate mutations, worst screenshot 0.82 %); baseline summary identical, new gate scripts only; earlier-file edits = additive clipboard tests/helper only; reqwest pinned `=0.13.5` rustls-only. Controller mutations 1/6 killed; the survivors are test gaps and become the **first items of 6m**: (1) **spoofed NameOwnerChanged** in `power/upower.rs` and `bluetooth/bluez.rs` — zbus's client-side match filter cannot match a well-known sender (`zbus-6.2.0 match_rule/mod.rs`: "We can't match against a well-known name"), so a unicast `NameOwnerChanged(arg0=<service>, …, "")` from any peer reaches the stream and the in-code `sender == org.freedesktop.DBus` check is the only guard; add a rogue-peer test per service that sends it and asserts the service stays available (**revised after the Phase 6m audit:** wrong premise — zbus parses `org.freedesktop.DBus` as a UNIQUE name (zbus_utils 6.1.0 `parse_unique_name` special case), so the rule's `sender='org.freedesktop.DBus'` IS filtered client-side and removing only the in-code check is equivalent; the 6m rows mutate rule + check together, which the rogue-peer tests kill); (2) **BlueZ `InterfacesRemoved` root-path guard** (only `InterfacesAdded` is pinned): add a non-root `InterfacesRemoved` test; (3) **weather superseded response** (`shared.generation != generation` → `false` survives because the `_gate` serialises fetches and the following Paris fetch overwrites the late Berlin answer): make the second place's request fail and assert the late first-place answer is never shown/emitted; (4) **weather `active()` without location** (`enabled && location.is_some()` → `enabled` survives): assert the `no-location` unavailable reason/status. Each new test must fail under the matching mutation (list it in the 6m mutation table).
- **Accepted after the Phase 6m controller audit (Oct 31).** Controller `TIER=quick scripts/phase6m-gate.sh` in a 14G scope: PASS (820 s; 668 tests ok / 0 failed; 21 gate mutations incl. the four 6a gaps; worst screenshot 0.82 %; sandbox audit clean). A first run failed ONLY the live-PID leak detector: an external HDMI-A-1 hotplug (Sunshine stream start, 10:04:31) segfaulted plasmashell and island-rust died with it; not a sandbox leak. Baseline summary identical, new phase6m scripts only; earlier-file edits additive (6a tests, private_bus/system_mock helpers, system_services wiring). state.db: claude-sonnet-5-5, 268 calls, no subagents, writes only in island-tauri + scratch. Controller mutations 1/6 killed; 4 equivalent (MPRIS path and `arg0` interface guards are also enforced client-side by zbus's `path=`/`arg0=` match rules; dropping the equalizer `hint.is_some()` gate is caught by `begin()` returning `idle`; a symlinked cache file is rejected by the 0o077 mode check because symlink mode is 0o777). **First item of 6b:** lyrics cache read with a group-only widened file (`0o640`) must return None (current test uses 0o644, so `& 0o007` survives). Also carried: spectrum magnitude-normalisation scale is unpinned (Sonnet note); nothing calls `media_set_visible` until Phase 7 wires it; the real LRCLIB smoke stays `#[ignore]`.
- zbus services tested against mocks on a private bus; weather tested with a local HTTP fixture (inject base URL), plus one real Open-Meteo call marked `#[ignore]`.
- **Gate:** each service reports capabilities; stopping the mocked daemon yields `available:false` then recovery; no zero-fabrication.
- **Music extras (added after Phase 2 review, §0.5):** lyrics from LRCLIB (pure core accepted in `staging/lrclib-core`: query path, response classification, cache key; port it, and when porting require the 200 body to be a JSON object before deserializing, because serde accepts a JSON array as the record struct (controller probe `[false,null,"[00:01.00]x"]` → Synced); add that test; keyless; local HTTP fixture in tests, one real call `#[ignore]`; synced LRC timing driven by MPRIS position; cache in the data dir; opt-in network). Live equalizer from a PipeWire monitor-capture of the playing stream with a Rust FFT into a small fixed number of bands, emitted at ≤ 30 Hz only while the music activity is visible; capture stops when hidden or paused; never records the microphone; volume untouched. Tests use a synthetic sine fixture stream on a private PipeWire instance (or a recorded fixture if that is infeasible, stated as such).

### Phase 6b — New apps (added after Nick's concept review, Oct 30)
Timer (timer / pomodoro / stopwatch; actor-owned timers with injected clock, survives view switches, completion raises an activity + notification), Scratchpad (plain-text notes in the data dir, debounced atomic saves), Downloads (recent files in the XDG download dir via directory notifications; open/reveal), Calendar (backend decided in §0.5 "Calendar backend": private Google iCal URL subscription, read-only, plus island-local .ics events, both via `staging/ics-core`), Controls + Volume mixer (volume, brightness, quick toggles; per-app streams). Pure cores go to `staging/` first (timer state machine, scratchpad store, downloads list model). Each app is a module-registry module. **Gate:** each core's unit tests + mutations; services against private PipeWire/D-Bus/data dirs only; the real volume/brightness/calendar are never changed in tests.
- **Controller split (Oct 31, after the Phase 6m audit):** two serial sessions in one crate. **6b1** = the 6m carried test gap (lyrics cache `0o640`) + Timer (port `staging/timer-core`), Scratchpad (port `staging/scratchpad-core`), Downloads (port `staging/downloads-core`), Calendar (port `staging/ics-core`; Google secret iCal URL + local .ics per §0.5) as module-registry modules with commands/events; gate `scripts/phase6b1-gate.sh` (`TIER=quick|full`, full chains `scripts/phase6m-gate.sh TIER=full`). **6b2** = Controls: volume mixer (port `staging/mixer-core`), brightness (`staging/brightness-core`), quick toggles (`staging/quick-toggles-core`), system tiles (`staging/system-tiles-core`), agent tracker (`staging/agent-tracker-core`), update badge (`staging/updates-core`); gate `scripts/phase6b2-gate.sh` chaining 6b1. The controller runs the FULL tier after 6b2.
- **Accepted after the Phase 6b1 controller audit (Oct 31).** Sol (session local-bb6b71df…, gpt-6.1-sol, 305 calls + one read-only review subagent) could not finish its own gate run (its terminal killed the foreground run at 420 s). The controller ran `TIER=quick scripts/phase6b1-gate.sh` in a 14G scope: PASS (817 s; 867 tests ok / 0 failed; 13 gate mutations; worst screenshot 0.86 %; sandbox + live audits clean). Baseline summary identical; new phase6b1 scripts only; earlier-file edits = the lyrics 0o640 test + additive wiring (apps snapshot field, module registry in settings, `AppServices` in system_services; `settings_set` now runs `configure` in `blocking`). **Controller amendment:** Sol's gate ran `hermes-agent-usage` and asserted Codex usage, so it failed for the controller once Codex weekly hit 98 %; the controller removed that block (usage guards belong in the agent's procedure, never in a gate verdict, and the probe spends Claude usage). Controller mutations 5/7 killed; 2 test gaps → **first items of 6b2:** (1) a calendar URL with userinfo (`https://user@host/…`) must be rejected (`|| !url.username().is_empty()` removal survives); (2) `downloads open` of a regular file present in the directory but not in the model's visible items (e.g. a `.part`/hidden/expired entry) must be refused (membership-check removal survives). **Decisions ratified:** the four local-app modules (Timer, Scratchpad, Downloads, Calendar) default enabled, the Calendar subscription stays unconfigured until the secret file exists; completed timer activity stays until reset/cancel (Phase 7 adds dismiss); a separate Timer actor feeding the StateActor activity intents is fine. **Carried to 6b2 (bug):** the calendar publishes its default window at UTC offset 0 and unknown TZIDs fall back to floating time; 6b2 must resolve the local zone and IANA TZIDs with DST from the system tz database (verify a crate such as `jiff` with its system-tzdb feature on crates.io/docs.rs before pinning; tests use fixed TZ fixtures, not the host zone).
- **Accepted after the Phase 6b2 controller audit (Oct 31).** Sonnet started 6b2 (session local-8b4a57ca…) and its chat ended without a report; Sol (local-67fecd4d…, two runs split by a Codex session reset) reviewed and completed it. The controller ran `TIER=quick scripts/phase6b2-gate.sh` in a 14G scope: PASS (2300 s; 45 named acceptance scenarios; 35 compiling gate mutations with identical restores and fresh source hashes; bindings check; npm audit 0; four-view sheets viewed, unchanged apart from clock/PID/cursor). Baseline summary identical; only new phase6b2 scripts; state.db clean. **Controller amendments:** (a) `scripts/phase6b2_extra_cases.py` row `updates-argv-gains-write-flag` uses the same-length `["paru", "-Syu"]` (the 3-element form did not compile against a fixed-array assertion, so it was not valid evidence); (b) `scripts/phase1-gate.sh`, `phase1_5-gate.sh`, `phase2-gate.sh`, `phase2_5-gate.sh` now pass when the live island is NOT running both before and after (any change still fails) — it is only a sandbox-leak detector and was down after a plasmashell crash. **FULL CLEAN chain skipped for 6b2 by Nick's decision** (run 1 hit (b); run 2 failed only because Nick launched his island from the panel pill mid-run; phases 1→2 had all passed in it). The FULL chain therefore runs after Phase 7 (and before 9) as already required. Controller mutations 3/4 killed (sink-name `-` guard, reserved voice words, mixer 150 % bound); 1 test gap → **first item of Phase 7:** `agents/reader.py` `?mode=ro` → `?mode=rwc` survives; add a test that a missing state.db is reported unavailable and is NOT created. Decisions ratified: brightness module enabled and agent tracker disabled in fresh defaults; colour copy requires the Clipboard module enabled (never starts history monitoring silently); screenshot/picker completion async; voice list refreshes on start or explicit refresh.

### Phase 7 — Frontend translation to TypeScript
Order: `ipc/client.ts` + stores → `Island.tsx`/`IslandSurface.tsx`/`geometry.ts` (translate the already actor-driven renderer; mode-from-`IslandStateChanged` and hover-timer removal moved to Phase 3, revised after Phase 3 review) → `TerminalView` (xterm write-callback acks) → `SettingsView` → `MonitorView` → `AiStatsView` → new `NotificationsView`, `ClipboardView`, `QuickInfo`, `NowPlaying`, app-grid launcher, side buttons + Layout editor, Phase 6b app views (revised after Nick's concept review: build to §0.5 "Design direction") → remove `legacy-shim.ts` and `AnchorSignal`. Enable `freezePrototype` at the end.
- **Gate after every file:** `npm run typecheck`, GUI smoke, and settled-screenshot evidence. Revised after Nick's concept review: the terminal keeps its Phase-2 rendering (fonts, xterm colours); everything else is compared against the §0.5 concept pictures in both appearances (skin colours and Neutral black), with animation endpoints and frame pacing measured. Pixel differences must be explained, not ignored.
- **Controller split + coexistence (revised before dispatch, Nov 1).** The TS renderer is a NEW page `island.html` (entry `ui/island-main.tsx`, sources under `ui/island/`, `ui/stores/`, `ui/views/`) selected by a new `--ui island` variant; `harness` (`index.html`, still the default) and `legacy` (`legacy.html` + `legacy-shim.ts` + `ui/legacy/*` + `AnchorSignal`) stay byte-for-byte as they are, because every earlier gate (phase2/3 legacy byte-identity, `phase3-screens.py` legacy parity, Phase 3–6 GUI tests) runs against them unchanged. Removing `legacy-shim.ts`, `AnchorSignal` and the legacy page therefore moves to **Phase 9** (legacy retirement, with Nick), not Phase 7; Phase 9 also makes `island` the production default. New GUI tests use `--ui island` and canonical `data-state` names from the start. Two serial sessions in one crate: **7a** = carried 6b2 gap (agents `reader.py` must open state.db with `mode=ro`: a missing db is reported unavailable and NOT created), `ui/ipc/client.ts` + rev-guarded topic stores (bootstrap fills all before first render), `Island`/`IslandSurface`/`geometry.ts`/`Topline`/`ViewPager` (translate the actor-driven renderer; no renderer hover timers, intents via `island_dispatch`, geometry only for the current rev), `TerminalView` (xterm write-callback acks, Phase-2 terminal rendering, same-PTY reopen with draft), `SettingsView` (incl. appearance: follow skin / Neutral black, module settings from the registry), `MonitorView` (polls only while visible, one in flight), `AiStatsView` (dashboard style, port `ui-staging` AiDashboard), the `ui-staging` shell/tokens as the visual base; gate `scripts/phase7a-gate.sh` (`TIER=quick|full`, full chains `TIER=full scripts/phase6b2-gate.sh`). **7b** = app-grid launcher with jiggle edit mode, side buttons + Layout editor (`layout-core` geometry incl. short-island overlap), Notifications/Clipboard/QuickInfo/NowPlaying (wire `media_set_visible`)/Timer (dismiss affordance for completed activity)/Calendar/Scratchpad/Downloads/Controls+Mixer/Brightness/agent chips/update badge views ported from `ui-staging` with validated adapters, overlap-at-pill morph, `freezePrototype: true`; gate `scripts/phase7b-gate.sh` chaining 7a. 7b also closes two ui-staging round-2 test gaps found by controller mutations (4/6 killed): ClipboardView newest-first order (`b.copiedAt - a.copiedAt` reversed survives) and NowPlaying negative `positionMs` clamp (`Math.max(0, …)` removal survives; the elapsed label must never show a negative time). The controller runs the FULL chain after 7b.
- **Accepted after the Phase 7a controller audit (Nov 1).** Controller re-ran `TIER=quick scripts/phase7a-gate.sh` (no runner override) PASS in 2094 s: 1207 tests, 0 failed, 16 gate mutations, legacy screenshot parity, new real-EIS `kwin_phase7a_island`. Baseline/statedb clean; earlier-file edits additive only (`--ui island` wiring, bindings/client). Controller mutations 5/5 killed (arrow guard on inputs, VisualChanged rev not advancing, MonitorView always visible, VisualChanged listener dropped, pointer leave sent as enter; last three via the real GUI test). RATIFIED: separate private `appearance.json` + additive `VisualChanged` envelope (legacy payloads unchanged); large-view endpoints 1040 px terminal / 640 px other views (radius 24, scrollable per-view heights bounded by viewport − 64 px). 7a's four view tabs are temporary; 7b replaces them with the launcher.
- **Phase 7b controller decision (revised after the Phase 7b blocker, Nov 1).** `src-tauri/tests/kwin_phase7a_island.rs` tests the same `--ui island` page that 7b changes by design, so it is the ONE earlier test 7b may amend (recorded here; every other earlier test, gate and mutation script stays unchanged): (a) `settled()` compares native ShapeInput with the exact union of every visible painted interactive rect (outer `.island` + each visible side button, gaps excluded; no bounding box), still empty when idle; (b) `view()` reaches each view by real EIS clicks through the launcher (no hidden controls, no test-only navigation); (c) endpoint and offset checks keep their asserted values for the outer surface; only the additional side-button rects and any approved pill-overlap geometry are added; draft/PTY/appearance/monitor/cleanup assertions stay. If a 7b change moves code a `scripts/phase7a_cases.py` row anchors on, that row's anchor may be updated to the equivalent new code (same broken behaviour, same named test), listed in VERIFICATION.md; 7a's belt must still kill all 16. Launcher policy: Hermes home + Settings are neither movable nor disableable, including indirect displacement by other moves; enforce it in a production wrapper over `launcher-arrange-core` semantics (staging stays untouched). Adapters fence per query/lifecycle generation where a service restarts its revision at zero on re-enable or shares revisions across windows (calendar).
- **Accepted after the Phase 7b controller audit (Nov 1).** Sol (one session + 4 resumes) → controller re-ran `TIER=quick scripts/phase7b-gate.sh` (no runner override) PASS in 3859 s: whole cargo suite incl. earlier GUI binaries, amended `kwin_phase7a_island` + 9-scenario `kwin_phase7b_launcher`, 17 gate mutations (8 killed by real GUI), legacy screenshot parity, 7a/7b visuals. Baseline summary identical; gate-sha diff = new phase7b scripts only; `kwin_phase7a_island.rs` diff inside the decision (union + launcher navigation + marked diagnostics); window/* edits additive and island-page-only (`over_panel` placement, input/blur split). Controller mutations 2/4 killed (lock indirect visible-count, media privacy-hide coalescing); 2 survivors judged near-equivalent, recorded as Phase 8 test gaps: `item.enabled |= item.locked` in `LockedArrangement::new` (output path re-forces `enabled || locked`), and AppsView effect cleanup `mediaVisibility.set(false)` (the effect's next run sets false on hide; only a true unmount while showing music relies on it). RATIFIED: private `layout.json`, `unchangedSizing` marker (keeps 1040/640 until the user picks a preset), short-island rule (bottom row below max(island, side columns) + gap), closed native-pill overlay ignores DOM enter (its ShapeInput is empty; only the panel starts hover), frozen-prototype xterm build fix. NICK DECIDED (Nov 1): FULL chain skipped for now; it still runs before any old runtime is removed (Phase 9 retirement step).

### Phase 8 — Hardening and other platforms
- Idle resource comparison against island-rust using matched isolated process trees (`island-rust/tests/idle_resources.py` methodology).
- Windows: input-region hook backend, `WS_EX_*` flags, UserNotificationListener spike from an MSIX/sparse-identity build, GSMTC, Bluetooth watcher.
- macOS: NSPanel + mouse monitors, notch placement.
- Optional: QML appearance push protocol.

### Phase 9 — Live switch and legacy retirement (requires user approval)
- **Controller split (revised before dispatch, Nov 1; Nick approved Phase 9).** The CLI still refused every non-test launch (Phase 1 wording), so **9a** (agent) = live-launch readiness: live `--overlay` → `.island-data-overlay` per §2.6 (shared with island-rust, additive Tauri-only files, rollback on the same dir), preview → `.island-data`, `--ui` default stays harness (live launcher passes `--ui island`), live singleton/coexistence, anchor-client-compatible status/panel protocol, island-rust-compatible settings writes, `build.sh`/`run.sh` with embedded release assets, prepared (not applied) anchor-client patch + autostart entry + SWITCH.md; gate `scripts/phase9a-gate.sh`. **9b** (controller with Nick) = the switch below. XDG data migration and legacy removal stay later steps, after the FULL chain and a rollback drill.
- Follow the ripple-island runtime switch procedure: fresh archive, preserve autostart state, WM_DELETE only the island-rust window, verify exit, launch Tauri overlay, read back `/proc/PID/exe`, bounds, empty idle ShapeInput, socket mode. Update `anchor-client.py` and autostart only after that.
- Present an exact move list for data-dir migration before moving anything.
- Remove Electron/Wry runtimes only after acceptance + a rollback drill.

---

## 9. Risk register

| Risk | Impact | Mitigation / owner phase |
|---|---|---|
| tauri-specta/specta are RC | breaking API on bump | exact pins; ts-rs swap path (§5.6) |
| WebKitGTK transparent repaint bug (tauri#12800) | stale pixels | Phase 1 screenshot gate |
| KWin behaviour differs from xfwm4 (frame, focus, shape routing) | wrong click-through / stolen focus | Phase 1.5 nested-KWin tests (§2.7) |
| NVIDIA/DMABUF blank webview | blank window | Tauri Linux graphics guide; `WEBKIT_DISABLE_DMABUF_RENDERER=1` only in launcher, only if reproduced |
| No layer-shell hook in Tauri | must stay on XWayland | §0.4; revisit if tao adds a pre-realize hook |
| gtk-rs 0.18 line unmaintained | advisories | pinned by Tauri; track Tauri's GTK4 migration |
| Notification monitor privacy | leaking message bodies | opt-in mirror, redact option, memory-only default |
| Clipboard privacy | password capture | default off, sensitive-MIME skip, deny regex, pause |
| Windows listener needs package identity | feature gap | spike; `unsupported` until proven |
| Fullscreen apps over the island | island hidden | not promised (same as today) |
