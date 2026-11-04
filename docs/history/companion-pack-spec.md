# Companion pack implementation spec

> Historical: the original pack spec. Obelisk was later removed; see `../notch-companion-characters.md` for the current set.

Five stable IDs: `notchmimic` (Null), `obelisk` (Little Obelisk), `companioncube` (Companion Cube · Portal), `nurgling` (Nurgling), `hollowknight` (Hollow Knight). Preserve Blob default and unknown fallback.

Original vector-like QML renditions in 20 px allocated squares, with static Shape geometry and plain Item translations for movement. Null has gripping hands, ivory eyes and hover teeth; Obelisk has charcoal facets and yellow slit; Cube has corner bumpers, cross panels and pink heart (no eyes); Nurgling has green belly, horns and cheeky teeth; Knight has forked ivory horns, black eye sockets and dark cloak. Appropriate eyes use the brain's bounded gaze without extra Behavior. Fixed character palettes use scheme-responsive edge contrast.

Shared HabitCharacter base: one 125 ms timer, only running during a six-frame (750 ms) burst; triggered by resting blink with 12 second timestamp cooldown. No independent idle timer. A single integer frame drives device-pixel translations; reset immediately when hidden, unattended, motion disabled, happy, sleepy, thinking or destroyed. Each character exposes the same observable frame/running contract for deterministic tests.

Verify all registration layers and Rust persistence, picker accessibility, switching/fallback, layout room at normal width, bounded gaze, hover/sleep, burst duration/change bounds and all cancellation gates. Use scoped offscreen QML test/render harness; save references, logs, PNGs and measured image diffs under `evidence/companion-pack/`. Reference downloads are evidence only, never shipped artwork. Actual-size frames enlarged nearest-neighbor and 4x renders must be visually inspected. No deployment, service restarts, live settings changes, commits or live CPU claims.
