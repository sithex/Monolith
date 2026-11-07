# AI Agents tab: Live page (port of Habby's metrics dock)

Status: implemented in Monolith. Imported from Centenaria feature commit `6827394`;
reviewed the Agent Live changes in follow-up `5448e1b` separately from its Claude Code feature.
Original sources named by the fork: Habby `app/lib/habby/metrics-model.ts` (pure logic),
`metrics-reader.ts` (file/db reads), `metrics.ts` (blocks), `app/assets/habby-metrics.js`
(renderer), and `hermes-plugin/habby-metrics`. Original design: `DESIGN-metrics-panel.md`.
The original Habby repository was not supplied; the provenance available here is the friend's fork.

## Layout

The AI Agents tab (`view: 'ai'`, `ui/views/AiStatsView.tsx`) gets a **Live | Usage** toggle.
- **Usage** = Nick's existing **Overview | Sessions** dashboard and styling unchanged, plus two
  Habby History cards: 5-hour block history and top sessions this week (click copies the session
  id for `/resume`). Live is selected initially; the outer switch uses the existing segmented tabs.
- **Live** = Habby's Live page as React cards in the existing `ai-card` style: header (model chip,
  TEAM badge, status dot, turn timer), alerts, activity + last-5-steps, context gauge, cache
  heartbeat, this turn, session, subagents (+ background processes).
- No VS Code sessions (Habby X4) in this phase.

## Data path

```
Hermes ──(habby-metrics plugin)──> ~/.hermes/cache/habby-metrics/{snapshot-<pid>.json, plan.json}
Hermes ──> ~/.hermes/state.db, logs/agent.log, runtime/active_sessions.json, processes.json
      Rust services/agent_live.rs  (dumb, read-only bridge: files, /proc, read-only sqlite)
      ──> command agent_live_raw ──> ui/views/agent-live-model.ts (Habby logic, ported)
      ──> buildView ──> React cards
```

Rust never interprets plugin JSON beyond "is it valid JSON"; parsing/normalising stays in the TS
model so Habby's tested logic is reused. Rust does: file reads, the rotation-safe agent.log cursor,
/proc scan, sqlite queries (ported verbatim from metrics-reader.ts), and the blocks file.

## Contract (both sides MUST follow this exactly)

Tauri commands (snake_case in Rust, camelCase args/fields over IPC via serde rename_all camelCase):

### `agent_live_raw(req: AgentLiveRequest) -> AgentLiveRaw`

```ts
interface AgentLiveRequest {
  sessionId: string | null      // session the UI resolved last tick; null = none yet
  logCursor: string | null      // opaque, from the previous AgentLiveRaw.log.cursor; null = start
  history: boolean              // true only while the Usage page is shown (history is ~60 s cached)
}
interface AgentLiveRaw {
  now: number                   // epoch ms
  hermesHome: string
  islandPid: number
  terminalPid: number | null    // pid of the Hermes child the island's terminal runs (None if not running)
  procs: { pid: number, ppid: number, exe: string }[] | null   // this user's processes from /proc (exe = basename of /proc/<pid>/exe or comm)
  activeSessions: unknown | null        // raw JSON of runtime/active_sessions.json (null if missing/invalid)
  activeSessionsError: string | null
  processes: unknown | null             // raw JSON of ~/.hermes/processes.json
  snapshots: { pid: number, alive: boolean, mtimeMs: number, data: unknown }[]  // every cache/habby-metrics/snapshot-<pid>.json that parses, newest first; alive = /proc/<pid> exists
  snapshotError: string | null
  plan: unknown | null                  // raw JSON of cache/habby-metrics/plan.json
  planError: string | null
  log: { cursor: string, lines: string[], rotated: boolean, error: string | null }
        // complete new lines of logs/agent.log since logCursor (rotation-safe: inode change or
        // shrink => rotated=true and read the new file from 0, plus the rest of agent.log.1 if
        // it is the old inode). logCursor null => only the last 512 KiB (drop the first partial
        // line). Max 2 MiB per call. Never hold the file open between calls.
  db: DbData | null                     // Habby DbData shape (metrics-model.ts:742) for req.sessionId; null when sessionId null
  dbError: string | null
  dbBusy: boolean                       // sqlite busy/locked this call (db = null then; UI keeps last good)
  history: HistoryRaw | null            // Habby HistoryRaw shape (metrics-model.ts:762); only when req.history
  historyError: string | null
}
```

`DbData` / `HistoryRaw` field names and units are exactly Habby's (timestamps in epoch ms like
Habby's reader produces; check `toMs` usage in metrics-reader.ts). Port the SQL from
`metrics-reader.ts` verbatim (read-only, `mode=ro`, busy timeout 100 ms, open → queries → close
every call, never write/VACUUM/checkpoint; a missing table/column => that field null/empty, not an error).

### `agent_live_blocks(op: { type: 'get' } | { type: 'set', blocks: { resetAt: number, peak: number }[] }) -> { blocks: { resetAt: number, peak: number }[] }`

Persists 5-hour block history to `<island data dir>/agent-live-blocks.json` (atomic write, max 200
entries kept newest). Same role as Habby's `habby-metrics-blocks.json`.

## Polling (UI)

Only while the AI Agents tab is visible: Live page 1 s while a turn is running, 5 s idle; Usage
page 60 s. Nothing when hidden. If the session the UI resolves differs from the one it requested,
re-request once immediately.

## Security

Read-only on everything under `~/.hermes`. No message text: the plugin writes only redacted tool
previews ≤ 60 chars and subagent goals ≤ 80 chars. Render with React text only (no
`dangerouslySetInnerHTML`). No network.

## Monolith integration and safety

- The bridge uses Nick's blocking command helper and existing `Services` state. Commands were
  hand-added to `lib.rs`, `build.rs`, module declarations and `permissions/island.toml`;
  `capabilities/island.json` already grants that permission set to the single island window.
  No broader filesystem or network capability is added.
- The client wraps the generated commands with Nick's `unwrap`. Bindings are regenerated only
  with `cargo test --locked --lib export_bindings -- --ignored`, never copied from the fork.
- `createVisiblePoller` gates both the feed and display clock. Game-mode pauses, hidden documents,
  inactive views and unmounts stop scheduling. Cancellation also prevents a hidden first read
  from launching session-discovery follow-ups or block writes. Block writes are awaited.
- Hermes profile resolution honors `HERMES_HOME` in production, but test instances always read
  the configured scratch home. No plugin starts automatically and the bridge has no worker thread.
- SQLite uses a private read-only VFS, including WAL/SHM opens: committed WAL records remain
  visible without creating, deleting or changing database sidecars. Fixtures verify bytes and
  file membership before/after reads. Missing optional schema fields fail soft; busy/corrupt
  sources are not turned into successful empty data. Session IDs use bound SQL parameters.
- Files, snapshot enumeration, process enumeration, log cursors and retained block history are
  bounded. The rotation-safe tailer never exposes a partial-line suffix as a complete record.
  The sole writer is the atomic block-history file in the island data directory; Hermes-home
  destinations (including symlinks/parent segments) are rejected.

Without the plugin, Live displays **Live metrics plugin not installed**, without an error toast
or repeated logging. Available log/database estimates still render, or explicit unavailable
placeholders when no session data exists. Usage's existing account/session dashboards and database
history work independently of the plugin; the 5-hour card shows saved peaks or an explanatory
empty state. With the plugin enabled by the user, a new Hermes session supplies detailed status,
context/cache/turn/subagent activity. Plan windows appear only when Hermes already cached them.
Prices from the imported model are list-price estimates, not subscription bills.

### Optional installation: source only

Neither installation script is run by the app or by the port's checks. If the user later runs
`hermes-plugin/habby-metrics/install.sh --yes`, it would:

1. Copy the selected Hermes home's `config.yaml` to `backups/config.yaml.<timestamp>`.
2. Replace `plugins/habby-metrics/` with `plugin.yaml`, `__init__.py` and `README.md`.
3. Run `hermes plugins enable habby-metrics`, updating `plugins.enabled` in `config.yaml`.
4. Verify discovery and enabled status. On the default home, Hermes may notify its running
   messaging gateway to reload plugins; other running sessions keep their current plugin state.
5. Subsequent hook activity writes snapshots/events and cached plan windows to
   `cache/habby-metrics/`. Tool previews/goals are redacted but may still contain sensitive
   free text; `HABBY_METRICS_PREVIEW=0` suppresses both.

The Monolith copy removes upstream's opt-in provider fetching. Even an inherited
`HABBY_METRICS_PLAN=fetch` remains cache-only; `0`/`off`/`false` disables plan mirroring.
The source-only plugin checks exercise this with fake threads, without Hermes imports or writes.
The inherited full plugin integration harness has not been run or installed against a real home.

### Deliberately excluded

The same fork commit also changes `integrations/plasma/anchor.rs` to divide device-pixel output
sizes by fractional scale, and `island/plasma-anchor/contents/ui/main.qml` to rediscover placement
on screen-geometry changes. Its `TerminalView.tsx`, `terminal-keys.ts` and terminal-key tests add
Hermes-specific copy/paste/newline/select-all/redo remaps, selection tracking, right-click behavior
and font zoom. None of those changes are imported. Nick's newer terminal and Plasma code wins.

In `5448e1b`, `AgentLiveView.tsx` and its test changes principally wire in Claude Code readers.
Those readers, mocks and source badges are excluded. The standalone `hCost` export is retained;
there is no Claude Code command, poller, component, stylesheet or capability in this port.

## Provenance and licence inventory

The supplied fork contains the GPL v3 text in `LICENSE`; `LICENSE-RIPPLE` states GPL-3.0-or-later
for non-Ripple code and retains Ripple's MIT notice, Copyright (c) 2025 Piko Can Fly. These files
are already present in Monolith and are not replaced. No separate Habby/plugin licence or
per-file copyright/SPDX header was found in the supplied feature. Attribution comments naming
Habby and its source files are retained. The original Habby repository was not available to
independently establish any additional licence terms; verify that provenance before external
redistribution if needed.

Files containing copied/adapted feature code from the fork (including its Habby-derived logic):

- `island-tauri/src-tauri/src/commands/agent_live.rs`
- `island-tauri/src-tauri/src/contract/agent_live.rs`
- `island-tauri/src-tauri/src/services/agent_live.rs`
- `island-tauri/src-tauri/src/services/agent_live/db.rs`
- `island-tauri/src-tauri/src/services/agent_live/log.rs`
- `island-tauri/ui/views/agent-live-model.ts`
- `island-tauri/ui/views/agent-live-ipc.ts` (rewritten around Nick's client and clipboard helper)
- `island-tauri/ui/views/AgentLiveView.tsx`
- `island-tauri/ui/island/components/AgentLive.tsx`
- `island-tauri/ui/island/components/AgentLiveParts.tsx`
- `island-tauri/ui/island/components/AgentLiveUsage.tsx`
- `island-tauri/ui/island/agent-live.css`
- `island-tauri/tests/phase7b-agent-live-model.test.mjs`
- `island-tauri/tests/phase7b-agent-live-ui.test.mjs`
- `hermes-plugin/habby-metrics/__init__.py` (Monolith cache-only adaptation)
- `hermes-plugin/habby-metrics/install.sh`
- `hermes-plugin/habby-metrics/uninstall.sh`
- `hermes-plugin/habby-metrics/tests/_common.py`
- `hermes-plugin/habby-metrics/tests/harness_hermes.py`
- `hermes-plugin/habby-metrics/tests/node_reader.cjs`
- `hermes-plugin/habby-metrics/tests/run_tests.py`
- `hermes-plugin/habby-metrics/tests/test_core.py`
- `hermes-plugin/habby-metrics/tests/test_fixes.py`
- `hermes-plugin/habby-metrics/tests/test_plan.py`
- `hermes-plugin/habby-metrics/tests/verify_home.py`

Also imported/adapted non-code material: this document, plugin `.gitignore`, `README.md` and
`plugin.yaml`. The new `tests/test_no_network.py` is Monolith-specific test code, not copied.
Small integration additions were hand-merged into `Cargo.toml`, `Cargo.lock`, `build.rs`,
`permissions/island.toml`, `src/{commands,contract,services}/mod.rs`, `src/lib.rs`,
`ui/ipc/client.ts`, `ui/views/AiStatsView.tsx` and `tests/phase7b-ai.test.mjs` (paths relative to
`island-tauri/`, with Rust paths under `src-tauri/`). Existing Nick code was not replaced.
The two `permissions/autogenerated/agent_live_*.toml` files and `ui/ipc/bindings.ts` are locally
generated from the commands/contracts, not copied from the fork.

## Validation

- `cargo test --locked --lib`: 539 passed, 0 failed, 3 intentionally ignored.
- `cargo clippy --locked --all-targets`: success, no warnings.
- `npx tsc --noEmit`: success, no diagnostics.
- `npm run -s test:ui`: 435 passed, 0 failed (296 existing plus the new feature tests).
- `npm run -s build:ui`: success, manifest generated.
- Cache-only plugin checks: 2 passed. Installer/uninstaller syntax checked with `bash -n`,
  never executed. No GUI tests or island processes were started, stopped or restarted.
