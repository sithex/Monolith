//! SPEC §8 Phase 2.5 — panel-overlap spike on a private nested KWin (6.8, OpenGL ES compositing).
//!
//! One scenario answers the four questions with real input, real compositor captures and real
//! window-manager state, and asserts every fact it found (fail closed):
//!   (a) stacking + input: which window type draws the island over a dock-type panel at the pill
//!       while its EMPTY input region leaves the panel's hover/click intact; what buries it;
//!   (b) morph: pill rectangle → hover size, spring 350/32/0.8, compositor burst frames;
//!   (c) blur: `_KDE_NET_WM_BLUR_BEHIND_REGION` mirrored from the input-region owner;
//!   (d) frame pacing of the morph (rAF deltas, 10 cycles with and without geometry reports).
//! Never touches the real session: private bus, scratch HOME/XDG_*, `kwin_wayland --virtual`.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "helpers/spike_analysis.rs"]
mod spike_analysis;
#[path = "helpers/spike_stack.rs"]
mod spike_stack;

use common::{
    assert_no_focus_requested, assert_not_focused_on_show, atom, find_island_window, focus_report,
    input_region, lock, prop_bytes, prop_u32, rr, wait_for, Evidence,
};
use hermes_island_lib::{
    contract::island::IslandMode,
    identity,
    window::shape::{normalize_rows, shape_rows, ShapeRow},
};
use serde_json::{json, Value};
use spike_analysis::{
    measure, near, sharpness, stats, Capture, Measured, ISLAND_YELLOW, PANEL_BLUE, PILL_TEAL,
};
use spike_stack::{Eis, Stack};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::Path,
    thread::sleep,
    time::Duration,
};
use x11rb::{
    connection::Connection,
    protocol::{
        shape::{ConnectionExt as _, SK, SO},
        xproto::{
            AtomEnum, ClipOrdering, ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask,
            ImageFormat, MapState, NotifyMode, PropMode, Rectangle, WindowClass,
        },
        Event,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

/// Screen geometry of the sandbox: panel 44 px, island window origin (210, 0), pill centre.
const PANEL_H: u16 = 44;
const WIN_ORIGIN: (i16, i16) = (210, 0);
const PILL_CENTRE: (i16, i16) = (800, 22);
/// Most teal (pill) pixels that may peek around the island's anti-aliased rounded corners.
const MAX_TEAL_PX: usize = 40;

// ---------------------------------------------------------------------------------------------
// the stand-in panel: DOCK window + strut + a rounded pill child that logs real pointer events
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Top,
    Pill,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ev {
    Enter(Part),
    Leave(Part),
    Press(Part),
    Release(Part),
}

struct Panel {
    conn: RustConnection,
    top: u32,
    pill: u32,
}

fn set_atoms(conn: &RustConnection, window: u32, prop: &str, values: &[&str]) {
    let list: Vec<u32> = values.iter().map(|v| atom(conn, v)).collect();
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atom(conn, prop),
        AtomEnum::ATOM,
        &list,
    )
    .unwrap();
}

fn set_name(conn: &RustConnection, window: u32, title: &str) {
    conn.change_property8(
        PropMode::REPLACE,
        window,
        atom(conn, "_NET_WM_NAME"),
        atom(conn, "UTF8_STRING"),
        title.as_bytes(),
    )
    .unwrap();
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        format!("{title}\0{title}\0").as_bytes(),
    )
    .unwrap();
}

/// WM_NORMAL_HINTS with USPosition|USSize so KWin honours the requested position.
fn user_position(conn: &RustConnection, window: u32, x: i32, y: i32, w: u32, h: u32) {
    let mut hints = [0u32; 18];
    hints[..5].copy_from_slice(&[3, x as u32, y as u32, w, h]);
    conn.change_property32(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_NORMAL_HINTS,
        AtomEnum::WM_SIZE_HINTS,
        &hints,
    )
    .unwrap();
}

impl Panel {
    /// `accepts_focus` = WM_HINTS input: a dock that KWin may activate on click (worst case for
    /// stacking) or one that never takes focus.
    fn create(display: &str, accepts_focus: bool) -> Panel {
        let conn = RustConnection::connect(Some(display)).unwrap().0;
        let root = conn.setup().roots[0].root;
        let mask = EventMask::ENTER_WINDOW
            | EventMask::LEAVE_WINDOW
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE;
        let (top, pill) = (conn.generate_id().unwrap(), conn.generate_id().unwrap());
        conn.create_window(
            0,
            top,
            root,
            0,
            0,
            1600,
            PANEL_H,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(0x0019_3399) // PANEL_BLUE
                .event_mask(mask),
        )
        .unwrap();
        conn.create_window(
            0,
            pill,
            top,
            725,
            4,
            150,
            36,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(0x0000_cccc) // PILL_TEAL
                .event_mask(mask),
        )
        .unwrap();
        // The pill is as round as the Plasma pill: the exact rows the island's own input region
        // uses for the same rounded rect.
        let rows = normalize_rows(
            &shape_rows(rr(0., 0., 150., 36., 18.), IslandMode::Hover, 1.0, false).unwrap(),
        );
        let rects: Vec<Rectangle> = rows
            .iter()
            .map(|r| Rectangle {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            })
            .collect();
        conn.shape_rectangles(
            SO::SET,
            SK::BOUNDING,
            ClipOrdering::UNSORTED,
            pill,
            0,
            0,
            &rects,
        )
        .unwrap();
        set_atoms(
            &conn,
            top,
            "_NET_WM_WINDOW_TYPE",
            &["_NET_WM_WINDOW_TYPE_DOCK"],
        );
        let strut_partial = [0u32, 0, u32::from(PANEL_H), 0, 0, 0, 0, 0, 0, 1599, 0, 0];
        conn.change_property32(
            PropMode::REPLACE,
            top,
            atom(&conn, "_NET_WM_STRUT_PARTIAL"),
            AtomEnum::CARDINAL,
            &strut_partial,
        )
        .unwrap();
        conn.change_property32(
            PropMode::REPLACE,
            top,
            atom(&conn, "_NET_WM_STRUT"),
            AtomEnum::CARDINAL,
            &[0, 0, u32::from(PANEL_H), 0],
        )
        .unwrap();
        // WM_HINTS: flags = InputHint, input = accepts_focus.
        conn.change_property32(
            PropMode::REPLACE,
            top,
            AtomEnum::WM_HINTS,
            AtomEnum::WM_HINTS,
            &[1, u32::from(accepts_focus), 1, 0, 0, 0, 0, 0, 0],
        )
        .unwrap();
        set_name(&conn, top, "spike-panel");
        user_position(&conn, top, 0, 0, 1600, u32::from(PANEL_H));
        conn.map_window(pill).unwrap();
        conn.map_window(top).unwrap();
        conn.flush().unwrap();
        wait_for("stand-in panel viewable", Duration::from_secs(10), || {
            let attrs = conn.get_window_attributes(pill).ok()?.reply().ok()?;
            (attrs.map_state == MapState::VIEWABLE).then_some(())
        });
        Panel { conn, top, pill }
    }

    fn part(&self, window: u32) -> Option<Part> {
        match window {
            w if w == self.top => Some(Part::Top),
            w if w == self.pill => Some(Part::Pill),
            _ => None,
        }
    }

    /// Everything the panel received since the last call (crossings with grab modes ignored).
    fn events(&self) -> Vec<Ev> {
        self.conn.get_input_focus().unwrap().reply().unwrap(); // flush + sync
        let mut out = Vec::new();
        while let Some(event) = self.conn.poll_for_event().unwrap() {
            let ev = match event {
                Event::EnterNotify(e) if e.mode == NotifyMode::NORMAL => {
                    self.part(e.event).map(Ev::Enter)
                }
                Event::LeaveNotify(e) if e.mode == NotifyMode::NORMAL => {
                    self.part(e.event).map(Ev::Leave)
                }
                Event::ButtonPress(e) => self.part(e.event).map(Ev::Press),
                Event::ButtonRelease(e) => self.part(e.event).map(Ev::Release),
                _ => None,
            };
            out.extend(ev);
        }
        out
    }

    fn destroy(self) {
        self.conn.destroy_window(self.top).unwrap();
        self.conn.flush().unwrap();
    }
}

// ---------------------------------------------------------------------------------------------
// page control through the test-control socket (reads DOM state / calls the spike API only;
// real pointer and key input always arrives through KWin's EIS)
// ---------------------------------------------------------------------------------------------

fn request(s: &Stack, value: Value) -> Value {
    let mut stream = UnixStream::connect(s.root.join("inst/test-control.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(90)))
        .unwrap();
    writeln!(stream, "{value}").unwrap();
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).unwrap();
    let reply: Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply["ok"], true, "request {value}: {reply}");
    reply
}

fn eval(s: &Stack, js: &str) -> Value {
    request(s, json!({"op": "eval", "js": js, "timeoutMs": 60000}))["value"].clone()
}

fn counters(s: &Stack) -> Value {
    eval(s, "window.__spike.state().counters")
}

fn until_page(s: &Stack, what: &str, js: &str) {
    wait_for(what, Duration::from_secs(30), || {
        (eval(s, js) == true).then_some(())
    });
}

// ---------------------------------------------------------------------------------------------
// window-manager state helpers
// ---------------------------------------------------------------------------------------------

fn stacking(conn: &RustConnection, root: u32) -> Vec<u32> {
    prop_bytes(conn, root, "_NET_CLIENT_LIST_STACKING", AtomEnum::WINDOW)
        .unwrap_or_default()
        .chunks(4)
        .map(|c| u32::from_ne_bytes(c.try_into().unwrap()))
        .collect()
}

/// `_NET_CLIENT_LIST_STACKING` is bottom-to-top.
fn above(conn: &RustConnection, root: u32, upper: u32, lower: u32) -> bool {
    let order = stacking(conn, root);
    let pos = |w| order.iter().position(|x| *x == w);
    matches!((pos(upper), pos(lower)), (Some(u), Some(l)) if u > l)
}

fn label(p: [u8; 3]) -> &'static str {
    if near(p, ISLAND_YELLOW, 3) {
        "island"
    } else if near(p, PILL_TEAL, 3) {
        "panel-pill"
    } else if near(p, PANEL_BLUE, 3) {
        "panel"
    } else {
        "other"
    }
}

fn pill_pixel(s: &Stack) -> &'static str {
    let shot = s.capture();
    label(shot.at(PILL_CENTRE.0 as usize, PILL_CENTRE.1 as usize))
}

fn wait_region(conn: &RustConnection, window: u32, want: &[ShapeRow], what: &str) {
    let want = normalize_rows(want);
    wait_for(what, Duration::from_secs(20), || {
        (normalize_rows(&input_region(conn, window)) == want).then_some(())
    });
}

/// Mutation-testing hook ONLY (set SPIKE_MUTATION_SKIP_REGION_READBACK): lets a deliberately broken region
/// reach the physical-input assertions instead of failing at the readback, like Phase 1.5 did.
fn region_readback_enabled() -> bool {
    std::env::var_os("SPIKE_MUTATION_SKIP_REGION_READBACK").is_none()
}

fn hover_rows() -> Vec<ShapeRow> {
    // The spike page's hover rect in window coordinates (ui/spike-main.tsx HOVER).
    shape_rows(rr(380., 4., 420., 150., 28.), IslandMode::Hover, 1.0, true).unwrap()
}

fn assert_spike_identity(conn: &RustConnection, window: u32, pid: u32, type_atom: &str) {
    assert_eq!(
        prop_u32(conn, window, "_NET_WM_PID", AtomEnum::CARDINAL),
        Some(pid)
    );
    let name = prop_bytes(conn, window, "_NET_WM_NAME", atom(conn, "UTF8_STRING")).unwrap();
    assert_eq!(String::from_utf8(name).unwrap(), identity::TITLE_OVERLAY);
    let class = prop_bytes(conn, window, "WM_CLASS", AtomEnum::STRING).unwrap();
    let mut parts = class.split(|b| *b == 0);
    assert_eq!(parts.next(), Some(identity::WM_CLASS_INSTANCE.as_bytes()));
    assert_eq!(parts.next(), Some(identity::WM_CLASS_CLASS.as_bytes()));
    let want = atom(conn, type_atom);
    let types = prop_bytes(conn, window, "_NET_WM_WINDOW_TYPE", AtomEnum::ATOM).unwrap();
    assert!(
        types
            .chunks(4)
            .any(|c| u32::from_ne_bytes(c.try_into().unwrap()) == want),
        "window type must be {type_atom}"
    );
}

/// KWin kept the requested position (top-left over the panel) and added no frame.
fn assert_over_the_panel(conn: &RustConnection, window: u32, root: u32) {
    let t = conn
        .translate_coordinates(window, root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(
        (t.dst_x, t.dst_y),
        WIN_ORIGIN,
        "KWin must keep the island window at y=0, over the {PANEL_H} px panel strut"
    );
    let extents = prop_bytes(conn, window, "_NET_FRAME_EXTENTS", AtomEnum::CARDINAL);
    assert!(
        extents.as_ref().is_none_or(|e| e.iter().all(|b| *b == 0)),
        "island must be undecorated: {extents:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// (a) stacking + input
// ---------------------------------------------------------------------------------------------

struct Variant {
    name: &'static str,
    spec: &'static str,
    type_atom: &'static str,
    /// Does the stand-in dock accept focus (KWin may activate it on click)?
    dock_takes_focus: bool,
    /// Expected: island drawn over the panel pill right after it mapped.
    over_at_start: bool,
    /// Expected: island still drawn over the panel after the dock was clicked/activated.
    over_after_dock_click: bool,
}

const VARIANTS: [Variant; 4] = [
    Variant {
        name: "toolbar-no-above",
        spec: "type=toolbar,above=0",
        type_atom: "_NET_WM_WINDOW_TYPE_TOOLBAR",
        dock_takes_focus: true,
        over_at_start: false,
        over_after_dock_click: false,
    },
    Variant {
        name: "toolbar-above",
        spec: "type=toolbar,above=1",
        type_atom: "_NET_WM_WINDOW_TYPE_TOOLBAR",
        dock_takes_focus: true,
        over_at_start: true,
        over_after_dock_click: false,
    },
    Variant {
        name: "toolbar-above-nofocus-dock",
        spec: "type=toolbar,above=1",
        type_atom: "_NET_WM_WINDOW_TYPE_TOOLBAR",
        dock_takes_focus: false,
        over_at_start: true,
        over_after_dock_click: true,
    },
    Variant {
        name: "notification",
        spec: "type=notification,above=1",
        type_atom: "_NET_WM_WINDOW_TYPE_NOTIFICATION",
        dock_takes_focus: true,
        over_at_start: true,
        over_after_dock_click: true,
    },
];

struct Session<'a> {
    s: &'a mut Stack,
    conn: RustConnection,
    root: u32,
    panel: Panel,
    app: u32,
    window: u32,
}

impl<'a> Session<'a> {
    fn start(s: &'a mut Stack, spec: &str, dock_takes_focus: bool, type_atom: &str) -> Self {
        let conn = s.connect();
        let root = conn.setup().roots[0].root;
        let panel = Panel::create(&s.display, dock_takes_focus);
        wait_for("panel is a KWin client", Duration::from_secs(10), || {
            stacking(&conn, root).contains(&panel.top).then_some(())
        });
        let app = s.start_island(spec);
        let window = wait_for("spike island mapped", Duration::from_secs(60), || {
            find_island_window(&conn, root, app)
        });
        assert_spike_identity(&conn, window, app, type_atom);
        assert_over_the_panel(&conn, window, root);
        assert_not_focused_on_show(&conn, root, window);
        wait_for("test socket", Duration::from_secs(20), || {
            s.root.join("inst/test-control.sock").exists().then_some(())
        });
        until_page(
            s,
            "spike page ready",
            "document.querySelector('.spike-island')?.dataset.ready==='1'",
        );
        // Native-pill Still: applied before show, so the readback is EMPTY (not the X default).
        if region_readback_enabled() {
            wait_region(&conn, window, &[], "empty input region over the pill");
        }
        Session {
            s,
            conn,
            root,
            panel,
            app,
            window,
        }
    }

    fn finish(self) {
        self.panel.destroy();
        self.s.stop_child(self.app);
        wait_for("island window gone", Duration::from_secs(10), || {
            (!stacking(&self.conn, self.root).contains(&self.window)).then_some(())
        });
    }
}

/// Real hover + click on the pill: the PANEL must receive them although the island is drawn there.
fn pill_reaches_panel(sess: &Session, eis: &mut Eis, what: &str) {
    eis.move_to(300, 500);
    sess.panel.events();
    eis.move_to(PILL_CENTRE.0, PILL_CENTRE.1);
    let hover = sess.panel.events();
    assert!(
        hover.contains(&Ev::Enter(Part::Pill)),
        "{what}: panel pill got no hover (enter) at the pill: {hover:?}"
    );
    eis.click();
    let click = sess.panel.events();
    assert!(
        click.contains(&Ev::Press(Part::Pill)) && click.contains(&Ev::Release(Part::Pill)),
        "{what}: panel pill got no click through the island's empty input region: {click:?}"
    );
    eis.move_to(300, 20);
    let leave = sess.panel.events();
    assert!(
        leave.contains(&Ev::Leave(Part::Pill)),
        "{what}: panel pill got no leave: {leave:?}"
    );
}

fn stacking_variant(s: &mut Stack, eis: &mut Eis, v: &Variant, evidence: &Evidence) -> Value {
    let sess = Session::start(s, v.spec, v.dock_takes_focus, v.type_atom);
    let (conn, root, window) = (&sess.conn, sess.root, sess.window);
    let over_start = above(conn, root, window, sess.panel.top);
    // Phase 3 flake fix (controller audit c): the stacking list is current as soon as the window
    // is mapped, but KWin may not have composited the new window yet. Poll captures (bounded,
    // 2 s) until the pixel agrees with the expected side; the assertion below is unchanged, so a
    // wrong stacking order still fails after the bound.
    let settle_deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut px_start = pill_pixel(sess.s);
    while (px_start == "island") != v.over_at_start && std::time::Instant::now() < settle_deadline {
        sleep(Duration::from_millis(50));
        px_start = pill_pixel(sess.s);
    }
    assert_eq!(
        (over_start, px_start == "island"),
        (v.over_at_start, v.over_at_start),
        "{}: stacking list and composited pixel at the pill must agree that the island is {} the panel at start (pixel: {px_start})",
        v.name,
        if v.over_at_start { "over" } else { "under" }
    );
    if v.over_at_start {
        sess.s
            .capture()
            .crop(500, 0, 700, 120)
            .save_png(&evidence.path(&format!("stack-{}-start.png", v.name)));
    }
    pill_reaches_panel(&sess, eis, v.name);
    // The island saw neither hover nor click, and was never activated or given a focus request.
    let seen = counters(sess.s);
    assert_eq!(
        (
            seen["pointerenter"].as_u64(),
            seen["pointerdown"].as_u64(),
            seen["click"].as_u64()
        ),
        (Some(0), Some(0), Some(0)),
        "{}: island must not see the pill pointer events: {seen}",
        v.name
    );
    assert_no_focus_requested(conn, window);
    let (_, active) = focus_report(conn, root);
    assert_ne!(
        active, window,
        "{}: island became the active window",
        v.name
    );
    if region_readback_enabled() {
        wait_region(conn, window, &[], "input region still empty");
    }
    // After the (possibly activating) panel click.
    let over_after = above(conn, root, window, sess.panel.top);
    let px_after = pill_pixel(sess.s);
    assert_eq!(
        (over_after, px_after == "island"),
        (v.over_after_dock_click, v.over_after_dock_click),
        "{}: after the dock click the island must be {} the panel (list {over_after}, pixel {px_after}; active window {active:#x}, panel {:#x})",
        v.name,
        if v.over_after_dock_click { "still over" } else { "buried under" },
        sess.panel.top
    );
    sess.s
        .capture()
        .crop(500, 0, 700, 120)
        .save_png(&evidence.path(&format!("stack-{}-after-dock-click.png", v.name)));
    let result = json!({
        "variant": v.name, "spec": v.spec, "dockTakesFocus": v.dock_takes_focus,
        "overAtStart": over_start, "pixelAtStart": px_start,
        "overAfterDockClick": over_after, "pixelAfterDockClick": px_after,
        "activeAfterDockClick": if active == sess.panel.top { "panel" } else { "other" },
        "panelGotHoverAndClick": true, "islandSawNoPointerEvents": true,
    });
    sess.finish();
    result
}

// ---------------------------------------------------------------------------------------------
// (a, continued) expanded region, keyboard, close; (b) morph; (d) pacing — notification variant
// ---------------------------------------------------------------------------------------------

fn expanded_region_input(sess: &Session, eis: &mut Eis) {
    let (conn, root, window) = (&sess.conn, sess.root, sess.window);
    let opened = eval(sess.s, "window.__spike.open(true)");
    assert_eq!(
        opened["errors"],
        json!([]),
        "geometry reports failed: {opened}"
    );
    wait_region(
        conn,
        window,
        &hover_rows(),
        "hover region == rounded hover rows",
    );
    sess.panel.events();
    // Inside the expanded island, below the panel strip ...
    eis.move_to(800, 100);
    eis.click();
    assert_eq!(
        counters(sess.s)["click"],
        1,
        "island must receive the click"
    );
    assert_eq!(
        focus_report(conn, root).1,
        window,
        "click-activation of the island"
    );
    assert!(
        sess.panel
            .events()
            .iter()
            .all(|e| !matches!(e, Ev::Press(_) | Ev::Release(_))),
        "the panel must not see a click that landed inside the island"
    );
    // ... keyboard goes to the island's page, never submitted (no Enter) ...
    eis.type_text("ab");
    let after_keys = eval(sess.s, "window.__spike.state()");
    assert_eq!(after_keys["counters"]["keydown"], 2, "{after_keys}");
    assert_eq!(after_keys["hasFocus"], true);
    // ... and at the old pill position, now covered by the expanded island.
    eis.move_to(PILL_CENTRE.0, PILL_CENTRE.1);
    eis.click();
    assert_eq!(counters(sess.s)["click"], 2);
    assert!(
        sess.panel
            .events()
            .iter()
            .all(|e| !matches!(e, Ev::Press(_) | Ev::Release(_))),
        "the expanded island must now own the pill position"
    );
    // The panel outside the island still works.
    eis.move_to(300, 20);
    eis.click();
    assert!(
        sess.panel.events().contains(&Ev::Press(Part::Top)),
        "panel outside the island must keep receiving clicks"
    );
    // Close: empty region again and the pill is the panel's once more.
    let closed = eval(sess.s, "window.__spike.close(true)");
    assert_eq!(closed["errors"], json!([]), "{closed}");
    wait_region(conn, window, &[], "region empty again after closing");
    pill_reaches_panel(sess, eis, "after close");
    assert!(
        above(conn, root, window, sess.panel.top),
        "NOTIFICATION island must stay over the (just activated) dock"
    );
}

fn read_burst(
    s: &Stack,
    burst: &mut spike_stack::Burst,
    start_js: &str,
    dir: &Path,
) -> (Vec<(f64, Measured, Capture)>, Value) {
    burst.start((500, 0, 700, 170), 450, dir);
    sleep(Duration::from_millis(80));
    assert!(eval(s, start_js) == true);
    let meta = burst.finish();
    let stride = meta["stride"].as_u64().unwrap() as usize;
    let first = meta["frames"][0]["t0"].as_f64().unwrap();
    let mut frames = Vec::new();
    for f in meta["frames"].as_array().unwrap() {
        let i = f["i"].as_u64().unwrap();
        let raw = fs::read(dir.join(format!("frame_{i:03}.raw"))).unwrap();
        let cap = Capture::from_raw(&raw, stride, meta["format"].as_u64().unwrap(), 700, 170);
        let t_ms = (f["t1"].as_f64().unwrap() - first) * 1000.;
        let m = measure(&cap, (500, 0)).unwrap_or_else(|e| {
            let name = format!("morph-frame-{i:03}-FAILED.png");
            cap.save_png(&Evidence::new("phase2_5").path(&name));
            panic!("frame {i} (t+{t_ms:.0} ms) is not a coherent island: {e} (saved {name})")
        });
        frames.push((t_ms, m, cap));
    }
    (frames, meta)
}

fn contact_sheet(frames: &[(f64, Measured, Capture)], path: &Path) {
    // 480x140 crop around the pill/hover area; 4 columns with a 2 px magenta gutter.
    let (cw, ch, cols) = (480usize, 160usize, 4usize);
    let rows = frames.len().div_ceil(cols);
    let mut sheet = Capture::solid(cols * (cw + 2), rows * (ch + 2), [255, 0, 255]);
    for (n, (_, _, cap)) in frames.iter().enumerate() {
        let tile = cap.crop(60, 0, cw, ch);
        let (ox, oy) = ((n % cols) * (cw + 2), (n / cols) * (ch + 2));
        for y in 0..ch {
            for x in 0..cw {
                sheet.set(ox + x, oy + y, tile.at(x, y));
            }
        }
    }
    sheet.save_png(path);
}

fn check_morph(frames: &[(f64, Measured, Capture)], opening: bool, label: &str) -> Value {
    let (first, last) = (&frames[0].1, &frames[frames.len() - 1].1);
    let (start, end) = if opening {
        (first, last)
    } else {
        (last, first)
    };
    // The morph starts exactly on the pill rectangle (panel height) ...
    assert!((end.bbox.0 - 590).abs() <= 1, "{label}: hover x {end:?}");
    assert!(
        (start.bbox.0 - 725).abs() <= 1 && (start.bbox.1 - 4).abs() <= 1,
        "{label}: starts at the pill origin: {start:?}"
    );
    assert!(
        (start.w - 150).abs() <= 2 && (start.h - 36).abs() <= 2,
        "{label}: starts at the pill size 150x36: {start:?}"
    );
    // ... and ends on the hover size 420x150 (radius 28 is enforced by `measure`'s model).
    assert!(
        (end.w - 420).abs() <= 2 && (end.h - 150).abs() <= 2,
        "{label}: ends at the hover size 420x150: {end:?}"
    );
    let mid = frames
        .iter()
        .filter(|(_, m, _)| m.t > 0.04 && m.t < 0.96)
        .count();
    assert!(
        mid >= 12,
        "{label}: only {mid} distinct in-flight frames were captured"
    );
    // Monotonic growth/shrink, one coherent shape per frame, no pill remnant (seam).
    let mut worst_teal = 0;
    for pair in frames.windows(2) {
        let (a, b) = (&pair[0].1, &pair[1].1);
        let step = if opening { b.w - a.w } else { a.w - b.w };
        assert!(
            step >= -1,
            "{label}: width went the wrong way: {a:?} -> {b:?}"
        );
    }
    for (t_ms, m, _) in frames {
        worst_teal = worst_teal.max(m.teal);
        assert!(
            m.teal <= MAX_TEAL_PX,
            "{label}: pill colour visible beside the island at t+{t_ms:.0} ms ({} px): a seam/gap",
            m.teal
        );
    }
    // Skip the idle lead-in (the static first frame to the first moving one): only gaps
    // between in-flight frames say anything about dropped frames.
    let gaps: Vec<f64> = frames.windows(2).skip(1).map(|p| p[1].0 - p[0].0).collect();
    json!({"label": label, "distinctFrames": frames.len(), "inFlightFrames": mid,
        "widths": frames.iter().map(|f| f.1.w).collect::<Vec<_>>(),
        "worstTealPx": worst_teal,
        "maxGapBetweenInFlightFramesMs": gaps.iter().cloned().fold(0., f64::max)})
}

fn pacing_summary(results: &Value) -> Value {
    let all = |key: &str| -> Vec<f64> {
        results
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| {
                r[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
            })
            .collect()
    };
    let deltas = stats(&all("deltas"));
    let acks = all("ackMs");
    let reports: u64 = results
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["reports"].as_u64().unwrap())
        .sum();
    let errors: usize = results
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["errors"].as_array().unwrap().len())
        .sum();
    assert_eq!(errors, 0, "geometry reports failed during the morphs");
    let ack = (!acks.is_empty()).then(|| stats(&acks));
    json!({"morphs": results.as_array().unwrap().len(), "frames": deltas.n,
        "rafDeltaMs": {"p50": deltas.p50, "p95": deltas.p95, "max": deltas.max, "over20ms": deltas.over_20ms},
        "reports": reports,
        "ackLatencyMs": ack.map(|a| json!({"p50": a.p50, "p95": a.p95, "max": a.max}))})
}

/// DRM render-node descriptors (/dev/dri/renderD*) held by the app and by its direct children
/// (WebKit web/network processes): non-zero means the process talks to the GPU, zero software.
fn render_node_fds(app: u32) -> Value {
    let mut report = serde_json::Map::new();
    for entry in fs::read_dir("/proc").unwrap().filter_map(Result::ok) {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let stat = fs::read_to_string(entry.path().join("stat")).unwrap_or_default();
        let ppid = stat
            .rsplit_once(')')
            .and_then(|(_, r)| r.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u32>().ok());
        if pid != app && ppid != Some(app) {
            continue;
        }
        let comm = fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
        let fds = fs::read_dir(entry.path().join("fd"))
            .map(|d| {
                d.filter_map(Result::ok)
                    .filter_map(|f| fs::read_link(f.path()).ok())
                    .filter(|t| t.to_string_lossy().starts_with("/dev/dri/renderD"))
                    .count()
            })
            .unwrap_or(0);
        report.insert(format!("{}:{pid}", comm.trim()), json!(fds));
    }
    Value::Object(report)
}

// ---------------------------------------------------------------------------------------------
// (c) blur
// ---------------------------------------------------------------------------------------------

/// A normal, undecorated window painted with an 8 px black/white checkerboard (tiled background
/// pixmap) at screen (500, 60) 700x300: high contrast, below the panel strip.
fn pattern_window(display: &str) -> (RustConnection, u32) {
    let conn = RustConnection::connect(Some(display)).unwrap().0;
    let setup = conn.setup();
    let root = setup.roots[0].root;
    let depth = setup.roots[0].root_depth;
    let bpp = setup
        .pixmap_formats
        .iter()
        .find(|f| f.depth == depth)
        .unwrap()
        .bits_per_pixel;
    assert_eq!(bpp, 32, "checkerboard tile assumes 32 bpp pixmaps");
    let mut data = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            let v = if (x / 8 + y / 8) % 2 == 0 { 255u8 } else { 0 };
            data.extend_from_slice(&[v, v, v, 0]);
        }
    }
    let (pixmap, gc, window) = (
        conn.generate_id().unwrap(),
        conn.generate_id().unwrap(),
        conn.generate_id().unwrap(),
    );
    conn.create_pixmap(depth, pixmap, root, 16, 16).unwrap();
    conn.create_gc(gc, pixmap, &CreateGCAux::new()).unwrap();
    conn.put_image(
        ImageFormat::Z_PIXMAP,
        pixmap,
        gc,
        16,
        16,
        0,
        0,
        0,
        depth,
        &data,
    )
    .unwrap();
    conn.create_window(
        0,
        window,
        root,
        500,
        60,
        700,
        300,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new().background_pixmap(pixmap),
    )
    .unwrap();
    // _MOTIF_WM_HINTS: decorations = none.
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atom(&conn, "_MOTIF_WM_HINTS"),
        atom(&conn, "_MOTIF_WM_HINTS"),
        &[2, 0, 0, 0, 0],
    )
    .unwrap();
    set_name(&conn, window, "spike-pattern");
    user_position(&conn, window, 500, 60, 700, 300);
    conn.map_window(window).unwrap();
    conn.flush().unwrap();
    wait_for("pattern window viewable", Duration::from_secs(10), || {
        let attrs = conn.get_window_attributes(window).ok()?.reply().ok()?;
        (attrs.map_state == MapState::VIEWABLE).then_some(())
    });
    (conn, window)
}

/// None = the property does not exist (no blur); Some(empty) = it exists but is EMPTY, which
/// KWin treats as "blur the whole window". The distinction is the point: prop_bytes cannot make
/// it (it maps a zero-length value to None).
fn blur_property(conn: &RustConnection, window: u32) -> Option<Vec<ShapeRow>> {
    let reply = conn
        .get_property(
            false,
            window,
            atom(conn, "_KDE_NET_WM_BLUR_BEHIND_REGION"),
            AtomEnum::CARDINAL,
            0,
            4096,
        )
        .unwrap()
        .reply()
        .unwrap();
    if reply.type_ == u32::from(AtomEnum::NONE) {
        return None;
    }
    assert_eq!(reply.type_, u32::from(AtomEnum::CARDINAL));
    let bytes = reply.value;
    let values: Vec<u32> = bytes
        .chunks(4)
        .map(|c| u32::from_ne_bytes(c.try_into().unwrap()))
        .collect();
    assert_eq!(values.len() % 4, 0, "blur property is x,y,w,h quadruples");
    Some(
        values
            .chunks(4)
            .map(|q| ShapeRow {
                x: q[0] as i16,
                y: q[1] as i16,
                width: q[2] as u16,
                height: q[3] as u16,
            })
            .collect(),
    )
}

/// Patches (screen px) — all over the checkerboard (x 500..1200, y 60..360), none near its corners:
/// inside the open island, beside it, and in the cut-away bottom-left rounded corner.
const INSIDE: (usize, usize, usize, usize) = (700, 80, 200, 60);
const BESIDE: (usize, usize, usize, usize) = (1030, 80, 150, 60);
const CORNER: (usize, usize, usize, usize) = (591, 149, 4, 4);

fn patch_equal(a: &Capture, b: &Capture, p: (usize, usize, usize, usize)) -> bool {
    (p.1..p.1 + p.3).all(|y| (p.0..p.0 + p.2).all(|x| a.at(x, y) == b.at(x, y)))
}

fn translucent_open(s: &Stack) {
    assert!(eval(s, "window.__spike.setAlpha(0.3)") == true);
    let opened = eval(s, "window.__spike.open(true)");
    assert_eq!(opened["errors"], json!([]), "{opened}");
    sleep(Duration::from_millis(400)); // KWin applies the new buffer/property and blurs
}

// ---------------------------------------------------------------------------------------------
// the scenario
// ---------------------------------------------------------------------------------------------

#[test]
fn kwin_spike_panel() {
    let _guard = lock();
    let evidence = Evidence::new("phase2_5");
    let mut summary = json!({});
    let mut stack = Stack::new("spike");
    let mut eis = stack.eis();

    // Environment facts, recorded rather than assumed.
    let support = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.kde.KWin",
            "--object-path",
            "/KWin",
            "--method",
            "org.kde.KWin.supportInformation",
        ])
        .env_clear()
        .envs(stack.env())
        .output()
        .unwrap();
    let support = String::from_utf8_lossy(&support.stdout).replace("\\n", "\n");
    let line = |key: &str| {
        support
            .lines()
            .find(|l| l.starts_with(key))
            .unwrap_or("")
            .trim()
            .to_owned()
    };
    summary["compositor"] = json!({
        "type": line("Compositing Type:"), "renderer": line("OpenGL renderer string:"),
        "refresh": line("Refresh Rate:"),
        "blurEffectLoaded": support.split("Loaded Effects:").nth(1).is_some_and(|e| e.lines().take(40).any(|l| l.trim() == "blur")),
    });
    assert!(
        summary["compositor"]["type"]
            .as_str()
            .unwrap()
            .contains("OpenGL"),
        "blur needs OpenGL compositing: {}",
        summary["compositor"]
    );
    assert_eq!(
        summary["compositor"]["blurEffectLoaded"], true,
        "KWin blur effect must be loaded for the blur check"
    );

    // ---- (a) stacking + input matrix ----
    let mut matrix = Vec::new();
    for v in &VARIANTS {
        matrix.push(stacking_variant(&mut stack, &mut eis, v, &evidence));
    }
    summary["stackingMatrix"] = Value::Array(matrix);

    // ---- winner: NOTIFICATION. expanded input, keyboard, morph, pacing, blur baseline ----
    let sess = Session::start(
        &mut stack,
        "type=notification,above=1",
        true,
        "_NET_WM_WINDOW_TYPE_NOTIFICATION",
    );
    expanded_region_input(&sess, &mut eis);
    let Session {
        s,
        conn,
        panel,
        app,
        window,
        root,
    } = sess;
    // Park the pointer away from the morph area so no hover state interferes with captures.
    eis.move_to(300, 600);
    panel.events();

    // (b) morph burst, open then close, on real compositor frames
    let mut burst = s.burst_helper();
    let open_dir = s.root.join("burst-open");
    let (open, _) = read_burst(s, &mut burst, "window.__spike.startOpen(true)", &open_dir);
    until_page(s, "open settled", "window.__spike.last()!==null");
    let mut morph = vec![check_morph(&open, true, "open")];
    contact_sheet(&open, &evidence.path("morph-open-contact-sheet.png"));
    open[0]
        .2
        .save_png(&evidence.path("morph-open-first-frame.png"));
    open[open.len() / 2]
        .2
        .save_png(&evidence.path("morph-open-mid-frame.png"));
    open[open.len() - 1]
        .2
        .save_png(&evidence.path("morph-open-last-frame.png"));
    let close_dir = s.root.join("burst-close");
    let (close, _) = read_burst(s, &mut burst, "window.__spike.startClose(true)", &close_dir);
    until_page(s, "close settled", "window.__spike.last()!==null");
    morph.push(check_morph(&close, false, "close"));
    contact_sheet(&close, &evidence.path("morph-close-contact-sheet.png"));
    summary["morph"] = Value::Array(morph);
    wait_region(&conn, window, &[], "region empty after the morphs");

    // (d) frame pacing: 10 open+close cycles with per-frame geometry reports, 10 without
    summary["webgl"] = eval(s, "window.__spike.webgl()");
    let with = eval(s, "window.__spike.cycles(10, true)");
    wait_region(&conn, window, &[], "empty after pacing run (with reports)");
    let without = eval(s, "window.__spike.cycles(10, false)");
    summary["pacing"] = json!({
        "withGeometryReports": pacing_summary(&with),
        "withoutReports": pacing_summary(&without),
        "note": "nested KWin: OpenGL ES on the host GPU into a VIRTUAL output (no scanout/vsync); not the real desktop",
    });
    for key in ["withGeometryReports", "withoutReports"] {
        let p = &summary["pacing"][key];
        assert!(
            p["frames"].as_u64().unwrap() >= 300,
            "too few frames measured: {p}"
        );
        let p50 = p["rafDeltaMs"]["p50"].as_f64().unwrap();
        assert!(
            (8.0..=25.0).contains(&p50),
            "implausible rAF median {p50} ms: {p}"
        );
    }
    assert!(
        summary["pacing"]["withGeometryReports"]["reports"]
            .as_u64()
            .unwrap()
            > 100
    );
    // Does WebKit render on the GPU? Render-node fds of THIS app and its direct children only
    // (the live island-rust has the same process names; never look at it).
    summary["renderNodeFds"] = render_node_fds(app);

    // (c) baseline without blur: translucent island over the checkerboard stays sharp
    let (pattern_conn, pattern) = pattern_window(&s.display);
    sleep(Duration::from_millis(600));
    let reference = s.capture(); // island closed: pattern + panel only
    reference.save_png(&evidence.path("blur-reference-island-closed.png"));
    assert!(
        sharpness(&reference, INSIDE.0, INSIDE.1, INSIDE.2, INSIDE.3) > 0.10,
        "the checkerboard reference must be sharp (pattern window painted?)"
    );
    translucent_open(s);
    let baseline = s.capture();
    baseline.save_png(&evidence.path("blur-off-baseline.png"));
    let base_inside = sharpness(&baseline, INSIDE.0, INSIDE.1, INSIDE.2, INSIDE.3);
    assert!(
        base_inside > 0.10,
        "no blur property: pattern seen through the island stays sharp ({base_inside})"
    );
    assert!(
        blur_property(&conn, window).is_none(),
        "blur=0 must not set the property"
    );
    let _ = eval(s, "window.__spike.close(true)");
    panel.destroy();
    s.stop_child(app);
    wait_for("island gone", Duration::from_secs(10), || {
        (!stacking(&conn, root).contains(&window)).then_some(())
    });
    drop(conn);

    // (c) blur on: the property is mirrored from the input-region owner
    let sess = Session::start(
        s,
        "type=notification,above=1,blur=1",
        true,
        "_NET_WM_WINDOW_TYPE_NOTIFICATION",
    );
    assert!(
        blur_property(&sess.conn, sess.window).is_none(),
        "empty Still region ⇒ no blur property (an empty property would blur the WHOLE window)"
    );
    translucent_open(sess.s);
    let rows = blur_property(&sess.conn, sess.window).expect("blur property must be set when open");
    assert_eq!(
        normalize_rows(&rows),
        normalize_rows(&input_region(&sess.conn, sess.window)),
        "blur region must equal the input region applied by the same owner"
    );
    assert_eq!(normalize_rows(&rows), normalize_rows(&hover_rows()));
    let blurred = sess.s.capture();
    blurred.save_png(&evidence.path("blur-on.png"));
    let inside = sharpness(&blurred, INSIDE.0, INSIDE.1, INSIDE.2, INSIDE.3);
    let beside = sharpness(&blurred, BESIDE.0, BESIDE.1, BESIDE.2, BESIDE.3);
    assert!(
        inside < 0.02,
        "blurred inside the island shape expected, sharpness {inside}"
    );
    assert!(
        beside > 0.10,
        "outside the shape must stay sharp, sharpness {beside}"
    );
    assert!(
        patch_equal(&blurred, &reference, BESIDE) && patch_equal(&blurred, &reference, CORNER),
        "pixels outside the rounded shape (beside it and in its cut-away corner) must equal the unblurred reference"
    );
    assert!(
        !patch_equal(&blurred, &baseline, INSIDE),
        "blur must change the pixels inside the shape"
    );
    let _ = eval(sess.s, "window.__spike.close(true)");
    wait_region(
        &sess.conn,
        sess.window,
        &[],
        "region empty after blur close",
    );
    wait_for(
        "blur property removed with the region",
        Duration::from_secs(10),
        || {
            blur_property(&sess.conn, sess.window)
                .is_none()
                .then_some(())
        },
    );
    summary["blur"] = json!({
        "propertyRows": rows.len(), "equalsInputRegion": true,
        "sharpness": {"insideBlurOn": inside, "insideBlurOff": base_inside, "besideBlurOn": beside},
        "outsideShapeIdenticalToReference": true,
    });
    sess.finish();

    drop(pattern_conn);
    let _ = pattern;
    fs::write(
        evidence.path("summary.json"),
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .unwrap();
    println!("PHASE2_5_SUMMARY {summary}");
    stack.shutdown();
    let scratch = stack.root.clone();
    drop(stack);
    assert!(!scratch.exists(), "spike scratch leaked");
}
