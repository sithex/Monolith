//! On-demand desktop application enumeration and detached launching (Linux).
//!
//! Reuse the existing gtk::gio dependency, like notifications/effects.rs. GIO owns XDG
//! precedence, nested desktop-file IDs, TryExec and desktop-entry parsing. No hand-rolled
//! Exec parser, recursive filesystem scan, startup enumeration, or background poller.
//!
//! Launch through a transient user scope: ordinary DesktopAppInfo::launch detaches the
//! process but does not guarantee escape from the island's systemd cgroup. `gio launch`
//! still uses GLib's desktop-entry launcher, with the session environment inherited by
//! the scope. No shell and no unscoped fallback. Never use process::run_bounded here:
//! its process-group cleanup would also kill the newly launched application.

use crate::{contract::desktop_apps::DesktopApp, error::IslandError};

/// Match the layout core's current 64-byte embedded-ID limit, with stricter leading bytes.
/// This is a suffixless desktop-file ID, not a path or an option. Do not trim/normalise it.
pub fn valid_desktop_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with(['.', '-'])
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}

fn glyph(categories: &str) -> &'static str {
    let has = |category| categories.split(';').any(|value| value == category);
    if has("Audio") || has("Music") || has("Player") {
        "music"
    } else if has("TerminalEmulator") || has("Development") {
        "terminal"
    } else if has("Monitor") || has("System") {
        "monitor"
    } else if has("WebBrowser") || has("Network") || has("Chat") {
        "network"
    } else if has("Video") || has("AudioVideo") || has("Game") {
        "play"
    } else if has("Graphics") || has("Photography") {
        "camera"
    } else if has("Office") || has("TextEditor") {
        "note"
    } else if has("Settings") {
        "settings"
    } else {
        "grid"
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use gtk::gio::{self, prelude::*};
    use std::{
        ffi::OsString,
        os::unix::process::CommandExt,
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };

    const SYSTEMD_RUN: &str = "/usr/bin/systemd-run";
    const GIO: &str = "/usr/bin/gio";
    const LAUNCH_TIMEOUT: Duration = Duration::from_secs(10);

    /// `None` asks GLib to use XDG_CURRENT_DESKTOP (including multi-desktop sessions).
    /// Explicit desktop arguments make filtering fixtures independent of the user's session.
    fn visible(app: &gio::DesktopAppInfo, desktop: Option<&str>) -> bool {
        app.string("Type").as_deref() == Some("Application")
            && !app.is_hidden()
            && !app.is_nodisplay()
            && app.shows_in(desktop)
    }

    fn describe(id: &str, app: &gio::DesktopAppInfo, desktop: Option<&str>) -> Option<DesktopApp> {
        if !valid_desktop_id(id) || !visible(app, desktop) {
            return None;
        }
        let name = app.string("Name")?.to_string();
        if name.trim().is_empty() {
            return None;
        }
        Some(DesktopApp {
            id: id.to_owned(),
            name,
            glyph: glyph(app.categories().as_deref().unwrap_or_default()).to_owned(),
        })
    }

    pub(super) fn list() -> Vec<DesktopApp> {
        // GIO merges the XDG search path, including Hidden user overrides. Flatpak exports
        // must be in the session's XDG_DATA_DIRS; do not silently search outside that policy.
        let mut apps = gio::AppInfo::all()
            .into_iter()
            .filter_map(|app| app.downcast::<gio::DesktopAppInfo>().ok())
            .filter_map(|app| {
                let file_id = app.id()?;
                let id = file_id.strip_suffix(".desktop")?;
                describe(id, &app, None)
            })
            .collect::<Vec<_>>();
        apps.sort_by_cached_key(|app| (app.name.to_lowercase(), app.id.clone()));
        apps
    }

    fn resolve(id: &str) -> Result<PathBuf, IslandError> {
        if !valid_desktop_id(id) {
            return Err(IslandError::invalid("Invalid desktop application ID"));
        }
        // Resolve again on every launch: don't trust a stale picker result or UI-supplied path.
        let app = gio::DesktopAppInfo::new(&format!("{id}.desktop"))
            .ok_or_else(|| IslandError::unavailable("Desktop application is not installed"))?;
        if describe(id, &app, None).is_none() {
            return Err(IslandError::unavailable(
                "Desktop application is not visible",
            ));
        }
        let path = app
            .filename()
            .ok_or_else(|| IslandError::unavailable("Desktop application file is unavailable"))?;
        validate_path(&path)?;
        if !path.is_file() {
            return Err(IslandError::unavailable(
                "Desktop application file is unavailable",
            ));
        }
        Ok(path)
    }

    fn validate_path(path: &Path) -> Result<(), IslandError> {
        if !path.is_absolute() || path.extension().is_none_or(|ext| ext != "desktop") {
            return Err(IslandError::invalid(
                "Invalid installed desktop application file",
            ));
        }
        Ok(())
    }

    fn launch_args(path: &Path) -> Result<Vec<OsString>, IslandError> {
        validate_path(path)?;
        // Do not include the renderer ID in a unit name: systemd generates a unique one.
        // Disable systemd's own $ expansion too; argv boundaries alone don't disable it.
        let mut args: Vec<OsString> = [
            "--user",
            "--scope",
            "--collect",
            "--quiet",
            "--no-ask-password",
            "--expand-environment=no",
            "--",
            GIO,
            "launch",
        ]
        .map(OsString::from)
        .into();
        args.push(path.as_os_str().to_owned());
        Ok(args)
    }

    fn wait_launcher(mut child: Child) -> Result<(), IslandError> {
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(IslandError::unavailable(
                            "Could not launch desktop application",
                        ))
                    };
                }
                Ok(None) if started.elapsed() < LAUNCH_TIMEOUT => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                result => {
                    // Kill/reap only our launcher PID, never a process group or user scope.
                    // A timeout is ambiguous: the app might have started; never auto-retry it.
                    let _ = child.kill();
                    let _ = child.wait();
                    return if result.is_err() {
                        Err(IslandError::io(
                            "Cannot wait for desktop application launcher",
                        ))
                    } else {
                        Err(IslandError::timeout(
                            "Desktop application launcher timed out",
                        ))
                    };
                }
            }
        }
    }

    pub(super) fn launch(id: &str) -> Result<(), IslandError> {
        let path = resolve(id)?;
        let mut command = Command::new(SYSTEMD_RUN);
        command
            .args(launch_args(&path)?)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: setsid is async-signal-safe and touches no Rust shared state. Null stdio
        // prevents the launched app keeping IPC pipes/terminal descriptors open.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let child = command
            .spawn()
            .map_err(|_| IslandError::unavailable("Desktop application launcher unavailable"))?;
        // Wait for systemd-run + gio startup, NOT the GUI application's lifetime. Report
        // non-zero startup exits, unlike the fork's fire-and-forget reaper. No raw stderr IPC.
        wait_launcher(child)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn fixture(extra: &str) -> gio::DesktopAppInfo {
            let keyfile = glib::KeyFile::new();
            keyfile
                .load_from_data(
                    &format!(
                        "[Desktop Entry]\nType=Application\nName=Real application name\nExec=/usr/bin/true\n{extra}"
                    ),
                    glib::KeyFileFlags::NONE,
                )
                .unwrap();
            gio::DesktopAppInfo::from_keyfile(&keyfile).unwrap()
        }

        #[test]
        fn name_is_the_entry_name_not_generic_name_or_id() {
            let app = fixture("GenericName=Generic label\nName[fr]=Autre nom\nCategories=Audio;\n");
            let entry = describe("test", &app, Some("KDE")).unwrap();
            assert_eq!(entry.name, "Real application name");
            assert_eq!(entry.glyph, "music");
            assert_eq!(entry.id, "test");
        }

        #[test]
        fn hidden_and_nodisplay_are_filtered_for_listing_and_launch() {
            for extra in [
                "NoDisplay=true\n",
                "Hidden=true\n",
                "Hidden=true\nNoDisplay=true\n",
            ] {
                assert!(describe("test", &fixture(extra), Some("KDE")).is_none());
            }
            assert!(describe(
                "test",
                &fixture("NoDisplay=false\nHidden=false\n"),
                Some("KDE")
            )
            .is_some());
        }

        #[test]
        fn onlyshowin_and_notshowin_follow_the_selected_desktop() {
            let app = fixture("OnlyShowIn=KDE;LXQt;\n");
            assert!(describe("test", &app, Some("KDE")).is_some());
            assert!(describe("test", &app, Some("LXQt")).is_some());
            assert!(describe("test", &app, Some("GNOME")).is_none());
            let excluded = fixture("NotShowIn=KDE;\n");
            assert!(describe("test", &excluded, Some("KDE")).is_none());
            assert!(describe("test", &excluded, Some("GNOME")).is_some());
        }

        #[test]
        fn desktop_action_groups_do_not_override_the_main_name() {
            let app = fixture("[Desktop Action Other]\nName=Other action\nExec=/usr/bin/false\n");
            assert_eq!(
                describe("test", &app, Some("KDE")).unwrap().name,
                "Real application name"
            );
        }

        #[test]
        fn empty_name_and_unsafe_ids_are_not_listed() {
            assert!(describe("test", &fixture("Name=\n"), Some("KDE")).is_none());
            for id in ["-option", "nested/file", "../escape"] {
                assert!(describe(id, &fixture(""), Some("KDE")).is_none());
                assert!(resolve(id).is_err());
            }
        }

        #[test]
        fn scope_argv_keeps_the_desktop_file_as_one_literal_argument() {
            let path = Path::new("/opt/App files/$literal.desktop");
            let args = launch_args(path).unwrap();
            let expected: Vec<OsString> = [
                "--user",
                "--scope",
                "--collect",
                "--quiet",
                "--no-ask-password",
                "--expand-environment=no",
                "--",
                "/usr/bin/gio",
                "launch",
                "/opt/App files/$literal.desktop",
            ]
            .map(OsString::from)
            .into();
            assert_eq!(args, expected);
            assert!(launch_args(Path::new("relative.desktop")).is_err());
            assert!(launch_args(Path::new("/absolute/not-a-desktop.txt")).is_err());
        }
    }
}

/// No cache or service initialisation: call this only for an explicitly opened picker.
pub fn list() -> Vec<DesktopApp> {
    #[cfg(target_os = "linux")]
    {
        linux::list()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

pub fn launch(id: &str) -> Result<(), IslandError> {
    if !valid_desktop_id(id) {
        return Err(IslandError::invalid("Invalid desktop application ID"));
    }
    #[cfg(target_os = "linux")]
    {
        linux::launch(id)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(IslandError::unsupported(
            "Desktop application launching requires Linux",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_ids_are_bounded_suffixless_slugs_not_paths_or_options() {
        for id in [
            "spotifast",
            "io.missioncenter.MissionCenter",
            "kde-org.kde.kate",
            "app_2",
        ] {
            assert!(valid_desktop_id(id), "{id}");
        }
        assert!(valid_desktop_id(&"a".repeat(64)));
        for id in [
            "",
            "/absolute",
            "a/b",
            "../escape",
            ".hidden",
            "-option",
            "--help",
            "x y",
            "a;b",
            "a\0b",
            "é",
        ] {
            assert!(!valid_desktop_id(id), "{id:?}");
        }
        assert!(!valid_desktop_id(&"a".repeat(65)));
    }

    #[test]
    fn glyphs_are_a_fixed_island_allowlist() {
        for (categories, expected) in [
            ("Audio;Music;", "music"),
            ("Development;", "terminal"),
            ("System;Monitor;", "monitor"),
            ("Network;", "network"),
            ("Game;", "play"),
            ("Graphics;", "camera"),
            ("Office;", "note"),
            ("Settings;", "settings"),
            ("MadeUp;", "grid"),
        ] {
            assert_eq!(glyph(categories), expected);
        }
    }
}
