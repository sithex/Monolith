//! Civil-date and local-time arithmetic (proleptic Gregorian; no chrono).

/// Days since 1970-01-01 (Unix epoch) for a civil date. Howard Hinnant's
/// `days_from_civil` algorithm. `m` in 1..=12, `d` in 1..=31.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let m = m as i64;
    let d = d as i64;
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Inverse of [`days_from_civil`]: `(year, month, day)`.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// Weekday of a day number: 0 = Monday … 6 = Sunday (1970-01-01 was a
/// Thursday, i.e. weekday 3).
pub fn weekday(days: i64) -> u32 {
    (days + 3).rem_euclid(7) as u32
}

/// Floor division for signed integers.
#[inline]
pub(crate) fn floor_div(a: i128, b: i128) -> i128 {
    let q = a / b;
    let r = a % b;
    if r != 0 && (r < 0) != (b < 0) {
        q - 1
    } else {
        q
    }
}

/// Local calendar-day index (days since epoch) for a UTC timestamp.
pub fn local_day_index(t: i64, offset: i32) -> i64 {
    floor_div(t as i128 + offset as i128, 86_400) as i64
}

/// Local hour index (hours since epoch) for a UTC timestamp.
pub fn local_hour_index(t: i64, offset: i32) -> i64 {
    floor_div(t as i128 + offset as i128, 3_600) as i64
}

/// UTC second at which a local hour starts.
pub fn hour_start_utc(local_hour: i64, offset: i32) -> i64 {
    (local_hour as i128 * 3_600 - offset as i128) as i64
}

/// `(weekday 0=Monday, hour 0..24)` for a UTC timestamp in local time.
pub fn local_weekday_and_hour(t: i64, offset: i32) -> (u32, u32) {
    let local = t as i128 + offset as i128;
    let day = floor_div(local, 86_400) as i64;
    let hour = floor_div(local, 3_600).rem_euclid(24) as u32;
    (weekday(day), hour)
}
