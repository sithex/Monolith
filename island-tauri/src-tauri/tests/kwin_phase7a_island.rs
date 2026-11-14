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

/// A stand-in Plasma job server on the stack's private session bus (KDE interface XML shapes),
/// so the Downloads page shows a real Transfers card from the island's passive job monitor.
struct JobServer;
#[zbus::interface(name = "org.kde.JobViewServerV2")]
impl JobServer {
    #[zbus(name = "requestView")]
    fn request_view(
        &self,
        _app: String,
        _caps: i32,
        _hints: std::collections::HashMap<String, zbus::OwnedVariant>,
    ) -> zbus::OwnedObjectPath {
        zbus::OwnedObjectPath::try_from("/org/kde/notificationmanager/jobs/JobView_1").unwrap()
    }
}
/// Starts a job server plus a KDE-style client that requests one copy view, 47 % done.
fn fake_copy(s: &Stack) -> (zbus::Connection, zbus::Connection) {
    use zbus::Variant;
    let address = format!("unix:path={}/bus", s.root.display());
    let block = |f| tauri::async_runtime::block_on(f);
    let server = block(
        zbus::connection::Builder::address(address.as_str())
            .name("org.kde.JobViewServer")
            .serve_at("/JobViewServer", JobServer)
            .build(),
    )
    .unwrap();
    // Plasma owns the server long before any copy starts: let the island's monitor arm on it.
    until(s, "7a job monitor armed", 10, "window.__TAURI_INTERNALS__.invoke('transfers_get').then(t=>t.detail==='monitoring')");
    let client = block(zbus::connection::Builder::address(address.as_str()).build()).unwrap();
    fs::create_dir_all(s.root.join("Videos")).unwrap();
    let dest = format!("file://{}/Videos/holiday%20film.mkv", s.root.display());
    let hints: std::collections::HashMap<String, zbus::OwnedVariant> = [
        ("title", Variant::from("Copying")),
        ("percent", Variant::from(47u32)),
        ("processedBytes", Variant::from(47u64 << 20)),
        ("totalBytes", Variant::from(100u64 << 20)),
        ("speed", Variant::from(3u64 << 20)),
        ("destUrl", Variant::from(dest)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), zbus::OwnedVariant::try_from(v).unwrap()))
    .collect();
    tauri::async_runtime::block_on(client.call_method(
        Some("org.kde.JobViewServer"),
        "/JobViewServer",
        Some("org.kde.JobViewServerV2"),
        "requestView",
        &("org.kde.dolphin", 3i32, hints),
    ))
    .unwrap();
    (server, client)
}

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
fn settings_section(s: &Stack, eis: &mut Wheel, origin: (i32, i32), section: &str) {
    click(
        s,
        eis,
        origin,
        &format!(".settings-rail [data-section={section}]"),
    );
    until(
        s,
        &format!("7a settings section {section} selected"),
        8,
        &format!(
            "document.querySelector('.settings-rail [aria-current=page]')?.dataset.section==={}",
            json!(section)
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
/// Select a view through the actor (pages hidden from the launcher by default included).
fn invoke_dispatch_view(s: &Stack, id: &str) {
    eval(s, &format!("window.__TAURI_INTERNALS__.invoke('island_dispatch',{{intent:{{type:'selectView',view:{}}}}}).then(()=>true)", json!(id)));
}
fn screenshot(s: &Stack, conn: &RustConnection, window: u32, name: &str) {
    let overlay = host::overlay_size();
    assert_eq!(eval(s, "(()=>{const p=document.querySelector('.app-pane:not([hidden])');return p.scrollWidth<=p.clientWidth+1})()"), true, "view overflows horizontally");
    // Blinking carets (a focused field, the layout editor's mini terminal) never settle; freeze
    // them for the pixel comparison only.
    eval(s, "document.getElementById('test-no-caret')||document.head.append(Object.assign(document.createElement('style'),{id:'test-no-caret',textContent:'*{caret-color:transparent!important}.le-mini-caret{animation:none!important}'}))");
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
    // Drop shelf (Session 5): parked paths seeded before launch (the test instance serves no
    // D-Bus, so nothing can drop onto it): a file with a long name, a folder, and one that
    // has since been deleted.
    let parked = s.root.join("shelf-files");
    fs::create_dir_all(parked.join("Project folder")).unwrap();
    let long_name = parked.join("quarterly report with a rather long name that has to wrap.pdf");
    fs::write(&long_name, b"x").unwrap();
    let inst = s.root.join("inst");
    fs::create_dir_all(&inst).unwrap();
    std::fs::set_permissions(&inst, <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700)).unwrap();
    {
        use hermes_island_lib::integrations::shelf::core::Shelf;
        let mut shelf = Shelf::default();
        shelf.add(vec![long_name.clone(), parked.join("Project folder"), parked.join("gone.txt")], 1, 12);
        hermes_island_lib::integrations::private_file::write(&inst.join("shelf.json"), shelf.to_json().as_bytes(), 65536).unwrap();
    }
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
    // Unavailable services render ServiceNotice (AppPrimitives.tsx): a plain sentence, never a raw
    // code, and a Settings button only where a setting exists. The test instance has no Downloads
    // folder (not-configured: card, no button); weather enabled without a location is no-location
    // ("Choose location" inside the Weather card, which is stacked with the Battery/Bluetooth cards).
    eval(&s, "window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{weatherEnabled:true}})");
    for (id, ready) in [
        ("downloads", ".service-notice[data-kind=setup]"),
        ("quickInfo", ".service-notice-action button"),
    ] {
        invoke_dispatch_view(&s, id);
        until(&s, &format!("7a {id} service notice"), 15, &format!("document.querySelector('.island').dataset.view==={} && getComputedStyle(document.querySelector('.app-pane:not([hidden])')).opacity==='1' && !!document.querySelector('.app-pane:not([hidden]) {ready}')", json!(id)));
        settled(&s, &conn, window, IslandMode::Large);
        let g = eval(&s, "(()=>{const R=e=>e.getBoundingClientRect(),pane=document.querySelector('.app-pane:not([hidden])');\
            const hit=(a,t)=>a.left<t.right-1&&a.right>t.left+1&&a.top<t.bottom-1&&a.bottom>t.top+1;\
            const notices=[...pane.querySelectorAll('.service-notice')].map(n=>{const card=n.closest('.card'),cr=R(card),b=n.querySelector('.service-notice-action button'),br=b&&R(b);\
            const texts=[n.querySelector('.service-notice-text'),card.querySelector(':scope > h3')].filter(Boolean).map(R);\
            return {text:n.innerText,button:!!b,size:br?Math.min(br.width,br.height):0,inCard:!br||(br.left>=cr.left-0.5&&br.right<=cr.right+0.5&&br.top>=cr.top-0.5&&br.bottom<=cr.bottom+0.5),\
            overlap:!!br&&texts.some(t=>hit(br,t))}});\
            const cards=[...pane.querySelectorAll('.app-stack > .card')].filter(e=>e.offsetParent).map(R);const cardGap=cards.length<2?99:Math.min(...cards.slice(1).map((b,i)=>b.top-cards[i].bottom));\
            return {notices,cards:cards.length,cardGap,raw:/Unavailable ·|not-configured|no-location/.test(pane.innerText)}})()");
        let notices = g["notices"].as_array().unwrap();
        assert!(!notices.is_empty(), "{id}: no service notice {g}");
        assert_eq!(g["raw"], false, "{id}: raw status code shown {g}");
        for n in notices {
            assert_eq!(n["inCard"], true, "{id}: notice button outside its card {g}");
            assert_eq!(n["overlap"], false, "{id}: notice button overlaps its sentence or card title {g}");
            if n["button"] == true {
                assert!(n["size"].as_f64().unwrap() >= 20.0, "{id}: notice button collapsed {g}");
            }
        }
        assert!(g["cardGap"].as_f64().unwrap() >= 10.0, "{id}: stacked cards touch {g}");
        if id == "downloads" {
            // The Downloads folder comes from the desktop's user dirs: no Settings row, no button.
            assert!(notices.iter().all(|n| n["button"] == false), "downloads: unexpected button {g}");
            // A KDE copy in progress: the Transfers card sits above the notice, its "Show in
            // folder" button inside its row and clear of every line of text, the bar inside the
            // row, and a real gap between the card and the notice below it.
            let copy = fake_copy(&s);
            until(&s, "7a transfers card", 15, "!!document.querySelector('.app-pane:not([hidden]) .transfer-row .row-actions button')");
            settled(&s, &conn, window, IslandMode::Large);
            let t = eval(&s, "(()=>{const R=e=>e.getBoundingClientRect(),pane=document.querySelector('.app-pane:not([hidden])');\
                const hit=(a,t)=>a.left<t.right-1&&a.right>t.left+1&&a.top<t.bottom-1&&a.bottom>t.top+1;\
                const inside=(a,c)=>a.left>=c.left-0.5&&a.right<=c.right+0.5&&a.top>=c.top-0.5&&a.bottom<=c.bottom+0.5;\
                const section=pane.querySelector('.transfers'),row=section.querySelector('.transfer-row'),rr=R(row),b=row.querySelector('.row-actions button'),br=R(b);\
                const texts=[...row.querySelectorAll('.row-content strong, .row-content small')].map(R),bar=R(row.querySelector('.transfer-bar'));\
                const next=section.nextElementSibling,nr=next&&R(next);\
                return {label:b.innerText,text:row.innerText,buttonIn:inside(br,rr),barIn:inside(bar,rr),overlap:texts.some(x=>hit(br,x))||hit(br,bar),\
                size:Math.min(br.width,br.height),barWidth:R(row.querySelector('.transfer-bar > span')).width/bar.width,\
                gap:nr?nr.top-R(section).bottom:-1,above:!!nr&&next.matches('.card'),overflow:pane.scrollWidth>pane.clientWidth+1}})()");
            assert_eq!(t["label"], "Show in folder", "{t}");
            assert!(t["text"].as_str().unwrap().contains("Copying · 47%"), "{t}");
            assert!(t["text"].as_str().unwrap().contains("Dolphin →"), "{t}");
            assert_eq!(t["buttonIn"], true, "transfers: button outside its row {t}");
            assert_eq!(t["barIn"], true, "transfers: bar outside its row {t}");
            assert_eq!(t["overlap"], false, "transfers: button overlaps the text or bar {t}");
            assert!(t["size"].as_f64().unwrap() >= 20.0, "transfers: button collapsed {t}");
            assert!((t["barWidth"].as_f64().unwrap() - 0.47).abs() < 0.02, "transfers: bar is not 47% {t}");
            assert_eq!(t["above"], true, "transfers: card is not above the notice {t}");
            assert!(t["gap"].as_f64().unwrap() >= 10.0, "transfers: card touches the notice {t}");
            assert_eq!(t["overflow"], false, "transfers: page scrolls sideways {t}");
            // Show in folder reaches Rust; a test instance never opens a real file manager.
            let reveal = eval(&s, "window.__TAURI_INTERNALS__.invoke('transfer_reveal',{id:'/org/kde/notificationmanager/jobs/JobView_1'}).then(()=>'opened',e=>e.code)");
            assert_eq!(reveal, "unsupported", "{reveal}");
            screenshot(&s, &conn, window, "transfers-downloads");
            // The copy's client leaves the bus: the card goes away by itself.
            drop(copy);
            until(&s, "7a transfers card cleared", 15, "!document.querySelector('.transfer-row')");
        } else {
            assert!(g["cards"].as_u64().unwrap() >= 2, "quickInfo: expected stacked cards {g}");
            assert!(notices.iter().any(|n| n["button"] == true && n["text"].as_str().unwrap().contains("Choose a location")), "quickInfo: no location button {g}");
        }
        screenshot(&s, &conn, window, &format!("service-notice-{id}"));
    }
    eval(&s, "window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{weatherEnabled:false}})");
    // Drop shelf page: three seeded cards (one moved/deleted). Every button inside its card and
    // clear of the text, cards at least 10 px apart, no sideways scroll.
    invoke_dispatch_view(&s, "shelf");
    until(&s, "7a shelf cards", 15, "document.querySelector('.island').dataset.view==='shelf' && getComputedStyle(document.querySelector('.app-pane:not([hidden])')).opacity==='1' && document.querySelectorAll('.app-pane:not([hidden]) .shelf-row').length===3");
    settled(&s, &conn, window, IslandMode::Large);
    let g = eval(&s, "(()=>{const R=e=>e.getBoundingClientRect(),pane=document.querySelector('.app-pane:not([hidden])'),rows=[...pane.querySelectorAll('.shelf-row')];\
        const inside=(a,b)=>a.left>=b.left-.5&&a.right<=b.right+.5&&a.top>=b.top-.5&&a.bottom<=b.bottom+.5,hit=(a,b)=>a.left<b.right-.5&&b.left<a.right-.5&&a.top<b.bottom-.5&&b.top<a.bottom-.5;\
        const cards=rows.map(row=>{const rr=R(row),texts=[...row.querySelectorAll('.row-content > *')].map(R),buttons=[...row.querySelectorAll('.row-actions button')];\
          return {labels:buttons.map(b=>b.textContent),inside:buttons.every(b=>inside(R(b),rr)),overlap:buttons.some(b=>texts.some(t=>hit(R(b),t)))||buttons.some((b,i)=>buttons.some((c,j)=>i<j&&hit(R(b),R(c)))),\
            small:buttons.some(b=>R(b).height<20||R(b).width<30),draggable:row.draggable,textInside:texts.every(t=>inside(t,rr))};});\
        const gaps=rows.slice(1).map((r,i)=>R(r).top-R(rows[i]).bottom),clear=pane.querySelector('.shelf-clear'),cr=clear&&R(clear),last=R(rows[rows.length-1]);\
        return {cards,gaps,overflow:pane.scrollWidth>pane.clientWidth+1,clearBelow:!!clear&&cr.top>=last.bottom+10,clearIn:!!clear&&inside(cr,R(pane))}})()");
    let cards = g["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 3, "{g}");
    assert_eq!(cards[0]["labels"], json!(["Open", "Reveal", "Copy path", "Remove"]), "{g}");
    assert_eq!(cards[2]["labels"], json!(["Copy path", "Remove"]), "deleted item keeps only Copy/Remove {g}");
    for c in cards {
        assert_eq!(c["inside"], true, "shelf: button outside its card {g}");
        assert_eq!(c["textInside"], true, "shelf: text outside its card {g}");
        assert_eq!(c["overlap"], false, "shelf: button overlaps text or another button {g}");
        assert_eq!(c["small"], false, "shelf: button collapsed {g}");
    }
    assert!(g["gaps"].as_array().unwrap().iter().all(|v| v.as_f64().unwrap() >= 10.0), "shelf: cards touch {g}");
    assert_eq!(g["overflow"], false, "shelf: page scrolls sideways {g}");
    assert_eq!(g["clearBelow"], true, "shelf: Clear all touches the cards {g}");
    assert_eq!(g["clearIn"], true, "shelf: Clear all outside the page {g}");
    screenshot(&s, &conn, window, "shelf");
    // Open/Reveal reach Rust by id; a test instance never opens a real app.
    let opened = eval(&s, "window.__TAURI_INTERNALS__.invoke('shelf_get').then(v=>window.__TAURI_INTERNALS__.invoke('shelf_command',{op:{type:'open',id:v.items[0].id}})).then(()=>'opened',e=>e.code)");
    assert_eq!(opened, "unsupported", "{opened}");
    eval(&s, "(document.querySelector('[aria-label=\"Remove gone.txt from the shelf\"]').click(),true)");
    until(&s, "7a shelf remove", 10, "document.querySelectorAll('.app-pane:not([hidden]) .shelf-row').length===2");
    assert!(!fs::read_to_string(inst.join("shelf.json")).unwrap().contains("gone.txt"));
    // Navigation guard (Rust on_navigation): the page may not load anything outside the app.
    eval(&s, &format!("(location.href={},true)", json!(format!("file://{}", long_name.display()))));
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(eval(&s, "location.protocol+'//'+location.host+'|'+!!document.querySelector('.island')"), "tauri://localhost|true", "a file URL replaced the island page");
    // Skin is the fresh-install default (services/frontend.rs), so pick neutral first: every
    // iteration is a real change that must persist without a restart.
    for theme in ["neutral", "skin"] {
        view(&s, &conn, window, &mut eis, origin, "settings");
        // Colour schemes live in Settings → Look (SettingsView.tsx).
        settings_section(&s, &mut eis, origin, "look");
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
                    "!!document.querySelector('[role=progressbar][aria-label=\"CPU usage\"][aria-valuenow]')",
                );
            }
            if id == "ai" {
                // AI Agents opens on Live: with no metrics plugin it shows the calm empty state,
                // then the Usage tab holds the original dashboard. The view reopens on the last
                // page used, so pick Live explicitly (this loop visits AI more than once).
                eval(
                    &s,
                    "document.querySelector('.ai-stats-tabs [role=tab]:nth-child(1)').click()",
                );
                until(
                    &s,
                    "7a AI Live page without plugin",
                    15,
                    "document.querySelector('.app-pane:not([hidden])').innerText.includes('Live metrics plugin not installed')",
                );
                eval(
                    &s,
                    "document.querySelector('.ai-stats-tabs [role=tab]:nth-child(2)').click()",
                );
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
                // Settings reopens on the last section used, so open General explicitly.
                eval(&s, "document.querySelector('.settings-rail [data-section=general]').click()");
                std::thread::sleep(Duration::from_millis(400));
                let text = eval(&s, "document.querySelector('.app-pane:not([hidden])').innerText");
                let text = text.as_str().unwrap().to_lowercase();
                // Seven-section icon rail.
                assert_eq!(
                    eval(&s, "[...document.querySelectorAll('.settings-rail [data-section]')].map(b=>b.dataset.section).join(',')"),
                    "general,notifications,look,notch,terminal,apps,island",
                    "settings rail sections"
                );
                assert!(
                    ["behavior", "clock", "shortcuts"].iter().all(|t| text.contains(t)),
                    "settings pane: {text}"
                );
                // Rendered layout of every section: the rail keeps its width and sits left of the
                // content, nothing spills past the pane horizontally, every row's control stays
                // inside its row, and a row's label never overlaps its control.
                for section in ["general", "notifications", "look", "notch", "terminal", "apps", "island"] {
                    eval(&s, &format!("document.querySelector('.settings-rail [data-section={section}]').click()"));
                    std::thread::sleep(Duration::from_millis(450));
                    let g = eval(&s, "(()=>{const R=e=>e.getBoundingClientRect(),rail=R(document.querySelector('.settings-rail')),c=document.querySelector('.settings-content'),cr=R(c),pane=R(document.querySelector('.app-pane:not([hidden])'));\
                        const bad=[...c.querySelectorAll('.settings-row')].filter(r=>r.offsetParent).map(r=>{const rr=R(r),l=r.querySelector(':scope > .label'),kids=[...r.children].filter(k=>k!==l);\
                        const out=kids.some(k=>{const kr=R(k);return kr.width>0&&(kr.right>rr.right+1||kr.left<rr.left-1)});const lap=l&&kids.some(k=>{const kr=R(k);return kr.width>0&&kr.left<R(l).right-1&&kr.top<R(l).bottom-1&&kr.bottom>R(l).top+1});\
                        return (out||lap)?(r.dataset.settingId||r.textContent.slice(0,30))+(out?':out':':overlap'):null}).filter(Boolean);\
                        const cards=[...c.querySelectorAll(':scope > .settings-card, :scope > section')].filter(e=>e.offsetParent).map(R);const cardGap=cards.length<2?99:Math.min(...cards.slice(1).map((b,i)=>b.top-cards[i].bottom));\
                        return {rail:rail.width,gap:cr.left-rail.right,cardGap,fitClient:c.clientHeight,fitScroll:c.scrollHeight,spill:c.scrollWidth-c.clientWidth,paneRight:pane.right-cr.right,rows:c.querySelectorAll('.settings-row').length,bad}})()");
                    assert!((30.0..=56.0).contains(&g["rail"].as_f64().unwrap()), "{section}: rail width {g}");
                    assert!(g["gap"].as_f64().unwrap() >= 0.0, "{section}: content overlaps rail {g}");
                    assert!(g["spill"].as_f64().unwrap() <= 1.0, "{section}: horizontal overflow {g}");
                    assert!(g["paneRight"].as_f64().unwrap() >= -1.0, "{section}: content past the pane {g}");
                    assert!(g["rows"].as_u64().unwrap() > 0, "{section}: empty section {g}");
                    // Separate cards: a visible gap between every pair of stacked cards.
                    assert!(g["cardGap"].as_f64().unwrap() >= 10.0, "{section}: cards touch {g}");
                    // The island grows to fit the section (up to its maximum height) instead of
                    // squeezing it into a small scroll box: Settings scrolls inside .settings-content,
                    // and that hidden height must still count when the island sizes itself.
                    let (client, scroll) = (g["fitClient"].as_f64().unwrap(), g["fitScroll"].as_f64().unwrap());
                    assert!(client >= scroll.min(450.0) - 1.0, "{section}: island did not grow to fit Settings {g}");
                    assert_eq!(g["bad"], serde_json::json!([]), "{section}: misplaced controls {g}");
                    screenshot(&s, &conn, window, &format!("{theme}-settings-{section}"));
                }
                eval(&s, "document.querySelector('.settings-rail [data-section=general]').click()");
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
    settings_section(&s, &mut eis, origin, "look");
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
