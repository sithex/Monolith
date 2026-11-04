# Companion correction verification

## Result and scope

Implemented the controller-owned `docs/companion-revision-spec.md`. No deployment,
release build, commits, shell/app restarts, user-settings edits, desktop input,
live screenshots, skill/memory writes, or delegated work. Existing
`evidence/companion-pack/` was not used as an output destination. All new captures,
reference images and logs are under `evidence/companion-revision/`.

## Implementation

- **Obelisk removed:** artwork deleted, registry/picker/public Rust variant and
  generated TS member removed. A boundary-only custom `Deserialize` maps the old
  string `obelisk` to `Blob`; resaves serialize `blob`. Tests exercise a real scratch
  settings file, preserve width, accent and companion-enabled values, and verify
  canonical resave. QML explicitly tests `obelisk` alongside unknown/prototype names
  falling back to Blob. Other companion artwork is unchanged.
- **Why not a serde alias:** actually generating bindings with an alias on Blob
  exposed `obelisk` again as a TS union member. The new canonical-choice binding test
  failed against that output (`bindings-red.txt`); the custom input migration fixes
  it without hand-editing generated bindings.
- **Cube:** consumes existing `shownX/shownY` only. No polling, new timer, additional
  Behavior, or moving Shape geometry. Whole-device-pixel translations plus exactly
  five possible angles (-8, -4, 0, 4, 8 degrees); unchanged assignments are skipped.
  The complete cube is constantly scaled to 14px before transformation. Tracking
  and the finite habit share the same capped angle. Hover centers the tilt and
  changes the heart; it does not add another transform layer.
- **Freeze policy:** hold the complete last transform while art/face is hidden,
  monitor unattended, reduced motion enabled, or sleeping. Resume recomputes from
  current brain offsets. Habit cancellation does not sneak in a transform reset
  across those gates. Device-pixel changes recompute only when active.
- **Bounds/tests:** actual brain pointer relay properties (`pointerX`, `pointerY`,
  `pointerAt`) drive left/right/up/down/center. No direct art-offset writes in the
  tracking tests or tracking captures. Duplicate snapped/jitter inputs assert zero
  transform change signals. Every transformed corner is checked across all cardinal
  combinations, habit frames and hover, at scales 1, 1.25, 1.5 and 2. Existing pack
  tests retain finite habit timing, gate cancellation and zero timer ticks after
  unloading. Original Cube fails the new pointer and bounds tests (`qml-red.txt`).
- **Nurgling:** original paths, not an embedded/copy-pasted external asset. Single
  crooked ivory horn, separate broad head and pear-shaped sagging paunch, asymmetrical
  toothy grin, beady 1px eyes, dark navel, two yellow pustules, short feet and clawed
  arms. Hover raises the right arm and shows a tongue; the finite scratch habit moves
  that arm. Gaze and blink remain brain-driven. Picker uses the same silhouette,
  palette, mouth and belly details. New behavior test exercises eye gaze/blink,
  hover and a blink-triggered scratch with settling.

## Actual reference inspected

- Primary product page: https://www.warhammer.com/en-US/shop/Nurglings-2017
- Actual official miniature image fetched and visually inspected:
  https://www.warhammer.com/app/resources/catalog/product/920x950/99129915037_Nurglings01.jpg?fm=webp&w=650&h=670
- Saved original: `evidence/companion-revision/references/official-nurglings.webp`.
- Inspected front miniature detail:
  `references/front-reference-crop.png` and `front-reference-crop-4x.png`, relative
  to that evidence directory. Pillow pixel samples and a 16-color crop quantization
  are in `references/palette.json`.

The photograph contains several different flesh colors and poses. The front bloated
miniature supplies the paunch/head proportions, small eyes and broad, uneven toothy
mouth; the surrounding creatures reinforce the short limbs and crooked horns. The
original miniature rendition deliberately chooses a coherent **olive/putrid** palette
rather than copying the photograph's lighting or every sculpted wound: flesh
`#929d50`, belly `#b3b96a`, outline `#343c25`, teeth `#e2d5aa`, horn `#ded2a2`.
Full-image and cropped-reference visual inspection informed the design. At 20px,
beady eyes and fewer large teeth read better than the rejected generic imp's two
horns and oversized eyes. The first render was refined by enlarging/lightening the
crooked horn and reducing the eyes from 2px to 1px; final sheets and picker were
inspected again at native size and nearest-neighbor enlargement. This remains an
original small-scale interpretation, not a claim to reproduce every miniature detail.

## Executed gates

Heavy commands used foreground user scopes with `MemorySwapMax=0`, `nice -n 15`
and `ionice -c 3`. QML/UI/render scopes used `MemoryMax=3G`; Rust scopes used
`MemoryMax=5G`. No background jobs were started.

From repository root, the QML and render commands were:

```sh
systemd-run --user --scope -p MemoryMax=3G -p MemorySwapMax=0 nice -n 15 ionice -c 3 env QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software /usr/lib/qt6/bin/qmltestrunner -input island/qmltests -o evidence/companion-revision/qml-all.txt,txt
systemd-run --user --scope -p MemoryMax=3G -p MemorySwapMax=0 nice -n 15 ionice -c 3 python island/qmlrenders/render_companions.py --output evidence/companion-revision
systemd-run --user --scope -p MemoryMax=3G -p MemorySwapMax=0 nice -n 15 ionice -c 3 python island/qmlrenders/companion_strips.py --output evidence/companion-revision > evidence/companion-revision/strip-results.txt
```

From `island-tauri/`:

```sh
systemd-run --user --scope -p MemoryMax=3G -p MemorySwapMax=0 nice -n 15 ionice -c 3 sh -c "npm run typecheck > ../evidence/companion-revision/typecheck.txt 2>&1 && npm run test:ui > ../evidence/companion-revision/ui-tests.txt 2>&1"
```

From `island-tauri/src-tauri/`, the following actual Cargo commands ran inside the
5G low-priority foreground scopes (logs are relative to repository root below):

| Command | Result | Evidence log |
|---|---|---|
| `cargo test --lib services::settings::tests -- --test-threads=1` | 8 passed, 0 failed | `rust-settings.txt` |
| `cargo test --lib export_bindings -- --ignored` | 1 passed, 0 failed; regenerated output | `bindings-generate.txt` |
| `cargo test --locked --lib settings -- --test-threads=1` | 25 passed, 0 failed | `rust-all-settings.txt` |
| `cargo test --locked --lib bindings` | 3 passed, 0 failed, 1 ignored (export explicitly run separately) | `bindings-check.txt` |
| `cargo test --lib generated_types_are_clean` | 1 passed, 0 failed | `types-clean.txt` |

Other final gates:

| Gate | Actual result | Evidence |
|---|---|---|
| Complete QML behavior suite | **35 passed, 0 failed** | `qml-all.txt` |
| Frontend typecheck | exit 0 | `typecheck.txt` |
| Frontend UI tests | **170 passed, 0 failed** | `ui-tests.txt` |
| QML offscreen rendering | **4 passed, 0 failed** | `render-log.txt` |
| Frame/pixel assertion script | exit 0 | `strip-results.txt`, `image-diffs.json`, `tracking-diffs.json` |
| `git diff --check` | exit 0 | final terminal verification |
| `rustfmt --edition 2021 --config skip_children=true --check island-tauri/src-tauri/src/contract/settings.rs island-tauri/src-tauri/src/services/settings.rs island-tauri/src-tauri/src/lib.rs` | exit 0 | `rustfmt.txt` |

Qt 6.12.0 software/offscreen rendering was used. Main component compile emits the
usual offscreen Wayland/KDE theme warnings, but passes. A raw rustfmt check recursed
into unrelated pre-existing formatting differences; only touched blocks were corrected
and the final non-recursive touched-file check passes. Qt Test rejects arbitrary
command-line arguments; the new Python `--output` wrapper safely sets the output
property in a temporary harness (with absolute file-URL imports) and cleans it up.
`grabImage().save()` has no boolean return; Python validates actual decoded files
and their differences rather than treating its return value as success.

## Real visual evidence

All paths below are relative to `evidence/companion-revision/`:

- `native-1x.png`: actual 20px images, normal/hover/sleep/habit/settled.
- `strips-1x.png`: real 1x render enlarged with nearest-neighbor, same states.
- `strips-4x.png`: independent 4x render, same states; `native-4x.png` is a small
  inspection sheet, not a substitute for the native 1x capture.
- `timelines-1x.png`, `timelines-4x.png`: real timed six-tick habits and return to rest.
- `cube-tracking-1x-native.png`, `cube-tracking-1x-enlarged.png`,
  `cube-tracking-4x-native.png`, `cube-tracking-4x-enlarged.png`: left, center,
  right, up, down, settled from the brain's pointer input. Native and enlarged 1x
  grids were visually inspected: heart stays centered on the cube, directional
  tilt/translation are visible and corners are intact.
- `picker/previews.png`: rasterized **actual authored SVG** previews on light/dark.
- `renders/*.png`: original individual QML captures.
- `verification-summary.json`: machine-readable parsed gate totals and source paths.

Changed pixels versus center for actual tracking (per-channel tolerance <=1;
**no positional tolerance**, same as the existing software-render harness):

| Scale | Left | Right | Up | Down | Settled |
|---|---:|---:|---:|---:|---:|
| 1x | 202 | 209 | 205 | 199 | **0** |
| 4x | 2779 | 2812 | 2792 | 2727 | **0** |

Nurgling 1x hover changes **54** pixels; its scratch habit changes **39/48** pixels
on alternating timed poses and returns to **0** changed pixels. Cube timed tip changes
**184/196** pixels and returns to **0**. All four remaining pack characters return
exactly to baseline within the harness's bounded channel tolerance at both scales.
The initial larger-eye Nurgling is not the final artifact. Final 1x/4x sheets,
actual 20px grid, picker and timed strips were visually inspected after regeneration.

## Source changes

- `island/plasma-anchor/contents/ui/Companion.qml`
- `island/plasma-anchor/contents/ui/characters/CompanionCube.qml`
- `island/plasma-anchor/contents/ui/characters/Nurgling.qml`
- Deleted `island/plasma-anchor/contents/ui/characters/Obelisk.qml`
- `island-tauri/src-tauri/src/contract/settings.rs`
- `island-tauri/src-tauri/src/services/settings.rs`
- `island-tauri/src-tauri/src/lib.rs` (canonical public binding regression test)
- Regenerated `island-tauri/ui/ipc/bindings.ts`
- `island-tauri/ui/island/companionCharacters.tsx`
- `island-tauri/tests/phase7b-notch-controls.test.mjs`
- `island/qmltests/tst_companion_pack.qml`
- New `island/qmltests/tst_companion_revision.qml`
- `island/qmlrenders/tst_companion_render.qml`
- `island/qmlrenders/companion_strips.py`
- New `island/qmlrenders/render_companions.py`
- New `docs/companion-revision-verification.md` (this report)

The controller-owned specification and previous pack documents/evidence were not modified by the implementer. No implementation blocker remained at handoff.

## Independent audit and deployment

Controller reran the full QML suite (35 passed), frontend typecheck/UI suite (170 passed), and Rust settings tests (25 passed), inspected the actual-size enlarged artwork and pointer-driven Cube grid, and reviewed the core tracking code. Release build passed using `build.sh` under CPUWeight/IOWeight 20, MemoryMax 8G, nice 15 and idle I/O scheduling; log `evidence/companion-revision/deploy-build.log`.

Rollback backup verified at `/home/nick/Hermes/Backups/Monolith/companion-revision-20261104T072502Z/`. Installed QML byte comparisons passed; removed the obsolete installed `Obelisk.qml` too. Restarted Plasma, waited for it to settle, gracefully stopped the old release and launched `monolith-companion-revision-1793777296` through `run.sh`. New PID 1317859 is online with a valid native anchor and three reconciled monitor notches; Companion Cube selection preserved and pointer-relay script loaded. Open produced pinned state and 200 input rectangles; closure initially exceeded the fixed 3-second status read but subsequent readback confirmed idle and zero rectangles. Inspected the real open-overlay screenshot and startup log.

Live Cube verification injected two directional coordinates through the existing D-Bus Pointer relay (not physical mouse movement). Screenshots `live-cube-left.png` and `live-cube-right.png` have different companion pixels within crop bbox `(4, 7, 22, 23)`, and enlarged crops show opposite intact Cube tilts. Restored the relay to the real cursor coordinates after capture. This establishes deployed QML reacts visibly to relay inputs; no physical clicks or pointer movement were used. User selection remains Companion Cube. Screenshots are under `evidence/companion-revision/`. Live Nurgling switching and CPU A/B were not measured; Nurgling visual/behavior coverage remains offscreen.
