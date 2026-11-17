//! One revision/value pair for Phase 7a's visual stores, without changing legacy events.
//! Sampling is serialized with preference writes; delayed old wakeups sample CURRENT values.
use crate::{
    contract::frontend::{Appearance, VisualChanged},
    error::IslandError,
    services::{autostart::atomic_write, Services},
    window::ISLAND_LABEL,
};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::Manager;
use tauri_specta::Event;

struct State {
    appearance: Appearance,
    current: Option<VisualChanged>,
}
pub struct VisualService {
    file: PathBuf,
    state: Mutex<State>,
}
impl VisualService {
    pub fn new(data: PathBuf) -> Result<Self, IslandError> {
        let file = data.join("appearance.json");
        let appearance = match fs::read(&file) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| IslandError::invalid("Invalid appearance preference"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Appearance::Skin,
            Err(_) => return Err(IslandError::io("Cannot read appearance preference")),
        };
        Ok(Self {
            file,
            state: Mutex::new(State {
                appearance,
                current: None,
            }),
        })
    }
    /// Native startup must not wait behind a visual snapshot computing the skin palette.
    pub fn appearance(&self) -> Option<Appearance> {
        self.state.try_lock().ok().map(|state| state.appearance)
    }

    pub fn snapshot(
        &self,
        services: &Services,
        choice: Option<Appearance>,
    ) -> Result<VisualChanged, IslandError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| IslandError::internal("Visual state poisoned"))?;
        let settings = services.settings.get()?;
        let palette = services.palette.get()?;
        if let Some(choice) = choice {
            let bytes = serde_json::to_vec(&choice)
                .map_err(|_| IslandError::internal("Cannot encode appearance"))?;
            atomic_write(&self.file, &bytes, "Cannot persist appearance")?;
            state.appearance = choice;
        }
        let mut next = VisualChanged {
            rev: state.current.as_ref().map_or(0, |v| v.rev),
            settings,
            palette,
            appearance: state.appearance,
        };
        if state.current.as_ref() != Some(&next) {
            next.rev = next.rev.wrapping_add(1);
            state.current = Some(next.clone());
        }
        Ok(next)
    }
}

/// Called only from existing appearance/settings wakeups. No timer or renderer polling.
pub fn publish(app: &tauri::AppHandle) {
    if app.state::<Services>().runtime.ui != crate::contract::island::UiKind::Island {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(visual) = app.try_state::<VisualService>() else {
            return;
        };
        match visual.snapshot(&app.state::<Services>(), None) {
            Ok(snapshot) => {
                if let Err(error) = snapshot.emit_to(&app, ISLAND_LABEL) {
                    tracing::warn!(%error, "VisualChanged emit failed");
                }
            }
            Err(error) => tracing::warn!(%error, "Visual snapshot unavailable"),
        }
    });
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use std::{sync::Arc, time::Duration};

    #[test]
    fn native_appearance_selection_does_not_wait_for_a_visual_snapshot() {
        let dir = crate::services::scratch("native-appearance-selection");
        fs::write(dir.join("appearance.json"), br#""ocean""#).unwrap();
        let visual = Arc::new(VisualService::new(dir.clone()).unwrap());
        assert_eq!(visual.appearance(), Some(Appearance::Ocean));
        let held = visual.state.lock().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = {
            let visual = visual.clone();
            std::thread::spawn(move || {
                tx.send(visual.appearance()).unwrap();
            })
        };
        let selection = rx.recv_timeout(Duration::from_millis(250));
        drop(held);
        worker.join().unwrap();
        assert_eq!(
            selection.unwrap(),
            None,
            "busy visual state means not ready, not wait"
        );
        assert_eq!(visual.appearance(), Some(Appearance::Ocean));
        fs::remove_dir_all(dir).unwrap();
    }
}
