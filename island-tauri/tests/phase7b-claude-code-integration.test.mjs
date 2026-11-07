// Exercise the integrated components and real visible-poller hook without a browser or IPC server.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import * as React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
import { load as loadClaude } from './claude-code-loader.mjs';
const source = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const M = loadClaude('ui/views/claude-code-model.ts');

test('Claude card and history lead render even without Hermes plugin data', () => {
  const { ClaudeCodeCard, HistoryTotalsCard } = load('ui/island/components/ClaudeCodeCard.tsx');
  const { AgentLivePage } = load('ui/island/components/AgentLive.tsx');
  const { AgentLiveUsageCards } = load('ui/island/components/AgentLiveUsage.tsx');
  const extra = React.createElement(ClaudeCodeCard, { live: { sessions: [], more: 0, mode: 'registry', idleHideMin: 30, noFolder: true, unavailable: null, registry: '0' }, now: 0 });
  const live = renderToStaticMarkup(React.createElement(AgentLivePage, { view: null, now: 0, onCopy() {}, extra }));
  assert.match(live, /Loading/); assert.match(live, /No Claude Code folder/);
  const usage = renderToStaticMarkup(React.createElement(AgentLiveUsageCards, { history: null, ids: [], now: 0, onCopy() {}, pluginMissing: true, loaded: true, lead: React.createElement(HistoryTotalsCard, { history: null }) }));
  assert.match(usage, /Today \/ this week/); assert.match(usage, /No plan windows: habby-metrics is not installed/);
  const integration = source('ui/views/AgentLiveView.tsx');
  assert.match(integration, /extra={<ClaudeCodeLive active={active} now={now}\/>}/);
  assert.match(integration, /useClaudeHistory\(active,/);
  assert.match(integration, /history={cc.history} lead={cc.lead}/);
  assert.match(source('ui/views/AiStatsView.tsx'), /className="ai-agents"/);
});

test('real Claude hook cancels hidden flights, stops hidden timers and catches up only visibly', async () => {
  const slots = [], effects = [], timers = new Map(), reads = [], listeners = new Map();
  let index = 0, nextTimer = 0;
  const hooks = { ...React,
    useState(initial) { const i = index++; slots[i] ??= { value: typeof initial === 'function' ? initial() : initial }; return [slots[i].value, value => { slots[i].value = typeof value === 'function' ? value(slots[i].value) : value; }]; },
    useRef(initial) { return slots[index++] ??= { current: initial }; },
    useEffect(fn, deps) { const i = index++, old = slots[i]; if (!old || deps.some((value, n) => old.deps[n] !== value)) { slots[i] = { deps }; effects.push(() => { old?.cleanup?.(); slots[i].cleanup = fn(); }); } },
  };
  const window = { setTimeout(fn, ms) { const id = ++nextTimer; timers.set(id, { ms, fn() { timers.delete(id); fn(); } }); return id; }, clearTimeout(id) { timers.delete(id); } };
  const document = { visibilityState: 'visible', addEventListener(name, fn) { listeners.set(name, fn); }, removeEventListener(name) { listeners.delete(name); } };
  const require = name => {
    if (name === 'react') return hooks;
    if (name === 'react/jsx-runtime') return { jsx: React.createElement, jsxs: React.createElement };
    if (name === './claude-code-model') return M;
    if (name === './agent-live-model') return load('ui/views/agent-live-model.ts');
    if (name === './visible-poller') return load('ui/views/visible-poller.ts');
    if (name === './claude-code-ipc') return { claudeCodeRaw: req => new Promise(resolve => reads.push({ req, resolve })) };
    if (name.endsWith('/ClaudeCodeCard') || name.endsWith('.css')) return {};
    throw Error(`Unexpected import: ${name}`);
  };
  const js = ts.transpileModule(source('ui/views/ClaudeCodeView.tsx'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX } }).outputText;
  const module = { exports: {} };
  vm.runInNewContext(`(function(require,module,exports){${js}\n})`, { window, document, Date, Promise })(require, module, module.exports);
  const render = active => { index = 0; module.exports.useClaudeCode(active, 'live'); while (effects.length) effects.shift()(); };
  const tick = async () => { for (let i = 0; i < 20; i++) await new Promise(resolve => setImmediate(resolve)); };
  const raw = more => ({ now: Date.now(), claudeDir: null, folderMissing: true, registry: [], registryError: null, files: [], more, error: null });
  try {
    render(false); assert.equal(reads.length, 0); assert.equal(timers.size, 0);
    render(true); await tick(); assert.equal(reads.length, 1);
    render(false); reads[0].resolve(raw(false)); await tick();
    assert.equal(module.exports.claudeFeed().loaded, false, 'hidden reply cannot mutate the shared feed');
    assert.equal(timers.size, 0);
    render(true); await tick(); assert.equal(reads.length, 2);
    reads[1].resolve(raw(false)); await tick();
    const idle = [...timers.values()].find(timer => timer.ms === 5000); assert.ok(idle);
    document.visibilityState = 'hidden'; listeners.get('visibilitychange')();
    assert.equal(timers.size, 0); idle.fn(); await tick(); assert.equal(reads.length, 2);
    document.visibilityState = 'visible'; listeners.get('visibilitychange')(); await tick();
    assert.equal(reads.length, 3); reads[2].resolve(raw(true)); await tick();
    const catchup = [...timers.values()].find(timer => timer.ms === M.MORE_DELAY_MS); assert.ok(catchup);
    render(false); assert.equal(timers.size, 0); catchup.fn(); await tick(); assert.equal(reads.length, 3);
  } finally { for (const slot of slots) slot?.cleanup?.(); }
});
