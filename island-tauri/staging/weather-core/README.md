# weather-core

Standalone pure Rust library for Ripple Phase 6 weather. Only serde/serde_json;
no IO, network, clock reads, threads, platform dependencies, or unsafe code.

- `forecast_url(base, place)`: Celsius current temperature, WMO code, daylight;
  automatic timezone. `base` is an HTTP(S) origin or path prefix, e.g.
  `https://api.open-meteo.com` or `http://localhost:8080/fixture/`.
- `geocode_url(base, query, count, language)`: trimmed 2–100 Unicode-scalar query,
  1–10 results, RFC 3986 UTF-8 percent encoding. Appends `/v1/search`.
- `parse_forecast` / `parse_geocode`: bounded, defensive response parsing.
  Contract structs serialize camelCase; upstream fields stay snake_case.
- `on_failure`: preserve the caller's last-good snapshot and mark stale, or
  return unavailable with null measurements when no successful fetch exists.
- `next_refresh_unix`: next UTC quarter-hour; saturates only at u64 overflow.
- `wmo_condition` / `Condition::icon_key`: explicit code mapping and stable
  renderer-neutral icon keys, not actual icon assets. Undocumented in-range code
  (0–99) return None; the parser still stores them so the UI can show a generic
  icon (code 97 heavy thunderstorm is mapped).

Use snake_case Rust fields (`temp_c`, `is_day`, `country_code`, `updated_at`);
serde provides the requested `tempC`, `isDay`, `countryCode`, `updatedAt` wire keys.
The adapter owns HTTP status handling, streaming body bounds, timeout (SPEC:
10 seconds), scheduling/resume, persistence and UI Fahrenheit conversion.
No automatic age threshold is invented: staleness is failure-driven.

Tests use hand-written documentation-derived fixtures, never live requests.
See VERIFICATION.md for adoption of upstream code 97, undocumented in-range
code tolerance, policy decisions, actual gate output and deliberate-breakage
mutation evidence.

Run, in order, under nice priority:

    nice -n 10 cargo fmt --check
    nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    nice -n 10 cargo test --locked

`scripts/verify.py` reproduces recorded gates and isolated mutation checks.
