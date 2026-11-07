# habby-metrics — optional Hermes plugin for Monolith's AI Agents tab (Linux)

Monolith adaptation of Centenaria's Habby port: network plan fetching is disabled
(including `HABBY_METRICS_PLAN=fetch`). Only Hermes's existing in-memory cache is mirrored.
The plugin is shipped as source, not installed automatically. Upstream test/version notes below
are provenance, not a claim that this port installed or exercised the plugin against a live home.

Observer-only Hermes plugin. It turns Hermes hook events into small files that Centenaria reads for the
**AI Agents tab → Live page** (Rust `services/agent_live.rs` → `ui/views/agent-live-model.ts`; contract in
`docs/agent-live.md`). Linux port of the plugin from the Habby (Windows) fork; the name `habby-metrics` and the
`cache/habby-metrics` directory are part of that contract and are kept. Original design:
`DESIGN-metrics-panel.md` F3; spike corrections C1–C5; fix pass `habby-FIXES-metrics.md`. Files only,
no network listener. **No message text is written** (no prompts, replies, tool arguments or results). The
only free text on disk is a ≤ 60-char tool preview (command, or the file **name** of a path) and a ≤ 80-char
subagent goal, both pattern-redacted — see Privacy for exactly what that does and does not catch, and
`HABBY_METRICS_PREVIEW=0` to turn both off.

- Tested against Hermes **0.22.6** (`hermes-agent` f0d94882e8, Linux; originally 0.22.2 on Windows). Hook
  names/payloads can change on upgrades; re-run `tests/run_tests.py` after every Hermes update.
  0.22.6 drift handled: `api_request_error` no longer sends `error_type`; it is read from `error["type"]`
  (the error message is never kept).
- User plugins are opt-in: the plugin loads only when `plugins.enabled` lists `habby-metrics` (install.sh
  does this with `hermes plugins enable`). Hermes processes started afterwards load it; running ones don't.

## Install / uninstall (per user, no admin)

```bash
cd hermes-plugin/habby-metrics  # from the Monolith repository root
./install.sh            # dry run: prints the plan, changes nothing
./install.sh --yes      # config backup + copy to ~/.hermes/plugins/habby-metrics + enable + verify
./uninstall.sh --yes    # config backup + disable + remove plugin dir and cache/habby-metrics
```

Both scripts refuse to change anything without `--yes`. The home is `--hermes-home <dir>`, else
`$HERMES_HOME`, else `~/.hermes`; for any non-default home the `hermes` child is isolated
(`HERMES_DATA_DIR_SUFFIX`, `HERMES_GATEWAY_LOCK_DIR`) so it cannot nudge the live gateway/dashboard.
`--hermes <path>` overrides the `hermes` lookup (PATH). Config backups go to
`<HERMES_HOME>/backups/config.yaml.<YYYYmmdd-HHMMSS>` and are never deleted. The plugin goes to
`<HERMES_HOME>/plugins/habby-metrics` (Hermes's user plugin dir). The installer verifies the result with
Hermes's own plugin discovery (`tests/verify_home.py` under the Hermes python: listed **and enabled**) and
`hermes plugins list --user --json`.

**What install/uninstall do NOT do to running processes:** Hermes processes that are already running keep
their current plugin state until they restart — after uninstall, a running Hermes keeps writing its snapshot
(and re-creates `cache/habby-metrics`) until it exits. On the default home, `hermes plugins enable/disable`
also asks a **running messaging gateway** to reload plugins, so the gateway may load the plugin immediately
on install; its threads can only stop when that gateway restarts.

## How it works

- 16 hooks: `pre_llm_call`, `pre_api_request`, `post_api_request`, `api_request_error`, `pre_tool_call`,
  `post_tool_call`, `subagent_start`, `subagent_stop`, `pre_approval_request`, `post_approval_response`,
  `on_human_input_request`, `on_human_input_resolved`, `on_session_start`, `on_session_end`,
  `pre_auxiliary_call`, `post_auxiliary_call` (the aux pair is used only for `aux_task` = compression).
  `pre_llm_call` returns `None` (it never injects context).
- Each hook copies a few scalars and does one `SimpleQueue.put` (measured ≈ 0.5–0.9 µs median, ≤ 3.3 µs p99).
  Every body is `try/except` and returns `None` (`pre_tool_call` is fail-closed in Hermes).
- Threads start lazily on the first hook (short `hermes …` CLI commands stay idle), both daemon:
  `habby-metrics-writer` (applies events, appends `events-<pid>.jsonl`, writes the snapshot ≤ 4 Hz; blocks
  with zero CPU when every session is idle; **re-stamps the snapshot every 30 s while any session is busy**)
  and `habby-metrics-plan` (plan windows; see below). If the writer cannot start, the hooks become no-ops.
- The plugin loads in **every** Hermes process that runs agents (TUI gateway, messaging gateway, cron …);
  each writes its own `snapshot-<pid>.json` / `events-<pid>.jsonl`. The snapshot is removed at interpreter
  exit; files of processes that are gone (dead pid; or older than 1 h when liveness can't be checked; or
  older than 7 days) are pruned at writer start and every 10 min.
- **Background review / `/btw` forks** share the parent's session id. Hermes fires `pre_llm_call` only for
  real (persisted) turns, never for those forks, so a call whose `turn_id` was not announced by
  `pre_llm_call` goes to the session's `background` bucket and never changes its status, turn or totals.
  Fallback when `pre_llm_call` was never seen (plugin loaded mid-turn): a final answer (`finish_reason` stop,
  no tool calls, nothing running) sets the status to `idle`.
- **Context compaction** gives the conversation a new session id with the same `turn_id`: the plugin moves
  totals, turn, compactions and subagents to the new id (`continued_from`), and the old id keeps a stub with
  `continued_to` (late events for the old id are routed to the new one).

## File contract (Centenaria reads these; see `docs/agent-live.md`)

Directory: `<HERMES_HOME>/cache/habby-metrics/` (HERMES_HOME from `hermes_constants.get_hermes_home()` in the
Hermes process; default `~/.hermes`). All timestamps are ISO-8601 UTC with ms and `Z`.
All JSON is UTF-8. Fields marked *(additive)* are extras beyond the original contract; readers may ignore them.

### `snapshot-<pid>.json` — atomic (`os.replace`), ≤ 4 Hz, on change + every 30 s while busy

```jsonc
{
  "v": 1, "pid": 23848, "written_at": "2026-10-21T00:52:32.240Z", "plugin_version": "1.1.0",
  "sessions": {
    "<session_id>": {                       // EVERY session seen in this process, incl. subagent children
      "status": "idle|thinking|tool|waiting|compacting|error",
      "model": "claude-opus-5-5",
      "platform": "tui",                    // (additive) 'subagent' for children
      "parent_session_id": null,            // set on child sessions (from subagent_start)
      "role": null, "goal": null,           // child sessions: role, goal (≤ 80 chars, redacted)
      "subagent_id": null,                  // (additive) child sessions: the parent's subagent id (e.g. "sa-0")
      "turn": { "started_at": "…", "ended_at": null,     // ended_at (additive): null while the turn runs
                "calls": 3, "input": 30, "output": 1200, "cache_read": 260000, "cache_write": 4000,
                "cost_usd": 0.0912 },                   // null when any call in the turn had no price
      "current_tool": { "name": "terminal", "preview": "git status --short", "started_at": "…" } | null,
      "steps": [ { "tool": "read_file", "status": "ok|running|error", "ms": 12 } ],   // last 5 of this turn
      "last_call": { "in": 12, "out": 345, "cache_read": 88000, "cache_write": 2100, "prompt": 90112,
                     "ttfb_ms": 600, "latency_ms": 2500, "at": "…",
                     "cost_usd": 0.035, "finish_reason": "tool_use" },            // last two: (additive)
      "prompt_history": [ 88000, 89100, 90112 ],   // (additive) prompt tokens of the last ≤ 10 calls
      "context": { "used": 90112, "max": 1000000, "pct": 9.0, "estimated": false },  // null until known
      "waiting": { "kind": "approval|clarify|sudo", "since": "…" } | null,
      "subagents": [ { "id": "sa-0", "session_id": "<child sid>", "goal": "…≤80, redacted", "role": "leaf",
                       "model": "claude-fable-5-1", "status": "running|completed|failed|interrupted",
                       "started_at": "…", "ms": 4321, "tools": 7, "input": 250, "output": 3100,
                       "cache_read": 240000, "cache_write": 9000, "cost_usd": 0.41 } ],   // last 32
      "compressions": 0,
      "totals": { "calls": 18, "input": 44, "output": 13641, "cache_read": 877934, "cache_write": 117175,
                  "cost_usd": 1.03, "tool_calls": 17, "tools_by_name": { "terminal": 9, "read_file": 8 } },
      "background": { "calls": 2, "input": 10, "output": 80, "cache_read": 180000, "cache_write": 0,
                      "cost_usd": 0.04, "tool_calls": 1 } | null,   // (additive) review forks, not in totals
      "continued_from": null, "continued_to": null,                   // (additive) compaction rotation
      "first_seen": "…", "updated_at": "…"  // (additive)
    }
  }
}
```

Semantics:
- `status`: `thinking` from `pre_llm_call`/`pre_api_request` (and after a tool ends); `tool` while any tool
  call is running or the last response asked for tools; `idle` after a final answer and after
  `on_session_end` (fires once per turn); `waiting` between `on_human_input_request` and `…_resolved`
  (approval — CLI and TUI/gateway paths — clarify, sudo; only when the event carries a session id);
  `compacting` during a compression aux call; `error` after `api_request_error` (until the next request) or
  a failed turn.
- `turn` resets on a new `turn_id` / the first event after a finished turn; `totals` accumulate for the life
  of the process. `input` = uncached input tokens (Anthropic semantics), `prompt` = input + cache_read +
  cache_write = context fill. Subagent index rows use the same split (`input` uncached + `cache_read`/`cache_write`).
- `context.used` = last call's prompt tokens, `max` = Hermes's `context_length`; before the first response
  of a session it is Hermes's `approx_input_tokens` with `estimated: true`.
- `cost_usd` = API-list-price estimate from Hermes's own `agent.usage_pricing.estimate_usage_cost`
  (subscription users are not billed this; label it an estimate). `null` when Hermes had no price for
  any call in that bucket. **Fast mode is not priced:** Hermes's hook usage drops `raw_usage`, so fast-mode
  calls are priced at the normal rate here (state.db prices them correctly). On the messaging gateway,
  state.db rows hold absolute totals that already include subagents, so a "parent + children" sum there
  would double-count (the TUI path writes per-call deltas and is not affected).
- Subagents: the parent's `subagents[]` is the index; each child also has its own full entry under
  `sessions` (keyed by its session id) with `parent_session_id`/`role`/`goal`. If `child_session_id` is
  unknown at `subagent_start`, the row is linked to the next new session that says `platform: subagent`
  within 30 s.
- At most 64 sessions per snapshot (oldest idle ones dropped).
- Pick the file by the island terminal's Hermes pid (`runtime/active_sessions.json`), or scan
  `snapshot-*.json` (newest first) and filter by session id. **A file whose pid is not running
  (`/proc/<pid>` missing) is stale; while the pid runs the file is current** (heartbeat), whatever its age.
- Reading: plain read + JSON parse; `os.replace` is atomic on Linux, so a torn read never happens (tested
  with Python and Node readers). Readers never block the writer.

### `plan.json` — official plan windows (one file shared by all processes)

```jsonc
{ "fetched_at": "…" | null,              // when these windows were fetched (kept across failures)
  "windows": [ { "label": "Current session", "used_percent": 17.0, "reset_at": "…", "scope": "account" },
               { "label": "Current week",    "used_percent": 39.0, "reset_at": "…", "scope": "account" } ],
  "details": [ "…" ],                      // Hermes's extra lines (e.g. extra-usage credits)
  "unavailable_reason": null | "fetch failed" | "<Hermes reason>",
  "source": "fetch|hermes-cache",          // (additive)
  "attempted_at": "…", "next_attempt_at": "…", "failures": 0 }   // (additive) cross-process backoff
```
- Only windows present in the response are listed (this plan has no Opus-week window); labels are Hermes's.
- **Default: cache only, no network.** The plan thread copies the windows from Hermes's in-memory
  `account_usage_cache` (the TUI refreshes it on its per-turn `session.usage`) when that copy is newer than
  `plan.json`. Consumers should treat it as stale after 15 min and ignore windows whose reset time has passed.
- This Monolith copy is **cache-only**. The upstream `HABBY_METRICS_PLAN=fetch` opt-in is disabled;
  it does not contact providers or refresh credentials. `HABBY_METRICS_PLAN=0` disables the plan
  thread entirely. The inherited poller's injected fake-fetch path remains for unit tests only.
- `plan.off` in this directory (create it with `touch` or from a consumer setting; Centenaria is read-only
  and does not write it) is checked on **every poll**: while it exists the plugin neither copies the cache
  nor fetches.

### `events-<pid>.jsonl` — append-only, one JSON object per line, one file per process

Rotated by its only writer at 5 MB → `events-<pid>.1.jsonl` (no shared file, no lock); pruned with the
snapshot when the process is gone. Nothing reads these yet (reserved for docket items #9–#14).

`{"ts": "…", "pid": 23848, "session_id": "…"|null, "event": "<name>", …counts/names only}`

| event | extra fields |
|---|---|
| `session_start` | model, platform |
| `session_continued` | continued_from |
| `turn_start` | model |
| `api_call` | model, n, input, output, cache_read, cache_write, prompt, ttfb_ms, latency_ms, tool_calls, cost_usd; `background: true` for fork calls |
| `api_error` | error_type (class name), status_code, retryable, background |
| `tool_start` | tool (`background: true` for fork tools) |
| `tool_end` | tool, status (`ok`/`error`), ms |
| `wait_start` / `wait_end` | kind; `wait_end` adds outcome (e.g. `deny`, `once`, `answered`) |
| `approval_request` / `approval_response` | surface, pattern (pattern key) / choice |
| `compaction_start` / `compaction_end` | `compaction_end`: ok |
| `subagent_start` | child_session_id, role |
| `subagent_stop` | child_session_id, status, ms, tools |
| `turn_end` | completed, interrupted, failed, calls |

Null fields are omitted. No previews or goals are written to events.

## Privacy

Never copied out of a hook payload: `assistant_message`, `response`, tool `args` (except the preview source)
and `result`, `user_message`, `conversation_history`, `request_messages`, `system_prompt`, human-input
`prompt`, approval `command`/`description`, `child_summary`, `error_message`.

The only free text on disk (snapshot only):
- **tool preview**, ≤ 60 chars, from the first present argument of `command`, `path`, `file_path`, `pattern`,
  `goal`, `name`, `action`. For `path`/`file_path` only the **file name** is kept (no directories).
  `query` and `url` are never previewed. Shell commands are kept as typed (they are content: a private path
  inside a command survives).
- **subagent goal**, ≤ 80 chars.

Both go through `agent.redact.redact_sensitive_text(force=True)` plus extra patterns for `-p<password>`
(`mysql -pS3cret`), `password|passwd|pwd|secret <value>` / `=value`, `--password=…`/`--token …`, and
`net user <name> <password>`, then are flattened to one line; a redaction failure writes an empty string,
never the raw text. Redaction is pattern-based: a secret in a shape none of the patterns knows can survive.
**`HABBY_METRICS_PREVIEW=0`** in Hermes's environment writes no previews and no goals at all.

## Security notes

- No listener, no IPC: plain files in the user's own Hermes home, written atomically.
- Network: none by default. With `HABBY_METRICS_PLAN=fetch`, the plan thread calls Anthropic's OAuth usage
  endpoint through Hermes's `fetch_account_usage` — **only while the Claude Code access token is currently
  valid** (read with Hermes's read-only `read_claude_code_credentials` + `is_claude_code_token_valid`), and
  the token is passed explicitly, so the plugin never makes Hermes resolve or **refresh** a credential (a
  refresh would rotate the token shared with Claude Code in VS Code). An expired token means "skip until
  later". The token is never written to any file.
- Plan data written: labels, percentages, reset times and Hermes's detail lines (e.g. "Extra usage x / y
  USD"); no token or account id.

## Tests

```bash
"$(ls -d ~/.hermes/tools/python-*/bin/python3 | sort -V | tail -1)" -I tests/run_tests.py
```
Unit tests (payload → snapshot shape, child sessions, statuses, privacy sentinels, real `agent.redact`, preview
rules, per-pid events rotation, atomic writes vs. Python and Node readers, heartbeat, background-review
forks, compaction rotation, unknown cost, pruning, start/write failures, plan cache/fetch/token/plan.off/
backoff/lock with a fake fetch — never the network), then `tests/harness_hermes.py` twice (enabled /
not-enabled) in a scratch HERMES_HOME through the real `discover_plugins()`, `model_tools.handle_function_call`,
`prompt_dangerous_approval`, the gateway approval wait, `_collect_pre_llm_call_context`,
`_fire_pre/post_api_request_hook`, `_invoke_api_request_error_hook` (including a background-review-style fork with `_persist_disabled`), plus
per-hook cost. `tests/verify_home.py <home> [--allow-live]` checks an installed home (read-only discovery).
Evidence: `~/.hermes/cache/scratch/metrics-plugin/ev/`. Tests never touch the live home.
