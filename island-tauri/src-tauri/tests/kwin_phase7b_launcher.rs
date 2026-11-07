//! Phase7b real launcher input and isolated workspace persistence. No live services.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "helpers/phase7b_acceptance.rs"]
mod acceptance;
mod common;
#[path = "helpers/gui_stack.rs"]
#[allow(dead_code)]
mod gui_stack;
#[path = "helpers/phase3_host.rs"]
mod host;
#[path = "helpers/pipewire_fixture.rs"]
mod pipewire_fixture;
#[path = "helpers/private_bus.rs"]
mod private_bus;
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
    launch_with(s, None, None)
}
fn launch_with(s: &Stack, bus: Option<String>, pipewire: Option<std::path::PathBuf>) -> Owned {
    let log = fs::File::create(s.root.join("app-7b.log")).unwrap();
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
                "HERMES_ISLAND_MPRIS_BUS",
                bus.unwrap_or_else(|| format!("unix:path={}/bus", s.root.display())),
            )
            .env(
                "PIPEWIRE_RUNTIME_DIR",
                pipewire.unwrap_or_else(|| s.root.join("no-pipewire")),
            )
            .env("HERMES_7B_CAPTURE_LOG", s.root.join("capture-starts.log"))
            .env(
                "HERMES_ISLAND_CONTROLS_SESSION_BUS",
                format!("unix:path={}/bus", s.root.display()),
            )
            .env(
                "HERMES_ISLAND_CONTROLS_SYSTEM_BUS",
                format!("unix:path={}/bus", s.root.display()),
            )
            .env("HERMES_ISLAND_DOWNLOAD_DIR", s.root.join("downloads"))
            .env("HERMES_ISLAND_AGENT_DB", s.root.join("absent-state.db"))
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
fn surfaces(s: &Stack) -> Vec<RoundedRect> {
    serde_json::from_value(eval(s,"[document.querySelector('.island'),...document.querySelectorAll('[data-side-button]')].filter(e=>e.getClientRects().length&&getComputedStyle(e).visibility!=='hidden').map(e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,radius:parseFloat(getComputedStyle(e).borderRadius)||0}})")).unwrap()
}
fn settled(s: &Stack, c: &RustConnection, w: u32, mode: IslandMode) {
    let mut previous = None;
    let mut since = Instant::now();
    wait_for(
        "7b exact native surface/button union stable",
        Duration::from_secs(12),
        || {
            let r = surfaces(s);
            let actual = normalize_rows(&input_region(c, w));
            let expected = region_rows(&r, mode, 1., true).unwrap();
            let pill: Option<RoundedRect> = serde_json::from_value(eval(s,"(()=>{const s=getComputedStyle(document.documentElement),n=k=>parseFloat(s.getPropertyValue('--native-pill-'+k));return n('width')>0?{x:n('x'),y:n('y'),width:n('width'),height:n('height'),radius:0}:null})()")).unwrap();
            let expected = pill.map_or_else(
                || expected.clone(),
                |pill| {
                    hermes_island_lib::window::shape::subtract_rect(&expected, pill, 1.).unwrap()
                },
            );
            let now = (r, actual.clone());
            if previous.as_ref() != Some(&now) {
                previous = Some(now);
                since = Instant::now();
                return None;
            }
            (actual == expected && since.elapsed() >= Duration::from_millis(250)).then_some(())
        },
    );
}
fn point(s: &Stack, origin: (i32, i32), selector: &str) -> (i32, i32) {
    let p=eval(s,&format!("(()=>{{const q={0};const e=document.querySelector(q);if(!e)throw Error('Missing real input target '+q+'; present: '+[...document.querySelectorAll('[data-testid]')].map(e=>e.dataset.testid).join(' '));e.scrollIntoView({{block:'nearest'}});const r=e.getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]}})()",json!(selector)));
    (
        origin.0 + p[0].as_f64().unwrap() as i32,
        origin.1 + p[1].as_f64().unwrap() as i32,
    )
}
fn click(s: &Stack, e: &mut Wheel, origin: (i32, i32), selector: &str) {
    let p = point(s, origin, selector);
    e.click_at(p.0, p.1);
}
/// Pick a colour scheme through the real picker. Schemes with variants live inside a
/// collapsible family row (SchemePicker.tsx): open the family first when its radio is hidden.
fn pick_scheme(s: &Stack, e: &mut Wheel, o: (i32, i32), id: &str, family: &str) {
    let radio = format!("[data-testid=appearance-{id}]");
    if eval(s, &format!("!document.querySelector({})", json!(radio))) == true {
        click(s, e, o, &format!("[data-testid=scheme-family-{family}]"));
        until(
            s,
            &format!("7b scheme family {family} expanded"),
            8,
            &format!("document.querySelector('[data-testid=scheme-family-{family}]')?.getAttribute('aria-expanded')==='true' && !!document.querySelector({})", json!(radio)),
        );
    }
    click(s, e, o, &radio);
}
/// Select the stable section ID on the Settings rail (ui/views/SettingsView.tsx).
fn settings_section(s: &Stack, e: &mut Wheel, o: (i32, i32), section: &str) {
    click(s, e, o, &format!(".settings-rail [data-section={section}]"));
    until(
        s,
        &format!("7b settings section {section} selected"),
        8,
        &format!(
            "document.querySelector('.settings-rail [aria-current=page]')?.dataset.section==={}",
            json!(section)
        ),
    );
}
fn open(s: &Stack, c: &RustConnection, w: u32, e: &mut Wheel, o: (i32, i32), view: &str) {
    settled(s, c, w, IslandMode::Large);
    // On the launcher page the Apps side button is Back (geometry.ts sideBackFor), so only press
    // it when another page is showing.
    if eval(s, "document.querySelector('.island').dataset.view") != "launcher" {
        click(s, e, o, "[data-testid=side-apps]");
    }
    until(s,&format!("7b real side button opens launcher (before {view})"),10,"document.querySelector('.island').dataset.view==='launcher'&&!!document.querySelector('.app-grid')");
    settled(s, c, w, IslandMode::Large);
    click(s, e, o, &format!("[data-testid=view-{view}]"));
    until(
        s,
        &format!("7b real tile opens {view}"),
        10,
        &format!(
            "document.querySelector('.island').dataset.view==={}",
            json!(view)
        ),
    );
    assert_eq!(actor_state(s)["view"], view);
    settled(s, c, w, IslandMode::Large);
    assert_eq!(eval(s,"document.querySelector('.app-pane:not([hidden])').innerText.includes('Malformed app snapshot')"),false);
}
fn capture(s: &Stack, c: &RustConnection, w: u32, name: &str) {
    let overlay = host::overlay_size();
    let mut last = None;
    wait_for(
        "7b content and pixels settle",
        Duration::from_secs(8),
        || {
            let pair = (
                eval(
                    s,
                    "document.querySelector('.app-pane:not([hidden])').innerText",
                ),
                if name.ends_with("-edit") {
                    serde_json::to_vec(
                        &surfaces(s)
                            .iter()
                            .map(|r| [r.x, r.y, r.width, r.height])
                            .collect::<Vec<_>>(),
                    )
                    .unwrap()
                } else {
                    Shot::grab(c, w, overlay.0, overlay.1).bgra
                },
            );
            let same = last.as_ref() == Some(&pair);
            last = Some(pair);
            if same {
                Some(())
            } else {
                sleep(Duration::from_millis(250));
                None
            }
        },
    );
    let evidence = Evidence::new("phase7b");
    Shot::grab(c, w, overlay.0, overlay.1).save_png(&evidence.path(&format!("{name}.png")));
    fs::write(evidence.path(&format!("{name}.json")),serde_json::to_vec(&eval(s,"[document.querySelector('.island'),...document.querySelectorAll('[data-side-button]')].map(e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height}})")).unwrap()).unwrap();
}
#[test]
fn launcher_apps_layout_and_draft_use_real_input_in_private_instance() {
    let _guard = lock();
    let mut s = Stack::new("phase7b-launcher");
    prepare(&s);
    fs::create_dir(s.root.join("downloads")).unwrap();
    let mut app = launch(&s);
    let c = s.connect();
    let root = c.setup().roots[0].root;
    let w = wait_for("7b window", Duration::from_secs(30), || {
        find_window_sized(&c, root, app.0.id(), host::overlay_size())
    });
    wait_for("7b control socket", Duration::from_secs(20), || {
        s.root.join("inst/test-control.sock").exists().then_some(())
    });
    until(
        &s,
        "7b seeded page",
        20,
        "!!document.querySelector('.island')",
    );
    // Registry-only fixture setup. Flat privacy/network/audio settings remain OFF.
    eval(&s,"window.__TAURI_INTERNALS__.invoke('settings_get').then(s=>{const r=s.moduleRegistry;for(const m of r.modules)if(m.config&&typeof m.config.type==='string')m.enabled=true;return window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{moduleRegistry:r}})})");
    let pos = c
        .translate_coordinates(w, root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    let o = (i32::from(pos.dst_x), i32::from(pos.dst_y));
    let mut e = Wheel::new(&s);
    client(&s, "pin");
    until(
        &s,
        "7b large",
        10,
        "document.querySelector('.island').dataset.state==='large'",
    );
    until(
        &s,
        "7b fixture prompt",
        20,
        "window.__islandTerminalTest?.text().includes('island-test>')===true",
    );
    settled(&s, &c, w, IslandMode::Large);
    click(&s, &mut e, o, ".terminal");
    e.send("type UNSUBMITTED_7B_DRAFT");
    until(
        &s,
        "7b draft",
        8,
        "window.__islandTerminalTest.text().includes('UNSUBMITTED_7B_DRAFT')",
    );
    eval(&s,"(window.__savedTerminal=window.__islandTerminalTest.identity,window.__savedTerminal.options.cursorBlink=false,window.__savedTerminal.blur(),true)");
    let pty = status(&s)["pty"].clone();
    for theme in ["skin", "neutral"] {
        open(&s, &c, w, &mut e, o, "settings");
        settings_section(&s, &mut e, o, "look");
        pick_scheme(&s, &mut e, o, theme, "monolith");
        until(
            &s,
            "7b real theme selection",
            8,
            &format!("document.documentElement.dataset.theme==={}", json!(theme)),
        );
        for view in [
            "terminal",
            "monitor",
            "ai",
            "settings",
            "notifications",
            "clipboard",
            "music",
            "timer",
            "calendar",
            "scratchpad",
            "downloads",
            // No "controls": that app was retired and its tile is never drawn (Launcher.tsx).
            "mixer",
            "brightness",
            "agents",
            "updates",
            "quickInfo",
        ] {
            open(&s, &c, w, &mut e, o, view);
            if view == "ai" {
                eval(
                    &s,
                    "document.querySelector('.ai-stats-tabs [role=tab]:nth-child(2)').click()",
                );
                until(
                    &s,
                    "7b AI loaded",
                    35,
                    "document.querySelector('.ai-stats').getAttribute('aria-busy')==='false'",
                );
            }
            if view == "monitor" {
                until(
                    &s,
                    "7b monitor real interval",
                    8,
                    "!!document.querySelector('progress[aria-label=\"CPU usage\"]')",
                );
            }
            capture(&s, &c, w, &format!("{theme}-{view}"));
        }
        click(&s, &mut e, o, "[data-testid=side-apps]");
        until(
            &s,
            "7b launcher capture",
            8,
            "document.querySelector('.island').dataset.view==='launcher'",
        );
        settled(&s, &c, w, IslandMode::Large);
        capture(&s, &c, w, &format!("{theme}-launcher"));
        acceptance::begin_edit(&s, &mut e, o);
        capture(&s, &c, w, &format!("{theme}-edit"));
        click(&s, &mut e, o, ".launcher-toolbar button");
        until(
            &s,
            "7b leave edit after capture",
            8,
            "document.querySelector('.app-grid')?.dataset.editing==='false'",
        );
        open(&s, &c, w, &mut e, o, "settings");
        settings_section(&s, &mut e, o, "island");
        point(&s, o, ".layout-editor");
        settled(&s, &c, w, IslandMode::Large);
        capture(&s, &c, w, &format!("{theme}-layout"));
    }
    open(&s, &c, w, &mut e, o, "terminal");
    assert_eq!(status(&s)["pty"], pty);
    assert_eq!(eval(&s,"window.__savedTerminal===window.__islandTerminalTest.identity&&window.__islandTerminalTest.text().includes('UNSUBMITTED_7B_DRAFT')"),true);
    click(&s, &mut e, o, ".terminal");
    e.send("chord 29 22");
    open(&s, &c, w, &mut e, o, "settings");
    settings_section(&s, &mut e, o, "island");
    // First size tile = Compact (360 wide), LayoutEditor.tsx TILES. The editor lays itself out
    // from an async geometry reply, so let the surface and the tile rect settle before pressing.
    settled(&s, &c, w, IslandMode::Large);
    let mut last = None;
    wait_for(
        "7b size tile position stable",
        Duration::from_secs(8),
        || {
            let now = point(&s, o, ".le-tiles button:first-child");
            let stable = last == Some(now);
            last = Some(now);
            std::thread::sleep(Duration::from_millis(300));
            stable.then_some(())
        },
    );
    click(&s, &mut e, o, ".le-tiles button:first-child");
    until(
        &s,
        "7b preset width changes without restart",
        10,
        "Math.abs(document.querySelector('.island').getBoundingClientRect().width-360)<0.1",
    );
    settled(&s, &c, w, IslandMode::Large);
    let layout: Value =
        serde_json::from_slice(&fs::read(s.root.join("inst/layout.json")).unwrap()).unwrap();
    assert_eq!(layout["preset"], "compact");
    assert_eq!(unsafe { libc::kill(app.0.id() as i32, libc::SIGTERM) }, 0);
    wait_for("7b owned app exits", Duration::from_secs(15), || {
        app.0.try_wait().unwrap().is_some().then_some(())
    });
    drop(e);
    drop(app);
    s.shutdown();
    let path = s.root.clone();
    drop(s);
    assert!(!path.exists());
}
