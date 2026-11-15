//! Autoclicker, pure part: defaults, validation, the saved-file format and interval jitter.
use crate::contract::apps::{AutoclickerConfig, ClickButton, ClickKind, TriggerMode};

pub const MIN_INTERVAL_MS: u32 = 1;
pub const MAX_INTERVAL_MS: u32 = 3_600_000;
pub const MAX_JITTER_PERCENT: u32 = 50;
pub const MAX_CLICK_LIMIT: u32 = 10_000_000;
pub const MAX_TIME_LIMIT_S: u32 = 86_400;
pub const MAX_START_DELAY_S: u32 = 10;
pub const MAX_HOTKEY_BYTES: usize = 64;

pub fn defaults() -> AutoclickerConfig {
    AutoclickerConfig {
        interval_ms: 100,
        jitter_percent: 0,
        button: ClickButton::Left,
        kind: ClickKind::Single,
        mode: TriggerMode::Toggle,
        click_limit: 0,
        time_limit_s: 0,
        start_delay_s: 3,
        hotkey: "F8".into(),
    }
}

/// Every field in range and a hotkey KGlobalAccel can bind (or `""`).
pub fn validate(c: &AutoclickerConfig) -> Result<(), &'static str> {
    if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&c.interval_ms) {
        return Err("Interval must be between 1 ms and 1 hour");
    }
    if c.jitter_percent > MAX_JITTER_PERCENT {
        return Err("Randomness must be 0–50 %");
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
    if c.hotkey.len() > MAX_HOTKEY_BYTES || crate::keyseq::parse(&c.hotkey).is_err() {
        return Err("KDE can't bind that hotkey");
    }
    Ok(())
}

/// The saved file; anything unreadable or out of range falls back to the defaults.
pub fn from_json(text: &str) -> AutoclickerConfig {
    serde_json::from_str::<AutoclickerConfig>(text)
        .ok()
        .filter(|c| validate(c).is_ok())
        .unwrap_or_else(defaults)
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
