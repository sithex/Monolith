//! Additive Controls clipboard write adapter. Never enables history monitoring implicitly.
use super::{history::ClipContent, lock, Clipboard};
use crate::error::IslandError;
impl Clipboard {
    /// Explicit colour action only. A disabled clipboard stays disabled (history capture is
    /// never switched on implicitly): without the worker the colour goes through `wl-copy`.
    pub fn copy_colour_text(&self, text: &str) -> Result<(), IslandError> {
        if text.len() != 7
            || !text.starts_with('#')
            || !text[1..].bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(IslandError::invalid("Invalid colour clipboard payload"));
        }
        let worker = lock(&self.worker)?;
        match worker.as_ref() {
            Some(worker) => worker.copy(ClipContent::Text { text: text.into() }),
            // History off: hand the colour to `wl-copy`, which keeps serving it after we
            // return. Nothing starts watching the clipboard.
            None => {
                drop(worker);
                wl_copy(text)
            }
        }
    }
}

/// One-off copy through wl-clipboard's `wl-copy` (fixed argv, the validated colour on stdin).
/// `wl-copy` forks a small server that holds the selection; its first process exits at once.
fn wl_copy(text: &str) -> Result<(), IslandError> {
    use std::{
        io::Write,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let missing = || {
        IslandError::unavailable(
            "Install wl-clipboard, or turn on clipboard history, to copy picked colours",
        )
    };
    let mut child = Command::new("wl-copy")
        .args(["--type", "text/plain"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| missing())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| IslandError::unavailable(format!("wl-copy: {e}")))?;
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => {
                return Err(IslandError::unavailable("wl-copy couldn't copy the colour"))
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(IslandError::timeout("wl-copy timed out"));
            }
        }
    }
}
