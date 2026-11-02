//! Phase 6a: the weather service against a LOCAL HTTP fixture with an injected base URL.
//! No test here contacts Open-Meteo; the single real call is `real_open_meteo_smoke`, which is
//! `#[ignore]`d and never part of the gate.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "helpers/http_fixture.rs"]
mod http_fixture;
#[path = "helpers/private_bus.rs"]
mod private_bus;

use hermes_island_lib::{
    contract::{
        settings::TempUnit,
        system::{GeoPlace, WeatherState},
    },
    error::ErrorCode,
    integrations::weather::{WeatherConfig, WeatherService, WeatherSettings},
};
use http_fixture::{Fixture, Mode};
use private_bus::wait_until;
use std::{sync::Arc, thread::sleep, time::Duration};

const T: Duration = Duration::from_secs(10);

fn berlin() -> GeoPlace {
    GeoPlace {
        name: "Berlin".into(),
        latitude: 52.52,
        longitude: 13.405,
        country: "Germany".into(),
    }
}

fn paris() -> GeoPlace {
    GeoPlace {
        name: "Paris".into(),
        latitude: 48.8566,
        longitude: 2.3522,
        country: "France".into(),
    }
}

fn settings(enabled: bool, location: Option<GeoPlace>, unit: TempUnit) -> WeatherSettings {
    WeatherSettings {
        enabled,
        location,
        unit,
    }
}

fn service(fixture: &Fixture, period: u64, s: WeatherSettings) -> Arc<WeatherService> {
    WeatherService::new(
        WeatherConfig {
            forecast_base: fixture.base(),
            geocode_base: fixture.base(),
            period_secs: period,
        },
        s,
    )
    .unwrap()
}

fn refresh(svc: &WeatherService) -> WeatherState {
    tauri::async_runtime::block_on(svc.refresh()).unwrap()
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64
}

#[test]
fn weather_without_a_location_makes_no_request_whatever_else_happens() {
    let fixture = Fixture::start(Mode::Ok {
        temp: 20.0,
        code: 2,
    });
    for (enabled, location) in [(false, None), (true, None), (false, Some(berlin()))] {
        let svc = service(
            &fixture,
            900,
            settings(enabled, location, TempUnit::Celsius),
        );
        svc.start();
        sleep(Duration::from_millis(400));
        let state = refresh(&svc);
        assert!(!state.available && !state.stale);
        assert_eq!(
            (
                state.temp_c,
                state.temp,
                state.code,
                state.is_day,
                state.updated_at
            ),
            (None, None, None, None, None)
        );
        let status = svc.status();
        assert!(!status.available);
        assert_eq!(
            status.detail,
            if enabled { "no-location" } else { "disabled" }
        );
        svc.shutdown();
    }
    assert_eq!(
        fixture.requests(),
        Vec::<String>::new(),
        "the fixture saw no request of any kind"
    );
}

#[test]
fn weather_fetches_once_when_a_location_is_set_and_unit_changes_need_no_request() {
    let fixture = Fixture::start(Mode::Ok {
        temp: 23.5,
        code: 2,
    });
    let svc = service(&fixture, 900, settings(false, None, TempUnit::Fahrenheit));
    svc.start();
    sleep(Duration::from_millis(300));
    assert_eq!(fixture.count("/v1/forecast"), 0);
    let mut topic = svc.topic.subscribe();
    topic.borrow_and_update();
    svc.configure(settings(true, Some(berlin()), TempUnit::Fahrenheit));
    wait_until("worker fetches after the location is set", T, || {
        svc.current().available.then_some(())
    });
    let state = svc.current();
    assert_eq!(state.temp_c, Some(23.5));
    assert!(
        (state.temp.unwrap() - 74.3).abs() < 1e-9,
        "{:?}",
        state.temp
    );
    assert_eq!(state.unit, TempUnit::Fahrenheit);
    assert_eq!((state.code, state.is_day), (Some(2), Some(true)));
    assert_eq!(state.icon.as_deref(), Some("partly-cloudy-day"));
    assert_eq!(state.place.as_ref().unwrap().name, "Berlin");
    assert!(!state.stale);
    assert!((now_ms() - state.updated_at.unwrap()).abs() < 15_000.0);
    assert!(topic.has_changed().unwrap(), "WeatherChanged topic fired");
    assert_eq!(fixture.count("/v1/forecast"), 1);
    let url = &fixture.requests()[0];
    assert!(url.contains("latitude=52.52&longitude=13.405"), "{url}");
    assert!(
        url.contains("temperature_unit=celsius"),
        "stored as Celsius: {url}"
    );
    assert_eq!(svc.status().detail, "open-meteo");
    // Unit selection re-renders; it never refetches.
    let rev = svc.current().rev;
    svc.configure(settings(true, Some(berlin()), TempUnit::Celsius));
    let celsius = svc.current();
    assert_eq!(
        (celsius.temp, celsius.unit),
        (Some(23.5), TempUnit::Celsius)
    );
    assert!(celsius.rev > rev);
    sleep(Duration::from_millis(300));
    assert_eq!(
        fixture.count("/v1/forecast"),
        1,
        "unit change made a request"
    );
    svc.shutdown();
}

#[test]
fn weather_failures_keep_the_last_good_value_stale_and_never_fabricate() {
    let fixture = Fixture::start(Mode::Ok {
        temp: 12.0,
        code: 61,
    });
    let svc = service(
        &fixture,
        900,
        settings(true, Some(berlin()), TempUnit::Celsius),
    );
    let good = refresh(&svc);
    assert!(good.available && !good.stale);
    assert_eq!(good.temp_c, Some(12.0));
    for (mode, detail) in [
        (Mode::Status(500), "http-500"),
        (Mode::Malformed, "invalid-response"),
        (Mode::Oversize, "response-too-large"),
        (Mode::Implausible, "invalid-response"),
    ] {
        fixture.set_mode(mode);
        let state = refresh(&svc);
        assert!(state.available && state.stale, "{detail}: {state:?}");
        assert_eq!((state.temp_c, state.code), (Some(12.0), Some(61)));
        assert_eq!(
            state.updated_at, good.updated_at,
            "stale keeps the old time"
        );
        let status = svc.status();
        assert!(!status.available);
        assert_eq!(status.detail, detail);
    }
    fixture.set_mode(Mode::Ok {
        temp: 13.0,
        code: 61,
    });
    let healed = refresh(&svc);
    assert!(healed.available && !healed.stale);
    assert_eq!(healed.temp_c, Some(13.0));
    assert!(svc.status().available);

    // With nothing good yet, every value is null and the service is simply unavailable.
    let fresh = Fixture::start(Mode::Malformed);
    let cold = service(
        &fresh,
        900,
        settings(true, Some(berlin()), TempUnit::Fahrenheit),
    );
    let state = refresh(&cold);
    assert!(!state.available && !state.stale);
    assert_eq!(
        (
            state.temp_c,
            state.temp,
            state.code,
            state.icon,
            state.is_day,
            state.updated_at
        ),
        (None, None, None, None, None, None)
    );
}

#[test]
fn weather_geocode_is_user_initiated_bounded_and_independent_of_enablement() {
    let fixture = Fixture::start(Mode::Ok { temp: 1.0, code: 0 });
    let svc = service(&fixture, 900, settings(false, None, TempUnit::Celsius));
    let places = tauri::async_runtime::block_on(svc.geocode("Berlin")).unwrap();
    assert_eq!(places.len(), 2);
    assert_eq!(places[0].name, "Berlin");
    assert_eq!(places[0].country, "Deutschland");
    assert_eq!(
        places[1].country, "",
        "an absent country is empty, not invented"
    );
    assert_eq!(
        fixture.count("/v1/search?name=Berlin&count=10&language=en"),
        1
    );
    assert_eq!(fixture.count("/v1/forecast"), 0);
    for bad in ["", " x ", &"y".repeat(101)] {
        let error = tauri::async_runtime::block_on(svc.geocode(bad)).unwrap_err();
        assert_eq!(error.code, ErrorCode::Invalid, "{bad:?}");
    }
    assert_eq!(
        fixture.requests().len(),
        1,
        "rejected queries made no request"
    );
    fixture.set_mode(Mode::Status(503));
    let error = tauri::async_runtime::block_on(svc.geocode("Berlin")).unwrap_err();
    assert_eq!(error.code, ErrorCode::Unavailable);
    // A place whose name needs escaping is encoded, never passed through raw.
    fixture.set_mode(Mode::Ok { temp: 1.0, code: 0 });
    tauri::async_runtime::block_on(svc.geocode("São Paulo&x=1")).unwrap();
    assert!(
        fixture
            .requests()
            .last()
            .unwrap()
            .starts_with("/v1/search?name=S%C3%A3o%20Paulo%26x%3D1&"),
        "{:?}",
        fixture.requests()
    );
}

#[test]
fn weather_location_change_discards_the_old_place_and_a_superseded_response() {
    let fixture = Fixture::start(Mode::Slow {
        ms: 700,
        temp: 30.0,
    });
    let svc = service(
        &fixture,
        900,
        settings(true, Some(berlin()), TempUnit::Celsius),
    );
    svc.start();
    wait_until("slow request for Berlin arrived", T, || {
        (fixture.count("/v1/forecast") == 1).then_some(())
    });
    // The place changes while the Berlin answer is still in flight.
    fixture.set_mode(Mode::Ok { temp: 5.0, code: 3 });
    svc.configure(settings(true, Some(paris()), TempUnit::Celsius));
    wait_until("Paris is shown", T, || {
        let s = svc.current();
        (s.place.as_ref().is_some_and(|p| p.name == "Paris") && s.temp_c == Some(5.0)).then_some(())
    });
    sleep(Duration::from_millis(900));
    let state = svc.current();
    assert_eq!(state.place.unwrap().name, "Paris");
    assert_eq!(
        state.temp_c,
        Some(5.0),
        "the late Berlin answer (30 C) never replaced Paris"
    );
    assert!(fixture
        .requests()
        .iter()
        .any(|r| r.contains("latitude=48.8566&longitude=2.3522")));
    // Removing the location clears the old place's data and stops all requests.
    let requests = fixture.requests().len();
    svc.configure(settings(true, None, TempUnit::Celsius));
    let cleared = svc.current();
    assert!(!cleared.available && cleared.temp_c.is_none() && cleared.place.is_none());
    sleep(Duration::from_millis(500));
    assert_eq!(fixture.requests().len(), requests);
    svc.shutdown();
}

#[test]
fn weather_refreshes_on_the_wall_clock_period_and_stops_on_shutdown() {
    let fixture = Fixture::start(Mode::Ok { temp: 9.0, code: 0 });
    let svc = service(
        &fixture,
        2,
        settings(true, Some(berlin()), TempUnit::Celsius),
    );
    svc.start();
    wait_until(
        "three scheduled fetches within the period",
        Duration::from_secs(12),
        || (fixture.count("/v1/forecast") >= 3).then_some(()),
    );
    svc.shutdown();
    let stopped = fixture.count("/v1/forecast");
    sleep(Duration::from_millis(2600));
    assert_eq!(
        fixture.count("/v1/forecast"),
        stopped,
        "no fetch after shutdown"
    );
}

#[test]
#[ignore = "contacts the real Open-Meteo; run by hand only"]
fn real_open_meteo_smoke() {
    let svc = WeatherService::new(
        WeatherConfig::default(),
        settings(true, Some(berlin()), TempUnit::Celsius),
    )
    .unwrap();
    let state = refresh(&svc);
    assert!(state.available && state.temp_c.is_some(), "{state:?}");
}

#[test]
fn weather_late_first_place_answer_is_never_shown_when_the_second_place_fails() {
    // Berlin's request is slow; the place changes to Paris, whose request FAILS. The late
    // Berlin answer must be discarded: if it were accepted, Berlin (or a stale copy of it, via
    // Paris's failure path) would be shown for the wrong place.
    let fixture = Fixture::start(Mode::Slow {
        ms: 700,
        temp: 30.0,
    });
    let svc = service(
        &fixture,
        900,
        settings(true, Some(berlin()), TempUnit::Celsius),
    );
    svc.start();
    wait_until("slow request for Berlin arrived", T, || {
        (fixture.count("/v1/forecast") == 1).then_some(())
    });
    fixture.set_mode(Mode::Status(500));
    let mut topic = svc.topic.subscribe();
    topic.borrow_and_update();
    svc.configure(settings(true, Some(paris()), TempUnit::Celsius));
    let mut seen = Vec::new();
    wait_until("Paris request failed", T, || {
        seen.push(svc.current());
        (fixture.count("/v1/forecast") == 2 && svc.status().detail == "http-500").then_some(())
    });
    sleep(Duration::from_millis(1000));
    seen.push(svc.current());
    assert!(
        seen.iter()
            .all(|s| s.temp_c != Some(30.0) && !s.available && s.place.is_none()),
        "the superseded Berlin answer was shown: {seen:?}"
    );
    assert!(
        !topic.has_changed().unwrap() || topic.borrow().as_ref().is_none_or(|s| s.temp_c.is_none()),
        "the superseded Berlin answer was emitted"
    );
    let state = svc.current();
    assert!(!state.available && !state.stale);
    assert_eq!(
        (state.temp_c, state.code, state.updated_at),
        (None, None, None)
    );
    assert_eq!(svc.status().detail, "http-500");
    svc.shutdown();
}

#[test]
fn weather_enabled_without_a_location_reports_no_location_not_fetching() {
    let fixture = Fixture::start(Mode::Ok { temp: 3.0, code: 0 });
    // Reaching "enabled, no location" through configure() (the settings-change path), both from
    // a disabled service and from one that had a location.
    let svc = service(&fixture, 900, settings(false, None, TempUnit::Celsius));
    svc.start();
    svc.configure(settings(true, None, TempUnit::Celsius));
    let status = svc.status();
    assert!(
        !status.available && status.detail == "no-location",
        "{status:?}"
    );
    svc.configure(settings(true, Some(berlin()), TempUnit::Celsius));
    wait_until("fetched", T, || svc.current().available.then_some(()));
    sleep(Duration::from_millis(300));
    let requests = fixture.count("/v1/forecast");
    svc.configure(settings(true, None, TempUnit::Celsius));
    let status = svc.status();
    assert!(
        !status.available && status.detail == "no-location",
        "{status:?}"
    );
    svc.configure(settings(false, None, TempUnit::Celsius));
    assert_eq!(svc.status().detail, "disabled");
    sleep(Duration::from_millis(300));
    assert_eq!(fixture.count("/v1/forecast"), requests);
    svc.shutdown();
}
