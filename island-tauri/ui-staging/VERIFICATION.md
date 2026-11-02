# UI staging verification

Status: implementation and all non-GUI gates PASS. Screenshots / settled visual
acceptance are UNVERIFIED under the explicit concurrent-GUI isolation rule.
This is a standalone presentational package, NOT Phase 7 or live-switch acceptance.

## Scope and files created

All authored/copied/generated files are under
`/home/nick/Hermes/Ripple/island-tauri/ui-staging/`. No edits to `ui/`,
`src-tauri/`, `scripts/`, Rust staging cores, SPEC, parent manifests or parent
VERIFICATION. No git commit/push, global/system installs, skills or memory edits.
No running-island, live data directory, real clipboard/audio/power/desktop access.
No display variables exported in the persistent shell. Only owned dev-server
processes were started and terminated; the screenshot runner started no GUI.

Main deliverables:

- `.gitignore`, `package.json`, `package-lock.json`, `tsconfig.json`,
  `vite.config.ts`, `index.html`, `README.md`, this `VERIFICATION.md`.
- `src/models.ts`, `src/arrangement.ts`, `src/mocks.ts`, `src/gallery.tsx`.
- `src/tokens/themes.css`, `src/styles.css`.
- `src/components/{IslandShell,AppGrid,SideButtons,TimerApp,ControlsApp,AiDashboard,Icon}.tsx`
  and `src/components/index.ts` (also exports TerminalHome, LayoutEditor,
  AgentRunChips and UI_SPRING).
- Four `src/assets/fonts/OpenRunde-{Regular,Medium,Semibold,Bold}.woff`, copied
  byte-identically from the existing renderer assets, never edited at source.
- `tests/{setup.ts,arrangement.test.ts,components.test.tsx,arrangement-fixtures.json}`.
- `contract-probe/{Cargo.toml,Cargo.lock,src/main.rs}`; a development-only public
  API consumer. Build/docs output confined to its ignored `target/` directory.
- `tools/{generate-contracts,mutations,verify,dev-smoke,screenshots}.py`.
- `evidence/`: actual check logs, eight mutation JSON/log/restoration-diff sets,
  mutation summary, source hashes, dev HTTP smoke log and screenshot skip record.
- Built `dist/` gallery and private `node_modules/` (both ignored/rebuildable).

The complete source import tree is local to this package or pinned npm packages.
There are no Tauri imports or native commands in component code. A static scan
for colour literals/IPC in components passed. All colour values are in tokens;
component SVGs inherit `currentColor`. Repo branch rechecked: `hermes-island`.

## Prerequisite reads and pictures

Read SPEC §0.5 (including the full long More tiles/features bullet), §6,
§8 Phase 7 and the full Staged pure cores bullet under Phase 3; legacy skin CSS,
Toast sibling-root pattern; required core lib.rs/model.rs files where present;
ripple-island Tauri section and migration reference. Public fields re-exported
from other source filenames were inspected through locally generated rustdoc API
descriptions inside this package, not edits to those crates.

All four original concept images were loaded with the image tool, unchanged:

- `concept-pics/Screenshot_20261030_125805.png`: app launcher.
- `concept-pics/Screenshot_20261030_125909.png`: timer.
- `concept-pics/Screenshot_20261030_125938.png`: AI dashboard.
- `concept-pics/Screenshot_20261030_130007.png`: layout editor/settings.

## Dependency verification

Before pinning/installing, queried npm for actual versions and relevant engines /
peer dependencies. Node on this machine: `v26.12.1`. Exact pinned versions:

    react / react-dom         19.3.1 (within the parent's ^19)
    framer-motion             12.34.0 (exact parent match)
    vite                      8.3.5
    @vitejs/plugin-react      6.1.2
    typescript                5.9.3
    @types/react / react-dom  19.3.0
    @types/node               26.6.6
    vitest                    5.0.5
    @testing-library/react    16.3.3
    @testing-library/dom      10.4.2
    jsdom                     30.1.3

Only this folder received npm installs. Real installation output:

    added 99 packages, and audited 100 packages in 3s
    found 0 vulnerabilities

No browser package or system package was installed. System PyGObject successfully
imported WebKit2 4.1 without starting a display. Xvfb is already installed.

## Executed verification — real output

All test/build commands ran under `nice -n 10`. The verify script captures
combined output and exit status in `evidence/checks.json` plus the named logs.
Final `nice -n 10 python3 tools/verify.py` exited 0:

    $ nice -n 10 python3 tools/generate-contracts.py --check
    Rust conformance: 16 traces, 1216 transitions; checked tests/arrangement-fixtures.json
    exit=0

    $ nice -n 10 npm run typecheck
    npm notice run hermes-island-ui-staging@0.1.0 typecheck
    npm notice run tsc --noEmit
    exit=0

    $ nice -n 10 npm test
     RUN  v5.0.5 /home/nick/Hermes/Ripple/island-tauri/ui-staging
     Test Files  2 passed (2)
          Tests  48 passed (48)
       Start at  02:32:02
       Duration  4.30s (transform 61%, environment 17%, tests 16%, import 4%, setup 2%)
    exit=0

    $ nice -n 10 npm audit --audit-level=low
    found 0 vulnerabilities
    exit=0

    $ nice -n 10 npm run build
    vite v8.3.5 building client environment for production...
    transforming...
    ✓ 423 modules transformed.
    rendering chunks...
    computing gzip size...
    dist/index.html                                 0.41 kB │ gzip:   0.28 kB
    dist/assets/OpenRunde-Regular-XRnp7O8j.woff   183.54 kB
    dist/assets/OpenRunde-Bold-DIFtOJ1o.woff      189.65 kB
    dist/assets/OpenRunde-Semibold-CCOYa82w.woff  189.70 kB
    dist/assets/OpenRunde-Medium-DS3YmeE4.woff    194.90 kB
    dist/assets/index-BBxJPqsT.css                 16.07 kB │ gzip:   4.19 kB
    dist/assets/index-ByOQtUGP.js                 378.81 kB │ gzip: 119.46 kB
    ✓ built in 655ms
    exit=0

    PASS: strict typecheck, unit tests, low-level audit, build, Rust fixture freshness and byte-identical bundled fonts

`nice -n 10 python3 tools/dev-smoke.py` exited 0, actually started npm run dev,
fetched both the HTML and transformed gallery module over loopback and stopped
its owned process group. No browser was started:

    PASS: npm run dev → 127.0.0.1:5197 HTTP 200, actual gallery module served; no browser started

Saved server output (`evidence/dev-smoke.log`):

    npm notice run hermes-island-ui-staging@0.1.0 dev
    npm notice run vite --host 127.0.0.1
      VITE v8.3.5  ready in 103 ms
      ➜  Local:   http://127.0.0.1:5197/

Other checks:

- `git diff --check -- island-tauri/ui-staging`: exit 0 (new untracked package;
  this is not a substitute for the TS/test/build gates).
- Four bundled font copies compared byte-identical to existing renderer fonts.
- `evidence/source-hashes.json` contains real SHA-256 hashes of src/tests/tools.
- Lockfile and build are local to this package. Public-API probe uses its own
  locked Cargo resolution, with target output inside ui-staging.

### Unit/interaction coverage

- Locked dominates disabled input and cannot be disabled; movable locked tiles.
- Re-enable appends to visible order, preserves hidden items.
- Cancel restores snapshot; unchanged commit is null; commits include ALL IDs,
  including hidden/inert modules, plus only changed enabled flags.
- Visible-index moves, clamping, reconcile of items AND snapshot; removed IDs are
  not resurrected by cancel and newly added IDs are not lost.
- Constructor bounds, editing guards, unknown IDs and hidden-move rejection.
- 16 real Rust public-API traces compare 1216 transitions in the TS reducer.
- Pointer hold at 499 vs 500 ms, release-click suppression, movement/cancel abort;
  menu entry, Done/Cancel/Escape exit, keyboard reorder controls, hidden re-enable.
- Reduced-motion badges without wiggle and normal-motion edit path.
- Pending optimistic value/busy styling, unavailable reasons and disabled actions,
  unknown-not-off toggles; power cycle, timed keep-awake, output and voice intents.
- Online-first peers; unknown backend/last-seen and unknown update counts.
- Chip elapsed/model/children and exact supplied +N overflow; no overflow at zero.
- Partial/unavailable dashboard data, shell home/menu, supplied geometry,
  preset/add/keyboard-move layout intents and timer tabs/ruler/start.

## Deliberate-breakage mutations

`nice -n 10 npm run mutations` exited 0. Every mutant ran independently after a
green baseline, failed the intended named assertion (exit 1), was reverted in
finally, and produced an EMPTY `*.restore.diff` plus an exact byte equality check.
No mutant is installed. JSON reports and SHA-256 restoration hashes are under
`evidence/mutations/`.

| Mutation | Named failing test | Test exit | Restore diff |
|---|---|---:|---|
| locked-disable | launcher-arrange-core rules / locked never disabled, even with disabled input | 1 | empty |
| cancel-keeps-edits | launcher-arrange-core rules / cancel restores the exact snapshot | 1 | empty |
| reenable-first | launcher-arrange-core rules / re-enabled goes last visible, before trailing hidden | 1 | empty |
| incomplete-permutation | launcher-arrange-core rules / commit returns full-permutation order including hidden and enabled diff | 1 | empty |
| ignore-reduced-motion | AppGrid interactions / reduced motion shows badges without wiggle | 1 | empty |
| hide-pending-state | Controls / pending toggle displays optimistic value distinctly from confirmed | 1 | empty |
| overflow-off-by-one | Dashboard and agent chips / chips overflow is supplied core count, includes elapsed model children | 1 | empty |
| ignore-escape | AppGrid interactions / Escape cancels edit changes and restores hidden tile | 1 | empty |

Real runner output ended:

    KILLED locked-disable: launcher-arrange-core rules locked never disabled, even with disabled input · restore diff empty
    KILLED cancel-keeps-edits: launcher-arrange-core rules cancel restores the exact snapshot · restore diff empty
    KILLED reenable-first: launcher-arrange-core rules re-enabled goes last visible, before trailing hidden · restore diff empty
    KILLED incomplete-permutation: launcher-arrange-core rules commit returns full-permutation order including hidden and enabled diff · restore diff empty
    KILLED ignore-reduced-motion: AppGrid interactions reduced motion shows badges without wiggle · restore diff empty
    KILLED hide-pending-state: Controls pending toggle displays optimistic value distinctly from confirmed · restore diff empty
    KILLED overflow-off-by-one: Dashboard and agent chips chips overflow is supplied core count, includes elapsed model children · restore diff empty
    KILLED ignore-escape: AppGrid interactions Escape cancels edit changes and restores hidden tile · restore diff empty
    8/8 mutants killed by named tests; every restore byte-identical

The final green 48-test run occurred AFTER those restored mutations.
Final readback also confirmed all eight restoration hashes still match their
current target files and the owned dev-server port is closed.

## Screenshots and comparison status

Screenshots taken: NONE. No browser/display server was started by this task.

At initial discovery the requested pgrep pattern found the other agent's nested
KWin and several phase gate scripts. All coding, dependency, HTTP and unit checks
were completed without starting a GUI. Rechecked at the end using the screenshot
runner; its actual result was:

    $ nice -n 10 npm run screenshots
    SKIPPED screenshots: other agent GUI gate active
    4024769 /usr/sbin/bash -lic set +m; cd /home/nick/Hermes/Ripple/island-tauri && CLEAN=1 scripts/phase6a-gate.sh > evidence/phase6a/gate-run.log 2>&1; echo "exit=$?" >> evidence/phase6a/gate-run.log
    4024789 bash scripts/phase6a-gate.sh

`evidence/screenshots/status.json` records status UNVERIFIED and those exact
gate processes. One additional final recheck also found the gate's active
`90022 kwin_wayland --virtual --xwayland --socket hi-gui-wayland --width 1600 --height 1000 --no-lockscreen --no-global-shortcuts`;
the status JSON includes that final snapshot. A zero exit means “safe skip”, NOT screenshot acceptance. The planned
engine is existing WebKitGTK 4.1 with PyGObject on private Xvfb; NO engine was
actually used to render the gallery. The capture path itself is UNVERIFIED.
When gates are idle it targets seven component stories in both themes (14 PNGs)
under `evidence/screenshots/{skin,neutral}-{shell,grid,layout,timer,controls,dashboard,chips}.png`.
Those paths are TARGETS, not claims of existing screenshot files.

I viewed all four concepts, but could not view my own screenshots alongside them
because the isolation rule prohibited their capture. Visual comparison remains
open. Known implementation/design differences (NOT pixel-verified findings):

- Launcher uses four columns (three on narrow screens), not the concept's five;
  explicit Hermes and Settings locked tiles, hidden row and keyboard reorder
  controls are added. No Watch/camera-mirror/recent-captures out-of-scope tiles.
- Hermes home placeholder and green illustrative skin replace the concept's
  wallpaper framing. Default tokens still derive from real --skin-* roles when
  the production host supplies them; no user's real palette was read.
- Neutral black retains the concept's black surface, dark cards, large rounded
  corners and semantic icon colours, but keeps legacy OpenRunde/radius/shadow
  tokens rather than claiming identical system typography.
- Timer puts a larger centered countdown below a 30-tick illustration rather
  than a smaller number to the right of the concept's horizontal ruler.
- Dashboard follows provider/spending/now/trend/models/projects/activity cards;
  its heatmap is the core's 7×24 weekday/hour shape, NOT the concept's 13-week
  graphic. Unknown costs/limits and partial data remain explicit.
- Layout preview is a host-supplied fixed miniature, without the concept's full
  desktop settings navigation. Sliders/presets emit changes but do not invent
  geometry. Keyboard move controls are additional visible affordances.
- Controls is intentionally more explicit about pending/unavailable data and
  read-only networking than the concept thumbnails.

## Decisions

1. Standalone Vite package, exact npm pins, no production root mounts/shim edits.
   Toast's sibling-root pattern is reference only; Phase 7 chooses the live mount.
2. Shared legacy skin roles and OpenRunde assets; all components use semantic
   tokens. Neutral black is an opt-in scoped theme. Green gallery skin is labelled
   illustrative, with a separate Legacy fallback selector.
3. Controlled component props; only ephemeral menu/gesture state lives inside.
   The TS arrangement reducer is a gallery fixture, not a second production actor.
4. Rust public-API conformance supplements the requested named cases. It caught
   and corrected a real draft mismatch in hidden-item absolute positioning.
5. Unknown registry entries remain inert and in the full order. Planned Phase 6b
   mock IDs are explicitly not claims that today's seven-ID registry knows them.
6. ToggleState/KeepAwake are accessor/view projections because their pure-core
   state machines are not serde contracts. KeepAwake props never expose cookies.
   Clock/request/cookie ownership stays with Rust on port.
7. Chips consume the core-capped list and core-supplied overflow; no double cap
   or invented count in the component. Network peers are defensively online-first.
8. LayoutEditor has no geometry algorithm. Supplied add-slot rects and keyboard
   source/destination actions expose the integration seam without copying math.
9. Gallery timers are frozen, mutable mock snapshots, not competing timer clocks.
   Core/actor will own real ticking, deadlines, notifications and persistence.
10. Follow the user's explicit safe-skip rule rather than racing Sonnet's GUI gate.
    No screenshot or visual-conformance claim is inferred from HTTP/build/jsdom.

## UNVERIFIED

- All gallery screenshots in both themes and settled visual comparison with the
  concepts; actual WebKitGTK execution/rendering/font loading of this package.
- The WebKitGTK capture path, including snapshot bindings and failure cleanup,
  was not exercised because the concurrent gate remained active.
- Real browser mouse/touch drag-to-reorder and layout dragging; jsdom covers
  reducer + gesture timer/menu/keyboard alternatives, not compositor pointer input.
- Real browser/screen-reader focus behaviour and dynamic OS reduced-motion changes;
  unit tests use accessible roles, focus assertions and a matchMedia fixture.
- Live actor/IPC/registry/service integration, real timers and optimistic timeouts,
  xterm and composer, geometry/ShapeInput/blur, animation frame pacing,
  freezePrototype, HiDPI/multi-monitor/fullscreen. These are Phase 7 integration
  gates, not functionality this presentational package silently claims to own.
- Short-island left/right/bottom collision resolution remains layout/shape-owner
  work; this package renders the provided geometry exactly.

## Phase 7 port work

See README's full checklist. In brief: replace mirrored props with generated
bindings/validated adapters; bootstrap revision-guarded stores; wire IslandIntents
to the actor and atomic registry edits; use Rust Arrangement instead of the gallery
reducer; preserve unknown modules; port real layout-core and feed one native
shape/blur owner including side buttons; clamp short-island collisions; mount the
retained xterm with parse acknowledgements; push palette updates and honour font
preference; complete actual timer/service state; preserve live smoke DOM hooks;
then run the isolated GUI, visual and frame-pacing gates in both themes. No live
switch is authorized or performed here.

## Lessons learned

- Visible order and full permutation are different contracts: hidden entries can
  shift absolute indices when a visible tile moves. Differential core output
  caught a bug that hand-written happy-path expectations missed.
- A null update count is unknown, not zero; a null toggle is unknown, not off.
  Pending must display the requested value AND a distinct pending/busy affordance.
- Consume core overflow once; summing/re-capping already capped chips invents runs.
- Keep geometry math outside presentational components, including add slots.
- Build/HTTP/jsdom success is not visual acceptance. Concurrent-GUI isolation is
  a gate, not a suggestion; report missing evidence rather than manufacturing it.

Usage guard ran at start, during work and before this report. Last real output:
`codex Session: 15% used`, `codex Weekly: 86% used`; neither stop threshold reached.
No skills/memory were modified; lessons live here as requested.

## Round 2

Status: implementation, typecheck, unit tests, audit, build and mutations PASS.
Screenshot gate initially BLOCKED by the concurrent nested KWin; final visual
status is recorded below separately. This is still a standalone component
workshop, not Phase 7 runtime acceptance.

### Scope / files

All authored changes stay in `/home/nick/Hermes/Ripple/island-tauri/ui-staging/`.
No changes to `src-tauri/`, `ui/`, parent `scripts/`, staging Rust crates, SPEC,
parent VERIFICATION or parent manifests. No dependency installation or version
changes; React 19.3.1, TypeScript 5.9.3 and framer-motion 12.34.0 retained.
No commit/push, skills/memory edits, real service calls, user clipboard/audio/
power/brightness access, live-island/data-dir access, or persistent display-env
changes. Read-only research children inspected only contracts; no child wrote code.

Created:

- `src/app-models.ts`: explicit view projections and AppIntents, extended through
  IslandIntents in the existing `src/models.ts`.
- `src/app-mocks.ts`, `src/app-gallery.tsx`, `src/app-styles.css`.
- `src/components/AppPrimitives.tsx`: Empty, ServiceCard, bounded local SafeImage,
  downloads size display.
- `src/components/{CalendarApp,ScratchpadApp,DownloadsApp,VolumeMixer,
  NotificationsView,ClipboardView,NowPlaying,QuickInfo,SettingsModules}.tsx`;
  Brightness is exported from VolumeMixer.tsx.
- `tests/apps.test.tsx`, `tests/artwork.test.ts`;
  `tools/{round2-inspect,wait-screenshots,review-sheets}.py`.
- `evidence/round2/`: real command logs, source provenance/hashes, mutation
  reports/logs/empty restoration diffs, GUI isolation history and visual status.

Changed additively: `src/models.ts`, `src/components/Icon.tsx` (new original
weather/Bluetooth glyphs only), `src/components/index.ts`, `src/gallery.tsx`,
`tests/arrangement.test.ts`, `tests/components.test.tsx` (new required callback
spies only), `tools/{mutations,verify,screenshots}.py`, README and this file.
Round-1 component logic/CSS/tokens/fonts/manifest/lockfile and the Round-1 report
above were not rewritten. Existing arrangement implementation already handled
the requested boundaries correctly: these were test gaps, not code defects.

### Required regressions — done first

Added named tests, then deliberately broke the existing correct implementation,
observed each fail, restored byte-identically and reran the named test file green:

    KILLED move-clamp: launcher-arrange-core rules last visible past end with trailing hidden is a no-op · restore diff empty
    KILLED id-64-rejected: launcher-arrange-core rules accepts exactly 64-byte module id · restore diff empty
    KILLED id-65-accepted: launcher-arrange-core rules rejects exactly 65-byte module id · restore diff empty
    3/3 mutants killed by named tests; every restore byte-identical

    $ nice -n 10 npm test -- --run tests/arrangement.test.ts
    Test Files  1 passed (1)
         Tests  31 passed (31)
    Duration  608ms

The ID tests deliberately use multibyte `é`: 32 copies are accepted, and the
same string plus ASCII `a` is rejected. These are the launcher-arrange 64-byte
boundaries, NOT module-registry's separate 32-byte ASCII-slug restriction.
The clamp regression asserts exact full permutation including trailing hidden
entries and a null unchanged commit, not merely the visible order.

### Executed checks — real output

All builds/tests/verification ran under `nice -n 10`. Foreground timeouts were
180 seconds for individual commands, 300 for the aggregate verifier/mutations,
and 360 for screenshots (runner's own capture deadline is 280 seconds).
No gate status depends on the usage guard; it is an agent procedure only.

`nice -n 10 python3 tools/verify.py --round2` exited 0 after restored mutations:

    $ nice -n 10 python3 tools/generate-contracts.py --check
    Rust conformance: 16 traces, 1216 transitions; checked tests/arrangement-fixtures.json
    exit=0

    $ nice -n 10 npm run typecheck
    npm notice run hermes-island-ui-staging@0.1.0 typecheck
    npm notice run tsc --noEmit
    exit=0

    $ nice -n 10 npm test
    Test Files  4 passed (4)
         Tests  97 passed (97)
    Duration  3.06s
    exit=0

    $ nice -n 10 npm audit --audit-level=low
    found 0 vulnerabilities
    exit=0

    $ nice -n 10 npm run build
    vite v8.3.5 building client environment for production...
    ✓ 436 modules transformed.
    ✓ built in 137ms
    exit=0

    PASS: strict typecheck, unit tests, low-level audit, build, Rust fixture freshness and byte-identical bundled fonts

Full real stdout/stderr and exit statuses: `evidence/round2/{checks.json,
contracts.log,typecheck.log,unit-tests.log,audit.log,build.log}`.

`nice -n 10 python3 tools/round2-inspect.py` exited 0:

    PASS: source provenance recorded for 14 cores + bindings; 20 source files pass no-IPC/IO/timer/colour-literal scan; 22 mutation hashes and empty restore diffs match final files

Source SHA-256 map: `evidence/round2/source-hashes.json`. Static audit is a
supplement to actual typecheck/tests/build, not a claim to prove arbitrary IO
absence. The new views have no native imports, fetch, clipboard API, interval
or hardware commands. Visible focus inherits existing token rules; textarea
gets an explicit token focus outline. Controls are native buttons, selects,
checkboxes, ranges and textarea. No Insert shortcut exists.

### Coverage

- Calendar month/agenda, supplied local labels, all-day multi-day spans, recurring
  marker, subscription read-only/local distinction, unconfigured, stale-last-good,
  empty window, navigation/view/refresh/setup/selection intents; secret extra prop
  never reaches DOM or an anchor.
- Scratchpad pinned-first without mutating input, controlled editor, select/create/
  delete/pin intents, explicit saving/saved/error indicators, empty list/editor.
- Downloads latest-first, size/age/kind icon, name-only Open/Reveal intents,
  in-progress/settling, and a malformed ready `.part` still cannot Open.
- Mixer master/output/per-stream intents, idle stream, tri-state mute, nullable
  volume unknown without a zero-valued slider; unavailable/master absent/empty.
- Brightness display identity, known/unknown values, native range focus and
  change/keyboard-release/pointer-release intents; unavailable/empty displays.
- Notifications app grouping/newest ordering, DND, dismiss/clear/actions,
  passive disabled actions, empty clear disabled, hidden body/image/action-label
  suppression, PNG-only capped image sources (reject remote/file/SVG/oversized).
- Clipboard text/image/files summaries, paused/resume, copy/pin/unpin/delete,
  empty history, malformed concealed mock absent from content AND action IDs.
- Music artwork/title/artist, supplied position/lyrics index, clock stays frozen
  when fake time advances, transport capabilities, unknown position, no player,
  plain/synced/loading/instrumental/unavailable lyrics, band clamping and reduced
  motion remains static across changing spectrum props.
- Weather WMO conditions incl. 97/99 and unmapped 42 generic icon/text; missing
  temperature, charging/low/critical/full/unknown/absent battery, Bluetooth known/
  unknown battery, each unavailable service includes reason.
- Module settings locked terminal/Settings even if metadata misflags both,
  enable intents, full-permutation reorder including inert unknown, settings stub.

### Mutation table

`nice -n 10 npm run mutations` exited 0. Each mutant failed a named non-fuzz test,
was restored in `finally`, and has an empty restore diff plus matching SHA-256.
The first eight Round-1 mutants were rerun too; original evidence is retained.
New/round-2 rows below (full table incl. old eight in `mutations/summary.json`):

| Mutation | Named failing test | Test exit | Restore |
|---|---|---:|---|
| move-clamp | last visible past end with trailing hidden is a no-op | 1 | byte-identical |
| id-64-rejected | accepts exactly 64-byte module id | 1 | byte-identical |
| id-65-accepted | rejects exactly 65-byte module id | 1 | byte-identical |
| concealed-clipboard | concealed mock is never rendered or actionable | 1 | byte-identical |
| unknown-volume-zero | volume None is unknown never zero and mute remains tri-state | 1 | byte-identical |
| part-openable | part files cannot open even if malformed ready state arrives | 1 | byte-identical |
| hidden-notification | hidden notification never renders body image or actions | 1 | byte-identical |
| settings-lock-ignored | locked home and Settings cannot be disabled even if metadata misflags them | 1 | byte-identical |
| equalizer-reduced-ignored | reduced equalizer stays static across changing spectrum props | 1 | byte-identical |
| calendar-secret-leak | calendar never renders secret URL even on malformed extra prop | 1 | byte-identical |
| unmapped-weather-throws | unmapped WMO code uses generic icon and unknown text | 1 | byte-identical |
| saving-shown-saved | save indicator is props-driven saving | 1 | byte-identical |
| passive-action-enabled | passive actions disabled and empty clear disabled | 1 | byte-identical |
| broken-artwork-checksum | artwork fixture PNG chunks have valid checksums and real dimensions | 1 | byte-identical |

Actual runner ending:

    KILLED unmapped-weather-throws: QuickInfo unmapped WMO code uses generic icon and unknown text · restore diff empty
    KILLED saving-shown-saved: ScratchpadApp save indicator is props-driven saving · restore diff empty
    KILLED passive-action-enabled: NotificationsView passive actions disabled and empty clear disabled · restore diff empty
    KILLED broken-artwork-checksum: artwork fixture PNG chunks have valid checksums and real dimensions · restore diff empty
    22/22 mutants killed by named tests; every restore byte-identical

### Decisions / contract provenance

1. Read all four concept images with the vision tool, unchanged. Follow existing
   round-1 rounded-card structure, tokens, OpenRunde, shell and icons. No new npm
   packages needed, so no unverified new version pin.
2. Read required pure-core lib.rs/model.rs files where present. Some definitions
   (ics, scratchpad, notifications) are reexports in different files and could not
   be verified within the requested Rust read scope. Used the allowed read-only
   generated contract instead; no invented claim of exact wire compatibility.
3. Mirrored `island-tauri/ui/ipc/bindings.ts` at mtime
   `2026-10-31 14:24:27.516934734 -0400`, repository HEAD
   `60d4bbb4425df763035ed603a25a4ed7f946e7ae`. That HEAD does not version the
   untracked migration tree: mtime/content SHA in `evidence/round2/provenance.json`
   are the real concurrent-file identity. Recheck bindings in Phase 7.
4. Keep IPC AgendaRow and Scratchpad note fields; add explicit calendar view
   projection for expanded dayKeys/instanceKey/recurrence/local labels. Backend
   currently lacks some of these presentation fields. No secret URL/raw fetch
   detail prop. The stale label must be supplied safely by the adapter. Setup,
   navigation and select are intents; full local-event editing is a later route.
5. Scratchpad save state is a view projection (dirty/error -> saving/saved/error);
   neither component nor gallery invents a debounce timer. Rust owns atomic IO,
   scheduling and production IDs. Fixture picker makes all states inspectable.
6. Mixer core's confirmed muted bool is extended to nullable in the UI adapter
   projection for unknown/unconfirmed state. Unknown mute is mixed+disabled,
   not assumed off. Null volume has no range/zero substitute. Write ceiling 100%
   is deliberate least-surprising behaviour; observed boosts remain displayed.
7. Core brightness percent already includes drag/settle optimism; use it as-is.
   Emit change/end per display, leave throttling/echo guards to Rust. Controlled
   components never mutate prop snapshots.
8. Hidden notification content also suppresses attachments/action labels as a
   privacy-safe extension. Clipboard only models normal summaries; a defensive
   runtime concealed flag check protects the hand-mirrored boundary. No full
   clipboard payloads or automatic image retrieval are rendered.
9. SafeImage only accepts capped inline PNGs. Arbitrary artwork URLs could send
   network requests or read files; the production adapter must supply bounded
   service-owned/proxied image data. The generated 64px orbit artwork uses the
   existing neutral-token palette and is fictional, not real cover art.
10. Lyrics `timeMs`/`activeIndex` follow generated IPC, not the core's unserialized
    `start_ms`/offset types. Selection/offset remain host-owned; position never
    extrapolates inside React. Reduced motion renders fixed-height static bars.
11. QuickInfo deliberately uses core-style PowerSnapshot names/enums, not IPC's
    lowercase state/percent; Phase 7 must map explicitly. Service<T> reasons and
    a supplied alert projection are not claimed generated contracts. Weather
    mapping matches core incl. code 97; unknown codes never throw or imply sun.
12. Registry rows preserve config opaque and do not dump it to DOM. Current known
    modules differ between staging and integration; derive knownness from live
    validated registry on port. Terminal/Settings locks override bad metadata.
13. GUI capture never bypasses Sonnet's exclusion rule. The extended runner uses
    the round-1 WebKitGTK/Xvfb path, not Electron/Playwright or the user's display,
    and writes round-2 evidence separately. Two equal PNGs 250ms apart are the
    settle criterion (bounded retries); actual font load/page errors are checked.

### Screenshot status / visual comparison

Initial invocation at approximately 14:47 EDT safely skipped with exit 0:

    SKIPPED screenshots: other agent GUI gate active
    1495339 kwin_wayland --virtual --xwayland --socket hi-gui-wayland --width 1600 --height 1000 --no-lockscreen --no-global-shortcuts

No GUI was started by that invocation. A skip is NOT screenshot acceptance.
The read-only recheck at 14:52 EDT returned empty. Final capture rerun after
the artwork fix also passed all three immediate exclusion checks with empty
pgrep stdout. The blocker cleared before a 10-minute wait was needed; no need
to consume the rest of the permitted 60-minute wait.

Final `nice -n 10 npm run screenshots` exited 0, actual engine WebKitGTK 2.54.2
(GI WebKit2 API 4.1), private Xvfb 1100×1800, ephemeral profile, private HOME/XDG,
disabled session bus. Output excerpts:

    GUI isolation preflight: pgrep produced no matches
    GUI isolation immediately before Xvfb: pgrep produced no matches
    GUI isolation immediately before WebKitGTK: pgrep produced no matches
    CAPTURED evidence/round2/screenshots/skin-calendar.png
    CAPTURED evidence/round2/screenshots/skin-music.png
    CAPTURED evidence/round2/screenshots/neutral-notifications.png
    CAPTURED evidence/round2/screenshots/neutral-quickinfo-unavailable.png
    PASS: 80 gallery screenshots, both themes

MESA emitted DRI3 acceleration warnings under Xvfb. Capture succeeded, but these
are not GPU/compositor frame-pacing measurements. Every accepted scene required
one visible story, loaded OpenRunde, decoded visible images (`complete` and
`naturalWidth > 0`), no page errors, and two byte-identical PNGs 250ms apart.
No user display/browser was used. No capture/static-server process remained in
the targeted post-run pgrep check.

Artifacts:

- `evidence/round2/screenshots/status.json`: exact 80-path inventory and engine.
- `evidence/round2/screenshots/capture.log`: actual final capture stdout/stderr.
- `evidence/round2/gui-isolation-checks.json`: read-only exclusion history.
- `evidence/round2/review/concepts-and-{skin,neutral}.png`: all four original
  concepts above the new actual component crops. Original concept files unchanged.
- `evidence/round2/review/{skin,neutral}-states-{1,2,3,4,5}.png`: all scene crops.

Viewed the original concepts, all review sheets, selected full screenshots and
the final corrected music/notification captures. Honest visual differences:

- The concepts show a desktop-integrated floating island, wallpaper and round
  external side buttons. These new stories are individual components inside the
  reused shell on a labelled workshop page with a Fixture picker. Side-button
  compositing remains the round-1 shell/layout story and Phase-7 integration.
- Neutral black retains black outer surfaces, near-black rounded cards, quiet
  hierarchy and coloured icons. The illustrative Hermes skin is visibly green
  with lime accents, not the concept's purple/blue wallpaper or exact multicolour
  semantics. It is not a read of the real user's skin.
- New app content is predominantly vertical/list-based (mixer, notifications,
  clipboard, quick info and registry). The concepts mainly demonstrate launcher,
  timer and AI dashboard, so no claim of 1:1 designs for these extra apps is made.
- Mixer/brightness expose output identity, explicit unknown rows and readable
  state labels rather than the concept's tiny compact quick-control thumbnails.
  Volume mixer has a nested section heading; it is deliberately not a toolbar-
  only compact widget. Keyboard order buttons/settings stubs add visual density.
- Calendar's month is a seven-column grid with small event labels and repeated
  multi-day chips, not a contiguous cross-column span ribbon. Agenda provides
  larger supplied time/date/span/recurrence labels. Very narrow calendar labels
  need Phase-7 sizing/zoom acceptance; current tested viewport has no observed
  new-view clipping.
- Scratchpad uses a split pinned-first list/editor and visible save line.
  Notifications/clipboard have explicit privacy/paused/empty/error labels; these
  are additional affordances, not shown in the four concepts.
- Now playing has custom orbit mock artwork, transport, spectrum and a rounded
  lyric card with the active line emphasized. It is a full app view, not the
  concept's tiny “nothing playing” widget in the layout miniature.
- Round-1 retained differences are now visually observable too: launcher has four
  columns rather than five, timer countdown below rather than beside the ruler,
  dashboard uses core 7×24 activity data rather than the concept's 13-week grid,
  layout preview is a fixed supplied miniature rather than full desktop settings.
  Those pieces were not redesigned in this round.

Image-validation lesson from actual visual inspection: the first tiny mock PNG
had a bad IDAT CRC. jsdom saw an img tag, while WebKit showed the fallback icon.
Replaced it with generated valid orbit art; added the named chunk-CRC/dimension
test and deliberate checksum mutation. The final runner also requires decoded
visible images, and final screenshots visibly show artwork/notification attachment.
Both themes and all scenes were recaptured after that fix. Do not infer decoding
from DOM presence alone.

### Phase 7 port / UNVERIFIED

- Replace hand-mirrored props with generated bindings + explicit validated view
  adapters, including calendar local labels/recurrence/instance identity, safe
  stale labels, save/error states, nullable mixer state and battery alerts.
- Seed rev-guarded stores before render; wire typed intents to validated commands
  and Rust registry/actor, not the gallery adapters. Revalidate file membership,
  notification action capability, module locks and IPC payloads server-side.
- Wire media_set_visible, lyrics/equalizer consent and playing/hidden gates;
  fetch/proxy/cap artwork through the safe service path. No frontend timer/audio
  capture is introduced by these pieces.
- Preserve one native shape/blur owner and live DOM/smoke hooks; integrate retained
  xterm, terminal draft/PTY, actor navigation and back/grid. These remain UNVERIFIED.
- Production service integrations, calendar timezone/DST behaviour, local-event
  editor, actual debounced atomic persistence, real playback, real full clipboard
  retrieval, hardware slider response, per-module config forms are UNVERIFIED.
- Browser physical keyboard/screen-reader/touch, dynamic OS reduced-motion,
  HiDPI/multi-monitor, native transparency/blur/shape and frame pacing remain
  Phase 7 acceptance. jsdom focus/change tests do not prove compositor input.
- Final visual status: captured and reviewed in both themes; pixel identity to
  concepts is NOT claimed. Physical interaction/frame pacing/live mounting remain
  UNVERIFIED as above. No Phase 9 switch done.

### Lessons learned

- Test both sides of byte boundaries, with multibyte IDs; a 66-byte rejection
  does not pin the exact 64/65 decision. Keep module-registry and launcher bounds
  distinct rather than “repairing” valid input to the other schema.
- A visible-order assertion misses a last-visible item crossing trailing hidden
  entries. Assert full permutation and unchanged commit to pin no-op semantics.
- Null is unavailable, not zero/off. Unknown sliders should not imply a position
  that the host never observed. Capabilities must disable action buttons too.
- Reduced motion must freeze changes from incoming spectrum props, not merely
  remove a CSS transition; the equalizer mutation pins that distinction.
- Keep model mismatch explicit: core structs, generated IPC and UI projections
  are different contracts. Record concurrent binding provenance and port adapters.
- Build success is not GUI acceptance. Retain safe skips and actual exclusion
  checks rather than borrowing old screenshot artifacts or racing another gate.
- An img tag is not decoded artwork. Validate PNG CRCs and real browser natural
  dimensions; jsdom cannot expose a browser decoder rejecting bad fixture bytes.

Final usage guard: codex Session 27%, Weekly 4%; below both requested thresholds.
`nice -n 10 python3 tools/dev-smoke.py` also passed (actual loopback HTML/module
HTTP 200, owned server terminated; no browser). `git diff --check` passed and
branch readback remained `hermes-island`. No commit/push or skills/memory edits.
