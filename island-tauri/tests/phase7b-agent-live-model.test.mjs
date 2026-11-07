// Habby's metrics dock model tests (habby-tests/metrics.test.cjs + metrics-fixes.test.cjs), ported to
// ui/views/agent-live-model.ts. Same assertions, same JSON-equality `check`; every check is one node test.
// Not ported: LogTailer (fs; Rust owns the cursor now, see the LogAccumulator tests), dock geometry, config,
// panel follower, Electron dock window, the sqlite reader (Rust services/agent_live.rs) and the static
// checks of Habby's dock page (phase7b-agent-live-ui.test.mjs checks the React cards instead).
// readSessionAndPlugin / readBgProcs run over AgentLiveRaw-shaped data instead of files.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load } from './phase7b-loader.mjs';
const M = load('ui/views/agent-live-model.ts');
const check = (label, got, want) => test(label, () => assert.equal(JSON.stringify(got), JSON.stringify(want)));
const ok = (label, cond, detail) => check(label + (detail !== undefined ? ` (${detail})` : ''), !!cond, true);

// ======== metrics.test.cjs ========
// ---- log parser (agent/turn_usage.py variants, SPIKE C9) -----------------------------------
const L = {
    full: '2026-10-20 19:32:14,028 INFO [20261020_193116_fdf142] agent.conversation_loop: API call #8: model=claude-opus-5-5 provider=anthropic in=89979 out=799 total=90778 latency=7.8s cache=87746/89979 (98%) write=2231 id=msg_011CgEMw',
    upstream: '2026-10-20 19:32:15,000 INFO [20261020_193116_fdf142] agent.conversation_loop: API call #9: model=claude-opus-5-5 provider=anthropic in=91000 out=100 total=91100 latency=2.0s cache=90000/91000 (98%) write=1000 id=msg_x upstream=req_abc',
    writeOnly: '2026-10-20 19:32:16,000 INFO [20261020_193116_fdf142] agent.conversation_loop: API call #10: model=claude-opus-5-5 provider=anthropic in=5000 out=10 total=5010 latency=1.0s write=4990 id=msg_y',
    unavail: '2026-10-20 19:32:17,000 INFO [20261020_193116_fdf142] agent.conversation_loop: API call #11: model=claude-opus-5-5 provider=anthropic in=? out=? total=? latency=1.0s usage=unavailable',
    turnEnd: '2026-10-20 19:32:18,000 INFO [20261020_193116_fdf142] agent.conversation_loop: Turn ended: reason=text_response(finish_reason=stop) model=claude-opus-5-5 api_calls=4/90',
    tool: '2026-10-20 19:32:13,000 INFO [20261020_193116_fdf142] agent.tool_executor: tool terminal completed (8.84s, 162 chars)',
    toolFail: '2026-10-20 19:32:13,500 INFO [20261020_193116_fdf142] agent.tool_executor: tool write_file failed (0.1s): {"error": "x"}',
    toolErr: '2026-10-20 19:32:13,700 WARNING [20261020_193116_fdf142] agent.tool_executor: Tool terminal returned error (2.5s): {"output": ""}',
    retry: '2026-10-20 19:33:00,000 WARNING [20261020_193116_fdf142] agent.conversation_loop: API call failed (attempt 1/3) error_type=RateLimitError HTTP 429 provider=anthropic',
    ctx: '2026-10-20 19:30:00,000 INFO [20261020_193116_fdf142] agent.context_compressor: Context compressor initialized: model=claude-opus-5-5 context_length=1000000 threshold=0.8',
    noise: '2026-10-20 19:32:13,000 INFO tui_gateway.server: something else',
}
const c1 = M.parseLogLine(L.full)
check('log: full line', c1 && { k: c1.kind, sid: c1.call.sid, n: c1.call.n, p: c1.call.prompt, o: c1.call.output, cr: c1.call.cacheRead, cw: c1.call.cacheWrite, pct: c1.call.hitPct, lat: c1.call.latencyS },
    { k: 'call', sid: '20261020_193116_fdf142', n: 8, p: 89979, o: 799, cr: 87746, cw: 2231, pct: 98, lat: 7.8 })
check('log: local timestamp', c1.call.at, new Date(2026, 9, 20, 19, 32, 14, 28).getTime())
check('log: upstream= after id= (C9)', [M.parseLogLine(L.upstream).call.upstream, M.parseLogLine(L.upstream).call.id], ['req_abc', 'msg_x'])
const w = M.parseLogLine(L.writeOnly).call
check('log: write= without cache=', [w.cacheRead, w.cacheWrite, w.prompt], [null, 4990, 5000])
const u = M.parseLogLine(L.unavail)
check('log: usage=unavailable variant', [u.kind, u.call.prompt, u.call.output], ['call', null, null])
check('log: Turn ended', [M.parseLogLine(L.turnEnd).kind, M.parseLogLine(L.turnEnd).reason], ['turnEnd', 'text_response'])
check('log: tool completed / failed / returned error', [L.tool, L.toolFail, L.toolErr].map(x => { const e = M.parseLogLine(x); return [e.tool, e.status, e.ms] }), [['terminal', 'ok', 8840], ['write_file', 'error', 100], ['terminal', 'error', 2500]])
check('log: rate limit', [M.parseLogLine(L.retry).kind, M.parseLogLine(L.retry).what], ['error', 'rate-limited'])
check('log: context_length', M.parseLogLine(L.ctx).contextLength, 1000000)
check('log: unrelated / garbage lines -> null', [M.parseLogLine(L.noise), M.parseLogLine('garbage'), M.parseLogLine('')], [null, null, null])

// ---- LogState: turns, steps, costs
const st = new M.LogState()
for (const l of [L.ctx, L.tool, L.toolFail, L.toolErr, L.full, L.upstream, L.turnEnd]) st.apply(M.parseLogLine(l))
const ss = st.sessions.get('20261020_193116_fdf142')
check('state: calls, turn closed', [ss.calls.length, ss.turnStart, ss.turns.length, ss.turns[0].calls], [2, 2, 1, 2])
ok('state: turn cost = sum of list-price call costs', Math.abs(ss.turns[0].cost - (M.callCost(c1.call) + M.callCost(M.parseLogLine(L.upstream).call))) < 1e-12)
check('state: per-session context length', ss.contextLength, 1000000)
for (let i = 0; i < 7; i++) st.apply(M.parseLogLine(L.tool))
check('state: last 5 steps only', ss.steps.length, 5)
check('cost: opus-5-5 list price (DB check value $0.1581)', +M.costOf('claude-opus-5-5', { input: 10, output: 1294, cacheRead: 54879, cacheWrite: 24243 }).toFixed(4), 0.1581)
check('cost: unknown model -> null', M.costOf('gpt-x', { input: 1, output: 1, cacheRead: 0, cacheWrite: 0 }), null)

// ---- LogAccumulator (replaces LogTailer): Rust returns complete lines + an opaque cursor
{
    const got = []
    const acc = new M.LogAccumulator(l => got.push(l))
    const line = n => `2026-10-20 19:00:00,000 INFO [s] agent.conversation_loop: API call #${n}: model=m provider=p in=1 out=1 total=2 latency=1.0s`
    check('acc: first chunk applied, cursor kept', [acc.feed({ cursor: 'c1', lines: [line(1), line(2) + '\r'], rotated: false, error: null }), acc.cursor, acc.state.sessions.get('s').calls.length], [null, 'c1', 2])
    check('acc: CR stripped (CRLF logs)', got[1].endsWith('latency=1.0s'), true)
    acc.feed({ cursor: 'c2', lines: [line(3), line(4), line(5), line(6)], rotated: true, error: null })
    check('acc: rotation keeps the session state and order (old file rest first, then the new file)', [acc.state.sessions.get('s').calls.map(c => c.n), acc.stats.rotations, acc.cursor], [[1, 2, 3, 4, 5, 6], 1, 'c2'])
    check('acc: error passthrough keeps the last cursor when none is sent', [acc.feed({ cursor: null, lines: [], rotated: false, error: 'agent.log not found' }), acc.cursor], ['agent.log not found', 'c2'])
    check('acc: garbage chunk / lines never throw', [acc.feed(null), acc.feed({ cursor: 'c3', lines: [5, null, 'garbage'], rotated: false, error: null }), acc.stats.lines], ['agent.log n/a', null, 7])
}

// ---- session resolution
const entries = M.parseActiveSessions({ entries: [
    { pid: 23848, session_id: 'S_MAIN', surface: 'tui', started_at: 1792536650.31, metadata: { live_session_id: 'dbed35c5' } },
    { pid: 24560, session_id: 'S_GW', surface: 'gateway', started_at: 1792536700 },
    { pid: 'bad' }, null,
] })
check('active_sessions: parsed, bad entries dropped', entries.map(e => e.sessionId), ['S_MAIN', 'S_GW'])
const procs = [
    { pid: 100, ppid: 1, exe: 'Habby.exe' }, { pid: 200, ppid: 100, exe: 'OpenConsole.exe' }, { pid: 300, ppid: 200, exe: 'hermes.exe' },
    { pid: 23848, ppid: 300, exe: 'python.exe' }, { pid: 24560, ppid: 1, exe: 'python.exe' }, { pid: 0, ppid: 0, exe: '[System Process]' },
]
check('resolve: the session whose pid is under Habby', [M.resolveSession(entries, procs, 100).entry.sessionId, M.resolveSession(entries, procs, 100).matchedBy], ['S_MAIN', 'habby-child'])
check('resolve: another Habby (no child) -> newest live, tui first', [M.resolveSession(entries, procs, 999).entry.sessionId, M.resolveSession(entries, procs, 999).matchedBy], ['S_MAIN', 'newest-live'])
check('resolve: dead pids -> none', M.resolveSession(entries, [{ pid: 1, ppid: 0, exe: 'x' }], 999).matchedBy, 'none')
check('resolve: no process list (no koffi) -> newest tui entry', M.resolveSession(entries, null, 100).entry.sessionId, 'S_MAIN')

    check('resolve (island): the terminal child itself is an active_sessions pid', M.resolveIslandSession(entries, procs, 23848, 1).matchedBy, 'habby-child')
check('resolve (island): a descendant of the terminal child', M.resolveIslandSession(entries, procs, 300, 1).entry.sessionId, 'S_MAIN')
check('resolve (island): no terminal child -> island pid tree, then newest live', [M.resolveIslandSession(entries, procs, null, 100).matchedBy, M.resolveIslandSession(entries, procs, null, 999).matchedBy], ['habby-child', 'newest-live'])
check('resolve (island): no process list, exact pid match still wins', M.resolveIslandSession(M.parseActiveSessions({ entries: [{ pid: 7, session_id: 'GW', surface: 'tui', started_at: 9e9 }, { pid: 8, session_id: 'MINE', surface: 'gateway' }] }), null, 8, 1).entry.sessionId, 'MINE')

// ---- snapshot / plan: defensive
check('snapshot: garbage -> null / empty', [M.parseSnapshot(null), M.parseSnapshot('x'), M.parseSnapshot({}).sessions.size], [null, null, 0])
const snapRaw = {
    pid: 23848, written_at: Date.now() / 1000,
    sessions: {
        S_MAIN: {
            status: 'tool', model: 'claude-fable-5-1',
            turn: { started_at: Date.now() / 1000 - 30, calls: 3, input: 100, output: 2000, cache_read: 80000, cache_write: 5000, cost_usd: 0.5 },
            current_tool: { name: 'terminal', preview: 'git status\u0007\nmore', started_at: Date.now() / 1000 - 4 },
            steps: [{ tool: 'read_file', status: 'ok', ms: 40 }, { tool: 'terminal', status: 'running', ms: null }, { nope: 1 }],
            last_call: { in: 100, out: 300, cache_read: 80000, cache_write: 5000, prompt: 85100, ttfb_ms: 900, latency_ms: 4000, at: Date.now() / 1000 - 2 },
            context: { used: 85100, max: 1000000, pct: 8.51, estimated: false },
            waiting: null, compressions: 1,
            subagents: [{ id: 'S_CHILD1', goal: 'x'.repeat(200), role: 'coder', model: 'claude-opus-5-5', status: 'running', started_at: Date.now() / 1000 - 60, tools: 4, input: 1000, output: 500, cost_usd: 0.1 }],
            tools_by_name: { terminal: 9, read_file: 4, patch: 2 },
        },
        S_CHILD1: { parent_session_id: 'S_MAIN', role: 'coder', goal: 'Build the dock', status: 'tool', model: 'claude-opus-5-5', current_tool: { name: 'patch', preview: 'app/lib', started_at: Date.now() / 1000 - 1 }, session: { calls: 5, input: 10, output: 900, cache_read: 40000, cache_write: 3000, cost_usd: 0.04, tools: 4 } },
        S_CHILD2: { parent_session_id: 'S_MAIN', role: 'qa', status: 'idle', started_at: Date.now() / 1000 - 30 },
        S_OTHER: { status: 'thinking' },
        BAD: 5,
    },
}
const snap = M.parseSnapshot(snapRaw)
const sm = snap.sessions.get('S_MAIN')
check('snapshot: sessions (non-objects dropped)', [...snap.sessions.keys()], ['S_MAIN', 'S_CHILD1', 'S_CHILD2', 'S_OTHER'])
check('snapshot: preview control chars stripped', sm.currentTool.preview, 'git status more')
check('snapshot: bad steps dropped', sm.steps.map(x => x.status), ['ok', 'running'])
check('snapshot: goal truncated to 80', sm.subagents[0].goal.length, 80)
check('snapshot: tools_by_name sorted', sm.toolsByName.map(x => x.name), ['terminal', 'read_file', 'patch'])
check('snapshot: child session parent + totals', [snap.sessions.get('S_CHILD1').parentSessionId, snap.sessions.get('S_CHILD1').totals.output], ['S_MAIN', 900])
{
    // the exact shape documented by Coder B (hermes-plugin/habby-metrics/README.md)
    const b = M.parseSnapshot({ v: 1, pid: 1, written_at: new Date().toISOString(), plugin_version: '1.0.0', sessions: {
        P: { status: 'tool', model: 'claude-fable-5-1', platform: 'tui', parent_session_id: null, role: null, goal: null,
            turn: { started_at: new Date().toISOString(), ended_at: null, calls: 3, input: 30, output: 1200, cache_read: 260000, cache_write: 4000, cost_usd: 0.0912 },
            current_tool: { name: 'terminal', preview: 'git status --short', started_at: new Date().toISOString() },
            steps: [{ tool: 'read_file', status: 'ok', ms: 12 }],
            last_call: { in: 12, out: 345, cache_read: 88000, cache_write: 2100, prompt: 90112, ttfb_ms: 600, latency_ms: 2500, at: new Date().toISOString(), cost_usd: 0.035, finish_reason: 'tool_use' },
            context: { used: 90112, max: 1000000, pct: 9.0, estimated: false }, waiting: null,
            subagents: [{ id: 'sa-0', session_id: 'C', goal: 'g', role: 'leaf', model: 'claude-opus-5-5', status: 'running', started_at: new Date().toISOString(), ms: 4321, tools: 7, input: 250000, output: 3100, cost_usd: 0.41 }],
            compressions: 0,
            totals: { calls: 18, input: 44, output: 13641, cache_read: 877934, cache_write: 117175, cost_usd: 1.03, tool_calls: 17, tools_by_name: { terminal: 9, read_file: 8 } },
            first_seen: new Date(Date.now() - 60e3).toISOString(), updated_at: new Date().toISOString() },
        C: { status: 'thinking', model: 'claude-opus-5-5', platform: 'subagent', parent_session_id: 'P', role: 'leaf', goal: 'g', subagent_id: 'sa-0', totals: { calls: 2, input: 1, output: 2, cache_read: 3, cache_write: 4, cost_usd: 0.01, tool_calls: 1, tools_by_name: { patch: 1 } } },
    } })
    const p = b.sessions.get('P')
    check('snapshot (B contract): subagent id = child session id, tools_by_name from totals, first_seen', [p.subagents[0].id, p.toolsByName.map(x => x.n), p.totals.tools, p.startedAt !== null], ['C', [9, 8], 17, true])
    const vb = JSON.parse(JSON.stringify(M.buildView({ sessionId: 'P', matchedBy: 'habby-child', snapshot: b, snapshotError: null, plan: null, planError: 'x', db: null, dbError: 'x', log: null, logError: 'x', history: null, blocks: [] }, { now: Date.now(), cacheTtlSec: 300, contextMaxDefault: 1e6, page: 'live', pluginStaleSec: 120 })))
    check('view (B contract, no DB/log): tree = P + one child C (no duplicate sa-0 row)', [vb.agents.main.id, vb.agents.subs.map(x => [x.id, x.status, x.calls, x.toolsByName[0].name])], ['P', [['C', 'thinking', 2, 'patch']]])
    check('view (B contract, no DB/log): main totals from the plugin', [vb.agents.main.cost, vb.agents.main.calls, vb.stats.toolsByName[0].name, vb.totals.calls], [1.03, 18, 'terminal', 20])
}
const plan = M.parsePlan({ fetched_at: Date.now() / 1000 - 60, windows: [
    { label: 'Current session', used_percent: 91, reset_at: new Date(Date.now() + 2.5 * 3600e3).toISOString(), scope: 'five_hour' },
    { label: 'Current week', used_percent: 39, reset_at: new Date(Date.now() + 5 * 86400e3).toISOString() },
    { label: 'broken' },
], unavailable_reason: null })
check('plan: windows (bad dropped)', plan.windows.map(x => x.label), ['Current session', 'Current week'])
check('plan: window length', plan.windows.map(M.windowLengthMs), [5 * 3600e3, 7 * 86400e3])
const pace5 = M.planPace(plan.windows[0], Date.now())
ok('pace: 91 % after 2.5 of 5 h -> hits the limit before reset', pace5 && !pace5.onPace && pace5.hitAt < plan.windows[0].resetAt)
check('pace: 39 % after 2 of 7 days -> projected 136 % -> not on pace', M.planPace(plan.windows[1], Date.now()).onPace, false)
check('pace: 10 % after 2 of 7 days -> on pace', M.planPace({ ...plan.windows[1], pct: 10 }, Date.now()).onPace, true)
check('plan: garbage -> null', [M.parsePlan(null), M.parsePlan({}).windows.length], [null, 0])

// ---- alerts (X1)
const lv = (p) => M.planAlerts({ windows: [{ label: '5h', pct: p, resetAt: null, scope: null }] }).map(a => a.level + ':' + /≥ (\d+)/.exec(a.text)[1])
check('alerts: plan 80 / 90 / 95 thresholds', [lv(79), lv(80), lv(90), lv(96)], [[], ['warn:80'], ['crit:90'], ['crit:95']])
const base = { plan: null, contextPct: null, turnCost: null, priorTurnCosts: [], status: null, lastError: null, now: Date.now() }
check('alerts: context >= 85', M.buildAlerts({ ...base, contextPct: 86 }).length, 1)
check('alerts: cost spike > 5x median (needs >= 3 prior turns)', [M.buildAlerts({ ...base, turnCost: 1.1, priorTurnCosts: [0.2, 0.2, 0.2] }).length, M.buildAlerts({ ...base, turnCost: 0.9, priorTurnCosts: [0.2, 0.2, 0.2] }).length, M.buildAlerts({ ...base, turnCost: 9, priorTurnCosts: [0.2] }).length], [1, 0, 0])
check('alerts: rate limited in the last 5 min, not older', [M.buildAlerts({ ...base, lastError: { at: Date.now() - 1000, what: 'rate-limited' } })[0].level, M.buildAlerts({ ...base, lastError: { at: Date.now() - 400e3, what: 'rate-limited' } }).length], ['crit', 0])
check('alerts: error status', M.buildAlerts({ ...base, status: 'error' }).length, 1)

// ---- view: plugin-free run (DB + log only) must still fill the DB/log widgets
const now = Date.now()
const dbData = {
    live: { id: 'S_MAIN', title: 'Metrics build', model: 'claude-opus-5-5', calls: 20, input: 44, output: 13641, cacheRead: 877934, cacheWrite: 117175, cost: 1.03, tools: 17, startedAt: now - 3600e3, endedAt: null, lastActivityAt: now - 5e3, lastSeen: now - 5e3 },
    children: [
        { id: 'S_CHILD1', title: null, model: 'claude-opus-5-5', calls: 4, input: 10, output: 1294, cacheRead: 54879, cacheWrite: 24243, cost: 0.1581, tools: 3, startedAt: now - 120e3, endedAt: null, lastActivityAt: null, lastSeen: now - 10e3 },
        { id: 'S_OLD', title: 'old one', model: 'claude-opus-5-5', calls: 2, input: 1, output: 1, cacheRead: 1, cacheWrite: 1, cost: 0.01, tools: 0, startedAt: now - 7200e3, endedAt: now - 7000e3, lastActivityAt: null, lastSeen: now - 7000e3 },
    ],
    tasks: [{ task: '', model: 'claude-opus-5-5', calls: 16, cost: 0.85 }, { task: 'background_review', model: 'claude-opus-5-5', calls: 4, cost: 0.18 }],
    delegations: [{ id: 'd1', state: 'running', dispatchedAt: now - 120e3, completedAt: null, goals: ['Write tests'] }],
    newestTopLevel: null, toolsByName: [{ name: 'terminal', n: 9 }], turns: 6,
}
const logSt = new M.LogState()
const mk = (n, at, sid = 'S_MAIN') => `${fmtTs(at)} INFO [${sid}] agent.conversation_loop: API call #${n}: model=claude-opus-5-5 provider=anthropic in=${100000 + n * 5000} out=500 total=1 latency=4.0s cache=${99000 + n * 5000}/${100000 + n * 5000} (99%) write=1000 id=msg_${n}`
function fmtTs (t) { const d = new Date(t); const p = (x, n = 2) => String(x).padStart(n, '0'); return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())},${p(d.getMilliseconds(), 3)}` }
for (let i = 1; i <= 4; i++) logSt.apply(M.parseLogLine(mk(i, now - 60e3 + i * 5e3)))
logSt.apply(M.parseLogLine(mk(1, now - 8e3, 'S_CHILD1')))
const opts = { now, cacheTtlSec: 300, contextMaxDefault: 1000000, page: 'live', pluginStaleSec: 120 }
const inpFree = { sessionId: 'S_MAIN', matchedBy: 'habby-child', snapshot: null, snapshotError: 'plugin not installed', plan: null, planError: 'plugin not installed', db: dbData, dbError: null, log: logSt, logError: null, history: null, blocks: [] }
const v0 = M.buildView(inpFree, opts)
const json0 = JSON.parse(JSON.stringify(v0))
check('view (no plugin): sources', json0.sources, { plugin: 'plugin not installed', plan: 'plugin not installed', db: 'ok', log: 'ok' })
check('view (no plugin): status estimated from the log', [json0.header.status, json0.header.statusProv], ['thinking', 'estimate'])
check('view (no plugin): context from the last log prompt', [json0.context.used, json0.context.prov, +json0.context.pct.toFixed(1)], [120000, 'estimate', 12])
check('view (no plugin): calls-left estimate from growth', json0.context.callsLeft, Math.floor((1e6 - 120000) / 5000))
check('view (no plugin): session $ / tokens from state.db', [json0.session.cost, json0.session.tokens.cacheRead, json0.session.background.cost], [1.03, 877934, 0.18])
check('view (no plugin): plan = null (widget shows n/a)', json0.plan, null)
check('view (no plugin): agent tree main + running DB child; finished > 60 s hidden', [json0.agents.main.role, json0.agents.subs.map(x => [x.id, x.status, x.prov]), json0.agents.earlier], ['main agent', [['S_CHILD1', 'running', 'estimate']], 1])
check('view (no plugin): DB child tokens/cost', [json0.agents.subs[0].cost, json0.agents.subs[0].tokens.cacheWrite], [0.1581, 24243])
check('view (no plugin): totals strip (all agents)', [json0.totals.agents, json0.totals.active, +json0.totals.cost.toFixed(4), json0.totals.calls], [3, 2, 1.1981, 26])
check('view (no plugin): stats', [json0.stats.calls, json0.stats.turns, json0.stats.avgLatencyS, json0.stats.toolsByName[0].name], [20, 6, 4, 'terminal'])
check('view (no plugin): no breakdown without snapshot (X8 hidden)', json0.context.breakdown, null)
check('view: no innerHTML-worthy types (plain JSON round-trip)', JSON.stringify(v0).length > 100, true)

// ---- view with the plugin snapshot: R10 agent tree from child sessions
const v1 = JSON.parse(JSON.stringify(M.buildView({ ...inpFree, snapshot: snap, snapshotError: null, plan, planError: null }, opts)))
check('view (plugin): exact status + model', [v1.header.status, v1.header.statusProv, v1.header.model, v1.header.team], ['tool', 'exact', 'Fable 5.1', true])
check('view (plugin): activity line', v1.activity.text, 'Running terminal: git status more')
check('view (plugin): context exact', [v1.context.used, v1.context.prov], [85100, 'exact'])
check('view (plugin): X8 breakdown from last_call categories', v1.context.breakdown.map(x => x.label), ['cached (read)', 'cache write', 'uncached input'])
check('view (plugin): agents from snapshot child sessions + DB child merged', v1.agents.subs.map(x => [x.id, x.role, x.status]).sort(), [['S_CHILD1', 'coder', 'tool'], ['S_CHILD2', 'qa', 'idle']].sort())
const ch1 = v1.agents.subs.find(x => x.id === 'S_CHILD1')
check('view (plugin): child current tool + plugin totals win over DB', [ch1.tool.name, ch1.tokens.output, ch1.calls], ['patch', 900, 5])
check('view (plugin): main row now/turn/tools', [v1.agents.main.tool.name, v1.agents.main.turn.cost, v1.stats.toolsByName[0].n, v1.stats.ttfbMs, v1.stats.compressions], ['terminal', 0.5, 9, 900, 1])
check('view (plugin): plan alerts (91 % -> crit)', v1.alerts.map(a => a.level), ['crit'])
// stale snapshot -> back to log status
const stale = M.parseSnapshot({ ...snapRaw, written_at: now / 1000 - 3600 })
const v2 = M.buildView({ ...inpFree, snapshot: stale, snapshotError: null }, opts)
check('view (stale plugin): status falls back to the log', [v2.sources.plugin, v2.header.statusProv], ['stale', 'estimate'])
// empty everything
const v3 = JSON.parse(JSON.stringify(M.buildView({ sessionId: null, matchedBy: 'none', snapshot: null, snapshotError: 'n/a', plan: null, planError: 'n/a', db: null, dbError: 'state.db not found', log: null, logError: 'agent.log not found', history: null, blocks: [] }, opts)))
check('view (nothing): no crash, n/a everywhere', [v3.header.status, v3.agents.main, v3.session, v3.turn, v3.context.used], [null, null, null, null, null])

// ---- history + 5 h blocks
const today = (() => { const d = new Date(now); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}` })()
const hist = M.buildHistory({ days: [{ d: today, sessions: 3, tokens: 1000, cost: 2, subCost: 0.5 }], models: [{ d: today, model: 'claude-opus-5-5', cost: 1.5, tokens: 1 }, { d: today, model: 'claude-fable-5-1', cost: 0.5, tokens: 1 }], top: [{ id: 'a', title: 'T', cost: 2 }], savings: 12 }, [], now)
const wd = (new Date(now).getDay() + 6) % 7
check('history: today tile, heat 11 weeks + this week, models 14 days', [hist.today.cost, hist.heat.length, hist.models.length, hist.models[13].by], [2, 77 + wd + 1, 14, { opus: 1.5, fable: 0.5, sonnet: 0, haiku: 0, other: 0 }])
check('history: heat is weekday-aligned (last cell = today in its weekday row, 12 columns)', [hist.heat[hist.heat.length - 1].d, hist.heat[hist.heat.length - 1].row, hist.heat[hist.heat.length - 1].col, hist.heat[0].row], [today, wd, 11, 0])
{
    // calendar stepping across the DST change (2026-11-01, America/New_York): no skipped / duplicated day
    const t = new Date(2026, 10, 2, 0, 30).getTime()
    check('history: dayBack steps by calendar date over DST', [M.dayBack(t, 0), M.dayBack(t, 1), M.dayBack(t, 2)], ['2026-11-02', '2026-11-01', '2026-10-31'])
    const days = new Set(M.buildHistory({ days: [], models: [], top: [], savings: 0 }, [], t).heat.map(x => x.d))
    const hh = M.buildHistory({ days: [], models: [], top: [], savings: 0 }, [], t).heat
    check('history: heat has one cell per calendar day', days.size, hh.length)
}
check('history: subagent share', hist.subShare, 25)
let b = M.updateBlocks([], plan)
check('blocks: first 5 h window recorded', [b.changed, b.blocks.length, b.blocks[0].peak], [true, 1, 91])
b = M.updateBlocks(b.blocks, { windows: [{ ...plan.windows[0], pct: 95, resetAt: plan.windows[0].resetAt + 30e3 }] })
check('blocks: same window (reset jitter) keeps the peak', [b.blocks.length, b.blocks[0].peak], [1, 95])

// ---- reader over AgentLiveRaw (metrics.test.cjs reader section, plugin files part)
{
    const t = now / 1000
    const base = { islandPid: 1, terminalPid: 100, procs, activeSessions: null, snapshots: [], snapshotError: null, plan: null, planError: null }
    const raw = { ...base, activeSessions: { entries: [{ pid: 23848, session_id: 'S_MAIN', surface: 'tui', started_at: t }] } }
    let rr = M.readSessionAndPlugin({ ...raw, snapshotError: 'not found', planError: 'not found' })
    check('reader: no plugin dir -> not installed (footer reads "plugin not installed", QA nit 5)', [rr.sessionId, rr.matchedBy, rr.snapshot, rr.snapshotError, rr.planError], ['S_MAIN', 'habby-child', null, 'not installed', 'not installed'])
    rr = M.readSessionAndPlugin({ ...raw, snapshots: [{ pid: 23848, alive: true, mtimeMs: now, data: snapRaw }], plan: { windows: [{ label: 'Current session', used_percent: 17 }] } })
    check('reader: snapshot-<pid>.json by the active-session pid + plan.json', [rr.snapshotPid, rr.snapshot.sessions.has('S_MAIN'), rr.plan.windows[0].pct], [23848, true, 17])
    rr = M.readSessionAndPlugin({ ...raw, procs: [...procs, { pid: 777, ppid: 1, exe: 'python' }], snapshots: [{ pid: 777, alive: true, mtimeMs: now, data: snapRaw }] })
    check('reader: fallback = any snapshot that knows the session', rr.snapshotPid, 777)
    rr = M.readSessionAndPlugin({ ...raw, snapshots: [] })
    check('reader: torn snapshot (Rust drops files that do not parse) -> null, no throw', rr.snapshot, null)
    rr = M.readSessionAndPlugin({ ...base, activeSessions: null })
    check('reader: no active_sessions -> no session (no db-newest fallback over IPC)', [rr.sessionId, rr.matchedBy], [null, 'none'])
    // live Hermes 0.22: the island's TUI runs its agent in the gateway; active_sessions carries a routing key
    const chat = 'local-6ca0'
    const gw = { pid: 1176, session_id: `agent:main:local:dm:${chat}`, surface: 'gateway:local', started_at: t, metadata: { chat_id: chat, platform: 'local' } }
    const gwProcs = [...procs, { pid: 1176, ppid: 1, exe: 'python3' }]
    const gwSnap = { pid: 1176, written_at: t, sessions: { [chat]: { status: 'tool', model: 'claude-opus-5-5', platform: 'tui', updated_at: new Date(now).toISOString() } } }
    rr = M.readSessionAndPlugin({ ...base, procs: gwProcs, activeSessions: { entries: [gw] }, snapshots: [{ pid: 1176, alive: true, mtimeMs: now, data: gwSnap }] })
    check('reader: gateway:local entry -> chat_id session (state.db + snapshot key), gateway-local match', [rr.sessionId, rr.matchedBy, rr.snapshotPid, rr.snapshot.sessions.has(chat)], [chat, 'gateway-local', 1176, true])
    rr = M.readSessionAndPlugin({ ...base, procs: gwProcs, activeSessions: { entries: [gw] }, snapshots: [] })
    check('reader: gateway:local entry, no snapshot yet -> chat_id from the routing key', rr.sessionId, chat)
    rr = M.readSessionAndPlugin({ ...base, procs: gwProcs, activeSessions: { entries: [{ ...gw, session_id: 'opaque', metadata: { chat_id: '12345', platform: 'telegram' } }] }, snapshots: [] })
    check('reader: non-local chat_id never replaces session_id unless a snapshot knows it', rr.sessionId, 'opaque')
    const old = { pid: 1176, written_at: t, sessions: {
        SUB: { platform: 'subagent', updated_at: new Date(now).toISOString() },
        OLD: { platform: 'tui', updated_at: new Date(now - 60e3).toISOString() },
        ROT: { platform: 'tui', updated_at: new Date(now - 1e3).toISOString(), continued_to: 'NEW' },
        NEWEST: { platform: 'tui', updated_at: new Date(now - 5e3).toISOString() },
    } }
    rr = M.readSessionAndPlugin({ ...base, procs: gwProcs, activeSessions: null, snapshots: [{ pid: 1176, alive: true, mtimeMs: now, data: old }] })
    check('reader: no active_sessions -> most recent top-level snapshot session (no subagents, no rotated)', [rr.sessionId, rr.matchedBy, rr.snapshotPid], ['NEWEST', 'snapshot-newest', 1176])
    rr = M.readSessionAndPlugin({ ...base, activeSessions: null, snapshots: [{ pid: 4242, alive: false, mtimeMs: now, data: old }] })
    check('reader: snapshot of a dead process is not a fallback', [rr.sessionId, rr.matchedBy], [null, 'none'])
}

// ======== metrics-fixes.test.cjs ========
{
const now = Date.now()
const iso = t => new Date(t).toISOString()
const opts = { now, cacheTtlSec: 300, contextMaxDefault: 1e6, page: 'live', pluginStaleSec: 120 }
const base = { sessionId: 'S', matchedBy: 'habby-child', snapshot: null, snapshotError: null, plan: null, planError: null, db: null, dbError: 'x', log: null, logError: 'x', history: null, blocks: [] }
const view = (inp, o = {}) => JSON.parse(JSON.stringify(M.buildView({ ...base, ...inp }, { ...opts, ...o })))

// ---- M1: a waiting snapshot 130 s old stays "waiting" while its Hermes process lives ------------------
const waitingSnap = M.parseSnapshot({ pid: 4242, written_at: iso(now - 130e3), sessions: { S: {
    status: 'waiting', model: 'claude-opus-5-5', waiting: { kind: 'approval', since: iso(now - 131e3) },
    turn: { started_at: iso(now - 140e3), calls: 1, input: 1, output: 1, cache_read: 0, cache_write: 0, cost_usd: 0.01 },
    subagents: [], steps: [],
} } })
const w1 = view({ snapshot: waitingSnap, snapshotPidAlive: true })
check('M1: t+130 s, pid alive -> still "Waiting for your approval" (exact)', [w1.header.status, w1.header.statusProv, w1.activity.text, w1.agents.main.waiting.kind, w1.sources.plugin], ['waiting', 'exact', 'Waiting for your approval', 'approval', 'ok'])
const w2 = view({ snapshot: waitingSnap, snapshotPidAlive: false })
check('M1: pid gone -> stale', [w2.sources.plugin, w2.agents.main.waiting], ['stale', null])
const w3 = view({ snapshot: waitingSnap, snapshotPidAlive: null })
check('M1: no process list -> age rule (130 s > 120 s = stale)', w3.sources.plugin, 'stale')
const w4 = view({ snapshot: M.parseSnapshot({ pid: 1, written_at: iso(now - 30e3), sessions: { S: { status: 'waiting', waiting: { kind: 'approval' } } } }), snapshotPidAlive: null })
check('M1: no process list, heartbeat 30 s ago -> fresh', w4.header.status, 'waiting')

// ---- M5: stale / past-reset plan ------------------------------------------------------------------
const plan = M.parsePlan({ fetched_at: iso(now - 60e3), windows: [
    { label: 'Current session', used_percent: 96, reset_at: iso(now - 10 * 60e3) },
    { label: 'Current week', used_percent: 50, reset_at: iso(now + 3 * 86400e3) },
], details: ['Extra usage: 12.40 / 50.00 USD', 7, ''] })
const p1 = view({ plan })
check('M5: past reset -> expired, no pace, no marker; live window keeps both', p1.plan.windows.map(w => [w.expired, w.pace === null, w.marker === null]), [[true, true, true], [false, false, false]])
check('M5: no crit banner for a window that already reset', p1.alerts.filter(a => /Current session/.test(a.text)).length, 0)
check('QA m4: details[] (extra-usage credits) parsed, junk dropped', p1.plan.details, ['Extra usage: 12.40 / 50.00 USD'])
ok('QA m4: pace marker = elapsed fraction of the 7-day window', Math.abs(p1.plan.windows[1].marker - 4 / 7) < 1e-6, p1.plan.windows[1].marker)
const oldPlan = M.parsePlan({ fetched_at: iso(now - 20 * 60e3), windows: [{ label: 'Current session', used_percent: 96, reset_at: iso(now + 3600e3) }], unavailable_reason: 'fetch failed' })
const p2 = view({ plan: oldPlan })
check('M5: data older than 15 min (backoff kept the windows) -> stale, no alerts, no pace', [p2.plan.stale, p2.alerts.length, p2.plan.windows[0].pace], [true, 0, null])
check('M5: fresh plan still alerts', view({ plan: M.parsePlan({ fetched_at: iso(now - 60e3), windows: [{ label: 'Current session', used_percent: 96, reset_at: iso(now + 3600e3) }] }) }).alerts.map(a => a.level), ['crit'])
{
    // pace from fetchedAt: 40 % measured 1 h after the window opened, read 2 h later
    const w = { label: 'Current session', pct: 40, resetAt: now + 2 * 3600e3, scope: null }
    const fetched = now - 2 * 3600e3 // window opened at now-3h; at fetch 1 h had elapsed
    const pace = M.planPace(w, now, fetched)
    ok('M5: pace uses the rate at fetch time (40 %/h -> 100 % 1.5 h after the fetch)', pace && !pace.onPace && Math.abs(pace.hitAt - (fetched + 1.5 * 3600e3)) < 1000, pace && (pace.hitAt - fetched) / 3600e3)
    const naive = M.planPace(w, now)
    ok('M5: the old now-based projection was optimistic', naive && naive.onPace === true)
    check('M5: past reset -> no pace', M.planPace({ ...w, resetAt: now - 1 }, now, fetched), null)
}

// ---- QA m1 / nit 6 / minor 15: subagent rows -------------------------------------------------------
{
    const sn = M.parseSnapshot({ pid: 1, written_at: iso(now), sessions: {
        S: { status: 'tool', model: 'claude-fable-5-1', subagents: [{ id: 'C', session_id: 'C', role: 'leaf', status: 'running', started_at: iso(now - 5e3), input: 3, output: 4, cache_read: 5, cache_write: 6 }] },
        C: { parent_session_id: 'S', role: 'leaf', status: 'waiting', waiting: { kind: 'clarify', since: iso(now - 2e3) }, goal: 'Audit' },
    } })
    const db = { live: null, children: [{ id: 'D', title: null, model: 'claude-opus-5-5', calls: 1, input: 1, output: 1, cacheRead: 1, cacheWrite: 1, cost: 0.1, tools: 0, startedAt: now - 1000, endedAt: null, lastActivityAt: null, lastSeen: now }], tasks: [], delegations: [{ id: 'd', state: 'running', dispatchedAt: now, completedAt: null, goals: ['WRONG GOAL'] }], newestTopLevel: null, toolsByName: [], turns: null }
    const v = view({ snapshot: sn, snapshotPidAlive: true, db })
    const c = v.agents.subs.find(x => x.id === 'C')
    check('QA m1: subagent row carries its waiting kind', [c.waiting && c.waiting.kind, c.status], ['clarify', 'waiting'])
    check('QA nit 6: "leaf" shown as "subagent"', c.role, 'subagent')
    check('minor 15: no delegation goal attached by index', v.agents.subs.find(x => x.id === 'D').goal, null)
    const sn2 = M.parseSnapshot({ pid: 1, written_at: iso(now), sessions: { S: { status: 'tool', subagents: [{ id: 'Z', status: 'running', input: 3, output: 4, cache_read: 5, cache_write: 6 }] } } })
    check('minor 1: index-row tokens keep cache read/write', view({ snapshot: sn2, snapshotPidAlive: true }).agents.subs[0].tokens, { input: 3, output: 4, cacheRead: 5, cacheWrite: 6 })
}

// ---- QA m5, minors 3/4/13: context + turn numbers from one source -------------------------------------
{
    const st = new M.LogState()
    const ts = t => { const d = new Date(t); const p = (x, n = 2) => String(x).padStart(n, '0'); return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())},${p(d.getMilliseconds(), 3)}` }
    for (let i = 0; i < 5; i++) st.apply(M.parseLogLine(`${ts(now - 50e3 + i * 1000)} INFO [S] agent.conversation_loop: API call #${i}: model=claude-opus-5-5 provider=anthropic in=${10000 + i * 100} out=10 total=1 latency=1.0s`))
    st.apply(M.parseLogLine(`${ts(now - 60e3)} INFO [OTHER] agent.context_compressor: Context compressor initialized: model=x context_length=200000 threshold=0.8`))
    const sn = M.parseSnapshot({ pid: 1, written_at: iso(now), sessions: { S: {
        status: 'thinking', model: 'claude-opus-5-5', context: { used: 500000, max: 1000000, pct: 50 },
        prompt_history: [400000, 450000, 500000],
        turn: { started_at: iso(now - 9e3), calls: 2, input: 10, output: 2000, cache_read: 90000, cache_write: 1000, cost_usd: 0.3 },
        last_call: { in: 5, out: 1000, cache_read: 1, cache_write: 1, prompt: 500000, ttfb_ms: 1150, latency_ms: 3150 },
    } } })
    const v = view({ snapshot: sn, snapshotPidAlive: true, log: st, logError: null })
    check('QA m5: calls left from the snapshot prompt history only ((1M - 500k) / 50k)', v.context.callsLeft, 10)
    const vlog = view({ log: st, logError: null })
    check('QA m5: without a fresh snapshot, calls left from agent.log only ((1M - 10.4k) / 100)', vlog.context.callsLeft, Math.floor((1e6 - 10400) / 100))
    check('minor 13: another session\'s 200k context length is not used', vlog.context.max, 1e6)
    check('minor 3: "This turn input" = prompt (uncached + cache read + cache write)', v.turn.input, 91010)
    check('minor 4: generation tok/s = out / (latency - TTFB)', [Math.round(v.turn.tps), v.turn.tpsGen, v.turn.ttfbMs], [500, true, 1150])
}

// ---- minor 14: only the live session's own retry lines raise the rate-limit banner ----------------------
{
    const L = '2026-10-20 19:33:00,000 WARNING [S] agent.conversation_loop: API call failed (attempt 1/3) error_type=RateLimitError HTTP 429'
    const aux = '2026-10-20 19:33:00,000 WARNING [S] agent.auxiliary_client: rate limit on provider x, cooling down'
    const pool = '2026-10-20 19:33:00,000 WARNING agent.credential_pool: credential cooldown after rate limit'
    check('minor 14: conversation-loop retry parsed; aux / pool warnings ignored', [M.parseLogLine(L).what, M.parseLogLine(aux), M.parseLogLine(pool)], ['rate-limited', null, null])
    const st = new M.LogState()
    st.apply({ kind: 'error', sid: 'OTHER', at: now - 1000, what: 'rate-limited' })
    check('minor 14: another session\'s retry raises no banner here', view({ log: st, logError: null }).alerts.length, 0)
}

// ---- nits / minors on the model ------------------------------------------------------------------
check('nit 6: no process list -> unverified', M.resolveSession(M.parseActiveSessions({ entries: [{ pid: 9, session_id: 'A' }] }), null, 1).matchedBy, 'unverified')
{
    const db = { live: { id: 'S', title: 't', model: 'claude-opus-5-5', calls: 1, input: 1, output: 1, cacheRead: 1, cacheWrite: 1, cost: 1, tools: 0, startedAt: now - 1e3, endedAt: null, lastActivityAt: null, lastSeen: now },
        children: [], tasks: [{ task: 'background_review', model: 'claude-opus-5-5', calls: 2, cost: 0.18 }, { task: 'background_review', model: 'claude-fable-5-1', calls: 3, cost: 1.37 }, { task: '', model: 'x', calls: 1, cost: 1 }],
        delegations: [], newestTopLevel: null, toolsByName: [], turns: null, chain: ['R', 'S'] }
    const v = view({ db, dbError: null })
    check('QA nit 4: background tasks grouped by task', v.session.background.tasks.map(t => [t.task, +t.cost.toFixed(2), t.calls]), [['background_review', 1.55, 5]])
    check('M3: session card notes the compaction chain', v.session.continuations, 1)
    check('M4: db busy shows in the footer source', view({ db, dbError: null, dbBusy: true }).sources.db, 'db busy')
    const sn = M.parseSnapshot({ pid: 1, written_at: iso(now), sessions: {
        OLD: { status: 'idle', continued_to: 'S' },
        S: { status: 'tool', continued_from: 'OLD', model: 'claude-opus-5-5', background: { calls: 2, cost_usd: 0.04, tool_calls: 1 }, compressions: 1 },
    } })
    const v2 = view({ sessionId: 'OLD', snapshot: sn, snapshotPidAlive: true })
    check('M3: snapshot lookup follows continued_to (before the TUI re-anchors)', [v2.header.status, v2.context.compressions], ['tool', 1])
    check('M2: background-review bucket on the main row', v2.agents.main.background, { calls: 2, cost: 0.04, tools: 1 })
    const v3 = view({ db, dbError: null, bgProcs: [{ sessionId: 'R', name: 'node', startedAt: now, alive: true }, { sessionId: 'S', name: 'python', startedAt: now, alive: false }, { sessionId: 'X', name: 'ruby', startedAt: now, alive: null }] })
    check('QA m4: background processes under the agent that started them (chain-aware, dead ones dropped)', v3.agents.main.procs.map(p => p.name), ['node'])
}

// ---- reader: processes.json + snapshot liveness (metrics-fixes reader section, over AgentLiveRaw data)
{
    const t = now / 1000
    const pj = [
        { session_id: 'proc_1', command: '"/usr/lib/node_modules/bin/node" server.js --token SECRET', pid: 111, pid_scope: 'host', started_at: t - 60, parent_session_id: 'CONT' },
        { session_id: 'proc_2', command: 'python -m http.server 8000', pid: 222, pid_scope: 'host', started_at: t - 30, parent_session_id: 'SUB2' },
        { session_id: 'proc_3', command: 'sleep 5', pid: 333, parent_session_id: '' },
    ]
    const bp = M.parseBgProcs(pj, [{ pid: 111, ppid: 1, exe: 'node' }])
    check('QA m4: processes.json -> program names only, liveness from the process list', bp.map(p => [p.sessionId, p.name, p.alive]), [['CONT', 'node', true], ['SUB2', 'python', false]])
    ok('QA m4: no arguments/paths kept', !JSON.stringify(bp).includes('SECRET') && !JSON.stringify(bp).includes('node_modules'))
    check('QA m4: missing/garbage processes.json -> []', [M.parseBgProcs(null, null).length, M.parseBgProcs({}, null).length], [0, 0])
    const sn = (pid, at) => ({ pid, written_at: iso(at), sessions: { CONT: { status: 'tool' } } })
    const files = [{ pid: 500, alive: false, mtimeMs: now - 1000, data: sn(500, now - 1000) }, { pid: 600, alive: true, mtimeMs: now - 5000, data: sn(600, now - 5000) }]
    const raw = { islandPid: 1, terminalPid: null, procs: [{ pid: 600, ppid: 1, exe: 'python' }, { pid: 900, ppid: 1, exe: 'hermes' }], activeSessions: { entries: [{ pid: 900, session_id: 'CONT', surface: 'tui' }] }, snapshots: files, snapshotError: null, plan: null, planError: null }
    const rr = M.readSessionAndPlugin(raw)
    check('minor 9: fallback ignores snapshots of dead pids; pid alive reported', [rr.snapshotPid, rr.snapshotPidAlive], [600, true])
    const rr2 = M.readSessionAndPlugin({ ...raw, procs: null, activeSessions: { entries: [{ pid: 500, session_id: 'CONT', surface: 'tui' }] } })
    check('M1: no process list -> entry unverified, pid liveness unknown (age rule applies)', [rr2.snapshotPid, rr2.snapshotPidAlive, rr2.matchedBy], [500, null, 'unverified'])
    const rr3 = M.readSessionAndPlugin({ ...raw, islandPid: 1, procs: [{ pid: 1, ppid: 0, exe: 'hermes-island' }, { pid: 500, ppid: 1, exe: 'python' }], activeSessions: { entries: [{ pid: 500, session_id: 'CONT', surface: 'tui' }] } })
    check('M1: the island child\'s snapshot with its pid alive -> fresh by pid', [rr3.matchedBy, rr3.snapshotPidAlive], ['habby-child', true])
}
}
