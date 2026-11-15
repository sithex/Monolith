//! Autoclicker, pure part: defaults, validation, the saved-file format (settings and
//! lifetime stats) and interval jitter.
use super::keys;
use crate::contract::apps::{AutoclickerConfig, ClickButton, ClickKind, ClickTarget, TriggerMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MIN_INTERVAL_MS: u32 = 1;
pub const MAX_INTERVAL_MS: u32 = 3_600_000;
pub const MAX_JITTER_PERCENT: u32 = 50;
pub const MAX_CLICK_LIMIT: u32 = 10_000_000;
pub const MAX_TIME_LIMIT_S: u32 = 86_400;
pub const MAX_START_DELAY_S: u32 = 10;
pub const MAX_HOTKEY_BYTES: usize = 64;
pub const MAX_PRESS_MS: u32 = 1000;
pub const MAX_BURST: u32 = 1000;

pub fn defaults() -> AutoclickerConfig {
    AutoclickerConfig {
        interval_ms: 100,
        jitter_percent: 0,
        target: ClickTarget::Mouse,
        button: ClickButton::Left,
        key: "Space".into(),
        kind: ClickKind::Single,
        press_ms: 0,
        mode: TriggerMode::Toggle,
        burst_count: 10,
        click_limit: 0,
        time_limit_s: 0,
        start_delay_s: 3,
        hotkey: "F8".into(),
        stop_hotkey: "Shift+Esc".into(),
    }
}

fn bindable(text: &str) -> Result<Option<i32>, ()> {
    if text.len() > MAX_HOTKEY_BYTES {
        return Err(());
    }
    crate::keyseq::parse(text).map_err(|_| ())
}

/// Every field in range, hotkeys KGlobalAccel can bind (or `""`), and a pressed key that is
/// neither hotkey (pressing it would trigger the autoclicker itself).
pub fn validate(c: &AutoclickerConfig) -> Result<(), &'static str> {
    if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&c.interval_ms) {
        return Err("Interval must be between 1 ms and 1 hour");
    }
    if c.jitter_percent > MAX_JITTER_PERCENT {
        return Err("Randomness must be 0–50 %");
    }
    if c.press_ms > MAX_PRESS_MS {
        return Err("Press length must be 0–1000 ms");
    }
    if !(1..=MAX_BURST).contains(&c.burst_count) {
        return Err("Burst must be 1–1000 clicks");
    }
    if c.click_limit > MAX_CLICK_LIMIT {
        return Err("Click limit is too large");
    }
    if c.time_limit_s > MAX_TIME_LIMIT_S {
        return Err("Time limit must be at most 24 hours");
    }
    if c.start_delay_s > MAX_START_DELAY_S {
        return Err("Start delay must be 0–10 s");
    }
    let Ok(hotkey) = bindable(&c.hotkey) else {
        return Err("KDE can't bind that hotkey");
    };
    let Ok(stop) = bindable(&c.stop_hotkey) else {
        return Err("KDE can't bind that stop hotkey");
    };
    if hotkey.is_some() && hotkey == stop {
        return Err("The stop hotkey must differ from the hotkey");
    }
    let Ok(key) = bindable(&c.key) else {
        return Err("The virtual keyboard can't press that key");
    };
    if c.target == ClickTarget::Key {
        if keys::chord(&c.key).is_none() {
            return Err("The virtual keyboard can't press that key");
        }
        if key.is_some() && (key == hotkey || key == stop) {
            return Err("The pressed key can't be one of the hotkeys");
        }
    }
    Ok(())
}

/// The saved file: settings and lifetime totals.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub config: AutoclickerConfig,
    pub lifetime_clicks: u64,
    pub lifetime_ms: u64,
}

impl Default for Saved {
    fn default() -> Self {
        Self {
            config: defaults(),
            lifetime_clicks: 0,
            lifetime_ms: 0,
        }
    }
}

/// A config from older or partial JSON: known fields over the defaults (fields added since
/// keep their defaults, unknown ones are dropped); `None` when it is out of range.
fn config_from(value: &Value) -> Option<AutoclickerConfig> {
    let Value::Object(given) = value else {
        return None;
    };
    let mut merged = serde_json::to_value(defaults()).ok()?;
    if let Value::Object(base) = &mut merged {
        for (k, v) in given {
            if base.contains_key(k) {
                base.insert(k.clone(), v.clone());
            }
        }
    }
    serde_json::from_value::<AutoclickerConfig>(merged)
        .ok()
        .filter(|c| validate(c).is_ok())
}

/// Reads the saved file. A bare config (the 5.2.0 format) is accepted; anything unreadable
/// or out of range falls back to the defaults.
pub fn load(text: &str) -> Saved {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Saved::default();
    };
    let Some(config_value) = value.get("config").filter(|v| v.is_object()) else {
        return Saved {
            config: config_from(&value).unwrap_or_else(defaults),
            ..Saved::default()
        };
    };
    let count = |key| value.get(key).and_then(Value::as_u64).unwrap_or(0);
    Saved {
        config: config_from(config_value).unwrap_or_else(defaults),
        lifetime_clicks: count("lifetimeClicks"),
        lifetime_ms: count("lifetimeMs"),
    }
}

pub fn store(saved: &Saved) -> String {
    serde_json::to_string_pretty(saved).unwrap_or_default()
}

/// The config part alone (older callers and tests).
pub fn from_json(text: &str) -> AutoclickerConfig {
    load(text).config
}

pub fn to_json(c: &AutoclickerConfig) -> String {
    serde_json::to_string_pretty(c).unwrap_or_default()
}

/// xorshift64*: cheap, good enough to humanise intervals, never used for anything secret.
pub struct Jitter(u64);
impl Jitter {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// `interval_ms` spread uniformly by ±`percent`, in microseconds, never below 1 ms.
    pub fn interval_us(&mut self, interval_ms: u32, percent: u32) -> u64 {
        let base = u64::from(interval_ms) * 1000;
        if percent == 0 {
            return base;
        }
        let spread = base * u64::from(percent.min(MAX_JITTER_PERCENT)) / 100;
        let offset = self.next() % (2 * spread + 1);
        (base + offset).saturating_sub(spread).max(1000)
    }
}
