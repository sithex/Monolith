//! Terminal transport types (SPEC §3.4, §5.2).
//!
//! Attach sends Snapshot before newer Data and returns the same snapshot for legacy callers.
//! The adapter deduplicates that return value (SPEC §3.4).

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSnapshot {
    pub pid: Option<u32>,
    pub running: bool,
    /// Replay buffer (at most 1 MiB, UTF-8 safe).
    pub data: String,
    /// Sequence number of the newest chunk contained in `data` (wrapping `u32`).
    pub seq: u32,
    /// Omitted (like the legacy renderer's `undefined`) until the process exited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional, type = i32)]
    pub exit_code: Option<i32>,
}

/// Output stream of one attached webview. `Data` chunks are ≤ 64 KiB and must be acknowledged
/// with `terminal_ack(seq)` once the terminal emulator has parsed them; `Exit` is not subject to
/// credit.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TerminalEvent {
    Snapshot(TerminalSnapshot),
    Data {
        seq: u32,
        data: String,
    },
    Exit {
        code: Option<i32>,
        signal: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn events_are_internally_tagged_camel_case() {
        assert_eq!(
            serde_json::to_value(TerminalEvent::Data {
                seq: 3,
                data: "é".into()
            })
            .unwrap(),
            json!({"type": "data", "seq": 3, "data": "é"})
        );
        assert_eq!(
            serde_json::to_value(TerminalEvent::Exit {
                code: Some(7),
                signal: None
            })
            .unwrap(),
            json!({"type": "exit", "code": 7, "signal": null})
        );
    }

    #[test]
    fn snapshot_omits_exit_code_until_exit() {
        let mut snapshot = TerminalSnapshot {
            pid: Some(4),
            running: true,
            data: String::new(),
            seq: 0,
            exit_code: None,
        };
        let running = serde_json::to_value(&snapshot).unwrap();
        assert!(running.get("exitCode").is_none());
        assert_eq!(running["pid"], 4);
        assert_eq!(
            serde_json::to_value(TerminalEvent::Snapshot(snapshot.clone())).unwrap(),
            json!({"type":"snapshot","pid":4,"running":true,"data":"","seq":0})
        );
        snapshot.exit_code = Some(7);
        assert_eq!(serde_json::to_value(&snapshot).unwrap()["exitCode"], 7);
    }
}
