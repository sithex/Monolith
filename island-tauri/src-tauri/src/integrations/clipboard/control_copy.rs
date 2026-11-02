//! Additive Controls clipboard write adapter. Never enables history monitoring implicitly.
use super::{history::ClipContent, lock, Clipboard};
use crate::error::IslandError;
impl Clipboard {
    /// Explicit colour action only. Disabled clipboard stays disabled: callers receive a
    /// dependency-unavailable error rather than silently enabling privacy-sensitive capture.
    pub fn copy_colour_text(&self, text: &str) -> Result<(), IslandError> {
        if text.len() != 7
            || !text.starts_with('#')
            || !text[1..].bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(IslandError::invalid("Invalid colour clipboard payload"));
        }
        let worker = lock(&self.worker)?;
        worker
            .as_ref()
            .ok_or_else(|| {
                IslandError::unavailable("Enable the Clipboard module to copy a picked colour")
            })?
            .copy(ClipContent::Text { text: text.into() })
    }
}
