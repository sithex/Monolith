//! The brightness display model: tracks displays announced by PowerDevil
//! (org.kde.ScreenBrightness), converts events into view state, and
//! coalesces slider drags into throttled `SetBrightnessWithContext`
//! requests (`SetRequest`).

use crate::integrations::brightness::core::error::DragError;
use crate::integrations::brightness::core::name::{validate_display_name, OBJECT_PATH_PREFIX};
use crate::integrations::brightness::core::percent::{percent_of, raw_for_percent_of, MAX_PERCENT};
use crate::integrations::brightness::core::{CONTEXT, SUPPRESS_INDICATOR};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Maximum number of displays tracked; extra names are dropped and counted.
pub const MAX_DISPLAYS: usize = 16;
/// Minimum interval between two poll-emitted requests for the same display.
pub const THROTTLE_MS: u64 = 50;
/// A released drag keeps showing its target for this long after release.
pub const SETTLE_MS: u64 = 500;

/// Outcome of applying an [`Event`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    /// Whether any tracked state changed (added/removed/reordered or any
    /// stored field of a display changed).
    pub changed: bool,
    /// Names that are new and have no Props yet; each appears at most once
    /// until its Props arrive or it is removed.
    pub fetch: Vec<String>,
}

/// A display as shown to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayView {
    pub name: String,
    pub label: String,
    pub is_internal: bool,
    /// Percent to display: the drag target while dragging (and for
    /// `SETTLE_MS` after release), otherwise the model value. `None` when
    /// the display is unavailable (`max <= 0`).
    pub percent: Option<u8>,
    pub dragging: bool,
}

/// A `SetBrightnessWithContext` method-call request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRequest {
    pub name: String,
    pub path: String,
    pub brightness: i32,
    pub flags: u32,
    pub context: String,
}

impl SetRequest {
    /// `name` is always validated before a request is built.
    fn new(name: &str, brightness: i32) -> Self {
        Self {
            name: name.to_string(),
            path: format!("{OBJECT_PATH_PREFIX}{name}"),
            brightness,
            flags: SUPPRESS_INDICATOR,
            context: CONTEXT.to_string(),
        }
    }
}

/// An event from the Tauri side (decoded from D-Bus).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    /// The full `DisplaysDBusNames` property.
    DisplayNames(Vec<String>),
    DisplayAdded(String),
    DisplayRemoved(String),
    /// The child interface's GetAll result.
    Props {
        name: String,
        label: String,
        is_internal: bool,
        max_brightness: i32,
        brightness: i32,
    },
    BrightnessChanged {
        name: String,
        brightness: i32,
        source_client: String,
        context: String,
    },
    RangeChanged {
        name: String,
        max_brightness: i32,
        brightness: i32,
    },
}

#[derive(Debug)]
struct Drag {
    target_pct: u8,
    last_sent: Option<i32>,
    last_send_ms: Option<u64>,
}

#[derive(Debug)]
struct Entry {
    name: String,
    label: String,
    is_internal: bool,
    max_brightness: i32,
    brightness: i32,
    drag: Option<Drag>,
    released_at: Option<u64>,
    settle_target: Option<u8>,
}

impl Entry {
    fn new(name: String) -> Self {
        Self {
            name,
            label: String::new(),
            is_internal: false,
            max_brightness: 0,
            brightness: 0,
            drag: None,
            released_at: None,
            settle_target: None,
        }
    }
}

/// Store a brightness value, clamping to `0..=max` when `max > 0`
/// (returning whether it was clamped); when `max <= 0` the display is
/// unavailable and the raw value is kept unclamped. Sets `*changed` when
/// the stored value differs.
fn store_brightness(e: &mut Entry, max: i32, brightness: i32, changed: &mut bool) -> bool {
    if max > 0 {
        let was_clamped = !(0..=max).contains(&brightness);
        let clamped = brightness.clamp(0, max);
        if e.brightness != clamped {
            e.brightness = clamped;
            *changed = true;
        }
        was_clamped
    } else {
        if e.brightness != brightness {
            e.brightness = brightness;
            *changed = true;
        }
        false
    }
}

/// The brightness display model. Single-threaded, no IO, never panics.
#[derive(Debug, Default)]
pub struct BrightnessModel {
    entries: Vec<Entry>,
    ignored_count: u32,
    clamped_count: u32,
    last_now: u64,
}

impl BrightnessModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cumulative count of ignored/unknown/invalid names (signals for
    /// unknown or invalid names, invalid `DisplayNames` entries, and names
    /// beyond `MAX_DISPLAYS`).
    pub fn ignored(&self) -> u32 {
        self.ignored_count
    }

    /// Cumulative count of brightness values stored clamped (outside
    /// `0..=max`).
    pub fn clamped(&self) -> u32 {
        self.clamped_count
    }

    /// Current max brightness for a display (for inspection/assertions).
    pub fn max_brightness(&self, name: &str) -> Option<i32> {
        self.entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.max_brightness)
    }

    /// Apply one decoded D-Bus event.
    pub fn apply(&mut self, event: Event) -> Applied {
        let mut changed = false;
        let mut fetch = Vec::new();
        match event {
            Event::DisplayNames(names) => {
                let original_names: Vec<String> =
                    self.entries.iter().map(|e| e.name.clone()).collect();
                let mut seen = HashSet::new();
                let mut next = Vec::new();
                for name in names {
                    if validate_display_name(&name).is_err() {
                        self.ignored_count = self.ignored_count.saturating_add(1);
                        continue;
                    }
                    if next.len() >= MAX_DISPLAYS {
                        self.ignored_count = self.ignored_count.saturating_add(1);
                        continue;
                    }
                    if !seen.insert(name.clone()) {
                        continue;
                    }
                    match self.entries.iter().position(|e| e.name == name) {
                        Some(pos) => next.push(self.entries.remove(pos)),
                        None => {
                            next.push(Entry::new(name.clone()));
                            fetch.push(name);
                            changed = true;
                        }
                    }
                }
                if original_names.len() != next.len() {
                    changed = true;
                } else {
                    for (old, new) in original_names.iter().zip(next.iter()) {
                        if *old != new.name {
                            changed = true;
                            break;
                        }
                    }
                }
                self.entries = next;
            }
            Event::DisplayAdded(name) => {
                if validate_display_name(&name).is_err() {
                    self.ignored_count = self.ignored_count.saturating_add(1);
                } else if self.entries.iter().any(|e| e.name == name) {
                    // already present: no-op
                } else if self.entries.len() >= MAX_DISPLAYS {
                    self.ignored_count = self.ignored_count.saturating_add(1);
                } else {
                    self.entries.push(Entry::new(name.clone()));
                    fetch.push(name);
                    changed = true;
                }
            }
            Event::DisplayRemoved(name) => {
                if validate_display_name(&name).is_err() {
                    self.ignored_count = self.ignored_count.saturating_add(1);
                } else if let Some(pos) = self.entries.iter().position(|e| e.name == name) {
                    self.entries.remove(pos);
                    changed = true;
                } else {
                    self.ignored_count = self.ignored_count.saturating_add(1);
                }
            }
            Event::Props {
                name,
                label,
                is_internal,
                max_brightness,
                brightness,
            } => {
                if validate_display_name(&name).is_err() {
                    self.ignored_count = self.ignored_count.saturating_add(1);
                } else if let Some(e) = self.entries.iter_mut().find(|e| e.name == name) {
                    changed |= e.label != label || e.is_internal != is_internal;
                    e.label = label;
                    e.is_internal = is_internal;
                    if e.max_brightness != max_brightness {
                        e.max_brightness = max_brightness;
                        changed = true;
                    }
                    if store_brightness(e, max_brightness, brightness, &mut changed) {
                        self.clamped_count = self.clamped_count.saturating_add(1);
                    }
                } else {
                    self.ignored_count = self.ignored_count.saturating_add(1);
                }
            }
            Event::BrightnessChanged {
                name, brightness, ..
            } => {
                if validate_display_name(&name).is_err() {
                    self.ignored_count = self.ignored_count.saturating_add(1);
                } else if let Some(e) = self.entries.iter_mut().find(|e| e.name == name) {
                    // Stored regardless of drag state (visible after settle);
                    // the view hides echoes during drag/settle.
                    if store_brightness(e, e.max_brightness, brightness, &mut changed) {
                        self.clamped_count = self.clamped_count.saturating_add(1);
                    }
                } else {
                    self.ignored_count = self.ignored_count.saturating_add(1);
                }
            }
            Event::RangeChanged {
                name,
                max_brightness,
                brightness,
            } => {
                if validate_display_name(&name).is_err() {
                    self.ignored_count = self.ignored_count.saturating_add(1);
                } else if let Some(e) = self.entries.iter_mut().find(|e| e.name == name) {
                    if e.max_brightness != max_brightness {
                        e.max_brightness = max_brightness;
                        changed = true;
                    }
                    if store_brightness(e, max_brightness, brightness, &mut changed) {
                        self.clamped_count = self.clamped_count.saturating_add(1);
                    }
                } else {
                    self.ignored_count = self.ignored_count.saturating_add(1);
                }
            }
        }
        Applied { changed, fetch }
    }

    /// The displays in `DisplaysDBusNames` order.
    pub fn displays(&self) -> Vec<DisplayView> {
        self.entries
            .iter()
            .map(|e| DisplayView {
                name: e.name.clone(),
                label: e.label.clone(),
                is_internal: e.is_internal,
                percent: self.view_percent(e),
                dragging: e.drag.is_some(),
            })
            .collect()
    }

    fn view_percent(&self, e: &Entry) -> Option<u8> {
        if e.max_brightness <= 0 {
            return None;
        }
        if let Some(drag) = &e.drag {
            return Some(drag.target_pct);
        }
        if let Some(at) = e.released_at {
            if self.last_now.saturating_sub(at) < SETTLE_MS {
                return e.settle_target;
            }
        }
        percent_of(e.max_brightness, e.brightness)
    }

    /// Start or update a drag session for a display. Unknown or unavailable
    /// displays, and percent > 100, are errors.
    pub fn drag(&mut self, name: &str, pct: u8, now_ms: u64) -> Result<(), DragError> {
        self.last_now = now_ms;
        let e = self
            .entries
            .iter_mut()
            .find(|e| e.name == name)
            .ok_or(DragError::UnknownDisplay)?;
        if e.max_brightness <= 0 {
            return Err(DragError::UnavailableDisplay);
        }
        if pct > MAX_PERCENT {
            return Err(DragError::PercentOutOfRange);
        }
        if let Some(drag) = &mut e.drag {
            drag.target_pct = pct;
        } else {
            e.drag = Some(Drag {
                target_pct: pct,
                last_sent: None,
                last_send_ms: None,
            });
        }
        e.released_at = None;
        e.settle_target = None;
        Ok(())
    }

    /// Emit at most one request per display (the latest unsent target),
    /// throttled to one per `THROTTLE_MS`.
    pub fn poll(&mut self, now_ms: u64) -> Vec<SetRequest> {
        self.last_now = now_ms;
        let mut out = Vec::new();
        for e in self.entries.iter_mut() {
            let drag = match &mut e.drag {
                Some(d) => d,
                None => continue,
            };
            if e.max_brightness <= 0 {
                continue;
            }
            let raw = match raw_for_percent_of(e.max_brightness, drag.target_pct) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if Some(raw) == drag.last_sent {
                continue;
            }
            if let Some(t) = drag.last_send_ms {
                if now_ms.saturating_sub(t) < THROTTLE_MS {
                    continue;
                }
            }
            drag.last_sent = Some(raw);
            drag.last_send_ms = Some(now_ms);
            out.push(SetRequest::new(&e.name, raw));
        }
        out
    }

    /// End a drag, flushing the pending (unsent) target immediately. `None`
    /// when the display is unknown, not dragging, or nothing is unsent.
    pub fn release(&mut self, name: &str, now_ms: u64) -> Option<SetRequest> {
        self.last_now = now_ms;
        let e = self.entries.iter_mut().find(|e| e.name == name)?;
        let drag = e.drag.take()?;
        let request = if e.max_brightness > 0 {
            match raw_for_percent_of(e.max_brightness, drag.target_pct) {
                Ok(raw) if Some(raw) != drag.last_sent => Some(SetRequest::new(&e.name, raw)),
                _ => None,
            }
        } else {
            None
        };
        e.released_at = Some(now_ms);
        e.settle_target = Some(drag.target_pct);
        request
    }

    /// Earliest ms at which `poll`/settle has work: the next throttle
    /// release for an unsent target, or the settle expiry of a released
    /// drag (UI refresh). `None` when idle.
    pub fn next_wakeup(&self, now_ms: u64) -> Option<u64> {
        let mut best: Option<u64> = None;
        for e in &self.entries {
            let candidate = if let Some(drag) = &e.drag {
                if e.max_brightness <= 0 {
                    None
                } else {
                    match raw_for_percent_of(e.max_brightness, drag.target_pct) {
                        Ok(raw) if Some(raw) != drag.last_sent => match drag.last_send_ms {
                            None => Some(now_ms),
                            Some(t) if now_ms.saturating_sub(t) >= THROTTLE_MS => Some(now_ms),
                            Some(t) => Some(t.saturating_add(THROTTLE_MS)),
                        },
                        _ => None,
                    }
                }
            } else if let Some(at) = e.released_at {
                if now_ms.saturating_sub(at) < SETTLE_MS {
                    Some(at.saturating_add(SETTLE_MS))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(c) = candidate {
                best = Some(match best {
                    Some(b) => b.min(c),
                    None => c,
                });
            }
        }
        best
    }

    /// A failed platform write must not remain optimistically displayed or suppress the
    /// next drag's identical target. Confirmed brightness remains untouched.
    pub fn write_failed(&mut self, name: &str) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.name == name) {
            e.drag = None;
            e.released_at = None;
            e.settle_target = None;
        }
    }
}
