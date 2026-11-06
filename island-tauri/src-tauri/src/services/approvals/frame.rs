//! Parsing of the Hermes TUI's mirrored gateway event frames.
//!
//! Only `approval.request`, `clarify.request`, their `.settled` partners and `request.cancel`
//! are understood; every other frame (the TUI mirrors *all* events, including streamed
//! tokens) is dropped without being parsed beyond a substring check. The gateway already
//! redacts command/question text; it is shown verbatim after control characters are replaced.

use crate::contract::approvals::{ApprovalKind, ChoiceStyle};
use serde_json::Value;

/// Frames longer than this are never parsed (they cannot be a prompt).
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// Longest command/question text kept; past it the prompt is shown read-only.
pub const MAX_TEXT_CHARS: usize = 6000;
const MAX_DESCRIPTION_CHARS: usize = 300;
const MAX_LABEL_CHARS: usize = 200;
const MAX_ID_CHARS: usize = 128;
/// Digits `1`..`9` are single-key quick picks; clarify lists longer than this stay read-only.
pub const MAX_CLARIFY_CHOICES: usize = 9;

/// The TUI's fixed approval order (`APPROVAL_OPTS` in `components/prompts.tsx`).
const APPROVAL_ORDER: [&str; 4] = ["once", "session", "always", "deny"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub style: ChoiceStyle,
    /// The key the TUI maps to this option (`1`..`9`).
    pub digit: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub kind: ApprovalKind,
    /// The gateway's id for the prompt (`prompt_id`).
    pub prompt_id: String,
    pub session_id: String,
    pub text: String,
    pub truncated: bool,
    pub description: String,
    pub choices: Vec<Choice>,
    /// Set when the request can be shown but never answered from the island.
    pub read_only: Option<&'static str>,
    /// Gateway timeout in seconds, when it gave one.
    pub timeout_s: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Incoming {
    Request(Box<Request>),
    Settled { prompt_id: String },
    Cancel { id: String },
}

/// Replace control characters (except newline and tab) and bidirectional overrides with
/// U+FFFD, so what is displayed is what the terminal would show and nothing reorders it.
pub fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| {
            let bidi = matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}');
            if (c.is_control() && c != '\n' && c != '\t') || bidi {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

fn cap(text: &str, max: usize) -> (String, bool) {
    let clean = sanitize(text);
    if clean.chars().count() <= max {
        return (clean, false);
    }
    (clean.chars().take(max).collect(), true)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().count() <= MAX_ID_CHARS && !id.chars().any(char::is_control)
}

fn approval_label(choice: &str) -> (&'static str, ChoiceStyle) {
    match choice {
        "once" => ("Approve", ChoiceStyle::Once),
        "session" => ("Session", ChoiceStyle::Session),
        "always" => ("Always", ChoiceStyle::Always),
        _ => ("Deny", ChoiceStyle::Deny),
    }
}

/// `approvalOptions` of the TUI: the gateway's `choices` filter the fixed order; without them
/// `smart_denied` leaves once/deny and an explicit `allow_permanent: false` drops "always".
fn approval_options(payload: &Value) -> Vec<&'static str> {
    if let Some(list) = payload.get("choices").filter(|v| !v.is_null()) {
        let listed: Vec<&str> = list
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        return APPROVAL_ORDER
            .iter()
            .copied()
            .filter(|c| listed.contains(c))
            .collect();
    }
    if payload.get("smart_denied").and_then(Value::as_bool) == Some(true) {
        return vec!["once", "deny"];
    }
    if payload.get("allow_permanent").and_then(Value::as_bool) == Some(false) {
        return vec!["once", "session", "deny"];
    }
    APPROVAL_ORDER.to_vec()
}

fn parse_approval(prompt_id: String, session_id: String, payload: &Value) -> Request {
    let command = payload.get("command").and_then(Value::as_str).unwrap_or("");
    let (text, truncated) = cap(command, MAX_TEXT_CHARS);
    let description = payload
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("dangerous command");
    let options = approval_options(payload);
    let choices: Vec<Choice> = options
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let (label, style) = approval_label(id);
            Choice {
                id: (*id).to_owned(),
                label: label.to_owned(),
                style,
                digit: u8::try_from(i + 1).unwrap_or(9),
            }
        })
        .collect();
    let read_only = if choices.is_empty() {
        Some("No choices to offer here")
    } else if truncated {
        Some("Command too long to review here")
    } else if text.trim().is_empty() {
        Some("No command text to review")
    } else {
        None
    };
    Request {
        kind: ApprovalKind::Approval,
        prompt_id,
        session_id,
        text,
        truncated,
        description: cap(description, MAX_DESCRIPTION_CHARS).0,
        choices,
        read_only,
        timeout_s: timeout(payload),
    }
}

fn parse_clarify(prompt_id: String, session_id: String, payload: &Value) -> Option<Request> {
    let batch = payload
        .get("questions")
        .and_then(Value::as_array)
        .is_some_and(|q| !q.is_empty());
    let single = payload
        .get("question")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    let first_batch_question = payload
        .get("questions")
        .and_then(Value::as_array)
        .and_then(|q| {
            q.iter()
                .filter_map(|v| v.get("question").and_then(Value::as_str))
                .map(str::trim)
                .find(|s| !s.is_empty())
        });
    // The TUI opens no card for a clarify without any usable question; neither do we.
    let question = if batch { first_batch_question? } else { single };
    if question.is_empty() {
        return None;
    }
    let (text, truncated) = cap(question, MAX_TEXT_CHARS);
    let raw_choices = payload.get("choices").and_then(Value::as_array);
    let all_strings = raw_choices.is_some_and(|c| c.iter().all(Value::is_string));
    let choices: Vec<Choice> = match raw_choices {
        Some(list) if all_strings => list
            .iter()
            .filter_map(Value::as_str)
            .take(MAX_CLARIFY_CHOICES)
            .enumerate()
            .map(|(i, label)| {
                let digit = u8::try_from(i + 1).unwrap_or(9);
                Choice {
                    id: digit.to_string(),
                    label: cap(label, MAX_LABEL_CHARS).0,
                    style: ChoiceStyle::Option,
                    digit,
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    let read_only = if batch {
        Some("Several questions: answer in the terminal")
    } else if payload.get("multi_select").and_then(Value::as_bool) == Some(true) {
        Some("Multiple answers allowed: answer in the terminal")
    } else if raw_choices.is_none_or(|c| c.is_empty()) {
        Some("Type your answer in the terminal")
    } else if !all_strings {
        Some("Unsupported choices: answer in the terminal")
    } else if raw_choices.is_some_and(|c| c.len() > MAX_CLARIFY_CHOICES) {
        Some("Too many choices: answer in the terminal")
    } else if truncated {
        Some("Question too long to review here")
    } else {
        None
    };
    Some(Request {
        kind: ApprovalKind::Clarify,
        prompt_id,
        session_id,
        text,
        truncated,
        description: String::new(),
        choices,
        read_only,
        timeout_s: timeout(payload),
    })
}

fn timeout(payload: &Value) -> Option<u32> {
    let seconds = payload.get("timeout_s").and_then(Value::as_f64)?;
    (seconds.is_finite() && seconds > 0.0 && seconds < 86_400.0).then_some(seconds as u32)
}

/// Parse one mirrored text frame. `None` = not ours, malformed or something the TUI itself
/// would ignore.
pub fn parse(frame: &str) -> Option<Incoming> {
    if frame.len() > MAX_FRAME_BYTES {
        return None;
    }
    // The TUI mirrors every event; skip the JSON parse unless the frame can be one of ours.
    if !(frame.contains("approval.")
        || frame.contains("clarify.")
        || frame.contains("request.cancel"))
    {
        return None;
    }
    let value: Value = serde_json::from_str(frame).ok()?;
    if !value.is_object() || value.get("method").and_then(Value::as_str) != Some("event") {
        return None;
    }
    let params = value.get("params").filter(|p| p.is_object())?;
    let kind = params.get("type").and_then(Value::as_str)?;
    let payload = params.get("payload").filter(|p| p.is_object());
    match kind {
        "approval.request" | "clarify.request" => {
            let payload = payload?;
            let prompt_id = payload.get("prompt_id").and_then(Value::as_str)?;
            let session_id = params.get("session_id").and_then(Value::as_str)?;
            // The TUI ignores a prompt without a numeric generation or session; so do we.
            payload
                .get("execution_generation")
                .and_then(Value::as_f64)?;
            if !valid_id(prompt_id) || !valid_id(session_id) {
                return None;
            }
            let (prompt_id, session_id) = (prompt_id.to_owned(), session_id.to_owned());
            let request = if kind == "approval.request" {
                parse_approval(prompt_id, session_id, payload)
            } else {
                parse_clarify(prompt_id, session_id, payload)?
            };
            Some(Incoming::Request(Box::new(request)))
        }
        "approval.settled" | "clarify.settled" => {
            let id = payload?.get("prompt_id").and_then(Value::as_str)?;
            valid_id(id).then(|| Incoming::Settled {
                prompt_id: id.to_owned(),
            })
        }
        "request.cancel" => {
            let id = payload?.get("id").and_then(Value::as_str)?;
            valid_id(id).then(|| Incoming::Cancel { id: id.to_owned() })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn event(kind: &str, payload: Value) -> String {
        json!({"jsonrpc":"2.0","method":"event","params":{"type":kind,"session_id":"s1","payload":payload}})
            .to_string()
    }

    fn approval(payload: Value) -> Request {
        let mut base = json!({"prompt_id":"p1","execution_generation":3,"command":"rm -rf build","description":"recursive delete"});
        for (k, v) in payload.as_object().cloned().unwrap_or_default() {
            base[k] = v;
        }
        match parse(&event("approval.request", base)) {
            Some(Incoming::Request(r)) => *r,
            other => panic!("not a request: {other:?}"),
        }
    }

    fn ids(r: &Request) -> Vec<String> {
        r.choices.iter().map(|c| c.id.clone()).collect()
    }

    #[test]
    fn approval_digits_follow_the_tui_order_for_every_choice_set() {
        let all = approval(json!({}));
        assert_eq!(ids(&all), ["once", "session", "always", "deny"]);
        assert_eq!(
            all.choices.iter().map(|c| c.digit).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        // The gateway's list filters; the order stays fixed whatever order it lists them in.
        let listed = approval(json!({"choices":["deny","once","session","always"]}));
        assert_eq!(ids(&listed), ["once", "session", "always", "deny"]);
        let two = approval(json!({"choices":["deny","once"]}));
        assert_eq!(ids(&two), ["once", "deny"]);
        assert_eq!(two.choices[1].digit, 2);
        let smart = approval(json!({"smart_denied":true}));
        assert_eq!(ids(&smart), ["once", "deny"]);
        let no_always = approval(json!({"allow_permanent":false}));
        assert_eq!(ids(&no_always), ["once", "session", "deny"]);
        assert_eq!(no_always.choices[2].digit, 3);
        // `choices` wins over the flags; unknown names are dropped like the TUI's filter.
        let wins = approval(
            json!({"choices":["once","bogus","deny"],"smart_denied":false,"allow_permanent":false}),
        );
        assert_eq!(ids(&wins), ["once", "deny"]);
        assert!(all.read_only.is_none());
    }

    #[test]
    fn approval_without_usable_choices_or_text_is_read_only() {
        assert!(approval(json!({"choices":[]})).read_only.is_some());
        assert!(approval(json!({"choices":["bogus"]})).read_only.is_some());
        assert!(approval(json!({"command":"   "})).read_only.is_some());
        let long = approval(json!({"command":"x".repeat(MAX_TEXT_CHARS + 1)}));
        assert!(long.truncated && long.read_only.is_some());
        assert_eq!(long.text.chars().count(), MAX_TEXT_CHARS);
        let exact = approval(json!({"command":"x".repeat(MAX_TEXT_CHARS)}));
        assert!(!exact.truncated && exact.read_only.is_none());
    }

    #[test]
    fn text_is_sanitized_but_keeps_newlines_and_tabs() {
        let r = approval(
            json!({"command":"echo a\n\tb\u{1b}[31mred\u{7}\u{202E}evil","description":"x\u{0}y"}),
        );
        assert_eq!(r.text, "echo a\n\tb\u{FFFD}[31mred\u{FFFD}\u{FFFD}evil");
        assert_eq!(r.description, "x\u{FFFD}y");
    }

    #[test]
    fn missing_generation_session_or_prompt_id_is_ignored_like_the_tui_does() {
        let ok = json!({"prompt_id":"p","execution_generation":1,"command":"ls"});
        assert!(parse(&event("approval.request", ok.clone())).is_some());
        for key in ["prompt_id", "execution_generation"] {
            let mut p = ok.clone();
            p.as_object_mut().map(|o| o.remove(key));
            assert!(parse(&event("approval.request", p)).is_none(), "{key}");
        }
        let no_session = json!({"jsonrpc":"2.0","method":"event","params":{"type":"approval.request","payload":ok}});
        assert!(parse(&no_session.to_string()).is_none());
        let mut bad = ok;
        bad["prompt_id"] = json!("a\u{1}b");
        assert!(parse(&event("approval.request", bad)).is_none());
    }

    fn clarify(payload: Value) -> Option<Request> {
        let mut base = json!({"prompt_id":"c1","execution_generation":2});
        for (k, v) in payload.as_object().cloned().unwrap_or_default() {
            base[k] = v;
        }
        match parse(&event("clarify.request", base)) {
            Some(Incoming::Request(r)) => Some(*r),
            _ => None,
        }
    }

    #[test]
    fn clarify_single_select_choices_are_digits() {
        let r = clarify(json!({"question":"  Which branch? ","choices":["main","dev"]})).unwrap();
        assert_eq!(r.kind, ApprovalKind::Clarify);
        assert_eq!(r.text, "Which branch?");
        assert_eq!(ids(&r), ["1", "2"]);
        assert_eq!(r.choices[1].digit, 2);
        assert!(r.read_only.is_none());
    }

    #[test]
    fn clarify_shapes_that_digits_cannot_answer_stay_read_only() {
        assert!(clarify(json!({"question":""})).is_none());
        assert!(clarify(json!({})).is_none());
        for payload in [
            json!({"question":"q"}),
            json!({"question":"q","choices":null}),
            json!({"question":"q","choices":[]}),
            json!({"question":"q","choices":["a","b"],"multi_select":true}),
            json!({"question":"q","choices":["a",3]}),
            json!({"question":"q","choices":(0..10).map(|i| i.to_string()).collect::<Vec<_>>()}),
            json!({"question":"q","choices":["a"],"questions":[{"qid":"x","question":"one"}]}),
            json!({"question":"q","choices":["a"],"questions":[{"qid":"x","question":"one"},{"qid":"y","question":"two"}]}),
        ] {
            let r = clarify(payload.clone()).unwrap();
            assert!(r.read_only.is_some(), "{payload}");
        }
        let nine = clarify(
            json!({"question":"q","choices":(0..9).map(|i| i.to_string()).collect::<Vec<_>>()}),
        )
        .unwrap();
        assert!(nine.read_only.is_none());
        assert_eq!(nine.choices.len(), 9);
    }

    #[test]
    fn settled_and_cancel_carry_ids() {
        assert_eq!(
            parse(&event(
                "approval.settled",
                json!({"prompt_id":"p1","execution_generation":3})
            )),
            Some(Incoming::Settled {
                prompt_id: "p1".into()
            })
        );
        assert_eq!(
            parse(&event("clarify.settled", json!({"prompt_id":"c1"}))),
            Some(Incoming::Settled {
                prompt_id: "c1".into()
            })
        );
        assert_eq!(
            parse(&event(
                "request.cancel",
                json!({"id":"p1","method":"approval"})
            )),
            Some(Incoming::Cancel { id: "p1".into() })
        );
        assert!(parse(&event("request.cancel", json!({}))).is_none());
    }

    #[test]
    fn everything_else_is_dropped() {
        for text in [
            event("message.delta", json!({"text":"approval.request in prose"})),
            event("session.info", json!({})),
            event("sudo.request", json!({"prompt_id":"s"})),
            event("secret.request", json!({"prompt_id":"s"})),
            "not json approval.".to_owned(),
            "[]".to_owned(),
            json!(["approval.request"]).to_string(),
            json!({"method":"request","params":{"type":"approval.request"}}).to_string(),
            json!({"method":"event","params":[]}).to_string(),
            json!({"method":"event","params":{"type":"approval.request","session_id":"s","payload":[1]}}).to_string(),
            json!({"method":"event","params":{"type":"approval.request","session_id":"s","payload":null}}).to_string(),
        ] {
            assert!(parse(&text).is_none(), "{text}");
        }
        let huge = event(
            "approval.request",
            json!({"prompt_id":"p","execution_generation":1,"command":"x".repeat(MAX_FRAME_BYTES)}),
        );
        assert!(parse(&huge).is_none());
    }
}
