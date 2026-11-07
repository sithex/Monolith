//! AI Agents "Live" page bridge (docs/agent-live.md). Rust only reads; the TS model
//! (`ui/views/agent-live-model.ts`, ported from Habby) interprets everything. `DbData` /
//! `HistoryRaw` field names and units are exactly Habby's `metrics-model.ts` (epoch ms).

use super::ai::JsonValue;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLiveRequest {
    /// Session the UI resolved last tick; `None` = none yet (then `db` is `None`).
    pub session_id: Option<String>,
    /// Opaque, from the previous `AgentLiveRaw.log.cursor`; `None` = start.
    pub log_cursor: Option<String>,
    /// True only while the Usage page is shown (history is cached ~60 s).
    pub history: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentLiveProc {
    pub pid: u32,
    pub ppid: u32,
    /// Basename of `/proc/<pid>/exe`, else `comm`.
    pub exe: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentLiveSnapshot {
    pub pid: u32,
    /// `/proc/<pid>` exists.
    pub alive: bool,
    pub mtime_ms: f64,
    #[specta(type = JsonValue)]
    pub data: serde_json::Value,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentLiveLog {
    pub cursor: String,
    /// Complete new lines of `logs/agent.log` since the request cursor.
    pub lines: Vec<String>,
    pub rotated: bool,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DbSession {
    pub id: String,
    pub title: Option<String>,
    pub model: Option<String>,
    pub calls: f64,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub cost: Option<f64>,
    pub tools: f64,
    pub started_at: Option<f64>,
    pub ended_at: Option<f64>,
    pub last_activity_at: Option<f64>,
    pub last_seen: Option<f64>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DbTask {
    pub task: String,
    pub model: Option<String>,
    pub calls: f64,
    pub cost: Option<f64>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DbDelegation {
    pub id: String,
    pub state: Option<String>,
    pub dispatched_at: Option<f64>,
    pub completed_at: Option<f64>,
    pub goals: Vec<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DbToolCount {
    pub name: String,
    pub n: f64,
}

/// Habby `DbData` (metrics-model.ts).
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DbData {
    /// The live session; with compaction continuations the chain SUM (id = newest segment).
    pub live: Option<DbSession>,
    /// Compression continuation chain, root first, newest last.
    pub chain: Vec<String>,
    pub children: Vec<DbSession>,
    pub tasks: Vec<DbTask>,
    pub delegations: Vec<DbDelegation>,
    /// Newest not-ended top-level session (Habby's `db-newest` fallback).
    pub newest_top_level: Option<String>,
    pub tools_by_name: Vec<DbToolCount>,
    pub turns: Option<f64>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDay {
    pub d: String,
    pub sessions: f64,
    pub tokens: f64,
    pub cost: f64,
    pub sub_cost: f64,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryModel {
    pub d: String,
    pub model: String,
    pub cost: f64,
    pub tokens: f64,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryTop {
    pub id: String,
    pub title: Option<String>,
    pub cost: f64,
    pub segments: f64,
}

/// Habby `HistoryRaw` (metrics-model.ts); the optional VS Code (X4) fields are never set here.
#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRaw {
    pub days: Vec<HistoryDay>,
    pub models: Vec<HistoryModel>,
    pub top: Vec<HistoryTop>,
    pub savings: Option<f64>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentLiveRaw {
    /// Epoch ms.
    pub now: f64,
    pub hermes_home: String,
    pub island_pid: u32,
    /// Pid of the Hermes child the island's terminal runs.
    pub terminal_pid: Option<u32>,
    /// This user's processes from `/proc`; `None` when `/proc` is unreadable.
    pub procs: Option<Vec<AgentLiveProc>>,
    #[specta(type = JsonValue)]
    pub active_sessions: serde_json::Value,
    pub active_sessions_error: Option<String>,
    #[specta(type = JsonValue)]
    pub processes: serde_json::Value,
    /// Every `snapshot-<pid>.json` that parses, newest first.
    pub snapshots: Vec<AgentLiveSnapshot>,
    pub snapshot_error: Option<String>,
    #[specta(type = JsonValue)]
    pub plan: serde_json::Value,
    pub plan_error: Option<String>,
    pub log: AgentLiveLog,
    pub db: Option<DbData>,
    pub db_error: Option<String>,
    /// sqlite busy/locked this call (`db` is `None` then; the UI keeps its last good copy).
    pub db_busy: bool,
    pub history: Option<HistoryRaw>,
    pub history_error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLiveBlock {
    pub reset_at: f64,
    pub peak: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum AgentLiveBlocksOp {
    Get {},
    Set { blocks: Vec<AgentLiveBlock> },
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentLiveBlocks {
    pub blocks: Vec<AgentLiveBlock>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_names_follow_the_contract() {
        let req: AgentLiveRequest =
            serde_json::from_value(json!({"sessionId": null, "logCursor": "c", "history": true}))
                .unwrap();
        assert_eq!(req.log_cursor.as_deref(), Some("c"));
        let get: AgentLiveBlocksOp = serde_json::from_value(json!({"type": "get"})).unwrap();
        assert_eq!(get, AgentLiveBlocksOp::Get {});
        let set: AgentLiveBlocksOp = serde_json::from_value(
            json!({"type": "set", "blocks": [{"resetAt": 1.0, "peak": 2.0}]}),
        )
        .unwrap();
        assert!(matches!(set, AgentLiveBlocksOp::Set { blocks } if blocks.len() == 1));
        let log = serde_json::to_value(AgentLiveLog {
            cursor: "x".into(),
            lines: vec![],
            rotated: false,
            error: None,
        })
        .unwrap();
        assert_eq!(
            log,
            json!({"cursor": "x", "lines": [], "rotated": false, "error": null})
        );
    }
}
