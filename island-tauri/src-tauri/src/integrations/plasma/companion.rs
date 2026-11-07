//! Notch companion: the pointer position the notch's little face looks toward.
//!
//! Plasma widgets cannot read the global pointer on Wayland, so a tiny KWin script
//! (`companion-gaze.qml`, loaded only while the companion is on) sends it here as a
//! `Pointer(x, y)` call, throttled in KWin to 20 Hz and only while the pointer moves.
//! The island re-broadcasts it as the `Gaze(x, y)` signal, which every notch widget
//! receives with Plasma's D-Bus `SignalWatcher` and turns into eye movement itself.
//! Nothing here polls, and nothing runs while the pointer is still.
use crate::services::process::run_bounded;
use anyhow::{Context, Result};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    process::Command,
    sync::Mutex,
    time::{Duration, Instant},
};
use zbus::object_server::SignalEmitter;

pub const BUS_NAME: &str = "local.hermes.Monolith";
pub const OBJECT_PATH: &str = "/Companion";
pub const INTERFACE: &str = "local.hermes.Monolith.Companion";
/// The KWin plugin name the island owns; only this name is ever unloaded.
pub const SCRIPT_NAME: &str = "monolith-companion-gaze";
const SCRIPT: &str = include_str!("companion-gaze.qml");
/// Faster calls than this are dropped (KWin sends at most one per 50 ms).
const MIN_INTERVAL: Duration = Duration::from_millis(12);
/// Screen coordinates beyond this are nonsense, not a pointer.
const MAX_COORDINATE: f64 = 1_000_000.0;

/// A plausible global pointer position, rounded to whole logical pixels.
pub fn clean_point(x: f64, y: f64) -> Option<(f64, f64)> {
    (x.is_finite() && y.is_finite() && x.abs() <= MAX_COORDINATE && y.abs() <= MAX_COORDINATE)
        .then(|| (x.round(), y.round()))
}

struct Gaze {
    last: Mutex<Option<Instant>>,
}

impl Gaze {
    fn admit(&self, now: Instant) -> bool {
        let Ok(mut last) = self.last.lock() else {
            return false;
        };
        if last.is_some_and(|t| now.duration_since(t) < MIN_INTERVAL) {
            return false;
        }
        *last = Some(now);
        true
    }
}

#[zbus::interface(name = "local.hermes.Monolith.Companion")]
impl Gaze {
    /// The pointer moved to (`x`, `y`), global logical pixels. Rebroadcast as `Gaze`.
    async fn pointer(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        x: f64,
        y: f64,
    ) -> zbus::fdo::Result<()> {
        if let Some((x, y)) = clean_point(x, y) {
            if self.admit(Instant::now()) {
                Self::gaze(&emitter, x, y)
                    .await
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
            }
        }
        Ok(())
    }

    #[zbus(signal)]
    async fn gaze(emitter: &SignalEmitter<'_>, x: f64, y: f64) -> zbus::Result<()>;
}

/// The session-bus endpoint; dropping it releases the name.
pub struct CompanionBus {
    _connection: zbus::Connection,
}

impl CompanionBus {
    pub async fn start() -> Result<Self> {
        let build = zbus::connection::Builder::session()
            .name(BUS_NAME)
            .serve_at(
                OBJECT_PATH,
                Gaze {
                    last: Mutex::new(None),
                },
            )
            .build();
        let connection = tokio::time::timeout(Duration::from_secs(3), build)
            .await
            .context("Companion D-Bus start timed out")??;
        Ok(Self {
            _connection: connection,
        })
    }
}

fn kwin(arguments: &[&str]) -> Result<String> {
    let mut command = Command::new(super::qdbus::program());
    command.arg("org.kde.KWin").args(arguments);
    let out = run_bounded(command, Duration::from_secs(3), 4096, false)?;
    Ok(String::from_utf8_lossy(&out).trim().to_owned())
}

/// A private per-user copy of the script (KWin loads scripts from a file path).
fn script_file() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;
    let dir = PathBuf::from(base).join("monolith");
    if !dir.exists() {
        fs::DirBuilder::new().mode(0o700).create(&dir)?;
    }
    let meta = fs::symlink_metadata(&dir)?;
    // SAFETY: geteuid has no preconditions.
    if !meta.is_dir()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.permissions().mode() & 0o077 != 0
    {
        anyhow::bail!("Companion script directory must be private");
    }
    let path = dir.join("companion-gaze.qml");
    let temp = dir.join(format!(".companion-gaze-{}.qml", std::process::id()));
    {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temp)?;
        file.write_all(SCRIPT.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&temp, &path)?;
    Ok(path)
}

/// Unloads the island's own KWin script (only `SCRIPT_NAME`); harmless when absent.
pub fn unload_script() -> Result<()> {
    kwin(&[
        "/Scripting",
        "org.kde.kwin.Scripting.unloadScript",
        SCRIPT_NAME,
    ])?;
    Ok(())
}

/// (Re)loads and starts the pointer script. A copy left by a crashed island is replaced.
pub fn load_script() -> Result<()> {
    let path = script_file()?;
    let path = path
        .to_str()
        .context("Companion script path must be UTF-8")?;
    unload_script()?;
    let id = kwin(&[
        "/Scripting",
        "org.kde.kwin.Scripting.loadDeclarativeScript",
        path,
        SCRIPT_NAME,
    ])?;
    let id: i64 = id.parse().context("KWin returned an invalid script id")?;
    if id < 0 {
        anyhow::bail!("KWin refused the companion script");
    }
    kwin(&[&format!("/Scripting/Script{id}"), "org.kde.kwin.Script.run"])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plausible_points_are_rebroadcast() {
        assert_eq!(clean_point(3200.4, 11.6), Some((3200.0, 12.0)));
        assert_eq!(clean_point(-1920.0, 0.0), Some((-1920.0, 0.0)));
        assert_eq!(clean_point(f64::NAN, 1.0), None);
        assert_eq!(clean_point(1.0, f64::INFINITY), None);
        assert_eq!(clean_point(2e6, 1.0), None);
    }

    #[test]
    fn bursts_faster_than_kwin_sends_are_dropped() {
        let gaze = Gaze {
            last: Mutex::new(None),
        };
        let t = Instant::now();
        assert!(gaze.admit(t));
        assert!(!gaze.admit(t + Duration::from_millis(5)));
        assert!(gaze.admit(t + Duration::from_millis(33)));
    }

    #[test]
    fn script_is_throttled_event_driven_and_names_this_service() {
        assert!(SCRIPT.contains("onCursorPosChanged"));
        assert!(SCRIPT.contains("interval: 50"));
        assert!(
            !SCRIPT.contains("repeat: true"),
            "never a free-running poll"
        );
        assert!(
            SCRIPT.contains("fullScreen"),
            "silent while a fullscreen window has focus"
        );
        assert!(
            SCRIPT.contains("onFailed"),
            "backs off while the island is gone"
        );
        for name in [BUS_NAME, OBJECT_PATH, INTERFACE, "\"Pointer\""] {
            assert!(SCRIPT.contains(name), "script must call {name}");
        }
    }
}
