//! Bounded local assets and nonblocking, rate-limited notification audio. Never a shell.
use super::{dbus::DbusNotification, popups::app_rule};
use crate::{contract::settings::Settings, error::IslandError};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::{Duration, Instant},
};

pub fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn roots() -> Vec<PathBuf> {
    let mut roots = vec![];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share"));
    }
    roots.extend(
        std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".into())
            .split(':')
            .filter(|s| s.starts_with('/'))
            .map(PathBuf::from),
    );
    roots
}
fn png(path: &Path) -> Option<String> {
    if !path.is_absolute() || !path.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?.thumbnail(64, 64);
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(out.into_inner())
    ))
}
pub fn icon(hint: &str) -> Option<String> {
    if hint.starts_with('/') {
        return png(Path::new(hint));
    }
    if let Some(path) = hint.strip_prefix("file://") {
        return png(Path::new(path));
    }
    if !identifier(hint) {
        return None;
    }
    for root in roots() {
        for theme in ["hicolor", "breeze", "breeze-dark", "Adwaita"] {
            for size in ["64x64", "48x48", "32x32", "128x128", "256x256"] {
                for context in ["apps", "status", "devices"] {
                    if let Some(icon) =
                        png(&root.join(format!("icons/{theme}/{size}/{context}/{hint}.png")))
                    {
                        return Some(icon);
                    }
                }
            }
        }
        if let Some(icon) = png(&root.join(format!("pixmaps/{hint}.png"))) {
            return Some(icon);
        }
    }
    None
}
#[derive(Default)]
pub struct Sound {
    last: Mutex<Option<Instant>>,
}
/// Plasma plays hint sounds when it owns popups, so we only play while our inhibitor is held
/// (no double sound). Plasma mutes the `event`-role stream while inhibited, so ours uses the
/// `notification` role; these checks are what honour the user's DND/game-mode/app-rule policy.
pub fn audible(app: &str, suppress: bool, settings: &Settings, takeover_held: bool) -> bool {
    takeover_held
        && settings.notification_sound
        && settings.notifications_enabled
        && !settings.dnd
        && !settings.game_mode
        && !suppress
        && !settings.notification_denied_apps.iter().any(|a| a == app)
        && app_rule(settings, app).is_none_or(|r| r.sound)
}
impl Sound {
    pub fn play(&self, n: &DbusNotification, settings: &Settings, takeover_held: bool) {
        if !audible(&n.input.app_name, n.suppress_sound, settings, takeover_held) {
            return;
        }
        let Ok(mut last) = self.last.lock() else {
            return;
        };
        if last.is_some_and(|t| t.elapsed() < Duration::from_millis(800)) {
            return;
        }
        *last = Some(Instant::now());
        let (file, name) = choose_sound(n, settings);
        play_detached(file, name);
    }
}
pub const DEFAULT_SOUND: &str = "message-new-instant";
/// Sound-theme ids installed on this system (`<data dir>/sounds/<theme>/stereo/<id>.<ext>`),
/// sorted and de-duplicated across themes. Only valid ids are returned.
pub fn installed_sounds() -> Vec<String> {
    let mut dirs: Vec<PathBuf> = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into())
        .split(':')
        .map(PathBuf::from)
        .collect();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.insert(0, PathBuf::from(home).join(".local/share"));
    }
    let mut ids = std::collections::BTreeSet::new();
    for root in dirs {
        let Ok(themes) = std::fs::read_dir(root.join("sounds")) else {
            continue;
        };
        for theme in themes.flatten() {
            let Ok(files) = std::fs::read_dir(theme.path().join("stereo")) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !matches!(ext, "oga" | "ogg" | "wav") {
                    continue;
                }
                if let Some(id) = path.file_stem().and_then(|s| s.to_str()) {
                    if crate::contract::settings::sound_id_ok(id) {
                        ids.insert(id.to_owned());
                    }
                }
            }
        }
    }
    ids.into_iter().take(400).collect()
}
/// Which sound to play: the app's rule, then the global choice; when both are "" the app's
/// own hint (file, then theme name) and finally the default.
pub fn choose_sound(n: &DbusNotification, settings: &Settings) -> (Option<String>, String) {
    let chosen = app_rule(settings, &n.input.app_name)
        .map(|r| r.sound_id.as_str())
        .filter(|id| !id.is_empty())
        .or(Some(settings.notification_sound_id.as_str()).filter(|id| !id.is_empty()))
        .filter(|id| crate::contract::settings::sound_id_ok(id))
        .map(str::to_owned);
    if let Some(id) = chosen {
        return (None, id);
    }
    let file = n
        .sound_file
        .as_deref()
        .filter(|s| Path::new(s).is_absolute())
        .filter(|s| std::fs::metadata(s).is_ok_and(|m| m.is_file() && m.len() <= 8 * 1024 * 1024))
        .map(str::to_owned);
    let name = n
        .sound_name
        .as_deref()
        .filter(|s| identifier(s))
        .unwrap_or(DEFAULT_SOUND)
        .to_owned();
    (file, name)
}
/// Settings → Notifications preview: plays a theme sound now (an explicit user action, so it
/// ignores DnD/game mode). "" previews the default.
pub fn preview(id: &str) -> Result<(), IslandError> {
    let id = if id.is_empty() { DEFAULT_SOUND } else { id };
    if !crate::contract::settings::sound_id_ok(id) {
        return Err(IslandError::invalid("Invalid sound"));
    }
    play_detached(None, id.to_owned());
    Ok(())
}
fn play_detached(file: Option<String>, name: String) {
    let _ = std::thread::Builder::new()
        .name("notification-sound".into())
        .spawn(move || {
            let mut player = Command::new("canberra-gtk-play");
            player.args([
                "--property=media.role=notification",
                "--cache-control=volatile",
            ]);
            if let Some(path) = file {
                player.arg("--file").arg(path);
            } else {
                player.arg("--id").arg(name);
            }
            let _ = crate::services::process::run_bounded(
                player,
                Duration::from_secs(5),
                4096,
                false,
            );
        });
}
pub fn launch(desktop: &str) -> Result<(), IslandError> {
    if !identifier(desktop) {
        return Err(IslandError::invalid("Invalid application desktop entry"));
    }
    // Resolve an installed desktop entry; never accept a path, command, or URI from a hint.
    use gtk::gio::prelude::AppInfoExt;
    let id = if desktop.ends_with(".desktop") {
        desktop.to_owned()
    } else {
        format!("{desktop}.desktop")
    };
    let app = gtk::gio::DesktopAppInfo::new(&id)
        .ok_or_else(|| IslandError::unavailable("Sending application is not installed"))?;
    app.launch(&[], None::<&gtk::gio::AppLaunchContext>)
        .map_err(|_| IslandError::unavailable("Could not open sending application"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sound_policy_respects_inhibitor_dnd_game_mode_hints_and_app_rules() {
        use crate::contract::settings::{NotificationAppRule, RuleUrgency};
        let mut s = Settings::default();
        assert!(audible("Mail", false, &s, true));
        assert!(!audible("Mail", false, &s, false), "KDE plays it itself");
        assert!(!audible("Mail", true, &s, true), "suppress-sound hint");
        s.notification_app_rules.push(NotificationAppRule {
            app: "Mail".into(),
            popup: true,
            sound: false,
            urgency: RuleUrgency::App,
            sound_id: String::new(),
        });
        assert!(!audible("Mail", false, &s, true));
        assert!(audible("Chat", false, &s, true));
        s.notification_app_rules.clear();
        s.dnd = true;
        assert!(!audible("Mail", false, &s, true));
        s.dnd = false;
        s.game_mode = true;
        assert!(!audible("Mail", false, &s, true));
        s.game_mode = false;
        s.notification_sound = false;
        assert!(!audible("Mail", false, &s, true));
    }
    #[test]
    fn sound_choice_prefers_app_rule_then_global_then_hint() {
        use crate::contract::settings::{NotificationAppRule, RuleUrgency};
        let mut n = super::super::dbus::NotifyArgs {
            app_name: "Mail".into(),
            replaces_id: 0,
            app_icon: String::new(),
            summary: "Hi".into(),
            body: String::new(),
            actions: Vec::new(),
            hints: Default::default(),
            expire_timeout: -1,
        }
        .parse(1)
        .unwrap();
        n.sound_name = Some("bell".into());
        let mut s = Settings::default();
        assert_eq!(choose_sound(&n, &s), (None, "bell".into()), "app hint when nothing chosen");
        n.sound_name = None;
        assert_eq!(choose_sound(&n, &s).1, DEFAULT_SOUND);
        s.notification_sound_id = "complete".into();
        assert_eq!(choose_sound(&n, &s), (None, "complete".into()));
        s.notification_app_rules.push(NotificationAppRule {
            app: "Mail".into(),
            popup: true,
            sound: true,
            urgency: RuleUrgency::App,
            sound_id: "message-new-email".into(),
        });
        assert_eq!(choose_sound(&n, &s).1, "message-new-email");
        n.input.app_name = "Chat".into();
        assert_eq!(choose_sound(&n, &s).1, "complete");
        assert!(preview("../../etc/passwd").is_err() && preview("--file=/x").is_err());
    }
    #[test]
    fn asset_identifiers_are_not_paths_or_options() {
        for bad in [
            "",
            ".",
            "..",
            "../../file",
            "/file",
            "file:///x",
            "--id=x",
            "app;whoami",
            "app\n",
        ] {
            assert!(!identifier(bad), "{bad}");
        }
        assert!(identifier("org.kde.konsole"));
        assert!(icon("https://invalid/icon.png").is_none());
    }
}
