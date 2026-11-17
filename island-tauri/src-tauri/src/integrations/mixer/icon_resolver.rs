//! Local theme icons, rasterized to the same PNG data URLs as notification icons.
//! All filesystem and decoder work runs on a blocking worker, never the mixer actor.
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Cursor, Read},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime},
};

const MAX_SOURCE: usize = 1024 * 1024;
const MAX_OUTPUT: usize = 32 * 1024;
const CACHE_ENTRIES: usize = 128;
const MAX_THEMES: usize = 32;
const MAX_DIRS: usize = 2048;
const RECHECK: Duration = Duration::from_secs(5);
type Ini = HashMap<String, HashMap<String, String>>;

fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 255
        && !s.starts_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn relative(s: &str) -> bool {
    !s.is_empty() && s.len() <= 512 && s.split('/').all(identifier)
}
fn read(file: File) -> Option<Vec<u8>> {
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_SOURCE as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= MAX_SOURCE).then_some(bytes)
}
fn open(path: &Path) -> Option<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)
        .ok()
}
fn ini(bytes: &[u8]) -> Option<Ini> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut result = Ini::new();
    let mut section = String::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = s.to_owned();
        } else if let Some((key, value)) = line.split_once('=') {
            result
                .entry(section.clone())
                .or_default()
                .insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    Some(result)
}
#[derive(Clone)]
struct Directory {
    path: String,
    distance: u32,
}
struct Theme {
    name: String,
    dirs: Vec<Directory>,
}

/// Roots retain their configured spelling for invalidation; reads validate canonical roots.
struct Resolver {
    roots: Vec<PathBuf>,
    config: PathBuf,
    themes: Vec<Theme>,
    watched: Vec<PathBuf>,
    stamps: Vec<Option<SystemTime>>,
    checked: Instant,
    cache: VecDeque<(String, Option<String>)>,
}
impl Resolver {
    fn system() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let data_home = absolute_env("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        let mut roots = vec![home.join(".icons"), data_home.join("icons")];
        if data_home != home.join(".local/share") {
            roots.push(home.join(".local/share/icons"));
        }
        roots.extend(
            std::env::var("XDG_DATA_DIRS")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "/usr/local/share:/usr/share".into())
                .split(':')
                .filter(|s| s.starts_with('/'))
                .take(32)
                .map(|s| PathBuf::from(s).join("icons")),
        );
        roots.push(PathBuf::from("/usr/share/pixmaps"));
        let config = absolute_env("XDG_CONFIG_HOME")
            .unwrap_or_else(|| home.join(".config"))
            .join("kdeglobals");
        Self::new(roots, config)
    }
    fn new(roots: Vec<PathBuf>, config: PathBuf) -> Self {
        let mut this = Self {
            roots,
            config,
            themes: vec![],
            watched: vec![],
            stamps: vec![],
            checked: Instant::now(),
            cache: VecDeque::new(),
        };
        this.reload();
        this
    }
    /// Check the opened descriptor too: a symlink/rename race must not turn an icon
    /// lookup into a read outside the configured roots. Internal theme aliases work.
    fn bytes(&self, path: &Path) -> Option<Vec<u8>> {
        let canonical = path.canonicalize().ok()?;
        let roots: Vec<_> = self
            .roots
            .iter()
            .filter_map(|r| r.canonicalize().ok())
            .collect();
        if !roots.iter().any(|r| canonical.starts_with(r)) {
            return None;
        }
        let file = open(&canonical)?;
        let actual = fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).ok()?;
        if !roots.iter().any(|r| actual.starts_with(r)) {
            return None;
        }
        read(file)
    }
    fn reload(&mut self) {
        self.cache.clear();
        self.themes.clear();
        self.watched = self.roots.clone();
        self.watched.push(self.config.clone());
        let selected = open(&self.config)
            .and_then(read)
            .and_then(|b| ini(&b))
            .and_then(|i| i.get("Icons")?.get("Theme").cloned())
            .filter(|s| identifier(s))
            .unwrap_or_else(|| "hicolor".into());
        let mut seen = HashSet::new();
        self.theme(&selected, &mut seen);
        self.theme("hicolor", &mut seen);
        self.stamps = self.timestamps();
        self.checked = Instant::now();
    }
    fn theme(&mut self, name: &str, seen: &mut HashSet<String>) {
        if !identifier(name)
            || (seen.len() >= MAX_THEMES && name != "hicolor")
            || !seen.insert(name.to_owned())
        {
            return;
        }
        let mut index = None;
        for root in &self.roots {
            let base = root.join(name);
            self.watched.push(base.clone());
            let path = base.join("index.theme");
            self.watched.push(path.clone());
            if index.is_none() {
                index = self.bytes(&path).and_then(|b| ini(&b));
            }
        }
        let Some(index) = index else {
            return;
        };
        let Some(main) = index.get("Icon Theme") else {
            return;
        };
        let dirs = ["Directories", "ScaledDirectories"]
            .iter()
            .filter_map(|k| main.get(*k))
            .flat_map(|s| s.split(','))
            .map(str::trim)
            .filter(|s| relative(s))
            .take(MAX_DIRS)
            .filter_map(|s| {
                Some(Directory {
                    path: s.into(),
                    distance: distance(index.get(s)?)?,
                })
            })
            .collect();
        self.themes.push(Theme {
            name: name.into(),
            dirs,
        });
        if let Some(parents) = main.get("Inherits") {
            for parent in parents.split(',').map(str::trim).take(MAX_THEMES) {
                // hicolor is always searched last, even if explicitly inherited.
                if parent != "hicolor" {
                    self.theme(parent, seen);
                }
            }
        }
    }
    fn timestamps(&self) -> Vec<Option<SystemTime>> {
        self.watched
            .iter()
            .map(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
            .collect()
    }
    fn refresh(&mut self) {
        if self.checked.elapsed() >= RECHECK {
            if self.timestamps() != self.stamps {
                self.reload();
            }
            self.checked = Instant::now();
        }
    }
    fn resolve(&mut self, name: &str) -> Option<String> {
        if !identifier(name) {
            return None;
        }
        self.refresh();
        if let Some((_, value)) = self.cache.iter().find(|(key, _)| key == name) {
            return value.clone();
        }
        let value = self.lookup(name);
        if self.cache.len() == CACHE_ENTRIES {
            self.cache.pop_front();
        }
        self.cache.push_back((name.to_owned(), value.clone()));
        value
    }
    fn lookup(&self, name: &str) -> Option<String> {
        for theme in &self.themes {
            let mut dirs: Vec<_> = theme.dirs.iter().collect();
            dirs.sort_by_key(|d| d.distance);
            // Within a theme, prefer a PNG in the 32–48 px band, then an SVG
            // at the requested size, then the nearest remaining representation.
            for distance in dirs
                .iter()
                .map(|d| d.distance)
                .collect::<std::collections::BTreeSet<_>>()
            {
                for ext in ["png", "svg"] {
                    for dir in dirs.iter().filter(|d| d.distance == distance) {
                        for root in &self.roots {
                            let path = root
                                .join(&theme.name)
                                .join(&dir.path)
                                .join(format!("{name}.{ext}"));
                            if let Some(data) = self.bytes(&path).and_then(|b| encode(b, ext)) {
                                return Some(data);
                            }
                        }
                    }
                }
            }
        }
        for root in &self.roots {
            for ext in ["png", "svg"] {
                if let Some(data) = self
                    .bytes(&root.join(format!("{name}.{ext}")))
                    .and_then(|b| encode(b, ext))
                {
                    return Some(data);
                }
            }
        }
        None
    }
}
fn absolute_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}
/// Distance from the preferred physical 32–48 px band, respecting Scale/Type.
fn distance(section: &HashMap<String, String>) -> Option<u32> {
    let number = |key: &str, default: u32| {
        section
            .get(key)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(default)
            .min(4096)
    };
    let size = number("Size", 0);
    if size == 0 {
        return None;
    }
    let scale = number("Scale", 1).max(1);
    let (min, max) = match section
        .get("Type")
        .map(String::as_str)
        .unwrap_or("Threshold")
    {
        "Fixed" => (size, size),
        "Scalable" => (number("MinSize", size), number("MaxSize", size)),
        "Threshold" => (
            size.saturating_sub(number("Threshold", 2)),
            size.saturating_add(number("Threshold", 2)),
        ),
        _ => return None,
    };
    if min > max {
        return None;
    }
    Some(
        (min * scale)
            .saturating_sub(48)
            .max(32_u32.saturating_sub(max * scale)),
    )
}
fn encode(bytes: Vec<u8>, ext: &str) -> Option<String> {
    let png = if ext == "svg" {
        use gtk::gdk_pixbuf::{prelude::PixbufLoaderExt, PixbufLoader};
        // Loading bytes (not a filename) gives librsvg no base URI: external file
        // references cannot resolve. Only the rasterized PNG crosses IPC/CSP.
        let loader = PixbufLoader::with_type("svg").ok()?;
        loader.set_size(48, 48);
        let written = loader.write(&bytes);
        let closed = loader.close();
        written.ok()?;
        closed.ok()?;
        loader.pixbuf()?.save_to_bufferv("png", &[]).ok()?
    } else {
        let mut reader =
            image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(2048);
        limits.max_image_height = Some(2048);
        limits.max_alloc = Some(32 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().ok()?.thumbnail(48, 48);
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).ok()?;
        out.into_inner()
    };
    (png.len() <= MAX_OUTPUT).then(|| format!("data:image/png;base64,{}", STANDARD.encode(png)))
}

pub(super) async fn resolve(names: Vec<String>) -> HashMap<String, String> {
    tokio::task::spawn_blocking(move || {
        static RESOLVER: OnceLock<Mutex<Resolver>> = OnceLock::new();
        let Ok(mut resolver) = RESOLVER
            .get_or_init(|| Mutex::new(Resolver::system()))
            .lock()
        else {
            return HashMap::new();
        };
        names
            .into_iter()
            .filter_map(|name| resolver.resolve(&name).map(|icon| (name, icon)))
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
