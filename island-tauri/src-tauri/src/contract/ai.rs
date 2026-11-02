//! AI usage / account connection / shared voice types.

use serde::{Deserialize, Serialize};

/// Allow-list of providers `ai_connect` may start an OAuth flow for. Anything else is rejected
/// by serde before the command runs.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
pub enum AiProvider {
    #[serde(rename = "openai-codex")]
    OpenaiCodex,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "openrouter")]
    Openrouter,
    #[serde(rename = "nous")]
    Nous,
}

impl AiProvider {
    pub const ALL: [AiProvider; 4] = [
        AiProvider::OpenaiCodex,
        AiProvider::Anthropic,
        AiProvider::Openrouter,
        AiProvider::Nous,
    ];

    pub fn id(self) -> &'static str {
        match self {
            AiProvider::OpenaiCodex => "openai-codex",
            AiProvider::Anthropic => "anthropic",
            AiProvider::Openrouter => "openrouter",
            AiProvider::Nous => "nous",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AiProvider::OpenaiCodex => "OpenAI Codex",
            AiProvider::Anthropic => "Anthropic",
            AiProvider::Openrouter => "OpenRouter",
            AiProvider::Nous => "Nous",
        }
    }

    /// Konsole argv (identical to island-rust `auth_args`).
    pub fn auth_args(self) -> Vec<String> {
        let mut args = vec![
            "--separate",
            "-e",
            "hermes",
            "auth",
            "add",
            self.id(),
            "--type",
            "oauth",
        ];
        if self == AiProvider::OpenaiCodex {
            args.push("--browser");
        }
        args.into_iter().map(String::from).collect()
    }
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub label: String,
}

/// TypeScript-side description of arbitrary JSON. `serde_json::Value`'s own specta impl drags in
/// 64-bit integers (forbidden across the IPC), so the two passthrough fields declare this type
/// with `#[specta(type = JsonValue)]`; at runtime they stay `serde_json::Value`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(untagged)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(std::collections::BTreeMap<String, JsonValue>),
}

/// `account` and `local` are provider-specific detail objects passed through from
/// `hermes usage --json` / `island/ai-local-usage.py` (SPEC §5.2: the only untyped part).
/// `account.accounts` contains explicit anthropic and openai-codex usage objects.
/// `local.dashboard` contains 91-day session-start cohorts, hourly cohorts, period
/// shares and recency-fenced active sessions. These are cumulative session costs,
/// not a per-call ledger; its `periodBasis` and `note` must remain visible in UI.
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AiUsage {
    #[specta(type = JsonValue)]
    pub account: serde_json::Value,
    #[specta(type = JsonValue)]
    pub local: serde_json::Value,
    pub providers: Vec<ProviderInfo>,
    pub fetched_at: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AiConnectResult {
    pub launched: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional, type = AiProvider)]
    pub provider: Option<AiProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional, type = String)]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional, type = String)]
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceOp {
    Status,
    Toggle,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceState {
    Muted,
    Unmuted,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    pub state: VoiceState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn connect_result_omits_absent_keys_like_the_legacy_payload() {
        let failed = AiConnectResult {
            launched: false,
            provider: None,
            message: None,
            error: Some("Unsupported provider.".into()),
        };
        assert_eq!(
            serde_json::to_value(failed).unwrap(),
            json!({"launched": false, "error": "Unsupported provider."})
        );
        let ok = AiConnectResult {
            launched: true,
            provider: Some(AiProvider::Nous),
            message: Some("m".into()),
            error: None,
        };
        assert_eq!(
            serde_json::to_value(ok).unwrap(),
            json!({"launched": true, "provider": "nous", "message": "m"})
        );
    }
}
