# Companion pack verification

## Delivered

Five original, 20 logical-pixel QML renditions, registered through the loader, Rust settings enum, generated TypeScript contract and accessible SVG character picker:

| ID | Label | Hover | One-off habit |
|---|---|---|---|
| `notchmimic` | Null | Ivory toothy grin | Peeks upward while little hands grip the edge |
| `obelisk` | Little Obelisk | Brighter yellow slit | Faceted cap briefly separates |
| `companioncube` | Companion Cube · Portal | Heart brightens | Bounded, stepped 4°/8° tip, then upright |
| `nurgling` | Nurgling | Cheeky tongue | Tiny arm scratch/wave |
| `hollowknight` | Hollow Knight | Tiny nail appears | Head lean and cloak twitch |

Blob remains the default; unknown strings, including `null`, `constructor` and `__proto__`, fall back to Blob. Prototype names no longer accidentally resolve through the QML map. Settings persistence and rejection of invalid values remain strict.

## Performance contract

`HabitCharacter.qml` owns one stopped-by-default 125 ms timer. A resting blink can start a 750 ms burst, subject to a 12-second timestamp cooldown. Cooldown itself has **no timer**. Visual positions use whole-device-pixel steps. Cube's tip is a finite stepped Item transform, not an animated path or perpetual rotation: its 16 px square remains between approximately 0.964 and 19.036 inside the 20 px allocation even at 8°. Shapes have static geometry; there are no shaders, layers, effects, canvases, added Behaviors, or continuous custom idle loops.

Hidden art/face, unattended monitor, reduced motion, hover, sleeping and thinking all stop/reset the burst immediately. Unloading destroys the timer; tests connect to its signal and verify there are no subsequent ticks. Brain gaze is reused only by characters with eyes; Cube has no eyes. Colors are intentionally character-specific, with scheme-responsive edges for the charcoal silhouettes. Picker Knight gained a contrasting mask outline after inspection on a light surface.

## Commands and results

Run from `/home/nick/Hermes/Monolith` unless another directory is specified. Logs are under `/home/nick/Hermes/Monolith/evidence/companion-pack/`.

```sh
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  /usr/lib/qt6/bin/qmltestrunner -input island/qmltests \
  -o evidence/companion-pack/qml-all.txt,txt
```

**24 passed, 0 failed.** The dedicated pack accounts for 12 passes, including setup/cleanup: all five character loading/switching/fallback/state/gaze/burst/cleanup cases and all five layout cases. Layout is checked at 144 px with the muted-mic indicator, and 124 px without it; the existing compact-muted hide rule is preserved. Burst tests enforce a duration between 650 and 1000 ms, at most six post-start state changes, integer steps, no idle changes, a stopped idle timer, cooldown and cancellation on every gate. Expected offscreen platform/theme warnings occur only in the existing main-component test.

```sh
mkdir -p evidence/companion-pack/renders
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  /usr/lib/qt6/bin/qmltestrunner -input island/qmlrenders/tst_companion_render.qml \
  -o evidence/companion-pack/render-test.txt,txt
python island/qmlrenders/companion_strips.py > evidence/companion-pack/image-diffs.log
```

**3 render passes, 0 failures; image assertions pass.** Requires Pillow and `rsvg-convert`. Captures include normal, hover, sleep, a frozen repeatable habit pose, settled, and real timer-driven before/six-sample/after sequences. Individual 1× PNGs are actually 20×20; the 4× captures are 80×80 with a QML transform, not a claim of a physical high-DPI monitor. Contact sheets enlarge 1× captures using nearest-neighbor. All five real timed sequences visibly change and return to baseline. Pixel comparison ignores only per-channel differences of one intensity level, because software Shape cache rasterization produced 1-LSB antialias rounding on a few cloak-edge pixels. It does not ignore positional changes.

| Character | Changed habit pixels, 1× | Settled significant differences, 1× |
|---|---:|---:|
| Null | 154 | 0 |
| Little Obelisk | 60 | 0 |
| Companion Cube | 223 | 0 |
| Nurgling | 28 | 0 |
| Hollow Knight | 174 | 0 |

Timed sequences also have zero significant final differences at both capture sizes. Full results: `image-diffs.json`. I inspected reference images, both state-strip sizes, the actual timed 1× strip, and the picker on dark/light backgrounds. Refinements included keeping Null's peek within bounds, keeping the shard cap inside its slot, strengthening the Knight picker outline, and making Cube's bounded tip explicit.

From `island-tauri/`:

```sh
npm run typecheck
npm run test:ui
npm run build:ui
```

**All pass:** TypeScript clean; **170 UI tests passed**; Vite production build succeeds (516 modules). Picker tests exercise all seven IDs, selection payloads and checked-state semantics. The initial attempted `npm run build` failed because that script does not exist; the actual project script is `build:ui`, whose successful output replaces that attempt in `ui-build.log`.

From `island-tauri/src-tauri/`, heavy commands wrapped in `systemd-run --user --scope -p MemoryMax=5G nice -n 15 ionice -c3`:

```sh
cargo test --lib export_bindings -- --ignored --nocapture
cargo test --lib services::settings::tests -- --nocapture
cargo test --locked --lib -j4
cargo clippy --locked --lib --tests -j4 -- -D warnings
```

**Pass:** bindings exported by the real Rust test; **7 targeted settings tests passed**; **361 library tests passed, 3 ignored, 0 failed**; Clippy clean. The first foreground export attempt exceeded the tool's foreground timeout; retrying as a tracked memory-capped scope succeeded. All tracked build/test processes have exited; no test process was left running.

```sh
rustfmt --edition 2021 --check \
  island-tauri/src-tauri/src/contract/settings.rs \
  island-tauri/src-tauri/src/services/settings.rs
git diff --check
```

**Pass.** No wholesale Cargo formatting was used.

## Evidence paths

Single evidence parent: **`/home/nick/Hermes/Monolith/evidence/companion-pack/`**.

- `strips-1x.png`, `strips-4x.png`: normal / hover / sleep / habit / settled.
- `timelines-1x.png`, `timelines-4x.png`: real before / six timed frames / after.
- `renders/`: all original 20×20 and 80×80 captures.
- `image-diffs.json`, `image-diffs.log`: actual pixel counts and settling assertions.
- `picker/previews.png`: actual picker SVGs rasterized on dark and light surfaces; SVG/PNG sources alongside.
- `references/`: downloaded reference images, nearest-neighbor pixel dumps and `palettes.json` (image dimensions, alpha bounds, quantized colors and source URLs).
- `qml-all.txt`, `qml-pack.txt`, `render-test.txt`, `bindings.log`, `rust-settings.log`, `rust-all.log`, `clippy.log`, `rustfmt.log`, `typecheck.log`, `ui-tests.log`, `ui-build.log`: real command output.

### Reference sources

The original renditions do not ship reference pixels as character art. Reference files are inspection evidence only and retain their owners' rights; do not bundle them into the app.

- Knight wiki page: <https://hollowknight.wiki/w/Knight>
  - Actual idle image: <https://cdn.wikimg.net/en/hkwiki/images/4/47/The_Knight_Idle.png> (63×132).
  - Pillow palette identifies white/black mask and navy cloak; `#454d63` is retained for the cloak. The mask/horns dominate the upper silhouette.
- Portal wiki page: <https://theportalwiki.com/wiki/Weighted_Companion_Cube>
  - Image: <https://i1.theportalwiki.net/img/thumb/6/6b/Portal2_CompanionCube.png/300px-Portal2_CompanionCube.png> (300×291).
  - Gray corner pads and cross panels, muted pink heart, no invented facial eyes.
- Lexicanum: <https://wh40k.lexicanum.com/wiki/Nurgling>
  - Image: <https://wh40k.lexicanum.com/mediawiki/images/f/f3/Nurglings_TacticusIcon.jpg> (175×230).
  - Horns, protruding belly and toothy grin informed the proportions. The reference's warm yellow/brown colors were deliberately shifted to the requested sickly green palette.
- Null and Little Obelisk are original designs following the task's visual brief, not copied assets.

## Files changed

- New: `docs/companion-pack-spec.md`, this verification document.
- New QML: `characters/HabitCharacter.qml`, `NotchMimic.qml`, `Obelisk.qml`, `CompanionCube.qml`, `Nurgling.qml`, `HollowKnight.qml` under `island/plasma-anchor/contents/ui/`.
- Modified: `island/plasma-anchor/contents/ui/Companion.qml` registration and robust fallback.
- Modified: `island-tauri/src-tauri/src/contract/settings.rs`, `src/services/settings.rs`, generated `island-tauri/ui/ipc/bindings.ts`.
- Modified: `island-tauri/ui/island/companionCharacters.tsx`, `island-tauri/tests/phase7b-notch-controls.test.mjs`.
- New harnesses: `island/qmltests/tst_companion_pack.qml`, `island/qmlrenders/tst_companion_render.qml`, `island/qmlrenders/companion_strips.py`.
- Evidence under the single parent listed above; no unrelated source files modified.

## Independent controller audit

Re-ran the complete offscreen QML suite (24 passed), TypeScript checking (clean), UI suite (170 passed), render harness (3 passed), and `companion_strips.py` image assertions. All five timed habits visibly change and settle to zero significant baseline differences at both capture sizes. Inspected the 1× state contact sheet and shared habit-controller implementation. Reviewed stored Rust-library and Clippy command output (361 passed / 3 ignored and clean Clippy); these heavy commands were not independently re-run. `git diff --check` passed. No live deployment performed.

## Deployment follow-up

User authorized deployment. Release rebuilt successfully with `build.sh` under CPUWeight/IOWeight 20, nice 15, idle I/O scheduling and MemoryMax 8G; log: `evidence/companion-pack/deploy-build.log`. Verified rollback backup: `/home/nick/Hermes/Backups/Monolith/companion-pack-20261104T065508Z/` (previous binary, plasmoid, settings, panel configuration, source archive).

Copied plasmoid to `~/.local/share/plasma/plasmoids/local.hermes.island.notch3` and compared all seven changed/new QML files byte-for-byte. Restarted `plasma-plasmashell.service`, gracefully stopped old Monolith and started new release using `run.sh --overlay --ui island` in user unit `monolith-companion-pack-1793775503`. Readback: PID 1281276, online, valid DP-1 native anchor, three monitor notches reconciled, no monitor-notch error, Isaac selection preserved. Live pin/open produces 200 input rectangles; collapse returns idle with zero input rectangles. Real desktop screenshots saved as `evidence/companion-pack/deployed-open.png` and `deployed-closed.png`; inspected open capture and startup log. No character-specific QML loading errors observed. Whole-desktop journal retains unrelated widget/theme warnings. Live CPU A/B and switching each new character in the production UI have not been measured/tested; prior offscreen switching coverage remains applicable.

## Pre-deployment limitations / authorization boundary

No plasmoid deployment, desktop/service restart, app kill, live setting change, or commit was performed. Offscreen images are **not live desktop screenshots**. Real three-monitor compositor screenshots and alternating live CPU A/B comparisons are pending explicit deployment authorization; no live CPU reduction or compositor-performance measurement is claimed. The current evidence establishes stopped idle timers, bounded state changes, bounded bursts, real offscreen pixel changes, settling and integration correctness, not live panel CPU cost. Full standalone application packaging/integration tests were not run; library, UI, Clippy and offscreen QML gates were run.
