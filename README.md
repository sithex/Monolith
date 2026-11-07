# Monolith

A Dynamic Island for KDE Plasma, built around a Hermes Agent terminal.

Monolith lives in a small notch on the Plasma panel and shows the clock and date. Hover it and it shows you
what's playing. Click it and it grows into a panel with a full Hermes chat on the home page and an app grid
behind it. Click the notch again and it shrinks back. The terminal keeps running while it's collapsed, so a
long agent task survives being put away.

It started as a fork of [Ripple](https://github.com/TopMyster/Ripple), an Electron Dynamic Island. Almost
none of that code is left. The current version runs on Tauri 2 (a Rust core with a React UI in the system
WebKitGTK), and the notch is a native Plasma widget.

## What's in it

- **Hermes terminal.** A real PTY running Hermes (xterm.js on the front end). It picks up your Hermes skin
  colours, and you can set the exact launch command in Settings.
- **Apps.** Now Playing (any MPRIS player, with synced lyrics and a live equalizer), timer, calendar from an ICS
  link, scratchpad, recent downloads, clipboard history, notifications, a volume mixer, brightness, package
  updates, AI usage stats and a small system monitor.
- **Side buttons.** Round buttons along the island's edges that run quick actions: play/pause, Do Not Disturb,
  mic mute, power profile, keep-awake, screenshot, colour picker, or jump straight to an app. A layout editor
  sets where they go.
- **Customisation.** Colour schemes, including a custom one and one that follows your Hermes skin, plus size
  presets, glass/blur strength and notch colours. You can rearrange the launcher iPhone-style: hold a tile and
  drag.

Clipboard history, lyrics and the equalizer are off until you turn them on.

## Platform

Linux only, and really KDE Plasma 6 on Wayland only. Monolith leans on KWin for blur and window placement and on
Plasma for the notch widget, and it reads the desktop through D-Bus, PipeWire and `/proc`. Some parts work on
X11, but nobody tests that. Windows and macOS would each need a port.

Tested on CachyOS/Arch and Fedora 45 KDE. `scripts/install-deps.sh` installs the dependencies on either one;
[instructions.md](instructions.md) has the pacman and dnf lines it runs.

## Getting it running

Short version (the dependencies are listed in [instructions.md](instructions.md)):

```sh
git clone https://github.com/sithex/Monolith.git ~/Hermes/Monolith
cd ~/Hermes/Monolith
scripts/install-deps.sh
cd island-tauri
npm ci
./build.sh
./run.sh --overlay
```

Then install the notch widget and add it to your panel. [instructions.md](instructions.md) walks through
that, plus autostart, updating and the problems you're likely to hit.

## Layout

| Path | What's there |
|---|---|
| `island-tauri/src-tauri/` | Rust core: window, input region, PTY, settings, and every integration (pure parsers and models live beside each integration as `core`/`model` modules) |
| `island-tauri/ui/` | React + Framer Motion front end |
| `island-tauri/native/vblank-fix/` | An `LD_PRELOAD` shim so WebKitGTK animates at your monitor's real refresh rate |
| `island/plasma-anchor/` | The Plasma notch widget (QML) |
| `island/*.py` | Helpers the notch and the theme sync call |
| `docs/history/` | Build logs from the rewrite (historical; paths and runtimes in them are out of date) |

## Tests and secrets hook

See [the test commands and GUI suites](island-tauri/README.md#testing).
Enable the tracked pre-commit hook from the repository root:

```sh
git config core.hooksPath .githooks
bash scripts/check-secrets.sh          # staged changes
bash scripts/check-secrets.sh --all    # every tracked index blob
bash scripts/tests/check-secrets.sh    # isolated hook regression checks
```

The hook reads the complete staged versions of added, copied, renamed, modified and
mode-changed files, selected by `git diff --cached`; unstaged edits cannot hide a staged
secret. `--all` also checks unchanged tracked files. Symlink targets are scanned as text,
without following them. It detects private-key headers, GitHub/OpenAI/Anthropic/Slack/AWS
tokens, `.env` and `.env.*` files, `100.x.y.z` addresses, and absolute Linux/macOS home
paths (including the committer's own paths, so results are consistent across machines).
Reports contain file/line/category and `[REDACTED]`, never matching values. Exit 1 means
findings; exit 2 or another nonzero status means the scan could not complete.

`scripts/secrets-allowlist.txt` contains one POSIX extended regex per line, with blank
lines and `#` comments ignored. Rules match `repository/path:category:matched-value`;
anchor both ends and scope exceptions to exact synthetic fixtures. Exceptions apply
per finding, so an allowed path cannot hide a token on the same line. The staged
allowlist is used too. Do not allowlist real credentials. Historical findings remain
visible to `--all`; they do not block commits that do not change those files.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

Monolith began as a fork of Ripple, which is MIT licensed. Its original notice is kept in
[LICENSE-RIPPLE](LICENSE-RIPPLE) and still covers any code left over from Ripple.
