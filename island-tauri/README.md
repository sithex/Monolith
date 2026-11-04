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
| `src-tauri/tests/` | Integration tests; the `kwin_*` ones run a nested KWin and need a desktop session |
| `ui/island/` | The island UI |
| `ui/legacy/`, `legacy.html`, `spike.html` | Test-harness pages used by the GUI tests |
| `tests/*.test.mjs` | Front-end tests (`npm run test:ui`) |
| `native/vblank-fix/` | `LD_PRELOAD` shim so WebKitGTK animates at the monitor's refresh rate |

Older specs and verification logs are in [`docs/history/`](../docs/history/).
