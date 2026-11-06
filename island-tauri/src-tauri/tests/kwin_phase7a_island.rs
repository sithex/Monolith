//! Phase 7a real EIS input, retained terminal and settled pixels on a private nested KWin.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "helpers/gui_stack.rs"]
#[allow(dead_code)]
mod gui_stack;
#[path = "helpers/phase3_host.rs"]
mod host;
use common::*;
use gui_stack::Stack;
use hermes_island_lib::{
    contract::island::{IslandMode, RoundedRect},
    window::shape::{normalize_rows, region_rows},
};
use host::{actor_state, client, eval, prepare, status, until, Owned, Wheel};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::process::CommandExt,
    process::Command,
    thread::sleep,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection, protocol::xproto::ConnectionExt as _, rust_connection::RustConnection,
};

fn launch(s: &Stack) -> Owned {
    let log = fs::File::create(s.root.join("app.log")).unwrap();
    Owned(
        Command::new(env!("CARGO_BIN_EXE_hermes-island"))
            .args([
                "--test-instance",
                "--overlay",
                "--fixture-pty",
                "--ui",
                "island",
                "--data-dir",
            ])
            .arg(s.root.join("inst"))
            .env_clear()
            .envs(s.env())
            .env("DISPLAY", &s.display)
            .env("GDK_BACKEND", "x11")
            .env("HERMES_ISLAND_TEST", "1")
            .env("NO_AT_BRIDGE", "1")
            .env("GTK_USE_PORTAL", "0")
            .env("GIO_USE_PORTAL", "0")
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", s.root.join("bin").display()),
            )
            .process_group(0)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    )
}
fn rect(s: &Stack) -> RoundedRect {
    serde_json::from_value(eval(s, "(()=>{const e=document.querySelector('.island'),r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,radius:parseFloat(getComputedStyle(e).borderRadius)}})()" )).unwrap()
}
/// Exact outer-surface geometry check that reports the measured rect on failure.
/// Under the native notch (the fixture panel is native) the outer corner radius is the
/// product's `surfaceRadius(height)` (ui/island/motion-profile.ts), not a per-mode constant.
fn surface_radius(height: f64) -> f64 {
    (height.max(0.) * 0.34).min(28.)
}
fn assert_close(s: &Stack, field: &str, expected: f64) {
    let r = rect(s);
    let actual = match field {
        "width" => r.width,
        "radius" => r.radius,
        _ => unreachable!(),
    };
    assert!(
        (actual - expected).abs() < 0.1,
        "outer {field}: expected {expected}, measured {r:?}"
    );
}
fn settled(s: &Stack, conn: &RustConnection, window: u32, mode: IslandMode) {
    // Phase7b diagnostic begin
    eval(s,"(()=>{if(!window.__crossings){window.__crossings=[];for(const type of ['mouseover','mouseout'])addEventListener(type,e=>{const outer=document.querySelector('.island');window.__crossings.push({type,x:e.clientX,y:e.clientY,mode:outer?.dataset.state,target:e.target?.className,related:e.relatedTarget?.className});if(window.__crossings.length>60)window.__crossings.shift();});}return true})()");
    struct Diagnostic<'a> {
        s: &'a Stack,
        last: std::cell::RefCell<String>,
    }
    impl Drop for Diagnostic<'_> {
        fn drop(&mut self) {
            if std::thread::panicking() {
                eprintln!("7a settle last compared geometry: {}", self.last.borrow());
                eprintln!("7a settle timeout actor: {}", actor_state(self.s));
                eprintln!(
                    "7a DOM crossing trace: {}",
                    eval(self.s, "window.__crossings")
                );
                eprintln!("7a settle timeout DOM: {}", eval(self.s,"({mode:document.querySelector('.island')?.dataset.state,view:document.querySelector('.island')?.dataset.view,errors:window.__phase7bErrors,alerts:[...document.querySelectorAll('[role=alert]')].map(e=>e.textContent)})"));
            }
        }
    }
    let diagnostic = Diagnostic {
        s,
        last: std::cell::RefCell::new(String::new()),
    };
    // Phase7b diagnostic end
    let mut previous = None;
    let mut since = Instant::now();
    wait_for(
        "7a current mode outer DOM and ShapeInput settled",
        Duration::from_secs(12),
        || {
            // Controller-approved Phase 7b amendment: exact painted interactive union,
            // never a bounding box (the gaps between floating controls remain click-through).
            let current: Vec<RoundedRect> = serde_json::from_value(eval(s, "(()=>{const outer=document.querySelector('.island');return [outer,...document.querySelectorAll('[data-side-button]')].filter(e=>e.getClientRects().length&&getComputedStyle(e).visibility!=='hidden').map(e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,radius:parseFloat(getComputedStyle(e).borderRadius)||0}})})()" )).unwrap();
            let actual = normalize_rows(&input_region(conn, window));
            let expected = region_rows(&current, mode, 1., true).unwrap();
            // Controller-approved pill exclusion begin: QML owns its entire rectangular MouseArea.
            let pill: Option<RoundedRect> = serde_json::from_value(eval(s,"(()=>{const s=getComputedStyle(document.documentElement),n=k=>parseFloat(s.getPropertyValue('--native-pill-'+k));return n('width')>0?{x:n('x'),y:n('y'),width:n('width'),height:n('height'),radius:0}:null})()")).unwrap();
            let expected = pill.map_or_else(
                || expected.clone(),
                |pill| {
                    hermes_island_lib::window::shape::subtract_rect(&expected, pill, 1.).unwrap()
                },
            );
            // Controller-approved pill exclusion end.
            // Phase7b diagnostic begin
            *diagnostic.last.borrow_mut() = format!("requested={mode:?}; outer+side={current:?}; pill={pill:?}; expected={expected:?}; native={actual:?}");
            // Phase7b diagnostic end
            let pair = (current, actual.clone());
            if previous.as_ref() != Some(&pair) {
                previous = Some(pair);
                since = Instant::now();
                return None;
            }
            (actual == expected && since.elapsed() >= Duration::from_millis(250)).then_some(())
        },
    );
    if mode == IslandMode::Still {
        assert!(input_region(conn, window).is_empty());
    }
}
fn mode(s: &Stack, conn: &RustConnection, window: u32, name: &str, value: IslandMode) {
    until(
        s,
        &format!("7a canonical DOM and actor {name}"),
        12,
        &format!(
            "document.querySelector('.island')?.dataset.state==={}",
            json!(name)
        ),
    );
    assert_eq!(actor_state(s)["mode"], name);
    settled(s, conn, window, value);
}
fn click(s: &Stack, eis: &mut Wheel, origin: (i32, i32), selector: &str) {
    let point = eval(s, &format!("(()=>{{const e=document.querySelector({});if(!e)throw Error('missing click target; present: '+[...document.querySelectorAll('[data-testid]')].map(e=>e.dataset.testid).join(' '));e.scrollIntoView({{block:'nearest'}});const r=e.getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]}})()", json!(selector)));
    eis.click_at(
        origin.0 + point[0].as_f64().unwrap() as i32,
        origin.1 + point[1].as_f64().unwrap() as i32,
    );
}
fn settings_tab(s: &Stack, eis: &mut Wheel, origin: (i32, i32), name: &str) {
    let index = ["General", "Appearance", "Layout"]
        .iter()
        .position(|t| *t == name)
        .unwrap()
        + 1;
    click(
        s,
        eis,
        origin,
        &format!(".settings-tabs [role=tab]:nth-child({index})"),
    );
    until(
        s,
        &format!("7a settings tab {name} selected"),
        8,
        &format!(
            "document.querySelector('.settings-tabs [aria-selected=true]')?.textContent==={}",
            json!(name)
        ),
    );
}
/// Pick a colour scheme through the real picker. Schemes with variants live inside a
/// collapsible family row (SchemePicker.tsx): open the family first when its radio is hidden.
fn pick_scheme(s: &Stack, eis: &mut Wheel, origin: (i32, i32), id: &str, family: &str) {
    let radio = format!("[data-testid=appearance-{id}]");
    if eval(s, &format!("!document.querySelector({})", json!(radio))) == true {
        click(
            s,
            eis,
            origin,
            &format!("[data-testid=scheme-family-{family}]"),
        );
        until(
            s,
            &format!("7a scheme family {family} expanded"),
            8,
            &format!("document.querySelector('[data-testid=scheme-family-{family}]')?.getAttribute('aria-expanded')==='true' && !!document.querySelector({})", json!(radio)),
        );
    }
    click(s, eis, origin, &radio);
}
fn view(
    s: &Stack,
    conn: &RustConnection,
    window: u32,
    eis: &mut Wheel,
    origin: (i32, i32),
    id: &str,
) {
    settled(s, conn, window, IslandMode::Large);
    // Same production launcher and physical EIS path as the user. No hidden tabs.
    click(s, eis, origin, "[data-testid=side-apps]");
    until(s, "7a real side-button click opened launcher", 10,
        "document.querySelector('.island').dataset.view==='launcher' && !!document.querySelector('.app-grid')");
    assert_eq!(actor_state(s)["view"], "launcher");
    settled(s, conn, window, IslandMode::Large);
    click(s, eis, origin, &format!("[data-testid=view-{id}]"));
    until(s,&format!("7a real click selected {id}"),10,&format!("document.querySelector('.island').dataset.view==={} && getComputedStyle(document.querySelector('.app-pane:not([hidden])')).opacity==='1'",json!(id)));
    assert_eq!(actor_state(s)["view"], id);
    settled(s, conn, window, IslandMode::Large);
}
fn screenshot(s: &Stack, conn: &RustConnection, window: u32, name: &str) {
    let overlay = host::overlay_size();
    assert_eq!(eval(s, "(()=>{const p=document.querySelector('.app-pane:not([hidden])');return p.scrollWidth<=p.clientWidth+1})()"), true, "view overflows horizontally");
    let mut previous = None;
    wait_for(
        "7a view text and pixels identical on consecutive captures",
        Duration::from_secs(8),
        || {
            let pair = (
                eval(
                    s,
                    "document.querySelector('.app-pane:not([hidden])').innerText",
                ),
                Shot::grab(conn, window, overlay.0, overlay.1).bgra,
            );
            let stable = previous.as_ref() == Some(&pair);
            previous = Some(pair);
            if stable {
                Some(())
            } else {
                sleep(Duration::from_millis(250));
                None
            }
        },
    );
    let evidence = Evidence::new("phase7a");
    Shot::grab(conn, window, overlay.0, overlay.1).save_png(&evidence.path(&format!("{name}.png")));
    let r = rect(s);
    fs::write(
        evidence.path(&format!("{name}.json")),
        json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height,"radius":r.radius}).to_string(),
    )
    .unwrap();
}

#[test]
fn island_page_real_input_geometry_retained_draft_appearance_and_screenshots() {
    let _guard = lock();
    let mut s = Stack::new("phase7a");
    prepare(&s);
    let mut app = launch(&s);
    let conn = s.connect();
    let root = conn.setup().roots[0].root;
    let pid = app.0.id();
    let window = wait_for("7a island window", Duration::from_secs(30), || {
        find_window_sized(&conn, root, pid, host::overlay_size())
    });
    assert_identity(&conn, window, pid, true);
    assert_not_focused_on_show(&conn, root, window);
    wait_for("7a test socket", Duration::from_secs(20), || {
        s.root.join("inst/test-control.sock").exists().then_some(())
    });
    until(
        &s,
        "7a bootstrap before first mounted surface",
        20,
        "!!document.querySelector('.island')",
    );
    assert_eq!(
        eval(&s, "typeof window.island"),
        "undefined",
        "new page must not install legacy shim"
    );
    let pos = conn
        .translate_coordinates(window, root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    let origin = (i32::from(pos.dst_x), i32::from(pos.dst_y));
    mode(&s, &conn, window, "still", IslandMode::Still);
    // Still = the native pill (fixture widget 150x36); hover/terminal sizes per IslandSurface.tsx.
    assert_close(&s, "width", 150.);
    assert_close(&s, "radius", surface_radius(36.));
    let mut eis = Wheel::new(&s);
    eis.move_to(100, 900);
    eval(&s,"(()=>{window.__frames=[];let last=null;const end=performance.now()+3500;function sample(now){if(last!==null)window.__frames.push(now-last);last=now;if(now<end&&window.__frames.length<240)requestAnimationFrame(sample)}requestAnimationFrame(sample);return true})()");
    client(&s, "enter");
    mode(&s, &conn, window, "hover", IslandMode::Hover);
    assert_close(&s, "width", 360.);
    assert_close(&s, "radius", surface_radius(116.));
    let r = rect(&s);
    eis.move_to(origin.0 + r.x as i32 + 100, origin.1 + r.y as i32 + 80);
    client(&s, "leave");
    sleep(Duration::from_millis(450));
    assert_eq!(actor_state(&s)["mode"], "hover");
    eis.move_to(100, 900);
    mode(&s, &conn, window, "still", IslandMode::Still);
    client(&s, "enter");
    mode(&s, &conn, window, "hover", IslandMode::Hover);
    click(&s, &mut eis, origin, "[data-testid=open]");
    mode(&s, &conn, window, "large", IslandMode::Large);
    until(
        &s,
        "7a fixture prompt",
        20,
        "window.__islandTerminalTest?.text().includes('island-test>')===true",
    );
    // A fresh data dir starts on the Bigger layout preset (services/workspace.rs, PRESET_SIZES).
    assert_close(&s, "width", 760.);
    assert_close(&s, "radius", surface_radius(rect(&s).height));
    until(
        &s,
        "7a bounded frame sample finished",
        5,
        "window.__frames.length>=100",
    );
    let frames: Vec<f64> = serde_json::from_value(eval(&s, "window.__frames")).unwrap();
    let mut sorted = frames.clone();
    sorted.sort_by(f64::total_cmp);
    assert!(!sorted.is_empty());
    let pacing = json!({"samples":sorted.len(),"p50":sorted[sorted.len()/2],"p95":sorted[sorted.len()*95/100],"max":sorted.last().unwrap(),"over20ms":sorted.iter().filter(|v|**v>20.).count()});
    fs::write(
        Evidence::new("phase7a").path("frame-pacing.json"),
        pacing.to_string(),
    )
    .unwrap();
    println!("7a frame pacing (recorded, not threshold): {pacing}");
    // Only unsubmitted fixture text. Save actual xterm identity, not just the OS PTY PID.
    click(&s, &mut eis, origin, ".terminal");
    eis.send("type UNSUBMITTED_7A_DRAFT");
    until(
        &s,
        "7a real EIS draft",
        8,
        "window.__islandTerminalTest.text().includes('UNSUBMITTED_7A_DRAFT')",
    );
    eval(
        &s,
        "(window.__retainedXterm=window.__islandTerminalTest.identity,true)",
    );
    let pty = status(&s)["pty"].clone();
    assert!(pty.as_u64().unwrap() > 0);
    click(&s, &mut eis, origin, "[data-testid=collapse]");
    mode(&s, &conn, window, "still", IslandMode::Still);
    // Release remaps the X11 window; it must come back where it was (WM placement moved it).
    let after = conn
        .translate_coordinates(window, root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(
        (i32::from(after.dst_x), i32::from(after.dst_y)),
        origin,
        "collapse moved the overlay window"
    );
    assert_eq!(status(&s)["pty"], pty);
    client(&s, "pin");
    mode(&s, &conn, window, "large", IslandMode::Large);
    until(&s,"7a same xterm and draft on reopen",8,"window.__retainedXterm===window.__islandTerminalTest.identity && window.__islandTerminalTest.text().includes('UNSUBMITTED_7A_DRAFT')");
    assert_eq!(status(&s)["pty"], pty);
    click(&s, &mut eis, origin, ".terminal");
    eis.send("chord 29 22");
    until(
        &s,
        "7a unsubmitted draft cleared",
        8,
        "!window.__islandTerminalTest.text().includes('UNSUBMITTED_7A_DRAFT')",
    );
    eval(&s,"(window.__islandTerminalTest.identity.options.cursorBlink=false,window.__islandTerminalTest.identity.blur(),true)");
    // Skin is the fresh-install default (services/frontend.rs), so pick neutral first: every
    // iteration is a real change that must persist without a restart.
    for theme in ["neutral", "skin"] {
        view(&s, &conn, window, &mut eis, origin, "settings");
        // Colour schemes live on the Settings → Appearance tab (SettingsView.tsx).
        settings_tab(&s, &mut eis, origin, "Appearance");
        // skin is a standalone row; neutral lives in the Monolith family.
        pick_scheme(&s, &mut eis, origin, theme, "monolith");
        until(
            &s,
            "7a appearance changed without restart",
            8,
            &format!("document.documentElement.dataset.theme==={}", json!(theme)),
        );
        assert_eq!(
            serde_json::from_slice::<Value>(
                &fs::read(s.root.join("inst/appearance.json")).unwrap()
            )
            .unwrap(),
            theme
        );
        for id in ["terminal", "monitor", "ai", "settings"] {
            view(&s, &conn, window, &mut eis, origin, id);
            let text = eval(
                &s,
                "document.querySelector('.app-pane:not([hidden])').innerText",
            );
            if id == "monitor" {
                assert!(text.as_str().unwrap().contains("Memory"));
                until(
                    &s,
                    "7a monitor second sample has valid CPU interval",
                    8,
                    "!!document.querySelector('progress[aria-label=\"CPU usage\"]')",
                );
            }
            if id == "ai" {
                until(
                    &s,
                    "7a real AI adapter completed",
                    35,
                    "document.querySelector('.ai-stats').getAttribute('aria-busy')==='false'",
                );
                // Read after the adapter settled; the private HOME has no AI accounts.
                let text = eval(
                    &s,
                    "document.querySelector('.app-pane:not([hidden])').innerText",
                );
                let text = text.as_str().unwrap();
                assert!(
                    text.contains("Not connected") && text.contains("Sign in"),
                    "AI pane: {text}"
                );
            }
            if id == "settings" {
                // innerText follows CSS text-transform (card titles are uppercase).
                let text = text.as_str().unwrap().to_lowercase();
                assert!(
                    ["general", "appearance", "layout", "colour scheme"]
                        .iter()
                        .all(|t| text.contains(t)),
                    "settings pane: {text}"
                );
            }
            screenshot(&s, &conn, window, &format!("{theme}-{id}"));
        }
    }
    let calls = eval(&s, "window.__islandMonitorTest().requests");
    sleep(Duration::from_millis(1850));
    assert_eq!(
        eval(&s, "window.__islandMonitorTest().requests"),
        calls,
        "monitor polled while hidden"
    );
    assert_eq!(eval(&s, "window.__islandMonitorTest().visible"), false);
    // Settings-only geometry changes need a final region even without a Motion spring.
    let before = rect(&s);
    eval(
        &s,
        "window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{offsetX:24,offsetY:12}})",
    );
    until(&s, "7a outer surface offset applied event-driven", 8, &format!("(()=>{{const r=document.querySelector('.island').getBoundingClientRect();return Math.abs(r.x-{})<0.1&&Math.abs(r.y-{})<0.1}})()",before.x+24.,before.y+12.));
    settled(&s, &conn, window, IslandMode::Large);
    settings_tab(&s, &mut eis, origin, "Appearance");
    // Leave a non-default choice persisted so the reload below proves it was read back.
    pick_scheme(&s, &mut eis, origin, "neutral", "monolith");
    until(
        &s,
        "7a neutral re-selected",
        8,
        "document.documentElement.dataset.theme==='neutral'",
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(s.root.join("inst/appearance.json")).unwrap())
            .unwrap(),
        "neutral"
    );
    assert_eq!(eval(&s,"document.fonts.check('14px OpenRunde') && getComputedStyle(document.querySelector('[aria-label=\"UI font\"]')).fontFamily.includes('OpenRunde')"),true);
    // Reload is a new webview bootstrap; the persisted choice must still be neutral.
    eval(&s, "(setTimeout(()=>location.reload(),250),true)");
    sleep(Duration::from_millis(700));
    until(
        &s,
        "7a persisted appearance on new bootstrap",
        20,
        "!!document.querySelector('.island') && document.documentElement.dataset.theme==='neutral'",
    );
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
    wait_for(
        "7a owned app and fixture PTY exited",
        Duration::from_secs(15),
        || {
            (app.0.try_wait().unwrap().is_some()
                && !std::path::Path::new(&format!("/proc/{}", pty.as_u64().unwrap())).exists())
            .then_some(())
        },
    );
    drop(eis);
    drop(app);
    s.shutdown();
    let scratch = s.root.clone();
    drop(s);
    assert!(!scratch.exists());
}
