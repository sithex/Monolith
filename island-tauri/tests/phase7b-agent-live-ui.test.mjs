// AI Agents Live | Usage: AgentLiveFeed (port of Habby metrics.ts collect over agent_live_raw), the React
// cards, and the poll loop (only while shown; Live 1 s busy / 5 s idle, Usage 60 s).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import * as React from 'react';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
const M = load('ui/views/agent-live-model.ts');
const { AgentLivePage } = load('ui/island/components/AgentLive.tsx');
const { AgentLiveUsageCards, TopSessionsCard } = load('ui/island/components/AgentLiveUsage.tsx');
const src = p => readFileSync(new URL(`../${p}`, import.meta.url), 'utf8');

const now = Date.now();
const fmtTs = t => { const d = new Date(t); const p = (x, n = 2) => String(x).padStart(n, '0'); return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())},${p(d.getMilliseconds(), 3)}`; };
const call = (n, at, sid = 'S_MAIN') => `${fmtTs(at)} INFO [${sid}] agent.conversation_loop: API call #${n}: model=claude-opus-5-5 provider=anthropic in=${100000 + n * 5000} out=500 total=1 latency=4.0s cache=${99000 + n * 5000}/${100000 + n * 5000} (99%) write=1000 id=msg_${n}`;
const db = (cost = 1.03) => ({
  live: { id: 'S_MAIN', title: 'Metrics build', model: 'claude-opus-5-5', calls: 20, input: 44, output: 13641, cacheRead: 877934, cacheWrite: 117175, cost, tools: 17, startedAt: now - 3600e3, endedAt: null, lastActivityAt: now - 5e3, lastSeen: now - 5e3 },
  children: [{ id: 'S_CHILD1', title: 'Write tests', model: 'claude-opus-5-5', calls: 4, input: 10, output: 1294, cacheRead: 54879, cacheWrite: 24243, cost: 0.1581, tools: 3, startedAt: now - 120e3, endedAt: null, lastActivityAt: null, lastSeen: now - 10e3 }],
  tasks: [{ task: 'background_review', model: 'claude-opus-5-5', calls: 4, cost: 0.18 }], delegations: [], newestTopLevel: null, toolsByName: [{ name: 'terminal', n: 9 }], turns: 6,
});
const plan = { fetched_at: now / 1000 - 60, windows: [{ label: 'Current session', used_percent: 91, reset_at: new Date(now + 2.5 * 3600e3).toISOString(), scope: 'five_hour' }] };
const history = { days: [], models: [], top: [{ id: 'S_TOP', title: 'Big refactor', cost: 4.2, segments: 2 }, { id: 'S_TWO', title: null, cost: 1 }], savings: 12 };
function rawFor(req, over = {}) {
  return {
    now, hermesHome: '/fixture/.hermes', islandPid: 10, terminalPid: 20,
    procs: [{ pid: 10, ppid: 1, exe: 'hermes-island' }, { pid: 20, ppid: 10, exe: 'hermes' }, { pid: 21, ppid: 20, exe: 'python3' }, { pid: 99, ppid: 1, exe: 'python3' }],
    activeSessions: { entries: [{ pid: 99, session_id: 'S_OTHER', surface: 'tui', started_at: now / 1000 }, { pid: 21, session_id: 'S_MAIN', surface: 'tui', started_at: now / 1000 - 600 }] },
    activeSessionsError: null, processes: [{ command: 'npm run dev', pid: 21, parent_session_id: 'S_MAIN' }],
    snapshots: [], snapshotError: 'not found', plan, planError: null,
    log: { cursor: `c${(Number(req.logCursor?.slice(1)) || 0) + 1}`, lines: [], rotated: false, error: null },
    db: req.sessionId === 'S_MAIN' ? db() : null, dbError: req.sessionId ? null : 'no session', dbBusy: false,
    history: req.history ? history : null, historyError: null, ...over,
  };
}
function fakeDeps(script = () => ({})) {
  const calls = [], blockOps = [];
  return { calls, blockOps, deps: {
    raw: async req => { calls.push(req); return rawFor(req, script(req, calls.length)); },
    blocks: async op => { blockOps.push(op); return { blocks: op.type === 'get' ? [{ resetAt: now - 8 * 3600e3, peak: 40 }] : op.blocks }; },
  } };
}

test('feed: resolves the island terminal child session, re-requests once for it, accumulates the log', async () => {
  const f = fakeDeps((req, n) => ({ log: { cursor: `c${n}`, lines: n === 1 ? [call(1, now - 40e3), call(2, now - 30e3)] : [call(3, now - 20e3)], rotated: false, error: null } }));
  const feed = new M.AgentLiveFeed(f.deps);
  const r = await feed.tick('live');
  assert.deepEqual(f.calls, [{ sessionId: null, logCursor: null, history: false }, { sessionId: 'S_MAIN', logCursor: 'c1', history: false }]);
  assert.equal(r.requests, 2);
  assert.equal(r.view.who.id, 'S_MAIN'); assert.equal(r.view.who.matchedBy, 'habby-child');
  assert.equal(r.view.session.cost, 1.03);
  assert.equal(feed.log.state.sessions.get('S_MAIN').calls.length, 3);
  assert.equal(r.view.header.status, 'thinking'); assert.equal(r.active, true);
  assert.deepEqual(r.view.agents.main.procs.map(p => p.name), ['npm']);
  assert.equal(r.view.sources.plugin, 'not installed');
  await feed.tick('live');
  assert.deepEqual(f.calls[2], { sessionId: 'S_MAIN', logCursor: 'c2', history: false }, 'same session: one request per tick');
});

test('feed: db busy keeps the last good rows (M4); a session switch drops them', async () => {
  let busy = false, mine = true;
  const f = fakeDeps(req => ({
    ...(busy && req.sessionId === 'S_MAIN' ? { db: null, dbBusy: true } : {}),
    ...(mine ? {} : { activeSessions: { entries: [{ pid: 99, session_id: 'S_OTHER', surface: 'tui' }] } }),
  }));
  const feed = new M.AgentLiveFeed(f.deps);
  await feed.tick('live');
  busy = true;
  let r = await feed.tick('live');
  assert.equal(r.view.sources.db, 'db busy'); assert.equal(r.view.session.cost, 1.03);
  mine = false;
  r = await feed.tick('live');
  assert.equal(r.view.who.id, 'S_OTHER'); assert.equal(r.view.who.matchedBy, 'newest-live');
  assert.equal(r.view.session, null, 'no state.db rows for the new session, none borrowed from the old one');
});

test('feed: 5-hour blocks load once, record the plan peak and persist through agent_live_blocks', async () => {
  const f = fakeDeps();
  const feed = new M.AgentLiveFeed(f.deps);
  await feed.tick('live'); await feed.tick('live');
  assert.equal(f.blockOps.filter(o => o.type === 'get').length, 1);
  const sets = f.blockOps.filter(o => o.type === 'set');
  assert.equal(sets.length, 1, 'unchanged peak is not rewritten');
  assert.deepEqual(sets[0].blocks.map(b => b.peak), [40, 91]);
  const broken = new M.AgentLiveFeed({ ...f.deps, blocks: async () => { throw Error('command agent_live_blocks not found'); } });
  const r = await broken.tick('live');
  assert.match(broken.blocksError, /not found/); assert.equal(r.view.who.id, 'S_MAIN');
});

test('feed: Usage page asks for history and keeps it between polls; cadence follows Habby', async () => {
  const f = fakeDeps((req, n) => n > 2 ? { history: null, historyError: null } : {});
  const feed = new M.AgentLiveFeed(f.deps);
  let r = await feed.tick('history');
  assert.equal(f.calls[0].history, true);
  assert.equal(r.view.history.top[0].title, 'Big refactor'); assert.equal(r.history.top[0].id, 'S_TOP');
  r = await feed.tick('history');
  assert.equal(r.view.history.top.length, 2, 'cached history survives a poll without one');
  assert.deepEqual([M.nextDelay('live', true), M.nextDelay('live', false), M.nextDelay('history', true)], [1000, 5000, 60000]);
  await assert.rejects(new M.AgentLiveFeed({ raw: async () => { throw Error('unknown command agent_live_raw'); }, blocks: f.deps.blocks }).tick('live'), /unknown command/);
});

const liveView = async (over = {}) => {
  const snap = { pid: 21, written_at: now / 1000, sessions: { S_MAIN: {
    status: 'tool', model: 'claude-fable-5-1', turn: { started_at: now / 1000 - 30, calls: 3, input: 100, output: 2000, cache_read: 80000, cache_write: 5000, cost_usd: 0.5 },
    current_tool: { name: 'terminal', preview: 'git status', started_at: now / 1000 - 4 }, steps: [{ tool: 'read_file', status: 'ok', ms: 40 }, { tool: 'terminal', status: 'running' }],
    last_call: { in: 100, out: 300, cache_read: 80000, cache_write: 5000, prompt: 85100, ttfb_ms: 900, latency_ms: 4000, at: now / 1000 - 2 },
    context: { used: 870000, max: 1000000, pct: 87, estimated: false }, compressions: 1,
    subagents: [{ id: 'S_CHILD1', goal: 'Write tests', role: 'coder', model: 'claude-opus-5-5', status: 'running', started_at: now / 1000 - 60, tools: 4, input: 1000, output: 500, cost_usd: 0.1 }],
  } } };
  const f = fakeDeps(() => ({ snapshots: [{ pid: 21, alive: true, mtimeMs: now, data: snap }], snapshotError: null, ...over }));
  return (await new M.AgentLiveFeed(f.deps).tick('live')).view;
};

test('Live cards: header, alerts, activity, context, cache, turn, session, agents as React text', async () => {
  const view = await liveView();
  const html = renderToStaticMarkup(createElement(AgentLivePage, { view, now, onCopy() {} }));
  for (const t of ['Activity', 'Context', 'Cache', 'This turn', 'Session', 'Agents', 'Fable 5.1', 'TEAM', 'running tool', 'Running terminal: git status', 'Metrics build', 'Write tests', 'read_file', '870k / 1.00M', 'background: npm', 'warm', '$1.03'])
    assert.ok(html.includes(t), `missing ${t}`);
  assert.match(html, /Context 87 % full: compaction soon/);
  assert.match(html, /Current session: 91 % of plan used/);
  assert.match(html, /role="progressbar"/);
  assert.doesNotMatch(html, /NaN|undefined|\[object Object\]/);
});

test('Live cards: nothing resolved / load error render plain placeholders', async () => {
  const empty = M.buildView({ sessionId: null, matchedBy: 'none', snapshot: null, snapshotError: 'n/a', plan: null, planError: 'n/a', db: null, dbError: 'x', log: null, logError: 'agent.log not found', history: null, blocks: [] }, { now, ...M.VIEW_DEFAULTS, page: 'live' });
  const html = renderToStaticMarkup(createElement(AgentLivePage, { view: JSON.parse(JSON.stringify(empty)), now, onCopy() {} }));
  assert.match(html, /No live Hermes session found/); assert.match(html, /No live session/); assert.match(html, /log agent.log not found/);
  assert.doesNotMatch(html, /NaN|undefined/);
  assert.match(renderToStaticMarkup(createElement(AgentLivePage, { view: null, now, onCopy() {}, error: 'unknown command' })), /Live data unavailable: unknown command/);
});

test('Usage cards: 5-hour blocks and top sessions; a click copies the raw session id', async () => {
  const f = fakeDeps();
  const r = await new M.AgentLiveFeed(f.deps).tick('history');
  const ids = r.history.top.map(t => t.id);
  const html = renderToStaticMarkup(createElement(AgentLiveUsageCards, { history: r.view.history, ids, now, onCopy() {} }));
  assert.equal((html.match(/class="ai-bar-slot"/g) || []).length, 2);
  assert.match(html, /5-hour windows/); assert.match(html, /Top sessions this week/); assert.match(html, /Big refactor \(2 segments\)/); assert.match(html, /\$4\.20/);
  const copied = [];
  const walk = (n, out = []) => { if (!n || typeof n !== 'object') return out; if (Array.isArray(n)) { n.forEach(x => walk(x, out)); return out; } if (typeof n.type === 'function') return walk(n.type(n.props), out); out.push(n); walk(n.props?.children, out); return out; };
  walk(TopSessionsCard({ history: r.view.history, ids, onCopy: id => copied.push(id) })).filter(n => n.type === 'button').forEach(b => b.props.onClick());
  assert.deepEqual(copied, ['S_TOP', 'S_TWO']);
  assert.match(renderToStaticMarkup(createElement(AgentLiveUsageCards, { history: null, ids: [], now, onCopy() {} })), /Collecting/);
});

test('React text only, clipboard fallback, Live default with a Live | Usage toggle', () => {
  for (const p of ['ui/island/components/AgentLive.tsx', 'ui/island/components/AgentLiveUsage.tsx', 'ui/island/components/AgentLiveParts.tsx', 'ui/views/AgentLiveView.tsx', 'ui/views/AiStatsView.tsx'])
    assert.doesNotMatch(src(p), /dangerouslySetInnerHTML|innerHTML/, p);
  const ipc = src('ui/views/agent-live-ipc.ts');
  assert.match(ipc, /navigator\.clipboard/); assert.match(ipc, /execCommand\('copy'\)/);
  assert.match(ipc, /agentLiveRaw, agentLiveBlocks.*from '..\/ipc\/client'/);
  assert.doesNotMatch(ipc, /@tauri-apps\/api|invoke</);
  const view = src('ui/views/AiStatsView.tsx');
  assert.match(view, /useState<'live' \| 'usage'>\('live'\)/); assert.match(view, /<AgentLiveView active=\{active\}\/>/); assert.match(view, /<AgentLiveUsage active=\{active\}\/>/);
});

test('missing plugin is a stable informational empty state, with usage still available', async () => {
  const f = fakeDeps(() => ({ snapshots: [], snapshotError: 'not found', plan: null, planError: 'not found' }));
  const feed = new M.AgentLiveFeed(f.deps);
  for (let i = 0; i < 3; i++) {
    const r = await feed.tick('live');
    const html = renderToStaticMarkup(createElement(AgentLivePage, { view: r.view, now, onCopy() {} }));
    assert.match(html, /Live metrics plugin not installed/);
    assert.match(html, /installation is optional/);
    assert.doesNotMatch(html, /role="alert"|refresh failed|Live data unavailable|NaN|undefined/);
  }
  const usage = await feed.tick('history');
  assert.equal(usage.view.history.top.length, 2, 'state.db history needs no plugin');
  const empty = renderToStaticMarkup(createElement(AgentLiveUsageCards, { history: null, ids: [], now, onCopy() {}, pluginMissing: true, loaded: true }));
  assert.match(empty, /habby-metrics is not installed/);
  assert.doesNotMatch(empty, /Loading/);
  const saved = renderToStaticMarkup(createElement(AgentLiveUsageCards, { history: null, ids: [], now, onCopy() {}, blocks: [{ resetAt: now, peak: 42 }], loaded: true }));
  assert.match(saved, /max 42/);
});

test('hiding during block load or session discovery prevents chained IPC and persistence', async () => {
  let visible = true, release;
  const f = fakeDeps();
  const feed = new M.AgentLiveFeed({ ...f.deps, blocks: op => new Promise(resolve => { release = () => resolve({ blocks: [] }); }) });
  const pending = feed.tick('live', () => visible);
  visible = false; release();
  await assert.rejects(pending, /cancelled/);
  assert.equal(f.calls.length, 0, 'hidden before blocks loaded: no raw read');

  visible = true;
  const g = fakeDeps();
  const second = new M.AgentLiveFeed({ ...g.deps, raw: req => new Promise(resolve => { g.calls.push(req); release = () => resolve(rawFor(req)); }) });
  const read = second.tick('live', () => visible);
  await new Promise(resolve => setImmediate(resolve));
  visible = false; release();
  await assert.rejects(read, /cancelled/);
  assert.equal(g.calls.length, 1, 'no second read for the newly discovered session');
  assert.equal(g.blockOps.filter(op => op.type === 'set').length, 0, 'no hidden history write');
});

test('Live and Usage tabs gate activity and preserve the existing Overview / Sessions component', () => {
  const walk = (node, out = []) => {
    if (!node || typeof node !== 'object') return out;
    if (Array.isArray(node)) { node.forEach(child => walk(child, out)); return out; }
    out.push(node); walk(node.props?.children, out); return out;
  };
  const slots = []; let index = 0;
  const hooks = { ...React, useState(initial) { const i = index++; slots[i] ??= { value: initial }; return [slots[i].value, value => { slots[i].value = value; }]; } };
  const AgentLiveView = () => null, AgentLiveUsage = () => null;
  const require = name => {
    if (name === 'react') return hooks;
    if (name === 'react/jsx-runtime') return { jsx: React.createElement, jsxs: React.createElement };
    if (name === './game-polling') return { useGamePollingActive: active => active };
    if (name === './AgentLiveView') return { AgentLiveView, AgentLiveUsage };
    if (name.endsWith('.css')) return {};
    return {};
  };
  const js = ts.transpileModule(src('ui/views/AiStatsView.tsx'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX } }).outputText;
  const module = { exports: {} };
  vm.runInNewContext(`(function(require,module,exports){${js}\n})`)(require, module, module.exports);
  const render = active => { index = 0; return walk(module.exports.AiStatsView({ active })); };
  let nodes = render(true);
  assert.ok(nodes.some(n => n.type === AgentLiveView && n.props.active));
  assert.ok(!nodes.some(n => n.type === module.exports.AiUsageView));
  nodes.find(n => n.type === 'button' && n.props.children === 'Usage').props.onClick();
  nodes = render(true);
  assert.ok(nodes.some(n => n.type === module.exports.AiUsageView && n.props.active));
  assert.ok(nodes.some(n => n.type === AgentLiveUsage && n.props.active));
  assert.ok(!nodes.some(n => n.type === AgentLiveView));
  nodes = render(false);
  assert.ok(nodes.filter(n => n.type === AgentLiveUsage || n.type === module.exports.AiUsageView).every(n => !n.props.active));
});

test('poll loop: nothing while hidden, idle 5 s cadence, stops on hide, stale timers inert', async () => {
  const slots = [], effects = [], timers = new Map(), reads = []; let index = 0, nextTimer = 0;
  const hooks = { ...React,
    useState(initial) { const i = index++; slots[i] ??= { value: typeof initial === 'function' ? initial() : initial }; return [slots[i].value, v => { slots[i].value = typeof v === 'function' ? v(slots[i].value) : v; }]; },
    useRef(initial) { const i = index++; return slots[i] ??= { current: initial }; },
    useCallback(fn) { const i = index++; return (slots[i] ??= { fn }).fn; },
    useEffect(fn, deps) { const i = index++, old = slots[i]; if (!old || deps.some((v, n) => old.deps[n] !== v)) { slots[i] = { deps }; effects.push(() => { old?.cleanup?.(); slots[i].cleanup = fn(); }); } },
  };
  const js = ts.transpileModule(src('ui/views/AgentLiveView.tsx'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX } }).outputText;
  const module = { exports: {} };
  const fakeWindow = { setTimeout(fn, ms) { const id = ++nextTimer; timers.set(id, { fn: () => { timers.delete(id); fn(); }, ms }); return id; }, clearTimeout(id) { timers.delete(id); }, setInterval(fn, ms) { const id = ++nextTimer; timers.set(id, { fn, ms, every: true }); return id; }, clearInterval(id) { timers.delete(id); } };
  const require = name => {
    if (name === 'react') return hooks;
    if (name === 'react/jsx-runtime') return { jsx: React.createElement, jsxs: React.createElement };
    if (name === './agent-live-model') return M;
    if (name === './visible-poller') return load('ui/views/visible-poller.ts');
    if (name === './agent-live-ipc') return { agentLiveRaw: req => new Promise(resolve => reads.push({ req, resolve })), agentLiveBlocks: async () => ({ blocks: [] }), copyText: async () => true };
    if (name.endsWith('.css')) return {};
    if (name.endsWith('/AgentLive') || name.endsWith('/AgentLiveUsage')) return { AgentLivePage: () => null, AgentLiveUsageCards: () => null };
    throw Error(name);
  };
  const listeners = new Map();
  const document = { visibilityState: 'visible', addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: name => listeners.delete(name) };
  vm.runInNewContext(`(function(require,module,exports){${js}\n})`, { window: fakeWindow, document, Date, Promise, setImmediate })(require, module, module.exports);
  const render = active => { index = 0; module.exports.AgentLiveView({ active }); while (effects.length) effects.shift()(); };
  const tick = async () => { for (let i = 0; i < 20; i++) await new Promise(r => setImmediate(r)); };
  try {
    render(false); assert.equal(reads.length, 0); assert.equal(timers.size, 0);
    render(true); await tick(); assert.equal(reads.length, 1);
    const idle = { activeSessions: null };
    reads[0].resolve(rawFor(reads[0].req, idle)); await tick();
    const next = [...timers.values()].find(t => t.ms === 5000);
    assert.equal(next.ms, 5000, 'idle cadence'); assert.ok([...timers.values()].some(t => t.ms === 1000), 'live clock');
    next.fn(); await tick(); assert.equal(reads.length, 2);
    render(false); assert.equal(timers.size, 0, 'hidden: every timer cleared');
    reads[1].resolve(rawFor(reads[1].req, idle)); await tick();
    assert.equal(timers.size, 0, 'a flight that lands after hide schedules nothing');
    next.fn(); await tick(); assert.equal(reads.length, 2, 'stale callback is inert');
  } finally { for (const slot of slots) slot?.cleanup?.(); }
});
