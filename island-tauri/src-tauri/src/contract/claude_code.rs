//! Read-only Claude Code metrics. Message content is reduced to counts before IPC.
use super::ai::JsonValue;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeCodeCursor {
    pub key: String,
    pub ino: String,
    pub off: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeCodeRequest {
    pub cursors: Vec<ClaudeCodeCursor>,
    /// None = 16 MiB, completing the current line before yielding.
    pub budget_bytes: Option<f64>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeRegEntry {
    pub file: String,
    pub mtime_ms: f64,
    pub alive: bool,
    pub proc_start: Option<String>,
    /// Rust-whitelisted registry metadata only; never arbitrary registry JSON or session titles.
    #[specta(type = JsonValue)]
    pub data: serde_json::Value,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeUsage {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub cw1h: f64,
    pub fast: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(tag = "t", rename_all = "camelCase")]
#[allow(clippy::large_enum_variant)]
pub enum ClaudeCodeFact {
    A {
        at: Option<f64>,
        model: String,
        key: String,
        side: bool,
        ep: Option<String>,
        cwd: Option<String>,
        stop: Option<String>,
        tool: Option<String>,
        chars: f64,
        sig: f64,
        u: ClaudeCodeUsage,
    },
    Compact {
        at: Option<f64>,
    },
    TurnEnd {
        at: Option<f64>,
    },
    Sys {
        at: Option<f64>,
    },
    User {
        at: Option<f64>,
        meta: bool,
        interrupt: bool,
    },
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeFile {
    pub key: String,
    pub slug: String,
    pub session_id: String,
    pub sub: bool,
    pub ino: String,
    pub size: f64,
    pub mtime_ms: f64,
    pub reset: bool,
    pub from: f64,
    pub next: f64,
    pub facts: Vec<ClaudeCodeFact>,
    pub skipped_huge: f64,
    pub bad_lines: f64,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeRaw {
    pub now: f64,
    pub claude_dir: Option<String>,
    pub folder_missing: bool,
    pub registry: Vec<ClaudeRegEntry>,
    pub registry_error: Option<String>,
    pub files: Vec<ClaudeFile>,
    pub more: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn wire_names_and_strict_inputs() {
        let req: ClaudeCodeRequest = serde_json::from_value(json!({
            "cursors": [{"key":"p/s.jsonl","ino":"12","off":5}], "budgetBytes":null
        }))
        .unwrap();
        assert_eq!(req.cursors[0].off, 5.0);
        assert!(req.budget_bytes.is_none());
        assert!(serde_json::from_value::<ClaudeCodeRequest>(json!({
            "cursors":[],"budgetBytes":1,"x":1
        }))
        .is_err());
        assert!(serde_json::from_value::<ClaudeCodeRequest>(json!({
            "cursors":[{"key":"p/s.jsonl","ino":"12","off":0,"x":1}],"budgetBytes":null
        }))
        .is_err());
        assert_eq!(
            serde_json::to_value(ClaudeCodeFact::TurnEnd { at: None }).unwrap(),
            json!({"t":"turnEnd","at":null})
        );
        assert_eq!(
            serde_json::to_value(ClaudeCodeFact::User {
                at: None,
                meta: true,
                interrupt: false
            })
            .unwrap(),
            json!({"t":"user","at":null,"meta":true,"interrupt":false})
        );
    }
}
