//! Non-Linux placeholder: the Windows (`WH_MOUSE_LL` crossing logic) and macOS (`NSEvent`
//! monitors) backends are Phase 8. Until then every call reports `unsupported` explicitly.

use super::InputRegionBackend;
use crate::error::IslandError;
use crate::window::shape::ShapeRow;

pub struct PlatformBackend;

impl PlatformBackend {
    pub fn new(_window: tauri::WebviewWindow) -> Result<Self, IslandError> {
        Err(IslandError::unsupported(
            "input region backend is not implemented on this platform yet",
        ))
    }

    /// Phase 2.5 spike hook (Linux/KWin only); a no-op placeholder here.
    pub fn with_blur_region(self, _on: bool) -> Self {
        self
    }
}

impl InputRegionBackend for PlatformBackend {
    fn apply(&mut self, _region: &[ShapeRow]) -> Result<(), IslandError> {
        Err(IslandError::unsupported("input region backend"))
    }
    fn readback(&self) -> Result<Vec<ShapeRow>, IslandError> {
        Err(IslandError::unsupported("input region backend"))
    }
}
