//! Autoclicker: clicks a mouse button at a set rate wherever the pointer is, through a virtual
//! mouse on `/dev/uinput` (works on Wayland, no daemon). Started and stopped from the island
//! page or a KDE global hotkey (toggle, or hold to click). The config is saved to
//! `autoclicker.json`; arming is per session (the island always starts disarmed, so the hotkey
//! never clicks by surprise after a restart).
//!
//! One click thread per run. It re-reads the config on every click, so changes apply live,
//! and publishes the click count at most ten times a second.
pub mod core;
mod hotkey;
mod uinput;

use crate::{
    bus::Latest,
    contract::apps::{AutoclickerChanged, AutoclickerConfig, AutoclickerOp, ClickKind, TriggerMode},
    error::IslandError,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};

const FILE: &str = "autoclicker.json";
const FILE_CAP: usize = 16 * 1024;
/// Time for the compositor to pick up a new virtual device before its first click.
const DEVICE_SETTLE: Duration = Duration::from_millis(350);
const PUBLISH_EVERY: Duration = Duration::from_millis(100);
/// Key auto-repeat on compositors that resend "pressed" must not flip toggle mode.
const REPEAT_GUARD: Duration = Duration::from_millis(400);

struct Run {
    id: u64,
    stop: Arc<AtomicBool>,
    starting: Arc<AtomicBool>,
}

struct State {
    armed: bool,
    run: Option<Run>,
    next_id: u64,
    rev: u32,
    available: bool,
    detail: String,
    hotkey_state: &'static str,
    hotkey_detail: String,
    held: bool,
    last_press: Option<Instant>,
}

pub struct Autoclicker {
    file: PathBuf,
    test_instance: bool,
    config: Arc<Mutex<AutoclickerConfig>>,
    state: Mutex<State>,
    mouse: Mutex<Option<Arc<uinput::Mouse>>>,
    clicks: Arc<AtomicU64>,
    hotkey: Option<hotkey::HotkeyHost>,
    me: Weak<Autoclicker>,
    pub topic: Latest<AutoclickerChanged>,
}

fn unavailable_detail() -> String {
    "Monolith can't write to /dev/uinput. Load the module (sudo modprobe uinput) and log out \
     and back in, or add a udev rule giving your user access."
        .into()
}

impl Autoclicker {
    /// Loads the saved config. Test instances never touch `/dev/uinput` or KDE shortcuts:
    /// clicks are only counted there.
    pub fn new(data: &Path, test_instance: bool) -> Arc<Self> {
        let file = data.join(FILE);
        let config = match crate::integrations::private_file::read(&file, FILE_CAP) {
            Ok(Some(bytes)) => core::from_json(&String::from_utf8_lossy(&bytes)),
            _ => core::defaults(),
        };
        let available = test_instance || uinput::writable();
        let svc = Arc::new_cyclic(|me: &Weak<Self>| {
            let hotkey = (!test_instance).then(|| {
                let on_key = me.clone();
                let on_status = me.clone();
                hotkey::HotkeyHost::start(
                    Arc::new(move |key| {
                        if let Some(s) = on_key.upgrade() {
                            s.on_key(key);
                        }
                    }),
                    Arc::new(move |state, detail| {
                        if let Some(s) = on_status.upgrade() {
                            s.on_hotkey_status(state, detail);
                        }
                    }),
                )
            });
            Self {
                file,
                test_instance,
                config: Arc::new(Mutex::new(config)),
                state: Mutex::new(State {
                    armed: false,
                    run: None,
                    next_id: 1,
                    rev: 0,
                    available,
                    detail: if available {
                        String::new()
                    } else {
                        unavailable_detail()
                    },
                    hotkey_state: if test_instance { "unavailable" } else { "pending" },
                    hotkey_detail: if test_instance {
                        "Hotkeys are off in test runs".into()
                    } else {
                        String::new()
                    },
                    held: false,
                    last_press: None,
                }),
                mouse: Mutex::new(None),
                clicks: Arc::new(AtomicU64::new(0)),
                hotkey,
                me: me.clone(),
                topic: Latest::new(),
            }
        });
        svc.publish();
        svc
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, IslandError> {
        self.state
            .lock()
            .map_err(|_| IslandError::internal("Autoclicker state poisoned"))
    }

    fn config(&self) -> AutoclickerConfig {
        self.config
            .lock()
            .map(|c| c.clone())
            .unwrap_or_else(|_| core::defaults())
    }

    fn snapshot(&self, state: &mut State) -> AutoclickerChanged {
        state.rev = state.rev.wrapping_add(1);
        AutoclickerChanged {
            rev: state.rev,
            available: state.available,
            detail: state.detail.clone(),
            config: self.config(),
            armed: state.armed,
            running: state.run.is_some(),
            starting: state
                .run
                .as_ref()
                .is_some_and(|r| r.starting.load(Ordering::Relaxed)),
            clicks: self.clicks.load(Ordering::Relaxed) as f64,
            hotkey_state: state.hotkey_state.into(),
            hotkey_detail: state.hotkey_detail.clone(),
        }
    }

    fn publish(&self) {
        if let Ok(mut state) = self.lock() {
            let snap = self.snapshot(&mut state);
            self.topic.publish(snap);
        }
    }

    pub fn current(&self) -> Result<AutoclickerChanged, IslandError> {
        let mut state = self.lock()?;
        Ok(self.snapshot(&mut state))
    }

    pub fn command(&self, op: AutoclickerOp) -> Result<AutoclickerChanged, IslandError> {
        match op {
            AutoclickerOp::Configure { config } => {
                core::validate(&config).map_err(IslandError::invalid)?;
                let hotkey_changed = {
                    let mut current = self
                        .config
                        .lock()
                        .map_err(|_| IslandError::internal("Autoclicker config poisoned"))?;
                    let changed = current.hotkey != config.hotkey;
                    *current = config.clone();
                    changed
                };
                if let Err(error) = crate::integrations::private_file::write(
                    &self.file,
                    core::to_json(&config).as_bytes(),
                    FILE_CAP,
                ) {
                    tracing::warn!(%error, "Autoclicker settings not saved");
                }
                let state = self.lock()?;
                if hotkey_changed && state.armed {
                    if let Some(h) = &self.hotkey {
                        h.set(Some(config.hotkey.clone()));
                    }
                }
            }
            AutoclickerOp::Arm { armed } => {
                let mut state = self.lock()?;
                state.armed = armed;
                state.held = false;
                if let Some(h) = &self.hotkey {
                    h.set(armed.then(|| self.config().hotkey));
                }
                if !armed {
                    Self::stop_locked(&mut state);
                }
            }
            AutoclickerOp::Start {} => {
                let delay = Duration::from_secs(self.config().start_delay_s.into());
                let mut state = self.lock()?;
                self.start_locked(&mut state, delay)?;
            }
            AutoclickerOp::Stop {} => {
                let mut state = self.lock()?;
                Self::stop_locked(&mut state);
            }
        }
        let mut state = self.lock()?;
        let snap = self.snapshot(&mut state);
        self.topic.publish(snap.clone());
        Ok(snap)
    }

    fn stop_locked(state: &mut State) {
        if let Some(run) = state.run.take() {
            run.stop.store(true, Ordering::Relaxed);
        }
    }

    fn start_locked(&self, state: &mut State, delay: Duration) -> Result<(), IslandError> {
        if !state.available {
            return Err(IslandError::unavailable(unavailable_detail()));
        }
        if state.run.is_some() {
            return Ok(());
        }
        let (mouse, fresh) = if self.test_instance {
            (None, false)
        } else {
            let mut slot = self
                .mouse
                .lock()
                .map_err(|_| IslandError::internal("Autoclicker device poisoned"))?;
            match slot.as_ref() {
                Some(m) => (Some(m.clone()), false),
                None => match uinput::Mouse::create() {
                    Ok(m) => {
                        let m = Arc::new(m);
                        *slot = Some(m.clone());
                        (Some(m), true)
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Autoclicker virtual mouse unavailable");
                        state.available = false;
                        state.detail = unavailable_detail();
                        return Err(IslandError::unavailable(unavailable_detail()));
                    }
                },
            }
        };
        let delay = if fresh { delay.max(DEVICE_SETTLE) } else { delay };
        let id = state.next_id;
        state.next_id += 1;
        let stop = Arc::new(AtomicBool::new(false));
        let starting = Arc::new(AtomicBool::new(!delay.is_zero()));
        state.run = Some(Run {
            id,
            stop: stop.clone(),
            starting: starting.clone(),
        });
        state.detail.clear();
        self.clicks.store(0, Ordering::Relaxed);
        let job = Job {
            me: self.me.clone(),
            id,
            stop,
            starting,
            config: self.config.clone(),
            mouse,
            clicks: self.clicks.clone(),
            delay,
        };
        if let Err(error) = std::thread::Builder::new()
            .name("autoclicker".into())
            .spawn(move || job.run())
        {
            state.run = None;
            return Err(IslandError::internal(format!("Autoclicker thread: {error}")));
        }
        Ok(())
    }

    fn finished(&self, id: u64, error: Option<String>) {
        if let Ok(mut state) = self.lock() {
            if state.run.as_ref().is_some_and(|r| r.id == id) {
                state.run = None;
            }
            if let Some(error) = error {
                state.detail = error;
            }
            let snap = self.snapshot(&mut state);
            self.topic.publish(snap);
        }
    }

    fn on_key(&self, key: hotkey::Key) {
        let mode = self.config().mode;
        let Ok(mut state) = self.lock() else { return };
        if !state.armed {
            return;
        }
        match (mode, key) {
            (TriggerMode::Toggle, hotkey::Key::Pressed) => {
                let now = Instant::now();
                let repeat = state.held
                    && state
                        .last_press
                        .is_some_and(|t| now.duration_since(t) < REPEAT_GUARD);
                state.held = true;
                state.last_press = Some(now);
                if repeat {
                    return;
                }
                if state.run.is_some() {
                    Self::stop_locked(&mut state);
                } else if let Err(error) = self.start_locked(&mut state, Duration::ZERO) {
                    tracing::warn!(%error, "Autoclicker hotkey start failed");
                }
            }
            (TriggerMode::Hold, hotkey::Key::Pressed) => {
                state.held = true;
                if let Err(error) = self.start_locked(&mut state, Duration::ZERO) {
                    tracing::warn!(%error, "Autoclicker hotkey start failed");
                }
            }
            (TriggerMode::Hold, hotkey::Key::Released) => {
                state.held = false;
                Self::stop_locked(&mut state);
            }
            (TriggerMode::Toggle, hotkey::Key::Released) => state.held = false,
        }
        let snap = self.snapshot(&mut state);
        self.topic.publish(snap);
    }

    fn on_hotkey_status(&self, hotkey_state: &'static str, detail: String) {
        if let Ok(mut state) = self.lock() {
            if state.hotkey_state == hotkey_state && state.hotkey_detail == detail {
                return;
            }
            state.hotkey_state = hotkey_state;
            state.hotkey_detail = detail;
            let snap = self.snapshot(&mut state);
            self.topic.publish(snap);
        }
    }

    /// Stops clicking and releases the hotkey (bounded).
    pub fn shutdown(&self) {
        if let Ok(mut state) = self.lock() {
            Self::stop_locked(&mut state);
        }
        if let Some(h) = &self.hotkey {
            h.shutdown();
        }
    }
}

struct Job {
    me: Weak<Autoclicker>,
    id: u64,
    stop: Arc<AtomicBool>,
    starting: Arc<AtomicBool>,
    config: Arc<Mutex<AutoclickerConfig>>,
    mouse: Option<Arc<uinput::Mouse>>,
    clicks: Arc<AtomicU64>,
    delay: Duration,
}

impl Job {
    /// Sleeps until `until` in short slices; false when the run was stopped meanwhile.
    fn wait_until(&self, until: Instant) -> bool {
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return false;
            }
            let now = Instant::now();
            if now >= until {
                return true;
            }
            std::thread::sleep((until - now).min(Duration::from_millis(20)));
        }
    }

    fn publish(&self) {
        if let Some(s) = self.me.upgrade() {
            s.publish();
        }
    }

    fn click(&self, c: &AutoclickerConfig) -> std::io::Result<()> {
        let Some(mouse) = &self.mouse else {
            return Ok(());
        };
        mouse.button(c.button, true)?;
        mouse.button(c.button, false)?;
        if c.kind == ClickKind::Double {
            let gap = Duration::from_millis((u64::from(c.interval_ms) / 3).clamp(1, 30));
            std::thread::sleep(gap);
            mouse.button(c.button, true)?;
            mouse.button(c.button, false)?;
        }
        Ok(())
    }

    fn run(self) {
        let mut error = None;
        if self.wait_until(Instant::now() + self.delay) {
            if self.starting.swap(false, Ordering::Relaxed) {
                self.publish();
            }
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9e37_79b9_7f4a_7c15);
            let mut jitter = core::Jitter::new(seed);
            let started = Instant::now();
            let mut next = started;
            let mut last_publish = started;
            loop {
                if self.stop.load(Ordering::Relaxed) {
                    break;
                }
                let c = match self.config.lock() {
                    Ok(c) => c.clone(),
                    Err(_) => break,
                };
                let done = self.clicks.load(Ordering::Relaxed);
                if c.click_limit > 0 && done >= u64::from(c.click_limit) {
                    break;
                }
                if c.time_limit_s > 0
                    && started.elapsed() >= Duration::from_secs(c.time_limit_s.into())
                {
                    break;
                }
                if let Err(e) = self.click(&c) {
                    tracing::warn!(error = %e, "Autoclicker click failed");
                    error = Some(format!("Clicking stopped: {e}"));
                    break;
                }
                self.clicks.fetch_add(1, Ordering::Relaxed);
                let now = Instant::now();
                if now.duration_since(last_publish) >= PUBLISH_EVERY {
                    last_publish = now;
                    self.publish();
                }
                next += Duration::from_micros(jitter.interval_us(c.interval_ms, c.jitter_percent));
                // Fell far behind (a stall): restart the schedule instead of bursting.
                if now > next + Duration::from_millis(50) {
                    next = now;
                }
                if !self.wait_until(next) {
                    break;
                }
            }
        }
        if let Some(s) = self.me.upgrade() {
            s.finished(self.id, error);
        }
    }
}
