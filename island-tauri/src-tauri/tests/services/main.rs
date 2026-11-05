//! Service tests against private D-Bus buses, fake helpers and local fixtures (no display).
//!
//! One test binary instead of one per file: every integration test binary links the whole
//! app, so 13 separate binaries cost 13 links and gigabytes of target/. Run one
//! file's tests with `cargo test --test services <file_name>::`.
// Shared fixtures, loaded once for every file below.
#[path = "../helpers/calendar_fixture.rs"]
mod calendar_fixture;
#[path = "../helpers/control_fakes.rs"]
mod control_fakes;
#[path = "../helpers/controls_fixture.rs"]
mod controls_fixture;
#[path = "../helpers/extra_controls_fixture.rs"]
mod extra_controls_fixture;
#[path = "../helpers/http_fixture.rs"]
mod http_fixture;
#[path = "../helpers/lrclib_fixture.rs"]
mod lrclib_fixture;
#[path = "../helpers/pipewire_fixture.rs"]
mod pipewire_fixture;
#[path = "../helpers/private_bus.rs"]
mod private_bus;

mod apps6b1_services;
mod bluetooth_service;
mod brightness6b2;
mod carried6b2;
mod equalizer_service;
mod lyrics_service;
mod media_service;
mod mixer6b2;
mod power_service;
mod tiles6b2;
mod toggles6b2;
mod updates6b2;
mod weather_service;
