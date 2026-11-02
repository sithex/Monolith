# Hermes Island on Tauri v2 — build / run / test

Phase 1 skeleton (SPEC.md §8). Commands only; design lives in `SPEC.md`, evidence in `VERIFICATION.md`.

## Embedded terminal launch command

In the live island, open **Settings → General → Terminal → Hermes launch command**.
The field is saved separately from Hermes's own configuration. Editing or saving it never
replaces a running terminal; it is read for each new terminal process. After Hermes exits,
restart the terminal to use the new value, or restart the island.

- **Blank / Reset to built-in:** preserves the original island launch path (the resolved
  Hermes Python interpreter, `-I ~/Hermes/Dropdown/custom_hermes.py chat --tui`).
- `hermes`: use Hermes's configured default interface.
- `hermes --tui`: force the modern terminal interface.
- `hermes --cli`: force the classic interface. Flags are lowercase.
- A custom executable, including a quoted absolute path, may be supplied with arguments.

This is an executable-and-arguments field, **not a shell**: quotes group arguments, but
aliases, pipes, redirects, `$HOME`, `~` and command substitution are not expanded. Use an
absolute path when necessary. No flags are appended to a custom command. Syntax errors are
rejected when saving; a missing/non-executable program produces a launch error. Settings
remain accessible so the command can be corrected or reset. Commands and arguments are
not a secret store; do not put API keys here.

A custom wrapper can have its own side effects. For example, Nick's `hermes-local` restarts
his orb and plays a boot splash; plain `hermes` bypasses his custom banner/orb/voice wrapper.
Neither behaviour is added by the island. Collapse/reopen continues the same PTY regardless
of the saved command.

## Prerequisites (Linux)

Rust ≥ 1.85 (tested 1.99.1), Node + npm, `cargo install tauri-cli --version "^2.12" --locked`
(installs to `~/.cargo/bin`), WebKitGTK 4.1 + GTK 3 dev packages, and for the GUI tests:
`Xvfb`, `xfwm4`, `dbus-daemon`, `xfce4-xfconf` (the tests **fail** instead of skipping when these are missing).

## Everything (clean-build gate)

```sh
CLEAN=1 island-tauri/scripts/phase1-gate.sh
```

Runs typecheck, UI build, `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`,
`cargo test --locked` (unit + Xvfb integration), `cargo tree` checks, and fails if the live
`island-rust` process changed. Omit `CLEAN=1` for an incremental run.

## Individual steps

```sh
cd island-tauri
npm install
npm run typecheck
npm run build:ui                      # must run before cargo: tauri embeds dist/
cd src-tauri
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked                   # 33 unit tests + 3 Xvfb tests (~8 s)
cargo test --locked --test xvfb_shape -- --nocapture --test-threads=1
cargo test export_bindings -- --ignored   # regenerate ui/ipc/bindings.ts after changing contract/ or commands
```

## Running the app (Phase 1)

Phase 1 only runs as an isolated test instance. A live launch is refused until Phase 3
(singleton + status machinery) and `--data-dir` must never be a live directory.

```sh
GDK_BACKEND=x11 src-tauri/target/debug/hermes-island --test-instance --data-dir "$TMPDIR/island-scratch" [--overlay]
```

`--overlay` = a native Plasma pill owns Still (the popup is click-through while still). Debug keys in
the page: `m` cycles still → hover → large, `1`/`2`/`3` select directly. Evidence PNGs from the GUI
tests are written to `island-tauri/evidence/phase1/` (gitignored).
