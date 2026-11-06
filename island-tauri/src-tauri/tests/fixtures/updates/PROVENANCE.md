# Updates fixtures: where each file came from

All dnf/rpm files are real output from the `fedora:latest` docker image
(`sha256:a7d72ba2745b…`, Fedora 45 Container Image, dnf5 5.4.6 at first, 5.4.7 after the
in-container upgrade), captured on 2026-11-06. Nothing was edited by hand. Commands ran as uid
1000 unless they say otherwise; the runner adds `LC_ALL=C`, docker's default locale is already C.

Container setup (root, inside the throwaway container only):

```sh
docker run -d --name mdistro-updates-fx --cpus=8 --cpu-shares=256 fedora:latest sleep infinity
docker exec mdistro-updates-fx useradd -u 1000 -m u
docker exec mdistro-updates-fx dnf -q makecache
# Force upgrades: install older builds from the `fedora` repo (multilib, epoch, ^, ~, mixed case)
docker exec mdistro-updates-fx dnf -y -q --setopt=install_weak_deps=False --repo fedora install \
  xxd LibRaw bind-libs cldr-emoji-annotation ibus-libs xxhash-libs xxhash-libs.i686 mesa-filesystem
docker exec mdistro-updates-fx dnf -y -q --setopt=install_weak_deps=False downgrade \
  glibc glibc-common glibc-minimal-langpack
docker exec mdistro-updates-fx dnf -y -q --setopt=install_weak_deps=False --repo fedora install glibc.i686 xxhash-libs.x86_64
```

| File | Command | Exit |
|---|---|---|
| `dnf-check-upgrade-27.json` | `docker exec -u 1000 mdistro-updates-fx dnf -q check-upgrade --json` (stderr empty) | 0 |
| `rpm-qf-27.args` | the 27 `name.arch` keys from the JSON above, space separated | – |
| `rpm-qf-27.txt` | `docker exec -u 1000 mdistro-updates-fx rpm -q --qf '%{NAME}.%{ARCH}\t%{EVR}\n' $(cat rpm-qf-27.args)` (stderr empty) | 0 |
| `rpm-qf-missing.txt` | `… rpm -q --qf '%{NAME}.%{ARCH}\t%{EVR}\n' xxd.x86_64 not-a-package.x86_64 glibc.i686 kernel-core.x86_64` (stderr empty: the "not installed" lines are on stdout) | 2 |
| `dnf-check-upgrade-none.json` | after `docker exec mdistro-updates-fx dnf -y -q upgrade`: `docker exec -u 1000 mdistro-updates-fx dnf -q check-upgrade --json` (stdout `{}` + newline, stderr empty) | 0 |
| `dnf-check-upgrade-offline.stderr` | `docker run --rm --name mdistro-updates-offline --network none --cpus=8 --cpu-shares=256 --user 1000:1000 -e HOME=/tmp fedora:latest dnf -q check-upgrade --json` (stdout empty) | 1 |
| `dnf-check-upgrade-cacheonly-nocache.stderr` | same as above with `dnf -q -C check-upgrade --json` (stdout empty) | 1 |
| `dnf-check-upgrade-zeroepoch.json` | local repo `mdistro-local` with a test package built with `Epoch: 0` (1.0 installed with `rpm -i --nosignature`, 1.1 in the repo; `rpm-build` + `createrepo_c` installed in the container): `docker exec -u 1000 mdistro-updates-fx dnf -q check-upgrade --json` | 0 |
| `rpm-qf-zeroepoch.txt` | `docker exec -u 1000 mdistro-updates-fx rpm -q --qf '%{NAME}.%{ARCH}\t%{EVR}\n' mdistro-zeroepoch.noarch` | 0 |

The zero-epoch pair proves the normalisation rule: dnf prints `1.1-1`, rpm prints `0:1.0-1`
for the same package with an explicit `Epoch: 0`. A real epoch (`xxd` 2:, `bind-libs` 32:,
`cldr-emoji-annotation` 1:) is printed by both.

os-release samples:

| File | Source |
|---|---|
| `os-release.arch` | `docker run --rm --name mdistro-updates-arch --cpus=8 --cpu-shares=256 archlinux:latest cat /etc/os-release` |
| `os-release.cachyos` | the CachyOS host's `/etc/os-release` (read only) |
| `os-release.fedora-container` | `docker exec mdistro-updates-fx cat /etc/os-release` |
| `os-release.fedora-kde` | `/usr/lib/os-release` from `fedora-release-identity-kde-desktop-45-14.noarch.rpm` (`dnf download` + `rpm2cpio`, inside the container) |
| `os-release.fedora-kinoite` | `/usr/lib/os-release` from `fedora-release-identity-kinoite-45-14.noarch.rpm` (same way). Note `ID=fedora`: Kinoite is only told apart by `/run/ostree-booted`. |
| `os-release.ubuntu` | `docker run --rm --name mdistro-updates-ubuntu --cpus=8 --cpu-shares=256 ubuntu:latest cat /etc/os-release` |
