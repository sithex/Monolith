use super::blocking;
use crate::{
    contract::terminal::{TerminalEvent, TerminalSnapshot},
    error::IslandError,
    services::{terminal::TerminalSink, Services},
};
use std::sync::Arc;
use tauri::ipc::Channel;

/// Spawn the PTY on first call; later calls only replace the output channel (idempotent).
/// Returns the snapshot (replay + `seq`); the channel then carries only newer chunks.
#[tauri::command]
#[specta::specta]
pub async fn terminal_attach(
    on_event: Channel<TerminalEvent>,
    services: tauri::State<'_, Services>,
) -> Result<TerminalSnapshot, IslandError> {
    let terminal = Arc::clone(&services.terminal);
    let sink: Arc<dyn TerminalSink> =
        Arc::new(move |event: TerminalEvent| on_event.send(event).is_ok());
    blocking(move || terminal.attach(sink)).await
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_input(
    data: String,
    services: tauri::State<'_, Services>,
) -> Result<(), IslandError> {
    let terminal = Arc::clone(&services.terminal);
    blocking(move || terminal.input(&data)).await
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_resize(
    cols: u16,
    rows: u16,
    services: tauri::State<'_, Services>,
) -> Result<(), IslandError> {
    let terminal = Arc::clone(&services.terminal);
    blocking(move || terminal.resize(cols, rows)).await
}

/// Explicit user-initiated copy, independent of clipboard-history capture and
/// WebKit's native Copy accelerator. GTK owns the clipboard on the main thread.
#[tauri::command]
#[specta::specta]
pub async fn terminal_copy(text: String, window: tauri::WebviewWindow) -> Result<(), IslandError> {
    if text.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        window
            .run_on_main_thread(move || {
                let result = gdk::Display::default()
                    .ok_or_else(|| IslandError::unavailable("clipboard display unavailable"))
                    .map(|display| {
                        gtk::Clipboard::for_display(&display, &gdk::SELECTION_CLIPBOARD)
                            .set_text(&text);
                    });
                let _ = tx.send(result);
            })
            .map_err(|_| IslandError::unavailable("clipboard main thread unavailable"))?;
        rx.await
            .map_err(|_| IslandError::unavailable("clipboard write interrupted"))?
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = window;
        Err(IslandError::unsupported("native terminal copy unavailable"))
    }
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_restart(
    services: tauri::State<'_, Services>,
) -> Result<TerminalSnapshot, IslandError> {
    let terminal = Arc::clone(&services.terminal);
    blocking(move || terminal.restart()).await
}

/// The renderer has parsed everything up to `seq`: grant flow-control credit. Unknown or stale
/// sequence numbers are ignored, not errors.
#[tauri::command]
#[specta::specta]
pub async fn terminal_ack(
    seq: u32,
    services: tauri::State<'_, Services>,
) -> Result<(), IslandError> {
    services.terminal.ack(seq);
    Ok(())
}

/// Longest link the terminal will hand to the browser (in UTF-8 bytes).
const MAX_LINK_LEN: usize = 8192;

/// Only bounded http(s) links without whitespace or control characters may reach xdg-open.
fn checked_link(uri: &str) -> Result<&str, IslandError> {
    let scheme_ok = uri
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        || uri
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"));
    if !scheme_ok
        || uri.len() > MAX_LINK_LEN
        || uri.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(IslandError::invalid("Only http(s) links can be opened"));
    }
    Ok(uri)
}

/// Keep the fallback explicit: slicing the systemd argv risks trying to execute `--`.
fn spawn_link_with<T>(
    uri: &str,
    mut spawn: impl FnMut(&[&str]) -> std::io::Result<T>,
) -> std::io::Result<T> {
    spawn(&[
        "systemd-run",
        "--user",
        "--scope",
        "--collect",
        "--quiet",
        "--",
        "xdg-open",
        uri,
    ])
    .or_else(|_| spawn(&["xdg-open", uri]))
}

/// Override xterm's blocking confirm/window.open for OSC 8 links. Launch in a separate user
/// scope (or a detached child if systemd-run is absent), without waiting for the browser.
#[tauri::command]
#[specta::specta]
pub async fn terminal_open_link(uri: String) -> Result<(), IslandError> {
    blocking(move || {
        let uri = checked_link(&uri)?;
        let mut child = spawn_link_with(uri, |argv| {
            let mut command = std::process::Command::new(argv[0]);
            command
                .args(&argv[1..])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                // SAFETY: setsid is async-signal-safe and touches no Rust shared state.
                // Like the sign-in launcher, the fallback must not share our terminal session.
                unsafe {
                    command.pre_exec(|| {
                        if libc::setsid() < 0 {
                            Err(std::io::Error::last_os_error())
                        } else {
                            Ok(())
                        }
                    });
                }
            }
            command.spawn()
        })
        .map_err(|error| IslandError::unavailable(format!("Could not open link: {error}")))?;
        // Match the tray's detached-launch idiom: null stdio, then reap off the IPC thread.
        std::thread::Builder::new()
            .name("open-link".into())
            .spawn(move || {
                let _ = child.wait();
            })
            .map_err(|error| IslandError::internal(format!("open-link reaper: {error}")))?;
        Ok(())
    })
    .await
}

#[cfg(test)]
mod link_tests {
    use super::{checked_link, spawn_link_with, MAX_LINK_LEN};

    #[test]
    fn plain_http_links_pass_unchanged() {
        for uri in [
            "https://example.com/a?b=c#d",
            "http://localhost:8080/",
            "HTTPS://Example.com",
            "hTtP://example.com/é",
            "https://example.com/a;$(not-a-shell)?x='quoted'&y=%20",
        ] {
            assert_eq!(checked_link(uri).unwrap(), uri);
        }
    }

    #[test]
    fn non_http_schemes_and_options_are_refused() {
        for uri in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://example.com",
            "data:text/html,hello",
            "mailto:test@example.com",
            "//example.com",
            "https:example.com",
            "--help",
            "",
        ] {
            assert!(checked_link(uri).is_err(), "{uri:?}");
        }
    }

    #[test]
    fn whitespace_and_controls_are_refused_anywhere() {
        for character in [
            ' ', '\t', '\r', '\n', '\0', '\u{1b}', '\u{7f}', '\u{85}', '\u{a0}', '\u{2003}',
            '\u{2028}',
        ] {
            for uri in [
                format!("{character}https://example.com"),
                format!("https://example.com/a{character}b"),
                format!("https://example.com{character}"),
            ] {
                assert!(checked_link(&uri).is_err(), "{uri:?}");
            }
        }
    }

    #[test]
    fn link_limit_is_in_bytes_and_inclusive() {
        let boundary = format!("https://{}", "a".repeat(MAX_LINK_LEN - 8));
        assert_eq!(checked_link(&boundary).unwrap(), boundary);
        assert!(checked_link(&format!("{boundary}a")).is_err());
        let unicode = format!("https://{}", "é".repeat((MAX_LINK_LEN - 8) / 2));
        assert_eq!(checked_link(&unicode).unwrap(), unicode);
        assert!(checked_link(&format!("{unicode}é")).is_err());
    }

    #[test]
    fn systemd_launch_preserves_the_url_as_one_argument_without_a_shell() {
        let uri = "https://example.com/a;$(not-a-shell)?x='quoted'&y=%20";
        let mut calls = Vec::new();
        spawn_link_with(uri, |argv| {
            calls.push(argv.iter().map(|arg| arg.to_string()).collect::<Vec<_>>());
            Ok(())
        })
        .unwrap();
        assert_eq!(
            calls,
            [vec![
                "systemd-run",
                "--user",
                "--scope",
                "--collect",
                "--quiet",
                "--",
                "xdg-open",
                uri
            ]]
        );
    }

    #[test]
    fn missing_systemd_falls_back_to_xdg_open_not_an_argv_slice() {
        let uri = "https://example.com/?one=1&two=2";
        let mut calls = Vec::new();
        spawn_link_with(uri, |argv| {
            calls.push(argv.iter().map(|arg| arg.to_string()).collect::<Vec<_>>());
            if calls.len() == 1 {
                Err(std::io::Error::from(std::io::ErrorKind::NotFound))
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1], ["xdg-open", uri]);
    }

    #[test]
    fn launcher_failure_is_returned() {
        let mut calls = 0;
        let result: std::io::Result<()> = spawn_link_with("https://example.com", |_| {
            calls += 1;
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        });
        assert_eq!(calls, 2);
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }
}
