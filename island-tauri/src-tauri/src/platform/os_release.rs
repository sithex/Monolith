//! os-release(5): which distribution this is.
//!
//! [`parse`] is pure and total: it never panics and ignores lines it does not understand.
//! [`Distro::read`] reads `<root>/etc/os-release`, falling back to `<root>/usr/lib/os-release`
//! only when the first is missing or unreadable (the two files are never merged), and notes
//! whether `<root>/run/ostree-booted` exists (Fedora Kinoite and other image-based systems still
//! say `ID=fedora`, so this marker is the only way to tell them apart). Production passes `/`;
//! tests pass a scratch directory.

use std::path::Path;

/// Larger files are not os-release files; refuse rather than read them whole.
const MAX_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Distro {
    /// `ID`, lower-cased; `linux` when the file has no `ID` (os-release(5) default).
    pub id: String,
    /// `ID_LIKE`, split on whitespace, lower-cased, in file order.
    pub id_like: Vec<String>,
    /// `/run/ostree-booted` exists (image-based system: rpm-ostree, bootc).
    pub ostree: bool,
}

impl Distro {
    /// Reads the os-release of the system rooted at `root`. `None` when neither file can be read.
    pub fn read(root: &Path) -> Option<Self> {
        let text = ["etc/os-release", "usr/lib/os-release"]
            .iter()
            .find_map(|rel| read_capped(&root.join(rel)))?;
        let mut distro = parse(&text);
        distro.ostree = root.join("run/ostree-booted").exists();
        Some(distro)
    }

    /// True when `ID` or any `ID_LIKE` entry equals `name` (os-release(5) says to match either).
    pub fn is_like(&self, name: &str) -> bool {
        self.id == name || self.id_like.iter().any(|like| like == name)
    }
}

fn read_capped(path: &Path) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// Parses os-release text. `ostree` is always false here; [`Distro::read`] fills it in.
pub fn parse(text: &str) -> Distro {
    let mut id = None;
    let mut id_like = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let Some(value) = unquote(raw.trim()) else {
            continue;
        };
        match key.trim() {
            "ID" => id = Some(value.trim().to_ascii_lowercase()),
            "ID_LIKE" => {
                id_like = value
                    .split_whitespace()
                    .map(str::to_ascii_lowercase)
                    .collect();
            }
            _ => {}
        }
    }
    Distro {
        id: id
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| "linux".into()),
        id_like,
        ostree: false,
    }
}

/// Shell-style value: bare, `'single'` (literal) or `"double"` (with `\"`, `\\`, `\$`, `` \` ``
/// escapes). `None` for an unterminated quote or text after the closing quote.
fn unquote(raw: &str) -> Option<String> {
    let mut chars = raw.chars();
    match chars.next() {
        None => Some(String::new()),
        Some('\'') => {
            let body = &raw[1..];
            let end = body.find('\'')?;
            body[end + 1..].is_empty().then(|| body[..end].to_string())
        }
        Some('"') => {
            let mut out = String::new();
            let mut escaped = false;
            for (index, c) in raw.char_indices().skip(1) {
                if escaped {
                    if !matches!(c, '"' | '\\' | '$' | '`') {
                        out.push('\\');
                    }
                    out.push(c);
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    return raw[index + 1..].is_empty().then_some(out);
                } else {
                    out.push(c);
                }
            }
            None
        }
        Some(_) => Some(raw.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCH: &str = include_str!("../../tests/fixtures/updates/os-release.arch");
    const CACHYOS: &str = include_str!("../../tests/fixtures/updates/os-release.cachyos");
    const FEDORA_KDE: &str = include_str!("../../tests/fixtures/updates/os-release.fedora-kde");
    const FEDORA_CONTAINER: &str =
        include_str!("../../tests/fixtures/updates/os-release.fedora-container");
    const KINOITE: &str = include_str!("../../tests/fixtures/updates/os-release.fedora-kinoite");
    const UBUNTU: &str = include_str!("../../tests/fixtures/updates/os-release.ubuntu");

    fn d(id: &str, like: &[&str]) -> Distro {
        Distro {
            id: id.into(),
            id_like: like.iter().map(|s| (*s).into()).collect(),
            ostree: false,
        }
    }

    #[test]
    fn os_release_real_samples() {
        assert_eq!(parse(ARCH), d("arch", &[]));
        assert_eq!(parse(CACHYOS), d("cachyos", &["arch"]));
        assert_eq!(parse(FEDORA_KDE), d("fedora", &[]));
        assert_eq!(parse(FEDORA_CONTAINER), d("fedora", &[]));
        // Kinoite says ID=fedora too; only /run/ostree-booted tells it apart.
        assert_eq!(parse(KINOITE), d("fedora", &[]));
        assert_eq!(parse(UBUNTU), d("ubuntu", &["debian"]));
        assert!(parse(CACHYOS).is_like("arch"));
        assert!(!parse(CACHYOS).is_like("fedora"));
        assert!(!parse(UBUNTU).is_like("arch"));
    }

    #[test]
    fn os_release_quoting_comments_and_lists() {
        let text = "# comment\n  # indented comment\n\nNAME=\"x\"\nID='nobara'\nID_LIKE=\"rhel  centos\tfedora\"\n";
        assert_eq!(parse(text), d("nobara", &["rhel", "centos", "fedora"]));
        assert!(parse(text).is_like("fedora"));
        // Later assignments win, like sourcing the file in a shell.
        assert_eq!(parse("ID=arch\nID=fedora\n").id, "fedora");
        // Upper-case values are normalised.
        assert_eq!(parse("ID=Fedora\nID_LIKE=ARCH").id_like, vec!["arch"]);
        // Escapes inside double quotes.
        assert_eq!(unquote(r#""a\"b\\c\$d\`e\x""#).unwrap(), "a\"b\\c$d`e\\x");
        assert_eq!(unquote("''").unwrap(), "");
        assert_eq!(unquote("").unwrap(), "");
    }

    #[test]
    fn os_release_rejects_malformed_values_without_panicking() {
        // Unterminated quotes and trailing junk are ignored, not half-parsed.
        assert_eq!(parse("ID=\"fedora\nID_LIKE=arch").id, "linux");
        assert_eq!(parse("ID=\"fedora\"x").id, "linux");
        assert_eq!(parse("ID='arch").id, "linux");
        assert_eq!(parse("ID='arch'junk").id, "linux");
        assert_eq!(parse("no equals sign\nID=").id, "linux");
        assert_eq!(parse("").id, "linux");
        assert_eq!(parse("ID=\"\\").id, "linux");
        assert_eq!(parse("ID=\"é\"").id, "é");
        for text in [
            "=",
            "==",
            "\"",
            "ID=\"",
            "ID='",
            "\u{0}=\u{0}",
            "ID_LIKE=\"\\\"",
        ] {
            let _ = parse(text);
        }
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("os-release-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn os_release_read_prefers_etc_falls_back_to_usr_lib_and_never_merges() {
        let root = scratch("read");
        assert_eq!(Distro::read(&root), None);
        std::fs::create_dir_all(root.join("usr/lib")).unwrap();
        std::fs::write(root.join("usr/lib/os-release"), "ID=fedora\nID_LIKE=rhel\n").unwrap();
        assert_eq!(Distro::read(&root), Some(d("fedora", &["rhel"])));
        std::fs::create_dir_all(root.join("etc")).unwrap();
        std::fs::write(root.join("etc/os-release"), "ID=arch\n").unwrap();
        // /etc wins outright: no ID_LIKE=rhel leaks in from /usr/lib.
        assert_eq!(Distro::read(&root), Some(d("arch", &[])));
        // Ubuntu-style relative symlink /etc/os-release -> ../usr/lib/os-release.
        std::fs::remove_file(root.join("etc/os-release")).unwrap();
        std::os::unix::fs::symlink("../usr/lib/os-release", root.join("etc/os-release")).unwrap();
        assert_eq!(Distro::read(&root).unwrap().id, "fedora");
        // A dangling /etc link falls back too.
        std::fs::remove_file(root.join("etc/os-release")).unwrap();
        std::os::unix::fs::symlink("missing", root.join("etc/os-release")).unwrap();
        assert_eq!(Distro::read(&root).unwrap().id, "fedora");
        std::fs::create_dir_all(root.join("run")).unwrap();
        std::fs::write(root.join("run/ostree-booted"), "").unwrap();
        assert!(Distro::read(&root).unwrap().ostree);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn os_release_read_refuses_oversize_and_non_utf8_files() {
        let root = scratch("caps");
        std::fs::create_dir_all(root.join("etc")).unwrap();
        std::fs::write(
            root.join("etc/os-release"),
            "ID=arch\n".repeat((MAX_BYTES as usize / 8) + 1),
        )
        .unwrap();
        assert_eq!(Distro::read(&root), None);
        std::fs::write(root.join("etc/os-release"), b"ID=arch\n\xff\n").unwrap();
        assert_eq!(Distro::read(&root), None);
        std::fs::write(root.join("etc/os-release"), "ID=arch\n".repeat(64)).unwrap();
        assert_eq!(Distro::read(&root).unwrap().id, "arch");
        let _ = std::fs::remove_dir_all(&root);
    }
}
