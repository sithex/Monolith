//! Real EIS acceptance, privately scoped app/bus/PipeWire only. No synthetic navigation.
use super::*;
use std::os::unix::fs::PermissionsExt;
fn connected(s: &Stack, app: &Owned) -> (RustConnection, u32, Wheel, (i32, i32)) {
    let c = s.connect();
    let root = c.setup().roots[0].root;
    let w = wait_for("7b acceptance window", Duration::from_secs(30), || {
        find_island_window(&c, root, app.0.id())
    });
    wait_for(
        "7b acceptance private control",
        Duration::from_secs(20),
        || s.root.join("inst/test-control.sock").exists().then_some(()),
    );
    wait_for(
        "7b acceptance seeded with no JavaScript errors",
        Duration::from_secs(20),
        || {
            let state=eval(s,"({mounted:!!document.querySelector('.island'),errors:window.__phase7bErrors||[],body:document.body.innerText})");
            assert_eq!(
                state["errors"],
                json!([]),
                "renderer startup failure: {state}"
            );
            (state["mounted"] == true).then_some(())
        },
    );
    let p = c
        .translate_coordinates(w, root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    let e = Wheel::new(s);
    client(s, "pin");
    until(
        s,
        "7b acceptance large",
        12,
        "document.querySelector('.island')?.dataset.state==='large'",
    );
    settled(s, &c, w, IslandMode::Large);
    (c, w, e, (i32::from(p.dst_x), i32::from(p.dst_y)))
}
fn start(tag: &str) -> (Stack, Owned, RustConnection, u32, Wheel, (i32, i32)) {
    let s = Stack::new(tag);
    prepare(&s);
    fs::create_dir(s.root.join("downloads")).unwrap();
    let app = launch(&s);
    let (c, w, e, o) = connected(&s, &app);
    eval(&s,"window.__TAURI_INTERNALS__.invoke('settings_get').then(s=>{const r=s.moduleRegistry;for(const m of r.modules)if(m.config&&typeof m.config.type==='string')m.enabled=true;return window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{moduleRegistry:r}})})");
    (s, app, c, w, e, o)
}
fn finish(mut s: Stack, app: Owned, e: Wheel) {
    drop(e);
    let pid = app.0.id();
    drop(app);
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    s.shutdown();
    let p = s.root.clone();
    drop(s);
    assert!(!p.exists());
}
fn grid(s: &Stack, c: &RustConnection, w: u32, e: &mut Wheel, o: (i32, i32)) {
    click(s, e, o, "[data-testid=side-apps]");
    until(
        s,
        "7b launcher real click",
        10,
        "document.querySelector('.island')?.dataset.view==='launcher'",
    );
    settled(s, c, w, IslandMode::Large);
}
pub(super) fn begin_edit(s: &Stack, e: &mut Wheel, o: (i32, i32)) {
    click(s, e, o, "[aria-label='Launcher options']");
    until(
        s,
        "7b launcher edit menu",
        5,
        "!!document.querySelector('.popover button')",
    );
    click(s, e, o, ".popover button");
    until(
        s,
        "7b server entered edit",
        8,
        "document.querySelector('.app-grid')?.dataset.editing==='true'",
    );
}
fn order(s: &Stack) -> Value {
    eval(
        s,
        "[...document.querySelectorAll('[data-tile]')].map(e=>e.dataset.tile)",
    )
}
fn drag(s: &Stack, e: &mut Wheel, o: (i32, i32)) {
    let a = point(s, o, "[data-tile=timer] .app-tile");
    let b = point(s, o, "[data-tile=scratchpad] .app-tile");
    e.move_to(a.0, a.1);
    e.send("down");
    for n in 1..=12 {
        e.move_to(a.0 + (b.0 - a.0) * n / 12, a.1 + (b.1 - a.1) * n / 12);
    }
    e.send("up");
    until(s,"7b real drag changes authoritative order",8,"(()=>{const ids=[...document.querySelectorAll('[data-tile]')].map(e=>e.dataset.tile);return ids.indexOf('timer')>ids.indexOf('scratchpad')})()");
}
#[test]
fn launcher_real_drag_done_persists_across_restart() {
    let _guard = lock();
    let (s, app, c, w, mut e, o) = start("7b-drag");
    grid(&s, &c, w, &mut e, o);
    let before = order(&s);
    begin_edit(&s, &mut e, o);
    drag(&s, &mut e, o);
    let changed = order(&s);
    assert_ne!(before, changed);
    click(&s, &mut e, o, ".launcher-toolbar .primary");
    until(
        &s,
        "7b Done committed",
        10,
        "document.querySelector('.app-grid')?.dataset.editing==='false'",
    );
    let persisted = fs::read(s.root.join("inst/settings.json")).unwrap();
    assert!(
        serde_json::from_slice::<Value>(&persisted).unwrap()["moduleRegistry"]["modules"]
            .is_array()
    );
    drop(e);
    drop(app);
    let app = launch(&s);
    let (c, w, mut e, o) = connected(&s, &app);
    grid(&s, &c, w, &mut e, o);
    assert_eq!(order(&s), changed);
    finish(s, app, e);
}
#[test]
fn launcher_real_cancel_does_not_persist() {
    let _guard = lock();
    let (s, app, c, w, mut e, o) = start("7b-cancel");
    grid(&s, &c, w, &mut e, o);
    let before = order(&s);
    let disk = fs::read(s.root.join("inst/settings.json")).unwrap();
    begin_edit(&s, &mut e, o);
    drag(&s, &mut e, o);
    click(&s, &mut e, o, ".launcher-toolbar button");
    until(
        &s,
        "7b Cancel reverted",
        8,
        "document.querySelector('.app-grid')?.dataset.editing==='false'",
    );
    assert_eq!(order(&s), before);
    assert_eq!(fs::read(s.root.join("inst/settings.json")).unwrap(), disk);
    finish(s, app, e);
}
#[test]
fn launcher_real_disable_badge_hides_unlocked_module() {
    let _guard = lock();
    let (s, app, c, w, mut e, o) = start("7b-disable");
    grid(&s, &c, w, &mut e, o);
    begin_edit(&s, &mut e, o);
    click(&s, &mut e, o, "[aria-label='Hide Timer']");
    until(
        &s,
        "7b badge hides timer",
        8,
        "!document.querySelector('[data-tile=timer]')",
    );
    click(&s, &mut e, o, ".launcher-toolbar .primary");
    until(
        &s,
        "7b disable committed",
        8,
        "document.querySelector('.app-grid')?.dataset.editing==='false'",
    );
    assert_eq!(eval(&s,"window.__TAURI_INTERNALS__.invoke('settings_get').then(s=>s.moduleRegistry.modules.find(m=>m.id==='timer').enabled)"),false);
    assert_eq!(
        eval(&s, "!!document.querySelector('[data-testid=side-timer]')"),
        false
    );
    finish(s, app, e);
}
#[test]
fn launcher_home_and_settings_are_not_movable_or_disableable() {
    let _guard = lock();
    let (s, app, c, w, mut e, o) = start("7b-locked");
    grid(&s, &c, w, &mut e, o);
    begin_edit(&s, &mut e, o);
    for id in ["terminal", "settings"] {
        assert_eq!(eval(&s,&format!("(()=>{{const e=document.querySelector('[data-tile={id}]');return e.querySelector('.app-tile').dataset.locked==='true'&&!e.querySelector('.disable-badge')&&!e.querySelector('.move-controls')}})()")),true);
    }
    finish(s, app, e);
}
#[test]
fn timer_completed_activity_is_dismissable_by_real_input() {
    let _guard = lock();
    let (s, app, c, w, mut e, o) = start("7b-timer-dismiss");
    open(&s, &c, w, &mut e, o, "timer");
    eval(&s,"window.__TAURI_INTERNALS__.invoke('timer_command',{op:{type:'setCountdown',durationMs:1000}}).then(()=>window.__TAURI_INTERNALS__.invoke('timer_command',{op:{type:'start',kind:'countdown'}}))");
    until(
        &s,
        "7b completed activity visible",
        8,
        "document.querySelector('.timer-app')?.innerText.includes('Dismiss completed timer')",
    );
    let labels = eval(
        &s,
        "[...document.querySelectorAll('.timer-app button')].map(e=>e.textContent)",
    );
    assert!(labels
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l == "Dismiss completed timer"));
    click(&s, &mut e, o, "[data-testid=timer-dismiss]");
    until(&s,"7b real dismiss removes completed activity",8,"window.__TAURI_INTERNALS__.invoke('timer_get').then(s=>s.completedKind===null&&s.countdownStatus==='idle')");
    assert_eq!(
        eval(
            &s,
            "document.querySelector('.timer-app').innerText.includes('Dismiss completed timer')"
        ),
        false
    );
    finish(s, app, e);
}
#[test]
fn frozen_prototype_keeps_real_terminal_and_motion_working() {
    let _guard = lock();
    let (s, app, c, w, mut e, o) = start("7b-frozen");
    assert_eq!(eval(&s, "Object.isFrozen(Object.prototype)"), true);
    until(
        &s,
        "7b frozen real xterm prompt",
        15,
        "window.__islandTerminalTest?.text().includes('island-test>')===true",
    );
    click(&s, &mut e, o, ".terminal");
    e.send("type FROZEN_UNSUBMITTED");
    until(
        &s,
        "7b frozen composer",
        8,
        "window.__islandTerminalTest.text().includes('FROZEN_UNSUBMITTED')",
    );
    e.send("chord 29 22");
    open(&s, &c, w, &mut e, o, "timer");
    finish(s, app, e);
}
fn record_pacing(s: &Stack) {
    eval(s,"(()=>{window.__pacing={d:[],done:false};let p=null;const stop=performance.now()+1200;const frame=t=>{if(p!==null)window.__pacing.d.push(t-p);p=t;if(t<stop)requestAnimationFrame(frame);else window.__pacing.done=true;};requestAnimationFrame(frame);return true})()");
}
fn pacing(s: &Stack) -> Value {
    until(s, "7b pacing recorded", 5, "window.__pacing?.done===true");
    let v=eval(s,"(()=>{const d=window.__pacing.d.sort((a,b)=>a-b);return {count:d.length,p50:d[Math.floor(d.length*.5)],p95:d[Math.floor(d.length*.95)],max:d.at(-1),over20:d.filter(t=>t>20).length}})()");
    assert!(v["count"].as_u64().unwrap() > 3);
    v
}
#[test]
fn native_pill_side_union_and_launcher_app_pacing_use_real_input() {
    let _guard = lock();
    let (s, app, c, w, mut e, o) = start("7b-pacing");
    let pill:Value=eval(&s,"(()=>{const s=getComputedStyle(document.documentElement),n=k=>parseFloat(s.getPropertyValue('--native-pill-'+k));return {x:n('x'),y:n('y'),width:n('width'),height:n('height')}})()");
    assert!(pill["width"].as_f64().unwrap() > 0.);
    let rows = input_region(&c, w);
    let px = pill["x"].as_f64().unwrap() as i32;
    let py = pill["y"].as_f64().unwrap() as i32;
    assert!(!hermes_island_lib::window::shape::covers(&rows, px, py));
    assert!(!hermes_island_lib::window::shape::covers(
        &rows,
        px + 75,
        py + 16
    ));
    record_pacing(&s);
    grid(&s, &c, w, &mut e, o);
    let launcher = pacing(&s);
    record_pacing(&s);
    click(&s, &mut e, o, "[data-testid=view-timer]");
    until(
        &s,
        "7b timed app open",
        8,
        "document.querySelector('.island').dataset.view==='timer'",
    );
    settled(&s, &c, w, IslandMode::Large);
    let app_time = pacing(&s);
    fs::write(
        Evidence::new("phase7b").path("frame-pacing.json"),
        serde_json::to_vec_pretty(&json!({"launcher":launcher,"app":app_time})).unwrap(),
    )
    .unwrap();
    client(&s, "pin");
    until(
        &s,
        "7b idle empty input",
        8,
        "document.querySelector('.island').dataset.state==='still'",
    );
    settled(&s, &c, w, IslandMode::Still);
    assert!(input_region(&c, w).is_empty());
    finish(s, app, e);
}
fn captures(s: &Stack) -> Vec<u32> {
    fs::read_to_string(s.root.join("capture-starts.log"))
        .unwrap_or_default()
        .lines()
        .map(|s| s.parse().unwrap())
        .collect()
}
#[test]
fn media_capture_stops_on_real_navigation_and_collapse() {
    let _guard = lock();
    assert!(
        pipewire_fixture::PipeWire::available(),
        "private PipeWire tools required"
    );
    let mut pw = pipewire_fixture::PipeWire::start("7b-visible");
    pw.play(440., "alpha");
    let bus = private_bus::PrivateBus::start("7b-visible");
    let mut player = private_bus::PlayerMock::start(&bus, "alpha");
    player.send(json!({"op":"set","props":{"Metadata":{"xesam:title":["s","Private sine"],"xesam:artist":["as",["Fixture"]],"mpris:trackid":["o","/t/1"],"mpris:length":["x",200000000]},"PlaybackStatus":"Playing"}}));
    let s = Stack::new("7b-media");
    prepare(&s);
    fs::create_dir(s.root.join("downloads")).unwrap();
    let script = s.root.join("bin/pw-record");
    fs::write(&script,"#!/usr/bin/python3\nimport os,sys\nassert '--target' in sys.argv\nfd=os.open(os.environ['HERMES_7B_CAPTURE_LOG'],os.O_WRONLY|os.O_CREAT|os.O_APPEND,0o600)\nos.write(fd,(str(os.getpid())+'\\n').encode());os.close(fd)\nos.execv('/usr/bin/pw-record',['pw-record',*sys.argv[1:]])\n").unwrap();
    fs::set_permissions(script, fs::Permissions::from_mode(0o700)).unwrap();
    let app = launch_with(&s, Some(bus.address()), Some(pw.root.join("run")));
    let (c, w, mut e, o) = connected(&s, &app);
    eval(&s,"window.__TAURI_INTERNALS__.invoke('settings_get').then(s=>{s.moduleRegistry.modules.find(m=>m.id==='music').enabled=true;return window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{moduleRegistry:s.moduleRegistry,equalizerEnabled:true,lyricsEnabled:false}})})");
    until(&s,"7b hidden capture reason",10,"window.__TAURI_INTERNALS__.invoke('island_bootstrap').then(b=>b.capabilities.equalizerStatus.detail==='hidden')");
    assert!(captures(&s).is_empty());
    for collapse in [false, true] {
        open(&s, &c, w, &mut e, o, "music");
        wait_for(
            "7b private real capture started",
            Duration::from_secs(12),
            || captures(&s).last().copied(),
        );
        until(
            &s,
            "7b private equalizer active",
            8,
            "document.querySelector('.equalizer')!==null",
        );
        let pid = *captures(&s).last().unwrap();
        assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
        if collapse {
            client(&s, "pin");
        } else {
            open(&s, &c, w, &mut e, o, "terminal");
        }
        until(&s,"7b hidden media capture stopped",8,"window.__TAURI_INTERNALS__.invoke('island_bootstrap').then(b=>b.capabilities.equalizerStatus.detail==='hidden')");
        wait_for(
            "7b owned private capture reaped",
            Duration::from_secs(5),
            || (!std::path::Path::new(&format!("/proc/{pid}")).exists()).then_some(()),
        );
    }
    assert_eq!(captures(&s).len(), 2);
    fs::write(
        Evidence::new("phase7b").path("media-capture-counts.json"),
        serde_json::to_vec(&json!({"starts":captures(&s).len(),"reaped":true})).unwrap(),
    )
    .unwrap();
    finish(s, app, e);
    drop(player);
    drop(bus);
    drop(pw);
}
