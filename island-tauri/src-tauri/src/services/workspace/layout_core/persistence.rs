use super::error::LayoutError;
use super::model::{
    valid_id, valid_label, ButtonAction, Layout, LoadReport, Side, SideButton, SizePreset,
    DEFAULT_BUTTON_DIAMETER, DEFAULT_GAP, MAX_BUTTON_DIAMETER, MAX_GAP, MIN_BUTTON_DIAMETER,
    MIN_GAP, VERSION,
};
use serde_json::Value;
use std::collections::HashSet;

fn parse_side(v: &Value) -> Option<Side> {
    serde_json::from_value(v.clone()).ok()
}

fn parse_slot(v: &Value) -> Option<u8> {
    v.as_u64().and_then(|n| u8::try_from(n).ok())
}

fn parse_action(v: &Value) -> Option<ButtonAction> {
    let action: ButtonAction = serde_json::from_value(v.clone()).ok()?;
    if action.is_valid() {
        Some(action)
    } else {
        None
    }
}

fn parse_preset(v: &Value) -> Option<SizePreset> {
    let preset: SizePreset = serde_json::from_value(v.clone()).ok()?;
    if preset.is_valid() {
        Some(preset)
    } else {
        None
    }
}

fn parse_finite_in_range(v: &Value, lo: f64, hi: f64) -> Option<f64> {
    let n = v.as_f64()?;
    if n.is_finite() && n >= lo && n <= hi {
        Some(n)
    } else {
        None
    }
}

impl Layout {
    /// Serialize to the persistence format. Field names are camelCase.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|_| unreachable!("serializing a plain struct cannot fail"))
    }

    /// Parse the persistence format. Fatal typed errors: not valid JSON,
    /// top level not an object, or `version` missing / not 1. Per-button
    /// problems are tolerated (skipped and counted, first valid occurrence
    /// wins); invalid preset / diameter / gap fall back to defaults and are
    /// flagged. Unknown fields are ignored.
    pub fn from_json(input: &str) -> Result<(Layout, LoadReport), LayoutError> {
        let value: Value = serde_json::from_str(input).map_err(|_| LayoutError::InvalidJson)?;
        let obj = value.as_object().ok_or(LayoutError::NotAnObject)?;

        match obj.get("version").and_then(|v| v.as_u64()) {
            Some(1) => {}
            _ => return Err(LayoutError::InvalidVersion),
        }

        let mut report = LoadReport::default();

        let preset = match obj.get("preset") {
            None => SizePreset::Compact,
            Some(v) => match parse_preset(v) {
                Some(p) => p,
                None => {
                    report.preset_fallback = true;
                    SizePreset::Compact
                }
            },
        };

        let button_diameter = match obj.get("buttonDiameter") {
            None => DEFAULT_BUTTON_DIAMETER,
            Some(v) => match parse_finite_in_range(v, MIN_BUTTON_DIAMETER, MAX_BUTTON_DIAMETER) {
                Some(d) => d,
                None => {
                    report.diameter_fallback = true;
                    DEFAULT_BUTTON_DIAMETER
                }
            },
        };

        let gap = match obj.get("gap") {
            None => DEFAULT_GAP,
            Some(v) => match parse_finite_in_range(v, MIN_GAP, MAX_GAP) {
                Some(g) => g,
                None => {
                    report.gap_fallback = true;
                    DEFAULT_GAP
                }
            },
        };

        let mut buttons: Vec<SideButton> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut occupied: HashSet<(Side, u8)> = HashSet::new();

        match obj.get("buttons") {
            None | Some(Value::Null) => {}
            Some(Value::Array(array)) => {
                for item in array {
                    let item_obj = match item.as_object() {
                        Some(o) => o,
                        None => {
                            report.skipped += 1;
                            continue;
                        }
                    };
                    let id = match item_obj.get("id").and_then(|v| v.as_str()) {
                        Some(id) if valid_id(id) => id,
                        _ => {
                            report.skipped += 1;
                            continue;
                        }
                    };
                    let side = match item_obj.get("side").and_then(parse_side) {
                        Some(s) => s,
                        None => {
                            report.skipped += 1;
                            continue;
                        }
                    };
                    let slot = match item_obj.get("slot").and_then(parse_slot) {
                        Some(s) => s,
                        None => {
                            report.skipped += 1;
                            continue;
                        }
                    };
                    if slot >= side.capacity() {
                        report.skipped += 1;
                        continue;
                    }
                    if seen_ids.contains(id) {
                        report.skipped += 1;
                        continue;
                    }
                    if occupied.contains(&(side, slot)) {
                        report.skipped += 1;
                        continue;
                    }
                    let action = match item_obj.get("action").and_then(parse_action) {
                        Some(a) => a,
                        None => {
                            report.skipped += 1;
                            continue;
                        }
                    };
                    let label = match item_obj.get("label") {
                        None | Some(Value::Null) => None,
                        Some(Value::String(s)) if valid_label(s) => Some(s.clone()),
                        _ => {
                            report.skipped += 1;
                            continue;
                        }
                    };
                    seen_ids.insert(id.to_string());
                    occupied.insert((side, slot));
                    buttons.push(SideButton {
                        id: id.to_string(),
                        side,
                        slot,
                        action,
                        label,
                    });
                }
            }
            Some(_) => {
                // Present but not an array: tolerate as zero buttons.
                report.skipped += 1;
            }
        }

        Ok((
            Layout {
                version: VERSION,
                preset,
                button_diameter,
                gap,
                buttons,
            },
            report,
        ))
    }
}
