//! Phase 3 actor authority on a private nested KWin (SPEC §8 Phase 3, revised after review).
//!
//! Real `anchor-client.py` panel events + real EIS pointer/keyboard/wheel drive the UNCHANGED
//! legacy shim and the AMENDED renderer; every assertion reads the Rust ACTOR's state
//! (`island_bootstrap().state`), `status.json`, the DOM and the X ShapeInput readback.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "helpers/gui_stack.rs"]
#[allow(dead_code)] // Shared Phase 2 stack also exposes controls unused by this scenario.
mod gui_stack;
#[path = "helpers/phase3_host.rs"]
mod host;
use common::*;
use gui_stack::Stack;
use hermes_island_lib::contract::island::IslandMode;
use host::*;
use serde_json::{json, Value};
use std::{
    thread::sleep,
    time::{Duration, Instant},
};
use x11rb::connection::Connection;

const ORIGIN: (i32, i32) = (210, 44); // window origin under the fixture panel (see kwin_phase3_host)

fn wait_actor(s: &Stack, what: &str, secs: u64, ok: impl Fn(&Value) -> bool) -> Value {
    wait_for(what, Duration::from_secs(secs), || {
        let state = actor_state(s);
        ok(&state).then_some(state)
    })
}
/// Centre of a DOM element in screen coordinates (window origin + client rect).
fn centre(s: &Stack, selector: &str) -> (i32, i32) {
    let expr = format!("(()=>{{const e=document.querySelector({});if(!e)throw Error('missing '+{});const r=e.getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]}})()", json!(selector), json!(selector));
    let pos = eval(s, &expr);
    (
        ORIGIN.0 + pos[0].as_f64().unwrap() as i32,
        ORIGIN.1 + pos[1].as_f64().unwrap() as i32,
    )
}
fn sent_intents(s: &Stack) -> Vec<String> {
    eval(s, "window.__sentIntents.slice()")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}
fn wait_status_view(s: &Stack, view: &str, rev: u64) {
    wait_for(
        &format!("status.json view={view} rev={rev}"),
        Duration::from_secs(8),
        || {
            let file = status(s);
            (file["view"] == view && file["stateRev"] == rev).then_some(())
        },
    );
}

#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn actor_owns_hover_tabs_focus_cues_and_geometry() {
    let _guard = lock();
    let mut s = Stack::new("phase3-actor");
    prepare(&s);
    let mut app = launch(&s);
    let conn = s.connect();
    let root = conn.setup().roots[0].root;
    let pid = app.0.id();
    let window = wait_for("Phase3 actor window", Duration::from_secs(30), || {
        find_island_window(&conn, root, pid)
    });
    assert_identity(&conn, window, pid, true);
    assert_not_focused_on_show(&conn, root, window);
    wait_for("test socket + status", Duration::from_secs(20), || {
        (s.root.join("inst/test-control.sock").exists() && s.root.join("inst/status.json").exists())
            .then_some(())
    });
    wait_for("legacy shim ready", Duration::from_secs(20), || {
        (eval(
            &s,
            "!!window.island && !!document.querySelector('[data-state]')",
        ) == true)
            .then_some(())
    });
    assert!(input_region(&conn, window).is_empty());
    sleep(Duration::from_millis(1600));
    // Record every intent the renderer sends to the actor (and any click cue it receives).
    eval(&s,"(()=>{const f=window.fetch.bind(window);window.__sentIntents=[];window.fetch=(u,o)=>{try{if(String(u).includes('island_dispatch')){const b=o&&o.body;const t=typeof b==='string'?b:new TextDecoder().decode(b);window.__sentIntents.push(JSON.parse(t).intent.type)}}catch(e){window.__sentIntents.push('ERR:'+e)}return f(u,o)};return true})()");
    eval(&s,"(async()=>{window.__cues=[];const h=window.__TAURI_INTERNALS__.transformCallback(e=>window.__cues.push(e.payload));await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'island-cue',target:{kind:'Any'},handler:h});return true})()");
    let boot = actor_state(&s);
    assert_eq!(
        (
            boot["mode"].as_str(),
            boot["view"].as_str(),
            boot["terminalStarted"].as_bool()
        ),
        (Some("still"), Some("terminal"), Some(false)),
        "{boot}"
    );
    eval(&s,"(()=>{window.__mouse=[];for(const t of [\"mouseover\",\"mouseout\",\"mouseenter\",\"mouseleave\"])document.addEventListener(t,e=>window.__mouse.push(t+\"@\"+e.clientX+\",\"+e.clientY+\"/\"+(e.target.className||e.target.tagName)),true);return true})()");
    let mut eis = Wheel::new(&s);

    // ---- 1. the ORIGINAL hover-failure sequence ------------------------------------------
    // panel enter -> preview; then a DOM mouseleave the pointer never "entered" (the legacy
    // renderer's 300 ms leave timer used to collapse the preview). Rust must keep it open.
    eis.move_to(800, 22);
    client(&s, "enter");
    wait_mode(&s, &conn, window, "preview", IslandMode::Hover);
    let hovered = actor_state(&s);
    assert_eq!(hovered["mode"], "hover");
    // The popup appearing under the pointer churns real DOM enter/leave events; whichever
    // source the actor attributes the hover to, the preview must stay open.
    assert!(
        matches!(
            hovered["hoverCause"]["type"].as_str(),
            Some("panelPill" | "pointer")
        ),
        "{hovered}"
    );
    let leaves_before = sent_intents(&s)
        .iter()
        .filter(|i| *i == "pointerLeave")
        .count();
    eval(&s,"(document.querySelector('.island').dispatchEvent(new MouseEvent('mouseout',{bubbles:true,relatedTarget:null})),true)");
    wait_for(
        "renderer sent the spurious leave",
        Duration::from_secs(5),
        || {
            (sent_intents(&s)
                .iter()
                .filter(|i| *i == "pointerLeave")
                .count()
                > leaves_before)
                .then_some(())
        },
    );
    let held = Instant::now();
    while held.elapsed() < Duration::from_millis(1500) {
        assert_eq!(
            dom_state(&s),
            "preview",
            "spurious leave collapsed the preview"
        );
        assert_eq!(actor_state(&s)["mode"], "hover");
        assert_eq!(status(&s)["mode"], "preview");
        assert!(!input_region(&conn, window).is_empty());
        sleep(Duration::from_millis(100));
    }
    // a REAL leave: 300 ms grace, then idle with an EMPTY ShapeInput
    let left = Instant::now();
    client(&s, "leave");
    assert_eq!(dom_state(&s), "preview", "grace period not honoured");
    wait_mode(&s, &conn, window, "idle", IslandMode::Still);
    assert!(
        left.elapsed() >= Duration::from_millis(250),
        "collapsed before the 300 ms grace: {:?}",
        left.elapsed()
    );
    assert!(input_region(&conn, window).is_empty());
    assert_eq!(actor_state(&s)["mode"], "still");

    // real pointer path: hover via the panel, move INTO the popup, panel leaves while the
    // pointer is inside (stays), then the pointer leaves (grace, idle)
    client(&s, "enter");
    wait_mode(&s, &conn, window, "preview", IslandMode::Hover);
    let enters_before = sent_intents(&s)
        .iter()
        .filter(|i| *i == "pointerEnter")
        .count();
    eis.move_to(100, 900); // make sure the pointer is outside first, then move in
    sleep(Duration::from_millis(300));
    client(&s, "enter");
    wait_mode(&s, &conn, window, "preview", IslandMode::Hover);
    let enters_before = enters_before.max(
        sent_intents(&s)
            .iter()
            .filter(|i| *i == "pointerEnter")
            .count(),
    );
    // Phase 5 authorized timeout fix: the failed wait was "DOM mouseenter reached
    // the actor". Do not aim at the centre sampled BEFORE this second opening.
    // Wait for this exact entry target's DOM geometry AND native ShapeInput to
    // settle for 250ms (bounded 5s), then send ONE physical entry. The original
    // DOM-intent and actor assertions below remain unchanged; no event retry.
    let mut previous = None;
    let mut stable_since = Instant::now();
    let inside = wait_for(
        "DOM mouseenter target settled in DOM and native ShapeInput",
        Duration::from_secs(5),
        || {
            let geometry = eval(&s, "(()=>{const r=document.querySelector('[data-state]').getBoundingClientRect();return [r.x,r.y,r.width,r.height]})()");
            let current = (geometry, input_region(&conn, window));
            if previous.as_ref() != Some(&current) {
                previous = Some(current);
                stable_since = Instant::now();
                return None;
            }
            (stable_since.elapsed() >= Duration::from_millis(250))
                .then(|| centre(&s, "[data-state]"))
        },
    );
    eis.move_to(inside.0, inside.1);
    wait_for(
        "DOM mouseenter reached the actor",
        Duration::from_secs(5),
        || {
            (sent_intents(&s)
                .iter()
                .filter(|i| *i == "pointerEnter")
                .count()
                > enters_before)
                .then_some(())
        },
    );
    assert_eq!(actor_state(&s)["hoverCause"]["type"], "pointer");
    client(&s, "leave");
    sleep(Duration::from_millis(900));
    assert_eq!(
        dom_state(&s),
        "preview",
        "pointer inside must keep the preview"
    );
    assert_eq!(actor_state(&s)["mode"], "hover");
    let left = Instant::now();
    // Phase 4: a REAL EIS exit. GTK forwards the native crossing even when WebKit does not
    // produce DOM mouseleave. No synthetic event is allowed on this exit path.
    eis.move_to(100, 900);
    wait_mode(&s, &conn, window, "idle", IslandMode::Still);
    assert!(
        left.elapsed() >= Duration::from_millis(250),
        "{:?}",
        left.elapsed()
    );
    assert!(input_region(&conn, window).is_empty());

    // ---- 2. panel pin: focus, typed key, cue, PTY persistence ------------------------------
    assert_eq!(
        eval(
            &s,
            "window.island.setSettings({clickSounds:true}).then(x=>x.clickSounds)"
        ),
        true
    );
    eis.move_to(800, 22);
    client(&s, "pin");
    wait_mode(&s, &conn, window, "pinned", IslandMode::Large);
    let pinned = actor_state(&s);
    assert_eq!(
        (pinned["mode"].as_str(), pinned["terminalStarted"].as_bool()),
        (Some("large"), Some(true))
    );
    wait_for(
        "panel pin focuses the island",
        Duration::from_secs(5),
        || (focus_report(&conn, root).1 == window).then_some(()),
    );
    wait_for("fixture prompt", Duration::from_secs(20), || {
        (eval(
            &s,
            "window.__islandTerminalTest?.text()?.includes('island-test>')",
        ) == true)
            .then_some(())
    });
    // NO click happened: the typed key reaches xterm only through the actor's RequestFocus.
    eis.send("type x");
    wait_for("typed x reached xterm", Duration::from_secs(8), || {
        (eval(
            &s,
            "window.__islandTerminalTest.text().includes('island-test> x')",
        ) == true)
            .then_some(())
    });
    eis.send("chord 29 22"); // Ctrl+U: never submit the draft
    wait_for("draft cleared", Duration::from_secs(8), || {
        (eval(
            &s,
            "!window.__islandTerminalTest.text().includes('island-test> x')",
        ) == true)
            .then_some(())
    });
    wait_for(
        "click cue for the panel pin",
        Duration::from_secs(5),
        || (eval(&s, "window.__cues.length") == 1).then_some(()),
    );
    assert_eq!(eval(&s, "window.__cues[0].kind"), "click");
    wait_for("fixture PTY", Duration::from_secs(10), || {
        (status(&s)["pty"].as_u64().unwrap() > 0).then_some(())
    });
    let pty = status(&s)["pty"].as_u64().unwrap();
    // DOM-origin intents withhold the cue (the DOM click capture already played it)
    eval(&s,"window.__TAURI_INTERNALS__.invoke('island_dispatch',{intent:{type:'pin'}}).then(x=>x.mode)");
    sleep(Duration::from_millis(600));
    assert_eq!(
        eval(&s, "window.__cues.length"),
        1,
        "DOM-origin pin must not cue"
    );

    // ---- 3. legacy tabs, arrow key and wheel move the ACTOR's view -------------------------
    let order = ["terminal", "monitor", "ai", "settings"];
    let check_view = |view: &str| {
        let state = wait_actor(&s, &format!("actor view {view}"), 10, |st| {
            st["view"] == view
        });
        assert_eq!(state["mode"], "large");
        wait_for(
            &format!("DOM data-view {view}"),
            Duration::from_secs(5),
            || {
                (eval(&s, "document.querySelector('[data-state]').dataset.view") == view)
                    .then_some(())
            },
        );
        wait_status_view(&s, view, state["rev"].as_u64().unwrap());
    };
    sleep(Duration::from_millis(1700));
    for view in ["monitor", "ai", "settings", "terminal"] {
        let target = centre(&s, &format!("[data-testid=view-{view}]"));
        eis.click_at(target.0, target.1);
        check_view(view);
        assert!(sent_intents(&s).contains(&"selectView".to_owned()));
        sleep(Duration::from_millis(1700)); // the island springs must settle before the next click
    }
    // arrow keys (focus is on a tab button, not the terminal): terminal -> monitor -> ai
    let target = centre(&s, "[data-testid=view-monitor]");
    eis.click_at(target.0, target.1);
    check_view("monitor");
    sleep(Duration::from_millis(1700));
    eis.send("chord 106"); // KEY_RIGHT
    check_view("ai");
    sleep(Duration::from_millis(1700));
    eis.send("chord 105"); // KEY_LEFT
    check_view("monitor");
    // wheel over the view stage (not the terminal): a horizontal notch moves to the next view
    sleep(Duration::from_millis(1700));
    let stage = centre(&s, ".view-stage");
    eis.move_to(stage.0, stage.1);
    eval(&s, "(window.__wheel=[],addEventListener(\"wheel\",e=>window.__wheel.push([e.deltaX,e.deltaY,e.deltaMode,e.target.className,e.defaultPrevented].join(\"/\")),true),true)");
    // The first axis event after a pointer move can be lost while KWin establishes the
    // pointer focus; retry (bounded) until the page has SEEN a wheel event, then require that
    // exactly that wheel navigates.
    let mut seen = false;
    for _ in 0..4 {
        eis.send("scroll 2 0");
        sleep(Duration::from_millis(700));
        if eval(&s, "window.__wheel.length").as_u64().unwrap() > 0 {
            seen = true;
            break;
        }
    }
    assert!(seen, "no wheel event reached the page after 4 EIS scrolls");
    check_view(order[order.iter().position(|v| *v == "monitor").unwrap() + 1]);

    // ---- 4. geometry is accepted only for the actor's CURRENT rev ---------------------------
    let current = actor_state(&s)["rev"].as_u64().unwrap();
    // settled? wait until the renderer's reports settled the region for this rev
    sleep(Duration::from_millis(900));
    let region = input_region(&conn, window);
    assert!(!region.is_empty());
    let big = "{x:0,y:0,width:1180,height:820,radius:0}";
    let stale = eval(
        &s,
        &format!("window.island.layout({big},'pinned',1,{})", current - 1),
    );
    assert_eq!(
        stale["applied"], false,
        "stale rev must be applied:false, not an error"
    );
    let wrong_mode = eval(
        &s,
        &format!("window.island.layout({big},'preview',1,{current})"),
    );
    assert_eq!(wrong_mode["applied"], false);
    let future = eval(
        &s,
        &format!("window.island.layout({big},'pinned',1,{})", current + 1),
    );
    assert_eq!(
        future["applied"], false,
        "a rev the actor has not reached is not current"
    );
    sleep(Duration::from_millis(400)); // the shim's own settled follow-up report (140 ms)
    assert_eq!(
        input_region(&conn, window),
        region,
        "rejected reports must not move the region"
    );

    // ---- 5. minimize / reopen: same PTY; still => EMPTY region regardless of reports ---------
    client(&s, "pin");
    wait_mode(&s, &conn, window, "idle", IslandMode::Still);
    let idle_rev = actor_state(&s)["rev"].as_u64().unwrap();
    let forced = eval(
        &s,
        &format!("window.island.layout({big},'idle',1,{idle_rev})"),
    );
    assert_eq!(forced["applied"], false);
    sleep(Duration::from_millis(400));
    assert!(
        input_region(&conn, window).is_empty(),
        "idle region must stay empty"
    );
    let claim_large = eval(
        &s,
        &format!("window.island.layout({big},'pinned',1,{idle_rev})"),
    );
    assert_eq!(claim_large["applied"], false);
    assert!(input_region(&conn, window).is_empty());
    assert_eq!(status(&s)["pty"], pty);
    client(&s, "pin");
    wait_mode(&s, &conn, window, "pinned", IslandMode::Large);
    assert_eq!(status(&s)["pty"], pty, "same PTY after minimize/reopen");
    assert_eq!(actor_state(&s)["terminalStarted"], true);

    // ---- teardown --------------------------------------------------------------------------
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
    wait_for("SIGTERM exit", Duration::from_secs(15), || {
        (app.0.try_wait().unwrap().is_some()
            && !std::path::Path::new(&format!("/proc/{pty}")).exists())
        .then_some(())
    });
    drop(eis);
    drop(app);
    s.shutdown();
    let scratch = s.root.clone();
    drop(s);
    assert!(!scratch.exists());
}
