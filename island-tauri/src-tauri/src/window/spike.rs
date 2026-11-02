//! SPEC §8 Phase 2.5 — panel-overlap spike switches. **Test only.**
//!
//! `--spike-panel <spec>` selects the stacking / blur experiments the Phase 2.5 GUI test drives
//! on a nested KWin. It is accepted only together with `--test-instance` AND
//! `HERMES_ISLAND_TEST=1` (see `cli.rs`) and defaults off: without the flag no code in this
//! module runs and the window is created exactly as in Phase 1/2.
//!
//! Spec grammar: comma separated `key=value` pairs, every key optional, no duplicates.
//!
//! | key     | values                                         | default   |
//! |---------|------------------------------------------------|-----------|
//! | `type`  | `toolbar` `notification` `utility` `splash`    | `toolbar` |
//! | `above` | `0` `1` (`_NET_WM_STATE_ABOVE` via keep-above) | `1` (as production) |
//! | `blur`  | `0` `1` (`_KDE_NET_WM_BLUR_BEHIND_REGION`)     | `0`       |
//! | `y`     | window top in screen px (`0` = over the panel) | `0`       |

use crate::error::IslandError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpikeWindowType {
    Toolbar,
    Notification,
    Utility,
    Splash,
}

impl SpikeWindowType {
    pub fn name(self) -> &'static str {
        match self {
            Self::Toolbar => "toolbar",
            Self::Notification => "notification",
            Self::Utility => "utility",
            Self::Splash => "splash",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpikeConfig {
    pub window_type: SpikeWindowType,
    /// `false` removes the production keep-above hint (a control for the stacking question).
    pub above: bool,
    /// Mirror the applied input rows into `_KDE_NET_WM_BLUR_BEHIND_REGION`.
    pub blur: bool,
    pub y: i32,
}

impl Default for SpikeConfig {
    fn default() -> Self {
        Self {
            window_type: SpikeWindowType::Toolbar,
            above: true,
            blur: false,
            y: 0,
        }
    }
}

fn flag(key: &str, value: &str) -> Result<bool, IslandError> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        other => Err(IslandError::invalid(format!(
            "--spike-panel {key}= needs 0 or 1, got `{other}`"
        ))),
    }
}

impl SpikeConfig {
    pub fn parse(spec: &str) -> Result<Self, IslandError> {
        let mut config = Self::default();
        let mut seen: Vec<&str> = Vec::new();
        for pair in spec.split(',').filter(|p| !p.is_empty()) {
            let (key, value) = pair.split_once('=').ok_or_else(|| {
                IslandError::invalid(format!("--spike-panel entry `{pair}` is not key=value"))
            })?;
            if seen.contains(&key) {
                return Err(IslandError::invalid(format!(
                    "--spike-panel key `{key}` given twice"
                )));
            }
            seen.push(key);
            match key {
                "type" => {
                    config.window_type = match value {
                        "toolbar" => SpikeWindowType::Toolbar,
                        "notification" => SpikeWindowType::Notification,
                        "utility" => SpikeWindowType::Utility,
                        "splash" => SpikeWindowType::Splash,
                        other => {
                            return Err(IslandError::invalid(format!(
                            "--spike-panel type=`{other}` (toolbar, notification, utility, splash)"
                        )))
                        }
                    }
                }
                "above" => config.above = flag(key, value)?,
                "blur" => config.blur = flag(key, value)?,
                "y" => {
                    config.y = value
                        .parse::<i32>()
                        .ok()
                        .filter(|y| (0..=2000).contains(y))
                        .ok_or_else(|| {
                            IslandError::invalid(format!(
                                "--spike-panel y=`{value}` must be an integer 0..=2000"
                            ))
                        })?;
                }
                other => {
                    return Err(IslandError::invalid(format!(
                        "unknown --spike-panel key `{other}`"
                    )))
                }
            }
        }
        Ok(config)
    }
}

/// The page the spike loads (a separate Vite entry; never `ui/legacy`).
pub const SPIKE_PAGE: &str = "spike.html";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn empty_spec_is_the_production_stacking_over_the_panel() {
        let c = SpikeConfig::parse("").unwrap();
        assert_eq!(c, SpikeConfig::default());
        assert_eq!(c.window_type, SpikeWindowType::Toolbar);
        assert!(c.above && !c.blur && c.y == 0);
    }

    #[test]
    fn every_key_is_parsed() {
        let c = SpikeConfig::parse("type=notification,above=0,blur=1,y=44").unwrap();
        assert_eq!(
            c,
            SpikeConfig {
                window_type: SpikeWindowType::Notification,
                above: false,
                blur: true,
                y: 44
            }
        );
        for (name, ty) in [
            ("toolbar", SpikeWindowType::Toolbar),
            ("notification", SpikeWindowType::Notification),
            ("utility", SpikeWindowType::Utility),
            ("splash", SpikeWindowType::Splash),
        ] {
            let parsed = SpikeConfig::parse(&format!("type={name}")).unwrap();
            assert_eq!(parsed.window_type, ty);
            assert_eq!(ty.name(), name);
        }
    }

    #[test]
    fn bad_specs_are_rejected_not_ignored() {
        for bad in [
            "type=dock",
            "above=yes",
            "blur=2",
            "y=-1",
            "y=2001",
            "y=abc",
            "bogus=1",
            "above",
            "above=1,above=0",
        ] {
            let error = SpikeConfig::parse(bad).unwrap_err();
            assert_eq!(error.code, ErrorCode::Invalid, "{bad}");
        }
    }
}
