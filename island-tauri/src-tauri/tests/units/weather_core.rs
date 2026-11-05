use hermes_island_lib::integrations::weather::policy::{
    forecast_url, geocode_url, next_refresh_unix, on_failure, parse_forecast, parse_geocode,
    wmo_condition, Condition, GeoPlace, WeatherError, WeatherState, MAX_BODY_BYTES,
};
use serde_json::{json, Value};

// Hand-written documentation-derived fixtures; see fixtures/README.md.
const FORECAST: &[u8] = include_bytes!("../fixtures/weather/forecast.json");
const GEOCODE: &[u8] = include_bytes!("../fixtures/weather/geocode.json");
const API_ERROR: &[u8] = include_bytes!("../fixtures/weather/error.json");

fn place() -> GeoPlace {
    GeoPlace {
        id: 2950159,
        name: "Berlin".into(),
        latitude: 52.52437,
        longitude: 13.41053,
        country: Some("Deutschland".into()),
        country_code: Some("DE".into()),
        admin1: Some("Berlin".into()),
        timezone: Some("Europe/Berlin".into()),
    }
}

fn fixture() -> Value {
    match serde_json::from_slice(FORECAST) {
        Ok(value) => value,
        Err(error) => panic!("hand-written fixture invalid: {error}"),
    }
}

fn bytes(value: &Value) -> Vec<u8> {
    match serde_json::to_vec(value) {
        Ok(value) => value,
        Err(error) => panic!("test JSON failed: {error}"),
    }
}

fn forecast(value: &Value) -> Result<WeatherState, WeatherError> {
    parse_forecast(&bytes(value), &place(), 1234)
}

// Independent test decoder, not production encoder logic; %HH only, '+' stays '+'.
fn decode(input: &str) -> String {
    let input = input.as_bytes();
    let mut output = Vec::new();
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' {
            assert!(i + 2 < input.len(), "truncated percent escape");
            let high = char::from(input[i + 1]).to_digit(16);
            let low = char::from(input[i + 2]).to_digit(16);
            match (high, low) {
                (Some(h), Some(l)) => output.push((h * 16 + l) as u8),
                _ => panic!("invalid percent escape"),
            }
            i += 3;
        } else {
            output.push(input[i]);
            i += 1;
        }
    }
    match String::from_utf8(output) {
        Ok(text) => text,
        Err(error) => panic!("invalid encoded UTF-8: {error}"),
    }
}

#[test]
fn forecast_fixture_sets_all_fields_and_uses_fetch_unix_time_not_local_time() {
    let expected = WeatherState {
        available: true,
        stale: false,
        temp_c: Some(23.5),
        code: Some(2),
        is_day: Some(true),
        place: Some(place()),
        updated_at: Some(1234),
    };
    assert_eq!(parse_forecast(FORECAST, &place(), 1234), Ok(expected));
    let mut value = fixture();
    value["utc_offset_seconds"] = json!(-18000);
    value["timezone"] = json!("America/New_York");
    value["current"]["time"] = json!("not parsed as epoch");
    assert_eq!(forecast(&value).unwrap().updated_at, Some(1234));
    assert_eq!(
        parse_forecast(FORECAST, &place(), 0).unwrap().updated_at,
        Some(0)
    );
}

#[test]
fn forecast_ignores_unknown_fields_without_requiring_metadata() {
    let value = json!({
        "current_units": {"temperature_2m": "°C", "extra": [1, 2]},
        "current": {"temperature_2m": 0, "weather_code": 0, "is_day": 0, "future": {}},
        "future": {"anything": true}
    });
    let state = forecast(&value).unwrap();
    assert_eq!(state.temp_c, Some(0.0));
    assert_eq!(state.code, Some(0));
    assert_eq!(state.is_day, Some(false));
    assert!(state.available);
}

#[test]
fn forecast_missing_current_and_nested_fields_have_typed_errors() {
    for missing in ["current", "current_units"] {
        let mut value = fixture();
        value.as_object_mut().unwrap().remove(missing);
        assert_eq!(forecast(&value), Err(WeatherError::MissingField(missing)));
        value[missing] = Value::Null;
        assert_eq!(forecast(&value), Err(WeatherError::MissingField(missing)));
    }
    for missing in ["temperature_2m", "weather_code", "is_day"] {
        let mut value = fixture();
        value["current"].as_object_mut().unwrap().remove(missing);
        assert_eq!(forecast(&value), Err(WeatherError::MissingField(missing)));
    }
    let mut value = fixture();
    value["current_units"] = json!({});
    assert_eq!(
        forecast(&value),
        Err(WeatherError::MissingField("temperature_2m"))
    );
}

#[test]
fn forecast_rejects_wrong_current_containers_and_types() {
    for key in ["current", "current_units"] {
        for invalid in [json!([]), json!(5), json!("object"), json!(false)] {
            let mut value = fixture();
            value[key] = invalid;
            assert_eq!(forecast(&value), Err(WeatherError::Invalid(key)));
        }
    }
    for invalid in [json!("23"), json!(true), json!([]), json!({})] {
        let mut value = fixture();
        value["current"]["temperature_2m"] = invalid;
        assert_eq!(
            forecast(&value),
            Err(WeatherError::Invalid("temperature_2m"))
        );
    }
}

#[test]
fn forecast_requires_celsius_and_rejects_fahrenheit_or_any_other_unit() {
    for unit in [
        json!("°F"),
        json!("C"),
        json!("celsius"),
        json!("K"),
        json!(0),
        json!(false),
    ] {
        let mut value = fixture();
        value["current_units"]["temperature_2m"] = unit;
        assert_eq!(
            forecast(&value),
            Err(WeatherError::Invalid("current_units.temperature_2m"))
        );
    }
}

#[test]
fn forecast_temperature_range_is_inclusive_and_rejects_outliers() {
    for temperature in [-100.0, -0.0, 0.0, 70.0] {
        let mut value = fixture();
        value["current"]["temperature_2m"] = json!(temperature);
        assert_eq!(forecast(&value).unwrap().temp_c, Some(temperature));
    }
    for temperature in [-100.0001, 70.0001, -273.15, 1000.0] {
        let mut value = fixture();
        value["current"]["temperature_2m"] = json!(temperature);
        assert_eq!(
            forecast(&value),
            Err(WeatherError::Invalid("temperature_2m"))
        );
    }
}

#[test]
fn forecast_nonfinite_syntax_and_overflow_are_errors_not_measurements() {
    for token in ["NaN", "Infinity", "-Infinity", "1e400", "-1e400"] {
        let body = format!(
            "{{\"current_units\":{{\"temperature_2m\":\"°C\"}},\"current\":{{\"temperature_2m\":{token},\"weather_code\":0,\"is_day\":1}}}}"
        );
        assert_eq!(
            parse_forecast(body.as_bytes(), &place(), 1),
            Err(WeatherError::Json)
        );
    }
}

#[test]
fn forecast_codes_are_in_range_unsigned_integers_or_rejected() {
    for code in [0, 1, 2, 3, 4, 45, 97, 99] {
        let mut value = fixture();
        value["current"]["weather_code"] = json!(code);
        assert_eq!(forecast(&value).unwrap().code, Some(code));
    }
    assert_eq!(wmo_condition(4), None);
    assert_eq!(wmo_condition(97), Some(Condition::HeavyThunderstorm));
    for code in [
        json!(100),
        json!(65535),
        json!(65536),
        json!(-1),
        json!(2.0),
        json!("2"),
        json!(true),
    ] {
        let mut value = fixture();
        value["current"]["weather_code"] = code;
        assert_eq!(forecast(&value), Err(WeatherError::Invalid("weather_code")));
    }
}

#[test]
fn forecast_daylight_is_only_integer_zero_or_one() {
    for (day, expected) in [(0, false), (1, true)] {
        let mut value = fixture();
        value["current"]["is_day"] = json!(day);
        assert_eq!(forecast(&value).unwrap().is_day, Some(expected));
    }
    for day in [
        json!(2),
        json!(-1),
        json!(1.0),
        json!("1"),
        json!(true),
        json!([]),
    ] {
        let mut value = fixture();
        value["current"]["is_day"] = day;
        assert_eq!(forecast(&value), Err(WeatherError::Invalid("is_day")));
    }
}

#[test]
fn parsers_check_body_cap_before_json_including_exact_boundary() {
    assert_eq!(MAX_BODY_BYTES, 262144);
    let mut body = FORECAST.to_vec();
    body.resize(MAX_BODY_BYTES, b' ');
    assert!(parse_forecast(&body, &place(), 1).is_ok());
    body.push(b' ');
    assert_eq!(
        parse_forecast(&body, &place(), 1),
        Err(WeatherError::Oversize)
    );
    assert_eq!(parse_geocode(&body), Err(WeatherError::Oversize));
    let invalid = vec![0xff; MAX_BODY_BYTES + 1];
    assert_eq!(
        parse_forecast(&invalid, &place(), 1),
        Err(WeatherError::Oversize)
    );
    assert_eq!(parse_geocode(&invalid), Err(WeatherError::Oversize));
    let mut empty_results = b"{}".to_vec();
    empty_results.resize(MAX_BODY_BYTES, b' ');
    assert_eq!(parse_geocode(&empty_results), Ok(vec![]));
}

#[test]
fn both_parsers_return_api_reason_before_success_shape_checks() {
    let error = WeatherError::ApiError("Parameter count must be between 1 and 100.".into());
    assert_eq!(parse_forecast(API_ERROR, &place(), 1), Err(error.clone()));
    assert_eq!(parse_geocode(API_ERROR), Err(error));
    let mut value = fixture();
    value["error"] = json!(true);
    value["reason"] = json!("explicit upstream failure");
    assert_eq!(
        forecast(&value),
        Err(WeatherError::ApiError("explicit upstream failure".into()))
    );
    value["error"] = json!(false);
    assert!(forecast(&value).is_ok());
}

#[test]
fn api_reasons_truncate_at_two_hundred_unicode_characters_not_bytes() {
    let reason = "東京ö".repeat(100);
    let value = json!({"error":true, "reason": reason});
    let expected: String = reason.chars().take(200).collect();
    assert_eq!(
        forecast(&value),
        Err(WeatherError::ApiError(expected.clone()))
    );
    assert_eq!(
        parse_geocode(&bytes(&value)),
        Err(WeatherError::ApiError(expected))
    );
    for reason in ["", "short", &"x".repeat(200)] {
        let value = json!({"error": true, "reason": reason});
        assert_eq!(
            parse_geocode(&bytes(&value)),
            Err(WeatherError::ApiError(reason.to_owned()))
        );
    }
}

#[test]
fn malformed_error_reason_is_missing_or_invalid_without_fabricated_text() {
    for body in [
        b"{\"error\":true}".as_slice(),
        b"{\"error\":true,\"reason\":null}".as_slice(),
    ] {
        assert_eq!(
            parse_geocode(body),
            Err(WeatherError::MissingField("reason"))
        );
        assert_eq!(
            parse_forecast(body, &place(), 1),
            Err(WeatherError::MissingField("reason"))
        );
    }
    let value = json!({"error":true, "reason": {"nested":"no"}});
    assert_eq!(
        parse_geocode(&bytes(&value)),
        Err(WeatherError::Invalid("reason"))
    );
}

#[test]
fn both_parsers_reject_bad_json_utf8_trailing_data_and_nonobject_roots() {
    for body in [
        b"".as_slice(),
        b"{".as_slice(),
        b"\xff".as_slice(),
        b"{} {}".as_slice(),
    ] {
        assert_eq!(parse_geocode(body), Err(WeatherError::Json));
        assert_eq!(parse_forecast(body, &place(), 1), Err(WeatherError::Json));
    }
    for body in [
        b"null".as_slice(),
        b"[]".as_slice(),
        b"42".as_slice(),
        b"true".as_slice(),
        b"\"x\"".as_slice(),
    ] {
        assert_eq!(parse_geocode(body), Err(WeatherError::Invalid("root")));
        assert_eq!(
            parse_forecast(body, &place(), 1),
            Err(WeatherError::Invalid("root"))
        );
    }
}

#[test]
fn geocode_fixture_maps_snake_case_upstream_to_optional_contract_fields() {
    let places = parse_geocode(GEOCODE).unwrap();
    assert_eq!(places.len(), 2);
    assert_eq!(places[0], place());
    assert_eq!(places[1].id, 1);
    assert_eq!(places[1].latitude, -90.0);
    assert_eq!(places[1].longitude, 180.0);
    assert_eq!(places[1].country, None);
    assert_eq!(places[1].country_code, None);
    assert_eq!(places[1].admin1, None);
    assert_eq!(places[1].timezone, None);
}

#[test]
fn geocode_missing_empty_or_null_results_are_empty_but_wrong_container_is_invalid() {
    for value in [
        json!({}),
        json!({"results":[]}),
        json!({"results":null}),
        json!({"future":true}),
    ] {
        assert_eq!(parse_geocode(&bytes(&value)), Ok(vec![]));
    }
    for results in [json!({}), json!("no"), json!(false), json!(42)] {
        assert_eq!(
            parse_geocode(&bytes(&json!({"results": results}))),
            Err(WeatherError::Invalid("results"))
        );
    }
}

#[test]
fn geocode_drops_invalid_coordinates_and_blank_names_but_keeps_boundaries() {
    let mut entries = vec![];
    for (lat, lon) in [
        (90.0001, 0.0),
        (-90.0001, 0.0),
        (0.0, 180.0001),
        (0.0, -180.0001),
    ] {
        entries.push(json!({"id":1,"name":"Bad","latitude":lat,"longitude":lon}));
    }
    for name in ["", "  \t\n", "\u{2003}"] {
        entries.push(json!({"id":2,"name":name,"latitude":0,"longitude":0}));
    }
    entries.push(json!({"id":3,"name":"South","latitude":-90,"longitude":-180}));
    entries.push(json!({"id":4,"name":"North","latitude":90,"longitude":180}));
    let places = parse_geocode(&bytes(&json!({"results":entries}))).unwrap();
    assert_eq!(places.iter().map(|p| p.id).collect::<Vec<_>>(), vec![3, 4]);
}

#[test]
fn geocode_skips_malformed_entries_and_does_not_invent_required_values() {
    let mut entries = vec![Value::Null, json!(false), json!("bad"), json!([])];
    let valid = json!({"id":99,"name":"Valid","latitude":0,"longitude":0});
    for field in ["id", "name", "latitude", "longitude"] {
        let mut entry = valid.clone();
        entry.as_object_mut().unwrap().remove(field);
        entries.push(entry);
        let mut entry = valid.clone();
        entry[field] = Value::Null;
        entries.push(entry);
    }
    for (field, bad) in [
        ("id", json!(-1)),
        ("id", json!(1.0)),
        ("latitude", json!("0")),
        ("longitude", json!([])),
        ("country", json!(123)),
        ("country_code", json!(false)),
        ("admin1", json!({})),
        ("timezone", json!([])),
    ] {
        let mut entry = valid.clone();
        entry[field] = bad;
        entries.push(entry);
    }
    entries.push(valid);
    let places = parse_geocode(&bytes(&json!({"results": entries}))).unwrap();
    assert_eq!(places.len(), 1);
    assert_eq!(places[0].id, 99);
    assert_eq!(places[0].latitude, 0.0);
}

#[test]
fn geocode_retains_first_ten_valid_results_in_original_order() {
    let mut entries = vec![json!({"id":999,"name":"Invalid","latitude":100,"longitude":0})];
    for id in 1..=15 {
        entries.push(json!({"id":id,"name":format!("Place {id}"),"latitude":0,"longitude":0}));
        entries.push(Value::Null);
    }
    let places = parse_geocode(&bytes(&json!({"results":entries}))).unwrap();
    assert_eq!(places.len(), 10);
    assert_eq!(
        places.iter().map(|p| p.id).collect::<Vec<_>>(),
        (1..=10).collect::<Vec<_>>()
    );
}

#[test]
fn geocode_optional_nulls_are_preserved_as_none_and_extra_fields_ignored() {
    let body = json!({"results":[{"id":0,"name":"  Kept literally  ","latitude":0,"longitude":0,
        "country":null,"country_code":null,"admin1":null,"timezone":null,"future":true}]});
    let p = parse_geocode(&bytes(&body)).unwrap().remove(0);
    assert_eq!(p.name, "  Kept literally  ");
    assert_eq!(p.country, None);
    assert_eq!(p.country_code, None);
    assert_eq!(p.admin1, None);
    assert_eq!(p.timezone, None);
}

#[test]
fn urls_append_endpoint_once_below_origin_or_prefix_without_trailing_slash_dependency() {
    let p = place();
    let expected = "https://api.open-meteo.com/v1/forecast?latitude=52.52437&longitude=13.41053&current=temperature_2m,weather_code,is_day&temperature_unit=celsius&timezone=auto";
    assert_eq!(
        forecast_url("https://api.open-meteo.com", &p).unwrap(),
        expected
    );
    assert_eq!(
        forecast_url("https://api.open-meteo.com/", &p).unwrap(),
        expected
    );
    assert_eq!(
        forecast_url("https://api.open-meteo.com///", &p).unwrap(),
        expected
    );
    assert_eq!(
        geocode_url("http://localhost:8080/prefix/", "Berlin", 10, "de").unwrap(),
        "http://localhost:8080/prefix/v1/search?name=Berlin&count=10&language=de&format=json"
    );
    assert_eq!(
        geocode_url("https://geocoding-api.open-meteo.com", "Berlin", 1, "en"),
        geocode_url("https://geocoding-api.open-meteo.com/", "Berlin", 1, "en")
    );
}

#[test]
fn geocode_percent_encoding_roundtrips_reserved_characters_and_utf8() {
    for (query, encoded) in [
        ("Palm Coast", "Palm%20Coast"),
        ("A&B", "A%26B"),
        ("A#B", "A%23B"),
        ("A/B", "A%2FB"),
        ("Köln", "K%C3%B6ln"),
        ("東京", "%E6%9D%B1%E4%BA%AC"),
        ("A+B%?=", "A%2BB%25%3F%3D"),
        ("AZaz09-._~", "AZaz09-._~"),
    ] {
        let url = geocode_url("https://example.com", query, 1, "en").unwrap();
        let query_part = url.split('?').nth(1).unwrap();
        let name = query_part
            .split('&')
            .next()
            .unwrap()
            .strip_prefix("name=")
            .unwrap();
        assert_eq!(name, encoded, "{query}");
        assert_eq!(decode(name), query);
        assert_eq!(
            query_part.split('&').count(),
            4,
            "must not inject parameters"
        );
        assert!(!url.contains('#'));
    }
}

#[test]
fn geocode_trims_and_counts_unicode_characters_not_bytes() {
    assert!(geocode_url("https://example.com", "東京", 1, "en").is_ok());
    assert!(geocode_url("https://example.com", &"東".repeat(100), 1, "en").is_ok());
    for query in ["", " ", "A", "\t東\n", &"東".repeat(101), &"x".repeat(101)] {
        assert_eq!(
            geocode_url("https://example.com", query, 1, "en"),
            Err(WeatherError::Invalid("query"))
        );
    }
    assert_eq!(
        geocode_url("https://example.com", " \n Köln \t", 1, "en"),
        geocode_url("https://example.com", "Köln", 1, "en")
    );
}

#[test]
fn geocode_count_is_one_through_ten_inclusive() {
    for count in 1..=10 {
        assert!(geocode_url("https://example.com", "AB", count, "en").is_ok());
    }
    for count in [0, 11, 255] {
        assert_eq!(
            geocode_url("https://example.com", "AB", count, "en"),
            Err(WeatherError::Invalid("count"))
        );
    }
}

#[test]
fn language_is_encoded_literally_and_cannot_inject_parameters_or_fragment() {
    let language = "de&count=100#東京/ +";
    let url = geocode_url("https://example.com", "Berlin", 2, language).unwrap();
    let query = url.split('?').nth(1).unwrap();
    let encoded = query
        .split('&')
        .nth(2)
        .unwrap()
        .strip_prefix("language=")
        .unwrap();
    assert_eq!(decode(encoded), language);
    assert_eq!(query.split('&').count(), 4);
    assert!(!url.contains('#'));
}

#[test]
fn urls_reject_invalid_bases_instead_of_query_or_fragment_injection() {
    for base in [
        "",
        "/relative",
        "ftp://example.com",
        "https://",
        "http:///",
        "https://x?bad=1",
        "https://x#fragment",
        "https://user@host",
        "https://x\\evil",
        "https://x/has space",
        "https://x/\0",
    ] {
        assert_eq!(
            forecast_url(base, &place()),
            Err(WeatherError::Invalid("base")),
            "{base}"
        );
        assert_eq!(
            geocode_url(base, "Berlin", 1, "en"),
            Err(WeatherError::Invalid("base")),
            "{base}"
        );
    }
}

#[test]
fn forecast_url_and_parser_reject_bad_place_coordinates_including_nonfinite() {
    for (lat, lon) in [
        (f64::NAN, 0.0),
        (f64::INFINITY, 0.0),
        (0.0, f64::NEG_INFINITY),
        (91.0, 0.0),
        (-91.0, 0.0),
        (0.0, 181.0),
        (0.0, -181.0),
    ] {
        let mut p = place();
        p.latitude = lat;
        p.longitude = lon;
        assert_eq!(
            forecast_url("https://example.com", &p),
            Err(WeatherError::Invalid("coordinates"))
        );
        assert_eq!(
            parse_forecast(FORECAST, &p, 1),
            Err(WeatherError::Invalid("coordinates"))
        );
    }
    for (lat, lon) in [(-90.0, -180.0), (90.0, 180.0), (0.0, 0.0)] {
        let mut p = place();
        p.latitude = lat;
        p.longitude = lon;
        assert!(forecast_url("https://example.com", &p).is_ok());
    }
}

#[test]
fn failure_keeps_entire_last_good_snapshot_stale_without_mutating_input() {
    let last = parse_forecast(FORECAST, &place(), 99).unwrap();
    let original = last.clone();
    let stale = on_failure(Some(&last));
    let mut expected = original.clone();
    expected.stale = true;
    assert_eq!(stale, expected);
    assert_eq!(last, original);
    assert_eq!(on_failure(Some(&stale)), stale);
}

#[test]
fn failure_without_last_good_is_unavailable_with_nulls_never_fabricated_zeros() {
    assert_eq!(on_failure(None), WeatherState::default());
    assert_eq!(
        serde_json::to_value(on_failure(None)).unwrap(),
        json!({
            "available":false,"stale":false,"tempC":null,"code":null,"isDay":null,"place":null,"updatedAt":null
        })
    );
}

#[test]
fn next_refresh_exact_boundary_is_strictly_next_quarter_hour() {
    for boundary in [0, 900, 1800, 86400, 172800, 1_700_000_100] {
        assert_eq!(boundary % 900, 0);
        assert_eq!(next_refresh_unix(boundary), boundary + 900);
    }
}

#[test]
fn next_refresh_second_before_boundary_and_every_remainder_are_wall_clock_aligned() {
    assert_eq!(next_refresh_unix(899), 900);
    assert_eq!(next_refresh_unix(1799), 1800);
    for now in 1..20000 {
        let next = next_refresh_unix(now);
        assert!(next > now);
        assert!(next - now <= 900);
        assert_eq!(next % 900, 0);
    }
}

#[test]
fn next_refresh_saturates_when_boundary_is_not_representable_without_overflow() {
    for now in [u64::MAX, u64::MAX - 1] {
        assert_eq!(next_refresh_unix(now), u64::MAX);
    }
    let last_boundary = u64::MAX - u64::MAX % 900;
    assert_eq!(next_refresh_unix(last_boundary - 1), last_boundary);
    assert_eq!(next_refresh_unix(last_boundary), u64::MAX);
}

#[test]
fn wmo_mapping_pins_every_requested_code_condition_and_stable_icon_pair() {
    use Condition::*;
    let cases = [
        (0, ClearSky, "clear-day", "clear-night"),
        (1, MainlyClear, "mainly-clear-day", "mainly-clear-night"),
        (2, PartlyCloudy, "partly-cloudy-day", "partly-cloudy-night"),
        (3, Overcast, "overcast", "overcast"),
        (45, Fog, "fog", "fog"),
        (48, DepositingRimeFog, "rime-fog", "rime-fog"),
        (51, LightDrizzle, "drizzle", "drizzle"),
        (53, ModerateDrizzle, "drizzle", "drizzle"),
        (55, DenseDrizzle, "drizzle", "drizzle"),
        (
            56,
            LightFreezingDrizzle,
            "freezing-drizzle",
            "freezing-drizzle",
        ),
        (
            57,
            DenseFreezingDrizzle,
            "freezing-drizzle",
            "freezing-drizzle",
        ),
        (61, SlightRain, "rain", "rain"),
        (63, ModerateRain, "rain", "rain"),
        (65, HeavyRain, "rain", "rain"),
        (66, LightFreezingRain, "freezing-rain", "freezing-rain"),
        (67, HeavyFreezingRain, "freezing-rain", "freezing-rain"),
        (71, SlightSnowfall, "snow", "snow"),
        (73, ModerateSnowfall, "snow", "snow"),
        (75, HeavySnowfall, "snow", "snow"),
        (77, SnowGrains, "snow-grains", "snow-grains"),
        (80, SlightRainShowers, "rain-showers", "rain-showers"),
        (81, ModerateRainShowers, "rain-showers", "rain-showers"),
        (82, ViolentRainShowers, "rain-showers", "rain-showers"),
        (85, SlightSnowShowers, "snow-showers", "snow-showers"),
        (86, HeavySnowShowers, "snow-showers", "snow-showers"),
        (95, Thunderstorm, "thunderstorm", "thunderstorm"),
        (97, HeavyThunderstorm, "thunderstorm", "thunderstorm"),
        (
            96,
            ThunderstormSlightHail,
            "thunderstorm-hail",
            "thunderstorm-hail",
        ),
        (
            99,
            ThunderstormHeavyHail,
            "thunderstorm-hail",
            "thunderstorm-hail",
        ),
    ];
    for (code, condition, day_icon, night_icon) in cases {
        assert_eq!(wmo_condition(code), Some(condition), "{code}");
        assert_eq!(condition.icon_key(true), day_icon, "day {code}");
        assert_eq!(condition.icon_key(false), night_icon, "night {code}");
        let mut value = fixture();
        value["current"]["weather_code"] = json!(code);
        assert_eq!(forecast(&value).unwrap().code, Some(code));
    }
}

#[test]
fn wmo_unknown_codes_are_none_exhaustively() {
    let known = [
        0, 1, 2, 3, 45, 48, 51, 53, 55, 56, 57, 61, 63, 65, 66, 67, 71, 73, 75, 77, 80, 81, 82, 85,
        86, 95, 96, 97, 99,
    ];
    for code in 0..=u16::MAX {
        assert_eq!(
            wmo_condition(code).is_some(),
            known.contains(&code),
            "{code}"
        );
    }
}

#[test]
fn state_and_place_wire_shapes_are_camel_case_with_explicit_nulls_and_no_aliases() {
    let state = parse_forecast(FORECAST, &place(), 1234).unwrap();
    let encoded = serde_json::to_value(&state).unwrap();
    assert_eq!(
        encoded,
        json!({
            "available":true,"stale":false,"tempC":23.5,"code":2,"isDay":true,"updatedAt":1234,
            "place":{"id":2950159,"name":"Berlin","latitude":52.52437,"longitude":13.41053,
                "country":"Deutschland","countryCode":"DE","admin1":"Berlin","timezone":"Europe/Berlin"}
        })
    );
    assert_eq!(
        serde_json::from_value::<WeatherState>(encoded.clone()).unwrap(),
        state
    );
    let mut snake_only = encoded;
    for (camel, snake) in [
        ("tempC", "temp_c"),
        ("isDay", "is_day"),
        ("updatedAt", "updated_at"),
    ] {
        let value = snake_only.as_object_mut().unwrap().remove(camel).unwrap();
        snake_only[snake] = value;
    }
    let value = snake_only["place"]
        .as_object_mut()
        .unwrap()
        .remove("countryCode")
        .unwrap();
    snake_only["place"]["country_code"] = value;
    let decoded: WeatherState = serde_json::from_value(snake_only).unwrap();
    assert_eq!(decoded.temp_c, None);
    assert_eq!(decoded.is_day, None);
    assert_eq!(decoded.updated_at, None);
    assert_eq!(decoded.place.unwrap().country_code, None);
}

#[test]
fn geocode_upstream_camel_case_is_not_an_alias_for_country_code() {
    let body =
        json!({"results":[{"id":1,"name":"City","latitude":0,"longitude":0,"countryCode":"DE"}]});
    assert_eq!(parse_geocode(&bytes(&body)).unwrap()[0].country_code, None);
}

#[test]
fn errors_have_stable_display_and_implement_standard_error() {
    let errors = [
        WeatherError::Oversize,
        WeatherError::Json,
        WeatherError::ApiError("reason".into()),
        WeatherError::MissingField("current"),
        WeatherError::Invalid("query"),
    ];
    for error in errors {
        let as_error: &dyn std::error::Error = &error;
        assert!(!as_error.to_string().is_empty());
    }
}

#[test]
fn never_panic_lcg_random_byte_bodies_for_both_parsers() {
    let mut seed = 0x173a_92f8_d6c4_051bu64;
    let p = place();
    for iteration in 0..8192 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let len = ((seed >> 32) as usize) % 2048;
        let mut body = Vec::with_capacity(len);
        for _ in 0..len {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            body.push((seed >> 32) as u8);
        }
        let _ = parse_forecast(&body, &p, iteration);
        let _ = parse_geocode(&body);
        // Feed some arbitrary bytes into structurally valid envelopes as well.
        if iteration % 32 == 0 {
            let value = json!({"results":[String::from_utf8_lossy(&body)], "error": false});
            let _ = parse_geocode(&bytes(&value));
        }
    }
    let deeply_nested = format!("{}0{}", "[".repeat(1000), "]".repeat(1000));
    assert!(parse_forecast(deeply_nested.as_bytes(), &p, 1).is_err());
    assert!(parse_geocode(deeply_nested.as_bytes()).is_err());
}
