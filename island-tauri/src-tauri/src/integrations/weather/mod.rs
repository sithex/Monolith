//! Weather service (SPEC §3.5 "Weather", §5 `weather_refresh` / `weather_geocode`).
//!
//! `policy.rs` is the audited pure core ported unchanged from the former `staging/weather-core` crate (git history)
//! (URLs, parsing, WMO conditions, stale/refresh rules); `open_meteo.rs` moves bytes with a
//! 10 s timeout and a 256 KiB body cap. This module owns the settings gate and the worker.
//!
//! * Disabled until the user sets a location AND enables weather: no request is ever made
//!   otherwise, and there is no IP geolocation.
//! * The service stores Celsius; `weatherUnit` only selects the displayed `temp`.
//! * A failed refresh keeps the last good value with `stale: true`; with none, every value is
//!   `null` and `available` is false.
//! * Refresh cadence: every `period` seconds aligned to the wall clock (UTC quarter-hours by
//!   default), re-checked at most every 30 s so a suspend/resume catches up promptly.

pub mod open_meteo;
pub mod policy;

use crate::{
    bus::Latest,
    contract::{
        settings::{Settings, TempUnit},
        system::{GeoPlace, ServiceStatus, WeatherState},
    },
    error::IslandError,
    integrations::service_status::Reporter,
};
use open_meteo::FetchError;
use policy::WeatherError;
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{watch, Notify};

/// Endpoints and cadence. Production uses the real Open-Meteo hosts; a test instance may
/// inject a local fixture through the environment (ignored outside test instances).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeatherConfig {
    pub forecast_base: String,
    pub geocode_base: String,
    pub period_secs: u64,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            forecast_base: open_meteo::FORECAST_BASE.into(),
            geocode_base: open_meteo::GEOCODE_BASE.into(),
            period_secs: 900,
        }
    }
}

pub const ENV_FORECAST_BASE: &str = "HERMES_ISLAND_WEATHER_FORECAST_BASE";
pub const ENV_GEOCODE_BASE: &str = "HERMES_ISLAND_WEATHER_GEOCODE_BASE";
pub const ENV_PERIOD_SECS: &str = "HERMES_ISLAND_WEATHER_PERIOD_SECS";

impl WeatherConfig {
    /// Environment overrides apply to test instances only; a real run always talks to
    /// Open-Meteo, so a stray variable can never redirect the user's location.
    pub fn for_instance(test_instance: bool, env: impl Fn(&str) -> Option<String>) -> Self {
        let mut config = Self::default();
        if test_instance {
            if let Some(base) = env(ENV_FORECAST_BASE) {
                config.forecast_base = base;
            }
            if let Some(base) = env(ENV_GEOCODE_BASE) {
                config.geocode_base = base;
            }
            if let Some(period) = env(ENV_PERIOD_SECS).and_then(|v| v.parse::<u64>().ok()) {
                config.period_secs = period.max(1);
            }
        }
        config
    }
}

/// The weather-relevant slice of `Settings`.
#[derive(Clone, Debug, PartialEq)]
pub struct WeatherSettings {
    pub enabled: bool,
    pub location: Option<GeoPlace>,
    pub unit: TempUnit,
}

impl From<&Settings> for WeatherSettings {
    fn from(s: &Settings) -> Self {
        Self {
            enabled: s.weather_enabled,
            location: s.weather_location.clone(),
            unit: s.weather_unit,
        }
    }
}

impl WeatherSettings {
    fn active(&self) -> bool {
        self.enabled && self.location.is_some()
    }
}

struct Shared {
    settings: WeatherSettings,
    /// Bumped whenever the enabled/location pair changes; a response for an older value
    /// is discarded instead of being shown for the wrong place.
    generation: u64,
    last_good: Option<policy::WeatherState>,
    current: WeatherState,
}

pub struct WeatherService {
    game_pause: crate::integrations::plasma::game_effects::PollPause,
    pub status: Arc<Reporter>,
    pub topic: Latest<WeatherState>,
    config: WeatherConfig,
    client: reqwest::Client,
    shared: Mutex<Shared>,
    gate: tokio::sync::Mutex<()>,
    kick: Notify,
    stop: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Next wall-clock boundary strictly after `now` for a `period` in seconds.
pub fn next_boundary(now: u64, period: u64) -> u64 {
    let period = period.max(1);
    now.saturating_add(period - now % period)
}

fn core_place(place: &GeoPlace) -> policy::GeoPlace {
    policy::GeoPlace {
        id: 0,
        name: place.name.clone(),
        latitude: place.latitude,
        longitude: place.longitude,
        country: (!place.country.is_empty()).then(|| place.country.clone()),
        country_code: None,
        admin1: None,
        timezone: None,
    }
}

fn wire_place(place: &policy::GeoPlace) -> GeoPlace {
    GeoPlace {
        name: place.name.clone(),
        latitude: place.latitude,
        longitude: place.longitude,
        country: place
            .country
            .clone()
            .or_else(|| place.country_code.clone())
            .unwrap_or_default(),
    }
}

fn display_temp(celsius: f64, unit: TempUnit) -> f64 {
    match unit {
        TempUnit::Celsius => celsius,
        TempUnit::Fahrenheit => celsius * 9.0 / 5.0 + 32.0,
    }
}

/// Core state (Celsius, Unix seconds) → wire state in the user's unit; `rev` is stamped later.
fn render(core: &policy::WeatherState, unit: TempUnit) -> WeatherState {
    WeatherState {
        rev: 0,
        available: core.available,
        stale: core.stale,
        temp_c: core.temp_c,
        temp: core.temp_c.map(|c| display_temp(c, unit)),
        unit,
        code: core.code,
        icon: match (core.code.and_then(policy::wmo_condition), core.is_day) {
            (Some(condition), Some(day)) => Some(condition.icon_key(day).to_owned()),
            _ => None,
        },
        is_day: core.is_day,
        place: core.place.as_ref().map(wire_place),
        updated_at: core.updated_at.map(|s| s as f64 * 1000.0),
    }
}

fn error_detail(error: &WeatherError) -> &'static str {
    match error {
        WeatherError::Oversize => "response-too-large",
        WeatherError::ApiError(_) => "api-error",
        WeatherError::Json | WeatherError::MissingField(_) | WeatherError::Invalid(_) => {
            "invalid-response"
        }
    }
}

impl WeatherService {
    pub fn new(config: WeatherConfig, settings: WeatherSettings) -> Result<Arc<Self>, IslandError> {
        Self::new_gated(config, settings, Default::default())
    }
    pub fn new_gated(
        config: WeatherConfig,
        settings: WeatherSettings,
        game_pause: crate::integrations::plasma::game_effects::PollPause,
    ) -> Result<Arc<Self>, IslandError> {
        let client = open_meteo::client()
            .map_err(|_| IslandError::internal("Weather HTTP client unavailable"))?;
        let current = render(&policy::WeatherState::default(), settings.unit);
        let topic = Latest::new();
        topic.publish(current.clone());
        let service = Arc::new(Self {
            game_pause,
            status: Reporter::new("weather"),
            topic,
            config,
            client,
            shared: Mutex::new(Shared {
                settings,
                generation: 0,
                last_good: None,
                current,
            }),
            gate: tokio::sync::Mutex::new(()),
            kick: Notify::new(),
            stop: watch::channel(false).0,
            task: Mutex::new(None),
        });
        service.report_inactive();
        Ok(service)
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        match self.shared.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn current(&self) -> WeatherState {
        self.lock().current.clone()
    }

    pub fn status(&self) -> ServiceStatus {
        self.status.get()
    }

    pub fn config(&self) -> &WeatherConfig {
        &self.config
    }

    fn report_inactive(&self) {
        let shared = self.lock();
        if !shared.settings.enabled {
            self.status.unavailable("disabled");
        } else if shared.settings.location.is_none() {
            self.status.unavailable("no-location");
        }
    }

    fn publish(&self, shared: &mut Shared, core: &policy::WeatherState) {
        let mut next = render(core, shared.settings.unit);
        let unchanged = WeatherState {
            rev: shared.current.rev,
            ..next.clone()
        } == shared.current;
        if !unchanged {
            next.rev = shared.current.rev.wrapping_add(1);
            shared.current = next.clone();
            self.topic.publish(next);
        }
    }

    /// Apply new settings. A unit change re-renders the last good value without any request;
    /// an enabled/location change discards the old place's data and wakes the worker.
    pub fn configure(&self, settings: WeatherSettings) {
        let mut shared = self.lock();
        if shared.settings == settings {
            return;
        }
        let place_changed = shared.settings.enabled != settings.enabled
            || shared.settings.location != settings.location;
        shared.settings = settings;
        if place_changed {
            shared.generation = shared.generation.wrapping_add(1);
            shared.last_good = None;
        }
        let core = shared.last_good.clone().unwrap_or_default();
        self.publish(&mut shared, &core);
        let active = shared.settings.active();
        drop(shared);
        if place_changed {
            if active {
                self.status.unavailable("fetching");
            } else {
                self.report_inactive();
            }
            self.kick.notify_one();
        }
    }

    /// One fetch under the single-flight gate. Does nothing (and makes NO request) unless
    /// weather is enabled with a location.
    async fn fetch_now(&self) {
        let _gate = self.gate.lock().await;
        let (generation, place) = {
            let shared = self.lock();
            match (&shared.settings.enabled, &shared.settings.location) {
                (true, Some(place)) => (shared.generation, core_place(place)),
                _ => return,
            }
        };
        let outcome = match policy::forecast_url(&self.config.forecast_base, &place) {
            Err(_) => Err("invalid-base".to_owned()),
            Ok(url) => match open_meteo::get(&self.client, &url).await {
                Err(error) => Err(error.detail()),
                Ok(body) => policy::parse_forecast(&body, &place, unix_now())
                    .map_err(|e| error_detail(&e).to_owned()),
            },
        };
        let mut shared = self.lock();
        if shared.generation != generation {
            return; // superseded by a settings change; the worker refetches
        }
        match outcome {
            Ok(state) => {
                shared.last_good = Some(state.clone());
                self.publish(&mut shared, &state);
                drop(shared);
                self.status.available("open-meteo");
                self.status.connected();
            }
            Err(detail) => {
                let stale = policy::on_failure(shared.last_good.as_ref());
                self.publish(&mut shared, &stale);
                drop(shared);
                self.status.unavailable(&detail);
            }
        }
    }

    /// `weather_refresh`: fetch now when active, then return the current state.
    pub async fn refresh(&self) -> Result<WeatherState, IslandError> {
        self.fetch_now().await;
        Ok(self.current())
    }

    /// `weather_geocode`: user-initiated place search (works before weather is enabled, since
    /// choosing a location is how it gets enabled). Query limits come from the core.
    pub async fn geocode(&self, query: &str) -> Result<Vec<GeoPlace>, IslandError> {
        let url = policy::geocode_url(&self.config.geocode_base, query, 10, "en")
            .map_err(|_| IslandError::invalid("Search needs 2 to 100 characters"))?;
        let body = open_meteo::get(&self.client, &url)
            .await
            .map_err(|e| match e {
                FetchError::Timeout => IslandError::timeout("Place search"),
                _ => IslandError::unavailable("Place search is unavailable"),
            })?;
        let places = policy::parse_geocode(&body)
            .map_err(|_| IslandError::unavailable("Place search returned an invalid answer"))?;
        Ok(places
            .iter()
            .map(wire_place)
            .filter(|p| p.validate().is_ok())
            .collect())
    }

    pub fn start(self: &Arc<Self>) {
        let service = Arc::clone(self);
        let stopped = self.stop.subscribe();
        let task = tauri::async_runtime::spawn(service.worker(stopped));
        if let Ok(mut slot) = self.task.lock() {
            *slot = Some(task);
        }
    }

    async fn worker(self: Arc<Self>, mut stop: watch::Receiver<bool>) {
        let mut pause = self.game_pause.subscribe();
        loop {
            if *stop.borrow() {
                return;
            }
            let paused = *pause.borrow_and_update();
            if paused {
                tokio::select! { _ = stop.changed() => return, _ = pause.changed() => {} }
                continue;
            }
            let active = self.lock().settings.active();
            if !active {
                self.report_inactive();
                tokio::select! {
                    _ = stop.changed() => return,
                    _ = pause.changed() => {},
                    () = self.kick.notified() => {}
                }
                continue;
            }
            tokio::select! {
                _ = stop.changed() => return,
                _ = pause.changed() => continue,
                () = self.fetch_now() => {}
            }
            let target = next_boundary(unix_now(), self.config.period_secs);
            loop {
                let remaining = target.saturating_sub(unix_now());
                if remaining == 0 {
                    break;
                }
                tokio::select! {
                    _ = stop.changed() => return,
                    _ = pause.changed() => break,
                    () = self.kick.notified() => break,
                    () = tokio::time::sleep(Duration::from_secs(remaining.min(30))) => {}
                }
            }
        }
    }

    pub fn shutdown(&self) {
        self.stop.send_replace(true);
        let task = self.task.lock().ok().and_then(|mut slot| slot.take());
        if let Some(task) = task {
            let _ = tauri::async_runtime::block_on(async {
                tokio::time::timeout(Duration::from_secs(3), task).await
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(c: f64) -> policy::WeatherState {
        policy::WeatherState {
            available: true,
            stale: false,
            temp_c: Some(c),
            code: Some(2),
            is_day: Some(true),
            place: None,
            updated_at: Some(1_000),
        }
    }

    #[test]
    fn unit_selects_the_displayed_temperature_only() {
        let f = render(&core(20.0), TempUnit::Fahrenheit);
        assert_eq!((f.temp_c, f.temp), (Some(20.0), Some(68.0)));
        let c = render(&core(20.0), TempUnit::Celsius);
        assert_eq!((c.temp_c, c.temp), (Some(20.0), Some(20.0)));
        assert_eq!(render(&core(-40.0), TempUnit::Fahrenheit).temp, Some(-40.0));
    }

    #[test]
    fn unknown_values_stay_null() {
        let s = render(&policy::WeatherState::default(), TempUnit::Celsius);
        assert!(!s.available && !s.stale);
        assert_eq!(
            (s.temp_c, s.temp, s.code, s.icon, s.is_day, s.updated_at),
            (None, None, None, None, None, None)
        );
        // An undocumented-but-valid code parses with no icon; night/day unknown gives none.
        let mut odd = core(1.0);
        odd.code = Some(98);
        assert_eq!(render(&odd, TempUnit::Celsius).icon, None);
        let mut unknown_day = core(1.0);
        unknown_day.is_day = None;
        assert_eq!(render(&unknown_day, TempUnit::Celsius).icon, None);
        assert_eq!(
            render(&core(1.0), TempUnit::Celsius).icon.as_deref(),
            Some("partly-cloudy-day")
        );
    }

    #[test]
    fn updated_at_is_milliseconds() {
        assert_eq!(
            render(&core(1.0), TempUnit::Celsius).updated_at,
            Some(1_000_000.0)
        );
    }

    #[test]
    fn boundaries_align_to_the_period_and_match_the_core_quarter_hour() {
        for now in [0, 1, 899, 900, 901, 1_700_000_123] {
            assert_eq!(next_boundary(now, 900), policy::next_refresh_unix(now));
            assert!(next_boundary(now, 900) > now);
        }
        assert_eq!(next_boundary(7, 5), 10);
        assert_eq!(next_boundary(10, 5), 15);
        assert_eq!(next_boundary(3, 0), 4, "period 0 is treated as 1 s");
    }

    #[test]
    fn env_overrides_apply_to_test_instances_only() {
        let env = |name: &str| match name {
            ENV_FORECAST_BASE => Some("http://127.0.0.1:9".to_owned()),
            ENV_PERIOD_SECS => Some("3".to_owned()),
            _ => None,
        };
        let real = WeatherConfig::for_instance(false, env);
        assert_eq!(real, WeatherConfig::default());
        let test = WeatherConfig::for_instance(true, env);
        assert_eq!(test.forecast_base, "http://127.0.0.1:9");
        assert_eq!(test.period_secs, 3);
        assert_eq!(test.geocode_base, open_meteo::GEOCODE_BASE);
    }
}
