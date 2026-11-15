//! Autoclicker: clicks a mouse button (or presses a key) at a set rate wherever the pointer is,
//! through a virtual device on `/dev/uinput` (works on Wayland, no daemon). Started and
//! stopped from the island page or a KDE global hotkey (toggle, hold to click, or a burst per
//! press); a second hotkey, registered only while clicking, always stops it. Settings and
//! lifetime totals are saved to `autoclicker.json`; arming is per session (the island
//! always starts disarmed, so the hotkey never clicks by surprise after a restart).
//!
//! One click thread per run. It re-reads the config on every click, so changes apply live,
//! and publishes the click count at most ten times a second.
pub mod core;
mod hotkey;
pub mod keys;
mod uinput;

use crate::{
    bus::Latest,
    contract::apps::{
        AutoclickerChanged, AutoclickerConfig, AutoclickerOp, ClickKind, ClickTarget, TriggerMode,
    },
    error::IslandError,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};
use tokio::sync::watch;

const FILE: &str = "autoclicker.json";
const FILE_CAP: usize = 64 * 1024;
/// Time for the compositor to pick up a new virtual device before its first click.
const DEVICE_SETTLE: Duration = Duration::from_millis(350);
const PUBLISH_EVERY: Duration = Duration::from_millis(100);
/// While a button is held down nothing is counted; the run time still ticks on the page.
const HOLD_PUBLISH_EVERY: Duration = Duration::from_millis(500);
/// Key auto-repeat on compositors that resend "pressed" must not flip toggle mode.
const REPEAT_GUARD: Duration = Duration::from_millis(400);
/// Quitting waits this long for a run to release what it holds and save its totals.
const SHUTDOWN_WAIT: Duration = Duration::from_millis(300);

struct Run {
    id: u64,
    stop: Arc<AtomicBool>,
    starting: Arc<AtomicBool>,
    clicks: Arc<AtomicU64>,
    /// When clicking began (after the start delay).
    since: Arc<Mutex<Option<Instant>>>,
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
    stop_state: &'static str,
    stop_detail: String,
    held: bool,
    last_press: Option<Instant>,
    lifetime_clicks: u64,
    lifetime_ms: u64,
    /// The last finished run, shown until the next one starts.
    last_session_ms: u64,
    last_clicks: u64,
}

pub struct Autoclicker {
    file: PathBuf,
    test_instance: bool,
    config: Arc<Mutex<AutoclickerConfig>>,
    state: Mutex<State>,
    device: Mutex<Option<Arc<uinput::Mouse>>>,
    live_jobs: Arc<AtomicUsize>,
    hotkey: Option<hotkey::HotkeyHost>,
    stop_key: Option<hotkey::HotkeyHost>,
    running: watch::Sender<bool>,
    me: Weak<Autoclicker>,
    pub topic: Latest<AutoclickerChanged>,
}

fn unavailable_detail() -> String {
    "Monolith can't write to /dev/uinput. Load the module (sudo modprobe uinput) and log out \
     and back in, or add a udev rule giving your user access."
        .into()
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

impl Autoclicker {
    /// Loads the saved file. Test instances never touch `/dev/uinput` or KDE shortcuts:
    /// clicks are only counted there.
    pub fn new(data: &Path, test_instance: bool) -> Arc<Self> {
        let file = data.join(FILE);
        let saved = match crate::integrations::private_file::read(&file, FILE_CAP) {
            Ok(Some(bytes)) => core::load(&String::from_utf8_lossy(&bytes)),
            _ => core::Saved::default(),
        };
        let available = test_instance || uinput::writable();
        let svc = Arc::new_cyclic(|me: &Weak<Self>| {
            let host = |action, stop: bool| {
                let on_key = me.clone();
                let on_status = me.clone();
                hotkey::HotkeyHost::start(
                    action,
                    Arc::new(move |key| {
                        if let Some(s) = on_key.upgrade() {
                            if stop {
                                s.on_stop_key(key);
                            } else {
                                s.on_key(key);
                            }
                        }
                    }),
                    Arc::new(move |state, detail| {
                        if let Some(s) = on_status.upgrade() {
                            s.on_hotkey_status(stop, state, detail);
                        }
                    }),
                )
            };
            let hotkey = (!test_instance).then(|| host(hotkey::TOGGLE, false));
            let stop_key = (!test_instance).then(|| host(hotkey::STOP, true));
            Self {
                file,
                test_instance,
                config: Arc::new(Mutex::new(saved.config)),
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
                    hotkey_state: if test_instance {
                        "unavailable"
                    } else {
                        "pending"
                    },
                    hotkey_detail: if test_instance {
                        "Hotkeys are off in test runs".into()
                    } else {
                        String::new()
                    },
                    stop_state: "off",
                    stop_detail: String::new(),
                    held: false,
                    last_press: None,
                    lifetime_clicks: saved.lifetime_clicks,
                    lifetime_ms: saved.lifetime_ms,
                    last_session_ms: 0,
                    last_clicks: 0,
                }),
                device: Mutex::new(None),
                live_jobs: Arc::new(AtomicUsize::new(0)),
                hotkey,
                stop_key,
                running: watch::channel(false).0,
                me: me.clone(),
                topic: Latest::new(),
            }
        });
        svc.publish();
        svc
    }

    /// True while a run is active (including its start delay), for the closed notch.
    pub fn running(&self) -> watch::Receiver<bool> {
        self.running.subscribe()
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
        let (clicks, session_ms) = match &state.run {
            Some(run) => (
                run.clicks.load(Ordering::Relaxed),
                run.since
                    .lock()
                    .ok()
                    .and_then(|s| *s)
                    .map_or(0, |t| ms(t.elapsed())),
            ),
            None => (state.last_clicks, state.last_session_ms),
        };
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
            clicks: clicks as f64,
            session_ms: session_ms as f64,
            lifetime_clicks: state.lifetime_clicks as f64,
            lifetime_ms: state.lifetime_ms as f64,
            hotkey_state: state.hotkey_state.into(),
            hotkey_detail: state.hotkey_detail.clone(),
            stop_hotkey_state: state.stop_state.into(),
            stop_hotkey_detail: state.stop_detail.clone(),
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

    fn persist(&self, state: &State) {
        let saved = core::Saved {
            config: self.config(),
            lifetime_clicks: state.lifetime_clicks,
            lifetime_ms: state.lifetime_ms,
        };
        if self.test_instance && !self.file.parent().is_some_and(Path::exists) {
            return;
        }
        if let Err(error) = crate::integrations::private_file::write(
            &self.file,
            core::store(&saved).as_bytes(),
            FILE_CAP,
        ) {
            tracing::warn!(%error, "Autoclicker settings not saved");
        }
    }

    /// Replaces the config; re-registers the hotkeys that changed.
    fn set_config(&self, state: &mut State, config: AutoclickerConfig) -> Result<(), IslandError> {
        core::validate(&config).map_err(IslandError::invalid)?;
        let (hotkey_changed, stop_changed) = {
            let mut current = self
                .config
                .lock()
                .map_err(|_| IslandError::internal("Autoclicker config poisoned"))?;
            let changed = (
                current.hotkey != config.hotkey,
                current.stop_hotkey != config.stop_hotkey,
            );
            *current = config.clone();
            changed
        };
        self.persist(state);
        if hotkey_changed && state.armed {
            if let Some(h) = &self.hotkey {
                h.set(Some(config.hotkey.clone()));
            }
        }
        if stop_changed {
            self.sync_running(state);
        }
        Ok(())
    }

    pub fn command(&self, op: AutoclickerOp) -> Result<AutoclickerChanged, IslandError> {
        let mut state = self.lock()?;
        match op {
            AutoclickerOp::Configure { config } => self.set_config(&mut state, config)?,
            AutoclickerOp::Arm { armed } => {
                state.armed = armed;
                state.held = false;
                if let Some(h) = &self.hotkey {
                    h.set(armed.then(|| self.config().hotkey));
                }
                if !armed {
                    self.stop_locked(&mut state);
                }
            }
            AutoclickerOp::Start {} => {
                let delay = Duration::from_secs(self.config().start_delay_s.into());
                self.start_locked(&mut state, delay)?;
            }
            AutoclickerOp::Stop {} => self.stop_locked(&mut state),
            AutoclickerOp::ResetStats {} => {
                state.lifetime_clicks = 0;
                state.lifetime_ms = 0;
                self.persist(&state);
            }
        }
        let snap = self.snapshot(&mut state);
        self.topic.publish(snap.clone());
        Ok(snap)
    }

    /// The stop hotkey and the notch follow whether a run is active.
    fn sync_running(&self, state: &State) {
        let active = state.run.is_some();
        self.running
            .send_if_modified(|r| std::mem::replace(r, active) != active);
        if let Some(h) = &self.stop_key {
            h.set(active.then(|| self.config().stop_hotkey));
        }
    }

    fn stop_locked(&self, state: &mut State) {
        if let Some(run) = state.run.take() {
            run.stop.store(true, Ordering::Relaxed);
            // Shown until the run's thread reports the exact totals a moment later.
            state.last_clicks = run.clicks.load(Ordering::Relaxed);
            state.last_session_ms = run
                .since
                .lock()
                .ok()
                .and_then(|s| *s)
                .map_or(0, |t| ms(t.elapsed()));
        }
        self.sync_running(state);
    }

    /// In burst mode the run stops after `burstCount` clicks.
    fn start_locked(&self, state: &mut State, delay: Duration) -> Result<(), IslandError> {
        if !state.available {
            return Err(IslandError::unavailable(unavailable_detail()));
        }
        if state.run.is_some() {
            return Ok(());
        }
        let config = self.config();
        let burst = (config.mode == TriggerMode::Burst).then_some(config.burst_count);
        let (device, fresh) = if self.test_instance {
            (None, false)
        } else {
            let mut slot = self
                .device
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
                        tracing::warn!(%error, "Autoclicker virtual device unavailable");
                        state.available = false;
                        state.detail = unavailable_detail();
                        return Err(IslandError::unavailable(unavailable_detail()));
                    }
                },
            }
        };
        let delay = if fresh {
            delay.max(DEVICE_SETTLE)
        } else {
            delay
        };
        let id = state.next_id;
        state.next_id += 1;
        let stop = Arc::new(AtomicBool::new(false));
        let starting = Arc::new(AtomicBool::new(!delay.is_zero()));
        let clicks = Arc::new(AtomicU64::new(0));
        let since = Arc::new(Mutex::new(None));
        state.run = Some(Run {
            id,
            stop: stop.clone(),
            starting: starting.clone(),
            clicks: clicks.clone(),
            since: since.clone(),
        });
        state.detail.clear();
        let job = Job {
            me: self.me.clone(),
            id,
            stop,
            starting,
            config: self.config.clone(),
            device,
            clicks,
            since,
            live: self.live_jobs.clone(),
            delay,
            burst,
        };
        self.live_jobs.fetch_add(1, Ordering::AcqRel);
        if let Err(error) = std::thread::Builder::new()
            .name("autoclicker".into())
            .spawn(move || job.run())
        {
            self.live_jobs.fetch_sub(1, Ordering::AcqRel);
            state.run = None;
            return Err(IslandError::internal(format!(
                "Autoclicker thread: {error}"
            )));
        }
        self.sync_running(state);
        Ok(())
    }

    fn finished(&self, id: u64, clicks: u64, clicked: Duration, error: Option<String>) {
        if let Ok(mut state) = self.lock() {
            if state.run.as_ref().is_some_and(|r| r.id == id) {
                state.run = None;
            }
            // A run started since keeps the display; the totals count every run.
            if state.run.is_none() {
                state.last_session_ms = ms(clicked);
                state.last_clicks = clicks;
            }
            state.lifetime_clicks = state.lifetime_clicks.saturating_add(clicks);
            state.lifetime_ms = state.lifetime_ms.saturating_add(ms(clicked));
            if let Some(error) = error {
                state.detail = error;
            }
            if clicks > 0 {
                self.persist(&state);
            }
            self.sync_running(&state);
            let snap = self.snapshot(&mut state);
            self.topic.publish(snap);
        }
    }

    /// Toggle and burst: a press flips; held-key auto-repeat inside the guard is ignored.
    fn toggle(&self, state: &mut State) {
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
            self.stop_locked(state);
        } else if let Err(error) = self.start_locked(state, Duration::ZERO) {
            tracing::warn!(%error, "Autoclicker hotkey start failed");
        }
    }

    fn on_key(&self, key: hotkey::Key) {
        let mode = self.config().mode;
        let Ok(mut state) = self.lock() else { return };
        if !state.armed {
            return;
        }
        match (mode, key) {
            (TriggerMode::Toggle | TriggerMode::Burst, hotkey::Key::Pressed) => {
                self.toggle(&mut state)
            }
            (TriggerMode::Hold, hotkey::Key::Pressed) => {
                state.held = true;
                if let Err(error) = self.start_locked(&mut state, Duration::ZERO) {
                    tracing::warn!(%error, "Autoclicker hotkey start failed");
                }
            }
            (TriggerMode::Hold, hotkey::Key::Released) => {
                state.held = false;
                self.stop_locked(&mut state);
            }
            (TriggerMode::Toggle | TriggerMode::Burst, hotkey::Key::Released) => state.held = false,
        }
        let snap = self.snapshot(&mut state);
        self.topic.publish(snap);
    }

    /// The emergency stop: any press ends the run, armed or not.
    fn on_stop_key(&self, key: hotkey::Key) {
        if key != hotkey::Key::Pressed {
            return;
        }
        let Ok(mut state) = self.lock() else { return };
        if state.run.is_none() {
            return;
        }
        state.held = false;
        self.stop_locked(&mut state);
        let snap = self.snapshot(&mut state);
        self.topic.publish(snap);
    }

    fn on_hotkey_status(&self, stop: bool, hotkey_state: &'static str, detail: String) {
        if let Ok(mut guard) = self.lock() {
            let state = &mut *guard;
            let (s, d) = if stop {
                (&mut state.stop_state, &mut state.stop_detail)
            } else {
                (&mut state.hotkey_state, &mut state.hotkey_detail)
            };
            if *s == hotkey_state && *d == detail {
                return;
            }
            *s = hotkey_state;
            *d = detail;
            let snap = self.snapshot(state);
            self.topic.publish(snap);
        }
    }

    /// Stops clicking (letting a held button go and saving the totals) and releases the
    /// hotkeys, all bounded.
    pub fn shutdown(&self) {
        if let Ok(mut state) = self.lock() {
            self.stop_locked(&mut state);
        }
        let until = Instant::now() + SHUTDOWN_WAIT;
        while self.live_jobs.load(Ordering::Acquire) > 0 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        for h in [&self.hotkey, &self.stop_key].into_iter().flatten() {
            h.shutdown();
        }
    }
}

/// What one press sends, captured when it goes down so the matching release always follows.
#[derive(Clone)]
enum Press {
    Button(crate::contract::apps::ClickButton),
    Key(keys::Chord),
}

impl Press {
    fn of(c: &AutoclickerConfig) -> Self {
        match (c.target, keys::chord(&c.key)) {
            (ClickTarget::Key, Some(chord)) => Self::Key(chord),
            _ => Self::Button(c.button),
        }
    }

    fn down(&self, dev: &uinput::Mouse) -> std::io::Result<()> {
        match self {
            Self::Button(b) => dev.button(*b, true),
            Self::Key(chord) => {
                for m in &chord.modifiers {
                    dev.key(*m, true)?;
                }
                dev.key(chord.key, true)
            }
        }
    }

    /// Releases everything `down` may have pressed; every release is tried.
    fn up(&self, dev: &uinput::Mouse) -> std::io::Result<()> {
        match self {
            Self::Button(b) => dev.button(*b, false),
            Self::Key(chord) => {
                let mut result = dev.key(chord.key, false);
                for m in chord.modifiers.iter().rev() {
                    result = result.and(dev.key(*m, false));
                }
                result
            }
        }
    }
}

struct Job {
    me: Weak<Autoclicker>,
    id: u64,
    stop: Arc<AtomicBool>,
    starting: Arc<AtomicBool>,
    config: Arc<Mutex<AutoclickerConfig>>,
    device: Option<Arc<uinput::Mouse>>,
    clicks: Arc<AtomicU64>,
    since: Arc<Mutex<Option<Instant>>>,
    live: Arc<AtomicUsize>,
    delay: Duration,
    burst: Option<u32>,
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

    /// One press and release, held for `press_ms`. The release is sent even if the run is
    /// stopped meanwhile.
    fn tap(&self, dev: &uinput::Mouse, press: &Press, press_ms: u32) -> std::io::Result<()> {
        press.down(dev)?;
        if press_ms > 0 {
            std::thread::sleep(Duration::from_millis(press_ms.into()));
        }
        press.up(dev)
    }

    fn click(&self, c: &AutoclickerConfig) -> std::io::Result<()> {
        let Some(dev) = &self.device else {
            return Ok(());
        };
        let press = Press::of(c);
        self.tap(dev, &press, c.press_ms)?;
        if c.kind == ClickKind::Double {
            let gap = Duration::from_millis((u64::from(c.interval_ms) / 3).clamp(1, 30));
            std::thread::sleep(gap);
            self.tap(dev, &press, c.press_ms)?;
        }
        Ok(())
    }

    fn over_time(c: &AutoclickerConfig, started: Instant) -> bool {
        c.time_limit_s > 0 && started.elapsed() >= Duration::from_secs(c.time_limit_s.into())
    }

    /// Hold-down: one press kept until the run stops (or its time limit).
    fn hold(&self, c: &AutoclickerConfig, started: Instant) -> Option<String> {
        let press = Press::of(c);
        if let Some(dev) = &self.device {
            if let Err(e) = press.down(dev) {
                let _ = press.up(dev);
                return Some(format!("Clicking stopped: {e}"));
            }
        }
        self.clicks.fetch_add(1, Ordering::Relaxed);
        self.publish();
        loop {
            let limit = self.config.lock().map(|c| c.time_limit_s).unwrap_or(0);
            let tick = Instant::now() + HOLD_PUBLISH_EVERY;
            let until = if limit > 0 {
                tick.min(started + Duration::from_secs(limit.into()))
            } else {
                tick
            };
            if !self.wait_until(until)
                || (limit > 0 && started.elapsed().as_secs() >= u64::from(limit))
            {
                break;
            }
            self.publish();
        }
        if let Some(dev) = &self.device {
            if let Err(e) = press.up(dev) {
                return Some(format!("Couldn't release the button: {e}"));
            }
        }
        None
    }

    fn clicks(&self, started: Instant, mut c: AutoclickerConfig) -> Option<String> {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        let mut jitter = core::Jitter::new(seed);
        let mut next = started;
        let mut last_publish = started;
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return None;
            }
            let done = self.clicks.load(Ordering::Relaxed);
            let limit = match (self.burst, c.click_limit) {
                (Some(b), 0) => b,
                (Some(b), l) => b.min(l),
                (None, l) => l,
            };
            if (limit > 0 && done >= u64::from(limit)) || Self::over_time(&c, started) {
                return None;
            }
            if let Err(e) = self.click(&c) {
                tracing::warn!(error = %e, "Autoclicker click failed");
                return Some(format!("Clicking stopped: {e}"));
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
                return None;
            }
            c = match self.config.lock() {
                Ok(c) => c.clone(),
                Err(_) => return None,
            };
            // Switched to hold-down mid-run: the next pass holds instead.
            if c.kind == ClickKind::Hold {
                return self.hold(&c, started);
            }
        }
    }

    fn run(self) {
        let mut error = None;
        let mut clicked = Duration::ZERO;
        if self.wait_until(Instant::now() + self.delay) {
            let started = Instant::now();
            if let Ok(mut since) = self.since.lock() {
                *since = Some(started);
            }
            if self.starting.swap(false, Ordering::Relaxed) {
                self.publish();
            }
            if let Ok(c) = self.config.lock().map(|c| c.clone()) {
                error = if c.kind == ClickKind::Hold {
                    self.hold(&c, started)
                } else {
                    self.clicks(started, c)
                };
            }
            clicked = started.elapsed();
        }
        let clicks = self.clicks.load(Ordering::Relaxed);
        if let Some(s) = self.me.upgrade() {
            s.finished(self.id, clicks, clicked, error);
        }
        self.live.fetch_sub(1, Ordering::AcqRel);
    }
}
