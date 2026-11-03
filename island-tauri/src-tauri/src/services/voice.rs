//! Shared TTS mute switch (`~/Hermes/Voices/tts_mute.py`), same argv/paths as island-rust.

use super::process::run_bounded;
use crate::{
    contract::ai::{VoiceOp, VoicePanel, VoicePanelOp, VoiceState, VoiceStatus},
    error::IslandError,
    integrations::tiles::core::{
        is_valid_voice_name, parse_current, parse_voice_list, switch_argv, VoiceList,
        MAX_VOICE_BYTES,
    },
};
use std::{path::PathBuf, process::Command, time::Duration};

pub struct VoiceService {
    script: PathBuf,
    python: PathBuf,
    home: PathBuf,
    /// Test instances resolve `hermes-fish` from their scratch home only, never PATH.
    isolated: bool,
}

impl VoiceService {
    pub fn new(home: &std::path::Path) -> Self {
        Self::with_python(home, PathBuf::from("/usr/bin/python3"))
    }

    pub fn isolated(home: &std::path::Path, isolated: bool) -> Self {
        Self {
            isolated,
            ..Self::new(home)
        }
    }

    pub fn with_python(home: &std::path::Path, python: PathBuf) -> Self {
        Self {
            script: home.join("Hermes/Voices/tts_mute.py"),
            python,
            home: home.to_path_buf(),
            isolated: false,
        }
    }

    /// `hermes-fish` on PATH (or `~/.local/bin`), executable regular file only.
    fn fish(&self) -> Option<PathBuf> {
        use std::os::unix::fs::PermissionsExt;
        let mut dirs: Vec<PathBuf> = if self.isolated {
            Vec::new()
        } else {
            std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default()
        };
        dirs.push(self.home.join(".local/bin"));
        dirs.into_iter()
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join("hermes-fish"))
            .find(|p| {
                std::fs::metadata(p)
                    .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            })
    }

    fn fish_text(&self, fish: &std::path::Path, args: &[&str]) -> Result<String, IslandError> {
        let mut command = Command::new(fish);
        command.args(args);
        let bytes = run_bounded(command, Duration::from_secs(4), MAX_VOICE_BYTES, false)?;
        String::from_utf8(bytes).map_err(|_| IslandError::unavailable("Voice output is not UTF-8"))
    }

    fn voices(&self, fish: &std::path::Path) -> Result<VoiceList, IslandError> {
        let text = self.fish_text(fish, &[])?;
        // Two aliases sharing one reference id are both marked `[active]`; that is legal
        // here, `current` below decides which one the picker shows.
        let mut list = parse_voice_list(&text)
            .or_else(|_| parse_voice_list(&text.replace("  [active]", "")))
            .map_err(|_| IslandError::unavailable("Voice list invalid"))?;
        // `current` names the FIRST alias sharing the active reference id; the list may mark
        // several. Prefer `current` so the picker shows one stable selection.
        if let Ok(current) = self.fish_text(fish, &["current"]) {
            if let Ok(name) = parse_current(&current) {
                if list.voices.contains(&name) {
                    list.active = Some(name);
                }
            }
        }
        Ok(list)
    }

    /// Picker + mute state for the Hermes page. Missing helpers are reported, not errors.
    pub fn panel(&self, op: VoicePanelOp) -> Result<VoicePanel, IslandError> {
        let fish = self.fish();
        match &op {
            VoicePanelOp::ToggleMute {} => {
                if !self.script.is_file() {
                    return Err(IslandError::unavailable("TTS mute helper not installed"));
                }
                self.invoke("toggle")?;
            }
            VoicePanelOp::Select { name } => {
                let fish = fish
                    .as_deref()
                    .ok_or_else(|| IslandError::unavailable("hermes-fish not installed"))?;
                if name.starts_with('-') || !is_valid_voice_name(name) {
                    return Err(IslandError::invalid("Voice name invalid"));
                }
                let list = self.voices(fish)?;
                let argv = switch_argv(&list, name)
                    .map_err(|_| IslandError::invalid("Unknown or reserved voice name"))?;
                let args: Vec<&str> = argv.iter().skip(1).map(String::as_str).collect();
                self.fish_text(fish, &args)?;
            }
            VoicePanelOp::Refresh {} => {}
        }
        let list = fish.as_deref().map(|fish| self.voices(fish));
        if let (VoicePanelOp::Select { name }, Some(Ok(list))) = (&op, &list) {
            if list.active.as_deref() != Some(name.as_str()) {
                return Err(IslandError::unavailable("Voice switch was not confirmed"));
            }
        }
        let (voices_available, active, choices) = match list {
            Some(Ok(list)) => (true, list.active, list.voices),
            _ => (false, None, Vec::new()),
        };
        let muted = if self.script.is_file() {
            self.run(VoiceOp::Status)
                .ok()
                .map(|s| s.state == VoiceState::Muted)
        } else {
            None
        };
        Ok(VoicePanel {
            voices_available,
            active,
            choices,
            mute_available: muted.is_some(),
            muted: muted.unwrap_or(false),
        })
    }

    fn invoke(&self, op: &str) -> Result<Vec<u8>, IslandError> {
        let mut command = Command::new(&self.python);
        command.arg(&self.script).arg(op);
        run_bounded(command, Duration::from_secs(3), 16384, false)
    }

    pub fn run(&self, op: VoiceOp) -> Result<VoiceStatus, IslandError> {
        if op == VoiceOp::Toggle {
            self.invoke("toggle")?;
        }
        let bytes = self.invoke("status")?;
        let unknown = || IslandError::unavailable("Unknown shared voice status");
        let state = match std::str::from_utf8(&bytes).map_err(|_| unknown())?.trim() {
            "muted" => VoiceState::Muted,
            "unmuted" => VoiceState::Unmuted,
            _ => return Err(unknown()),
        };
        Ok(VoiceStatus { state })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::scratch;
    use std::fs;

    #[test]
    fn status_and_toggle_parse_the_shared_state_only() {
        let dir = scratch("voice");
        fs::create_dir_all(dir.join("Hermes/Voices")).unwrap();
        // Records every op it is called with, so the test proves toggle -> status ordering.
        fs::write(
            dir.join("Hermes/Voices/tts_mute.py"),
            "import sys,os\nlog=os.path.join(os.path.dirname(__file__),'ops')\nopen(log,'a').write(sys.argv[1]+'\\n')\nprint('unmuted' if sys.argv[1]=='toggle' else 'muted')\n",
        )
        .unwrap();
        let voice = VoiceService::new(&dir);
        assert_eq!(voice.run(VoiceOp::Status).unwrap().state, VoiceState::Muted);
        assert_eq!(voice.run(VoiceOp::Toggle).unwrap().state, VoiceState::Muted);
        assert_eq!(
            fs::read_to_string(dir.join("Hermes/Voices/ops")).unwrap(),
            "status\ntoggle\nstatus\n"
        );
        fs::write(dir.join("Hermes/Voices/tts_mute.py"), "print('loud')").unwrap();
        assert!(voice.run(VoiceOp::Status).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn panel_lists_switches_and_reports_missing_helpers() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("voice-panel");
        let voice = VoiceService::isolated(&dir, true);
        // Neither helper installed: everything unavailable, never an error.
        let empty = voice.panel(VoicePanelOp::Refresh {}).unwrap();
        assert!(!empty.voices_available && !empty.mute_available && empty.choices.is_empty());
        assert!(voice.panel(VoicePanelOp::ToggleMute {}).is_err());
        let bin = dir.join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        let state = dir.join("active");
        fs::write(&state, "alpha").unwrap();
        let script = format!(
            "#!/bin/sh\nS='{}'\ncase \"$1\" in\n '') a=$(cat \"$S\"); echo 'Fish Audio voices:'; echo; for v in alpha beta; do if [ \"$v\" = \"$a\" ]; then echo \"  $v  [active]\"; else echo \"  $v\"; fi; done;;\n current) cat \"$S\"; echo;;\n alpha|beta) printf %s \"$1\" > \"$S\";;\n *) exit 1;;\nesac\n",
            state.display()
        );
        fs::write(bin.join("hermes-fish"), script).unwrap();
        fs::set_permissions(bin.join("hermes-fish"), fs::Permissions::from_mode(0o755)).unwrap();
        let listed = voice.panel(VoicePanelOp::Refresh {}).unwrap();
        assert_eq!(listed.choices, ["alpha", "beta"]);
        assert_eq!(listed.active.as_deref(), Some("alpha"));
        let switched = voice
            .panel(VoicePanelOp::Select {
                name: "beta".into(),
            })
            .unwrap();
        assert_eq!(switched.active.as_deref(), Some("beta"));
        for bad in ["gamma", "current", "-x", "a b"] {
            assert!(voice
                .panel(VoicePanelOp::Select { name: bad.into() })
                .is_err());
        }
        assert_eq!(fs::read_to_string(&state).unwrap(), "beta");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn op_allow_list_is_enforced_by_the_type() {
        assert!(serde_json::from_str::<VoiceOp>("\"execute\"").is_err());
        assert_eq!(
            serde_json::from_str::<VoiceOp>("\"toggle\"").unwrap(),
            VoiceOp::Toggle
        );
    }
}
