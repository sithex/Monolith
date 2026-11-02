use ai_dashboard_core::{
    civil_from_days, days_from_civil, hour_start_utc, local_day_index, local_hour_index,
    local_weekday_and_hour, weekday,
};

fn ts(y: i64, m: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
    days_from_civil(y, m, d) * 86_400 + (h as i64) * 3600 + (mi as i64) * 60 + s as i64
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[test]
fn days_from_civil_known_anchors() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(2000, 1, 1), 10_957);
    assert_eq!(days_from_civil(2026, 10, 30), 20_756);
}

#[test]
fn civil_from_days_roundtrip_across_years() {
    let ms = [(1, 1), (2, 29), (3, 1), (6, 15), (10, 30), (12, 31)];
    for y in -500..=2500 {
        for &(m, d) in &ms {
            if m == 2 && d == 29 && !is_leap(y) {
                continue;
            }
            let days = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(days), (y, m, d));
        }
    }
}

#[test]
fn weekday_known_dates() {
    // 1970-01-01 = Thursday (=3)
    assert_eq!(weekday(0), 3);
    // 2000-01-01 = Saturday (=5)
    assert_eq!(weekday(10_957), 5);
    // 2019-01-01 = Tuesday (=1)
    assert_eq!(weekday(17_897), 1);
    // 2026-10-30 = Friday (=4)
    assert_eq!(weekday(20_756), 4);
    // 2026-10-26 = Monday (=0)
    assert_eq!(weekday(20_752), 0);
}

#[test]
fn local_day_index_respects_offset_boundary() {
    let midnight_utc_oct30 = ts(2026, 10, 30, 0, 0, 0);
    // -4h (America/New_York EDT): midnight UTC is still Oct 29 locally.
    assert_eq!(local_day_index(midnight_utc_oct30, -14_400), 20_755);
    // +5:30 (IST): midnight UTC is Oct 30 locally.
    assert_eq!(local_day_index(midnight_utc_oct30, 19_800), 20_756);
    // 23:59:59Z Oct 29 with +5:30 is already Oct 30.
    let before_midnight = ts(2026, 10, 29, 23, 59, 59);
    assert_eq!(local_day_index(before_midnight, 19_800), 20_756);
    // and with -4h it is still Oct 29.
    assert_eq!(local_day_index(before_midnight, -14_400), 20_755);
}

#[test]
fn hour_bucket_start_utc_property() {
    for t in [ts(2026, 10, 30, 4, 30, 0), -123_456_789, 0, i64::MAX / 2] {
        for offset in [-14_400i32, 0, 19_800] {
            let h = local_hour_index(t, offset);
            let start = hour_start_utc(h, offset);
            let rem = (t as i128 + offset as i128).rem_euclid(3600) as i64;
            assert_eq!(start, t - rem);
            assert!(t >= start && t - start < 3600);
        }
    }
}

#[test]
fn local_weekday_and_hour_known() {
    assert_eq!(
        local_weekday_and_hour(ts(2026, 10, 30, 10, 15, 0), 0),
        (4, 10)
    );
    assert_eq!(local_weekday_and_hour(ts(2026, 10, 26, 9, 0, 0), 0), (0, 9));
    // 10:15Z with +5:30 is 15:45 local (same day, hour 15).
    assert_eq!(
        local_weekday_and_hour(ts(2026, 10, 30, 10, 15, 0), 19_800),
        (4, 15)
    );
    // 23:30Z with -4h is 19:30 local (same day, hour 19).
    assert_eq!(
        local_weekday_and_hour(ts(2026, 10, 30, 23, 30, 0), -14_400),
        (4, 19)
    );
    // 01:30Z with -4h is 21:30 previous day (Oct 29, hour 21).
    assert_eq!(
        local_weekday_and_hour(ts(2026, 10, 30, 1, 30, 0), -14_400),
        (3, 21)
    );
}

#[test]
fn local_day_index_negative_timestamps_floor() {
    // 1969-12-31T23:00:00Z with -4h is 1969-12-31 19:00 local.
    assert_eq!(local_day_index(ts(1969, 12, 31, 23, 0, 0), -14_400), -1);
    assert_eq!(local_day_index(ts(1969, 12, 31, 23, 0, 0), 0), -1);
    // 1970-01-01T00:30:00Z with -4h is still 1969-12-31 locally.
    assert_eq!(local_day_index(ts(1970, 1, 1, 0, 30, 0), -14_400), -1);
    // 01:00Z with -4h is 1969-12-31 21:00 locally.
    assert_eq!(local_day_index(ts(1970, 1, 1, 1, 0, 0), -14_400), -1);
}
