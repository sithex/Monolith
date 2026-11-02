//! Live equalizer (SPEC §0.5, §8 Phase 6 "Music extras").
//!
//! A `pw-record` child captures the MONITOR of the playing stream and `spectrum.rs` (the
//! audited FFT core) turns it into a few smoothed bands. Hard rules, each pinned by a test:
//!
//! * Never a microphone or any source. The capture target is either the playing application's
//!   own output stream (`Stream/Output/Audio` node found through `pw-dump`) or, when it cannot
//!   be identified, the default SINK's monitor (`-P stream.capture.sink=true`). The argv always
//!   carries an explicit `--target`; `pw-record` without one would record the default source.
//! * Never changes volume or routing: it only reads.
//! * Capture runs only while the equalizer is enabled AND visible AND a player is playing. Any
//!   of those turning false kills the child (process group) and publishes `active: false`.
//! * At most 30 `EqualizerChanged` publishes per second.

use super::{
    spectrum::{Analyzer, BandConfig},
    PlayerHint,
};
use crate::{
    bus::Latest,
    contract::system::{EqualizerChanged, ServiceStatus},
    integrations::service_status::Reporter,
    services::process::run_bounded,
};
use serde_json::Value;
use std::{
    io::Read,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, watch};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;
/// Lower bound between two publishes (≤ 30 Hz).
pub const MIN_EMIT_GAP: Duration = Duration::from_millis(34);
const HELPER_TIMEOUT: Duration = Duration::from_secs(3);
const DUMP_LIMIT: usize = 4 * 1024 * 1024;
const RETRY_AFTER_EXIT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub enum ChildEnv {
    /// The child sees this process's environment (production).
    Inherit,
    /// Exactly these variables (tests with a private PipeWire instance).
    Only(Vec<(String, String)>),
}

#[derive(Clone, Debug)]
pub struct Tools {
    pub pw_record: PathBuf,
    pub pw_dump: PathBuf,
    pub pactl: PathBuf,
}

impl Default for Tools {
    fn default() -> Self {
        Self {
            pw_record: "pw-record".into(),
            pw_dump: "pw-dump".into(),
            pactl: "pactl".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EqConfig {
    pub tools: Tools,
    pub env: ChildEnv,
    /// Capture this sink's monitor instead of asking `pactl get-default-sink` (tests).
    pub sink_override: Option<String>,
}

impl Default for EqConfig {
    fn default() -> Self {
        Self {
            tools: Tools::default(),
            env: ChildEnv::Inherit,
            sink_override: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Want {
    enabled: bool,
    visible: bool,
    hint: Option<PlayerHint>,
    bands: usize,
}

impl Want {
    fn capturing(&self) -> bool {
        self.enabled && self.visible && self.hint.is_some()
    }

    fn reason(&self) -> &'static str {
        if !self.enabled {
            "disabled"
        } else if !self.visible {
            "hidden"
        } else if self.hint.is_none() {
            "idle"
        } else {
            "capturing"
        }
    }
}

/// What to capture, resolved from the player hint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub arg: String,
    /// The target is a sink: capture its monitor.
    pub sink: bool,
}

/// The `pw-record` argv. Pure and tested: an explicit target, a monitor flag for sinks, raw
/// stereo f32 on stdout, and nothing that selects a source.
pub fn record_args(target: &Target) -> Vec<String> {
    let mut args = vec!["--target".to_owned(), target.arg.clone()];
    if target.sink {
        args.push("-P".into());
        args.push("stream.capture.sink=true".into());
    }
    args.extend(
        [
            "--rate",
            "48000",
            "--channels",
            "2",
            "--format",
            "f32",
            "--latency",
            "20ms",
            "--raw",
            "-",
        ]
        .map(str::to_owned),
    );
    args
}

/// Find the playing application's own output stream in a `pw-dump` document: an audio
/// OUTPUT stream node (never a source or a sink) whose process id or application name matches.
pub fn find_stream(dump: &Value, hint: &PlayerHint) -> Option<String> {
    let nodes = dump.as_array()?;
    let mut by_pid = None;
    let mut by_name = None;
    for object in nodes {
        let props = &object["info"]["props"];
        if props["media.class"].as_str() != Some("Stream/Output/Audio") {
            continue;
        }
        let Some(serial) = props["object.serial"]
            .as_u64()
            .map(|n| n.to_string())
            .or_else(|| props["object.serial"].as_str().map(str::to_owned))
        else {
            continue;
        };
        let pid = props["application.process.id"]
            .as_u64()
            .or_else(|| props["application.process.id"].as_str()?.parse().ok());
        if hint.pid.is_some() && pid == hint.pid.map(u64::from) && by_pid.is_none() {
            by_pid = Some(serial.clone());
        }
        let names = [
            props["application.name"].as_str(),
            props["application.process.binary"].as_str(),
            props["node.name"].as_str(),
        ];
        if by_name.is_none()
            && names.iter().flatten().any(|n| {
                hint.names
                    .iter()
                    .any(|h| !h.is_empty() && n.to_lowercase() == *h)
            })
        {
            by_name = Some(serial);
        }
    }
    by_pid.or(by_name)
}

fn command(config: &EqConfig, program: &PathBuf) -> Command {
    let mut command = Command::new(program);
    if let ChildEnv::Only(vars) = &config.env {
        command.env_clear();
        command.envs(vars.iter().map(|(k, v)| (k, v)));
    }
    command
}

/// Resolve the capture target (blocking: runs bounded helpers).
pub fn resolve_target(config: &EqConfig, hint: &PlayerHint) -> Result<Target, &'static str> {
    let mut dump = command(config, &config.tools.pw_dump);
    dump.arg("-N");
    if let Ok(bytes) = run_bounded(dump, HELPER_TIMEOUT, DUMP_LIMIT, false) {
        if let Ok(document) = serde_json::from_slice::<Value>(&bytes) {
            if let Some(serial) = find_stream(&document, hint) {
                return Ok(Target {
                    arg: serial,
                    sink: false,
                });
            }
        }
    }
    let sink = match &config.sink_override {
        Some(sink) => sink.clone(),
        None => {
            let mut pactl = command(config, &config.tools.pactl);
            pactl.arg("get-default-sink");
            let bytes =
                run_bounded(pactl, HELPER_TIMEOUT, 4096, false).map_err(|_| "no-default-sink")?;
            String::from_utf8_lossy(&bytes).trim().to_owned()
        }
    };
    if sink.is_empty() || sink.chars().any(char::is_control) || sink.starts_with('-') {
        return Err("no-default-sink");
    }
    Ok(Target {
        arg: sink,
        sink: true,
    })
}

struct Capture {
    pid: u32,
    argv: Vec<String>,
    hint: Option<PlayerHint>,
    bands: usize,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Capture {
    fn kill(&mut self) {
        self.stop.store(true, Ordering::Release);
        // SAFETY: the child leads its own process group (process_group(0)); the group id is
        // its pid, which stays reserved until the capture thread reaps it after the join below.
        unsafe {
            libc::killpg(self.pid as i32, libc::SIGKILL);
        }
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::warn!("equalizer capture thread panicked");
            }
        }
    }
}

struct Shared {
    current: EqualizerChanged,
    capture: Option<Capture>,
}

pub struct EqualizerService {
    pub status: Arc<Reporter>,
    pub topic: Latest<EqualizerChanged>,
    config: EqConfig,
    want: watch::Sender<Want>,
    shared: Mutex<Shared>,
    stop: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl EqualizerService {
    pub fn new(config: EqConfig, enabled: bool, bands: usize) -> Arc<Self> {
        let current = EqualizerChanged {
            rev: 0,
            active: false,
            bands: Vec::new(),
        };
        let topic = Latest::new();
        topic.publish(current.clone());
        let want = Want {
            enabled,
            visible: false,
            hint: None,
            bands: bands.clamp(8, 32),
        };
        let status = Reporter::new("equalizer");
        status.unavailable(want.reason());
        Arc::new(Self {
            status,
            topic,
            config,
            want: watch::channel(want).0,
            shared: Mutex::new(Shared {
                current,
                capture: None,
            }),
            stop: watch::channel(false).0,
            task: Mutex::new(None),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        match self.shared.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn current(&self) -> EqualizerChanged {
        self.lock().current.clone()
    }

    pub fn status(&self) -> ServiceStatus {
        self.status.get()
    }

    /// Pid of the running capture child, if any (tests assert it is gone when hidden/paused).
    pub fn capture_pid(&self) -> Option<u32> {
        self.lock().capture.as_ref().map(|c| c.pid)
    }

    pub fn capture_argv(&self) -> Option<Vec<String>> {
        self.lock().capture.as_ref().map(|c| c.argv.clone())
    }

    pub fn configure(&self, enabled: bool, bands: usize) {
        let bands = bands.clamp(8, 32);
        self.want.send_if_modified(|want| {
            let changed = want.enabled != enabled || want.bands != bands;
            want.enabled = enabled;
            want.bands = bands;
            changed
        });
    }

    /// The music view is (not) on screen. Hidden = no capture.
    pub fn set_visible(&self, visible: bool) {
        self.want.send_if_modified(|want| {
            let changed = want.visible != visible;
            want.visible = visible;
            changed
        });
    }

    /// The playing player (or `None` when paused/stopped/absent).
    pub fn set_source(&self, hint: Option<PlayerHint>) {
        self.want.send_if_modified(|want| {
            let changed = want.hint != hint;
            want.hint = hint;
            changed
        });
    }

    fn publish(&self, active: bool, bands: Vec<f64>) {
        let mut shared = self.lock();
        publish_locked(&self.topic, &mut shared, active, bands);
    }

    pub fn start(self: &Arc<Self>) {
        let service = Arc::clone(self);
        let stopped = self.stop.subscribe();
        let want = self.want.subscribe();
        let task = tauri::async_runtime::spawn(service.supervisor(want, stopped));
        if let Ok(mut slot) = self.task.lock() {
            *slot = Some(task);
        }
    }

    fn stop_capture(&self) {
        let capture = self.lock().capture.take();
        if let Some(mut capture) = capture {
            capture.kill();
        }
        self.publish(false, Vec::new());
    }

    async fn supervisor(
        self: Arc<Self>,
        mut want: watch::Receiver<Want>,
        mut stop: watch::Receiver<bool>,
    ) {
        let (ended_tx, mut ended_rx) = mpsc::unbounded_channel::<u32>();
        let mut retry_at: Option<tokio::time::Instant> = None;
        loop {
            if *stop.borrow() {
                break;
            }
            let wanted = want.borrow_and_update().clone();
            let running = self
                .lock()
                .capture
                .as_ref()
                .map(|c| (c.hint.clone(), c.bands));
            let keep = wanted.capturing()
                && running
                    .as_ref()
                    .is_some_and(|(hint, bands)| *hint == wanted.hint && *bands == wanted.bands);
            if !keep && running.is_some() {
                let service = Arc::clone(&self);
                // Killing joins a thread: do it off the async worker.
                let _joined =
                    tauri::async_runtime::spawn_blocking(move || service.stop_capture()).await;
            }
            if !wanted.capturing() {
                self.status.unavailable(wanted.reason());
                retry_at = None;
            } else if !keep {
                match self.begin(&wanted, ended_tx.clone()).await {
                    Ok(()) => {
                        self.status.available("capturing");
                        self.status.connected();
                        retry_at = None;
                    }
                    Err(detail) => {
                        self.status.unavailable(detail);
                        retry_at = Some(tokio::time::Instant::now() + RETRY_AFTER_EXIT);
                    }
                }
            }
            tokio::select! {
                _ = stop.changed() => break,
                _ = want.changed() => {}
                ended = ended_rx.recv() => {
                    // A capture ended on its own (pw-record exited): only the CURRENT pid counts.
                    if let Some(pid) = ended {
                        let current = self.lock().capture.as_ref().map(|c| c.pid);
                        if current == Some(pid) {
                            let service = Arc::clone(&self);
                            let _joined = tauri::async_runtime::spawn_blocking(move || service.stop_capture()).await;
                            self.status.unavailable("capture-ended");
                            retry_at = Some(tokio::time::Instant::now() + RETRY_AFTER_EXIT);
                        }
                    }
                }
                () = async {
                    match retry_at {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await,
                    }
                } => { retry_at = None; }
            }
        }
        let service = Arc::clone(&self);
        let _joined = tauri::async_runtime::spawn_blocking(move || service.stop_capture()).await;
        self.status.unavailable("stopped");
    }

    async fn begin(
        self: &Arc<Self>,
        wanted: &Want,
        ended: mpsc::UnboundedSender<u32>,
    ) -> Result<(), &'static str> {
        let Some(hint) = wanted.hint.clone() else {
            return Err("idle");
        };
        let service = Arc::clone(self);
        let config = self.config.clone();
        let bands = wanted.bands;
        let spawn_hint = hint.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let target = resolve_target(&config, &spawn_hint)?;
            service.spawn_capture(&config, &target, spawn_hint, bands, ended)
        })
        .await
        .map_err(|_| "capture-failed")??;
        Ok(())
    }

    fn spawn_capture(
        self: &Arc<Self>,
        config: &EqConfig,
        target: &Target,
        hint: PlayerHint,
        bands: usize,
        ended: mpsc::UnboundedSender<u32>,
    ) -> Result<(), &'static str> {
        let analyzer = Analyzer::new(BandConfig {
            sample_rate: SAMPLE_RATE,
            bands,
            ..BandConfig::default()
        })
        .map_err(|_| "bad-band-config")?;
        let argv = record_args(target);
        let mut cmd = command(config, &config.tools.pw_record);
        cmd.args(&argv)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn().map_err(|_| "pw-record-missing")?;
        let Some(mut out) = child.stdout.take() else {
            // SAFETY: killing the group we just created.
            unsafe {
                libc::killpg(child.id() as i32, libc::SIGKILL);
            }
            let _reaped = child.wait();
            return Err("capture-failed");
        };
        let pid = child.id();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let service = Arc::clone(self);
        let thread = std::thread::Builder::new()
            .name("island-equalizer".into())
            .spawn(move || {
                service.read_loop(&mut out, analyzer, &thread_stop);
                // SAFETY: our own group; harmless if it already exited.
                unsafe {
                    libc::killpg(pid as i32, libc::SIGKILL);
                }
                let _reaped = child.wait();
                if !thread_stop.load(Ordering::Acquire) {
                    let _sent = ended.send(pid);
                }
            })
            .map_err(|_| "capture-failed")?;
        self.lock().capture = Some(Capture {
            pid,
            argv,
            hint: Some(hint),
            bands,
            stop,
            thread: Some(thread),
        });
        Ok(())
    }

    fn read_loop(&self, out: &mut impl Read, mut analyzer: Analyzer, stop: &AtomicBool) {
        let mut buffer = [0u8; 8192];
        let mut carry: Vec<u8> = Vec::with_capacity(8);
        let mut samples: Vec<f32> = Vec::with_capacity(2048);
        let mut last = Instant::now() - MIN_EMIT_GAP;
        while !stop.load(Ordering::Acquire) {
            let count = match out.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(count) => count,
            };
            carry.extend_from_slice(&buffer[..count]);
            let whole = carry.len() / 4 * 4;
            samples.clear();
            samples.extend(
                carry[..whole]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            );
            carry.drain(..whole);
            analyzer.push(&samples, CHANNELS);
            if last.elapsed() >= MIN_EMIT_GAP && !stop.load(Ordering::Acquire) {
                if let Some(bands) = analyzer.bands() {
                    let levels = bands
                        .iter()
                        .map(|v| f64::from(*v).clamp(0.0, 1.0))
                        .collect();
                    last = Instant::now();
                    let mut shared = self.lock();
                    // A capture that was stopped meanwhile must not publish a stale frame.
                    if shared.capture.is_some() && !stop.load(Ordering::Acquire) {
                        publish_locked(&self.topic, &mut shared, true, levels);
                    }
                }
            }
        }
    }

    pub fn shutdown(&self) {
        self.stop.send_replace(true);
        let task = self.task.lock().ok().and_then(|mut slot| slot.take());
        if let Some(task) = task {
            let _ = tauri::async_runtime::block_on(async {
                tokio::time::timeout(Duration::from_secs(5), task).await
            });
        }
        let capture = self.lock().capture.take();
        if let Some(mut capture) = capture {
            capture.kill();
        }
    }
}

fn publish_locked(
    topic: &Latest<EqualizerChanged>,
    shared: &mut Shared,
    active: bool,
    bands: Vec<f64>,
) {
    if shared.current.active == active && shared.current.bands == bands {
        return;
    }
    let next = EqualizerChanged {
        rev: shared.current.rev.wrapping_add(1),
        active,
        bands,
    };
    shared.current = next.clone();
    topic.publish(next);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hint(pid: Option<u32>, names: &[&str]) -> PlayerHint {
        PlayerHint {
            pid,
            names: names.iter().map(|n| (*n).to_owned()).collect(),
        }
    }

    fn dump() -> Value {
        json!([
            {"id": 40, "type": "PipeWire:Interface:Node", "info": {"props": {
                "media.class": "Audio/Source", "object.serial": 40, "node.name": "spotify",
                "application.process.id": 7}}},
            {"id": 41, "type": "PipeWire:Interface:Node", "info": {"props": {
                "media.class": "Stream/Output/Audio", "object.serial": 410, "application.name": "Firefox",
                "application.process.id": 100}}},
            {"id": 42, "type": "PipeWire:Interface:Node", "info": {"props": {
                "media.class": "Stream/Output/Audio", "object.serial": "420", "application.name": "Spotify",
                "application.process.binary": "spotify", "application.process.id": "200"}}},
            {"id": 43, "type": "PipeWire:Interface:Node", "info": {"props": {
                "media.class": "Stream/Input/Audio", "object.serial": 430, "application.name": "spotify"}}}
        ])
    }

    #[test]
    fn record_args_always_name_a_target_and_never_select_a_source() {
        let sink = record_args(&Target {
            arg: "alsa_output.x".into(),
            sink: true,
        });
        assert_eq!(
            &sink[..4],
            [
                "--target",
                "alsa_output.x",
                "-P",
                "stream.capture.sink=true"
            ]
        );
        let stream = record_args(&Target {
            arg: "410".into(),
            sink: false,
        });
        assert_eq!(&stream[..2], ["--target", "410"]);
        assert!(!stream.iter().any(|a| a.contains("capture.sink")));
        for args in [&sink, &stream] {
            assert_eq!(
                args.last().map(String::as_str),
                Some("-"),
                "raw PCM to stdout"
            );
            assert!(args.windows(2).any(|w| w == ["--format", "f32"]));
            assert!(!args
                .iter()
                .any(|a| a.to_lowercase().contains("source") || a.contains("@DEFAULT")));
        }
    }

    #[test]
    fn stream_lookup_matches_pid_first_then_name_and_only_output_streams() {
        assert_eq!(
            find_stream(&dump(), &hint(Some(200), &["nomatch"])).as_deref(),
            Some("420")
        );
        assert_eq!(
            find_stream(&dump(), &hint(Some(1), &["firefox"])).as_deref(),
            Some("410")
        );
        // The pid wins over a name that matches a different stream.
        assert_eq!(
            find_stream(&dump(), &hint(Some(100), &["spotify"])).as_deref(),
            Some("410")
        );
        // A source / input stream with the same name is never a target.
        assert_eq!(
            find_stream(&dump(), &hint(Some(7), &["spotify", "nomatch"])).as_deref(),
            Some("420")
        );
        assert_eq!(find_stream(&dump(), &hint(None, &["unknown"])), None);
        assert_eq!(
            find_stream(&json!({"not": "an array"}), &hint(None, &["x"])),
            None
        );
    }
}
