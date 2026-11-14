//! Drop shelf, pure part: strict `file://` URI validation, the URI encoding used for drag-out
//! and the store, and the bounded list model. No filesystem access except through the
//! `stat` callback the caller passes in.
use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Component, Path, PathBuf},
};

/// At most this many URIs per drop (the whole call is refused above it).
pub const MAX_URIS: usize = 64;
/// Longest accepted URI, in bytes.
pub const MAX_URI_BYTES: usize = 4096;
/// The `shelfMaxItems` range.
pub const MAX_ITEMS_RANGE: std::ops::RangeInclusive<usize> = 1..=50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    TooLong,
    NotFileScheme,
    RemoteHost,
    QueryOrFragment,
    BadEscape,
    NotAbsolute,
    ParentDir,
    Nul,
    ControlCharacter,
    Missing,
    Symlink,
    NotFileOrFolder,
}

/// What a path is on disk, as the caller's `stat` (`symlink_metadata`) saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Folder,
    Symlink,
    Other,
}

/// `file:///abs/path` or `file://localhost/abs/path` → the path's exact bytes.
/// Accepts both QUrl's pretty form (spaces and non-ASCII literal) and full percent-encoding.
pub fn parse_uri(uri: &str) -> Result<PathBuf, Reject> {
    if uri.len() > MAX_URI_BYTES {
        return Err(Reject::TooLong);
    }
    let rest = uri
        .get(..7)
        .filter(|s| s.eq_ignore_ascii_case("file://"))
        .map(|_| &uri[7..])
        .ok_or(Reject::NotFileScheme)?;
    let slash = rest.find('/').ok_or(Reject::NotAbsolute)?;
    let host = &rest[..slash];
    if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
        return Err(Reject::RemoteHost);
    }
    let encoded = &rest[slash..];
    if encoded.contains(['?', '#']) {
        return Err(Reject::QueryOrFragment);
    }
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = |b: Option<&u8>| b.and_then(|b| (*b as char).to_digit(16));
            let (Some(hi), Some(lo)) = (hex(bytes.get(i + 1)), hex(bytes.get(i + 2))) else {
                return Err(Reject::BadEscape);
            };
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    check_path_bytes(&out)?;
    Ok(PathBuf::from(OsString::from_vec(out)))
}

/// Absolute, no `..`, no NUL and no control characters (C0, DEL, and C1 in UTF-8 names).
/// Control characters are legal in Linux names; the shelf refuses them anyway (plan rule).
fn check_path_bytes(bytes: &[u8]) -> Result<(), Reject> {
    if bytes.first() != Some(&b'/') {
        return Err(Reject::NotAbsolute);
    }
    if bytes.contains(&0) {
        return Err(Reject::Nul);
    }
    if bytes.iter().any(|b| *b < 0x20 || *b == 0x7f) {
        return Err(Reject::ControlCharacter);
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        if text.chars().any(char::is_control) {
            return Err(Reject::ControlCharacter);
        }
    }
    if Path::new(std::ffi::OsStr::from_bytes(bytes))
        .components()
        .any(|c| c == Component::ParentDir)
    {
        return Err(Reject::ParentDir);
    }
    Ok(())
}

/// The path as a fully percent-encoded `file://` URI (ASCII; lossless for any byte).
pub fn encode_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Validates a URI and checks the thing it names exists and is a plain file or folder.
pub fn accept(uri: &str, stat: impl Fn(&Path) -> Option<Kind>) -> Result<(PathBuf, Kind), Reject> {
    let path = parse_uri(uri)?;
    match stat(&path) {
        None => Err(Reject::Missing),
        Some(Kind::Symlink) => Err(Reject::Symlink),
        Some(Kind::Other) => Err(Reject::NotFileOrFolder),
        Some(kind) => Ok((path, kind)),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub path: PathBuf,
    pub added_ms: u64,
}

/// Newest first, unique paths, never more than `max` items.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shelf {
    items: Vec<Item>,
}

pub fn clamp_max(max: f64) -> usize {
    if !max.is_finite() {
        return 12;
    }
    (max.round() as i64).clamp(
        *MAX_ITEMS_RANGE.start() as i64,
        *MAX_ITEMS_RANGE.end() as i64,
    ) as usize
}

impl Shelf {
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Parks `paths` (already validated), newest first. A path already on the shelf moves to
    /// the front. Returns whether anything changed.
    pub fn add(&mut self, paths: Vec<PathBuf>, now_ms: u64, max: usize) -> bool {
        let before = self.items.clone();
        for path in paths.into_iter().rev() {
            self.items.retain(|i| i.path != path);
            self.items.insert(
                0,
                Item {
                    path,
                    added_ms: now_ms,
                },
            );
        }
        self.trim(max);
        self.items != before
    }

    /// Drops the oldest items past `max`.
    pub fn trim(&mut self, max: usize) -> bool {
        let max = max.clamp(*MAX_ITEMS_RANGE.start(), *MAX_ITEMS_RANGE.end());
        let over = self.items.len() > max;
        self.items.truncate(max);
        over
    }

    pub fn remove(&mut self, path: &Path) -> bool {
        let len = self.items.len();
        self.items.retain(|i| i.path != path);
        self.items.len() != len
    }

    pub fn clear(&mut self) -> bool {
        let had = !self.items.is_empty();
        self.items.clear();
        had
    }

    /// `{"version":1,"items":[{"uri":"file:///…","addedMs":…}]}`: URIs keep any name lossless.
    pub fn to_json(&self) -> String {
        let items = self
            .items
            .iter()
            .map(|i| serde_json::json!({"uri": encode_uri(&i.path), "addedMs": i.added_ms}))
            .collect::<Vec<_>>();
        serde_json::json!({"version": 1, "items": items}).to_string()
    }

    /// Tolerant load: entries that no longer validate are skipped, the cap is applied.
    pub fn from_json(text: &str, max: usize) -> Self {
        let mut shelf = Self::default();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
            return shelf;
        };
        if value["version"] != 1 {
            return shelf;
        }
        for entry in value["items"].as_array().into_iter().flatten() {
            let (Some(uri), Some(added_ms)) = (entry["uri"].as_str(), entry["addedMs"].as_u64())
            else {
                continue;
            };
            if let Ok(path) = parse_uri(uri) {
                if !shelf.items.iter().any(|i| i.path == path) {
                    shelf.items.push(Item { path, added_ms });
                }
            }
        }
        shelf.trim(max);
        shelf
    }
}
