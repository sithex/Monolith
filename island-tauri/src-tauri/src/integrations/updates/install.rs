//! The Update button: opens Konsole running the package manager's full upgrade, so the
//! transaction, its prompts and the password stay with the user in a real terminal. The
//! island never runs a package manager itself, and nothing from the page reaches the command:
//! the argv is one of three fixed constants. Konsole runs in its own systemd scope, so quitting
//! the island never kills an upgrade halfway. One update window at a time; the list is
//! re-checked when it closes.
use crate::{
    contract::controls::{UpdateManager, UpdatesBackend},
    error::IslandError,
};
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
};

static OPEN: AtomicBool = AtomicBool::new(false);

/// Shown after the upgrade so the window doesn't vanish before it can be read.
const TAIL: &str = r#"status=$?; echo; if [ "$status" -eq 0 ]; then echo "Update finished."; else echo "The update stopped (exit code $status)."; fi; read -rsn1 -p "Press any key to close this window." _; echo"#;

/// The shell line Konsole runs for `manager` (fixed text, no user input).
pub fn script(manager: UpdateManager) -> String {
    let upgrade = match manager {
        UpdateManager::Paru => "paru -Syu",
        UpdateManager::Pacman => "sudo pacman -Syu",
        UpdateManager::Dnf => "sudo dnf upgrade --refresh",
    };
    format!("{upgrade}; {TAIL}")
}

pub fn konsole_argv(manager: UpdateManager) -> Vec<String> {
    [
        "konsole",
        "--separate",
        "-p",
        "tabtitle=System update",
        "-e",
        "bash",
        "-c",
    ]
    .into_iter()
    .map(String::from)
    .chain([script(manager)])
    .collect()
}

/// Whether `manager` belongs to this system's backend.
pub fn allowed(manager: UpdateManager, backend: UpdatesBackend) -> bool {
    matches!(
        (manager, backend),
        (
            UpdateManager::Paru | UpdateManager::Pacman,
            UpdatesBackend::Pacman
        ) | (UpdateManager::Dnf, UpdatesBackend::Dnf)
    )
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            let p = dir.join(program);
            Path::new(&p).is_file()
                && std::ffi::CString::new(p.as_os_str().as_encoded_bytes())
                    .is_ok_and(|c| unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0)
        })
    })
}

/// Opens the update window; `closed` runs (on a plain thread) once it is closed.
pub fn launch(
    manager: UpdateManager,
    backend: UpdatesBackend,
    closed: impl FnOnce() + Send + 'static,
) -> Result<(), IslandError> {
    if !allowed(manager, backend) {
        return Err(IslandError::invalid(
            "That package manager isn't used on this system",
        ));
    }
    if manager == UpdateManager::Paru && !on_path("paru") {
        return Err(IslandError::unavailable(
            "paru isn't installed. Use the repositories-only update instead.",
        ));
    }
    if !on_path("konsole") {
        return Err(IslandError::unavailable(
            "Konsole isn't installed, so the update can't be opened.",
        ));
    }
    if OPEN.swap(true, Ordering::AcqRel) {
        return Err(IslandError::invalid(
            "An update window is already open in Konsole",
        ));
    }
    let argv = konsole_argv(manager);
    let scoped = on_path("systemd-run");
    let mut command = if scoped {
        let mut c = Command::new("systemd-run");
        c.args(["--user", "--scope", "--collect", "--quiet", "--"]);
        c.args(&argv);
        c
    } else {
        let mut c = Command::new(&argv[0]);
        c.args(&argv[1..]);
        c
    };
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            OPEN.store(false, Ordering::Release);
            tracing::warn!(%error, "Update terminal did not start");
            return Err(IslandError::unavailable(
                "Couldn't open Konsole for the update",
            ));
        }
    };
    let spawned = std::thread::Builder::new()
        .name("system-update".into())
        .spawn(move || {
            if let Err(error) = child.wait() {
                tracing::debug!(%error, "Update terminal wait failed");
            }
            OPEN.store(false, Ordering::Release);
            closed();
        });
    if let Err(error) = spawned {
        // The window is open; only the re-check after it closes is lost.
        OPEN.store(false, Ordering::Release);
        tracing::warn!(%error, "Update terminal watcher did not start");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_manager_runs_one_fixed_upgrade_and_waits_before_closing() {
        assert!(script(UpdateManager::Paru).starts_with("paru -Syu; "));
        assert!(script(UpdateManager::Pacman).starts_with("sudo pacman -Syu; "));
        assert!(script(UpdateManager::Dnf).starts_with("sudo dnf upgrade --refresh; "));
        for m in [
            UpdateManager::Paru,
            UpdateManager::Pacman,
            UpdateManager::Dnf,
        ] {
            let argv = konsole_argv(m);
            assert_eq!(
                &argv[..7],
                [
                    "konsole",
                    "--separate",
                    "-p",
                    "tabtitle=System update",
                    "-e",
                    "bash",
                    "-c"
                ]
            );
            assert_eq!(argv.len(), 8, "one script argument, nothing appended");
            assert!(argv[7].contains("read -rsn1"), "the window waits for a key");
            assert!(
                !argv[7].contains("--noconfirm"),
                "the user confirms the transaction"
            );
        }
    }

    #[test]
    fn managers_are_tied_to_the_backend() {
        assert!(allowed(UpdateManager::Paru, UpdatesBackend::Pacman));
        assert!(allowed(UpdateManager::Pacman, UpdatesBackend::Pacman));
        assert!(!allowed(UpdateManager::Dnf, UpdatesBackend::Pacman));
        assert!(allowed(UpdateManager::Dnf, UpdatesBackend::Dnf));
        assert!(!allowed(UpdateManager::Pacman, UpdatesBackend::Dnf));
        for m in [
            UpdateManager::Paru,
            UpdateManager::Pacman,
            UpdateManager::Dnf,
        ] {
            assert!(!allowed(m, UpdatesBackend::Unsupported));
        }
    }
}
