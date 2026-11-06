//! Shared harness pieces for the GUI integration tests (`xvfb_shape.rs`, `kwin_shape.rs`):
//! constants, X11 readback helpers, pixel checks, identity/focus assertions. Each test file
//! owns its own sandbox (Xvfb+xfwm4 versus nested KWin) because the stacks differ.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use hermes_island_lib::{
    contract::island::{IslandMode, RoundedRect},
    identity,
    window::shape::{covers, normalize_rows, shape_rows, ShapeRow},
};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread::sleep,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        shape::{self, ConnectionExt as _},
        xproto::{AtomEnum, ConnectionExt as _, ImageFormat, MapState},
        xtest::ConnectionExt as _,
    },
    rust_connection::RustConnection,
};

/// Where a test run stores its evidence images (`island-tauri/evidence/<phase>/`).
pub struct Evidence {
    dir: PathBuf,
}
impl Evidence {
    pub fn new(phase: &str) -> Self {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../evidence")
            .join(phase);
        std::fs::create_dir_all(&dir).unwrap();
        Evidence { dir }
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

pub static SERIAL: Mutex<()> = Mutex::new(());

pub const SCREEN_W: u16 = 1600;
pub const SCREEN_H: u16 = 1000;
pub const WIN_W: u16 = 1180;
pub const WIN_H: u16 = 820;

pub struct Spec {
    pub name: &'static str,
    pub mode: IslandMode,
    pub rect: RoundedRect,
    pub key: char,
}
pub const fn rr(x: f64, y: f64, width: f64, height: f64, radius: f64) -> RoundedRect {
    RoundedRect {
        x,
        y,
        width,
        height,
        radius,
    }
}
// Must match ui/main.tsx SHAPES + TOP; a drift fails the readback assertions.
pub const STILL: Spec = Spec {
    name: "still",
    mode: IslandMode::Still,
    rect: rr(515., 8., 150., 36., 18.),
    key: '1',
};
pub const HOVER: Spec = Spec {
    name: "hover",
    mode: IslandMode::Hover,
    rect: rr(380., 8., 420., 150., 28.),
    key: '2',
};
pub const LARGE: Spec = Spec {
    name: "large",
    mode: IslandMode::Large,
    rect: rr(70., 8., 1040., 700., 24.),
    key: '3',
};

pub fn require_tool(name: &str) {
    let found = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
        .unwrap_or(false);
    assert!(
        found,
        "GUI test prerequisite `{name}` is not installed (tests fail closed)"
    );
}

pub fn wait_for<T>(what: &str, timeout: Duration, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = probe() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {timeout:?} waiting for: {what}"
        );
        sleep(Duration::from_millis(50));
    }
}

/// SIGTERM the child's whole process group (Xvfb then unlinks its own socket; bus-activated
/// helpers such as xfconfd and WebKit's subprocesses go with it), wait up to 3 s, then SIGKILL.
pub fn stop_group(child: &mut Child) {
    let group = format!("-{}", child.id());
    let signal = |name: &str| {
        let _ = Command::new("kill")
            .args([name, "--", group.as_str()])
            .stderr(Stdio::null())
            .status();
    };
    signal("-TERM");
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && matches!(child.try_wait(), Ok(None)) {
        sleep(Duration::from_millis(25));
    }
    signal("-KILL"); // stragglers in the group (or the leader, if it ignored TERM)
    let _ = child.wait();
}

// ---------------------------------------------------------------------------------------------
// X11 helpers
// ---------------------------------------------------------------------------------------------

pub fn atom(conn: &RustConnection, name: &str) -> u32 {
    conn.intern_atom(false, name.as_bytes())
        .unwrap()
        .reply()
        .unwrap()
        .atom
}

pub fn prop_bytes(
    conn: &RustConnection,
    win: u32,
    name: &str,
    ty: impl Into<u32>,
) -> Option<Vec<u8>> {
    let reply = conn
        .get_property(false, win, atom(conn, name), ty, 0, 1024)
        .ok()?
        .reply()
        .ok()?;
    (reply.value_len > 0).then_some(reply.value)
}

pub fn prop_u32(conn: &RustConnection, win: u32, name: &str, ty: AtomEnum) -> Option<u32> {
    let bytes = prop_bytes(conn, win, name, ty)?;
    Some(u32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?))
}

/// The mapped `pid` window with the 1180x820 preview/below-panel size (Legacy/Harness pages).
pub fn find_island_window(conn: &RustConnection, root: u32, pid: u32) -> Option<u32> {
    find_window_sized(conn, root, pid, (WIN_W, WIN_H))
}

/// The mapped `pid` window with exactly `size` (the Island page uses the overlap viewport).
pub fn find_window_sized(
    conn: &RustConnection,
    root: u32,
    pid: u32,
    (width, height): (u16, u16),
) -> Option<u32> {
    let mut stack = vec![root];
    while let Some(window) = stack.pop() {
        if prop_u32(conn, window, "_NET_WM_PID", AtomEnum::CARDINAL) == Some(pid) {
            let attrs = conn.get_window_attributes(window).ok()?.reply().ok()?;
            let geo = conn.get_geometry(window).ok()?.reply().ok()?;
            if attrs.map_state == MapState::VIEWABLE && geo.width == width && geo.height == height {
                return Some(window);
            }
        }
        if let Ok(cookie) = conn.query_tree(window) {
            if let Ok(tree) = cookie.reply() {
                stack.extend(tree.children);
            }
        }
    }
    None
}

pub fn input_region(conn: &RustConnection, window: u32) -> Vec<ShapeRow> {
    let reply = conn
        .shape_get_rectangles(window, shape::SK::INPUT)
        .unwrap()
        .reply()
        .unwrap();
    reply
        .rectangles
        .iter()
        .map(|r| ShapeRow {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        })
        .collect()
}

pub fn expected_region(spec: &Spec, native: bool) -> Vec<ShapeRow> {
    normalize_rows(&shape_rows(spec.rect, spec.mode, 1.0, native).unwrap())
}

/// Independent geometric assertions (do not rely on `shape_rows` being right).
///
/// Known, inherited property of the ported algorithm: when `2 * radius >= height` (a fully round
/// pill) there is no straight band, so even the widest row is inset by 1 px (`d = 0.5` →
/// `ceil(r - sqrt(r² - 0.25)) = 1`). The pointer region is therefore 1 px narrower at each side
/// than the painted pill; asserted explicitly instead of being hidden by a tolerance.
pub fn assert_rounded_region(rows: &[ShapeRow], spec: &Spec) {
    let RoundedRect {
        x,
        y,
        width,
        height,
        radius,
    } = spec.rect;
    let pill = 2. * radius >= height;
    let side = i32::from(pill); // 1 px inset per side for a full pill
    let (x, y, w, h) = (x as i32, y as i32, width as i32, height as i32);
    assert!(!rows.is_empty(), "{}: region is empty", spec.name);
    let min_x = rows.iter().map(|r| i32::from(r.x)).min().unwrap();
    let min_y = rows.iter().map(|r| i32::from(r.y)).min().unwrap();
    let max_x = rows
        .iter()
        .map(|r| i32::from(r.x) + i32::from(r.width))
        .max()
        .unwrap();
    let max_y = rows
        .iter()
        .map(|r| i32::from(r.y) + i32::from(r.height))
        .max()
        .unwrap();
    assert_eq!(
        (min_x, min_y, max_x, max_y),
        (x + side, y, x + w - side, y + h),
        "{}: bounding box",
        spec.name
    );
    assert_eq!(
        rows.len() as i32,
        h,
        "{}: one span per pixel row",
        spec.name
    );
    assert!(
        !covers(rows, x, y) && !covers(rows, x + w - 1, y + h - 1),
        "{}: corner pixels must be outside",
        spec.name
    );
    assert!(
        !covers(rows, x + w - 1, y) && !covers(rows, x, y + h - 1),
        "{}: corner pixels must be outside",
        spec.name
    );
    for (px, py) in [
        (x + w / 2, y),
        (x + side, y + h / 2),
        (x + w - 1 - side, y + h / 2),
        (x + w / 2, y + h - 1),
        (x + w / 2, y + h / 2),
    ] {
        assert!(
            covers(rows, px, py),
            "{}: ({px},{py}) must be inside",
            spec.name
        );
    }
    for (px, py) in [
        (x - 1, y + h / 2),
        (x + w, y + h / 2),
        (x + w / 2, y - 1),
        (x + w / 2, y + h),
        (0, 0),
        (i32::from(WIN_W) - 1, i32::from(WIN_H) - 1),
    ] {
        assert!(
            !covers(rows, px, py),
            "{}: ({px},{py}) must be outside",
            spec.name
        );
    }
}

pub fn focus_report(conn: &RustConnection, root: u32) -> (u32, u32) {
    let focus = conn.get_input_focus().unwrap().reply().unwrap().focus;
    let active = prop_u32(conn, root, "_NET_ACTIVE_WINDOW", AtomEnum::WINDOW).unwrap_or(0);
    (focus, active)
}

pub fn keycode_for(conn: &RustConnection, ch: char) -> u8 {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let map = conn
        .get_keyboard_mapping(min, max - min + 1)
        .unwrap()
        .reply()
        .unwrap();
    let per = usize::from(map.keysyms_per_keycode);
    let sym = ch as u32; // Latin-1 keysyms equal their code points
    let index = map
        .keysyms
        .chunks(per)
        .position(|c| c.contains(&sym))
        .unwrap_or_else(|| panic!("no keycode for {ch:?}"));
    min + index as u8
}

pub fn press(conn: &RustConnection, root: u32, ch: char) {
    let code = keycode_for(conn, ch);
    conn.xtest_fake_input(2, code, 0, root, 0, 0, 0).unwrap();
    conn.xtest_fake_input(3, code, 0, root, 0, 0, 0).unwrap();
    conn.flush().unwrap();
}

pub fn click(conn: &RustConnection, root: u32, x: i16, y: i16) {
    conn.xtest_fake_input(6, 0, 0, root, x, y, 0).unwrap(); // MotionNotify (absolute)
    conn.xtest_fake_input(4, 1, 0, root, 0, 0, 0).unwrap(); // ButtonPress
    conn.xtest_fake_input(5, 1, 0, root, 0, 0, 0).unwrap(); // ButtonRelease
    conn.flush().unwrap();
}

pub struct Shot {
    pub w: usize,
    pub h: usize,
    pub bgra: Vec<u8>,
}
impl Shot {
    pub fn grab(conn: &RustConnection, drawable: u32, w: u16, h: u16) -> Shot {
        assert_eq!(
            conn.setup().image_byte_order,
            x11rb::protocol::xproto::ImageOrder::LSB_FIRST
        );
        let image = conn
            .get_image(ImageFormat::Z_PIXMAP, drawable, 0, 0, w, h, !0)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(
            image.data.len(),
            usize::from(w) * usize::from(h) * 4,
            "expected 32 bpp"
        );
        Shot {
            w: usize::from(w),
            h: usize::from(h),
            bgra: image.data,
        }
    }
    pub fn px(&self, x: i32, y: i32) -> [u8; 4] {
        let i = (y as usize * self.w + x as usize) * 4;
        [
            self.bgra[i + 2],
            self.bgra[i + 1],
            self.bgra[i],
            self.bgra[i + 3],
        ] // RGBA
    }
    pub fn alpha(&self, x: i32, y: i32) -> u8 {
        self.px(x, y)[3]
    }
    /// Straight-alpha RGBA of this shot blended over an opaque background (premultiplied input).
    pub fn blend_over(&self, bg: [u8; 3]) -> Shot {
        let mut bgra = Vec::with_capacity(self.bgra.len());
        for y in 0..self.h as i32 {
            for x in 0..self.w as i32 {
                let [r, g, b, a] = self.px(x, y);
                let over = |c: u8, back: u8| {
                    u8::try_from(u16::from(c) + u16::from(back) * (255 - u16::from(a)) / 255)
                        .unwrap_or(255)
                };
                bgra.extend_from_slice(&[over(b, bg[2]), over(g, bg[1]), over(r, bg[0]), 255]);
            }
        }
        Shot {
            w: self.w,
            h: self.h,
            bgra,
        }
    }
    pub fn save_png(&self, path: &Path) {
        let mut rgba = Vec::with_capacity(self.w * self.h * 4);
        for y in 0..self.h as i32 {
            for x in 0..self.w as i32 {
                rgba.extend_from_slice(&self.px(x, y));
            }
        }
        image::RgbaImage::from_raw(self.w as u32, self.h as u32, rgba)
            .unwrap()
            .save(path)
            .unwrap();
    }
}

/// Window ARGB pixels: alpha 0 everywhere outside the island (+2 px AA margin), solid inside,
/// rounded corners transparent. `Ok(n)` = number of solid pixels.
pub fn check_window_alpha(shot: &Shot, spec: &Spec) -> Result<usize, String> {
    let RoundedRect {
        x,
        y,
        width,
        height,
        radius,
    } = spec.rect;
    let (x, y, w, h, r) = (
        x as i32,
        y as i32,
        width as i32,
        height as i32,
        radius as i32,
    );
    let mut stray = Vec::new();
    let mut solid = 0usize;
    let mut interior_total = 0usize;
    let mut interior_solid = 0usize;
    for py in 0..shot.h as i32 {
        for px in 0..shot.w as i32 {
            let a = shot.alpha(px, py);
            let near = px >= x - 2 && px < x + w + 2 && py >= y - 2 && py < y + h + 2;
            if !near && a != 0 {
                stray.push((px, py, a));
            }
            if a >= 250 {
                solid += 1;
            }
            if px >= x + r + 2 && px < x + w - r - 2 && py >= y + 2 && py < y + h - 2 {
                interior_total += 1;
                if a >= 250 {
                    interior_solid += 1;
                }
            }
        }
    }
    if !stray.is_empty() {
        return Err(format!(
            "{}: {} non-transparent pixels outside the island, first: {:?}",
            spec.name,
            stray.len(),
            &stray[..stray.len().min(5)]
        ));
    }
    if interior_solid != interior_total {
        return Err(format!(
            "{}: island interior not fully opaque ({interior_solid}/{interior_total})",
            spec.name
        ));
    }
    if shot.alpha(x, y) >= 128 || shot.alpha(x + w - 1, y + h - 1) >= 128 {
        return Err(format!(
            "{}: rounded corner pixels are not transparent",
            spec.name
        ));
    }
    if solid < (w * h) as usize * 8 / 10 {
        return Err(format!("{}: too few opaque pixels ({solid})", spec.name));
    }
    Ok(solid)
}

/// Poll until the window pixels show `spec` (page load / repaint latency), fail closed on timeout.
pub fn verify_pixels(
    conn: &RustConnection,
    window: u32,
    spec: &Spec,
    evidence: &Evidence,
    tag: &str,
) -> Shot {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let shot = Shot::grab(conn, window, WIN_W, WIN_H);
        match check_window_alpha(&shot, spec) {
            Ok(_) => {
                shot.save_png(&evidence.path(&format!("{tag}-{}-window-argb.png", spec.name)));
                return shot;
            }
            Err(message) if Instant::now() >= deadline => panic!("{tag}: {message}"),
            Err(_) => sleep(Duration::from_millis(100)),
        }
    }
}

/// Drive to `spec` via its direct key and wait until the X server's ShapeInput *and* the window
/// pixels both show it. Returns the (verified) window screenshot.
pub fn goto_and_verify(
    conn: &RustConnection,
    root: u32,
    window: u32,
    spec: &Spec,
    native: bool,
    evidence: &Evidence,
    tag: &str,
) -> Shot {
    press(conn, root, spec.key);
    let want = expected_region(spec, native);
    let rows = wait_for(
        &format!("{tag}/{}: ShapeInput == expected", spec.name),
        Duration::from_secs(20),
        || {
            let got = normalize_rows(&input_region(conn, window));
            (got == want).then_some(got)
        },
    );
    if !(native && spec.mode == IslandMode::Still) {
        assert_rounded_region(&rows, spec);
    } else {
        assert!(
            rows.is_empty(),
            "native-pill still must have an EMPTY input region"
        );
    }
    // The pixels must catch up with the same state.
    verify_pixels(conn, window, spec, evidence, tag)
}

pub fn lock() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn assert_identity(conn: &RustConnection, window: u32, pid: u32, overlay: bool) {
    assert_eq!(
        prop_u32(conn, window, "_NET_WM_PID", AtomEnum::CARDINAL),
        Some(pid),
        "_NET_WM_PID"
    );
    let utf8 = atom(conn, "UTF8_STRING");
    let name = prop_bytes(conn, window, "_NET_WM_NAME", utf8).expect("_NET_WM_NAME");
    let title = if overlay {
        identity::TITLE_OVERLAY
    } else {
        identity::TITLE_PREVIEW
    };
    assert_eq!(String::from_utf8(name).unwrap(), title, "_NET_WM_NAME");
    let class = prop_bytes(conn, window, "WM_CLASS", AtomEnum::STRING).expect("WM_CLASS");
    let mut parts = class
        .split(|b| *b == 0)
        .map(|p| String::from_utf8_lossy(p).into_owned());
    assert_eq!(
        parts.next().as_deref(),
        Some(identity::WM_CLASS_INSTANCE),
        "WM_CLASS instance"
    );
    assert_eq!(
        parts.next().as_deref(),
        Some(identity::WM_CLASS_CLASS),
        "WM_CLASS class"
    );
    // window type + state from the platform hooks
    let notification = atom(conn, identity::WINDOW_TYPE_ATOM);
    let types = prop_bytes(conn, window, "_NET_WM_WINDOW_TYPE", AtomEnum::ATOM)
        .expect("_NET_WM_WINDOW_TYPE");
    assert!(
        types
            .chunks(4)
            .any(|c| u32::from_ne_bytes(c.try_into().unwrap()) == notification),
        "window type must be NOTIFICATION"
    );
}

/// Focus on show, two independent signals:
/// 1. WM-independent request: GTK `focus_on_map(false)` publishes `_NET_WM_USER_TIME = 0`
///    ("do not focus me when mapped"). GDK puts it on the toplevel until it creates the
///    `_NET_WM_USER_TIME_WINDOW`, and on that window afterwards; which one carries it depends on
///    timing and the window manager (xfwm4: the user-time window; KWin: the toplevel). So both
///    are read: neither may hold a non-zero time and at least one must hold 0.
/// 2. Observed outcome: neither X input focus nor `_NET_ACTIVE_WINDOW` is the island. (xfwm4 never
///    auto-focuses TOOLBAR-type windows, so this alone would not catch a regression; hence 1.)
pub fn assert_no_focus_requested(conn: &RustConnection, window: u32) {
    let user_time_window = prop_u32(conn, window, "_NET_WM_USER_TIME_WINDOW", AtomEnum::WINDOW)
        .expect("_NET_WM_USER_TIME_WINDOW");
    let times = [
        (
            "toplevel",
            prop_u32(conn, window, "_NET_WM_USER_TIME", AtomEnum::CARDINAL),
        ),
        (
            "user-time window",
            prop_u32(
                conn,
                user_time_window,
                "_NET_WM_USER_TIME",
                AtomEnum::CARDINAL,
            ),
        ),
    ];
    for (owner, time) in times {
        assert!(
            matches!(time, None | Some(0)),
            "island published a non-zero user time on its {owner}: {time:?} (it asked for focus)"
        );
    }
    assert!(
        times.iter().any(|(_, t)| *t == Some(0)),
        "island must request no focus on map (_NET_WM_USER_TIME == 0 on toplevel or user-time window): {times:?}"
    );
}

pub fn assert_not_focused_on_show(conn: &RustConnection, root: u32, window: u32) {
    assert_no_focus_requested(conn, window);
    let (focus, active) = focus_report(conn, root);
    assert_ne!(focus, window, "island holds X input focus after show");
    assert_ne!(active, window, "island is _NET_ACTIVE_WINDOW after show");
}
