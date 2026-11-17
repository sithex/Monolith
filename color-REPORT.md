Implemented the color slice on `feat/aceternity-color`. Every existing ColorSwatch now opens the Apple Colors-style docked panel; the ColorSwatch props and colorPicker store API remain compatible.

This report is inside the authorized writable worktree. The shared report destination named in COMMON-RULES.md is outside the user's ONLY writable tree, so no file was written there.

Files changed:

- `island-tauri/ui/island/components/ColorField.tsx`: panel, palette, tab persistence, recent-colour persistence, RGB/HSV/hex helpers, keyboard handling, pointer capture, conditional EyeDropper, responsive docking and focus restoration.
- `island-tauri/ui/island/components/color-picker.css`: all picker/swatch styling, theme tokens, fixed body, segmented pill, gradients, focus rings and motion preferences.
- `island-tauri/ui/island/polish.css` and `ui/island/tokens/light-tone.css`: removed only the old picker/swatch rules.
- `island-tauri/ui/island/components/controls.css`: moved only color-picker selector clauses/rules into color-picker.css; other controls and all range/switch/checkbox rules are unchanged.
- `island-tauri/ui/island/Island.tsx`: one appended CSS import. The phase7b-loader cannot load CSS, so the component itself has no new relative imports, following the explicit shared-rule fallback.
- `island-tauri/tests/phase7b-color-picker.test.mjs`: 17 new tests for conversions, palette identity, keyboard index math and focus, API compatibility, storage, drag commits, validation, lifecycle, motion, docking, EyeDropper and literal-colour restrictions.
- `island-tauri/tests/phase7b-settings-rows.test.mjs`: added assertions for all three custom seed ColorSwatch props and patch callbacks; retained its ColorSwatch-to-color-swatch fixture mapping.
- `island-tauri/tests/phase7b-nov2c.test.mjs`: extension-region geometry uses the actual exported panel dimensions instead of the old 248 × 312 fixture.

All affected call sites use the replacement through the unchanged shared component; none needed edits:

- `island-tauri/ui/island/NotchControls.tsx:23`: shared swatch factory. `:50` supplies panelClockColor (notch colour), `:53` supplies notchBorderColor.
- `island-tauri/ui/island/SchemePicker.tsx:73`: customAccent, customBackground and customText seed swatches.
- `island-tauri/ui/island/Island.tsx:210`: the existing single ColorPickerPanel mount.
- Repository search found no additional ColorSwatch consumers.

Design and behaviour:

- 280 × 420 px panel; 200 px fixed tab body. Header 24, tabs 32, recent section 35, theme section 35, footer 28, five 8 px gaps, 12 px padding and 1 px border account for the full 420 px. Docking accounts for side controls, flips sides, clamps to the viewport and reduces dimensions with scrolling on small screens.
- Grid | Spectrum | Sliders tab preference persists in `monolith.colorPicker.tab`. The selected pill translates in approximately 200 ms ease-out. Tab content crossfades with 4 px horizontal travel in 150 ms, capped at 180 ms after motion-speed scaling. Keyboard/reduced-motion changes are instant; exiting tab content is inert and aria-hidden. Panel entrance is 200 ms, exit 150 ms, with its origin aimed at the swatch.
- The note's complete 144-cell, 12-column palette is preserved, including duplicates. Index keys avoid duplicate-key warnings; only the first matching cell is checked. Selection has a 2 px ui-text ring plus a contrasting inner ring. Arrows move roving focus, Home/End navigate within a row, and native button Enter/Space activation picks.
- Spectrum retains saturation/value plus hue, including fully dark colours. Captured primary-pointer drags update previews and commit on release; pointer moves do not reread layout. Both Spectrum controls also support keyboard changes.
- RGB rows retain native range keyboard semantics and provide 0–255 numeric fields. Each track varies one channel while retaining the current other channels. Hex validation accepts exactly six hexadecimal digits, with an optional leading hash.
- `monolith.colorPicker.recent` stores at most 12 valid, deduplicated colours, newest first. Commits add automatically; + pins the current preview without a settings write. Theme presets remain separate, with --skin-* roles first. Invalid/unavailable localStorage is handled gracefully.
- Chrome uses ui-raised/ui-surface, text, border, focus, shadow and v-* theme tokens. Only colour content gradients and movable white-ringed swatches contain literal colours. Dark/light standalone previews were inspected using the production React output and CSS in an isolated headless Firefox fixture; all three tabs and a full recent row fit. Temporary fixture files were removed.
- No opacity slider: every setting stores opaque `#rrggbb`. The pipette is rendered only when `'EyeDropper' in window`; unsupported WebKitGTK windows omit it. Cancelled/stale samples cannot change another request.
- Preserved the extension marker, geometry/surface events including exit completion, Escape consumption, close on collapse/page change, reset callback/label, Done, aria semantics and exit animation.

Final gates, run from this worktree's `island-tauri/` after the last source change:

- `npx tsc --noEmit > /tmp/monolith-color-tsc.log 2>&1`: exit 0, no diagnostics.
- `npm run -s test:ui > /tmp/monolith-color-test-ui.log 2>&1`: exit 0; **680 tests, 680 pass, 0 fail**, up from baseline 662 (+18).
- `npm run build:ui > /tmp/monolith-color-build.log 2>&1`: exit 0. Vite emitted its chunk-size advisory; build and UI manifest generation succeeded.
- `git diff --check HEAD~12..HEAD`: clean.

Implementation commits, in order (`git log --oneline HEAD~12..HEAD` before the report commit):

- 8cdeaf0 feat(color): add themed picker chrome and persistent segmented tabs
- 2c9956f feat(color): port Apple palette with accessible roving grid navigation
- d43dc68 feat(color): refine captured spectrum drags and keyboard controls
- d34f559 feat(color): add tinted RGB sliders numeric fields and conversions
- d8822c9 feat(color): persist recent colours beside skin-first theme presets
- 4fd2cd8 feat(color): unify hex validation and preserve reset and Done actions
- 448224f feat(color): offer pipette only when EyeDropper is available
- da1a036 fix(color): clamp taller picker docking and preserve island lifecycle
- ce11864 feat(color): crossfade fixed-height tabs with instant keyboard navigation
- f5f49ad style(color): theme picker surfaces and focus for dark and light tones
- 254a09e test(color): cover picker conversions persistence keyboard and lifecycle
- 1ad213a refactor(color): finish isolating picker styles and reduced-motion presses

UNVERIFIED: the live Tauri/WebKitGTK island, real KWin input/blur regions, real-device pointer capture and live EyeDropper sampling. No live app, release binary, desktop, configuration, deployment or push was touched. Standalone previews verify static appearance, not compositor behaviour or animation timing on the live overlay. No defects found in files outside this slice. Usage after the last source item: codex Session 25%, below the 85% guard.
