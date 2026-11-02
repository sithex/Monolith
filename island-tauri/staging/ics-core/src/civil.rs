//! Civil-date arithmetic (proleptic Gregorian; no chrono). Weekday numbers
//! follow the sibling `ai-dashboard-core` convention: 0 = Monday … 6 =
//! Sunday (1970-01-01 was a Thursday, weekday 3).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NaiveDateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl NaiveDateTime {
    pub fn new(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) -> Option<Self> {
        if !(1..=12).contains(&month) {
            return None;
        }
        if day == 0 || day > days_in_month(year, month) {
            return None;
        }
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        Some(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
        })
    }

    pub fn date(year: i32, month: u32, day: u32) -> Option<Self> {
        Self::new(year, month, day, 0, 0, 0)
    }

    pub fn days(&self) -> i64 {
        days_from_civil(self.year as i64, self.month, self.day)
    }

    /// Weekday number: 0 = Monday … 6 = Sunday.
    pub fn weekday_number(&self) -> u32 {
        weekday(self.days())
    }

    pub fn add_days(&self, days: i64) -> Self {
        let (y, m, d) = civil_from_days(self.days() + days);
        Self {
            year: y as i32,
            month: m,
            day: d,
            hour: self.hour,
            minute: self.minute,
            second: self.second,
        }
    }

    /// Target calendar month `months` after this instant: `(year, month)`.
    pub fn months_later(&self, months: i64) -> (i32, u32) {
        let total = self.year as i64 * 12 + (self.month as i64 - 1) + months;
        let y = total.div_euclid(12);
        let m = total.rem_euclid(12) + 1;
        (y as i32, m as u32)
    }
}

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

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

pub fn naive_to_unix(n: NaiveDateTime) -> i64 {
    n.days() * 86_400 + n.hour as i64 * 3_600 + n.minute as i64 * 60 + n.second as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{agenda, parse_calendar};

    #[test]
    fn is_leap_knows_century_and_four_hundred_rules() {
        assert!(is_leap(2000));
        assert!(!is_leap(1900));
        assert!(!is_leap(2100));
        assert!(is_leap(2024));
        assert!(!is_leap(2026));
    }

    #[test]
    fn days_from_civil_epoch_and_reference_points() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    }

    #[test]
    fn civil_from_days_round_trips_every_997th_day() {
        let mut z = -800_000i64;
        while z <= 800_000 {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z, "round-trip failed at day {z}");
            z += 997;
        }
    }

    #[test]
    fn weekday_of_2026_10_30_is_friday() {
        let d = NaiveDateTime::date(2026, 10, 30).unwrap();
        assert_eq!(d.weekday_number(), 4, "0=Mon..6=Sun, Friday=4");
    }

    #[test]
    fn year_2100_has_no_february_29() {
        // 2100 is a century non-leap year: February has 28 days.
        let feb28 = NaiveDateTime::date(2100, 2, 28).unwrap();
        let mar1 = feb28.add_days(1);
        assert_eq!((mar1.year, mar1.month, mar1.day), (2100, 3, 1));
        assert!(NaiveDateTime::date(2100, 2, 29).is_none());
    }

    #[test]
    fn yearly_feb29_rule_starting_2096_has_no_instance_in_2100() {
        let c = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//x//EN\r\n\
                  BEGIN:VEVENT\r\nUID:u1\r\nDTSTART:20960229T100000Z\r\n\
                  RRULE:FREQ=YEARLY\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let (events, warnings) = parse_calendar(c).unwrap();
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        let ws = days_from_civil(2100, 1, 1) * 86_400;
        let we = days_from_civil(2101, 1, 1) * 86_400;
        let inst = agenda(&events, ws, we, 0, &|_, _| None);
        assert!(
            inst.is_empty(),
            "expected no Feb 29 instance in 2100: {inst:?}"
        );
    }
}
