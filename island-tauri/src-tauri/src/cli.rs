//! Command line (SPEC §2.6). Live mode resolves XDG configuration and state directories.
//!
//! `hermes-island [--overlay] --test-instance --data-dir PATH [--ui harness|legacy] [--fixture-pty]`
//!
//! Data overrides are test-only. `--fixture-pty` (bash fixture instead of Hermes) requires `--test-instance` AND the
//! environment variable `HERMES_ISLAND_TEST=1`.

use crate::{contract::island::UiKind, error::IslandError, window::spike::SpikeConfig};
use std::path::PathBuf;

pub const TEST_ENV: &str = "HERMES_ISLAND_TEST";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cli {
    /// A native Plasma pill owns Still: the popup window shows nothing and is click-through.
    pub overlay: bool,
    pub test_instance: bool,
    pub data_dir: Option<PathBuf>,
    /// Spawn the `bash --norc` fixture instead of Hermes (tests only).
    pub fixture_pty: bool,
    /// Which page the island window loads; `harness` keeps the Phase 1/1.5 tests unchanged.
    pub ui: UiKind,
    /// SPEC §8 Phase 2.5 panel-overlap spike (tests only; `None` = production window).
    pub spike: Option<SpikeConfig>,
}

impl Cli {
    /// `args` excludes `argv[0]`; reads `HERMES_ISLAND_TEST` from the environment.
    pub fn parse_from<I, S>(args: I) -> Result<Self, IslandError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::parse_with_env(args, std::env::var(TEST_ENV).ok().as_deref())
    }

    /// Pure parser: the value of `HERMES_ISLAND_TEST` is passed in.
    pub fn parse_with_env<I, S>(args: I, test_env: Option<&str>) -> Result<Self, IslandError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut cli = Cli::default();
        let mut args = args.into_iter().map(Into::into);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--overlay" => cli.overlay = true,
                "--test-instance" => cli.test_instance = true,
                "--data-dir" => {
                    let value = args
                        .next()
                        .ok_or_else(|| IslandError::invalid("--data-dir needs a path"))?;
                    cli.set_data_dir(value)?;
                }
                "--fixture-pty" => cli.fixture_pty = true,
                "--ui" => {
                    let value = args
                        .next()
                        .ok_or_else(|| IslandError::invalid("--ui needs harness or legacy"))?;
                    cli.ui = parse_ui(&value)?;
                }
                "--spike-panel" => {
                    let value = args
                        .next()
                        .ok_or_else(|| IslandError::invalid("--spike-panel needs a spec"))?;
                    cli.set_spike(&value)?;
                }
                "notify" => {
                    return Err(IslandError::unsupported(
                        "the notify subcommand arrives in Phase 4",
                    ))
                }
                other => {
                    if let Some(value) = other.strip_prefix("--data-dir=") {
                        cli.set_data_dir(value.to_owned())?;
                    } else if let Some(value) = other.strip_prefix("--ui=") {
                        cli.ui = parse_ui(value)?;
                    } else if let Some(value) = other.strip_prefix("--spike-panel=") {
                        cli.set_spike(value)?;
                    } else {
                        return Err(IslandError::invalid(format!("unknown argument `{other}`")));
                    }
                }
            }
        }
        cli.validate(test_env)?;
        Ok(cli)
    }

    fn set_spike(&mut self, spec: &str) -> Result<(), IslandError> {
        if self.spike.is_some() {
            return Err(IslandError::invalid(
                "--spike-panel must be given at most once",
            ));
        }
        self.spike = Some(SpikeConfig::parse(spec)?);
        Ok(())
    }

    fn set_data_dir(&mut self, value: String) -> Result<(), IslandError> {
        if value.is_empty() || self.data_dir.is_some() {
            return Err(IslandError::invalid(
                "--data-dir must be given exactly once and not be empty",
            ));
        }
        self.data_dir = Some(PathBuf::from(value));
        Ok(())
    }

    fn validate(&self, test_env: Option<&str>) -> Result<(), IslandError> {
        if self.spike.is_some() {
            if !self.test_instance {
                return Err(IslandError::invalid(
                    "--spike-panel requires --test-instance",
                ));
            }
            if test_env != Some("1") {
                return Err(IslandError::unsupported(
                    "--spike-panel requires HERMES_ISLAND_TEST=1 in the environment",
                ));
            }
            if !self.overlay {
                return Err(IslandError::invalid(
                    "--spike-panel needs --overlay (the native-pill Still region)",
                ));
            }
        }
        if self.fixture_pty {
            if !self.test_instance {
                return Err(IslandError::invalid(
                    "--fixture-pty requires --test-instance",
                ));
            }
            if test_env != Some("1") {
                return Err(IslandError::unsupported(
                    "--fixture-pty requires HERMES_ISLAND_TEST=1 in the environment",
                ));
            }
        }
        match (self.test_instance, &self.data_dir) {
            (true, Some(_)) => Ok(()),
            (true, None) => Err(IslandError::invalid(
                "--test-instance requires --data-dir (it never adopts a live data directory)",
            )),
            (false, Some(_)) => Err(IslandError::invalid(
                "--data-dir is only accepted together with --test-instance",
            )),
            (false, None) => Ok(()),
        }
    }
}

fn parse_ui(value: &str) -> Result<UiKind, IslandError> {
    match value {
        "harness" => Ok(UiKind::Harness),
        "legacy" => Ok(UiKind::Legacy),
        "island" => Ok(UiKind::Island),
        other => Err(IslandError::invalid(format!(
            "unknown --ui `{other}` (harness or legacy)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    fn parse(args: &[&str]) -> Result<Cli, IslandError> {
        Cli::parse_from(args.iter().copied())
    }

    #[test]
    fn accepts_test_instance_with_data_dir_in_both_spellings() {
        let a = parse(&["--test-instance", "--data-dir", "/x/y"]).unwrap();
        let b = parse(&["--overlay", "--data-dir=/x/y", "--test-instance"]).unwrap();
        assert_eq!(a.data_dir, Some(PathBuf::from("/x/y")));
        assert!(!a.overlay && b.overlay && a.test_instance && b.test_instance);
    }

    #[test]
    fn refuses_incomplete_test_or_live_directory_overrides() {
        for args in [&["--test-instance"][..], &["--data-dir", "/x"]] {
            assert!(parse(args).is_err(), "{args:?}");
        }
        assert!(!parse(&[]).unwrap().test_instance);
        assert!(parse(&["--overlay"]).unwrap().overlay);
    }

    #[test]
    fn rejects_unknown_empty_and_duplicate_arguments() {
        assert_eq!(parse(&["--bogus"]).unwrap_err().code, ErrorCode::Invalid);
        assert!(parse(&["--test-instance", "--data-dir"]).is_err());
        assert!(parse(&["--test-instance", "--data-dir="]).is_err());
        assert!(parse(&["--test-instance", "--data-dir", "/a", "--data-dir", "/b"]).is_err());
    }

    #[test]
    fn later_phase_features_are_explicitly_unsupported() {
        assert_eq!(parse(&["notify"]).unwrap_err().code, ErrorCode::Unsupported);
    }

    fn parse_env(args: &[&str], env: Option<&str>) -> Result<Cli, IslandError> {
        Cli::parse_with_env(args.iter().copied(), env)
    }

    #[test]
    fn fixture_pty_needs_test_instance_and_the_env_gate() {
        let ok = ["--test-instance", "--data-dir", "/x", "--fixture-pty"];
        assert!(parse_env(&ok, Some("1")).unwrap().fixture_pty);
        for env in [None, Some(""), Some("0"), Some("true")] {
            assert_eq!(
                parse_env(&ok, env).unwrap_err().code,
                ErrorCode::Unsupported,
                "{env:?}"
            );
        }
        // Without --test-instance it is refused no matter what the environment says.
        for args in [
            &["--fixture-pty"][..],
            &["--fixture-pty", "--data-dir", "/x"],
        ] {
            assert!(parse_env(args, Some("1")).is_err(), "{args:?}");
        }
        assert!(
            !parse_env(&["--test-instance", "--data-dir", "/x"], None)
                .unwrap()
                .fixture_pty
        );
    }

    #[test]
    fn spike_panel_is_test_only_overlay_only_and_defaults_off() {
        use crate::window::spike::SpikeConfig;
        let base = ["--test-instance", "--data-dir", "/x"];
        assert_eq!(parse_env(&base, Some("1")).unwrap().spike, None);
        let mut ok = base.to_vec();
        ok.extend(["--overlay", "--spike-panel", "above=0,blur=1"]);
        let cli = parse_env(&ok, Some("1")).unwrap();
        assert_eq!(
            cli.spike,
            Some(SpikeConfig::parse("above=0,blur=1").unwrap())
        );
        let mut equals = base.to_vec();
        equals.extend(["--overlay", "--spike-panel=y=0"]);
        assert!(parse_env(&equals, Some("1")).unwrap().spike.is_some());
        // The environment gate: anything but exactly "1" is refused.
        for env in [None, Some(""), Some("0"), Some("true")] {
            assert_eq!(
                parse_env(&ok, env).unwrap_err().code,
                ErrorCode::Unsupported,
                "{env:?}"
            );
        }
        // Without --test-instance, without --overlay, twice, or with a bad spec.
        assert!(parse_env(&["--overlay", "--spike-panel", ""], Some("1")).is_err());
        let mut no_overlay = base.to_vec();
        no_overlay.extend(["--spike-panel", ""]);
        assert!(parse_env(&no_overlay, Some("1")).is_err());
        let mut twice = ok.clone();
        twice.extend(["--spike-panel", ""]);
        assert!(parse_env(&twice, Some("1")).is_err());
        let mut bad = base.to_vec();
        bad.extend(["--overlay", "--spike-panel", "type=dock"]);
        assert_eq!(
            parse_env(&bad, Some("1")).unwrap_err().code,
            ErrorCode::Invalid
        );
        let mut missing = base.to_vec();
        missing.extend(["--overlay", "--spike-panel"]);
        assert!(parse_env(&missing, Some("1")).is_err());
    }

    #[test]
    fn ui_selection_defaults_to_the_harness() {
        use crate::contract::island::UiKind;
        let base = ["--test-instance", "--data-dir", "/x"];
        assert_eq!(parse_env(&base, None).unwrap().ui, UiKind::Harness);
        let mut legacy = base.to_vec();
        legacy.extend(["--ui", "legacy"]);
        assert_eq!(parse_env(&legacy, None).unwrap().ui, UiKind::Legacy);
        let mut equals = base.to_vec();
        equals.push("--ui=harness");
        assert_eq!(parse_env(&equals, None).unwrap().ui, UiKind::Harness);
        for bad in [&["--ui"][..], &["--ui", "html"], &["--ui=Legacy"]] {
            let mut args = base.to_vec();
            args.extend_from_slice(bad);
            assert_eq!(
                parse_env(&args, None).unwrap_err().code,
                ErrorCode::Invalid,
                "{bad:?}"
            );
        }
    }
}
