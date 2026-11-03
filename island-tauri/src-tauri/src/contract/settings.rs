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
/// Exact, case-sensitive app names, matching the notification store's policy.
pub const MAX_NOTIFICATION_DENIED_APPS: usize = 128;
pub const MAX_NOTIFICATION_APP_BYTES: usize = 256;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Phase 6b1: additive module settings. Secret subscription URLs are never config.
    #[serde(default)]
    pub module_registry: super::modules::Registry,
    pub autostart: bool,
    pub show_in_taskbar: bool,
    pub stealth: bool,
    pub click_sounds: bool,
    /// 0..=1200 ms; 0 disables hover preview.
    pub hover_delay: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub accent: String,
    pub background: String,
    pub border_enabled: bool,
    pub border_color: String,
    /// Pill BACKGROUND, not digits.
    pub panel_clock_color: String,
    #[serde(default = "default_true")]
    pub notch_border_enabled: bool,
    #[serde(default)]
    pub notch_border_color: String,
    #[serde(default = "default_notch_border_opacity")]
    pub notch_border_opacity: f64,
    #[serde(default = "default_notch_border_width")]
    pub notch_border_width: f64,
    pub ui_font: UiFont,
    pub show_date: bool,
    pub hardware_monitor: bool,
    pub terminal_font_size: f64,
    /// Direct executable and quoted arguments; empty retains the original Hermes launch.
    #[serde(default)]
    pub terminal_command: String,
    pub notifications_enabled: bool,
    pub system_notification_mirror: bool,
    /// Additive privacy setting; old files default to an empty deny list.
    #[serde(default)]
    pub notification_denied_apps: Vec<String>,
    pub toast_ttl_ms: f64,
    pub redact_bodies: bool,
    pub dnd: bool,
    pub clipboard_history: bool,
    pub clipboard_history_size: f64,
    pub clipboard_persist: bool,
    #[serde(default)]
    pub clipboard_deny_patterns: Vec<String>,
    pub battery_alerts: bool,
    pub battery_low_percent: f64,
    pub battery_critical_percent: f64,
    pub weather_enabled: bool,
    pub weather_location: Option<GeoPlace>,
    pub weather_unit: TempUnit,
    pub hour_format: HourFormat,
    pub display_id: Option<String>,
    /// Phase 6m (additive): fetch synced lyrics from LRCLIB for the playing track. A network
    /// call that sends title/artist/album/duration, so it is opt-in (default off).
    #[serde(default)]
    pub lyrics_enabled: bool,
    /// Phase 6m (additive): live equalizer from a monitor capture of the playing stream.
    /// Audio capture is privacy-sensitive, so it is opt-in (default off).
    #[serde(default)]
    pub equalizer_enabled: bool,
    /// Phase 6m (additive): number of equalizer bands, 8..=32 (module-registry range).
    #[serde(default = "default_equalizer_bands")]
    pub equalizer_bands: f64,
    /// Additive (post-9b): horizontal swipe / trackpad gesture switches between views.
    #[serde(default = "default_true")]
    pub swipe_navigation: bool,
    /// Additive (post-9b): Translucent island body over KWin blur-behind.
    #[serde(default = "default_true")]
    pub translucent: bool,
    /// Additive (Nov 2): how see-through the translucent body is, 0 (opaque) ..= 100.
    #[serde(default = "default_glass_strength")]
    pub glass_strength: f64,
    /// Additive (Nov 2): mirror the island colour scheme into the Hermes skin
    /// (`skins/ripple-island.yaml` + `hermes skin use`); off restores the previous skin.
    #[serde(default)]
    pub hermes_theme_sync: bool,
    /// Additive (Nov 2): the Custom colour scheme's seed colours (empty = scheme default).
    #[serde(default)]
    pub custom_accent: String,
    #[serde(default)]
    pub custom_background: String,
    #[serde(default)]
    pub custom_text: String,
    /// Additive (Nov 2c): Linux display server, applied at the next launch by `run.sh`.
    #[serde(default = "default_display_backend")]
    pub display_backend: DisplayBackend,
    /// Additive (Nov 2c): native notch width in logical px (144 = the original slot).
    #[serde(default = "default_notch_width")]
    pub notch_width: f64,
    #[serde(default)]
    pub show_notch_on_every_monitor: bool,
}

fn default_display_backend() -> DisplayBackend {
    DisplayBackend::Wayland
}
pub const NOTCH_WIDTH_RANGE: (f64, f64) = (110.0, 320.0);
fn default_notch_width() -> f64 {
    144.0
}

fn default_glass_strength() -> f64 {
    55.0
}

fn default_notch_border_opacity() -> f64 {
    0.1
}
fn default_notch_border_width() -> f64 {
    0.8
}

fn default_true() -> bool {
    true
}

fn default_equalizer_bands() -> f64 {
    16.0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            module_registry: super::modules::Registry::defaults(),
            autostart: false,
            show_in_taskbar: false,
            stealth: false,
            click_sounds: false,
            hover_delay: 180.0,
            offset_x: 0.0,
            offset_y: 0.0,
            accent: String::new(),
            background: String::new(),
            border_enabled: true,
            border_color: String::new(),
            panel_clock_color: String::new(),
            notch_border_enabled: true,
            notch_border_color: String::new(),
            notch_border_opacity: default_notch_border_opacity(),
            notch_border_width: default_notch_border_width(),
            ui_font: UiFont::System,
            show_date: true,
            hardware_monitor: true,
            terminal_font_size: 13.0,
            terminal_command: String::new(),
            notifications_enabled: true,
            system_notification_mirror: false,
            notification_denied_apps: Vec::new(),
            toast_ttl_ms: 6000.0,
            redact_bodies: false,
            dnd: false,
            clipboard_history: false,
            clipboard_history_size: 100.0,
            clipboard_persist: false,
            clipboard_deny_patterns: Vec::new(),
            battery_alerts: true,
            battery_low_percent: 20.0,
            battery_critical_percent: 10.0,
            weather_enabled: false,
            weather_location: None,
            weather_unit: TempUnit::Fahrenheit,
            hour_format: HourFormat::H12,
            display_id: None,
            lyrics_enabled: false,
            equalizer_enabled: false,
            equalizer_bands: 16.0,
            swipe_navigation: true,
            translucent: true,
            glass_strength: default_glass_strength(),
            hermes_theme_sync: false,
            custom_accent: String::new(),
            custom_background: String::new(),
            custom_text: String::new(),
            display_backend: default_display_backend(),
            notch_width: default_notch_width(),
            show_notch_on_every_monitor: false,
        }
    }
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
    /// The JSON written to `settings.json`: sorted keys, whole numbers as integers.
    pub fn to_file_map(&self) -> BTreeMap<&'static str, Value> {
        BTreeMap::from([
            ("moduleRegistry", json!(self.module_registry)),
            ("autostart", json!(self.autostart)),
            ("showInTaskbar", json!(self.show_in_taskbar)),
            ("stealth", json!(self.stealth)),
            ("clickSounds", json!(self.click_sounds)),
            ("hoverDelay", json_number(self.hover_delay)),
            ("offsetX", json_number(self.offset_x)),
            ("offsetY", json_number(self.offset_y)),
            ("accent", json!(self.accent)),
            ("background", json!(self.background)),
            ("borderEnabled", json!(self.border_enabled)),
            ("borderColor", json!(self.border_color)),
            ("panelClockColor", json!(self.panel_clock_color)),
            ("notchBorderEnabled", json!(self.notch_border_enabled)),
            ("notchBorderColor", json!(self.notch_border_color)),
            ("notchBorderOpacity", json_number(self.notch_border_opacity)),
            ("notchBorderWidth", json_number(self.notch_border_width)),
            ("uiFont", json!(self.ui_font.as_str())),
            ("showDate", json!(self.show_date)),
            ("hardwareMonitor", json!(self.hardware_monitor)),
            ("terminalFontSize", json_number(self.terminal_font_size)),
            ("terminalCommand", json!(self.terminal_command)),
            ("notificationsEnabled", json!(self.notifications_enabled)),
            (
                "systemNotificationMirror",
                json!(self.system_notification_mirror),
            ),
            (
                "notificationDeniedApps",
                json!(self.notification_denied_apps),
            ),
            ("toastTtlMs", json_number(self.toast_ttl_ms)),
            ("redactBodies", json!(self.redact_bodies)),
            ("dnd", json!(self.dnd)),
            ("clipboardHistory", json!(self.clipboard_history)),
            (
                "clipboardHistorySize",
                json_number(self.clipboard_history_size),
            ),
            ("clipboardPersist", json!(self.clipboard_persist)),
            ("clipboardDenyPatterns", json!(self.clipboard_deny_patterns)),
            ("batteryAlerts", json!(self.battery_alerts)),
            ("batteryLowPercent", json_number(self.battery_low_percent)),
            (
                "batteryCriticalPercent",
                json_number(self.battery_critical_percent),
            ),
            ("weatherEnabled", json!(self.weather_enabled)),
            ("weatherLocation", json!(self.weather_location)),
            ("weatherUnit", json!(self.weather_unit)),
            ("hourFormat", json!(self.hour_format)),
            ("displayId", json!(self.display_id)),
            ("lyricsEnabled", json!(self.lyrics_enabled)),
            ("equalizerEnabled", json!(self.equalizer_enabled)),
            ("equalizerBands", json_number(self.equalizer_bands)),
            ("swipeNavigation", json!(self.swipe_navigation)),
            ("translucent", json!(self.translucent)),
            ("glassStrength", json_number(self.glass_strength)),
            ("hermesThemeSync", json!(self.hermes_theme_sync)),
            ("customAccent", json!(self.custom_accent)),
            ("customBackground", json!(self.custom_background)),
            ("customText", json!(self.custom_text)),
            ("displayBackend", json!(self.display_backend)),
            ("notchWidth", json_number(self.notch_width)),
            (
                "showNotchOnEveryMonitor",
                json!(self.show_notch_on_every_monitor),
            ),
        ])
    }

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
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Outer `None` means omitted; `Some(None)` means explicitly clear a nullable setting.
fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Partial update. Unknown keys and wrong types are rejected by serde; range/pattern checks and
/// clamping happen in [`SettingsPatch::apply`].
/// Deserialize it through [`ObjectOnly`] at the IPC boundary (arrays are otherwise accepted by
/// serde's derived struct impl).
#[derive(Deserialize, Default, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = super::modules::Registry)]
    pub module_registry: Option<super::modules::Registry>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub autostart: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub show_in_taskbar: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub stealth: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub click_sounds: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub hover_delay: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub offset_x: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub offset_y: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = String)]
    pub accent: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = String)]
    pub background: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub border_enabled: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = String)]
    pub border_color: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = String)]
    pub panel_clock_color: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub notch_border_enabled: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = String)]
    pub notch_border_color: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub notch_border_opacity: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub notch_border_width: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = UiFont)]
    pub ui_font: Option<UiFont>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub show_date: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub hardware_monitor: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub terminal_font_size: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = String)]
    pub terminal_command: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub notifications_enabled: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub system_notification_mirror: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = Vec<String>)]
    pub notification_denied_apps: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub toast_ttl_ms: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub redact_bodies: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub dnd: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub clipboard_history: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub clipboard_history_size: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub clipboard_persist: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = Vec<String>)]
    pub clipboard_deny_patterns: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub battery_alerts: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub battery_low_percent: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub battery_critical_percent: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub weather_enabled: Option<bool>,
    #[serde(default, deserialize_with = "nullable")]
    #[specta(optional, type = Option<GeoPlace>)]
    pub weather_location: Option<Option<GeoPlace>>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = TempUnit)]
    pub weather_unit: Option<TempUnit>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = HourFormat)]
    pub hour_format: Option<HourFormat>,
    #[serde(default, deserialize_with = "nullable")]
    #[specta(optional, type = Option<String>)]
    pub display_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub lyrics_enabled: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub equalizer_enabled: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub equalizer_bands: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub swipe_navigation: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub translucent: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub glass_strength: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub hermes_theme_sync: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = String)]
    pub custom_accent: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = String)]
    pub custom_background: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = String)]
    pub custom_text: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = DisplayBackend)]
    pub display_backend: Option<DisplayBackend>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = f64)]
    pub notch_width: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    #[specta(optional, type = bool)]
    pub show_notch_on_every_monitor: Option<bool>,
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

impl SettingsPatch {
    pub fn is_empty(&self) -> bool {
        *self == SettingsPatch::default()
    }

    /// Validate the whole patch against `base`; any invalid key rejects everything.
    pub fn apply(&self, base: &Settings) -> Result<Settings, IslandError> {
        let mut next = base.clone();
        if let Some(command) = &self.terminal_command {
            parse_terminal_command(command)?;
            next.terminal_command = command.clone();
        }
        if let Some(registry) = &self.module_registry {
            registry
                .export_json()
                .map_err(|_| IslandError::invalid("Invalid module registry"))?;
            next.module_registry = registry.clone();
        }
        if let Some(v) = self.autostart {
            next.autostart = v;
        }
        if let Some(v) = self.show_in_taskbar {
            next.show_in_taskbar = v;
        }
        if let Some(v) = self.stealth {
            next.stealth = v;
        }
        if let Some(v) = self.click_sounds {
            next.click_sounds = v;
        }
        if let Some(v) = self.hover_delay {
            next.hover_delay = number("hoverDelay", v, HOVER_DELAY_RANGE)?;
        }
        if let Some(v) = self.offset_x {
            next.offset_x = number("offsetX", v, OFFSET_X_RANGE)?;
        }
        if let Some(v) = self.offset_y {
            next.offset_y = number("offsetY", v, OFFSET_Y_RANGE)?;
        }
        if let Some(v) = &self.accent {
            next.accent = color("accent", v)?;
        }
        if let Some(v) = &self.background {
            next.background = color("background", v)?;
        }
        if let Some(v) = self.border_enabled {
            next.border_enabled = v;
        }
        if let Some(v) = &self.border_color {
            next.border_color = color("borderColor", v)?;
        }
        if let Some(v) = &self.panel_clock_color {
            next.panel_clock_color = color("panelClockColor", v)?;
        }
        if let Some(v) = self.notch_border_enabled {
            next.notch_border_enabled = v;
        }
        if let Some(v) = &self.notch_border_color {
            next.notch_border_color = color("notchBorderColor", v)?;
        }
        if let Some(v) = self.notch_border_opacity {
            next.notch_border_opacity = number("notchBorderOpacity", v, (0.0, 1.0))?;
        }
        if let Some(v) = self.notch_border_width {
            next.notch_border_width = number("notchBorderWidth", v, (0.0, 4.0))?;
        }
        if let Some(v) = self.ui_font {
            next.ui_font = v;
        }
        if let Some(v) = self.show_date {
            next.show_date = v;
        }
        if let Some(v) = self.hardware_monitor {
            next.hardware_monitor = v;
        }
        if let Some(v) = self.terminal_font_size {
            next.terminal_font_size = number("terminalFontSize", v, TERMINAL_FONT_SIZE_RANGE)?;
        }
        if let Some(v) = self.notifications_enabled {
            next.notifications_enabled = v;
        }
        if let Some(v) = self.system_notification_mirror {
            next.system_notification_mirror = v;
        }
        if let Some(apps) = &self.notification_denied_apps {
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
            next.notification_denied_apps = apps.clone();
        }
        if let Some(v) = self.toast_ttl_ms {
            next.toast_ttl_ms = number("toastTtlMs", v, (1500.0, 20000.0))?;
        }
        if let Some(v) = self.redact_bodies {
            next.redact_bodies = v;
        }
        if let Some(v) = self.dnd {
            next.dnd = v;
        }
        if let Some(v) = self.clipboard_history {
            next.clipboard_history = v;
        }
        if let Some(v) = self.clipboard_history_size {
            next.clipboard_history_size = number("clipboardHistorySize", v, (10.0, 500.0))?;
        }
        if let Some(v) = self.clipboard_persist {
            next.clipboard_persist = v;
        }
        if let Some(patterns) = &self.clipboard_deny_patterns {
            crate::integrations::clipboard::history::HistoryConfig {
                deny_patterns: patterns.clone(),
                ..Default::default()
            }
            .validated()
            .map_err(|_| IslandError::invalid("Invalid clipboardDenyPatterns"))?;
            next.clipboard_deny_patterns = patterns.clone();
        }
        if let Some(v) = self.battery_alerts {
            next.battery_alerts = v;
        }
        if let Some(v) = self.battery_low_percent {
            next.battery_low_percent = number("batteryLowPercent", v, (5.0, 50.0))?;
        }
        if let Some(v) = self.battery_critical_percent {
            next.battery_critical_percent = number("batteryCriticalPercent", v, (1.0, 30.0))?;
        }
        if let Some(v) = self.weather_enabled {
            next.weather_enabled = v;
        }
        if let Some(v) = &self.weather_location {
            if let Some(place) = v {
                place.validate()?;
            }
            next.weather_location = v.clone();
        }
        if let Some(v) = self.weather_unit {
            next.weather_unit = v;
        }
        if let Some(v) = self.hour_format {
            next.hour_format = v;
        }
        if let Some(v) = &self.display_id {
            if v.as_ref().is_some_and(|id| {
                id.is_empty() || id.len() > 256 || id.chars().any(char::is_control)
            }) {
                return Err(IslandError::invalid("Invalid displayId"));
            }
            next.display_id = v.clone();
        }
        if let Some(v) = self.lyrics_enabled {
            next.lyrics_enabled = v;
        }
        if let Some(v) = self.equalizer_enabled {
            next.equalizer_enabled = v;
        }
        if let Some(v) = self.equalizer_bands {
            // Whole bands only: a fractional count would be silently truncated downstream.
            next.equalizer_bands = number("equalizerBands", v, (8.0, 32.0))?.round();
        }
        if let Some(v) = self.swipe_navigation {
            next.swipe_navigation = v;
        }
        if let Some(v) = self.translucent {
            next.translucent = v;
        }
        if let Some(v) = self.glass_strength {
            next.glass_strength = number("glassStrength", v, (0.0, 100.0))?.round();
        }
        if let Some(v) = self.hermes_theme_sync {
            next.hermes_theme_sync = v;
        }
        if let Some(v) = &self.custom_accent {
            next.custom_accent = color("customAccent", v)?;
        }
        if let Some(v) = &self.custom_background {
            next.custom_background = color("customBackground", v)?;
        }
        if let Some(v) = &self.custom_text {
            next.custom_text = color("customText", v)?;
        }
        if let Some(v) = self.display_backend {
            next.display_backend = v;
        }
        if let Some(v) = self.show_notch_on_every_monitor {
            next.show_notch_on_every_monitor = v;
        }
        if let Some(v) = self.notch_width {
            next.notch_width = number("notchWidth", v, NOTCH_WIDTH_RANGE)?.round();
        }
        Ok(next)
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
        // 40 existing keys + four independent notch border settings + glass strength,
        // Hermes theme sync, three Custom scheme seed colours, backend/notch width, terminal command.
        assert_eq!(keys.len(), 53);
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
