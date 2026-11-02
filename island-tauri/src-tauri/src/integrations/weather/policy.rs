//! Pure weather policy for Ripple's Phase 6 adapter.
//!
//! No transport, clock reads, IO, threads, or UI conversion live here. URL bases
//! are HTTP(S) origins or path prefixes, not full endpoint URLs. Wire contracts
//! use camelCase; Open-Meteo's snake_case response fields are parsed separately.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;

/// Inclusive response-body limit, checked before JSON decoding.
pub const MAX_BODY_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoPlace {
    pub id: u64,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub admin1: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherState {
    pub available: bool,
    pub stale: bool,
    pub temp_c: Option<f64>,
    pub code: Option<u16>,
    pub is_day: Option<bool>,
    pub place: Option<GeoPlace>,
    /// Successful fetch time supplied by the caller, in Unix seconds (UTC).
    pub updated_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WeatherError {
    Oversize,
    Json,
    ApiError(String),
    MissingField(&'static str),
    Invalid(&'static str),
}

impl fmt::Display for WeatherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Oversize => f.write_str("weather response exceeds 256 KiB"),
            Self::Json => f.write_str("invalid weather JSON"),
            Self::ApiError(reason) => write!(f, "Open-Meteo error: {reason}"),
            Self::MissingField(field) => write!(f, "missing weather field: {field}"),
            Self::Invalid(field) => write!(f, "invalid weather field: {field}"),
        }
    }
}

impl std::error::Error for WeatherError {}

/// Build `/v1/forecast` below the base, always storing Celsius internally.
pub fn forecast_url(base: &str, place: &GeoPlace) -> Result<String, WeatherError> {
    if !valid_coordinates(place.latitude, place.longitude) {
        return Err(WeatherError::Invalid("coordinates"));
    }
    let base = checked_base(base)?;
    Ok(format!(
        "{base}/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code,is_day&temperature_unit=celsius&timezone=auto",
        place.latitude, place.longitude
    ))
}

/// Build `/v1/search`. Query limits count Unicode scalar values after trimming.
/// Language is encoded literally, not normalized or treated as URL syntax.
pub fn geocode_url(
    base: &str,
    query: &str,
    count: u8,
    language: &str,
) -> Result<String, WeatherError> {
    let query = query.trim();
    if !(2..=100).contains(&query.chars().count()) {
        return Err(WeatherError::Invalid("query"));
    }
    if !(1..=10).contains(&count) {
        return Err(WeatherError::Invalid("count"));
    }
    let base = checked_base(base)?;
    Ok(format!(
        "{base}/v1/search?name={}&count={count}&language={}&format=json",
        percent_encode(query),
        percent_encode(language)
    ))
}

fn checked_base(base: &str) -> Result<&str, WeatherError> {
    let base = base.trim_end_matches('/');
    let rest = base
        .strip_prefix("https://")
        .or_else(|| base.strip_prefix("http://"))
        .ok_or(WeatherError::Invalid("base"))?;
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty()
        || base.chars().any(char::is_whitespace)
        || base.chars().any(char::is_control)
        || base.contains(['?', '#', '\\', '@'])
    {
        return Err(WeatherError::Invalid("base"));
    }
    Ok(base)
}

/// RFC 3986 unreserved bytes pass through; everything else is UTF-8 %HH.
fn percent_encode(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::new();
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    output
}

/// Parse only the requested current conditions. Response-local times and UTC
/// offsets are metadata, not fetch timestamps; `now_unix` is authoritative.
pub fn parse_forecast(
    body: &[u8],
    place: &GeoPlace,
    now_unix: u64,
) -> Result<WeatherState, WeatherError> {
    let root = parse_body(body)?;
    let current = required(&root, "current")?
        .as_object()
        .ok_or(WeatherError::Invalid("current"))?;
    let units = required(&root, "current_units")?
        .as_object()
        .ok_or(WeatherError::Invalid("current_units"))?;
    let unit = required(units, "temperature_2m")?;
    if unit.as_str() != Some("°C") {
        return Err(WeatherError::Invalid("current_units.temperature_2m"));
    }
    let temp_c = required(current, "temperature_2m")?
        .as_f64()
        .ok_or(WeatherError::Invalid("temperature_2m"))?;
    if !temp_c.is_finite() || !(-100.0..=70.0).contains(&temp_c) {
        return Err(WeatherError::Invalid("temperature_2m"));
    }
    let code = required(current, "weather_code")?
        .as_u64()
        .and_then(|n| u16::try_from(n).ok())
        .filter(|&n| n <= 99)
        .ok_or(WeatherError::Invalid("weather_code"))?;
    let is_day = match required(current, "is_day")?.as_u64() {
        Some(0) => false,
        Some(1) => true,
        _ => return Err(WeatherError::Invalid("is_day")),
    };
    if !valid_coordinates(place.latitude, place.longitude) {
        return Err(WeatherError::Invalid("coordinates"));
    }
    Ok(WeatherState {
        available: true,
        stale: false,
        temp_c: Some(temp_c),
        code: Some(code),
        is_day: Some(is_day),
        place: Some(place.clone()),
        updated_at: Some(now_unix),
    })
}

/// Invalid individual entries are dropped, including malformed/missing required
/// fields. The first ten valid entries are retained in upstream order.
pub fn parse_geocode(body: &[u8]) -> Result<Vec<GeoPlace>, WeatherError> {
    let root = parse_body(body)?;
    let results = match root.get("results") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(value) => value.as_array().ok_or(WeatherError::Invalid("results"))?,
    };
    Ok(results.iter().filter_map(parse_place).take(10).collect())
}

fn parse_place(value: &Value) -> Option<GeoPlace> {
    let place = GeoPlace {
        id: value.get("id")?.as_u64()?,
        name: value.get("name")?.as_str()?.to_owned(),
        latitude: value.get("latitude")?.as_f64()?,
        longitude: value.get("longitude")?.as_f64()?,
        country: optional_string(value, "country").ok()?,
        country_code: optional_string(value, "country_code").ok()?,
        admin1: optional_string(value, "admin1").ok()?,
        timezone: optional_string(value, "timezone").ok()?,
    };
    if place.name.trim().is_empty() || !valid_coordinates(place.latitude, place.longitude) {
        None
    } else {
        Some(place)
    }
}

fn optional_string(value: &Value, key: &'static str) -> Result<Option<String>, WeatherError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        _ => Err(WeatherError::Invalid(key)),
    }
}

fn valid_coordinates(latitude: f64, longitude: f64) -> bool {
    latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
}

fn parse_body(body: &[u8]) -> Result<Map<String, Value>, WeatherError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(WeatherError::Oversize);
    }
    let value: Value = serde_json::from_slice(body).map_err(|_| WeatherError::Json)?;
    let Value::Object(root) = value else {
        return Err(WeatherError::Invalid("root"));
    };
    if root.get("error").and_then(Value::as_bool) == Some(true) {
        let reason = required(&root, "reason")?
            .as_str()
            .ok_or(WeatherError::Invalid("reason"))?;
        return Err(WeatherError::ApiError(reason.chars().take(200).collect()));
    }
    Ok(root)
}

fn required<'a>(
    object: &'a Map<String, Value>,
    key: &'static str,
) -> Result<&'a Value, WeatherError> {
    object
        .get(key)
        .filter(|value| !value.is_null())
        .ok_or(WeatherError::MissingField(key))
}

/// Keep all last-good fields unchanged, marking the snapshot stale. With no
/// snapshot every optional field is null and both status flags are false.
/// The caller supplies a genuinely last-good snapshot, not an arbitrary state.
pub fn on_failure(last_good: Option<&WeatherState>) -> WeatherState {
    match last_good {
        Some(last) => WeatherState {
            stale: true,
            ..last.clone()
        },
        None => WeatherState::default(),
    }
}

/// Next UTC quarter-hour, strictly after now whenever representable.
/// Saturates at u64::MAX if the next boundary is unrepresentable (including
/// at MAX itself, where no u64 return value can be strictly greater).
pub fn next_refresh_unix(now_unix: u64) -> u64 {
    now_unix.saturating_add(900 - now_unix % 900)
}

/// The explicit Phase 6 WMO contract. Severity remains distinct even when icon
/// keys are shared. Code 97 (heavy thunderstorm) is included per controller
/// decision; any other in-range code (0..=99) parses but maps to no condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Condition {
    ClearSky,
    MainlyClear,
    PartlyCloudy,
    Overcast,
    Fog,
    DepositingRimeFog,
    LightDrizzle,
    ModerateDrizzle,
    DenseDrizzle,
    LightFreezingDrizzle,
    DenseFreezingDrizzle,
    SlightRain,
    ModerateRain,
    HeavyRain,
    LightFreezingRain,
    HeavyFreezingRain,
    SlightSnowfall,
    ModerateSnowfall,
    HeavySnowfall,
    SnowGrains,
    SlightRainShowers,
    ModerateRainShowers,
    ViolentRainShowers,
    SlightSnowShowers,
    HeavySnowShowers,
    Thunderstorm,
    HeavyThunderstorm,
    ThunderstormSlightHail,
    ThunderstormHeavyHail,
}

impl Condition {
    /// Stable renderer-neutral keys. Only clear/mainly-clear/partly-cloudy
    /// conditions vary by daylight; precipitation/fog icons are shared.
    pub const fn icon_key(self, is_day: bool) -> &'static str {
        match self {
            Self::ClearSky => {
                if is_day {
                    "clear-day"
                } else {
                    "clear-night"
                }
            }
            Self::MainlyClear => {
                if is_day {
                    "mainly-clear-day"
                } else {
                    "mainly-clear-night"
                }
            }
            Self::PartlyCloudy => {
                if is_day {
                    "partly-cloudy-day"
                } else {
                    "partly-cloudy-night"
                }
            }
            Self::Overcast => "overcast",
            Self::Fog => "fog",
            Self::DepositingRimeFog => "rime-fog",
            Self::LightDrizzle | Self::ModerateDrizzle | Self::DenseDrizzle => "drizzle",
            Self::LightFreezingDrizzle | Self::DenseFreezingDrizzle => "freezing-drizzle",
            Self::SlightRain | Self::ModerateRain | Self::HeavyRain => "rain",
            Self::LightFreezingRain | Self::HeavyFreezingRain => "freezing-rain",
            Self::SlightSnowfall | Self::ModerateSnowfall | Self::HeavySnowfall => "snow",
            Self::SnowGrains => "snow-grains",
            Self::SlightRainShowers | Self::ModerateRainShowers | Self::ViolentRainShowers => {
                "rain-showers"
            }
            Self::SlightSnowShowers | Self::HeavySnowShowers => "snow-showers",
            Self::Thunderstorm | Self::HeavyThunderstorm => "thunderstorm",
            Self::ThunderstormSlightHail | Self::ThunderstormHeavyHail => "thunderstorm-hail",
        }
    }
}

pub fn wmo_condition(code: u16) -> Option<Condition> {
    Some(match code {
        0 => Condition::ClearSky,
        1 => Condition::MainlyClear,
        2 => Condition::PartlyCloudy,
        3 => Condition::Overcast,
        45 => Condition::Fog,
        48 => Condition::DepositingRimeFog,
        51 => Condition::LightDrizzle,
        53 => Condition::ModerateDrizzle,
        55 => Condition::DenseDrizzle,
        56 => Condition::LightFreezingDrizzle,
        57 => Condition::DenseFreezingDrizzle,
        61 => Condition::SlightRain,
        63 => Condition::ModerateRain,
        65 => Condition::HeavyRain,
        66 => Condition::LightFreezingRain,
        67 => Condition::HeavyFreezingRain,
        71 => Condition::SlightSnowfall,
        73 => Condition::ModerateSnowfall,
        75 => Condition::HeavySnowfall,
        77 => Condition::SnowGrains,
        80 => Condition::SlightRainShowers,
        81 => Condition::ModerateRainShowers,
        82 => Condition::ViolentRainShowers,
        85 => Condition::SlightSnowShowers,
        86 => Condition::HeavySnowShowers,
        95 => Condition::Thunderstorm,
        96 => Condition::ThunderstormSlightHail,
        97 => Condition::HeavyThunderstorm,
        99 => Condition::ThunderstormHeavyHail,
        _ => return None,
    })
}
