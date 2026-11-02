//! Pure core behind SPEC §0.5 "Design direction" (Apps bullet) and §8
//! "Phase 6b" — the Downloads app for the Hermes Island widget: recent files
//! in the XDG download dir via directory notifications, with open/reveal.
//!
//! This crate decides **what the UI shows**. It takes scan results and change
//! events — the "what happened" facts produced by the Tauri host's inotify
//! watcher and `stat` calls — and turns them into the ordered, settled list of
//! [`DownloadItem`]s the UI renders. The host performs all filesystem work;
//! this crate performs none of it.
//!
//! Platform-free: no IO, no network, no threads, no clock, no `unsafe`, and
//! it never panics on any input. All times are injected `u64` milliseconds:
//! `now` (passed to [`DownloadsModel::apply`], [`DownloadsModel::tick`] and
//! [`DownloadsModel::items`]) is a monotonic counter used for settling, while
//! [`FileStat::mtime_ms`] is a wall-clock value taken from `stat`. Wire types
//! use camelCase through serde and accept no aliases.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A single directory entry as reported by the watcher or scanner.
///
/// `name` is a single path component; the watcher reports names exactly as
/// they appear in the directory. `mtime_ms` is a wall-clock timestamp taken
/// from `stat`; `size_bytes` is the current file length in bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    pub name: String,
    pub size_bytes: u64,
    pub mtime_ms: u64,
    pub is_dir: bool,
    pub is_symlink: bool,
}

/// A change event produced by the host's inotify layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DirEvent {
    /// A file appeared or its metadata changed; carries the fresh state.
    Upsert(FileStat),
    /// A file disappeared.
    Removed { name: String },
    /// A file was renamed. `to` carries the fresh state of the new name.
    Renamed { from: String, to: FileStat },
    /// Full replacement after an inotify overflow or watch re-arm.
    Rescan(Vec<FileStat>),
}

/// Kind of a downloaded file, derived from its final extension.
///
/// Extensions are matched case-insensitively. Multi-part extensions (for
/// example `.pkg.tar.zst`) are classified as a whole before single
/// extensions. A name with no dot, or whose only dot is the leading hidden
/// dot, is [`FileKind::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    /// zip tar gz tgz xz zst 7z rar bz2 (plus `.tar.*` composites)
    Archive,
    /// png jpg jpeg gif webp svg avif heic
    Image,
    /// mp4 mkv webm mov avi
    Video,
    /// mp3 flac ogg opus wav m4a
    Audio,
    /// pdf doc docx odt txt md rtf epub
    Document,
    /// deb rpm pkg.tar.zst AppImage flatpakref
    Package,
    /// rs py js ts json toml yaml sh
    Code,
    /// Anything else
    Other,
}

/// Lifecycle state of a tracked file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemState {
    /// Filename carries an in-progress suffix (`.part`, `.crdownload`, ...).
    InProgress,
    /// Final file whose size/mtime must remain stable for `settle_ms`.
    Settling,
    /// Settled, shown as complete.
    Ready,
}

/// One row of the UI list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadItem {
    pub name: String,
    pub size_bytes: u64,
    pub mtime_ms: u64,
    pub kind: FileKind,
    pub state: ItemState,
}

/// Configuration validation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// `max_items` outside the valid range 1..=500.
    MaxItemsOutOfRange(u64),
    /// `settle_ms` outside the valid range 0..=60000.
    SettleMsOutOfRange(u64),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MaxItemsOutOfRange(v) => {
                write!(f, "max_items must be within 1..=500, got {v}")
            }
            ConfigError::SettleMsOutOfRange(v) => {
                write!(f, "settle_ms must be within 0..=60000, got {v}")
            }
        }
    }
}

impl Error for ConfigError {}

/// Tuning knobs for the model.
///
/// Validate with [`DownloadsConfig::validate`] or construct through
/// [`DownloadsModel::new`], which validates for you.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadsConfig {
    /// Maximum number of items kept (newest by mtime). Valid: 1..=500.
    pub max_items: u64,
    /// Milliseconds an item's size/mtime must stay unchanged before it is
    /// Ready. Valid: 0..=60000.
    pub settle_ms: u64,
    /// Whether hidden names (leading `.`) are shown.
    pub show_hidden: bool,
}

impl Default for DownloadsConfig {
    fn default() -> Self {
        Self {
            max_items: 50,
            settle_ms: 1500,
            show_hidden: false,
        }
    }
}

impl DownloadsConfig {
    /// Check the config against the valid ranges. Returns a typed
    /// [`ConfigError`] on failure.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=500).contains(&self.max_items) {
            return Err(ConfigError::MaxItemsOutOfRange(self.max_items));
        }
        if self.settle_ms > 60_000 {
            return Err(ConfigError::SettleMsOutOfRange(self.settle_ms));
        }
        Ok(())
    }
}

/// Internal per-file state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryState {
    InProgress,
    Settling { deadline: u64 },
    Ready,
}

/// Internal tracked entry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    stat: FileStat,
    state: EntryState,
}

/// The downloads list model.
///
/// Feed it [`DirEvent`]s with [`DownloadsModel::apply`]; advance settling with
/// [`DownloadsModel::tick`]; read the list with [`DownloadsModel::items`].
#[derive(Debug)]
pub struct DownloadsModel {
    cfg: DownloadsConfig,
    entries: BTreeMap<String, Entry>,
    rejected: u64,
}

impl DownloadsModel {
    /// Create a validated model, or return a typed [`ConfigError`].
    pub fn new(cfg: DownloadsConfig) -> Result<Self, ConfigError> {
        cfg.validate()?;
        Ok(Self {
            cfg,
            entries: BTreeMap::new(),
            rejected: 0,
        })
    }

    /// Apply one filesystem change event at time `now` (monotonic ms).
    pub fn apply(&mut self, now: u64, ev: DirEvent) {
        match ev {
            DirEvent::Upsert(st) => self.upsert(now, st, false),
            DirEvent::Removed { name } => {
                self.entries.remove(&name);
            }
            DirEvent::Renamed { from, to } => {
                self.entries.remove(&from);
                self.upsert(now, to, true);
            }
            DirEvent::Rescan(list) => self.rescan(now, list),
        }
        self.evict();
    }

    /// Advance time to `now`, marking due Settling items Ready. Expiry is
    /// inclusive: a deadline equal to `now` becomes Ready.
    pub fn tick(&mut self, now: u64) {
        for entry in self.entries.values_mut() {
            if let EntryState::Settling { deadline } = entry.state {
                if deadline <= now {
                    entry.state = EntryState::Ready;
                }
            }
        }
    }

    /// Earliest settling deadline currently pending (None when nothing is
    /// settling). Note this reads *stored* state; callers that advance time
    /// should [`DownloadsModel::tick`] first.
    pub fn next_settle_at(&self) -> Option<u64> {
        self.entries
            .values()
            .filter_map(|e| match e.state {
                EntryState::Settling { deadline } => Some(deadline),
                _ => None,
            })
            .min()
    }

    /// The list the UI shows, sorted mtime descending with ties broken by
    /// name ascending (byte order). State is evaluated at `now`.
    pub fn items(&self, now: u64) -> Vec<DownloadItem> {
        let mut out: Vec<DownloadItem> = self
            .entries
            .values()
            .map(|e| {
                let state = match e.state {
                    EntryState::InProgress => ItemState::InProgress,
                    EntryState::Ready => ItemState::Ready,
                    EntryState::Settling { deadline } => {
                        if deadline <= now {
                            ItemState::Ready
                        } else {
                            ItemState::Settling
                        }
                    }
                };
                DownloadItem {
                    name: e.stat.name.clone(),
                    size_bytes: e.stat.size_bytes,
                    mtime_ms: e.stat.mtime_ms,
                    kind: classify(&e.stat.name),
                    state,
                }
            })
            .collect();
        out.sort_by(|a, b| {
            b.mtime_ms
                .cmp(&a.mtime_ms)
                .then_with(|| a.name.cmp(&b.name))
        });
        out
    }

    /// Number of names rejected so far (empty, `.`, `..`, or names containing
    /// `/` or NUL). Directories, symlinks and hidden names are not counted.
    pub fn rejected_count(&self) -> u64 {
        self.rejected
    }

    fn upsert(&mut self, now: u64, st: FileStat, force_settle: bool) {
        if !accept(&st, self.cfg.show_hidden, &mut self.rejected) {
            return;
        }
        let partial = is_partial(&st.name);
        if let Some(entry) = self.entries.get_mut(&st.name) {
            if partial {
                entry.stat = st;
                entry.state = EntryState::InProgress;
                return;
            }
            let same = entry.stat.size_bytes == st.size_bytes && entry.stat.mtime_ms == st.mtime_ms;
            entry.stat = st;
            if same && !force_settle {
                return;
            }
            entry.state = EntryState::Settling {
                deadline: now.saturating_add(self.cfg.settle_ms),
            };
        } else {
            let state = if partial {
                EntryState::InProgress
            } else {
                EntryState::Settling {
                    deadline: now.saturating_add(self.cfg.settle_ms),
                }
            };
            self.entries
                .insert(st.name.clone(), Entry { stat: st, state });
        }
    }

    fn rescan(&mut self, now: u64, list: Vec<FileStat>) {
        let old = std::mem::take(&mut self.entries);
        let mut next: BTreeMap<String, Entry> = BTreeMap::new();
        for st in list {
            if !accept(&st, self.cfg.show_hidden, &mut self.rejected) {
                continue;
            }
            let partial = is_partial(&st.name);
            let name = st.name.clone();
            let entry = match old.get(&name) {
                Some(prev)
                    if prev.stat.size_bytes == st.size_bytes
                        && prev.stat.mtime_ms == st.mtime_ms =>
                {
                    Entry {
                        stat: st,
                        state: prev.state,
                    }
                }
                _ => {
                    let state = if partial {
                        EntryState::InProgress
                    } else {
                        EntryState::Settling {
                            deadline: now.saturating_add(self.cfg.settle_ms),
                        }
                    };
                    Entry { stat: st, state }
                }
            };
            next.insert(name, entry);
        }
        self.entries = next;
    }

    /// Keep only the newest `max_items` by (mtime desc, name asc).
    fn evict(&mut self) {
        let max = self.cfg.max_items as usize;
        if self.entries.len() <= max {
            return;
        }
        let mut ranked: Vec<(&String, u64)> = self
            .entries
            .iter()
            .map(|(name, e)| (name, e.stat.mtime_ms))
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let keep: BTreeSet<String> = ranked
            .iter()
            .take(max)
            .map(|(name, _)| (*name).clone())
            .collect();
        self.entries.retain(|name, _| keep.contains(name));
    }
}

/// Filter a stat: returns true when it should enter the model.
fn accept(st: &FileStat, show_hidden: bool, rejected: &mut u64) -> bool {
    if st.is_dir || st.is_symlink {
        return false;
    }
    if invalid_name(&st.name) {
        *rejected += 1;
        return false;
    }
    if st.name.starts_with('.') && !show_hidden {
        return false;
    }
    true
}

/// Names that must be rejected and counted: empty, `.`, `..`, or anything
/// containing a path separator or NUL.
fn invalid_name(name: &str) -> bool {
    name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0')
}

/// Whether a filename carries an in-progress download suffix (case-
/// insensitive).
fn is_partial(name: &str) -> bool {
    const SUFFIXES: [&str; 5] = [
        ".part",
        ".crdownload",
        ".download",
        ".partial",
        ".opdownload",
    ];
    let lower = name.to_lowercase();
    SUFFIXES.iter().any(|s| lower.ends_with(s))
}

/// Classify a filename by its final extension (case-insensitive). Multi-part
/// extensions are matched before single ones. `.tar.gz` / `.tar.xz` /
/// `.tar.bz2` are Archive; `.pkg.tar.zst` is Package.
fn classify(name: &str) -> FileKind {
    const MULTI_PART: [(&str, FileKind); 4] = [
        (".pkg.tar.zst", FileKind::Package),
        (".tar.bz2", FileKind::Archive),
        (".tar.xz", FileKind::Archive),
        (".tar.gz", FileKind::Archive),
    ];
    let lower = name.to_lowercase();
    for (suffix, kind) in MULTI_PART {
        if lower.ends_with(suffix) {
            return kind;
        }
    }
    match lower.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => match ext {
            "zip" | "tar" | "gz" | "tgz" | "xz" | "zst" | "7z" | "rar" | "bz2" => FileKind::Archive,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif" | "heic" => FileKind::Image,
            "mp4" | "mkv" | "webm" | "mov" | "avi" => FileKind::Video,
            "mp3" | "flac" | "ogg" | "opus" | "wav" | "m4a" => FileKind::Audio,
            "pdf" | "doc" | "docx" | "odt" | "txt" | "md" | "rtf" | "epub" => FileKind::Document,
            "deb" | "rpm" | "appimage" | "flatpakref" => FileKind::Package,
            "rs" | "py" | "js" | "ts" | "json" | "toml" | "yaml" | "sh" => FileKind::Code,
            _ => FileKind::Other,
        },
        _ => FileKind::Other,
    }
}

/// Human-readable file size with 1 decimal using 1024 steps.
///
/// Examples: `0` -> "0 B", `1023` -> "1023 B", `1024` -> "1.0 KB",
/// `1048575` -> "1023.9 KB", `1048576` -> "1.0 MB". Values never show
/// "1024.0 KB" — at the boundary they roll up to the next unit. Never
/// panics, including at `u64::MAX`.
pub fn display_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    let mut unit = 1usize;
    let mut divisor = 1024u64;
    while unit < 4 && bytes >= divisor.saturating_mul(1024) {
        divisor *= 1024;
        unit += 1;
    }
    let whole = bytes / divisor;
    let rem = bytes % divisor;
    let tenths = (rem * 10) / divisor;
    format!("{}.{} {}", whole, tenths, UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(name: &str) -> FileKind {
        classify(name)
    }

    #[test]
    fn classify_full_table() {
        let cases: &[(&str, FileKind)] = &[
            ("a.zip", FileKind::Archive),
            ("a.tar", FileKind::Archive),
            ("a.gz", FileKind::Archive),
            ("a.tgz", FileKind::Archive),
            ("a.xz", FileKind::Archive),
            ("a.zst", FileKind::Archive),
            ("a.7z", FileKind::Archive),
            ("a.rar", FileKind::Archive),
            ("a.bz2", FileKind::Archive),
            ("a.tar.gz", FileKind::Archive),
            ("a.tar.xz", FileKind::Archive),
            ("a.tar.bz2", FileKind::Archive),
            ("a.PNG", FileKind::Image),
            ("a.png", FileKind::Image),
            ("a.jpg", FileKind::Image),
            ("a.jpeg", FileKind::Image),
            ("a.gif", FileKind::Image),
            ("a.webp", FileKind::Image),
            ("a.svg", FileKind::Image),
            ("a.avif", FileKind::Image),
            ("a.heic", FileKind::Image),
            ("a.mp4", FileKind::Video),
            ("a.mkv", FileKind::Video),
            ("a.webm", FileKind::Video),
            ("a.mov", FileKind::Video),
            ("a.avi", FileKind::Video),
            ("a.mp3", FileKind::Audio),
            ("a.flac", FileKind::Audio),
            ("a.ogg", FileKind::Audio),
            ("a.opus", FileKind::Audio),
            ("a.wav", FileKind::Audio),
            ("a.m4a", FileKind::Audio),
            ("a.pdf", FileKind::Document),
            ("a.doc", FileKind::Document),
            ("a.docx", FileKind::Document),
            ("a.odt", FileKind::Document),
            ("a.txt", FileKind::Document),
            ("a.md", FileKind::Document),
            ("a.rtf", FileKind::Document),
            ("a.epub", FileKind::Document),
            ("a.deb", FileKind::Package),
            ("a.rpm", FileKind::Package),
            ("a.flatpakref", FileKind::Package),
            ("a.pkg.tar.zst", FileKind::Package),
            ("a.PKG.TAR.ZST", FileKind::Package),
            ("a.AppImage", FileKind::Package),
            ("a.appimage", FileKind::Package),
            ("a.APPIMAGE", FileKind::Package),
            ("a.rs", FileKind::Code),
            ("a.py", FileKind::Code),
            ("a.js", FileKind::Code),
            ("a.ts", FileKind::Code),
            ("a.json", FileKind::Code),
            ("a.toml", FileKind::Code),
            ("a.yaml", FileKind::Code),
            ("a.sh", FileKind::Code),
            ("README", FileKind::Other),
            ("a", FileKind::Other),
            (".gitignore", FileKind::Other),
            (".hidden", FileKind::Other),
            ("file.", FileKind::Other),
            ("a.b.c", FileKind::Other),
            ("video.H264", FileKind::Other),
        ];
        for &(name, expected) in cases {
            assert_eq!(kind(name), expected, "name: {name}");
        }
    }

    #[test]
    fn classify_multipart_beats_single_extension() {
        assert_eq!(kind("linux.pkg.tar.zst"), FileKind::Package);
        assert_eq!(kind("a.pkg.tar.zst"), FileKind::Package);
        assert_eq!(kind("x.tar.zst"), FileKind::Archive);
        assert_eq!(kind("x.tar.gz"), FileKind::Archive);
        assert_eq!(kind("x.tar.xz"), FileKind::Archive);
        assert_eq!(kind("x.tar.bz2"), FileKind::Archive);
    }

    #[test]
    fn is_partial_case_insensitive_suffixes() {
        for name in [
            "x.part",
            "x.PART",
            "x.Part",
            "x.crdownload",
            "x.CRDOWNLOAD",
            "x.download",
            "x.partial",
            "x.opdownload",
        ] {
            assert!(is_partial(name), "expected partial: {name}");
        }
        for name in ["x.txt", "part", "x.parta", "x.partialx", "x"] {
            assert!(!is_partial(name), "expected not partial: {name}");
        }
    }

    #[test]
    fn invalid_name_table() {
        for name in ["", ".", "..", "a/b", "a\0b"] {
            assert!(invalid_name(name), "expected invalid: {name:?}");
        }
        for name in ["a", "a.b", ".hidden", "a b"] {
            assert!(!invalid_name(name), "expected valid: {name:?}");
        }
    }

    #[test]
    fn display_size_boundaries() {
        assert_eq!(display_size(0), "0 B");
        assert_eq!(display_size(1), "1 B");
        assert_eq!(display_size(1023), "1023 B");
        assert_eq!(display_size(1024), "1.0 KB");
        assert_eq!(display_size(1536), "1.5 KB");
        assert_eq!(display_size(1999), "1.9 KB");
        assert_eq!(display_size(1048575), "1023.9 KB");
        assert_eq!(display_size(1048576), "1.0 MB");
        assert_eq!(display_size(1073741824), "1.0 GB");
        assert_eq!(display_size(1099511627776), "1.0 TB");
        let _ = display_size(u64::MAX);
        assert_eq!(display_size(u64::MAX), "16777215.9 TB");
    }
}
