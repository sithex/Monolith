use super::*;
use std::{
    os::unix::fs::symlink,
    sync::atomic::{AtomicU64, Ordering},
};

struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        static ID: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "hi-mixer-icons-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("icons")).unwrap();
        Self(path)
    }
    fn put(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn theme(&self, name: &str, parents: &str) {
        self.put(&format!("icons/{name}/index.theme"), format!("[Icon Theme]\nName={name}\nInherits={parents}\nDirectories=16/apps,32/apps,48/apps,256/apps,scalable/apps\n[16/apps]\nSize=16\nType=Fixed\n[32/apps]\nSize=32\nType=Fixed\n[48/apps]\nSize=48\nType=Fixed\n[256/apps]\nSize=256\nType=Fixed\n[scalable/apps]\nSize=48\nType=Scalable\nMinSize=16\nMaxSize=512\n"));
    }
    fn resolver(&self, selected: &str) -> Resolver {
        self.put(
            "kdeglobals",
            format!("[Other]\nTheme=wrong\n[Icons]\nTheme={selected}\n"),
        );
        Resolver::new(
            vec![self.0.join("icons"), self.0.join("pixmaps")],
            self.0.join("kdeglobals"),
        )
    }
    fn png(&self, theme: &str, dir: &str, name: &str, red: u8) {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(48, 48, image::Rgba([red, 0, 0, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        self.put(
            &format!("icons/{theme}/{dir}/apps/{name}.png"),
            png.into_inner(),
        );
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn pixel(data: &str) -> u8 {
    let bytes = STANDARD
        .decode(data.strip_prefix("data:image/png;base64,").unwrap())
        .unwrap();
    let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert!(image.width() <= 48 && image.height() <= 48);
    image.get_pixel(0, 0)[0]
}

#[test]
fn theme_inheritance_current_wins_and_hicolor_is_last_even_with_cycles() {
    let t = Tree::new();
    t.theme("current", "hicolor,parent");
    t.theme("parent", "current");
    t.theme("hicolor", "");
    t.png("current", "256", "same", 1);
    t.png("parent", "48", "same", 2);
    t.png("parent", "48", "inherited", 3);
    t.png("hicolor", "48", "inherited", 4);
    t.png("hicolor", "48", "last", 5);
    let mut r = t.resolver("current");
    assert_eq!(pixel(&r.resolve("same").unwrap()), 1);
    assert_eq!(pixel(&r.resolve("inherited").unwrap()), 3);
    assert_eq!(pixel(&r.resolve("last").unwrap()), 5);
    assert_eq!(
        r.themes.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["current", "parent", "hicolor"]
    );
}
#[test]
fn size_preference_32_to_48_png_then_svg_then_nearest_size() {
    let t = Tree::new();
    t.theme("current", "");
    t.png("current", "256", "player", 1);
    t.png("current", "16", "player", 2);
    t.png("current", "48", "player", 3);
    t.put("icons/current/scalable/apps/player.svg", b"<svg xmlns='http://www.w3.org/2000/svg' width='48' height='48'><rect width='48' height='48' fill='red'/></svg>");
    assert_eq!(pixel(&t.resolver("current").resolve("player").unwrap()), 3);
    fs::remove_file(t.0.join("icons/current/48/apps/player.png")).unwrap();
    assert_eq!(
        pixel(&t.resolver("current").resolve("player").unwrap()),
        255
    );
    fs::remove_file(t.0.join("icons/current/scalable/apps/player.svg")).unwrap();
    assert_eq!(pixel(&t.resolver("current").resolve("player").unwrap()), 2);
    assert_eq!(
        distance(&HashMap::from([
            ("Size".into(), "16".into()),
            ("Scale".into(), "2".into()),
            ("Type".into(), "Fixed".into())
        ])),
        Some(0)
    );
}
#[test]
fn missing_invalid_names_and_absent_theme_fall_back_safely() {
    let t = Tree::new();
    t.theme("hicolor", "");
    t.png("hicolor", "32", "player", 7);
    let mut r = t.resolver("missing-theme");
    assert_eq!(pixel(&r.resolve("player").unwrap()), 7);
    for name in [
        "absent",
        "",
        "../player",
        "/player",
        "file:///player",
        "https://invalid/icon",
        "a\\b",
        ".hidden",
    ] {
        assert!(r.resolve(name).is_none(), "{name}");
    }
    assert!(!relative("../outside"));
    assert!(!relative("48/./apps"));
}
#[test]
fn oversize_corrupt_and_huge_dimension_images_are_rejected() {
    let t = Tree::new();
    t.theme("current", "");
    t.put("icons/current/48/apps/large.png", vec![0; MAX_SOURCE + 1]);
    t.put("icons/current/48/apps/broken.png", b"not png");
    t.put(
        "icons/current/scalable/apps/large-svg.svg",
        vec![b' '; MAX_SOURCE + 1],
    );
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::new(2049, 1)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    t.put("icons/current/48/apps/huge.png", png.into_inner());
    let mut r = t.resolver("current");
    for name in ["large", "broken", "large-svg", "huge"] {
        assert!(r.resolve(name).is_none(), "{name}");
    }
}
#[test]
fn symlink_file_directory_and_theme_index_cannot_escape_icon_roots() {
    let t = Tree::new();
    t.theme("current", "");
    t.png("current", "48", "inside", 10);
    fs::create_dir_all(t.0.join("outside")).unwrap();
    fs::copy(
        t.0.join("icons/current/48/apps/inside.png"),
        t.0.join("outside/private.png"),
    )
    .unwrap();
    symlink(
        t.0.join("outside/private.png"),
        t.0.join("icons/current/48/apps/escape.png"),
    )
    .unwrap();
    symlink(t.0.join("outside"), t.0.join("icons/current/48/escape-dir")).unwrap();
    symlink(
        t.0.join("icons/current/48/apps/inside.png"),
        t.0.join("icons/current/48/apps/alias.png"),
    )
    .unwrap();
    let mut r = t.resolver("current");
    assert!(r.resolve("escape").is_none());
    assert!(r
        .bytes(&t.0.join("icons/current/48/escape-dir/private.png"))
        .is_none());
    assert_eq!(pixel(&r.resolve("alias").unwrap()), 10);
    fs::copy(
        t.0.join("icons/current/index.theme"),
        t.0.join("outside/index.theme"),
    )
    .unwrap();
    fs::create_dir_all(t.0.join("icons/evil")).unwrap();
    symlink(
        t.0.join("outside/index.theme"),
        t.0.join("icons/evil/index.theme"),
    )
    .unwrap();
    assert!(t.resolver("evil").themes.is_empty());
}
#[test]
fn unthemed_pixmap_and_user_root_override() {
    let t = Tree::new();
    t.theme("current", "");
    t.png("current", "48", "player", 11);
    let user = t.0.join("user");
    fs::create_dir_all(user.join("current/48/apps")).unwrap();
    fs::copy(
        t.0.join("icons/current/48/apps/player.png"),
        user.join("current/48/apps/player.png"),
    )
    .unwrap();
    t.png("current", "48", "player", 12);
    let mut r = t.resolver("current");
    r.roots.insert(0, user);
    r.reload();
    assert_eq!(pixel(&r.resolve("player").unwrap()), 11);
    fs::create_dir_all(t.0.join("pixmaps")).unwrap();
    fs::copy(
        t.0.join("icons/current/48/apps/player.png"),
        t.0.join("pixmaps/fallback.png"),
    )
    .unwrap();
    assert_eq!(pixel(&r.resolve("fallback").unwrap()), 12);
}
#[test]
fn cache_is_bounded_including_misses_and_reloads_on_theme_change() {
    let t = Tree::new();
    t.theme("one", "");
    t.theme("two", "");
    t.png("one", "48", "player", 1);
    t.png("two", "48", "player", 2);
    let mut r = t.resolver("one");
    assert_eq!(pixel(&r.resolve("player").unwrap()), 1);
    for i in 0..CACHE_ENTRIES + 5 {
        assert!(r.resolve(&format!("missing-{i}")).is_none());
    }
    assert_eq!(r.cache.len(), CACHE_ENTRIES);
    t.put("kdeglobals", b"[Icons]\nTheme=two\n");
    r.stamps.clear();
    r.checked = Instant::now() - RECHECK;
    assert_eq!(pixel(&r.resolve("player").unwrap()), 2);
    assert_eq!(r.cache.len(), 1);
}
#[test]
fn svg_external_references_are_not_loaded() {
    let t = Tree::new();
    t.theme("current", "");
    t.png("current", "48", "private", 123);
    let private = t.0.join("icons/current/48/apps/private.png");
    t.put("icons/current/scalable/apps/external.svg", format!("<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='48' height='48'><image xlink:href='file://{}' width='48' height='48'/></svg>", private.display()));
    let data = t.resolver("current").resolve("external");
    if let Some(data) = data {
        assert_eq!(pixel(&data), 0, "external file was not painted");
    }
}
#[test]
#[ignore = "read-only installed-theme probe; no display or audio changes"]
fn installed_theme_probe() {
    let mut r = Resolver::system();
    println!(
        "themes={:?}",
        r.themes.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
    let names = std::env::var("MONOLITH_ICON_PROBE")
        .unwrap_or_else(|_| "firefox,org.kde.konsole,spotify,audio-volume-high".into());
    for name in names.split(',') {
        println!(
            "{name}: {}",
            r.resolve(name)
                .map(|s| format!("PNG data URL ({} bytes)", s.len()))
                .unwrap_or_else(|| "unresolved".into())
        );
    }
}
