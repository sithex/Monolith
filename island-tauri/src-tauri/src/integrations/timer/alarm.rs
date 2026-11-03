//! Audible timer completion. Plays a freedesktop sound-theme file through PipeWire
//! (`pw-play`, falling back to `paplay`) in its own process group, so dismissing the
//! finished timer can cut a still-ringing alarm. Missing players/sounds are silent no-ops.
use std::{
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
};

const SOUND_DIRS: [&str; 2] = [
    "/usr/share/sounds/freedesktop/stereo",
    "/usr/local/share/sounds/freedesktop/stereo",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chime {
    /// Countdown finished: the longer alarm-clock ring.
    Alarm,
    /// Pomodoro phase change: a short completion blip.
    Phase,
}

impl Chime {
    fn names(self) -> &'static [&'static str] {
        match self {
            Chime::Alarm => &["alarm-clock-elapsed.oga", "complete.oga", "bell.oga"],
            Chime::Phase => &["complete.oga", "bell.oga"],
        }
    }
}

pub fn sound_file(chime: Chime, dirs: &[PathBuf]) -> Option<PathBuf> {
    chime
        .names()
        .iter()
        .flat_map(|name| dirs.iter().map(move |dir| dir.join(name)))
        .find(|p| p.is_file())
}

#[derive(Default)]
pub struct AlarmPlayer {
    child: Mutex<Option<Child>>,
}

fn spawn(player: &str, file: &Path) -> std::io::Result<Child> {
    let mut command = Command::new(player);
    command
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe and touches no Rust shared state.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    command.spawn()
}

impl AlarmPlayer {
    pub fn play(&self, chime: Chime) {
        let dirs: Vec<PathBuf> = SOUND_DIRS.iter().map(PathBuf::from).collect();
        let Some(file) = sound_file(chime, &dirs) else {
            tracing::debug!("No timer sound file installed");
            return;
        };
        self.stop();
        let child = spawn("pw-play", &file).or_else(|_| spawn("paplay", &file));
        match child {
            Ok(child) => {
                if let Ok(mut slot) = self.child.lock() {
                    *slot = Some(child);
                }
            }
            Err(error) => tracing::debug!(%error, "Timer sound player unavailable"),
        }
    }

    /// Cut a still-ringing alarm (and reap a finished one).
    pub fn stop(&self) {
        let Ok(mut slot) = self.child.lock() else {
            return;
        };
        if let Some(mut child) = slot.take() {
            if matches!(child.try_wait(), Ok(None)) {
                // SAFETY: our unreaped child leads its own session; its PGID cannot be reused.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGTERM);
                }
            }
            if let Err(error) = child.wait() {
                tracing::debug!(%error, "Timer sound wait failed");
            }
        }
    }
}

impl Drop for AlarmPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_alarm_ring_and_falls_back_in_order() {
        let dir = std::env::temp_dir().join(format!("island-alarm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dirs = vec![dir.clone()];
        assert_eq!(sound_file(Chime::Alarm, &dirs), None);
        std::fs::write(dir.join("bell.oga"), b"x").unwrap();
        assert_eq!(sound_file(Chime::Alarm, &dirs), Some(dir.join("bell.oga")));
        std::fs::write(dir.join("complete.oga"), b"x").unwrap();
        assert_eq!(
            sound_file(Chime::Phase, &dirs),
            Some(dir.join("complete.oga"))
        );
        std::fs::write(dir.join("alarm-clock-elapsed.oga"), b"x").unwrap();
        assert_eq!(
            sound_file(Chime::Alarm, &dirs),
            Some(dir.join("alarm-clock-elapsed.oga"))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
