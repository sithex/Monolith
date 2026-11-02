//! Versioned, pure module settings. No persistence, IPC, or service activation.
//! Use [`Registry::import_json`] for bounded atomic replacement and
//! [`ModuleEntry::is_active`] instead of a stored unknown module's enabled flag.
#![forbid(unsafe_code)]

use serde::de::{DeserializeOwned, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const CURRENT_VERSION: u32 = 2;
pub const MAX_JSON_BYTES: usize = 256 * 1024;
pub const KNOWN_IDS: [&str; 17] = [
    "terminal",
    "monitor",
    "ai",
    "settings",
    "notifications",
    "clipboard",
    "music",
    "timer",
    "scratchpad",
    "downloads",
    "calendar",
    "brightness",
    "agent-tracker",
    "mixer",
    "quick-toggles",
    "update-badge",
    "system-tiles",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryError {
    InvalidId(String),
    DuplicateId(ModuleId),
    MissingModule(ModuleId),
    UnknownModule(ModuleId),
    InvalidPermutation,
    ConfigMismatch(ModuleId),
    InvalidConfig(ModuleId),
    UnsupportedVersion(u32),
    TooLarge,
    TooManyModules,
    Json(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RegistryError {}

/// A validated ASCII slug. Unknown slugs are retained, never normalized.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, specta::Type)]
#[serde(transparent)]
pub struct ModuleId(String);

impl ModuleId {
    pub fn new(id: impl Into<String>) -> Result<Self, RegistryError> {
        let id = id.into();
        let bytes = id.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 32
            || !bytes[0].is_ascii_lowercase()
            || !bytes
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        {
            return Err(RegistryError::InvalidId(id));
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn is_known(&self) -> bool {
        KNOWN_IDS.contains(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for ModuleId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// Opaque forward-compatible JSON, with only `f64` numbers in the wire type.
/// Object keys in opaque data are user data, not contract field names.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(untagged)]
pub enum ModuleJsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<ModuleJsonValue>),
    Object(BTreeMap<String, ModuleJsonValue>),
}

impl ModuleJsonValue {
    fn is_finite(&self) -> bool {
        match self {
            Self::Number(n) => n.is_finite(),
            Self::Array(values) => values.iter().all(Self::is_finite),
            Self::Object(values) => values.values().all(Self::is_finite),
            _ => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ModuleConfig {
    SystemTiles {},
    UpdateBadge {},
    QuickToggles {},
    Mixer {},
    AgentTracker {},
    Brightness {},
    Timer {},
    Scratchpad {},
    Downloads {},
    Calendar {},
    Terminal {
        font_size: f64,
    },
    Monitor {},
    Ai {},
    Settings {},
    Notifications {
        system_notification_mirror: bool,
        toast_ttl_ms: f64,
        redact_bodies: bool,
        dnd: bool,
    },
    Clipboard {
        history_size: f64,
        persist: bool,
    },
    Music {
        lyrics: bool,
        equalizer: bool,
        equalizer_bands: u32,
    },
    /// Only valid for an unknown module ID; serialized without a wrapper/tag.
    #[serde(untagged)]
    Unknown(ModuleJsonValue),
}

impl ModuleConfig {
    fn kind(&self) -> Option<&'static str> {
        match self {
            Self::SystemTiles {} => Some("system-tiles"),
            Self::UpdateBadge {} => Some("update-badge"),
            Self::QuickToggles {} => Some("quick-toggles"),
            Self::Mixer {} => Some("mixer"),
            Self::Brightness {} => Some("brightness"),
            Self::AgentTracker {} => Some("agent-tracker"),
            Self::Timer {} => Some("timer"),
            Self::Scratchpad {} => Some("scratchpad"),
            Self::Downloads {} => Some("downloads"),
            Self::Calendar {} => Some("calendar"),
            Self::Terminal { .. } => Some("terminal"),
            Self::Monitor {} => Some("monitor"),
            Self::Ai {} => Some("ai"),
            Self::Settings {} => Some("settings"),
            Self::Notifications { .. } => Some("notifications"),
            Self::Clipboard { .. } => Some("clipboard"),
            Self::Music { .. } => Some("music"),
            Self::Unknown(_) => None,
        }
    }

    fn validate(&self, id: &ModuleId) -> Result<(), RegistryError> {
        if (id.is_known() && self.kind() != Some(id.as_str()))
            || (!id.is_known() && self.kind().is_some())
        {
            return Err(RegistryError::ConfigMismatch(id.clone()));
        }
        let valid = match self {
            Self::Terminal { font_size } => in_range(*font_size, 8.0, 24.0),
            Self::Notifications { toast_ttl_ms, .. } => in_range(*toast_ttl_ms, 1500.0, 20000.0),
            Self::Clipboard { history_size, .. } => in_range(*history_size, 10.0, 500.0),
            Self::Music {
                equalizer_bands, ..
            } => (8..=32).contains(equalizer_bands),
            Self::Unknown(value) => value.is_finite(),
            _ => true,
        };
        if valid {
            Ok(())
        } else {
            Err(RegistryError::InvalidConfig(id.clone()))
        }
    }
}

fn in_range(value: f64, min: f64, max: f64) -> bool {
    value.is_finite() && (min..=max).contains(&value)
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModuleEntry {
    pub id: ModuleId,
    pub enabled: bool,
    pub order: u32,
    pub config: ModuleConfig,
}

impl ModuleEntry {
    /// Unknown modules are inert even if a future file stores enabled=true.
    pub fn is_active(&self) -> bool {
        self.id.is_known() && self.enabled
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EntryInput {
    id: ModuleId,
    enabled: bool,
    order: u32,
    config: Value,
}

impl<'de> Deserialize<'de> for ModuleEntry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let ObjectOnly(raw) = ObjectOnly::<EntryInput>::deserialize(d)?;
        let config = if raw.id.is_known() {
            serde_json::from_value(raw.config).map_err(serde::de::Error::custom)?
        } else {
            ModuleConfig::Unknown(
                serde_json::from_value(raw.config).map_err(serde::de::Error::custom)?,
            )
        };
        config.validate(&raw.id).map_err(serde::de::Error::custom)?;
        Ok(Self {
            id: raw.id,
            enabled: raw.enabled,
            order: raw.order,
            config,
        })
    }
}

/// Private registry fields prevent invalid intermediate states through the API.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    version: u32,
    modules: Vec<ModuleEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryInput {
    version: u32,
    modules: Vec<ModuleEntry>,
}

/// Reject positional arrays as struct inputs (serde derives accept them).
struct ObjectOnly<T>(T);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ObjectOnly<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = ObjectOnly<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                T::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(ObjectOnly)
            }
        }
        d.deserialize_map(ObjectVisitor(std::marker::PhantomData))
    }
}

impl<'de> Deserialize<'de> for Registry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let ObjectOnly(raw) = ObjectOnly::<RegistryInput>::deserialize(d)?;
        upgrade_input(raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportReport {
    pub unknown_ids: Vec<ModuleId>,
    pub upgraded_from: Option<u32>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::defaults()
    }
}

impl Registry {
    pub fn defaults() -> Self {
        let configs = [
            ModuleConfig::Terminal { font_size: 13.0 },
            ModuleConfig::Monitor {},
            ModuleConfig::Ai {},
            ModuleConfig::Settings {},
            ModuleConfig::Notifications {
                system_notification_mirror: false,
                toast_ttl_ms: 6000.0,
                redact_bodies: false,
                dnd: false,
            },
            ModuleConfig::Clipboard {
                history_size: 100.0,
                persist: false,
            },
            ModuleConfig::Music {
                lyrics: true,
                equalizer: true,
                equalizer_bands: 16,
            },
            ModuleConfig::Timer {},
            ModuleConfig::Scratchpad {},
            ModuleConfig::Downloads {},
            ModuleConfig::Calendar {},
            ModuleConfig::Brightness {},
            ModuleConfig::AgentTracker {},
            ModuleConfig::Mixer {},
            ModuleConfig::QuickToggles {},
            ModuleConfig::UpdateBadge {},
            ModuleConfig::SystemTiles {},
        ];
        let modules = KNOWN_IDS
            .into_iter()
            .zip(configs)
            .zip(0u32..)
            .map(|((id, config), order)| ModuleEntry {
                id: ModuleId(id.to_owned()),
                enabled: !matches!(
                    id,
                    "clipboard" | "music" | "agent-tracker" | "update-badge" | "system-tiles"
                ),
                order,
                config,
            })
            .collect();
        Self {
            version: CURRENT_VERSION,
            modules,
        }
    }

    /// Allows subsets of known modules and arbitrary valid unknown slugs.
    pub fn new(version: u32, modules: Vec<ModuleEntry>) -> Result<Self, RegistryError> {
        let registry = Self { version, modules };
        registry.validate()?;
        Ok(registry)
    }

    pub fn version(&self) -> u32 {
        self.version
    }
    pub fn modules(&self) -> &[ModuleEntry] {
        &self.modules
    }
    pub fn get(&self, id: &ModuleId) -> Option<&ModuleEntry> {
        self.modules.iter().find(|m| &m.id == id)
    }
    pub fn unknown_ids(&self) -> Vec<ModuleId> {
        self.modules
            .iter()
            .filter(|m| !m.id.is_known())
            .map(|m| m.id.clone())
            .collect()
    }

    pub fn validate(&self) -> Result<(), RegistryError> {
        if self.version != CURRENT_VERSION {
            return Err(RegistryError::UnsupportedVersion(self.version));
        }
        if u32::try_from(self.modules.len()).is_err() {
            return Err(RegistryError::TooManyModules);
        }
        let mut ids = BTreeSet::new();
        for module in &self.modules {
            if !ids.insert(&module.id) {
                return Err(RegistryError::DuplicateId(module.id.clone()));
            }
            module.config.validate(&module.id)?;
        }
        Ok(())
    }

    /// Exact permutation of ALL present IDs, including inert unknown modules.
    pub fn reorder(&mut self, ids: &[ModuleId]) -> Result<(), RegistryError> {
        let wanted: BTreeSet<_> = ids.iter().collect();
        let present: BTreeSet<_> = self.modules.iter().map(|m| &m.id).collect();
        if ids.len() != self.modules.len() || wanted.len() != ids.len() || wanted != present {
            return Err(RegistryError::InvalidPermutation);
        }
        let mut next = Vec::with_capacity(ids.len());
        for (id, order) in ids.iter().zip(0u32..) {
            let mut module = self
                .get(id)
                .ok_or(RegistryError::InvalidPermutation)?
                .clone();
            module.order = order;
            next.push(module);
        }
        self.modules = next;
        Ok(())
    }

    pub fn set_enabled(&mut self, id: &ModuleId, enabled: bool) -> Result<(), RegistryError> {
        let module = self
            .modules
            .iter_mut()
            .find(|m| &m.id == id)
            .ok_or_else(|| RegistryError::MissingModule(id.clone()))?;
        if enabled && !id.is_known() {
            return Err(RegistryError::UnknownModule(id.clone()));
        }
        module.enabled = enabled;
        Ok(())
    }

    pub fn set_config(&mut self, id: &ModuleId, config: ModuleConfig) -> Result<(), RegistryError> {
        let module = self
            .modules
            .iter_mut()
            .find(|m| &m.id == id)
            .ok_or_else(|| RegistryError::MissingModule(id.clone()))?;
        config.validate(id)?;
        module.config = config;
        Ok(())
    }

    /// Deterministic order by old order, then ID; original is unchanged.
    pub fn normalized(&self) -> Self {
        let mut next = self.clone();
        next.modules
            .sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
        for (module, order) in next.modules.iter_mut().zip(0u32..) {
            module.order = order;
        }
        next
    }

    pub fn export_json(&self) -> Result<String, RegistryError> {
        self.validate()?;
        let json = serde_json::to_string_pretty(&self.normalized()).map_err(json_error)?;
        if json.len() > MAX_JSON_BYTES {
            return Err(RegistryError::TooLarge);
        }
        Ok(json)
    }

    /// Parse a bounded document, upgrade if necessary, validate and normalize.
    pub fn from_json(json: &str) -> Result<(Self, ImportReport), RegistryError> {
        if json.len() > MAX_JSON_BYTES {
            return Err(RegistryError::TooLarge);
        }
        // Parse typed envelopes directly so duplicate contract fields are rejected.
        let ObjectOnly(raw) = parse::<ObjectOnly<RegistryInput>>(json)?;
        let version = raw.version;
        let next = upgrade_input(raw)?.normalized();
        let report = ImportReport {
            unknown_ids: next.unknown_ids(),
            upgraded_from: (version != CURRENT_VERSION).then_some(version),
        };
        Ok((next, report))
    }

    /// On ANY error the existing registry is byte-for-byte logically unchanged.
    pub fn import_json(&mut self, json: &str) -> Result<ImportReport, RegistryError> {
        let (next, report) = Self::from_json(json)?;
        *self = next;
        Ok(report)
    }
}

fn json_error(error: serde_json::Error) -> RegistryError {
    RegistryError::Json(error.to_string())
}
fn parse<T: DeserializeOwned>(json: &str) -> Result<T, RegistryError> {
    serde_json::from_str(json).map_err(json_error)
}

/// In-memory upgrade hook; textual input must use the bounded import API.
/// Synthetic v0 has the same entry schema but predates the music module.
pub fn upgrade(value: Value) -> Result<Registry, RegistryError> {
    let ObjectOnly(raw) =
        serde_json::from_value::<ObjectOnly<RegistryInput>>(value).map_err(json_error)?;
    upgrade_input(raw)
}

fn upgrade_input(mut raw: RegistryInput) -> Result<Registry, RegistryError> {
    match raw.version {
        CURRENT_VERSION => Registry::new(raw.version, raw.modules),
        0 | 1 => {
            // Validate BEFORE modifying, including duplicates and all config ranges.
            let mut next = Registry::new(CURRENT_VERSION, std::mem::take(&mut raw.modules))?;
            // Migration never activates a module this version doesn't understand.
            for module in &mut next.modules {
                if !module.id.is_known() {
                    module.enabled = false;
                }
            }
            if !next.modules.iter().any(|m| m.id.as_str() == "music") {
                let mut music = Registry::defaults().modules.remove(6);
                next = next.normalized();
                music.order =
                    u32::try_from(next.modules.len()).map_err(|_| RegistryError::TooManyModules)?;
                next.modules.push(music);
            }
            for module in Registry::defaults().modules.into_iter().skip(7) {
                if !next.modules.iter().any(|m| m.id == module.id) {
                    let mut module = module;
                    module.order = u32::try_from(next.modules.len())
                        .map_err(|_| RegistryError::TooManyModules)?;
                    next.modules.push(module);
                }
            }
            next.validate()?;
            Ok(next)
        }
        other => Err(RegistryError::UnsupportedVersion(other)),
    }
}

/// Retains original values for every unmapped/invalid key. The caller must keep
/// the original flat settings; this crate never renames or deletes their keys.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MigrationReport {
    pub mapped_keys: Vec<String>,
    pub untouched: BTreeMap<String, Value>,
    pub invalid_keys: Vec<String>,
    pub input_problem: Option<String>,
}

pub fn migrate_from_flat(value: &Value) -> (Registry, MigrationReport) {
    let mut registry = Registry::defaults();
    let mut report = MigrationReport::default();
    let Some(map) = value.as_object() else {
        report.input_problem =
            Some("expected a flat settings object; defaults retained".to_owned());
        return (registry, report);
    };
    for (key, value) in map {
        let mut next = registry.clone();
        match apply_flat(&mut next, key, value) {
            Some(true) => {
                registry = next;
                report.mapped_keys.push(key.clone());
            }
            result => {
                report.untouched.insert(key.clone(), value.clone());
                if result == Some(false) {
                    report.invalid_keys.push(key.clone());
                }
            }
        }
    }
    (registry, report)
}

// Some(true/false) = mapped valid/invalid; None = unrelated key, untouched.
fn apply_flat(registry: &mut Registry, key: &str, value: &Value) -> Option<bool> {
    let id = match key {
        "hardwareMonitor" => "monitor",
        "terminalFontSize" => "terminal",
        "notificationsEnabled"
        | "systemNotificationMirror"
        | "toastTtlMs"
        | "redactBodies"
        | "dnd" => "notifications",
        "clipboardHistory" | "clipboardHistorySize" | "clipboardPersist" => "clipboard",
        _ => return None,
    };
    let module = registry.modules.iter_mut().find(|m| m.id.as_str() == id)?;
    let valid_type = match (key, &mut module.config) {
        ("hardwareMonitor" | "notificationsEnabled" | "clipboardHistory", _) => {
            assign_bool(&mut module.enabled, value)
        }
        ("terminalFontSize", ModuleConfig::Terminal { font_size }) => {
            assign_number(font_size, value)
        }
        (
            "systemNotificationMirror",
            ModuleConfig::Notifications {
                system_notification_mirror,
                ..
            },
        ) => assign_bool(system_notification_mirror, value),
        ("toastTtlMs", ModuleConfig::Notifications { toast_ttl_ms, .. }) => {
            assign_number(toast_ttl_ms, value)
        }
        ("redactBodies", ModuleConfig::Notifications { redact_bodies, .. }) => {
            assign_bool(redact_bodies, value)
        }
        ("dnd", ModuleConfig::Notifications { dnd, .. }) => assign_bool(dnd, value),
        ("clipboardHistorySize", ModuleConfig::Clipboard { history_size, .. }) => {
            assign_number(history_size, value)
        }
        ("clipboardPersist", ModuleConfig::Clipboard { persist, .. }) => {
            assign_bool(persist, value)
        }
        _ => false,
    };
    Some(valid_type && registry.validate().is_ok())
}

fn assign_bool(target: &mut bool, value: &Value) -> bool {
    if let Some(value) = value.as_bool() {
        *target = value;
        true
    } else {
        false
    }
}
fn assign_number(target: &mut f64, value: &Value) -> bool {
    if let Some(value) = value.as_f64() {
        *target = value;
        true
    } else {
        false
    }
}
