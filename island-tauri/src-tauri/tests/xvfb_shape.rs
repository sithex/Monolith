//! Phase 1 gate: the real Tauri window under an isolated Xvfb, read back with x11rb.
//!
//! Everything here is hermetic and fail-closed:
//! * private Xvfb (`-displayfd`), private D-Bus whose only activatable service is xfconfd (needed
//!   by xfwm4), scratch `HOME`/`XDG_*`, `env_clear()` for every child — nothing touches the
//!   user's display, session bus, portals, clipboard, config or the live island;
//! * every wait is a bounded poll on an observable condition and panics on timeout;
//! * missing prerequisites (Xvfb, xfwm4, dbus-daemon) are a test failure, never a skip.
//!
//! Scenarios (serialised, each spawns its own stack):
//! 1. `overlay_cycle_with_window_manager` — native-pill mode under xfwm4: identity props, not
//!    focused on show (with a positive control proving the WM *would* focus), still → hover →
//!    large → still ShapeInput readback, window ARGB pixels transparent outside the island.
//! 2. `preview_cycle_non_native_pill` — non-native mode (the still pill keeps a region), 32-bit ARGB
//!    visual, preview title. (A real compositor capture is NOT possible here: xfwm4 refuses the
//!    llvmpipe GL renderer and no other compositor is installed; see VERIFICATION.md.)
//! 3. `click_through_is_physical_without_window_manager` — XTest clicks against an underlay
//!    window prove pointer events really pass through outside the shape and not inside it.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use hermes_island_lib::window::shape::normalize_rows;
use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    thread::sleep,
    time::Duration,
};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, WindowClass},
        Event,
    },
    rust_connection::RustConnection,
    CURRENT_TIME,
};

// ---------------------------------------------------------------------------------------------
// sandbox
// ---------------------------------------------------------------------------------------------

struct Sandbox {
    root: PathBuf,
    display: String,
    bus: String,
    children: Vec<(String, Child)>,
    _xvfb_stdout: Option<ChildStdout>,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        for tool in ["Xvfb", "xfwm4", "dbus-daemon"] {
            require_tool(tool);
        }
        let root = std::env::temp_dir().join(format!("hi-xvfb-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for d in [
            "config", "data", "cache", "state", "run", "home", "inst", "services", "evidence",
        ] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::set_permissions(
            root.join("run"),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        let bus_path = root.join("bus");
        fs::write(
            root.join("services/org.xfce.Xfconf.service"),
            "[D-BUS Service]\nName=org.xfce.Xfconf\nExec=/usr/lib/xfce4/xfconf/xfconfd\n",
        )
        .unwrap();
        fs::write(
            root.join("bus.conf"),
            format!(
                "<!DOCTYPE busconfig PUBLIC \"-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN\" \
                 \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\">\n\
                 <busconfig><type>session</type><listen>unix:path={}</listen>\
                 <servicedir>{}</servicedir><auth>EXTERNAL</auth>\
                 <policy context=\"default\"><allow send_destination=\"*\" eavesdrop=\"true\"/>\
                 <allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>\n",
                bus_path.display(),
                root.join("services").display()
            ),
        )
        .unwrap();

        let mut sandbox = Sandbox {
            bus: format!("unix:path={}", bus_path.display()),
            display: String::new(),
            children: Vec::new(),
            _xvfb_stdout: None,
            root,
        };
        let log = |s: &Sandbox, n: &str| fs::File::create(s.root.join(n)).unwrap();
        let dbus = Command::new("dbus-daemon")
            .args(["--nofork", "--nosyslog", "--config-file"])
            .arg(sandbox.root.join("bus.conf"))
            .env_clear()
            .envs(sandbox.base_env())
            .process_group(0)
            .stdout(log(&sandbox, "dbus.log"))
            .stderr(log(&sandbox, "dbus.err"))
            .spawn()
            .unwrap();
        sandbox.children.push(("dbus-daemon".into(), dbus));
        wait_for("private bus socket", Duration::from_secs(5), || {
            bus_path.exists().then_some(())
        });

        let mut xvfb = Command::new("Xvfb")
            .args([
                "-screen",
                "0",
                &format!("{SCREEN_W}x{SCREEN_H}x24"),
                "-nolisten",
                "tcp",
                "-displayfd",
                "1",
            ])
            .env_clear()
            .envs(sandbox.base_env())
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(log(&sandbox, "xvfb.err"))
            .spawn()
            .unwrap();
        let mut stdout = xvfb.stdout.take().unwrap();
        let mut line = String::new();
        BufReader::new(&mut stdout).read_line(&mut line).unwrap();
        sandbox.display = format!(":{}", line.trim());
        assert!(
            line.trim().parse::<u32>().is_ok(),
            "Xvfb did not report a display: {line:?}"
        );
        sandbox._xvfb_stdout = Some(stdout);
        sandbox.children.push(("Xvfb".into(), xvfb));
        sandbox
    }

    /// Scratch-only environment for every child (the bus-activated `xfconfd` inherits the bus's
    /// environment, so the bus must get this too or it falls back to the real `$HOME`).
    fn base_env(&self) -> Vec<(&'static str, String)> {
        let path = |d: &str| self.root.join(d).display().to_string();
        vec![
            ("PATH", std::env::var("PATH").unwrap_or_default()),
            ("LANG", "C.UTF-8".into()),
            ("HOME", path("home")),
            ("XDG_CONFIG_HOME", path("config")),
            ("XDG_DATA_HOME", path("data")),
            ("XDG_CACHE_HOME", path("cache")),
            ("XDG_STATE_HOME", path("state")),
            ("XDG_RUNTIME_DIR", path("run")),
            ("GDK_BACKEND", "x11".into()),
            ("NO_AT_BRIDGE", "1".into()),
            ("GTK_USE_PORTAL", "0".into()),
            ("GIO_USE_PORTAL", "0".into()),
            ("HERMES_ISLAND_TEST", "1".into()),
            ("RUST_LOG", "info".into()),
        ]
    }

    fn env(&self) -> Vec<(&'static str, String)> {
        let mut env = self.base_env();
        env.push(("DISPLAY", self.display.clone()));
        env.push(("DBUS_SESSION_BUS_ADDRESS", self.bus.clone()));
        env
    }

    fn spawn(&mut self, name: &str, program: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> u32 {
        let out = fs::File::create(self.root.join(format!("{name}.log"))).unwrap();
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(self.env())
            .process_group(0)
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .unwrap();
        let pid = child.id();
        self.children.push((name.into(), child));
        pid
    }

    fn connect(&self) -> RustConnection {
        wait_for("X connection", Duration::from_secs(10), || {
            RustConnection::connect(Some(&self.display))
                .ok()
                .map(|(c, _)| c)
        })
    }

    fn start_window_manager(&mut self, compositor: bool) {
        self.spawn(
            "xfwm4",
            "xfwm4",
            &[if compositor {
                "--compositor=on"
            } else {
                "--compositor=off"
            }],
        );
        let conn = self.connect();
        let root = conn.setup().roots[0].root;
        let check = atom(&conn, "_NET_SUPPORTING_WM_CHECK");
        wait_for(
            "window manager (_NET_SUPPORTING_WM_CHECK)",
            Duration::from_secs(15),
            || {
                let reply = conn
                    .get_property(false, root, check, AtomEnum::WINDOW, 0, 1)
                    .ok()?
                    .reply()
                    .ok()?;
                (reply.value_len > 0).then_some(())
            },
        );
    }

    fn start_app(&mut self, overlay: bool) -> u32 {
        let data = self.root.join("inst");
        let data = data.to_str().unwrap().to_owned();
        let mut args = vec!["--test-instance", "--data-dir", data.as_str()];
        if overlay {
            args.push("--overlay");
        }
        self.spawn("app", env!("CARGO_BIN_EXE_hermes-island"), &args)
    }

    fn app_exited(&mut self) -> Option<String> {
        let (_, child) = self.children.iter_mut().find(|(n, _)| n == "app")?;
        child.try_wait().ok().flatten().map(|s| s.to_string())
    }

    fn evidence(&self, name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../evidence/phase1");
        fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if std::thread::panicking() {
            for log in ["app.log", "xfwm4.log", "xvfb.err", "dbus.err"] {
                if let Ok(text) = fs::read_to_string(self.root.join(log)) {
                    let tail: Vec<&str> = text.lines().rev().take(25).collect();
                    eprintln!(
                        "---- {log} (last lines) ----\n{}",
                        tail.into_iter().rev().collect::<Vec<_>>().join("\n")
                    );
                }
            }
        }
        for (_, child) in self.children.iter_mut().rev() {
            stop_group(child);
        }
        // Children are gone; retry briefly in case a straggler was still flushing into the dir.
        for _ in 0..10 {
            if fs::remove_dir_all(&self.root).is_ok() || !self.root.exists() {
                break;
            }
            sleep(Duration::from_millis(50));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// scenarios
// ---------------------------------------------------------------------------------------------

#[test]
fn overlay_cycle_with_window_manager() {
    let _g = lock();
    let mut sb = Sandbox::new("wm");
    sb.start_window_manager(false);
    let pid = sb.start_app(true);
    let conn = sb.connect();
    let root = conn.setup().roots[0].root;
    let window = wait_for("island window mapped", Duration::from_secs(60), || {
        assert!(
            sb.app_exited().is_none(),
            "app exited early: {:?}",
            sb.app_exited()
        );
        find_island_window(&conn, root, pid)
    });

    // identity (frozen in identity.rs)
    assert_identity(&conn, window, pid, true);

    // the window is where placement asked: top-centre of the (whole-screen) work area
    let geo = conn.get_geometry(window).unwrap().reply().unwrap();
    let origin = conn
        .translate_coordinates(window, root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!((geo.width, geo.height), (WIN_W, WIN_H));
    assert_eq!(
        (origin.dst_x, origin.dst_y),
        (((SCREEN_W - WIN_W) / 2) as i16, 0),
        "window origin"
    );

    // native-pill still is applied BEFORE show: the very first observation is already empty
    // (an unset input shape reads back as one full-window rectangle).
    assert!(
        input_region(&conn, window).is_empty(),
        "native-pill still must be an explicitly EMPTY input region"
    );

    // not focused on show
    assert_not_focused_on_show(&conn, root, window);
    // positive control: this WM focuses an ordinary new window, so the assertion above has teeth
    let control = conn.generate_id().unwrap();
    conn.create_window(
        0,
        control,
        root,
        0,
        900,
        100,
        50,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new().background_pixel(0x00ff00),
    )
    .unwrap();
    conn.map_window(control).unwrap();
    conn.flush().unwrap();
    wait_for(
        "WM focuses an ordinary control window",
        Duration::from_secs(10),
        || {
            (prop_u32(&conn, root, "_NET_ACTIVE_WINDOW", AtomEnum::WINDOW) == Some(control))
                .then_some(())
        },
    );
    conn.destroy_window(control).unwrap();

    // still pixels (first paint of the page)
    verify_pixels(
        &conn,
        window,
        &STILL,
        &Evidence::new("phase1"),
        "wm-initial",
    );
    // Xvfb input focus before the explicit focus below is still "not us"
    assert_ne!(
        focus_report(&conn, root).0,
        window,
        "island took focus while painting"
    );

    // explicit focus (what a user pin would do) so the debug keys reach the page
    conn.set_input_focus(
        x11rb::protocol::xproto::InputFocus::POINTER_ROOT,
        window,
        CURRENT_TIME,
    )
    .unwrap();
    conn.flush().unwrap();

    // cycle: still -> hover -> large -> still (the last step exercises shrink / stale-frame repaint)
    for spec in [&HOVER, &LARGE, &STILL, &LARGE, &HOVER, &STILL] {
        goto_and_verify(
            &conn,
            root,
            window,
            spec,
            true,
            &Evidence::new("phase1"),
            "wm",
        );
    }
    // final state really is empty again, via ShapeInput on the live window
    assert!(input_region(&conn, window).is_empty());
    assert!(sb.app_exited().is_none(), "app died during the cycle");
}

#[test]
fn preview_cycle_non_native_pill() {
    let _g = lock();
    let mut sb = Sandbox::new("preview");
    sb.start_window_manager(false);
    let pid = sb.start_app(false);
    let conn = sb.connect();
    let root = conn.setup().roots[0].root;
    let window = wait_for("island window mapped", Duration::from_secs(60), || {
        assert!(
            sb.app_exited().is_none(),
            "app exited early: {:?}",
            sb.app_exited()
        );
        find_island_window(&conn, root, pid)
    });
    assert_identity(&conn, window, pid, false);
    assert_not_focused_on_show(&conn, root, window);
    // A compositor can only blend per-pixel alpha if the window really has a 32-bit ARGB visual.
    assert_eq!(
        conn.get_geometry(window).unwrap().reply().unwrap().depth,
        32,
        "island window must be 32-bit ARGB"
    );

    // Non-native: the pre-show region is empty, then the first geometry report opens the still pill.
    let want = expected_region(&STILL, false);
    wait_for("preview still pill region", Duration::from_secs(20), || {
        (normalize_rows(&input_region(&conn, window)) == want).then_some(())
    });
    assert_rounded_region(&want, &STILL);
    verify_pixels(
        &conn,
        window,
        &STILL,
        &Evidence::new("phase1"),
        "preview-initial",
    );
    assert_ne!(
        focus_report(&conn, root).0,
        window,
        "island took focus while painting"
    );

    conn.set_input_focus(
        x11rb::protocol::xproto::InputFocus::POINTER_ROOT,
        window,
        CURRENT_TIME,
    )
    .unwrap();
    conn.flush().unwrap();
    for spec in [&HOVER, &LARGE, &STILL, &HOVER, &STILL] {
        let shot = goto_and_verify(
            &conn,
            root,
            window,
            spec,
            false,
            &Evidence::new("phase1"),
            "preview",
        );
        // No compositor exists on this machine/Xvfb, so this is a SIMULATED blend of the exact ARGB
        // window contents over magenta, for human inspection only (the assertions use raw alpha).
        shot.blend_over([0xff, 0x00, 0xff])
            .save_png(&sb.evidence(&format!("preview-{}-SIMULATED-composite.png", spec.name)));
    }
}

#[test]
fn click_through_is_physical_without_window_manager() {
    let _g = lock();
    let mut sb = Sandbox::new("xt");
    let conn = sb.connect();
    let root = conn.setup().roots[0].root;

    // Underlay: a full-screen override-redirect window mapped BEFORE the island (so it is below).
    let underlay = conn.generate_id().unwrap();
    conn.create_window(
        0,
        underlay,
        root,
        0,
        0,
        SCREEN_W,
        SCREEN_H,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new()
            .override_redirect(1)
            .background_pixel(0x00ff00ff)
            .event_mask(EventMask::BUTTON_PRESS),
    )
    .unwrap();
    conn.map_window(underlay).unwrap();
    conn.flush().unwrap();

    let pid = sb.start_app(true);
    let window = wait_for("island window mapped", Duration::from_secs(60), || {
        assert!(
            sb.app_exited().is_none(),
            "app exited early: {:?}",
            sb.app_exited()
        );
        find_island_window(&conn, root, pid)
    });
    let origin = conn
        .translate_coordinates(window, root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    let (ox, oy) = (origin.dst_x, origin.dst_y);

    // drain anything pending, then report whether the underlay received a ButtonPress for a click.
    let underlay_gets = |x: i16, y: i16| -> bool {
        while conn.poll_for_event().unwrap().is_some() {}
        click(&conn, root, x, y);
        conn.get_input_focus().unwrap().reply().unwrap(); // round trip: server has processed the click
        sleep(Duration::from_millis(150));
        conn.get_input_focus().unwrap().reply().unwrap();
        let mut got = false;
        while let Some(event) = conn.poll_for_event().unwrap() {
            if matches!(event, Event::ButtonPress(ref e) if e.event == underlay) {
                got = true;
            }
        }
        got
    };
    let at = |wx: i32, wy: i32| (ox + wx as i16, oy + wy as i16);

    // still + native pill: EMPTY region => even the window centre clicks through to the underlay
    wait_for("empty region", Duration::from_secs(20), || {
        input_region(&conn, window).is_empty().then_some(())
    });
    let (cx, cy) = at(i32::from(WIN_W) / 2, 25);
    assert!(
        underlay_gets(cx, cy),
        "still/native: click inside the (invisible) pill area must pass through"
    );

    // focus the island explicitly (no WM here) and open hover
    conn.set_input_focus(
        x11rb::protocol::xproto::InputFocus::POINTER_ROOT,
        window,
        CURRENT_TIME,
    )
    .unwrap();
    conn.flush().unwrap();
    press(&conn, root, HOVER.key);
    let want = expected_region(&HOVER, true);
    wait_for("hover region", Duration::from_secs(20), || {
        (normalize_rows(&input_region(&conn, window)) == want).then_some(())
    });

    let (x, y, w, h) = (
        HOVER.rect.x as i32,
        HOVER.rect.y as i32,
        HOVER.rect.width as i32,
        HOVER.rect.height as i32,
    );
    let (ix, iy) = at(x + w / 2, y + h / 2);
    assert!(
        !underlay_gets(ix, iy),
        "hover: click inside the card must be consumed by the island"
    );
    let (px, py) = at(x + 1, y + 1); // inside the bounding box but in the rounded-off corner
    assert!(
        underlay_gets(px, py),
        "hover: click in the cut-off rounded corner must pass through"
    );
    let (ox2, oy2) = at(x + w + 20, y + h / 2);
    assert!(
        underlay_gets(ox2, oy2),
        "hover: click beside the card (inside the 1180x820 window) must pass through"
    );
    let (bx, by) = at(x + w / 2, y + h + 40);
    assert!(
        underlay_gets(bx, by),
        "hover: click below the card must pass through"
    );

    // and back to still: click-through again at the former card centre
    press(&conn, root, STILL.key);
    wait_for("empty region again", Duration::from_secs(20), || {
        input_region(&conn, window).is_empty().then_some(())
    });
    assert!(
        underlay_gets(ix, iy),
        "after collapse the former card area must pass through again"
    );
}
