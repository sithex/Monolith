# Building notch companion characters

This is a brief for anyone, human or AI, adding or changing a character in the Monolith
notch: the little face at the left of the Plasma notch whose eyes follow your pointer.
The bar is the Blob and Isaac: they look right at real size, move smoothly, and cost
next to nothing. Read it all before writing code; most rules here come from a mistake
that already happened once.

## How it fits together

| Piece | File | Job |
|---|---|---|
| KWin script | `island-tauri/src-tauri/src/integrations/plasma/companion-gaze.qml` | Reads the global pointer (widgets can't on Wayland), sends it at most 20×/s, only while it moves, never while a fullscreen window is focused |
| Relay | `island-tauri/src-tauri/src/integrations/plasma/companion.rs` | D-Bus `local.hermes.Monolith /Companion`: takes `Pointer(x,y)`, re-broadcasts the `Gaze` signal to every notch; loads/unloads the KWin script |
| Brain | `island/plasma-anchor/contents/ui/Companion.qml` | Shared behaviour: gaze, blinks, glances, dozing, thinking look, hover, "is the pointer on my monitor" |
| Characters | `island/plasma-anchor/contents/ui/characters/<Name>.qml` | What it looks like, plus any habit of its own |
| Host | `island/plasma-anchor/contents/ui/PanelPill.qml` | Places the face (x 9, vertically centred, `preferredSize` square), moves the mic mark past it |
| Setting | `island-tauri/src-tauri/src/contract/settings.rs` (`CompanionCharacter`) | `companionCharacter`, default `blob`; rides the appearance payload to the widget |
| Picker | `island-tauri/ui/island/companionCharacters.tsx` | Settings → Notch → Character tiles with SVG previews |
| Tests | `island/qmltests/tst_panel_notch.qml`, `island-tauri/src-tauri/src/services/settings.rs`, `island-tauri/tests/phase7b-notch-controls.test.mjs` | |

### Adding a character: the four places

1. `characters/<Name>.qml`: an `Item` with `property Item face`, and optionally
   `readonly property real preferredSize` (the notch square, default 16; Isaac is 18)
   and `readonly property real gazeScale` (how far the eyes travel; 1 for the Blob, 0.6
   for Isaac's wide-set eyes).
2. Add the name to the `characters` map in `Companion.qml`.
3. Add a variant to `CompanionCharacter` in `contract/settings.rs` (lowercase serde),
   then regenerate the TS bindings: `cd island-tauri/src-tauri && cargo test --locked
   --lib export_bindings -- --ignored`.
4. Add a tile with an SVG preview to `COMPANION_CHARACTERS` in `companionCharacters.tsx`.

Unknown names fall back to the Blob on both sides, so old or hand-edited settings never
break. Keep it that way.

### Character art: pixel sprites are the default for game characters

Small vector `Shape`s at 20 px antialias into grey mush at 1×: the first Cube, Nurgling
and Hollow Knight were drawn that way, read as blobs live, and the Cube's tilt-rotation
smeared it further. Characters from pixel games (and anything that must be recognisable
at 16–20 px) are drawn as **crisp pixel sprites** instead:

- Base the character on `characters/HabitCharacter.qml` and draw with
  `characters/PixelSprite.qml`: text rows plus a colour map, one unantialiased
  `Rectangle` per horizontal run. Every pixel lands on a screen pixel at 1×.
- Design the sprite first as text rows in Python (Pillow, nearest-neighbour preview at
  1× and 8×), iterate until it reads, then paste the rows into QML. Get proportions and
  palette from the real reference sprite.
- Layer it: static body sprite, then small moving sprites on top (pupils, a lid, a
  tongue). Move them only by whole-pixel `x`/`y`; swap a sprite's `rows` only for rare
  states (hover, doze, habit frame). Never rotate or scale pixel art.
- Make it follow the pointer, always. Set `gazeScale: 1`, then quantise with
  `gazePx(gazeX, k, lo, hi)` (whole sprite pixels, clamped) and move pupils by that. A
  character with no eyes turns instead: the Cube slides its front face up to two pixels
  toward the pointer and shows the side/top face it turns away from.
- Habits are `habitFrames` × `habitInterval` stepped bursts drawn from `habitFrame`. Aim
  directional habits at the pointer: set `habitDirection` in `onHabitStarted` and mirror
  the effect sprite with `Scale { xScale: -1 }` around the slot centre.
- Give every state a visible look: pointer tracking (left/right/up/down all differ),
  blink, hover reaction (`objectName: "hoverMark"`), doze, and the habit.
- Picker tiles: render the frames with `island/qmlrenders/render_pixel_companions.py`
  and trace them with `island/qmlrenders/pixel_previews.py`, so the tile is the real art.

### What the brain gives a character (read through `face`)

- `face.size`: side of the square in logical px.
- `face.shownX`, `face.shownY`: eye offset in logical px, already on a quarter-pixel grid.
  Move the eyes by exactly this; lean the head by a fraction (`face.quarter(shownX * .25)`).
- `face.blink` (1 open, ~0 shut, 1.25 wide-eyed on waking) and `face.lid` (1, .42
  dozing, 1.15 wide while Hermes waits). Multiply the eye height by both.
- `face.mood`: `idle | working | waiting | sleeping` (Settings → companionReactive; off =
  always `idle`). Waiting wins over working; both wake it. The brain already shows each
  mood on every character, so a new character works without code: eyes on the user and
  quicker blinks plus an amber `!` (`moodMark`) while waiting, the thinking glance while
  working, half lids plus `z` while dozing (ten minutes with no pointer move and no turn).
  A character may add a static look (the Cube's heart turns amber); `HabitCharacter`
  already holds habits while waiting. Never a new timer or animation for a mood.
- `face.settled`: the pointer hasn't moved for a minute. Constant animation must stop on
  it (Isaac's flow holds a frame); habits still run on the slower idle blinks.
- `face.happy` (pointer on the notch), `face.sleepy`, `face.thinking`, `face.motion`
  (false when the desktop turns animations off), `face.attended` (the pointer is on this
  notch's monitor).
- `face.blinked(resting)` signal after each idle blink; `resting` means the pointer has
  been still for 4 s. This is the hook for habits (Isaac's tear shot).
- `face.quarter(v)`: snap to quarter pixels.
- `face.bodyColor` / `face.eyeColor` follow the notch scheme. A character with fixed
  colours (Isaac's skin) may ignore them.

## Performance rules

Every visual change repaints the whole panel in plasmashell, and KWin composites it.
Drawing the face once costs nothing; redrawing it is the cost. The first version redrew
on every pointer update and cost about +9% of a core in plasmashell and +6% in KWin
while the mouse moved. The same look now costs about 1%.

1. **Change nothing unless a pixel would change.** Snap moving values to the pixel grid
   (quarter px for the gaze, whole *device* px for pixel-art details) and return early
   when the snapped value is unchanged. An invisible change still triggers a redraw.
2. **Compare against the target, not the animated value.** `Companion.retarget()` keeps
   `targetX`/`targetY`. Comparing a new target with a `Behavior`-animated `shownX`
   while a glide is running skips reversals (sleep→wake, glance→back) and leaves the
   glide finishing at the stale target: eyes stuck looking down.
3. **One animation layer.** Don't stack a `Behavior` on the raw value and another on
   the displayed value; small steps land in a single frame, and only real jumps (≥0.75 px:
   a flick, a glance) get a short glide (140 ms).
4. **Bursts, not loops.** Habits are one-off animations of under a second, triggered by
   `blinked(resting)` or a timer (tear shot ~0.5 s). Between bursts nothing runs.
5. **Unavoidable constant animation** (Isaac's tear flow) must be:
   - stepped: about 8 ticks/s, and drawn in whole device pixels so ticks that wouldn't
     move a pixel draw nothing;
   - shared: one timer drives both sides, so it's one redraw per tick;
   - gated: `running: face.attended && face.visible && face.motion && !face.happy &&
     !face.sleepy`.
6. **Rest when the pointer is parked.** Gate constant animation on `!face.settled` too:
   reading with the mouse parked is the common state, and a tick that runs for hours
   there is the steady cost. Measured offscreen (3 notches, pointer parked on one):
   Isaac 10–15 frames/s on the attended notch, ~1.4 frames/s once settled.
7. **Only the attended monitor moves.** The user runs a notch on each of three monitors.
   The brain already freezes everything on monitors the pointer isn't on; any timer or
   animation a character adds must also check `face.attended`. Ungated, every notch
   redraws its own panel and the cost triples.
8. **Don't bind `Shape` paths to moving values.** Shapes re-tessellate on every change.
   Use plain `Rectangle`s for moving parts; keep Shapes static, or bound only to things
   that change rarely (the hover smile), and `visible: false` when hidden.
9. **Nothing animates while hidden** (`visible`), on hover where it doesn't make sense,
   or with animations off (`face.motion`).

## Look and fidelity rules

1. **Design for 100% scale.** The user's monitors are 1×. A 16–18 px face means a
   detail at `size * .05` is under one pixel and *disappears*. This happened: a tear
   "flow" ran correctly in tests but couldn't be seen live. Anything that has to read
   must be at least one whole device pixel (`1 / Screen.devicePixelRatio` logical px)
   with strong contrast.
2. **Work from the real reference.** For a game or mascot character, fetch the actual
   sprite (for Isaac, the wiki's `Character_Isaac_appearance.png`, 28×33) and dump its
   pixels and palette with Pillow. Take proportions (eye spacing, eye size, mouth,
   outline thickness) and exact colours from the dump, not from memory. Isaac's palette:
   skin `#e3c6c5`, rim `#cf9c9b`, outline `#140606`, tears `#9cc8cc` / edge `#68989c`.
3. **Keep parts attached.** Anything that belongs to the eyes (tear streams) follows
   `shownX` / `shownY` so it doesn't detach when the character looks sideways. Clamp it
   so it stays inside the head.
4. **Stay out of the clock's way.** Projectiles and effects stay within the notch, left
   of the clock (the right-hand gap is about 15 px at 18 px face size), or at the rounded
   left edge. The pill clips its children.
5. **Character traits come first.** Ask how the character is usually shown. Isaac cries
   *all the time*, so the streams are permanent and the idle habit is something else
   (shooting a tear, his attack in the game). Hover cheers him up: the tears dry and he
   smiles.
6. Draw original renditions with shapes; don't paste copyrighted art into the repo.

## QML pitfalls that already bit

- A signal handler can run **before** the bindings that depend on the same property
  update. In `onPointerAtChanged`, test the pointer directly (`pointerHere(x, y)`)
  instead of reading the `attended` binding, or the first move onto a monitor is lost.
- Loader: pass `face` with `setSource(url, { face: face })`. Inline `component` blocks
  that reference outer ids don't resolve; use plain items driven by properties on the
  character's root.
- `PanelPill.hovered` is read-only; tests set `face.happy` instead.
- The offscreen test screen is small: tests set `face.screenArea = Qt.rect(-1e5, -1e5,
  2e5, 2e5)` so far-away pointers still count as "on this monitor".
- Kirigami icons (the mic mark) don't render under `qml6` offscreen; that's expected.

## Verifying: show it, don't assume it

1. **Tests:** add QML tests for every behaviour: eyes follow and stay inside the head;
   the character switches and falls back; habits start and *finish* (nothing left
   running); constant animations tick at the intended rate and stop when hovered,
   unattended or dozing; redraw counts stay bounded (connect to `...Changed` signals and
   count).
   For pixel companions `island/qmltests/tst_companion_revision.qml` is the pattern:
   drive real pointer positions through the brain and compare a *picture signature*
   (every visible run's rect and colour) — left/right/down must differ from centre,
   sub-pixel jitter and other-monitor moves must not change it, every run stays
   whole-pixel, unantialiased and inside the 20×20 slot in every gaze × habit × hover
   state, and a real burst changes the picture then settles back exactly. Testing only
   that a brain offset is clamped does not prove the art follows the mouse.
2. **Offscreen renders:** `python island/qmlrenders/render_pixel_companions.py --output
   <dir>` writes every look/hover/sleep/habit frame and 1×/8× contact sheets. Look at
   the sheet critically before claiming anything. at **1×** (the real size) zoomed with nearest-neighbour, plus
   4× for detail. Grab several frames in a row and diff them with Pillow
   (`ImageChops.difference(a, b).getbbox()`) to prove things move.
3. **Live:** install, then screenshot the real desktop several times and diff the notch
   region. Look up the notch position with `kscreen-doctor -o` and the status file; the
   full screenshot spans all monitors. Move the pointer *off* the notch first (hover
   dries Isaac's tears) and onto the right monitor, then restore it afterwards.
4. **Measure, honestly.** Compare A/B by swapping only the installed QML (e.g. the flow
   off vs on), restarting plasmashell, nudging the pointer so the notch knows where it
   is, then sampling `/proc/<pid>/stat` CPU of plasmashell and kwin_wayland over 25–30 s,
   at least twice each, alternating. Background activity on this desktop swings ±3–5%,
   bigger than a well-made animation. Report ranges and say when the result is within
   noise; don't present one noisy run as fact.

## Gates and deployment

From `island-tauri/` (run heavy builds with `nice -n 15 ionice -c3`):

```sh
npm run -s typecheck && npm run -s test:ui
cd src-tauri && cargo test --locked --lib -j4 \
  && cargo clippy --locked --lib --tests -j4 -- -D warnings
rustfmt --edition 2021 --check <touched .rs files>   # plain `cargo fmt` rewrites unrelated files
cd ../../island && QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input qmltests
```

- **QML-only change** (a character's drawing or habits): copy
  `island/plasma-anchor/.` to `~/.local/share/plasma/plasmoids/local.hermes.island.notch3/`,
  then `systemctl --user restart plasma-plasmashell.service`. No build needed. Then
  check that `qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.isScriptLoaded
  monolith-companion-gaze` is still `true`.
- **Setting, contract or Rust change:** also `./build.sh`, then restart the island (stop
  the pid in `.island-data-overlay/status.json`, relaunch with `systemd-run --user
  --unit=monolith-companion-$(date +%s) --collect island-tauri/run.sh --overlay --ui
  island`).
- Commit the source too. The installed widget is overwritten from the repo on the next
  install.

## Prompt to hand another AI

> Add a notch companion character "<Name>" to Monolith. Read
> `docs/notch-companion-characters.md` first and follow it exactly. Reference art: <link>.
> The character's signature traits: <traits>. Hover reaction: <reaction>. Idle habit
> (one-off burst): <habit>. Constant animation, if any: <describe>, stepped and gated per
> the rules. Deliver: the four-place change, QML tests (including bounded redraws and
> cleanup), 1× and 4× offscreen frame strips with pixel diffs, all gates green, a live
> screenshot diff after installing, and an honest CPU A/B. Report what you measured, not
> what you expect.
