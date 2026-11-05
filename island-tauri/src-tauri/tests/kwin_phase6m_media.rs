//! Phase 6m acceptance in the REAL app (nested KWin): MPRIS mock player on a private bus
//! (`HERMES_ISLAND_MPRIS_BUS`, honoured by test instances only), a local LRCLIB fixture, typed
//! events delivered into WebKit, `media_control` reaching only the mock, honest capabilities for
//! the opt-in lyrics / equalizer, and player loss clearing the state without restarting the app.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "helpers/gui_stack.rs"]
#[allow(dead_code)]
mod gui_stack;
#[path = "helpers/phase3_host.rs"]
mod host;
#[path = "helpers/lrclib_fixture.rs"]
mod lrclib_fixture;
#[path = "helpers/private_bus.rs"]
mod private_bus;
use common::*;
use gui_stack::Stack;
use host::{eval, until, Owned};
use lrclib_fixture::Lrclib;
use private_bus::{PlayerMock, PrivateBus};
use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};

fn launch(s: &Stack, mpris_bus: Option<&PrivateBus>, lrclib: Option<&Lrclib>) -> Owned {
    host::prepare(s);
    let log = fs::File::create(s.root.join("app.log")).unwrap();
    let mut command = host::command(s, &s.root.join("inst"));
    if let Some(bus) = mpris_bus {
        command.env("HERMES_ISLAND_MPRIS_BUS", bus.address());
    }
    if let Some(lrclib) = lrclib {
        command.env("HERMES_ISLAND_LRCLIB_BASE", lrclib.base());
    }
    let app = Owned(
        command
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    wait_for("phase6m private control", Duration::from_secs(30), || {
        s.root.join("inst/test-control.sock").exists().then_some(())
    });
    until(
        s,
        "phase6m legacy ready",
        20,
        "!!document.querySelector('[data-state]')",
    );
    app
}

fn bootstrap(s: &Stack) -> Value {
    eval(s, "window.__TAURI_INTERNALS__.invoke('island_bootstrap')")
}
/// The outcome of a command that may be rejected: `"ok"` or `"rejected:<error json>"`.
fn try_invoke(s: &Stack, name: &str, args: Value) -> String {
    let js = format!(
        "window.__TAURI_INTERNALS__.invoke({},{args}).then(()=>'ok',e=>'rejected:'+JSON.stringify(e))",
        json!(name)
    );
    eval(s, &js).as_str().unwrap().to_owned()
}
fn patch(s: &Stack, value: Value) {
    eval(s, &format!("window.island.setSettings({value})"));
}
fn listen(s: &Stack) {
    eval(
        s,
        "(async()=>{window.__ev={media:[],lyrics:[],eq:[],status:[]};const on=(event,key)=>window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event,target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(e=>window.__ev[key].push(e.payload))});await on('media-changed','media');await on('lyrics-changed','lyrics');await on('equalizer-changed','eq');await on('service-status','status');return true})()",
    );
}
fn finish(s: Stack, app: Owned) {
    let pid = app.0.id();
    drop(app);
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    let mut s = s;
    s.shutdown();
    let root = s.root.clone();
    drop(s);
    assert!(!root.exists());
}

fn song() -> Value {
    json!({
        "xesam:title": ["s", "Synced Song"],
        "xesam:artist": ["as", ["Artist"]],
        "xesam:album": ["s", "Album"],
        "mpris:trackid": ["o", "/t/1"],
        "mpris:length": ["x", 200_000_000i64],
    })
}

#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn media_lyrics_and_equalizer_work_end_to_end_in_the_real_app() {
    let _guard = lock();
    let bus = PrivateBus::start("p6m");
    let mut player = PlayerMock::start(&bus, "alpha");
    player.send(json!({"op":"set","props":{
        "Metadata": song(), "PlaybackStatus": "Playing", "Position": 5_000_000i64}}));
    let lrclib = Lrclib::start();
    let s = Stack::new("p6m");
    let app = launch(&s, Some(&bus), Some(&lrclib));
    let pid = app.0.id();

    // Capabilities + initial snapshot; lyrics and equalizer are OFF by default with a reason.
    wait_for("MPRIS available", Duration::from_secs(10), || {
        let b = bootstrap(&s);
        (b["capabilities"]["mediaStatus"]["available"] == true && !b["media"].is_null())
            .then_some(())
    });
    let b = bootstrap(&s);
    assert_eq!(b["capabilities"]["media"], true);
    assert_eq!(b["media"]["title"], "Synced Song");
    assert_eq!(b["media"]["player"], "alpha");
    assert_eq!(b["media"]["positionMs"], 5000.0);
    assert_eq!(b["media"]["durationMs"], 200000.0);
    assert!(b["mediaRev"].as_u64().unwrap() >= 1);
    assert_eq!(b["settings"]["lyricsEnabled"], false);
    assert_eq!(b["settings"]["equalizerEnabled"], false);
    assert_eq!(b["settings"]["equalizerBands"], 16);
    assert_eq!(b["capabilities"]["lyricsStatus"]["detail"], "disabled");
    assert_eq!(b["capabilities"]["equalizerStatus"]["detail"], "disabled");
    assert_eq!(b["lyrics"]["status"], "off");
    assert_eq!(lrclib.count(), 0, "lyrics are opt-in: no request yet");

    // Typed events arrive in WebKit; rev strictly increases.
    listen(&s);
    player.send(json!({"op":"set","props":{"PlaybackStatus":"Paused"}}));
    until(
        &s,
        "MediaChanged event in WebKit",
        8,
        "window.__ev.media.some(m=>m.nowPlaying&&m.nowPlaying.status==='paused'&&Number.isInteger(m.rev))",
    );

    // Commands: typed arguments, reach only the mock, bad shapes rejected before any call.
    assert_eq!(
        try_invoke(&s, "media_control", json!({"cmd":{"type":"playPause"}})),
        "ok"
    );
    assert_eq!(
        try_invoke(
            &s,
            "media_control",
            json!({"cmd":{"type":"seek","positionMs":12345}})
        ),
        "ok"
    );
    wait_for("mock received the calls", Duration::from_secs(8), || {
        (player.calls().len() == 2).then_some(())
    });
    let calls = player.calls();
    assert!(
        calls[0].starts_with("PlayPause") && calls[1].starts_with("SetPosition"),
        "{calls:?}"
    );
    for bad in [
        json!({"cmd":{"type":"explode"}}),
        json!({"cmd":{"type":"seek"}}),
        json!({"cmd":{"type":"seek","positionMs":1,"extra":1}}),
        json!({"cmd":"play"}),
    ] {
        assert!(
            try_invoke(&s, "media_control", bad.clone()).starts_with("rejected:"),
            "{bad}"
        );
    }
    assert!(try_invoke(
        &s,
        "media_control",
        json!({"cmd":{"type":"seek","positionMs":999999999}})
    )
    .starts_with("rejected:"));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(player.calls().len(), 2, "rejected commands made no call");

    // Opt in to lyrics: one request to the LOCAL fixture, a synced state arrives in WebKit.
    patch(&s, json!({"lyricsEnabled": true}));
    until(
        &s,
        "LyricsChanged synced event",
        10,
        "window.__ev.lyrics.some(l=>l.status==='synced'&&l.lines.length===3&&l.lines[0].text==='one')",
    );
    assert_eq!(lrclib.count(), 1);
    assert_eq!(bootstrap(&s)["lyrics"]["status"], "synced");
    // Opt-in equalizer + visibility: capture needs a resolvable PipeWire target; the scratch
    // runtime has none, so it reports why instead of recording anything.
    assert_eq!(
        try_invoke(&s, "media_set_visible", json!({"visible": true})),
        "ok"
    );
    assert!(
        try_invoke(&s, "media_set_visible", json!({"visible": "yes"})).starts_with("rejected:")
    );
    patch(&s, json!({"equalizerEnabled": true, "equalizerBands": 24}));
    wait_for("equalizer reports a reason", Duration::from_secs(8), || {
        let c = bootstrap(&s)["capabilities"]["equalizerStatus"].clone();
        (c["available"] == false && c["detail"] != "disabled" && c["detail"] != "starting")
            .then_some(())
    });
    assert_eq!(bootstrap(&s)["settings"]["equalizerBands"], 24);

    // Player quits: cleared (not stale), lyrics go idle, the app process is unchanged.
    player.quit();
    wait_for("media cleared", Duration::from_secs(8), || {
        bootstrap(&s)["media"].is_null().then_some(())
    });
    until(
        &s,
        "cleared MediaChanged event",
        8,
        "window.__ev.media.some(m=>m.nowPlaying===null)",
    );
    until(
        &s,
        "lyrics idle after the player quit",
        8,
        "window.__ev.lyrics.some(l=>l.status==='idle')",
    );
    assert_eq!(app.0.id(), pid);
    drop(lrclib);
    finish(s, app);
    drop(bus);
}

#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn a_test_instance_without_a_mock_bus_has_an_empty_media_state_and_stays_honest() {
    let _guard = lock();
    let s = Stack::new("p6m-nobus");
    let app = launch(&s, None, None);
    wait_for("media status", Duration::from_secs(10), || {
        let c = bootstrap(&s)["capabilities"].clone();
        (c["mediaStatus"]["detail"] != "starting").then_some(())
    });
    let b = bootstrap(&s);
    assert!(b["media"].is_null(), "no player: nothing fabricated");
    assert_eq!(b["lyrics"]["status"], "off");
    assert_eq!(b["capabilities"]["lyricsStatus"]["detail"], "disabled");
    assert_eq!(b["capabilities"]["equalizerStatus"]["detail"], "disabled");
    let error = try_invoke(&s, "media_control", json!({"cmd":{"type":"play"}}));
    assert!(
        error.starts_with("rejected:") && error.contains("unavailable"),
        "{error}"
    );
    finish(s, app);
}
