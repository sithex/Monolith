//! Shared TTS mute switch (`~/Hermes/Voices/tts_mute.py`), same argv/paths as island-rust.

use super::process::run_bounded;
use crate::{
    contract::ai::{VoiceOp, VoiceState, VoiceStatus},
    error::IslandError,
};
use std::{path::PathBuf, process::Command, time::Duration};

pub struct VoiceService {
    script: PathBuf,
    python: PathBuf,
}

impl VoiceService {
    pub fn new(home: &std::path::Path) -> Self {
        Self::with_python(home, PathBuf::from("/usr/bin/python3"))
    }

    pub fn with_python(home: &std::path::Path, python: PathBuf) -> Self {
        Self {
            script: home.join("Hermes/Voices/tts_mute.py"),
            python,
        }
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
    fn op_allow_list_is_enforced_by_the_type() {
        assert!(serde_json::from_str::<VoiceOp>("\"execute\"").is_err());
        assert_eq!(
            serde_json::from_str::<VoiceOp>("\"toggle\"").unwrap(),
            VoiceOp::Toggle
        );
    }
}
