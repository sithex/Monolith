#!/usr/bin/env bash
# Install what Monolith needs to build and run, with your distro's package manager.
#
#   scripts/install-deps.sh                  build + core runtime packages
#   scripts/install-deps.sh --with-optional  also the per-feature tools (mixer, sounds, screenshot...)
#   scripts/install-deps.sh --with-test-deps also what the Rust tests need (dbus-daemon, PyGObject, PipeWire)
#   scripts/install-deps.sh --print          only print the command; run nothing
#   scripts/install-deps.sh --yes            pass the package manager's non-interactive flag
#
# Supported: Arch and Arch-based (CachyOS, EndeavourOS...) with pacman, Fedora with dnf. The distro
# comes from /etc/os-release (else /usr/lib/os-release), ID and ID_LIKE. The command is always
# shown before it runs; it runs through sudo unless you are already root.
set -euo pipefail

print=0 optional=0 tests=0 yes=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --print) print=1 ;;
    --with-optional) optional=1 ;;
    --with-test-deps) tests=1 ;;
    --yes | -y) yes=1 ;;
    -h | --help) sed -n '2,13p' "$0"; exit 0 ;;
    *) printf 'install-deps: unknown option %s (see --help)\n' "$1" >&2; exit 2 ;;
  esac
  shift
done

# Overridable for tests only.
os_release=${INSTALL_DEPS_OS_RELEASE:-}
ostree_marker=${INSTALL_DEPS_OSTREE_MARKER:-/run/ostree-booted}
if [[ -z "$os_release" ]]; then
  if [[ -r /etc/os-release ]]; then os_release=/etc/os-release; else os_release=/usr/lib/os-release; fi
fi

# Read one key without sourcing the file: values may be quoted or not.
os_value() {
  local key=$1 line value
  [[ -r "$os_release" ]] || return 0
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ "$line" == "$key="* ]] || continue
    value=${line#"$key="}
    value=${value%\"}; value=${value#\"}
    value=${value%\'}; value=${value#\'}
    printf '%s' "$value"
    return 0
  done <"$os_release"
}

id=$(os_value ID)
id_like=$(os_value ID_LIKE)
name=$(os_value PRETTY_NAME)
family=""
read -r -a ids <<<"$id $id_like"
for word in "${ids[@]}"; do
  case "$word" in
    arch) family=arch; break ;;
    fedora) family=fedora; break ;;
  esac
done

# Package lists. Keep instructions.md in step with these.
arch_base=(rust nodejs npm webkit2gtk-4.1 gtk3 gtk-layer-shell base-devel python
  libayatana-appindicator kpackage qt6-tools)
arch_optional=(pipewire pipewire-pulse pipewire-audio libpulse libcanberra sound-theme-freedesktop
  libkscreen spectacle bluez bluez-utils fontconfig xdg-utils power-profiles-daemon pacman-contrib)
arch_tests=(dbus python-gobject pipewire pipewire-audio wireplumber)

fedora_base=(gcc pkgconf rust cargo nodejs npm webkit2gtk4.1-devel gtk-layer-shell-devel python3
  libayatana-appindicator-gtk3 kf6-kpackage qt6-qttools)
# No power-profiles-daemon: Fedora KDE ships tuned-ppd (same D-Bus API) and the two conflict.
fedora_optional=(pipewire-utils pulseaudio-utils libcanberra-gtk3 sound-theme-freedesktop
  libkscreen spectacle bluez fontconfig xdg-utils)
fedora_tests=(dbus-daemon python3-gobject-base pipewire wireplumber pipewire-utils)

refuse() {
  printf 'install-deps: %s\n' "$1" >&2
  cat >&2 <<'EOF'

Monolith is tested on Arch/CachyOS and Fedora KDE (Plasma 6, Wayland). On another distro,
install the equivalents of these by hand:
  build:   Rust + cargo, Node.js + npm, a C compiler, pkg-config,
           WebKitGTK 4.1 and gtk-layer-shell (with their development headers)
  runtime: python3, libayatana-appindicator3 (or libappindicator3), kpackagetool6 (KF6 KPackage),
           Qt 6 qdbus (Qt 6 tools)
  extras:  pactl, pw-record/pw-play (PipeWire), canberra-gtk-play, freedesktop sound theme,
           kscreen-doctor, spectacle, bluez, fontconfig, xdg-utils, power profiles daemon
EOF
  exit 1
}

if [[ -e "$ostree_marker" ]]; then
  refuse "${name:-this system} is image-based (rpm-ostree); install the packages in a toolbox or layer them yourself."
fi
case "$family" in
  arch)
    packages=("${arch_base[@]}")
    [[ "$optional" == 1 ]] && packages+=("${arch_optional[@]}")
    [[ "$tests" == 1 ]] && packages+=("${arch_tests[@]}")
    command=(pacman -S --needed)
    [[ "$yes" == 1 ]] && command+=(--noconfirm)
    ;;
  fedora)
    packages=("${fedora_base[@]}")
    [[ "$optional" == 1 ]] && packages+=("${fedora_optional[@]}")
    [[ "$tests" == 1 ]] && packages+=("${fedora_tests[@]}")
    command=(dnf install)
    [[ "$yes" == 1 ]] && command+=(-y)
    ;;
  *)
    refuse "${name:-${id:-this distro}} (ID=${id:-?}${id_like:+ ID_LIKE=$id_like}) is not supported by this script." ;;
esac

# Drop duplicates (pipewire is in both the optional and test sets), keeping the order.
unique=()
for package in "${packages[@]}"; do
  [[ " ${unique[*]} " == *" $package "* ]] || unique+=("$package")
done
command+=("${unique[@]}")
if [[ "$(id -u)" != 0 ]]; then command=(sudo "${command[@]}"); fi

notes() {
  [[ "$optional" == 1 ]] || return 0
  if [[ "$family" == fedora ]]; then
    echo "Note: the power profile button uses tuned-ppd, which Fedora KDE installs by default. Don't install power-profiles-daemon; it conflicts with tuned-ppd."
    echo "Note: the Updates app uses dnf, which Fedora already has."
  else
    echo "Note: the Updates app's AUR count needs paru, which comes from the AUR, not pacman."
  fi
}

show() { printf '%q' "$1"; shift; printf ' %q' "$@"; printf '\n'; }
if [[ "$print" == 1 ]]; then
  show "${command[@]}"
  notes
  exit 0
fi
printf 'install-deps: %s detected (%s). Running:\n  ' "${name:-$id}" "$family"
show "${command[@]}"
"${command[@]}"
notes
