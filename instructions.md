# Setting up Monolith

This covers a fresh install on KDE Plasma 6 (Wayland), day-to-day use, updating, and the problems you're likely
to hit.

Supported: Arch/CachyOS and Fedora KDE (Plasma 6, Wayland). Other distros have the same libraries under
different names; `scripts/install-deps.sh` lists what to look for when it doesn't know yours.

## 1. Dependencies

From a clone of the repo (step 2 if you don't have one yet):

```sh
scripts/install-deps.sh --print           # shows the command for your distro, runs nothing
scripts/install-deps.sh                   # runs it (through sudo)
scripts/install-deps.sh --with-optional   # also the per-feature tools in the table below
```

It reads `/etc/os-release` and uses pacman on Arch and Arch-based distros, dnf on Fedora. It shows the
command before running it. The same lines by hand:

```sh
# Arch / CachyOS
sudo pacman -S --needed rust nodejs npm webkit2gtk-4.1 gtk3 gtk-layer-shell base-devel python \
  libayatana-appindicator kpackage qt6-tools

# Fedora KDE
sudo dnf install gcc pkgconf rust cargo nodejs npm webkit2gtk4.1-devel gtk-layer-shell-devel python3 \
  libayatana-appindicator-gtk3 kf6-kpackage qt6-qttools
```

The tray icon needs libayatana-appindicator (or the older libappindicator). Without either the island
quits at start with "Failed to load ayatana-appindicator3 or appindicator3 dynamic library". Fedora KDE
ships `libappindicator-gtk3` by default, and that works too. `kpackage` gives `kpackagetool6` (step 4) and
`qt6-tools` gives the Qt 6 `qdbus` the island uses to find the notch: `qdbus6` on Arch, `qdbus-qt6` on
Fedora.

You also need [Hermes Agent](https://hermes-agent.nousresearch.com) installed and working in a normal
terminal, since that's what the island runs.

Optional extras, each needed by one feature (`--with-optional` installs all of them):

| Arch package | Fedora package | Feature |
|---|---|---|
| `pipewire`, `pipewire-pulse`, `pipewire-audio`, `libpulse` | `pipewire-utils`, `pulseaudio-utils` | Volume mixer, mic mute, equalizer, timer sound (`pactl`, `pw-record`, `pw-play`, `paplay`) |
| `libcanberra`, `sound-theme-freedesktop` | `libcanberra-gtk3`, `sound-theme-freedesktop` | Notification and timer sounds |
| `power-profiles-daemon` | `tuned-ppd` (installed by default) | Power profile button |
| `pacman-contrib` (`checkupdates`), `paru` | nothing (dnf is built in) | Updates app |
| `bluez`, `bluez-utils` | `bluez` | Bluetooth status |
| `libkscreen`, `spectacle`, `fontconfig`, `xdg-utils` | the same names | Screen list for the notch and Settings (`kscreen-doctor`), screenshot tile, font picker (`fc-list`), opening downloads and folders (`xdg-open`). A Plasma desktop usually has these already. |

Updates app: pacman + AUR (paru) on Arch, dnf on Fedora. paru comes from the AUR, so install it yourself.

On Fedora, don't install `power-profiles-daemon`. Fedora KDE uses `tuned-ppd`, which offers the same D-Bus
API, and the two packages conflict: installing one removes the other.

## 2. Where to put it

Clone it anywhere. The commands below use `~/Hermes/Monolith`:

```sh
git clone https://github.com/sithex/Monolith.git ~/Hermes/Monolith
```

Nothing has a path written in: the app finds its folder at runtime, and the notch finds the app through
the link you create in step 4.

## 3. Build

```sh
cd ~/Hermes/Monolith/island-tauri
npm ci
./build.sh
```

`build.sh` builds the UI first, then the Rust release binary. The first build takes a while (several minutes, a
few GB of disk). After that, rebuilds only touch what changed.

The vblank shim ships prebuilt as `native/vblank-fix/libvblank_fix.so`. If it doesn't load on your system,
rebuild it with `native/vblank-fix/build.sh`. Without it the island still works, but WebKitGTK caps its
animations at 60 fps.

## 4. Install the notch

The notch is a Plasma widget. Install it, then add it to the panel where you want the island to grow from:

```sh
kpackagetool6 -t Plasma/Applet -i ~/Hermes/Monolith/island/plasma-anchor
mkdir -p ~/.local/state/monolith
ln -sf ~/Hermes/Monolith/island/anchor-client.py ~/.local/state/monolith/anchor-client.py
```

The link tells the notch where this copy of Monolith lives, so clicking it can start the island the first
time. The island keeps the link up to date on every start, so moving the folder only needs one manual launch.

Right-click the panel, choose **Add or Manage Widgets**, find **Hermes Island Notch** and drag it into place.
Most people put it in the middle of a top panel, in place of the clock. It only works on a horizontal panel.

If you change the QML later, upgrade the installed copy with `-u` instead of `-i`. Plasma sometimes keeps
using the old compiled version until you remove the widget from the panel and add it back.

## 5. First run

```sh
~/Hermes/Monolith/island-tauri/run.sh --overlay
```

Click the notch. The island opens on the Hermes terminal.

With **Settings → General → Terminal** left empty, the island runs `hermes chat --tui` from your `PATH`.
To run something else, enter it there and restart the terminal. A few values that work:

- `hermes --tui` forces the newer interface; `hermes --cli` forces the classic one.
- Any executable path with arguments works too. The field isn't a shell, so `~`, `$HOME`, pipes and aliases
  are passed through literally. Use absolute paths.

Saving the setting doesn't touch a terminal that's already running. It applies the next time one starts.

## 6. Start it on login

Once it's running, right-click the tray icon and tick **Start at login**. That writes the entry below for you
(pointing at wherever this copy lives). To do it by hand, create `~/.config/autostart/hermes-island.desktop`:

```ini
[Desktop Entry]
Type=Application
Name=Hermes Island
Exec="/home/YOU/Hermes/Monolith/island-tauri/run.sh" --overlay
Path=/home/YOU/Hermes/Monolith
Terminal=false
X-KDE-autostart-phase=2
```

Replace `YOU` with your username. Desktop files don't expand `~`.

## Using it

- **Click the notch** to open or collapse the island. Collapsing keeps the terminal and whatever Hermes was
  doing. Quitting the island ends that terminal session.
- **Hover the notch** to tint it; the island doesn't open on hover.
- **The app grid** sits behind the terminal. Hold a tile (or use the `⋯` menu) to enter edit mode, then drag
  tiles to reorder or hide them. The Hermes home tile stays first; Settings can move anywhere after it but
  can't be hidden.
- **Voice picker and mute button** sit on the right of the Hermes page's title row when your Hermes has the
  two small voice helpers. See [docs/voice-controls.md](docs/voice-controls.md) for what they are and a
  prompt that has your own Hermes set them up.
- **Timer** rings when a countdown ends (a short chime for each Pomodoro phase) through PipeWire's
  `pw-play` (or `paplay`) and the freedesktop sound theme. On Fedora those come from `pipewire-utils`
  and `pulseaudio-utils`. Dismissing or resetting the timer stops the ring.
- **Lyrics**: on Now Playing, the quote button opens the lyrics beside the artwork. Click a line to jump
  there; scroll by hand and it stops following the song for a few seconds.
- **Side buttons** are set up in Settings → Layout. A slot only shows up if it fits on the island at its
  current size, so you get more slots on a bigger island.
- **Size** (Compact, Spacious, Bigger or Custom) is in Settings → Layout too. Colours, glass strength and the
  notch look are in Settings → Appearance.

- **The tray icon** (right-click) has the things the island itself can't do: Restart Monolith, Restart
  terminal, Restart Plasma panel (when the notch misbehaves), Open Monolith folder, Open data folder, View log
  (`.island-data-overlay/monolith.log`, previous run in `monolith.log.1`) and Start at login. Show/Hide,
  Do Not Disturb and Quit are there too.

Your settings live in `.island-data-overlay/` inside the repo. That folder is in `.gitignore`, so it never
gets committed. It holds your calendar link and clipboard history, among other things.

## Updating

```sh
cd ~/Hermes/Monolith
git pull
cd island-tauri && npm ci && ./build.sh
```

The build replaces the binary while the old one is still running, so restart the island afterwards: right-click
the tray icon → **Restart Monolith**. Or quit it from the tray icon (or `kill` the PID in `.island-data-overlay/status.json`), wait for it to exit, then run
`run.sh --overlay` again. Starting a second copy while the first is still running does nothing, because only
one instance is allowed.

## Troubleshooting

**Clicking the notch does nothing.** Check that the island is running (`pgrep -a hermes-island`) and that
`~/.local/state/monolith/anchor-client.py` links to your copy (step 4). You can test the bridge on its own:
`python3 island/anchor-client.py pin` should open the island.

**"Cannot bind ingest socket" after a crash or power cut.** A dead socket file was left behind. Make sure no
island is running first, then delete `.island-data-overlay/ingest.sock` and start it again. Don't delete it
while an island is running.

**The terminal shows an error instead of Hermes.** The launch command is wrong or not on `PATH`. Fix it in
Settings → General → Terminal; Settings stays reachable even when the terminal fails. `which hermes` gives
you the absolute path.

**Animations stutter or feel capped at 60 fps.** Check that `run.sh` found the shim: the process should
have `LD_PRELOAD` pointing at `libvblank_fix.so`. Rebuild the shim if it's missing.

**The island ignores blur or sits in the wrong place.** It needs KWin. Check Settings → General → Display
server: Wayland is the default and gets exact placement; X11 runs through XWayland and is the fallback.

## Development

Front-end checks, run from `island-tauri`:

```sh
npm run typecheck
npm run test:ui
```

Notch widget and helper checks, run from the repo root:

```sh
QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input island/qmltests     # Arch
QT_QPA_PLATFORM=offscreen /usr/lib64/qt6/bin/qmltestrunner -input island/qmltests   # Fedora
python3 -m unittest discover -s island/tests
```

Use the full path (or `qmltestrunner-qt6` on Fedora), not plain `qmltestrunner`: on Fedora that's the Qt 5
build, and on Arch it's a wrapper that exits without running anything. `scripts/precheck.sh` finds the right
one by itself. On Fedora the runner is in `qt6-qtdeclarative-devel`, and the widget tests also need
`kf6-kirigami libplasma plasma-workspace plasma5support` (a Plasma desktop has them). On Arch it's in
`qt6-declarative`.

For live UI editing, build a debug binary with `--no-default-features`. That drops Tauri's
`custom-protocol` feature, so the binary loads the page from Vite at `http://localhost:5173` instead of the
copy embedded at build time. Then run `npm run dev:ui` and start that binary with `--overlay --ui island`.
CSS and TSX edits then show up as soon as you save. Vite's hot reload is blocked by the release
content-security policy, so pass this override at build time (dev builds only):

```sh
TAURI_CONFIG='{"app":{"security":{"csp":null,"freezePrototype":false}}}' \
  cargo build --manifest-path src-tauri/Cargo.toml --locked --no-default-features
```

Rust checks, run from `island-tauri/src-tauri`:

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
TMPDIR=/tmp cargo test --locked --lib --test units --test services
```

Some tests need `TMPDIR` set (the units tests panic without it). The service tests start a private D-Bus
and PipeWire: `scripts/install-deps.sh --with-test-deps` installs what they need (`dbus python-gobject
pipewire pipewire-audio wireplumber` on Arch, `dbus-daemon python3-gobject-base pipewire wireplumber
pipewire-utils` on Fedora).

The `kwin_*`, `xvfb_shape` and `notification_monitor` tests drive a real nested KWin or Xvfb session.
They're slower and need those installed; `cargo test --locked` runs everything. Packages: `kwin xorg-xwayland
xorg-server-xvfb xfwm4 dbus python-gobject libei` on Arch, `kwin xorg-x11-server-Xwayland
xorg-x11-server-Xvfb xfwm4 dbus-daemon python3-gobject libei` on Fedora.

`scripts/fedora-container-check.sh` builds the committed tree (`git archive HEAD`) in a fresh Fedora
container and runs the Rust, TypeScript and UI tests there. It needs Docker.

If you change anything under `src-tauri/src/contract/` or the commands, regenerate the TypeScript bindings:

```sh
cargo test export_bindings -- --ignored
```

Tauri embeds the built UI into the binary at compile time. After a UI change, a release build needs
`npm run build:ui` before cargo. `build.sh` does that for you.

`cargo test` builds a large debug tree in `src-tauri/target/debug`. It's safe to delete; it all rebuilds.
Pure tests live in `tests/units/`, service tests in `tests/services/` (one binary each); run one file's
tests with, for example, `cargo test --test units timer_core_pomodoro::`.
