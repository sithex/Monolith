//! Which package manager the update badge queries. Chosen once from os-release, never from
//! which binaries exist: Fedora's repos ship pacman and Arch's ship dnf5, so a binary alone says
//! nothing about the system's own package manager.

use crate::platform::os_release::Distro;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Arch family (`ID` or `ID_LIKE` contains `arch`, e.g. CachyOS): `checkupdates` + `paru -Qua`.
    Pacman,
    /// Fedora family with a `dnf` on PATH: `dnf check-upgrade --json` + `rpm -q`.
    Dnf,
    /// Nothing is run; the badge shows a calm "not supported" state.
    Unsupported(Unsupported),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsupported {
    /// Neither /etc/os-release nor /usr/lib/os-release could be read.
    NoOsRelease,
    /// `/run/ostree-booted` exists (Kinoite and other image-based systems): packages are
    /// layered with rpm-ostree, which dnf does not see.
    ImageBased,
    /// A Fedora-family system without `dnf` on PATH.
    NoDnf,
    /// Any other distribution.
    OtherDistro,
}

/// Selection order: image-based → unsupported; Arch family → pacman; Fedora family with dnf →
/// dnf; anything else → unsupported. `dnf_resolves` only matters for the Fedora family.
pub fn select_backend(distro: Option<&Distro>, dnf_resolves: bool) -> Backend {
    let Some(distro) = distro else {
        return Backend::Unsupported(Unsupported::NoOsRelease);
    };
    if distro.ostree {
        Backend::Unsupported(Unsupported::ImageBased)
    } else if distro.is_like("arch") {
        Backend::Pacman
    } else if distro.is_like("fedora") {
        if dnf_resolves {
            Backend::Dnf
        } else {
            Backend::Unsupported(Unsupported::NoDnf)
        }
    } else {
        Backend::Unsupported(Unsupported::OtherDistro)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::os_release::parse;

    const ARCH: &str = include_str!("../../../../tests/fixtures/updates/os-release.arch");
    const CACHYOS: &str = include_str!("../../../../tests/fixtures/updates/os-release.cachyos");
    const FEDORA_KDE: &str =
        include_str!("../../../../tests/fixtures/updates/os-release.fedora-kde");
    const KINOITE: &str =
        include_str!("../../../../tests/fixtures/updates/os-release.fedora-kinoite");
    const UBUNTU: &str = include_str!("../../../../tests/fixtures/updates/os-release.ubuntu");

    fn with_ostree(mut d: Distro) -> Distro {
        d.ostree = true;
        d
    }

    #[test]
    fn backend_selection_follows_os_release_not_binaries() {
        use Backend::*;
        for dnf in [false, true] {
            // Arch family is pacman whether or not a dnf binary exists (Arch ships dnf5).
            assert_eq!(select_backend(Some(&parse(ARCH)), dnf), Pacman);
            assert_eq!(select_backend(Some(&parse(CACHYOS)), dnf), Pacman);
            // Ubuntu with dnf installed is still unsupported.
            assert_eq!(
                select_backend(Some(&parse(UBUNTU)), dnf),
                Unsupported(super::Unsupported::OtherDistro)
            );
            // Kinoite: ID=fedora, but image-based.
            assert_eq!(
                select_backend(Some(&with_ostree(parse(KINOITE))), dnf),
                Unsupported(super::Unsupported::ImageBased)
            );
            assert_eq!(
                select_backend(None, dnf),
                Unsupported(super::Unsupported::NoOsRelease)
            );
        }
        assert_eq!(select_backend(Some(&parse(FEDORA_KDE)), true), Dnf);
        assert_eq!(
            select_backend(Some(&parse(FEDORA_KDE)), false),
            Unsupported(super::Unsupported::NoDnf)
        );
        // Fedora derivatives via ID_LIKE.
        assert_eq!(
            select_backend(
                Some(&parse("ID=nobara\nID_LIKE=\"rhel centos fedora\"")),
                true
            ),
            Dnf
        );
        // An ostree Arch-like system is also image-based.
        assert_eq!(
            select_backend(Some(&with_ostree(parse(CACHYOS))), false),
            Unsupported(super::Unsupported::ImageBased)
        );
        // Missing ID defaults to "linux": unsupported.
        assert_eq!(
            select_backend(Some(&parse("NAME=x")), true),
            Unsupported(super::Unsupported::OtherDistro)
        );
    }
}
