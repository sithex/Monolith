//! Host side of the state actor: the effect sink (typed events + window effects) and the
//! managed handle. `machine.rs`/`actor.rs` stay Tauri-free; everything that touches the app
//! lives here (SPEC §8 Phase 3, "Effects reach the window from Rust").
use super::{
    actor::{self, Handle, MonotonicClock, Sink, Snapshot},
    machine::Effect,
};
use crate::{
    bus,
    contract::{events::CueKind, settings::Settings},
    error::IslandError,
    window::ISLAND_LABEL,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::Manager;

/// Applies actor effects. Runs on the actor task, so it only enqueues: `emit_to` and
/// `set_focus` hand work to Tauri's own dispatchers and never wait on the GTK main thread.
pub struct IslandSink {
    app: tauri::AppHandle,
}

impl IslandSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }

    /// `RequestFocus` — the explicit-pin path of SPEC §2.5 (panel pin / tray / tab selection).
    /// A refused focus request is logged, not fatal: losing keyboard focus must not take the
    /// state authority down. The GUI gate asserts that the request actually works.
    fn focus(&self) {
        match self.app.get_webview_window(ISLAND_LABEL) {
            Some(window) => {
                if let Err(error) = window.set_focus() {
                    tracing::warn!(%error, "Island focus request failed");
                }
            }
            None => tracing::debug!("Focus requested before the island window exists"),
        }
    }
}

impl Sink for IslandSink {
    fn apply(&mut self, effects: &[Effect], snapshot: &Snapshot) -> Result<(), IslandError> {
        // State first: the UI must never lag behind a cue or a focus change.
        if effects.contains(&Effect::EmitState) {
            bus::emit_state(&self.app, snapshot.state.clone())?;
        }
        for effect in effects {
            match effect {
                Effect::SetClipboardPaused(paused) => {
                    self.app
                        .state::<Arc<crate::integrations::clipboard::Clipboard>>()
                        .set_paused(*paused)?;
                }
                Effect::EmitState => {}
                Effect::PlayClick => bus::emit_cue(&self.app, CueKind::Click)?,
                Effect::RequestFocus => self.focus(),
                // The PTY is spawned lazily by `terminal_attach` once the renderer mounts the
                // terminal (`terminalStarted` in the emitted state is what mounts it).
                Effect::StartTerminal => tracing::debug!("Terminal start requested"),
                Effect::Quit => self.app.exit(0),
                Effect::RestartTerminal => {
                    let terminal = self
                        .app
                        .state::<crate::services::Services>()
                        .terminal
                        .clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        if let Err(error) = terminal.restart() {
                            tracing::warn!(%error, "Tray terminal restart failed");
                        }
                    });
                }
            }
        }
        Ok(())
    }
}

/// Managed Tauri state: the single `IslandState` authority.
pub struct IslandActor {
    pub handle: Handle,
    task: Mutex<Option<tokio::task::JoinHandle<Result<(), IslandError>>>>,
}

impl IslandActor {
    /// Must be called after the settings are loaded and before the window (and therefore the
    /// first `island_bootstrap`) exists.
    pub fn start(app: tauri::AppHandle, settings: Settings) -> Self {
        let (handle, task) = tauri::async_runtime::block_on(async move {
            actor::spawn(
                settings,
                Arc::new(MonotonicClock::default()),
                IslandSink::new(app),
            )
        });
        Self {
            handle,
            task: Mutex::new(Some(task)),
        }
    }

    /// Stop the actor and report how it ended (RunEvent::Exit).
    pub fn shutdown(&self) -> Result<(), IslandError> {
        self.handle.shutdown();
        let task = self
            .task
            .lock()
            .map_err(|_| IslandError::internal("actor slot is poisoned"))?
            .take();
        let Some(task) = task else {
            return Ok(());
        };
        let stopped = tauri::async_runtime::block_on(async move {
            tokio::time::timeout(Duration::from_secs(2), task).await
        });
        match stopped {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(IslandError::internal(format!("actor task failed: {error}"))),
            Err(_) => Err(IslandError::internal("actor did not stop within 2 s")),
        }
    }
}
