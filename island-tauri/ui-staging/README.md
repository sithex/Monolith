# Hermes Island UI staging

Standalone Phase 7 component workshop, not a Tauri runtime. Every application
value is an explicitly fictional fixture. No Tauri imports, IPC, service calls,
real clipboard/audio/power access, storage access or running-island integration.
Only `island-tauri/ui-staging/` is owned by this work.

## Run

From this directory (Node 26 was used):

    nice -n 10 npm ci
    nice -n 10 npm run dev

The gallery is at http://127.0.0.1:5197 (strict port, separate from Tauri's dev URL).

    nice -n 10 npm run typecheck
    nice -n 10 npm test
    nice -n 10 npm audit --audit-level=low
    nice -n 10 npm run build
    nice -n 10 npm run mutations
    nice -n 10 python3 tools/verify.py
    nice -n 10 python3 tools/dev-smoke.py
    nice -n 10 npm run screenshots

`dist/` is the production gallery. The screenshot runner serves it on an ephemeral
loopback port, not a real display. It refuses browser/Xvfb startup if another
agent's KWin or phase gate is active, records UNVERIFIED, and exits without a GUI.
WebKitGTK/PyGObject/Xvfb are existing system tools, not npm-installed dependencies.
Its browser HOME/XDG are private, session bus disabled, profile ephemeral. Do not
bypass its isolation check. Capturing is a separate acceptance gate, not implied
by a successful build or jsdom tests.

Deep links: `/?theme=neutral&story=timer`, `/?theme=skin&story=dashboard`,
`/?theme=skin&skin=legacy&story=grid`. Stories: shell, grid, layout, timer,
controls, dashboard, chips. Default green palette is an illustrative fixture,
NOT a read of the user's actual skin. “Legacy fallback” uses legacy defaults.

## Contents and contracts

- `src/tokens/themes.css`: all colour literals, legacy skin-role fallbacks,
  illustrative gallery palette, Neutral black and bundled OpenRunde definitions.
- `src/styles.css`: token-only visual rules, visible focus, responsive gallery.
- `src/components/`: exported presentational pieces, original inline SVG icons.
- `src/models.ts`: hand-mirrored staging DTOs plus typed `IslandIntents`.
- `src/arrangement.ts`: small pure gallery reducer. Production authority remains
  Rust, not this second reducer.
- `src/mocks.ts`, `src/gallery.tsx`: fictional snapshots and local intent adapters.
- `tests/`: unit/interaction tests and generated Rust differential fixtures.
- `contract-probe/`: development-only public-API consumer of staged Rust crates.
  Its lockfile is private to this package; it writes only its own target folder.
- `tools/`: real checks, deliberate mutations, dev HTTP smoke, isolated screenshots.
- `evidence/`: actual logs, mutation reports, restoration diffs and source hashes.

Mapping:

    launcher-arrange-core → GridItem / ArrangeCommit / reducer
    module-registry → app identity, full-permutation order and enabled diff
    layout-core → Layout / SideButton / SizePreset / Rect
    timer-core → TimerSnapshot (all three independent sub-snapshots)
    quick-toggles-core → PowerTile / Profile; ToggleState accessor projection
    system-tiles-core → Sink / OutputTile / NetworkTile / TailscaleTile / VoiceList
    agent-tracker-core → Chip / supplied overflow from Update
    updates-core → Badge, including unknown counts, error and staleness
    ai-dashboard-core → Dashboard / LimitCard / CostLine / HourBucket

ToggleState is NOT a serde wire type: the props project its `confirmed()` and
`pending()` accessors and display pending value first. The service adapter must
retain Rust request IDs and its 3000 ms timeout. KeepAwakeState is also a
non-serialized state machine: KeepAwakeView is explicitly a cookie-free UI
projection, not an invented IPC contract. Service<T> wraps missing services with
an actual reason. No DTO pretends these projections are generated bindings.

The registry currently knows seven IDs; the gallery also previews planned
Phase 6b modules (timer, controls, calendar, scratchpad, downloads). Their fixture
`known: true` means “implemented in the future UI design”, not “currently known
to module-registry”. The production adapter MUST derive knownness from the
integrated registry. Unknown IDs remain inert and part of the permutation.
Music/clipboard start hidden, matching registry privacy defaults.

## Editing and accessibility

AppGrid state is controlled by its caller. Hold a tile 500 ms or activate ⋯ →
Edit apps to enter. Motion provides a two-dimensional pointer drag; release over
a tile to choose its visible index. Arrow buttons are the keyboard reorder
alternative. Locked tiles can move but cannot be hidden. Hidden apps re-enable
last. Done emits a commit (null if unchanged); Cancel/Escape restore the snapshot.
The gallery wrapper forwards commits through `IslandIntents.arrange`.
Reduced-motion media preference is subscribed to: badges remain, rotation stops.
Pointer travel/cancel/leave aborts a pending hold, and the hold's trailing click
cannot open an app. Unknown modules have no enable action.

LayoutEditor places every rect exactly as supplied. It does no layout-core math.
The miniature is a separate supplied geometry, not an independently computed
layout. Drag onto a + slot to move, or activate ↗ then the destination + with
the keyboard. Round controls edit; + adds; presets and sliders send intents.
The gallery's geometry stays fixed when a preset changes, deliberately exposing
the boundary rather than implementing competing geometry logic in JavaScript.

Timer tabs support arrows/Home/End, and all actionable controls are native
buttons, ranges or selects with visible focus. There are no Insert bindings.
The minute range allows 1..1440; the concept-inspired tick illustration covers
1..30. The large number shows the supplied snapshot, including longer durations.
Timer gallery buttons change snapshots, but do not run clocks, complete phases,
or issue notifications. Controls likewise update/log local mocks only. The
initial mic card deliberately shows a static pending example.

## Rust conformance, without reading other core internals

The required lib.rs/model.rs reads were supplemented with generated rustdoc
public-API descriptions inside this package (no source edits elsewhere). This
exposed accurate public fields for types re-exported from other source files.

    nice -n 10 python3 tools/generate-contracts.py
    nice -n 10 python3 tools/generate-contracts.py --check

The probe calls the actual Rust Arrangement API for 16 deterministic traces,
1216 transitions, and serializes expected visible/hidden state, errors, edit
state and commits. Vitest compares the TS reducer at every step. Fixtures are
real core output, not hand-written “expected” Rust results. Source-path core
changes invalidate the freshness check. Rust is needed only for regeneration /
freshness, not npm build or the browser gallery.

Important: moving an item removes/inserts the actual item. Hidden items keep
relative order, not fixed absolute array indices. An out-and-back visible move
can therefore still change the full permutation when hidden items are crossed.

## Phase 7 port checklist

1. Port these pure components and token CSS behind the existing DOM contract;
   preserve `.island`, `.terminal-wrap`, `data-state`, `data-view` and smoke hooks.
   This package intentionally does NOT claim to implement that live DOM contract.
2. Replace hand-mirrored DTOs with generated bindings or explicit validated view
   adapters. Bootstrap rev-guarded topic stores before rendering, and reconcile
   registry changes into the Rust edit session/snapshot.
3. Wire IslandIntents to the actor/validated commands; keep navigation/mode,
   timer clocks, optimistic IDs/timeouts and module-registry edits authoritative
   in Rust. Apply ArrangeCommit atomically with full permutation plus enabled diff.
4. Feed real layout-core geometry (including missing-slot geometry) to both
   controls and the single native shape owner. Clamp/stack short-island side
   columns so they cannot overlap bottom controls. No new shape/geometry owner.
5. Mount retained xterm in TerminalHome's slot; keep parse-callback acks, PTY
   identity, draft preservation, existing fonts/colours and composer behaviour.
6. Push live --skin-* changes; remove the gallery fixture scope. Theme overrides
   are opt-in. Honour the existing font preference (OpenRunde vs system).
7. Complete dynamic timer ruler/real timer events; wire services to real
   unavailable/pending/error snapshots. No component may infer missing = zero.
8. Run isolated WebKitGTK GUI acceptance: settled screenshots against all four
   concepts in both themes, physical pointer drag/touch, real keyboard focus,
   reduced-motion, HiDPI, native shape/blur and spring frame pacing. Enable
   freezePrototype only after xterm/Motion pass. No Phase 9 live switch here.

See VERIFICATION.md for executed results, mutations, decisions, visual differences
and explicit UNVERIFIED items. Build success does not grant a tomb a heartbeat.

## Round 2 — app views

Additive components: CalendarApp, ScratchpadApp, DownloadsApp, VolumeMixer,
Brightness, NotificationsView, ClipboardView, NowPlaying, QuickInfo,
SettingsModules. Models/intents are in `src/app-models.ts`, fictional snapshots
in `src/app-mocks.ts`, interactive fixture adapters in `src/app-gallery.tsx`,
and token-only additions in `src/app-styles.css`. Round-1 components stay intact
apart from additive Icon glyphs/exports and the extended IslandIntents interface.

New deep-link stories (both `theme=skin` and `theme=neutral`): calendar,
scratchpad, downloads, mixer, brightness, notifications, clipboard, music,
quickinfo, modules. Each has an accessible Fixture selector for its states:

    /?theme=neutral&story=calendar&fixture=stale
    /?theme=skin&story=scratchpad&fixture=error
    /?theme=neutral&story=music&fixture=noPlayer
    /?theme=skin&story=quickinfo&fixture=unavailable

The gallery adapters mutate only in-memory mock snapshots. Scratchpad edits stay
in “saving” until a different fixture is chosen: there is deliberately no fake
save timer. “Copy”, “Open”, “Reveal”, notification actions and setup/refresh only
log typed mock intents; they never touch system services or open a real URL/file.

    nice -n 10 python3 tools/verify.py --round2
    nice -n 10 npm run mutations
    nice -n 10 python3 tools/round2-inspect.py
    nice -n 10 npm run screenshots

Round-2 evidence is separate under `evidence/round2/`; round-1 evidence and the
round-1 verification section are retained. Mutation runner accepts optional
mutation names for focused runs. The default runs the old eight, three new
boundary regressions, ten app-state mutants and a PNG-checksum mutant; every
target restores byte-exact.

The extended screenshot runner keeps the same isolated WebKitGTK/Xvfb path and
gate exclusion checks. It includes every new default story plus error/empty
variants in both themes, requires loaded OpenRunde and one visible story, catches
page errors, and requires two consecutive identical PNGs before accepting a scene.
An active GUI gate produces a safe skip, not visual acceptance.

Round-2 adapter boundaries for Phase 7:

- Calendar: retain IPC AgendaRow fields; supply instance identity, expanded day
  keys, recurrence flag, and preformatted local labels from a validated host
  adapter. No timezone/date conversion in the component. The URL and raw fetch
  error details have no prop field. Subscription setup is an intent, not a URL
  textbox. Local event CRUD remains a Phase-7 detail route after select.
- Scratchpad: project dirty/save errors to saving/saved/error; debounce/atomic
  persistence stays with Rust. The production adapter allocates note IDs.
- Downloads: pass validated service names only. Both state and partial-file
  suffix block Open; the host must independently validate every operation.
- Mixer: map core Sink/Stream identity, not appName-as-key; null volume/mute
  stays unknown. Slider writes are 0–100; boosted observed values are displayed
  but the UI does not offer boost. Brightness passes per-display change/end
  events; throttling, stale echoes and settle time stay in brightness-core.
- Notifications: pass already-sanitized text; hidden additionally suppresses body,
  image and action labels. Passive actions remain disabled. Service snapshots
  and delta revisions are a store concern, not component state.
- Clipboard: only normal ClipSummary objects exist in the type. Rust rejects
  concealed offers before ingestion; the view also drops malformed runtime
  summaries carrying concealed=true. Images/files are labelled previews; full
  content retrieval is an explicit adapter operation, never automatic rendering.
- Media: use IPC positionMs and lyric activeIndex, not a renderer clock. Port
  the core lyric offset/selection in the host. Equalizer is render-only, no
  smoothing/timer/recording, static at reduced motion. Wire media_set_visible
  with enabled/playing privacy gating in Phase 7.
- Images: SafeImage accepts capped data:image/png payloads only (no SVG, remote
  URL, file URL). Supply bounded service-owned/proxied artwork in Phase 7.
- QuickInfo: core weather/power/BlueZ fields use explicit view projections and
  reasons via Service<T>; no missing percentage is turned into zero. Adapt IPC
  lowercase power state/percent to core PowerView state/percentage explicitly.
- Modules: derive knownness/config from the integrated registry; list reordering
  includes unknown IDs; locked terminal/settings cannot disable even with bad
  metadata. Config bodies are deliberately not dumped; settings are typed stubs.

These pieces still do not own the native shape, navigation actor, live DOM smoke
contract, xterm, real services, or Phase 7 acceptance. See Round 2 verification.
