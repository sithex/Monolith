//! Phase 6a acceptance in the REAL app (nested KWin, private session bus) with private
//! system-style mocks: capabilities, initial snapshot, WebKit-delivered events, weather over a
//! local HTTP fixture, battery alerts, daemon loss/recovery without restarting the app, and
//! the guarantee that a test instance without an explicit bus never touches a real one.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "helpers/gui_stack.rs"]
#[allow(dead_code)]
mod gui_stack;
#[path = "helpers/phase3_host.rs"]
mod host;
#[path = "helpers/http_fixture.rs"]
mod http_fixture;
#[path = "helpers/private_bus.rs"]
mod private_bus;
use common::*;
use gui_stack::Stack;
use host::{eval, until, Owned};
use http_fixture::{Fixture, Mode};
use private_bus::{Mock, PrivateBus};
use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};

const DISPLAY: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const DEVICE: &str = "org.freedesktop.UPower.Device";
const HEADSET: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";

fn launch(s: &Stack, system_bus: Option<&PrivateBus>, fixture: Option<&Fixture>) -> Owned {
    host::prepare(s);
    let log = fs::File::create(s.root.join("app.log")).unwrap();
    let mut command = host::command(s, &s.root.join("inst"));
    if let Some(bus) = system_bus {
        command.env("DBUS_SYSTEM_BUS_ADDRESS", bus.address());
    }
    if let Some(fixture) = fixture {
        command
            .env("HERMES_ISLAND_WEATHER_FORECAST_BASE", fixture.base())
            .env("HERMES_ISLAND_WEATHER_GEOCODE_BASE", fixture.base())
            .env("HERMES_ISLAND_WEATHER_PERIOD_SECS", "3600");
    }
    let app = Owned(
        command
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    wait_for("phase6a private control", Duration::from_secs(30), || {
        s.root.join("inst/test-control.sock").exists().then_some(())
    });
    until(
        s,
        "phase6a legacy ready",
        20,
        "!!document.querySelector('[data-state]')",
    );
    app
}

fn caps(s: &Stack) -> Value {
    eval(
        s,
        "window.__TAURI_INTERNALS__.invoke('island_bootstrap').then(b=>b.capabilities)",
    )
}
fn bootstrap(s: &Stack) -> Value {
    eval(s, "window.__TAURI_INTERNALS__.invoke('island_bootstrap')")
}
fn invoke(s: &Stack, name: &str, args: Value) -> Value {
    eval(
        s,
        &format!("window.__TAURI_INTERNALS__.invoke({},{args})", json!(name)),
    )
}
fn patch(s: &Stack, value: Value) {
    eval(s, &format!("window.island.setSettings({value})"));
}
fn listen(s: &Stack) {
    eval(
        s,
        "(async()=>{window.__ev={power:[],weather:[],bt:[],status:[]};const on=(event,key)=>window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event,target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(e=>window.__ev[key].push(e.payload))});await on('power-changed','power');await on('weather-changed','weather');await on('bluetooth-changed','bt');await on('service-status','status');return true})()",
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
fn mock(slot: &mut Option<Mock>) -> &mut Mock {
    slot.as_mut().unwrap()
}

#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn services_report_capabilities_snapshots_events_alerts_and_recover_in_the_real_app() {
    let _guard = lock();
    let bus = PrivateBus::start("p6a");
    let mut upower = Some(Mock::start(&bus.env(), &bus.root, "upower"));
    let mut bluez = Some(Mock::start(&bus.env(), &bus.root, "bluez"));
    let fixture = Fixture::start(Mode::Ok {
        temp: 20.0,
        code: 2,
    });
    let s = Stack::new("p6a");
    let app = launch(&s, Some(&bus), Some(&fixture));
    let pid = app.0.id();

    // Capabilities + initial snapshots, weather disabled with NO request.
    wait_for(
        "power and bluetooth available",
        Duration::from_secs(10),
        || {
            let c = caps(&s);
            (c["powerStatus"]["available"] == true && c["bluetoothStatus"]["available"] == true)
                .then_some(())
        },
    );
    let b = bootstrap(&s);
    assert_eq!(b["capabilities"]["power"], true);
    assert_eq!(b["capabilities"]["bluetooth"], true);
    assert_eq!(b["capabilities"]["weather"], false);
    assert_eq!(b["capabilities"]["weatherStatus"]["detail"], "disabled");
    assert_eq!(b["power"]["percent"], 55.0);
    assert_eq!(b["power"]["state"], "discharging");
    assert_eq!(b["bluetooth"].as_array().unwrap().len(), 2);
    assert_eq!(b["bluetooth"][0]["name"], "Mock Headset");
    assert!(b["bluetoothRev"].as_u64().unwrap() >= 1);
    assert!(b["weather"]["tempC"].is_null() && b["weather"]["isDay"].is_null());
    assert_eq!(fixture.requests().len(), 0, "weather disabled: no request");

    // Property changes reach the webview as typed events.
    listen(&s);
    mock(&mut upower).send(
        json!({"op":"props","path":DISPLAY,"iface":DEVICE,"changed":{"Percentage":["d",19.0]}}),
    );
    until(
        &s,
        "PowerChanged event in WebKit",
        8,
        "window.__ev.power.some(p=>p.percent===19&&Number.isInteger(p.rev))",
    );
    mock(&mut bluez).send(json!({"op":"props","path":HEADSET,"iface":"org.bluez.Battery1","changed":{"Percentage":["y",65]}}));
    until(
        &s,
        "BluetoothChanged event in WebKit",
        8,
        "window.__ev.bt.some(e=>e.devices.some(d=>d.batteryPercent===65)&&Number.isInteger(e.rev))",
    );

    // Battery alerts become system alerts in the notification store (Low, then Critical).
    wait_for("Low alert stored", Duration::from_secs(8), || {
        let list = invoke(&s, "notifications_list", json!({}));
        list.as_array()
            .unwrap()
            .iter()
            .any(|n| n["source"] == "alert" && n["title"] == "Battery low")
            .then_some(())
    });
    mock(&mut upower).send(
        json!({"op":"props","path":DISPLAY,"iface":DEVICE,"changed":{"Percentage":["d",9.0]}}),
    );
    wait_for("Critical alert stored", Duration::from_secs(8), || {
        let list = invoke(&s, "notifications_list", json!({}));
        list.as_array()
            .unwrap()
            .iter()
            .any(|n| n["title"] == "Battery critically low" && n["urgency"] == "critical")
            .then_some(())
    });

    // Weather: location + enable → one fetch, WeatherChanged event, unit without refetch.
    let places = invoke(&s, "weather_geocode", json!({"query":"Berlin"}));
    assert_eq!(places[0]["name"], "Berlin");
    assert_eq!(fixture.count("/v1/forecast"), 0, "search is not a forecast");
    patch(
        &s,
        json!({"weatherLocation":{"name":"Berlin","latitude":52.52,"longitude":13.405,"country":"Germany"},"weatherEnabled":true,"weatherUnit":"fahrenheit"}),
    );
    until(
        &s,
        "WeatherChanged event with a value",
        10,
        "window.__ev.weather.some(w=>w.tempC===20&&w.temp===68&&w.isDay===true)",
    );
    assert_eq!(fixture.count("/v1/forecast"), 1);
    patch(&s, json!({"weatherUnit":"celsius"}));
    until(
        &s,
        "unit change re-rendered through the event",
        8,
        "window.__ev.weather.some(w=>w.unit==='celsius'&&w.temp===20)",
    );
    let refreshed = invoke(&s, "weather_refresh", json!({}));
    assert_eq!(refreshed["tempC"], 20.0);
    assert_eq!(
        fixture.count("/v1/forecast"),
        2,
        "unit change made no request; refresh made one"
    );
    assert_eq!(caps(&s)["weatherStatus"]["available"], true);

    // Daemon loss → available:false with a reason; restart → recovery. Same app process.
    upower.take().unwrap().kill();
    wait_for("power name-lost", Duration::from_secs(8), || {
        let c = caps(&s);
        (c["powerStatus"]["available"] == false && c["powerStatus"]["detail"] == "name-lost")
            .then_some(())
    });
    assert_eq!(bootstrap(&s)["power"]["available"], false);
    assert!(bootstrap(&s)["power"]["percent"].is_null());
    until(
        &s,
        "PowerChanged available:false event",
        8,
        "window.__ev.power.some(p=>p.available===false)",
    );
    until(
        &s,
        "ServiceStatus event names the reason",
        8,
        "window.__ev.status.some(x=>x.service==='power'&&x.detail==='name-lost')",
    );
    bluez.take().unwrap().kill();
    wait_for("bluetooth name-lost", Duration::from_secs(8), || {
        let c = caps(&s);
        (c["bluetoothStatus"]["available"] == false
            && c["bluetoothStatus"]["detail"] == "name-lost")
            .then_some(())
    });
    assert!(bootstrap(&s)["bluetooth"].as_array().unwrap().is_empty());
    upower = Some(Mock::start(&bus.env(), &bus.root, "upower"));
    bluez = Some(Mock::start(&bus.env(), &bus.root, "bluez"));
    wait_for("both services recover", Duration::from_secs(12), || {
        let c = caps(&s);
        (c["powerStatus"]["available"] == true
            && c["bluetoothStatus"]["available"] == true
            && c["powerStatus"]["connects"].as_u64() >= Some(2))
        .then_some(())
    });
    assert_eq!(bootstrap(&s)["power"]["percent"], 55.0);
    assert_eq!(app.0.id(), pid);
    drop((upower, bluez));
    drop(fixture);
    finish(s, app);
    drop(bus);
}

#[test]
#[ignore = "requires a private KWin/Xwayland compositor and WebKit; see README Testing"]
fn a_test_instance_without_an_explicit_system_bus_never_connects_to_any_bus() {
    let _guard = lock();
    let s = Stack::new("p6a-nobus");
    let app = launch(&s, None, None);
    wait_for("explicit reasons", Duration::from_secs(10), || {
        let c = caps(&s);
        (c["powerStatus"]["detail"] == "no-system-bus-configured"
            && c["bluetoothStatus"]["detail"] == "no-system-bus-configured")
            .then_some(())
    });
    let c = caps(&s);
    assert_eq!(c["power"], false);
    assert_eq!(c["bluetooth"], false);
    assert_eq!(c["powerStatus"]["connects"], 0);
    assert_eq!(c["bluetoothStatus"]["connects"], 0);
    let b = bootstrap(&s);
    assert_eq!(b["power"]["available"], false);
    assert!(b["power"]["percent"].is_null());
    assert_eq!(b["bluetooth"], json!([]));
    // Weather defaults: disabled, no location, nothing fabricated.
    assert_eq!(c["weatherStatus"]["detail"], "disabled");
    assert!(b["weather"]["updatedAt"].is_null());
    finish(s, app);
}
