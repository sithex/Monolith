//! Notch companion: the pointer position the notch's little face looks toward.
//!
//! Plasma widgets cannot read the global pointer on Wayland, so a tiny KWin script
//! (`companion-gaze.qml`, loaded only while the companion is on) sends it here as a
//! `Pointer(x, y)` call, throttled in KWin to 20 Hz and only while the pointer moves.
//! The island re-broadcasts it as the `Gaze(x, y)` signal, which every notch widget
//! receives with Plasma's D-Bus `SignalWatcher` and turns into eye movement itself.
//! Nothing here polls, and nothing runs while the pointer is still.
//!
//! The same object also carries file-copy progress for the closed notch
//! (`local.hermes.Monolith.Transfers`): a `CopyProgress(i)` signal in whole 10 % steps, at most
//! one per 3 s plus start and end (`jobs::core::NotchSteps`). A signal costs plasmashell no
//! config write and the island no process spawn, unlike the appearance push.
//!
//! The same connection also serves the drop shelf at `/Shelf`
//! (`local.hermes.Monolith.Shelf.Add(as uris) → u parked`): the notch hands dropped file
//! URIs over D-Bus so no shell command ever carries a file name. Rust validates every URI
//! (`shelf::core::parse_uri`).
use crate::services::process::run_bounded;
use anyhow::{Context, Result};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::watch;
use zbus::object_server::SignalEmitter;

pub const BUS_NAME: &str = "local.hermes.Monolith";
pub const OBJECT_PATH: &str = "/Companion";
pub const INTERFACE: &str = "local.hermes.Monolith.Companion";
pub const TRANSFERS_INTERFACE: &str = "local.hermes.Monolith.Transfers";
pub const SHELF_PATH: &str = "/Shelf";
pub const SHELF_INTERFACE: &str = "local.hermes.Monolith.Shelf";
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

/// Copy progress for the notch, on the same `/Companion` object.
struct Transfers {
    current: Arc<AtomicI32>,
}

#[zbus::interface(name = "local.hermes.Monolith.Transfers")]
impl Transfers {
    /// The value last sent with `CopyProgress` (-1 = no copy), for a notch that starts late.
    fn progress(&self) -> i32 {
        self.current.load(Ordering::Acquire)
    }

    /// -1 = no copy running, else 0..=100 in steps of 10.
    #[zbus(signal)]
    async fn copy_progress(emitter: &SignalEmitter<'_>, percent: i32) -> zbus::Result<()>;
}

/// The drop shelf's entry point for the notch.
struct ShelfBus {
    store: Arc<crate::integrations::shelf::ShelfStore>,
}

#[zbus::interface(name = "local.hermes.Monolith.Shelf")]
impl ShelfBus {
    /// Parks the dropped `file://` URIs (validated; invalid ones are skipped). Returns how
    /// many were parked: 0 when none were valid or the shelf is off (`shelfEnabled`).
    async fn add(&self, uris: Vec<String>) -> u32 {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || store.add_uris(&uris))
            .await
            .unwrap_or(0)
    }
}

/// Relays the copy aggregate to the notch with the stepped cadence (event-driven: it wakes
/// for a new aggregate or for the end of a 3 s gap, never on a timer of its own).
async fn relay_progress(
    connection: zbus::Connection,
    current: Arc<AtomicI32>,
    mut aggregate: watch::Receiver<Option<u8>>,
) {
    use crate::integrations::jobs::core::{NotchSteps, Step};
    let mut steps = NotchSteps::default();
    loop {
        let now = Instant::now();
        let desired = *aggregate.borrow_and_update();
        let later = match steps.decide(desired, now) {
            Step::Send(value) => {
                current.store(value, Ordering::Release);
                let sent = async {
                    let iface = connection
                        .object_server()
                        .interface::<_, Transfers>(OBJECT_PATH)
                        .await?;
                    Transfers::copy_progress(iface.signal_emitter(), value).await
                };
                if let Err(error) = sent.await {
                    tracing::debug!(%error, "Copy progress signal not sent");
                }
                steps.sent(value, now);
                continue;
            }
            Step::Later(at) => Some(at),
            Step::Hold => None,
        };
        tokio::select! {
            changed = aggregate.changed() => if changed.is_err() { break },
            () = async {
                match later {
                    Some(at) => tokio::time::sleep_until(at.into()).await,
                    None => std::future::pending().await,
                }
            } => {},
        }
    }
}

/// The session-bus endpoint; dropping it releases the name.
pub struct CompanionBus {
    connection: zbus::Connection,
    copy_current: Arc<AtomicI32>,
    relay: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl CompanionBus {
    /// `shelf`: serve `/Shelf` too (the drop shelf); `None` leaves it out.
    pub async fn start(shelf: Option<Arc<crate::integrations::shelf::ShelfStore>>) -> Result<Self> {
        Self::start_with(zbus::connection::Builder::session(), shelf).await
    }

    /// The same endpoint on an explicit bus (private-bus tests).
    pub async fn start_at(
        address: &str,
        shelf: Option<Arc<crate::integrations::shelf::ShelfStore>>,
    ) -> Result<Self> {
        Self::start_with(zbus::connection::Builder::address(address), shelf).await
    }

    async fn start_with(
        builder: zbus::connection::Builder<'_>,
        shelf: Option<Arc<crate::integrations::shelf::ShelfStore>>,
    ) -> Result<Self> {
        let copy_current = Arc::new(AtomicI32::new(-1));
        let mut builder = builder
            .name(BUS_NAME)
            .serve_at(
                OBJECT_PATH,
                Gaze {
                    last: Mutex::new(None),
                },
            )
            .serve_at(
                OBJECT_PATH,
                Transfers {
                    current: copy_current.clone(),
                },
            );
        if let Some(store) = shelf {
            builder = builder.serve_at(SHELF_PATH, ShelfBus { store });
        }
        let build = builder.build();
        let connection = tokio::time::timeout(Duration::from_secs(3), build)
            .await
            .context("Companion D-Bus start timed out")??;
        Ok(Self {
            connection,
            copy_current,
            relay: Mutex::new(None),
        })
    }

    /// Starts relaying the copy aggregate as `CopyProgress` signals (once).
    pub fn relay_copy_progress(&self, aggregate: watch::Receiver<Option<u8>>) {
        let task = tauri::async_runtime::spawn(relay_progress(
            self.connection.clone(),
            self.copy_current.clone(),
            aggregate,
        ));
        if let Ok(mut slot) = self.relay.lock() {
            if let Some(old) = slot.replace(task) {
                old.abort();
            }
        }
    }
}

impl Drop for CompanionBus {
    fn drop(&mut self) {
        if let Some(task) = self.relay.lock().ok().and_then(|mut t| t.take()) {
            task.abort();
        }
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
