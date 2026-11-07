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
  colours, and you can set the exact launch command in Settings. Links that Hermes prints (OSC 8) open in your
  browser; only `http`/`https` links are allowed.
- **Notifications in the notch.** Desktop notifications drop out of the notch as a card, with a timer bar and a
  close button, and can replace KDE's own popups. You pick the sound, globally or per app, with a preview button.
  Agent approval prompts can show up there too.
- **AI Agents.** Two pages:
  - **Live** follows the Hermes session that's running right now: model, context use, cache, the current
    turn, recent tool steps and subagents. It needs the optional
    [`habby-metrics`](hermes-plugin/habby-metrics) Hermes plugin (local only, `install.sh` /
    `uninstall.sh`).
  - **Usage** shows token and cost history, session lists and 5-hour block history.
  - Both pages also read Claude Code (CLI, VS Code and Desktop) sessions, read-only: no message text ever
    leaves its log files. See [docs/agent-live.md](docs/agent-live.md) and
    [docs/claude-code-usage.md](docs/claude-code-usage.md).
- **System monitor.** CPU, memory, GPU and disk, plus a **Processes** tab: per-app CPU, memory and GPU use. You
  can end or force-kill your own processes, after a confirm step.
- **Apps.** Now Playing (any MPRIS player, with synced lyrics and a live equalizer), timer, calendar from an ICS
  link, scratchpad, recent downloads, clipboard history, a volume mixer, brightness, package updates (pacman/paru
  and Fedora's dnf5) and a launcher for your installed desktop apps.
- **Side buttons.** Round buttons along the island's edges that run quick actions: play/pause, Do Not Disturb,
  mic mute, power profile, keep-awake, screenshot, colour picker, open a desktop app, or jump straight to an
  app. A layout editor sets where they go.
- **Notch companions.** A small animated character sits in the notch and watches your pointer. There are
  eleven to choose from: Blob, Isaac, Null, Companion Cube, Nurgling, Hollow Knight, Frog, Necron Warrior,
  Necron Overlord, Canoptek Scarab and the Eye of Sauron.
  [docs/notch-companion-characters.md](docs/notch-companion-characters.md) explains how to add your own.
- **Settings.** Seven sections in an icon rail on the left: General, Notifications, Look, Notch & companion,
  Terminal, Apps & widgets, and Island & backup. A search box at the top finds any setting and jumps to it.
  Every setting has a control, and you can export and import your settings as a backup.
- **Customisation.** Colour schemes, including a custom one and one that follows your Hermes skin, plus size
  presets, glass/blur strength and notch colours. You can rearrange the launcher iPhone-style: hold a tile and
  drag.

Clipboard history, lyrics, the equalizer and the Live metrics plugin are off until you turn them on.

## Platform

Linux only, and really KDE Plasma 6 on Wayland only. Monolith leans on KWin for blur and window placement and on
Plasma for the notch widget, and it reads the desktop through D-Bus, PipeWire and `/proc`. Some parts work on
X11, but nobody tests that. Windows and macOS would each need a port.

Tested on CachyOS/Arch and Fedora 45 KDE. `scripts/install-deps.sh` installs the dependencies on either one;
[instructions.md](instructions.md) has the pacman and dnf lines it runs.

## Getting it running

### Download a release

Each [release](https://github.com/sithex/Monolith/releases) has a prebuilt `hermes-island` binary for
x86_64 Linux. It needs glibc 2.43 or newer (current Arch, CachyOS and Fedora) plus the runtime libraries from
[instructions.md](instructions.md), and it still needs a checkout of the repository at the same tag for
`run.sh`, the notch widget and the helper scripts:

```sh
git clone --branch v5.0.0 https://github.com/sithex/Monolith.git ~/Hermes/Monolith
cd ~/Hermes/Monolith
mkdir -p island-tauri/src-tauri/target/release
tar -xzf ~/Downloads/monolith-v5.0.0-x86_64-linux.tar.gz -C island-tauri/src-tauri/target/release
island-tauri/run.sh --overlay
```

The release notes list the binary's SHA-256 checksum.

### Build it yourself

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

Either way, then install the notch widget and add it to your panel. [instructions.md](instructions.md) walks
through that, plus autostart, updating and the problems you're likely to hit.

## Layout

| Path | What's there |
|---|---|
| `island-tauri/src-tauri/` | Rust core: window, input region, PTY, settings, and every integration (pure parsers and models live beside each integration as `core`/`model` modules) |
| `island-tauri/ui/` | React + Framer Motion front end |
| `island-tauri/native/vblank-fix/` | An `LD_PRELOAD` shim so WebKitGTK animates at your monitor's real refresh rate |
| `island/plasma-anchor/` | The Plasma notch widget (QML) |
| `island/*.py` | Helpers the notch and the theme sync call |
| `hermes-plugin/habby-metrics/` | Optional Hermes plugin that feeds AI Agents → Live (local files only) |
| `docs/` | Feature notes: Agent Live, Claude Code usage, notch companions, voice controls |
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
