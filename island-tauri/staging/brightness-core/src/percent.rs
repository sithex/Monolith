//! The [`Display`] record and percent <-> raw conversions.

use crate::error::ValueError;
use serde::{Deserialize, Serialize};

/// Highest representable brightness percent.
pub const MAX_PERCENT: u8 = 100;

/// One display as reported by PowerDevil's ScreenBrightness interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    pub name: String,
    pub label: String,
    pub is_internal: bool,
    pub max_brightness: i32,
    pub brightness: i32,
}

/// `round(b × 100 / max)` half up, computed in i64; `None` when
/// `max <= 0`. `brightness` is clamped to `0..=max` first.
pub(crate) fn percent_of(max: i32, brightness: i32) -> Option<u8> {
    if max <= 0 {
        return None;
    }
    let clamped = brightness.clamp(0, max);
    let pct = (clamped as i64 * 100 + max as i64 / 2) / max as i64;
    Some(pct as u8)
}

/// Brightness percent of a display; `None` when unavailable (`max <= 0`).
pub fn percent(display: &Display) -> Option<u8> {
    percent_of(display.max_brightness, display.brightness)
}

/// `round(pct × max / 100)` half up, computed in i64, for a display.
pub fn raw_for_percent(display: &Display, pct: u8) -> Result<i32, ValueError> {
    raw_for_percent_of(display.max_brightness, pct)
}

/// Raw brightness for a percent against a max; the model relies on this
/// being `Ok` when `max > 0 && pct <= 100`.
pub(crate) fn raw_for_percent_of(max: i32, pct: u8) -> Result<i32, ValueError> {
    if pct > MAX_PERCENT {
        return Err(ValueError::PercentOutOfRange);
    }
    if max <= 0 {
        return Err(ValueError::MaxBrightnessNotPositive);
    }
    let raw = (pct as i64 * max as i64 + 50) / 100;
    Ok(raw as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_of_boundaries() {
        // max 10000
        assert_eq!(percent_of(10000, 0), Some(0));
        assert_eq!(percent_of(10000, 50), Some(1)); // 0.5% half up
        assert_eq!(percent_of(10000, 49), Some(0)); // 0.49% -> 0
        assert_eq!(percent_of(10000, 9950), Some(100)); // 99.5% half up
        assert_eq!(percent_of(10000, 10000), Some(100));
        // max 1
        assert_eq!(percent_of(1, 0), Some(0));
        assert_eq!(percent_of(1, 1), Some(100));
        // clamp
        assert_eq!(percent_of(10000, -5), Some(0));
        assert_eq!(percent_of(10000, 10005), Some(100));
        // max i32::MAX without overflow
        assert_eq!(percent_of(i32::MAX, 0), Some(0));
        assert_eq!(percent_of(i32::MAX, i32::MAX), Some(100));
        assert_eq!(percent_of(i32::MAX, i32::MAX / 2), Some(50));
        // unavailable
        assert_eq!(percent_of(0, 5), None);
        assert_eq!(percent_of(-1, 5), None);
    }

    #[test]
    fn raw_for_percent_boundaries() {
        let d = |max| Display {
            name: "d".to_string(),
            label: String::new(),
            is_internal: false,
            max_brightness: max,
            brightness: 0,
        };
        assert_eq!(raw_for_percent(&d(10000), 0), Ok(0));
        assert_eq!(raw_for_percent(&d(10000), 1), Ok(100));
        assert_eq!(raw_for_percent(&d(10000), 50), Ok(5000));
        assert_eq!(raw_for_percent(&d(10000), 100), Ok(10000));

        assert_eq!(raw_for_percent(&d(255), 1), Ok(3));
        assert_eq!(raw_for_percent(&d(255), 50), Ok(128));
        assert_eq!(raw_for_percent(&d(255), 100), Ok(255));

        assert_eq!(
            raw_for_percent(&d(10000), 101),
            Err(ValueError::PercentOutOfRange)
        );
        assert_eq!(
            raw_for_percent(&d(10000), 200),
            Err(ValueError::PercentOutOfRange)
        );
        assert_eq!(
            raw_for_percent(&d(0), 50),
            Err(ValueError::MaxBrightnessNotPositive)
        );
        assert_eq!(
            raw_for_percent(&d(-1), 50),
            Err(ValueError::MaxBrightnessNotPositive)
        );
        assert_eq!(raw_for_percent(&d(i32::MAX), 100), Ok(i32::MAX));
        assert_eq!(raw_for_percent(&d(i32::MAX), 50), Ok(1_073_741_824));
        assert_eq!(raw_for_percent(&d(i32::MAX), 0), Ok(0));
    }
}
