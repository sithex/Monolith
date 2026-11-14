//! Optional subsystems as command arguments.
//!
//! The island starts even when an optional subsystem (notifications, system services) could
//! not be built; that subsystem is then simply not managed. A command taking
//! `tauri::State<T>` for it would answer with Tauri's developer message ("state not managed
//! …"). `Managed<T>` answers with a typed `unavailable` error that names the subsystem.
use crate::error::IslandError;
use std::ops::Deref;
use tauri::{
    ipc::{CommandArg, CommandItem, InvokeError},
    Manager, Runtime,
};

/// A subsystem that may be missing after a degraded start.
pub trait Subsystem: Send + Sync + 'static {
    /// One plain sentence for the page.
    const UNAVAILABLE: &'static str;
}

impl Subsystem for crate::integrations::notifications::hub::Notifications {
    const UNAVAILABLE: &'static str = "Notifications are unavailable: they failed to start";
}
impl Subsystem for std::sync::Arc<crate::integrations::system_services::SystemServices> {
    const UNAVAILABLE: &'static str = "System services are unavailable: they failed to start";
}
impl Subsystem for std::sync::Arc<crate::integrations::clipboard::Clipboard> {
    const UNAVAILABLE: &'static str = "Clipboard history is unavailable: it failed to start";
}

/// The error a command returns when its subsystem is missing.
pub fn missing<T: Subsystem>() -> IslandError {
    IslandError::unavailable(T::UNAVAILABLE)
}

/// Looks a subsystem up outside a command (host code, the settings path).
pub fn require<T: Subsystem, R: Runtime>(app: &impl Manager<R>) -> Result<tauri::State<'_, T>, IslandError> {
    app.try_state::<T>().ok_or_else(missing::<T>)
}

/// `tauri::State<T>` for an optional subsystem: a typed `unavailable` error when missing.
pub struct Managed<'r, T: Subsystem>(tauri::State<'r, T>);

impl<'r, T: Subsystem> Managed<'r, T> {
    pub fn inner(&self) -> &'r T {
        self.0.inner()
    }
}
impl<T: Subsystem> Deref for Managed<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0.inner()
    }
}
impl<'r, 'de: 'r, T: Subsystem, R: Runtime> CommandArg<'de, R> for Managed<'r, T> {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        command
            .message
            .webview_ref()
            .try_state::<T>()
            .map(Managed)
            .ok_or_else(|| InvokeError::from(missing::<T>()))
    }
}
impl<T: Subsystem> specta::function::FunctionArg for Managed<'_, T> {
    fn to_datatype(_: &mut specta::Types) -> Option<specta::datatype::DataType> {
        None
    }
}
