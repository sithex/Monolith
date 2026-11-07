//! Full SPEC §5.2 settings contract. The original 16 keys retain their legacy semantics;
//! future-feature keys are validated and persisted but do not start those services.
//!
//! * Numbers are `f64`, clamped into range, and written to `settings.json` as integers when
//!   whole so the file stays identical to the legacy writer's JSON.
//! * Colors: `""` (follow the Hermes skin) or `#rrggbb`, anything else is `Invalid`.
//! * `SettingsPatch` is atomic: `apply` validates every present key before returning a value;
//!   the caller persists/publishes only the returned whole `Settings`.

use super::system::GeoPlace;
use crate::error::IslandError;
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum TempUnit {
    Fahrenheit,
    Celsius,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum HourFormat {
    H12,
    H24,
}

/// Linux display server the island window uses (read by `run.sh` at launch; a change applies
/// after the island restarts). `Wayland` = native GDK Wayland + wlr-layer-shell overlay,
/// `X11` = GDK on XWayland (the original runtime).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum DisplayBackend {
    Wayland,
    X11,
}

/// Who lives in the notch. New characters are new variants here, a file in the notch
/// widget's `characters/` folder and an entry in the UI's character list.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Default, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CompanionCharacter {
    /// The round face.
    #[default]
    Blob,
    /// A crying boy in a basement, after The Binding of Isaac.
    Isaac,
    Notchmimic,
    Companioncube,
    Nurgling,
    Hollowknight,
    /// A little frog that snaps flies out of the air.
    Frog,
    Necronwarrior,
    Necronoverlord,
    Canoptekscarab,
    Eyeofsauron,
    Salamander,
}

// Deserialize the removed ID at the boundary only. A serde alias on Blob makes
// specta include "obelisk" in the public TS union, incorrectly advertising it as
// a supported selectable value. Serialization and generated types stay canonical.
impl<'de> Deserialize<'de> for CompanionCharacter {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        match id.as_str() {
            "blob" | "obelisk" => Ok(Self::Blob),
            "isaac" => Ok(Self::Isaac),
            "notchmimic" => Ok(Self::Notchmimic),
            "companioncube" => Ok(Self::Companioncube),
            "nurgling" => Ok(Self::Nurgling),
            "hollowknight" => Ok(Self::Hollowknight),
            "frog" => Ok(Self::Frog),
            "necronwarrior" => Ok(Self::Necronwarrior),
            "necronoverlord" => Ok(Self::Necronoverlord),
            "canoptekscarab" => Ok(Self::Canoptekscarab),
            "eyeofsauron" => Ok(Self::Eyeofsauron),
            "salamander" => Ok(Self::Salamander),
            _ => Err(serde::de::Error::unknown_variant(
                &id,
                &[
                    "blob",
                    "isaac",
                    "notchmimic",
                    "companioncube",
                    "nurgling",
                    "hollowknight",
                    "frog",
                    "necronwarrior",
                    "necronoverlord",
                    "canoptekscarab",
                    "eyeofsauron",
                    "salamander",
                ],
            )),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum UiFont {
    System,
    Openrunde,
}

impl UiFont {
    pub fn as_str(self) -> &'static str {
        match self {
            UiFont::System => "system",
            UiFont::Openrunde => "openrunde",
        }
    }
}

/// `(min, max)` of each numeric setting.
pub const HOVER_DELAY_RANGE: (f64, f64) = (0.0, 1200.0);
pub const OFFSET_X_RANGE: (f64, f64) = (-400.0, 400.0);
pub const OFFSET_Y_RANGE: (f64, f64) = (-200.0, 200.0);
pub const TERMINAL_FONT_SIZE_RANGE: (f64, f64) = (8.0, 24.0);
pub const NOTCH_WIDTH_RANGE: (f64, f64) = (110.0, 320.0);
pub const ANIMATION_SPEED_RANGE: (f64, f64) = (50.0, 200.0);
pub const TERMINAL_SCROLLBACK_RANGE: (f64, f64) = (500.0, 100_000.0);
/// Exact, case-sensitive app names, matching the notification store's policy.
pub const MAX_NOTIFICATION_DENIED_APPS: usize = 128;
pub const MAX_NOTIFICATION_APP_BYTES: usize = 256;
pub const MAX_NOTCH_MONITORS: usize = 16;

/// Embedded terminal cursor shape (xterm.js `cursorStyle`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum TerminalCursorStyle {
    #[default]
    Block,
    Bar,
    Underline,
}

/// Per-app override of a notification's urgency (`App` = keep what the app sent).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum RuleUrgency {
    #[default]
    App,
    Low,
    Normal,
    Critical,
}

/// One app's notification rule, matched on the exact app name.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationAppRule {
    pub app: String,
    /// Show a popup card (false = history only).
    pub popup: bool,
    /// Play the notification sound.
    pub sound: bool,
    pub urgency: RuleUrgency,
    /// Sound-theme id for this app ("" = the global choice). Rules saved before this field
    /// existed load as "".
    #[serde(default)]
    pub sound_id: String,
}
pub const MAX_NOTIFICATION_APP_RULES: usize = 128;

/// Generates, from ONE table, everything a setting needs: the `Settings` field and its
/// default, the `settings.json` encoding, the `SettingsPatch` field (with its TS type) and the
/// validation in [`SettingsPatch::apply`]. Adding a setting = adding one row below.
///
/// Row: `field: Type = default, "fileKey", rule;` where rule is
/// * `plain` — any value of the type is valid;
/// * `num(RANGE)` — finite, clamped into RANGE; `round(RANGE)` also rounds to a whole number;
/// * `color` — `""` (follow the skin) or `#rrggbb`;
/// * `with(fn)` — `fn(&T) -> Result<T, IslandError>` validates/normalizes the value.
///
/// Keys missing from an older `settings.json` keep their default (container `serde(default)`);
/// `Option<T>` settings accept `null` in a patch to clear them.
macro_rules! settings_schema {
    ($( $(#[$meta:meta])* $field:ident : $ty:ty = $default:expr, $key:literal, $rule:ident $(($arg:expr))? ;)*) => {
        /// Every field is always present on the wire (no `?` in TypeScript); deserializing
        /// starts from the defaults (see the `Deserialize` impl below).
        #[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
        #[serde(rename_all = "camelCase")]
        pub struct Settings {
            $( $(#[$meta])* pub $field: $ty, )*
        }

        impl Default for Settings {
            fn default() -> Self {
                Self { $( $field: $default, )* }
            }
        }

        /// Every `settings.json` key, in table order.
        pub const SETTINGS_KEYS: &[&str] = &[$($key),*];

        impl Settings {
            /// The JSON written to `settings.json`: sorted keys, whole numbers as integers.
            pub fn to_file_map(&self) -> BTreeMap<&'static str, Value> {
                BTreeMap::from([$( ($key, FileValue::file_value(&self.$field)), )*])
            }
        }

        /// Partial update. Unknown keys and wrong types are rejected by serde; range/pattern
        /// checks and clamping happen in [`SettingsPatch::apply`].
        /// Deserialize it through [`ObjectOnly`] at the IPC boundary (arrays are otherwise
        /// accepted by serde's derived struct impl).
        #[derive(Deserialize, Default, Clone, Debug, PartialEq, specta::Type)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct SettingsPatch {
            $(
                #[serde(default, deserialize_with = "present")]
                #[specta(optional, type = $ty)]
                pub $field: Option<$ty>,
            )*
        }

        impl SettingsPatch {
            pub fn is_empty(&self) -> bool {
                *self == SettingsPatch::default()
            }

            /// Validate the whole patch against `base`; any invalid key rejects everything.
            pub fn apply(&self, base: &Settings) -> Result<Settings, IslandError> {
                let mut next = base.clone();
                $(
                    if let Some(v) = &self.$field {
                        next.$field = settings_rule!($rule $(($arg))?, $key, v);
                    }
                )*
                Ok(next)
            }
        }
    };
}

macro_rules! settings_rule {
    (plain, $key:literal, $v:ident) => {
        $v.clone()
    };
    (num($range:expr), $key:literal, $v:ident) => {
        number($key, *$v, $range)?
    };
    (round($range:expr), $key:literal, $v:ident) => {
        number($key, *$v, $range)?.round()
    };
    (color, $key:literal, $v:ident) => {
        color($key, $v)?
    };
    (with($check:expr), $key:literal, $v:ident) => {
        ($check)($v)?
    };
}

settings_schema! {
    /// Phase 6b1: additive module settings. Secret subscription URLs are never config.
    module_registry: super::modules::Registry = super::modules::Registry::defaults(), "moduleRegistry", with(check_registry);
    autostart: bool = false, "autostart", plain;
    show_in_taskbar: bool = false, "showInTaskbar", plain;
    stealth: bool = false, "stealth", plain;
    click_sounds: bool = false, "clickSounds", plain;
    /// 0..=1200 ms; 0 disables hover preview.
    hover_delay: f64 = 180.0, "hoverDelay", num(HOVER_DELAY_RANGE);
    offset_x: f64 = 0.0, "offsetX", num(OFFSET_X_RANGE);
    offset_y: f64 = 0.0, "offsetY", num(OFFSET_Y_RANGE);
    accent: String = String::new(), "accent", color;
    background: String = String::new(), "background", color;
    border_enabled: bool = true, "borderEnabled", plain;
    border_color: String = String::new(), "borderColor", color;
    /// Pill BACKGROUND, not digits.
    panel_clock_color: String = String::new(), "panelClockColor", color;
    notch_border_enabled: bool = true, "notchBorderEnabled", plain;
    notch_border_color: String = String::new(), "notchBorderColor", color;
    notch_border_opacity: f64 = 0.1, "notchBorderOpacity", num((0.0, 1.0));
    notch_border_width: f64 = 0.8, "notchBorderWidth", num((0.0, 4.0));
    ui_font: UiFont = UiFont::System, "uiFont", plain;
    show_date: bool = true, "showDate", plain;
    hardware_monitor: bool = true, "hardwareMonitor", plain;
    terminal_font_size: f64 = 13.0, "terminalFontSize", num(TERMINAL_FONT_SIZE_RANGE);
    /// Direct executable and quoted arguments; empty retains the original Hermes launch.
    terminal_command: String = String::new(), "terminalCommand", with(check_terminal_command);
    notifications_enabled: bool = true, "notificationsEnabled", plain;
    system_notification_mirror: bool = false, "systemNotificationMirror", plain;
    /// Exact app names that never notify (file order kept, never normalized).
    notification_denied_apps: Vec<String> = Vec::new(), "notificationDeniedApps", with(check_denied_apps);
    toast_ttl_ms: f64 = 6000.0, "toastTtlMs", num((1500.0, 20000.0));
    redact_bodies: bool = false, "redactBodies", plain;
    dnd: bool = false, "dnd", plain;
    clipboard_history: bool = false, "clipboardHistory", plain;
    clipboard_history_size: f64 = 100.0, "clipboardHistorySize", num((10.0, 500.0));
    clipboard_persist: bool = false, "clipboardPersist", plain;
    clipboard_deny_patterns: Vec<String> = Vec::new(), "clipboardDenyPatterns", with(check_clipboard_patterns);
    battery_alerts: bool = true, "batteryAlerts", plain;
    battery_low_percent: f64 = 20.0, "batteryLowPercent", num((5.0, 50.0));
    battery_critical_percent: f64 = 10.0, "batteryCriticalPercent", num((1.0, 30.0));
    weather_enabled: bool = false, "weatherEnabled", plain;
    weather_location: Option<GeoPlace> = None, "weatherLocation", with(check_place);
    weather_unit: TempUnit = TempUnit::Fahrenheit, "weatherUnit", plain;
    hour_format: HourFormat = HourFormat::H12, "hourFormat", plain;
    display_id: Option<String> = None, "displayId", with(check_display_id);
    /// Fetch synced lyrics from LRCLIB for the playing track. A network call that sends
    /// title/artist/album/duration, so it is opt-in (default off).
    lyrics_enabled: bool = false, "lyricsEnabled", plain;
    /// Live equalizer from a monitor capture of the playing stream (privacy: opt-in).
    equalizer_enabled: bool = false, "equalizerEnabled", plain;
    /// Whole bands only: a fractional count would be silently truncated downstream.
    equalizer_bands: f64 = 16.0, "equalizerBands", round((8.0, 32.0));
    /// Horizontal swipe / trackpad gesture switches between views.
    swipe_navigation: bool = true, "swipeNavigation", plain;
    /// Translucent island body over KWin blur-behind.
    translucent: bool = true, "translucent", plain;
    /// How see-through the translucent body is, 0 (opaque) ..= 100.
    glass_strength: f64 = 55.0, "glassStrength", round((0.0, 100.0));
    /// Mirror the island colour scheme into the Hermes skin; off restores the previous skin.
    hermes_theme_sync: bool = false, "hermesThemeSync", plain;
    /// The Custom colour scheme's seed colours (empty = scheme default).
    custom_accent: String = String::new(), "customAccent", color;
    custom_background: String = String::new(), "customBackground", color;
    custom_text: String = String::new(), "customText", color;
    /// Linux display server, applied at the next launch by `run.sh`.
    display_backend: DisplayBackend = DisplayBackend::Wayland, "displayBackend", plain;
    /// Native notch width in logical px (144 = the original slot).
    notch_width: f64 = 144.0, "notchWidth", round(NOTCH_WIDTH_RANGE);
    show_notch_on_every_monitor: bool = false, "showNotchOnEveryMonitor", plain;
    /// The small companion face in the notch whose eyes follow the pointer.
    companion_enabled: bool = true, "companionEnabled", plain;
    companion_character: CompanionCharacter = CompanionCharacter::Blob, "companionCharacter", plain;
    // ---- Nov 5 additions ----
    /// Game mode switches on by itself while the active window is fullscreen.
    game_mode_auto: bool = true, "gameModeAuto", plain;
    /// Manual game mode (side button / tray): no previews, toasts or pop-ups over games.
    game_mode: bool = false, "gameMode", plain;
    /// KDE global shortcut that toggles the island (KGlobalAccel syntax, `""` = none).
    open_shortcut: String = String::new(), "openShortcut", with(check_shortcut);
    /// Amber pulse on the closed notch while Hermes waits for you (approval/question).
    notch_waiting_pulse: bool = true, "notchWaitingPulse", plain;
    /// The companion looks busy while Hermes works, alert while it waits, dozes when idle.
    companion_reactive: bool = true, "companionReactive", plain;
    /// Hovering the closed notch peeks the playing track / running timer.
    notch_hover_peek: bool = true, "notchHoverPeek", plain;
    /// Output names that get a notch with `showNotchOnEveryMonitor` (empty = every monitor).
    notch_monitors: Vec<String> = Vec::new(), "notchMonitors", with(check_output_list);
    /// Output that shows the companion (`""` = every notch).
    companion_monitor: String = String::new(), "companionMonitor", with(check_output_or_empty);
    /// Replace springs/morphs with short fades.
    reduce_motion: bool = false, "reduceMotion", plain;
    /// Animation speed in percent (100 = designed timing, 200 = twice as fast).
    animation_speed: f64 = 100.0, "animationSpeed", round(ANIMATION_SPEED_RANGE);
    /// Embedded terminal font family (`""` = DejaVu Sans Mono).
    terminal_font_family: String = String::new(), "terminalFontFamily", with(check_font_family);
    terminal_scrollback: f64 = 5000.0, "terminalScrollback", round(TERMINAL_SCROLLBACK_RANGE);
    terminal_cursor_style: TerminalCursorStyle = TerminalCursorStyle::Block, "terminalCursorStyle", plain;
    terminal_cursor_blink: bool = true, "terminalCursorBlink", plain;
    // ---- Nov 6 additions ----
    /// Silence KDE's own notification popups (KDE keeps history and the bell); the island
    /// shows the popups instead. Off restores KDE's popups.
    silence_kde_popups: bool = false, "silenceKdePopups", plain;
    /// The island plays a sound for new notifications (per-app rules can mute).
    notification_sound: bool = true, "notificationSound", plain;
    /// Sound-theme id played for every notification ("" = the app's own sound, falling back
    /// to message-new-instant). Per-app rules can pick their own.
    notification_sound_id: String = String::new(), "notificationSoundId", with(check_sound_id);
    /// Per-app popup/sound/urgency overrides (exact app names, file order kept).
    notification_app_rules: Vec<NotificationAppRule> = Vec::new(), "notificationAppRules", with(check_app_rules);
    /// Hermes approval requests show Approve/Deny in the notch hover notice.
    approvals_in_notch: bool = true, "approvalsInNotch", plain;
    /// While game mode is on: island DND, frozen notch companion, paused background pollers.
    game_mode_dnd: bool = true, "gameModeDnd", plain;
    game_mode_freeze_companion: bool = true, "gameModeFreezeCompanion", plain;
    game_mode_pause_pollers: bool = true, "gameModePausePollers", plain;
    /// KDE global shortcut that opens the command palette (`""` = none).
    palette_shortcut: String = String::new(), "paletteShortcut", with(check_shortcut);
}

/// Strict per present key (wrong type = error, values validated/clamped like a patch); keys
/// missing from an older file keep their default and unknown keys are ignored.
impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut map = serde_json::Map::<String, Value>::deserialize(deserializer)?;
        map.retain(|key, _| SETTINGS_KEYS.contains(&key.as_str()));
        let patch = serde_json::from_value::<SettingsPatch>(Value::Object(map))
            .map_err(D::Error::custom)?;
        patch
            .apply(&Settings::default())
            .map_err(|e| D::Error::custom(format!("{e:?}")))
    }
}

/// How a setting is written to `settings.json`.
trait FileValue {
    fn file_value(&self) -> Value;
}
impl FileValue for f64 {
    fn file_value(&self) -> Value {
        json_number(*self)
    }
}
macro_rules! file_value_as_json {
    ($($t:ty),*) => { $(impl FileValue for $t { fn file_value(&self) -> Value { json!(self) } })* };
}
file_value_as_json!(
    bool,
    String,
    Vec<String>,
    Option<String>,
    Option<GeoPlace>,
    UiFont,
    TempUnit,
    HourFormat,
    DisplayBackend,
    CompanionCharacter,
    TerminalCursorStyle,
    Vec<NotificationAppRule>,
    super::modules::Registry
);

#[allow(clippy::ptr_arg)] // called with the field type by `settings_rule!`
fn check_app_rules(
    rules: &Vec<NotificationAppRule>,
) -> Result<Vec<NotificationAppRule>, IslandError> {
    let mut seen = BTreeSet::new();
    if rules.len() > MAX_NOTIFICATION_APP_RULES
        || rules.iter().any(|rule| {
            let app = &rule.app;
            app.is_empty()
                || app.len() > MAX_NOTIFICATION_APP_BYTES
                || app.trim() != app
                || app.chars().any(char::is_control)
                || !(rule.sound_id.is_empty() || sound_id_ok(&rule.sound_id))
                || !seen.insert(app)
        })
    {
        return Err(IslandError::invalid("Invalid notificationAppRules"));
    }
    Ok(rules.clone())
}

/// A freedesktop sound-theme id: never a path, URI or option (canberra `--id`).
pub fn sound_id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with(['.', '-'])
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}

#[allow(clippy::ptr_arg)] // called with the field type by `settings_rule!`
fn check_sound_id(id: &String) -> Result<String, IslandError> {
    if id.is_empty() || sound_id_ok(id) {
        Ok(id.clone())
    } else {
        Err(IslandError::invalid("Invalid notificationSoundId"))
    }
}

fn check_registry(
    registry: &super::modules::Registry,
) -> Result<super::modules::Registry, IslandError> {
    registry
        .export_json()
        .map_err(|_| IslandError::invalid("Invalid module registry"))?;
    Ok(registry.clone())
}

#[allow(clippy::ptr_arg)] // called with the field type by `settings_rule!`
fn check_terminal_command(command: &String) -> Result<String, IslandError> {
    parse_terminal_command(command)?;
    Ok(command.clone())
}

#[allow(clippy::ptr_arg)] // called with the field type by `settings_rule!`
fn check_denied_apps(apps: &Vec<String>) -> Result<Vec<String>, IslandError> {
    let mut seen = BTreeSet::new();
    if apps.len() > MAX_NOTIFICATION_DENIED_APPS
        || apps.iter().any(|app| {
            app.is_empty()
                || app.len() > MAX_NOTIFICATION_APP_BYTES
                || app.trim() != app
                || app.chars().any(char::is_control)
                || !seen.insert(app)
        })
    {
        return Err(IslandError::invalid("Invalid notificationDeniedApps"));
    }
    // Do not trim, fold case or sort: matching is exact and file order is retained.
    Ok(apps.clone())
}

#[allow(clippy::ptr_arg)] // called with the field type by `settings_rule!`
fn check_clipboard_patterns(patterns: &Vec<String>) -> Result<Vec<String>, IslandError> {
    crate::integrations::clipboard::history::HistoryConfig {
        deny_patterns: patterns.clone(),
        ..Default::default()
    }
    .validated()
    .map_err(|_| IslandError::invalid("Invalid clipboardDenyPatterns"))?;
    Ok(patterns.clone())
}

fn check_place(place: &Option<GeoPlace>) -> Result<Option<GeoPlace>, IslandError> {
    if let Some(place) = place {
        place.validate()?;
    }
    Ok(place.clone())
}

fn check_display_id(id: &Option<String>) -> Result<Option<String>, IslandError> {
    if id
        .as_ref()
        .is_some_and(|id| id.is_empty() || id.len() > 256 || id.chars().any(char::is_control))
    {
        return Err(IslandError::invalid("Invalid displayId"));
    }
    Ok(id.clone())
}

/// KGlobalAccel key sequence such as `Meta+\``, `Ctrl+Alt+Space`; `""` disables it.
#[allow(clippy::ptr_arg)] // called with the field type by `settings_rule!`
fn check_shortcut(value: &String) -> Result<String, IslandError> {
    if value.len() > 64
        || value
            .chars()
            .any(|c| c.is_control() || c == ',' || c == '\t')
        || crate::keyseq::parse(value).is_err()
    {
        return Err(IslandError::invalid("Invalid shortcut"));
    }
    Ok(value.trim().to_owned())
}

/// A KScreen output name (`DP-1`, `HDMI-A-1`, `eDP-1`).
pub fn is_output_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

#[allow(clippy::ptr_arg)] // called with the field type by `settings_rule!`
fn check_output_list(names: &Vec<String>) -> Result<Vec<String>, IslandError> {
    let mut seen = BTreeSet::new();
    if names.len() > MAX_NOTCH_MONITORS
        || names
            .iter()
            .any(|name| !is_output_name(name) || !seen.insert(name))
    {
        return Err(IslandError::invalid("Invalid notchMonitors"));
    }
    Ok(names.clone())
}

#[allow(clippy::ptr_arg)] // called with the field type by `settings_rule!`
fn check_output_or_empty(name: &String) -> Result<String, IslandError> {
    if name.is_empty() || is_output_name(name) {
        Ok(name.clone())
    } else {
        Err(IslandError::invalid("Invalid companionMonitor"))
    }
}

/// A CSS/xterm font family list; quotes and CSS syntax characters are refused so the value
/// can never escape the `font-family` declaration.
#[allow(clippy::ptr_arg)] // called with the field type by `settings_rule!`
fn check_font_family(value: &String) -> Result<String, IslandError> {
    if value.len() > 128
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '\'' | ';' | '{' | '}' | '<' | '>' | '\\'))
    {
        return Err(IslandError::invalid("Invalid terminalFontFamily"));
    }
    Ok(value.trim().to_owned())
}

/// Parse argv only: no shell evaluation, variable expansion, or case normalization.
pub fn parse_terminal_command(value: &str) -> Result<Vec<String>, IslandError> {
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(IslandError::invalid(
            "Invalid terminalCommand: maximum 4096 bytes, no control characters",
        ));
    }
    shell_words::split(value)
        .map_err(|_| IslandError::invalid("Invalid terminalCommand: unclosed quotes"))
}

pub fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

fn color(key: &str, value: &str) -> Result<String, IslandError> {
    if value.is_empty() || is_hex_color(value) {
        Ok(value.to_owned())
    } else {
        Err(IslandError::invalid(format!("Invalid value for {key}")))
    }
}

fn number(key: &str, value: f64, (min, max): (f64, f64)) -> Result<f64, IslandError> {
    if value.is_finite() {
        Ok(value.clamp(min, max))
    } else {
        Err(IslandError::invalid(format!("Invalid number for {key}")))
    }
}

/// JSON number that is an integer when whole (the legacy writer's output).
fn json_number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9.0e15 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

impl Settings {
    /// Lenient loader (island-rust semantics): unknown keys are ignored and individually invalid
    /// values keep their default; a non-object file yields the defaults.
    pub fn from_file_value(raw: &Value) -> Settings {
        let mut settings = Settings {
            module_registry: super::modules::migrate_from_flat(raw).0,
            ..Settings::default()
        };
        if let Value::Object(map) = raw {
            for (key, value) in map {
                let one = Value::Object(serde_json::Map::from_iter([(key.clone(), value.clone())]));
                let Ok(patch) = serde_json::from_value::<SettingsPatch>(one) else {
                    continue;
                };
                if let Ok(next) = patch.apply(&settings) {
                    settings = next;
                }
            }
        }
        settings
    }
}

/// Accept a present value but reject JSON `null` (the legacy validator rejected it too).
/// For an `Option<T>` setting, `null` deserializes as `Some(None)` = explicitly clear it.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Wire wrapper that accepts ONLY a JSON object. A derived struct `Deserialize` would also
/// accept a JSON array (`[]`, or positional `[true, 1]`) as a patch; the legacy validator
/// rejected every non-object. Used as the `settings_patch` command parameter; the TypeScript
/// type is the inner type's.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectOnly<T>(pub T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for ObjectOnly<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = ObjectOnly<T>;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object")
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                T::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(ObjectOnly)
            }
        }
        deserializer.deserialize_map(ObjectVisitor(std::marker::PhantomData))
    }
}

impl<T: specta::Type> specta::Type for ObjectOnly<T> {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        T::definition(types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The way the command receives it.
    fn patch(value: Value) -> Result<SettingsPatch, serde_json::Error> {
        serde_json::from_value::<ObjectOnly<SettingsPatch>>(value).map(|p| p.0)
    }

    #[test]
    fn open_shortcut_must_be_a_bindable_single_chord() {
        for ok in ["Meta+`", "Ctrl+Alt+H", "", " F9 "] {
            let p = patch(serde_json::json!({ "openShortcut": ok })).unwrap();
            let s = p.apply(&Settings::default()).unwrap();
            assert_eq!(s.open_shortcut, ok.trim());
        }
        for bad in ["Meta+", "Hyper+X", "Ctrl+K, Ctrl+C", "Meta+Foo"] {
            let p = patch(serde_json::json!({ "openShortcut": bad })).unwrap();
            assert!(p.apply(&Settings::default()).is_err(), "{bad}");
        }
        // Unbound by default (Nick: keep the option, never bind it for him).
        assert_eq!(Settings::default().open_shortcut, "");
        assert_eq!(Settings::default().palette_shortcut, "");
        let p = patch(serde_json::json!({ "paletteShortcut": "Meta+Foo" })).unwrap();
        assert!(p.apply(&Settings::default()).is_err());
    }

    #[test]
    fn notification_app_rules_are_exact_unique_and_bounded() {
        let ok = json!([{"app": "Discord", "popup": false, "sound": true, "urgency": "low"},
                        {"app": "discord", "popup": true, "sound": false, "urgency": "app"}]);
        let s = patch(json!({"notificationAppRules": ok}))
            .unwrap()
            .apply(&Settings::default())
            .unwrap();
        assert_eq!(s.notification_app_rules.len(), 2);
        assert_eq!(s.notification_app_rules[0].urgency, RuleUrgency::Low);
        for bad in [
            json!([{"app": "A", "popup": true, "sound": true, "urgency": "app"},
                   {"app": "A", "popup": false, "sound": true, "urgency": "app"}]),
            json!([{"app": " A", "popup": true, "sound": true, "urgency": "app"}]),
            json!([{"app": "", "popup": true, "sound": true, "urgency": "app"}]),
        ] {
            assert!(patch(json!({"notificationAppRules": bad}))
                .unwrap()
                .apply(&Settings::default())
                .is_err());
        }
        assert!(patch(json!({"notificationAppRules": [{"app": "A", "popup": true, "sound": true, "urgency": "loud"}]})).is_err());
        assert!(patch(json!({"notificationAppRules": [{"app": "A", "popup": true, "sound": true, "urgency": "app", "x": 1}]})).is_err());
        let many: Vec<_> = (0..=MAX_NOTIFICATION_APP_RULES)
            .map(
                |i| json!({"app": format!("a{i}"), "popup": true, "sound": true, "urgency": "app"}),
            )
            .collect();
        assert!(patch(json!({"notificationAppRules": many}))
            .unwrap()
            .apply(&Settings::default())
            .is_err());
    }

    #[test]
    fn notch_settings_validate_persist_independently_and_reject_atomically() {
        use crate::services::{autostart::Autostart, scratch, settings::SettingsService};
        use std::fs;
        let dir = scratch("notch-settings");
        let service = || {
            SettingsService::new(
                dir.join("data"),
                Autostart::with_default_launcher(dir.clone(), dir.join("app"), true),
            )
        };
        let svc = service();
        let base = svc.get().unwrap();
        let next = svc
            .patch(
                &patch(json!({"panelClockColor":"#123456",
            "notchBorderEnabled":false,"notchBorderColor":"#abcdef",
            "notchBorderOpacity":0.6,"notchBorderWidth":2.5}))
                .unwrap(),
            )
            .unwrap();
        let raw = next.to_file_map();
        assert_eq!(raw["notchBorderOpacity"], json!(0.6));
        assert_eq!(raw["notchBorderWidth"], json!(2.5));
        assert_eq!(raw["notchBorderEnabled"], json!(false));
        assert_eq!(next.border_enabled, base.border_enabled);
        assert_eq!(next.border_color, base.border_color);
        assert_eq!(service().get().unwrap(), next);
        let bytes = fs::read(dir.join("data/settings.json")).unwrap();
        assert!(svc
            .patch(&patch(json!({"notchBorderEnabled":true,"notchBorderColor":"red"})).unwrap())
            .is_err());
        assert_eq!(fs::read(dir.join("data/settings.json")).unwrap(), bytes);
        let limited = patch(json!({"notchBorderOpacity":2,"notchBorderWidth":20}))
            .unwrap()
            .apply(&next)
            .unwrap();
        assert_eq!(limited.to_file_map()["notchBorderOpacity"], json!(1));
        assert_eq!(limited.to_file_map()["notchBorderWidth"], json!(4));
        for value in [
            json!({"notchBorderOpacity":null}),
            json!({"notchBorderWidth":"2"}),
            json!({"notchBorderEnabled":0}),
        ] {
            assert!(patch(value).is_err());
        }
        let mut old = serde_json::to_value(&base).unwrap();
        for key in [
            "notchBorderEnabled",
            "notchBorderColor",
            "notchBorderOpacity",
            "notchBorderWidth",
        ] {
            old.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(serde_json::from_value::<Settings>(old).unwrap(), base);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn notification_deny_list_is_strict_bounded_and_never_normalized() {
        let base = Settings::default();
        let names = vec!["Mail", "mail", "メール"];
        let next = patch(json!({"notificationDeniedApps": names}))
            .unwrap()
            .apply(&base)
            .unwrap();
        assert_eq!(next.notification_denied_apps, names);
        assert_eq!(Settings::from_file_value(&json!(next.to_file_map())), next);
        assert_eq!(patch(json!({})).unwrap().apply(&next).unwrap(), next);
        assert!(patch(json!({"notificationDeniedApps": []}))
            .unwrap()
            .apply(&next)
            .unwrap()
            .notification_denied_apps
            .is_empty());
        for value in [
            Value::Null,
            json!("Mail"),
            json!([null]),
            json!([1]),
            json!({}),
        ] {
            assert!(patch(json!({"notificationDeniedApps": value})).is_err());
        }
        for names in [
            vec![""],
            vec![" "],
            vec![" Mail"],
            vec!["Mail "],
            vec!["Mail\nClient"],
            vec!["Mail\0Client"],
            vec!["Mail", "Mail"],
        ] {
            assert!(
                patch(json!({"stealth": true, "notificationDeniedApps": names}))
                    .unwrap()
                    .apply(&base)
                    .is_err()
            );
        }
        for (name, valid) in [
            ("x".repeat(MAX_NOTIFICATION_APP_BYTES), true),
            ("x".repeat(MAX_NOTIFICATION_APP_BYTES + 1), false),
            ("é".repeat(MAX_NOTIFICATION_APP_BYTES / 2), true),
            ("é".repeat(MAX_NOTIFICATION_APP_BYTES / 2 + 1), false),
        ] {
            assert_eq!(
                patch(json!({"notificationDeniedApps": [name]}))
                    .unwrap()
                    .apply(&base)
                    .is_ok(),
                valid
            );
        }
        let mut names: Vec<_> = (0..MAX_NOTIFICATION_DENIED_APPS)
            .map(|i| format!("app-{i}"))
            .collect();
        assert!(patch(json!({"notificationDeniedApps": names}))
            .unwrap()
            .apply(&base)
            .is_ok());
        names.push("one-too-many".into());
        assert!(patch(json!({"notificationDeniedApps": names}))
            .unwrap()
            .apply(&base)
            .is_err());
        // Old files remain compatible; invalid lists are rejected as a whole by the loader.
        let mut old = serde_json::to_value(&base).unwrap();
        old.as_object_mut()
            .unwrap()
            .remove("notificationDeniedApps");
        assert_eq!(
            serde_json::from_value::<Settings>(old.clone()).unwrap(),
            base
        );
        assert_eq!(Settings::from_file_value(&old), base);
        assert!(
            Settings::from_file_value(&json!({"notificationDeniedApps": ["Mail", ""]}))
                .notification_denied_apps
                .is_empty()
        );
    }

    #[test]
    fn notification_deny_list_persists_via_settings_service_and_rejects_atomically() {
        use crate::services::{autostart::Autostart, scratch, settings::SettingsService};
        use std::fs;

        let dir = scratch("notification-deny-settings");
        let service = || {
            SettingsService::new(
                dir.join("data"),
                Autostart::with_default_launcher(dir.clone(), dir.join("app"), true),
            )
        };
        let svc = service();
        let next = svc
            .patch(&patch(json!({"notificationDeniedApps": ["Private Mail", "Chat"]})).unwrap())
            .unwrap();
        assert_eq!(service().get().unwrap(), next);
        let file = dir.join("data/settings.json");
        let bytes = fs::read(&file).unwrap();
        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            raw["notificationDeniedApps"],
            json!(["Private Mail", "Chat"])
        );
        assert!(svc
            .patch(
                &patch(json!({"stealth": true, "notificationDeniedApps": ["Chat", "Chat"]}))
                    .unwrap()
            )
            .is_err());
        assert_eq!(svc.get().unwrap(), next);
        assert_eq!(fs::read(&file).unwrap(), bytes);
        assert_eq!(service().get().unwrap(), next);
        let cleared = svc
            .patch(&patch(json!({"notificationDeniedApps": []})).unwrap())
            .unwrap();
        assert!(cleared.notification_denied_apps.is_empty());
        assert_eq!(service().get().unwrap(), cleared);
        assert!(!dir.join(".config").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn nov2_appearance_keys_validate_clamp_and_load_from_old_files() {
        let base = Settings::default();
        assert_eq!(base.glass_strength, 55.0);
        assert!(!base.hermes_theme_sync);
        let next = patch(json!({"glassStrength": 140.4, "hermesThemeSync": true,
            "customAccent": "#AbC123", "customBackground": "", "customText": "#000000"}))
        .unwrap()
        .apply(&base)
        .unwrap();
        assert_eq!(next.glass_strength, 100.0);
        assert!(next.hermes_theme_sync);
        assert_eq!(next.custom_accent, "#AbC123");
        assert_eq!(
            patch(json!({"glassStrength": 12.6}))
                .unwrap()
                .apply(&base)
                .unwrap()
                .glass_strength,
            13.0
        );
        for bad in [
            json!({"customText": "red"}),
            json!({"customAccent": "#12345"}),
        ] {
            assert!(patch(bad).unwrap().apply(&base).is_err());
        }
        assert!(patch(json!({"glassStrength": null})).is_err());
        // A pre-Nov-2 settings.json (no new keys) keeps the defaults.
        let old = Settings::from_file_value(&json!({"translucent": false, "accent": "#ffffff"}));
        assert_eq!(old.glass_strength, 55.0);
        assert!(!old.translucent && !old.hermes_theme_sync && old.custom_text.is_empty());
        assert_eq!(old.to_file_map()["glassStrength"], json!(55));
    }

    #[test]
    fn terminal_command_quotes_and_escapes_are_argv_not_shell_expansion() {
        assert_eq!(parse_terminal_command(r#"'/absolute path/Exe' --TUI "double words" single\ word '' \"literal\" $HOME *.txt ; | >"#).unwrap(),
            ["/absolute path/Exe", "--TUI", "double words", "single word", "", "\"literal\"", "$HOME", "*.txt", ";", "|", ">"]);
        assert_eq!(parse_terminal_command("hermes").unwrap(), ["hermes"]);
        assert_eq!(
            parse_terminal_command("hermes --tui").unwrap(),
            ["hermes", "--tui"]
        );
        assert_eq!(
            parse_terminal_command("hermes --cli").unwrap(),
            ["hermes", "--cli"]
        );
        assert_eq!(
            parse_terminal_command("hermes-local").unwrap(),
            ["hermes-local"]
        );
    }

    #[test]
    fn defaults_cover_full_spec_and_preserve_legacy_keys() {
        let settings = Settings::default();
        let value = serde_json::to_value(&settings).unwrap();
        let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        let mut expected: Vec<_> = settings
            .to_file_map()
            .keys()
            .map(|k| k.to_string())
            .collect();
        expected.sort();
        // 55 keys up to Nov 3 + 14 on Nov 5 (game mode, shortcut, notch states, monitors,
        // motion, terminal options) + 8 on Nov 6 (KDE popups, sound, app rules, approvals,
        // game-mode extras, palette shortcut) + 1 later Nov 6 (notification sound choice).
        // Every key comes from the one `settings_schema!` table.
        assert_eq!(keys.len(), 78);
        assert_eq!(SETTINGS_KEYS.len(), 78);
        let mut table: Vec<_> = SETTINGS_KEYS.iter().map(|k| k.to_string()).collect();
        table.sort();
        assert_eq!(table, keys, "file keys must equal the serde (IPC) keys");
        assert_eq!(keys, expected);
        assert_eq!(settings.hover_delay, 180.0);
        assert_eq!(settings.ui_font, UiFont::System);
        assert!(settings.border_enabled && settings.show_date && settings.hardware_monitor);
        assert_eq!(value["uiFont"], "system");
    }

    #[test]
    fn all_future_keys_defaults_validation_clamping_and_nullable_clearing() {
        let defaults = Settings::default();
        let raw = serde_json::to_value(&defaults).unwrap();
        for (key, expected) in [
            ("notificationsEnabled", json!(true)),
            ("systemNotificationMirror", json!(false)),
            ("notificationDeniedApps", json!([])),
            ("toastTtlMs", json!(6000.0)),
            ("redactBodies", json!(false)),
            ("dnd", json!(false)),
            ("clipboardHistory", json!(false)),
            ("clipboardHistorySize", json!(100.0)),
            ("clipboardPersist", json!(false)),
            ("batteryAlerts", json!(true)),
            ("batteryLowPercent", json!(20.0)),
            ("batteryCriticalPercent", json!(10.0)),
            ("weatherEnabled", json!(false)),
            ("weatherLocation", Value::Null),
            ("weatherUnit", json!("fahrenheit")),
            ("hourFormat", json!("h12")),
            ("displayId", Value::Null),
        ] {
            assert_eq!(raw[key], expected, "{key}");
        }
        for (key, lo, hi) in [
            ("toastTtlMs", 1500.0, 20000.0),
            ("clipboardHistorySize", 10.0, 500.0),
            ("batteryLowPercent", 5.0, 50.0),
            ("batteryCriticalPercent", 1.0, 30.0),
        ] {
            for (input, expected) in [
                (-100.0, lo),
                (100000.0, hi),
                ((lo + hi) / 2.0, (lo + hi) / 2.0),
            ] {
                let s = patch(json!({key: input}))
                    .unwrap()
                    .apply(&defaults)
                    .unwrap();
                assert_eq!(
                    serde_json::to_value(s).unwrap()[key],
                    json!(expected),
                    "{key}"
                );
            }
            assert!(patch(json!({key: null})).is_err());
            assert!(patch(json!({key: "20"})).is_err());
        }
        let place = json!({"name":"Fixture", "latitude":42.5, "longitude":-71.0, "country":"US"});
        let next = patch(json!({"weatherLocation":place, "displayId":"fixture-display", "weatherUnit":"celsius", "hourFormat":"h24",
            "notificationsEnabled":false, "systemNotificationMirror":true, "redactBodies":true, "dnd":true,
            "clipboardHistory":true, "clipboardPersist":true, "batteryAlerts":false, "weatherEnabled":true}))
            .unwrap().apply(&defaults).unwrap();
        assert_eq!(patch(json!({})).unwrap().apply(&next).unwrap(), next);
        let cleared = patch(json!({"weatherLocation":null,"displayId":null}))
            .unwrap()
            .apply(&next)
            .unwrap();
        assert!(cleared.weather_location.is_none() && cleared.display_id.is_none());
        assert!(
            !next.notifications_enabled
                && next.system_notification_mirror
                && next.redact_bodies
                && next.dnd
        );
        assert!(
            next.clipboard_history
                && next.clipboard_persist
                && !next.battery_alerts
                && next.weather_enabled
        );
        assert_eq!(Settings::from_file_value(&json!(next.to_file_map())), next);
        for bad in [
            json!({"weatherUnit":"kelvin"}),
            json!({"hourFormat":"24"}),
            json!({"displayId":""}),
            json!({"displayId":12}),
            json!({"weatherLocation":{"name":"Fixture","latitude":91,"longitude":0,"country":"US"}}),
            json!({"weatherLocation":{"name":"Fixture","latitude":0,"longitude":181,"country":"US"}}),
            json!({"weatherLocation":{"name":"","latitude":0,"longitude":0,"country":"US"}}),
        ] {
            assert!(match patch(bad) {
                Err(_) => true,
                Ok(p) => p.apply(&next).is_err(),
            });
        }
        let nonfinite = SettingsPatch {
            toast_ttl_ms: Some(f64::NAN),
            ..SettingsPatch::default()
        };
        assert!(nonfinite.apply(&next).is_err());
    }

    #[test]
    fn music_settings_are_opt_in_validated_and_round_trip() {
        let defaults = Settings::default();
        assert!(!defaults.lyrics_enabled && !defaults.equalizer_enabled);
        assert_eq!(defaults.equalizer_bands, 16.0);
        let next =
            patch(json!({"lyricsEnabled": true, "equalizerEnabled": true, "equalizerBands": 24}))
                .unwrap()
                .apply(&defaults)
                .unwrap();
        assert!(next.lyrics_enabled && next.equalizer_enabled);
        assert_eq!(next.equalizer_bands, 24.0);
        assert_eq!(Settings::from_file_value(&json!(next.to_file_map())), next);
        // Clamped to the module-registry range and rounded to whole bands.
        for (asked, stored) in [(1.0, 8.0), (999.0, 32.0), (12.4, 12.0), (12.6, 13.0)] {
            let s = patch(json!({"equalizerBands": asked}))
                .unwrap()
                .apply(&defaults)
                .unwrap();
            assert_eq!(s.equalizer_bands, stored, "{asked}");
        }
        for bad in [
            json!({"lyricsEnabled": null}),
            json!({"lyricsEnabled": "yes"}),
            json!({"equalizerEnabled": 1}),
            json!({"equalizerBands": "16"}),
            json!({"equalizerBands": null}),
        ] {
            assert!(patch(bad).is_err());
        }
        let nan = SettingsPatch {
            equalizer_bands: Some(f64::NAN),
            ..SettingsPatch::default()
        };
        assert!(nan.apply(&defaults).is_err());
        // An old file without the keys keeps the defaults.
        let old = Settings::from_file_value(&json!({"hoverDelay": 200}));
        assert!(!old.lyrics_enabled && !old.equalizer_enabled && old.equalizer_bands == 16.0);
    }

    #[test]
    fn file_map_writes_whole_numbers_as_integers() {
        let map = Settings::default().to_file_map();
        assert_eq!(serde_json::to_string(&map["hoverDelay"]).unwrap(), "180");
        assert_eq!(
            serde_json::to_string(&map["terminalFontSize"]).unwrap(),
            "13"
        );
        let s = Settings {
            offset_x: 2.5,
            ..Settings::default()
        };
        assert_eq!(
            serde_json::to_string(&s.to_file_map()["offsetX"]).unwrap(),
            "2.5"
        );
    }

    #[test]
    fn patch_typing_is_strict() {
        // stealth:'yes', null, unknown key, non-object are all rejected by serde itself.
        assert!(patch(json!({"stealth": "yes"})).is_err());
        assert!(patch(json!({"stealth": null})).is_err());
        assert!(patch(json!({"hoverDelay": "5"})).is_err());
        assert!(patch(json!({"anything": true})).is_err());
        assert!(patch(json!([])).is_err());
        assert!(patch(json!([true, 1])).is_err());
        assert!(patch(json!("x")).is_err());
        assert!(patch(json!(null)).is_err());
        assert!(serde_json::from_str::<ObjectOnly<SettingsPatch>>(
            r#"{"stealth":true,"stealth":false}"#
        )
        .is_err());
        assert!(patch(json!({"uiFont": "comic"})).is_err());
        assert!(patch(json!({})).unwrap().is_empty());
    }

    #[test]
    fn patch_validation_is_atomic_and_clamps() {
        let base = Settings::default();
        let next = patch(json!({"accent": "#aBc123", "offsetX": 900, "terminalFontSize": 3}))
            .unwrap()
            .apply(&base)
            .unwrap();
        assert_eq!(next.offset_x, 400.0);
        assert_eq!(next.terminal_font_size, 8.0);
        assert_eq!(next.accent, "#aBc123");
        for bad in ["red", "#fff", "#12345g", "#1234567", "abcdef0"] {
            let result = patch(json!({"hoverDelay": 5, "borderColor": bad}))
                .unwrap()
                .apply(&base);
            assert!(result.is_err(), "{bad}");
        }
        assert_eq!(
            patch(json!({"accent": ""}))
                .unwrap()
                .apply(&next)
                .unwrap()
                .accent,
            ""
        );
        assert_eq!(
            patch(json!({"hoverDelay": -5}))
                .unwrap()
                .apply(&base)
                .unwrap()
                .hover_delay,
            0.0
        );
    }

    #[test]
    fn loader_ignores_unknown_and_skips_invalid_values() {
        let loaded = Settings::from_file_value(&json!({
            "hoverDelay": 99999, "accent": "nope", "stealth": true, "mystery": 1,
            "uiFont": "openrunde", "borderEnabled": "yes", "notificationsEnabled": false
        }));
        assert_eq!(loaded.hover_delay, 1200.0);
        assert_eq!(loaded.accent, "");
        assert!(loaded.stealth);
        assert_eq!(loaded.ui_font, UiFont::Openrunde);
        assert!(loaded.border_enabled);
        assert!(!loaded.notifications_enabled);
        assert_eq!(Settings::from_file_value(&json!([1])), Settings::default());
    }
}
