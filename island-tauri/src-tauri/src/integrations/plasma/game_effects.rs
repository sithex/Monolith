//! Transient game effects. Never patches or persists Settings (especially `dnd`).
use crate::contract::settings::Settings;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameEffects {
    pub suppress_notifications: bool,
    pub pause_pollers: bool,
    pub freeze_companion: bool,
}
impl GameEffects {
    pub fn plan(game: bool, settings: &Settings) -> Self {
        Self {
            suppress_notifications: game && settings.game_mode_dnd,
            pause_pollers: game && settings.game_mode_pause_pollers,
            freeze_companion: game && settings.game_mode_freeze_companion,
        }
    }
    pub fn effective_dnd(self, manual: bool) -> bool {
        manual || self.suppress_notifications
    }
}

/// Shared by workers, including services enabled while already paused. Changes wake both
/// async workers and the agents thread; a resume always starts a fresh iteration immediately.
#[derive(Clone)]
pub struct PollPause(Arc<PauseInner>);
struct PauseInner {
    value: watch::Sender<bool>,
    generation: Mutex<u64>,
    wake: Condvar,
}
impl Default for PollPause {
    fn default() -> Self {
        Self(Arc::new(PauseInner {
            value: watch::channel(false).0,
            generation: Mutex::new(0),
            wake: Condvar::new(),
        }))
    }
}
impl PollPause {
    pub fn set(&self, paused: bool) {
        let mut generation = self.0.generation.lock().unwrap_or_else(|e| e.into_inner());
        if *self.0.value.borrow() != paused {
            self.0.value.send_replace(paused);
            *generation = generation.wrapping_add(1);
            self.0.wake.notify_all();
        }
    }
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.0.value.subscribe()
    }
    pub fn paused(&self) -> bool {
        *self.0.value.borrow()
    }
    pub fn generation(&self) -> u64 {
        *self.0.generation.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Interruptible cadence for the synchronous agents reader. The stop predicate is
    /// inspected under the same generation lock used by wake_stop, preventing lost wakeups.
    pub fn wait_blocking(&self, seen: u64, duration: Duration, stopped: impl Fn() -> bool) {
        let generation = self.0.generation.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self
            .0
            .wake
            .wait_timeout_while(generation, duration, |generation| {
                *generation == seen && !stopped()
            });
    }
    pub fn wake_stop(&self) {
        let _generation = self.0.generation.lock().unwrap_or_else(|e| e.into_inner());
        self.0.wake.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct HttpProbe {
        base: String,
        hits: Arc<AtomicUsize>,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl HttpProbe {
        fn new() -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let base = format!("http://{}/v1/forecast", listener.local_addr().unwrap());
            let hits = Arc::new(AtomicUsize::new(0));
            let stop = Arc::new(AtomicBool::new(false));
            let (count, stopped) = (hits.clone(), stop.clone());
            let thread = std::thread::spawn(move || {
                while !stopped.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream
                                .set_read_timeout(Some(Duration::from_secs(1)))
                                .unwrap();
                            let _ = stream.read(&mut [0; 4096]);
                            count.fetch_add(1, Ordering::SeqCst);
                            // Deliberately invalid weather data exercises the real HTTP error/backoff path.
                            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2))
                        }
                        Err(e) => panic!("HTTP probe: {e}"),
                    }
                }
            });
            Self {
                base,
                hits,
                stop,
                thread: Some(thread),
            }
        }
        async fn wait_for(&self, count: usize) {
            tokio::time::timeout(Duration::from_secs(2), async {
                while self.hits.load(Ordering::SeqCst) < count {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
        }
    }
    impl Drop for HttpProbe {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            self.thread.take().unwrap().join().unwrap();
        }
    }
    #[tokio::test]
    async fn game_effects_weather_worker_pauses_and_resume_bypasses_hourly_cadence() {
        use crate::contract::{settings::TempUnit, system::GeoPlace};
        use crate::integrations::weather::{WeatherConfig, WeatherService, WeatherSettings};
        let probe = HttpProbe::new();
        let pause = PollPause::default();
        pause.set(true);
        let weather = WeatherService::new_gated(
            WeatherConfig {
                forecast_base: probe.base.clone(),
                geocode_base: probe.base.clone(),
                period_secs: 3600,
            },
            WeatherSettings {
                enabled: true,
                unit: TempUnit::Celsius,
                location: Some(GeoPlace {
                    name: "Fixture".into(),
                    latitude: 0.,
                    longitude: 0.,
                    country: "".into(),
                }),
            },
            pause.clone(),
        )
        .unwrap();
        weather.start();
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(probe.hits.load(Ordering::SeqCst), 0);
        pause.set(false);
        probe.wait_for(1).await;
        pause.set(true);
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(probe.hits.load(Ordering::SeqCst), 1);
        pause.set(false);
        probe.wait_for(2).await;
        tokio::task::spawn_blocking(move || weather.shutdown())
            .await
            .unwrap();
    }
    #[test]
    fn game_effects_plan_is_opt_in_per_effect_and_does_not_save_dnd() {
        let mut s = Settings::default();
        let before = s.clone();
        assert_eq!(GameEffects::plan(false, &s), GameEffects::default());
        let game = GameEffects::plan(true, &s);
        assert!(game.effective_dnd(false) && game.pause_pollers && game.freeze_companion);
        assert_eq!(s, before);
        s.game_mode_dnd = false;
        s.game_mode_pause_pollers = false;
        s.game_mode_freeze_companion = false;
        assert_eq!(GameEffects::plan(true, &s), GameEffects::default());
        assert!(GameEffects::plan(false, &s).effective_dnd(true));
    }
    #[tokio::test]
    async fn game_effects_resume_wakes_immediately_and_repeated_settings_do_not_wake() {
        let pause = PollPause::default();
        let mut changes = pause.subscribe();
        pause.set(true);
        changes.changed().await.unwrap();
        assert!(*changes.borrow_and_update());
        let generation = pause.generation();
        pause.set(true);
        assert!(!changes.has_changed().unwrap());
        assert_eq!(generation, pause.generation());
        pause.set(false);
        tokio::time::timeout(Duration::from_millis(100), changes.changed())
            .await
            .unwrap()
            .unwrap();
        assert!(!*changes.borrow_and_update());
        let start = std::time::Instant::now();
        pause.wait_blocking(generation, Duration::from_secs(60), || false);
        assert!(start.elapsed() < Duration::from_millis(100));
    }
}
