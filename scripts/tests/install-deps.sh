#!/usr/bin/env bash
# Checks for scripts/install-deps.sh against fake os-release files. Nothing is installed: the
# non --print runs go to stub sudo/pacman/dnf/id programs that only record their argv.
set -euo pipefail
script=$(cd "$(dirname "$0")/.." && pwd)/install-deps.sh
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
mkdir "$scratch/bin"
for tool in sudo pacman dnf; do
  printf '#!/bin/sh\necho "%s $*" >>"%s/argv"\n' "$tool" "$scratch" >"$scratch/bin/$tool"
done
# shellcheck disable=SC2016 # expands in the stub, not here
printf '#!/bin/sh\necho "${FAKE_UID:-1000}"\n' >"$scratch/bin/id"
chmod +x "$scratch/bin/"*
checks=0
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
release() { # name, content
  printf '%s\n' "$2" >"$scratch/$1"
}
run() { # os-release name, args... -> stdout+stderr in $out, exit code in $rc
  local file=$1
  shift
  rc=0
  out=$(PATH="$scratch/bin:$PATH" INSTALL_DEPS_OS_RELEASE="$scratch/$file" \
    INSTALL_DEPS_OSTREE_MARKER="$scratch/${OSTREE:-none}" bash "$script" "$@" 2>&1) || rc=$?
}
has() { [[ "$out" == *"$1"* ]] || fail "$2: expected '$1' in: $out"; checks=$((checks + 1)); }
lacks() { [[ "$out" != *"$1"* ]] || fail "$2: unexpected '$1' in: $out"; checks=$((checks + 1)); }
code() { [[ "$rc" == "$1" ]] || fail "$2: exit $rc, wanted $1: $out"; checks=$((checks + 1)); }

release arch 'NAME="Arch Linux"
PRETTY_NAME="Arch Linux"
ID=arch'
release cachyos 'NAME="CachyOS Linux"
PRETTY_NAME="CachyOS"
ID=cachyos
ID_LIKE=arch'
release endeavour "ID='endeavouros'
ID_LIKE='arch'"
release fedora 'NAME="Fedora Linux"
VERSION_ID=45
ID=fedora
PRETTY_NAME="Fedora Linux 45 (KDE Plasma Desktop Edition)"'
release nobara 'ID=nobara
ID_LIKE="rhel centos fedora"'
release ubuntu 'NAME="Ubuntu"
ID=ubuntu
ID_LIKE=debian
PRETTY_NAME="Ubuntu 26.04 LTS"'
# Keys that only end with or contain ID must not match.
release tricky 'VERSION_ID=arch
VARIANT_ID=fedora
ID=gentoo'

arch_base='pacman -S --needed rust nodejs npm webkit2gtk-4.1 gtk3 gtk-layer-shell base-devel python libayatana-appindicator kpackage qt6-tools'
fedora_base='dnf install gcc pkgconf rust cargo nodejs npm webkit2gtk4.1-devel gtk-layer-shell-devel python3 libayatana-appindicator-gtk3 kf6-kpackage qt6-qttools'

for distro in arch cachyos endeavour; do
  run "$distro" --print
  code 0 "$distro print"
  [[ "$out" == "sudo $arch_base" ]] || fail "$distro base line: $out"
  checks=$((checks + 1))
done
run arch --print --with-optional
has 'qt6-tools pipewire pipewire-pulse' 'arch optional'
has 'power-profiles-daemon pacman-contrib' 'arch optional ppd'
has 'paru' 'arch paru note'
run arch --print --with-test-deps
has 'qt6-tools dbus python-gobject pipewire pipewire-audio wireplumber' 'arch tests'
run arch --print --with-optional --with-test-deps --yes
has 'pacman -S --needed --noconfirm rust' 'arch --yes'
[[ $(grep -o ' pipewire-audio ' <<<"$out " | wc -l) == 1 ]] || fail "arch duplicate pipewire-audio: $out"
checks=$((checks + 1))

for distro in fedora nobara; do
  run "$distro" --print
  code 0 "$distro print"
  [[ "$out" == "sudo $fedora_base" ]] || fail "$distro base line: $out"
  checks=$((checks + 1))
done
run fedora --print --with-optional
has 'qt6-qttools pipewire-utils pulseaudio-utils libcanberra-gtk3' 'fedora optional'
lacks 'power-profiles-daemon ' 'fedora never ppd'
has 'tuned-ppd' 'fedora tuned-ppd note'
run fedora --print --with-test-deps
has 'qt6-qttools dbus-daemon python3-gobject-base pipewire wireplumber pipewire-utils' 'fedora tests'
run fedora --print --with-optional --with-test-deps -y
has 'dnf install -y gcc' 'fedora -y'
[[ $(grep -o ' pipewire-utils ' <<<"$out " | wc -l) == 1 ]] || fail "fedora duplicate pipewire-utils: $out"
checks=$((checks + 1))

: >"$scratch/ostree"
OSTREE=ostree run fedora --print
code 1 'kinoite refused'
has 'image-based' 'kinoite message'
lacks 'dnf install' 'kinoite no command'
run ubuntu --print
code 1 'ubuntu refused'
has 'Ubuntu 26.04 LTS (ID=ubuntu ID_LIKE=debian) is not supported' 'ubuntu message'
has 'WebKitGTK 4.1 and gtk-layer-shell' 'ubuntu package hints'
run tricky --print
code 1 'tricky keys refused'
run missing --print
code 1 'missing os-release refused'
run arch --frobnicate
code 2 'unknown option'

# --print never runs anything.
rm -f "$scratch/argv"
run fedora --print --with-optional
[[ ! -e "$scratch/argv" ]] || fail "--print ran: $(cat "$scratch/argv")"
checks=$((checks + 1))
# A real run shows the command, then runs it through sudo as a user...
run fedora --yes
code 0 'fedora real run'
has "Running:" 'shows command first'
[[ $(cat "$scratch/argv") == "sudo dnf install -y ${fedora_base#dnf install }" ]] || fail "user argv: $(cat "$scratch/argv")"
checks=$((checks + 1))
# ...and without sudo as root.
rm -f "$scratch/argv"
FAKE_UID=0 run arch
code 0 'arch root run'
[[ $(cat "$scratch/argv") == "$arch_base" ]] || fail "root argv: $(cat "$scratch/argv")"
checks=$((checks + 1))
FAKE_UID=0 run fedora --print
[[ "$out" == "$fedora_base" ]] || fail "root print: $out"
checks=$((checks + 1))

printf 'install-deps checks passed: %d\n' "$checks"
