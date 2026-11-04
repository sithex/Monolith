//! Settings service (port of island-rust `Services::new` + `setSettings`).
//!
//! `settings.json` lives in the data dir, written atomically (temp + rename, 0600). A patch is
//! validated as a whole by `SettingsPatch::apply`; the autostart entry is changed first and
//! rolled back to its exact prior bytes if persisting fails.

use super::autostart::{atomic_write, Autostart};
use crate::{
    contract::settings::{Settings, SettingsPatch},
    error::IslandError,
};
use serde_json::{Map, Value};
use std::{fs, path::PathBuf, sync::Mutex};

pub struct SettingsService {
    file: PathBuf,
    autostart: Autostart,
    current: Mutex<StoredSettings>,
}

struct StoredSettings {
    settings: Settings,
    /// Disk-only fields, never exposed over IPC. Preserve future/rollback settings verbatim.
    unknown: Map<String, Value>,
}

impl SettingsService {
    /// Loads known values leniently; retains unknown keys privately for lossless saves.
    pub fn new(data_dir: PathBuf, autostart: Autostart) -> Self {
        let file = data_dir.join("settings.json");
        let raw = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .unwrap_or(Value::Null);
        let settings = Settings::from_file_value(&raw);
        let known = settings.to_file_map();
        let unknown = raw
            .as_object()
            .map(|map| {
                map.iter()
                    .filter(|(key, _)| !known.contains_key(key.as_str()))
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            file,
            autostart,
            current: Mutex::new(StoredSettings { settings, unknown }),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, StoredSettings>, IslandError> {
        self.current
            .lock()
            .map_err(|_| IslandError::internal("settings state is poisoned"))
    }

    pub fn get(&self) -> Result<Settings, IslandError> {
        Ok(self.lock()?.settings.clone())
    }

    pub fn patch(&self, patch: &SettingsPatch) -> Result<Settings, IslandError> {
        let mut current = self.lock()?;
        let next = patch.apply(&current.settings)?;
        if patch.is_empty() {
            return Ok(current.settings.clone());
        }
        let mut saved = current.unknown.clone();
        saved.extend(
            next.to_file_map()
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value)),
        );
        let bytes = serde_json::to_vec_pretty(&saved)
            .map_err(|_| IslandError::internal("Cannot encode settings"))?;
        let autostart = patch.autostart;
        let prior = if autostart.is_some() {
            self.autostart.snapshot()?
        } else {
            None
        };
        let result = autostart
            .map_or(Ok(()), |enabled| self.autostart.set(enabled))
            .and_then(|()| atomic_write(&self.file, &bytes, "Cannot atomically persist settings"));
        if let Err(error) = result {
            if autostart.is_some() {
                if let Err(rollback) = self.autostart.restore(prior.as_deref()) {
                    return Err(IslandError::new(
                        error.code,
                        format!(
                            "{}; autostart rollback failed: {}",
                            error.message, rollback.message
                        ),
                    ));
                }
            }
            return Err(error);
        }
        current.settings = next.clone();
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contract::settings::{CompanionCharacter, ObjectOnly, UiFont},
        services::scratch,
    };
    use serde_json::json;

    fn patch(value: Value) -> SettingsPatch {
        serde_json::from_value(value).unwrap()
    }

    fn service(dir: &std::path::Path, data: &str, refuse: bool) -> SettingsService {
        SettingsService::new(
            dir.join(data),
            Autostart::with_default_launcher(dir.to_path_buf(), dir.join("app"), refuse),
        )
    }

    #[test]
    fn terminal_command_validates_atomically_and_preserves_exact_text() {
        let dir = scratch("terminal-command-settings");
        let svc = service(&dir, "data", true);
        let command = "  hermes --TUI 'two words'  ";
        let next = svc
            .patch(&patch(json!({"terminalCommand": command})))
            .unwrap();
        assert_eq!(next.to_file_map()["terminalCommand"], json!(command));
        assert_eq!(service(&dir, "data", true).get().unwrap(), next);
        let bytes = fs::read(dir.join("data/settings.json")).unwrap();
        for bad in [
            "x".repeat(4097),
            "é".repeat(2049),
            "hermes 'open".into(),
            "hermes \"open".into(),
            "hermes\n".into(),
            "hermes\t".into(),
            "hermes\0".into(),
            "hermes\u{85}".into(),
        ] {
            let error = svc
                .patch(&patch(json!({"stealth": true, "terminalCommand": bad})))
                .unwrap_err();
            assert_eq!(error.code, crate::error::ErrorCode::Invalid);
            assert_eq!(svc.get().unwrap(), next);
            assert_eq!(fs::read(dir.join("data/settings.json")).unwrap(), bytes);
        }
        for valid in ["é".repeat(2048), "x".repeat(4096), "   ".into(), "".into()] {
            assert!(svc.patch(&patch(json!({"terminalCommand": valid}))).is_ok());
        }
        assert_eq!(
            Settings::from_file_value(&json!({})).to_file_map()["terminalCommand"],
            json!("")
        );
        assert_eq!(
            Settings::from_file_value(&json!({"terminalCommand":"bad '"})).to_file_map()
                ["terminalCommand"],
            json!("")
        );
        for bad in [json!(null), json!(1), json!([])] {
            assert!(
                serde_json::from_value::<SettingsPatch>(json!({"terminalCommand": bad})).is_err()
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn settings_failure_restores_prior_autostart_entry() {
        for (prior, enabled) in [
            (None, true),
            (Some(b"custom prior entry\n".as_slice()), true),
            (Some(b"custom prior entry\n".as_slice()), false),
        ] {
            let dir = scratch("settings-rollback");
            let svc = service(&dir, "data", false);
            let entry = dir.join(".config/autostart/hermes-island.desktop");
            if let Some(bytes) = prior {
                fs::create_dir_all(entry.parent().unwrap()).unwrap();
                fs::write(&entry, bytes).unwrap();
            }
            // A directory at the destination forces rename to fail, even as root.
            fs::create_dir_all(dir.join("data/settings.json")).unwrap();
            let before = svc.get().unwrap();
            assert!(svc.patch(&patch(json!({"autostart": enabled}))).is_err());
            assert_eq!(svc.get().unwrap(), before);
            assert_eq!(fs::read(&entry).ok().as_deref(), prior);
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn multi_monitor_option_is_opt_in_persisted_and_strict() {
        let dir = scratch("monitor-option");
        let svc = service(&dir, "", false);
        assert!(!svc.get().unwrap().show_notch_on_every_monitor);
        svc.patch(&patch(json!({"showNotchOnEveryMonitor":true})))
            .unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(saved["showNotchOnEveryMonitor"], true);
        assert!(Settings::from_file_value(&saved).show_notch_on_every_monitor);
        assert!(!Settings::from_file_value(&json!({})).show_notch_on_every_monitor);
        for bad in [json!(null), json!("true"), json!(1)] {
            assert!(serde_json::from_value::<SettingsPatch>(
                json!({"showNotchOnEveryMonitor":bad})
            )
            .is_err());
        }
        svc.patch(&patch(json!({"showNotchOnEveryMonitor":false})))
            .unwrap();
        assert!(!svc.get().unwrap().show_notch_on_every_monitor);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn notch_companion_is_on_by_default_persisted_and_strict() {
        let dir = scratch("companion-option");
        let svc = service(&dir, "", false);
        assert!(svc.get().unwrap().companion_enabled);
        // An older settings file without the key keeps the companion on.
        assert!(Settings::from_file_value(&json!({})).companion_enabled);
        svc.patch(&patch(json!({"companionEnabled":false})))
            .unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(saved["companionEnabled"], false);
        assert!(!Settings::from_file_value(&saved).companion_enabled);
        for bad in [json!(null), json!("false"), json!(0)] {
            assert!(
                serde_json::from_value::<SettingsPatch>(json!({"companionEnabled":bad})).is_err()
            );
        }
        // The character: blob by default, persisted, unknown names refused and, in an
        // older or hand-edited file, ignored rather than breaking the other settings.
        assert_eq!(
            svc.get().unwrap().companion_character,
            CompanionCharacter::Blob
        );
        svc.patch(&patch(json!({"companionCharacter":"isaac"})))
            .unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(saved["companionCharacter"], "isaac");
        assert_eq!(
            Settings::from_file_value(&saved).companion_character,
            CompanionCharacter::Isaac
        );
        for id in [
            "notchmimic",
            "obelisk",
            "companioncube",
            "nurgling",
            "hollowknight",
        ] {
            svc.patch(&patch(json!({"companionCharacter": id})))
                .unwrap();
            let saved: serde_json::Value =
                serde_json::from_slice(&fs::read(dir.join("settings.json")).unwrap()).unwrap();
            assert_eq!(saved["companionCharacter"], id);
            let restored = Settings::from_file_value(&saved);
            assert_eq!(
                serde_json::to_value(restored.companion_character).unwrap(),
                id
            );
        }
        for bad in [json!(null), json!("Isaac"), json!("../x"), json!(1)] {
            assert!(
                serde_json::from_value::<SettingsPatch>(json!({"companionCharacter":bad})).is_err()
            );
        }
        let odd =
            Settings::from_file_value(&json!({"companionCharacter":"ghost","notchWidth":200}));
        assert_eq!(odd.companion_character, CompanionCharacter::Blob);
        assert_eq!(odd.notch_width, 200.0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn settings_defaults_validation_atomic_persistence() {
        let dir = scratch("settings");
        let svc = service(&dir, "", false);
        let defaults = svc.get().unwrap();
        assert_eq!(defaults, Settings::default());
        assert_eq!(
            serde_json::to_value(&defaults)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            55 // Includes the opt-in multi-monitor notch setting and the notch companion.
        );
        assert_eq!(defaults.hover_delay, 180.0);
        assert_eq!(defaults.ui_font, UiFont::System);
        assert!(defaults.border_enabled);
        let next = svc
            .patch(&patch(
                json!({"accent":"#aBc123","offsetX":900,"terminalFontSize":3}),
            ))
            .unwrap();
        assert_eq!(next.offset_x, 400.0);
        assert_eq!(next.terminal_font_size, 8.0);
        assert_eq!(next.accent, "#aBc123");
        // The persisted file keeps the legacy shape: integers, no `.0`.
        let text = fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(text.contains("\"offsetX\": 400,"), "{text}");
        assert!(text.contains("\"hoverDelay\": 180,"), "{text}");
        // Atomicity: one invalid key rejects the whole patch; nothing changes on disk or in memory.
        let on_disk = fs::read(dir.join("settings.json")).unwrap();
        assert!(svc
            .patch(&patch(json!({"stealth": true, "borderColor": "red"})))
            .is_err());
        assert_eq!(svc.get().unwrap(), next);
        assert_eq!(fs::read(dir.join("settings.json")).unwrap(), on_disk);
        // Wrong types / unknown keys / non-objects never reach the service (serde rejects them).
        assert!(serde_json::from_value::<SettingsPatch>(
            json!({"accent":"#ffffff","stealth":"yes"})
        )
        .is_err());
        assert!(
            serde_json::from_value::<ObjectOnly<SettingsPatch>>(json!({"anything":true})).is_err()
        );
        assert!(serde_json::from_value::<ObjectOnly<SettingsPatch>>(json!([])).is_err());
        // No temp files left behind.
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".new"))
            .collect();
        assert!(leftovers.is_empty());
        let reload = service(&dir, "", false);
        assert_eq!(reload.get().unwrap(), next);
        assert_eq!(
            reload.patch(&patch(json!({"accent":""}))).unwrap().accent,
            ""
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn future_settings_persist_and_nullable_clear_without_autostart_side_effects() {
        let dir = scratch("future-settings");
        let svc = service(&dir, "data", true);
        let next = svc
            .patch(&patch(
                json!({"notificationsEnabled":false,"systemNotificationMirror":true,
            "toastTtlMs":7000,"redactBodies":true,"dnd":true,"clipboardHistory":true,
            "clipboardHistorySize":222,"clipboardPersist":true,"batteryAlerts":false,
            "batteryLowPercent":25,"batteryCriticalPercent":9,"weatherEnabled":true,
            "weatherLocation":{"name":"Fixture","latitude":0,"longitude":0,"country":""},
            "weatherUnit":"celsius","hourFormat":"h24","displayId":"fixture"}),
            ))
            .unwrap();
        assert_eq!(service(&dir, "data", true).get().unwrap(), next);
        let cleared = svc
            .patch(&patch(json!({"weatherLocation":null,"displayId":null})))
            .unwrap();
        assert!(cleared.weather_location.is_none() && cleared.display_id.is_none());
        assert_eq!(service(&dir, "data", true).get().unwrap(), cleared);
        let bytes = fs::read(dir.join("data/settings.json")).unwrap();
        assert!(svc
            .patch(&patch(json!({"stealth":true,"displayId":""})))
            .is_err());
        assert_eq!(fs::read(dir.join("data/settings.json")).unwrap(), bytes);
        assert_eq!(svc.get().unwrap(), cleared);
        assert!(!dir.join(".config").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn empty_patch_is_a_no_op_and_refused_autostart_changes_nothing() {
        let dir = scratch("settings-refuse");
        let svc = service(&dir, "data", true);
        assert_eq!(svc.patch(&patch(json!({}))).unwrap(), Settings::default());
        assert!(!dir.join("data/settings.json").exists());
        let err = svc
            .patch(&patch(json!({"autostart": true, "stealth": true})))
            .unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::Unsupported);
        assert_eq!(svc.get().unwrap(), Settings::default());
        assert!(!dir.join(".config").exists());
        assert!(!dir.join("data/settings.json").exists());
        // Non-autostart keys still persist in a test instance.
        assert!(svc.patch(&patch(json!({"stealth": true}))).unwrap().stealth);
        fs::remove_dir_all(dir).unwrap();
    }
}
