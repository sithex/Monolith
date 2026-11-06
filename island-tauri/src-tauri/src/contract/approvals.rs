//! Pending Hermes approvals/clarify questions shown in the notch hover notice.
//!
//! The island never decides anything here: it mirrors what the Hermes TUI (running in the
//! island's own terminal) is asking and, only while every guard in
//! `services::approvals` holds, presses the TUI's own quick-pick digit for the chosen option.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalKind {
    /// A dangerous command waiting for once / session / always / deny.
    Approval,
    /// A clarify question with a short list of choices.
    Clarify,
}

/// Look of a choice button. `Option` is a clarify choice.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChoiceStyle {
    Once,
    Session,
    Always,
    Deny,
    Option,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalChoiceView {
    /// What `approval_answer` takes back: `once|session|always|deny`, or `1`..`9` for clarify.
    pub id: String,
    pub label: String,
    pub style: ChoiceStyle,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalPrompt {
    /// The island's id for this prompt; an answer must carry the id it saw.
    pub id: String,
    pub kind: ApprovalKind,
    /// The command (approval) or the question (clarify), control characters replaced.
    pub text: String,
    /// The text was cut at the display limit, so it cannot be reviewed here.
    pub text_truncated: bool,
    pub description: String,
    pub choices: Vec<ApprovalChoiceView>,
    /// Buttons may be pressed right now.
    pub answerable: bool,
    /// An answer was already sent from the island; waiting for Hermes to settle it.
    pub answered: bool,
    /// Why the buttons are off (shown instead of them), when `answerable` is false.
    pub note: Option<String>,
    /// Further requests queued behind this one.
    pub more: u32,
}

/// Latest-value topic: `rev` wraps, consumers keep the newest.
#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalChanged {
    pub rev: u32,
    pub prompt: Option<ApprovalPrompt>,
    /// The terminal title says Hermes waits, but no structured request is known
    /// (a sudo/secret prompt, or a TUI that is not mirroring): read-only "open terminal".
    pub waiting_unknown: bool,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalAnswer {
    pub prompt_id: String,
    pub choice: String,
}
