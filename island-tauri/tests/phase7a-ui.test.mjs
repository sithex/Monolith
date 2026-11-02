import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
async function load(path) {
  const source = readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
  const { outputText, diagnostics } = ts.transpileModule(source, { reportDiagnostics: true, compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } });
  assert.equal(diagnostics.length, 0);
  return import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`);
}
const { createTopic, beforeFirstRender } = await load('ui/stores/topic.ts');
const { canSwipe, arrowDirection, pointerIntent, minuteDelay, nextView, visibleViews } = await load('ui/island/navigation.ts');
const { geometryReport, createGeometryReporter } = await load('ui/island/geometry.ts');
const { createTerminalFlow } = await load('ui/views/terminal-flow.ts');
const { createVisiblePoller } = await load('ui/views/visible-poller.ts');
const { dashboardFromUsage, modulesFromRegistry } = await load('ui/views/adapters.ts');
const tick = async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); };
test('registry-disabled secondary views are omitted but Hermes and Settings remain reachable', () => {
  assert.deepEqual(visibleViews({ hardwareMonitor:true, moduleRegistry:{ modules:[{id:'monitor',enabled:false},{id:'ai',enabled:false},{id:'terminal',enabled:false}] } }), ['terminal','settings']);
});

test('older revisions never overwrite a topic or bootstrap', () => {
  const s = createTopic(); s.apply('event', 9); s.seed('old bootstrap', 8);
  assert.equal(s.getSnapshot(), 'event'); assert.equal(s.apply('old', 7), false);
  assert.equal(s.apply('equal', 9), false); assert.equal(s.getSnapshot(), 'event');
});
test('topic ordering wraps u32 and refuses ambiguous half range', () => {
  const s = createTopic(); s.seed('last', 0xffffffff); assert.equal(s.apply('wrapped', 0), true);
  assert.equal(s.apply('ambiguous', 0x80000000), false); assert.equal(s.getSnapshot(), 'wrapped');
});
test('first render happens only after all bootstrap stores are seeded', async () => {
  let resolve; const wait = new Promise(r => { resolve = r; });
  const a = createTopic(), b = createTopic(); let renders = 0;
  assert.throws(() => a.getSnapshot(), /bootstrap/);
  const work = beforeFirstRender(() => wait, values => { a.seed(values[0], 0); b.seed(values[1], 0); }, () => {
    assert.equal(a.getSnapshot(), 'settings'); assert.equal(b.getSnapshot(), 'palette'); renders++;
  });
  await tick(); assert.equal(renders, 0); resolve(['settings', 'palette']); await work; assert.equal(renders, 1);
});
test('bootstrap failure never renders defaults', async () => {
  let renders = 0;
  await assert.rejects(beforeFirstRender(async () => { throw Error('unavailable'); }, () => {}, () => { renders++; }));
  assert.equal(renders, 0);
});
test('pointer leave is one actor intent with no renderer timer or local mode write', async () => {
  const sent = [], timers = [], saved = globalThis.setTimeout;
  globalThis.setTimeout = callback => { timers.push(callback); return 1; };
  try { await pointerIntent(false, async intent => sent.push(intent)); }
  finally { globalThis.setTimeout = saved; }
  assert.deepEqual(sent, [{ type: 'pointerLeave' }]); assert.equal(timers.length, 0);
});
test('swipes cannot start on any form label link or editable control', () => {
  for (const name of ['input', 'label', 'a', 'select', 'textarea', 'button', '[contenteditable]', '.terminal']) {
    assert.equal(canSwipe({ closest: selector => selector.split(', ').includes(name) }), false, name);
  }
  assert.equal(canSwipe({ closest: () => null }), true);
});
test('range arrows remain in their control and navigation is bounded', () => {
  const input = { closest: selector => selector.includes('input') };
  assert.equal(arrowDirection('ArrowRight', input), 0);
  assert.equal(arrowDirection('ArrowLeft', { closest: () => null }), -1);
  assert.equal(nextView('terminal', -1, ['terminal', 'settings']), null);
});
test('clock is aligned to the wall clock minute rather than mount time', () => {
  assert.equal(minuteDelay(12345), 47655); assert.equal(minuteDelay(60000), 60000);
});
const state = { rev: 17, mode: 'large', view: 'terminal' };
function outer() { return { offsetLeft: 70, offsetTop: 8, style: { setProperty() {} }, getBoundingClientRect: () => ({ x: 82, y: 32, width: 1040, height: 672 }), querySelector: () => ({ getBoundingClientRect: () => ({ x: 100, y: 100, width: 900, height: 500 }) }) }; }
globalThis.getComputedStyle = () => ({ borderRadius: '24px' });
test('geometry measures the outer surface not the terminal or inner pane', () => {
  assert.deepEqual(geometryReport(outer(), state, true, 1).rects, [{ x: 82, y: 32, width: 1040, height: 672, radius: 24 }]);
});
test('coalesced geometry flush carries the current actor revision', async () => {
  let frame, frames = 0, current = state; const sent = [];
  globalThis.innerWidth = 1180; globalThis.innerHeight = 820; globalThis.devicePixelRatio = 1;
  globalThis.window = { addEventListener() {}, removeEventListener() {} };
  globalThis.ResizeObserver = class { observe() {} disconnect() {} };
  globalThis.requestAnimationFrame = callback => { frame = callback; frames++; return frames; };
  globalThis.cancelAnimationFrame = () => {};
  const owner = createGeometryReporter(outer(), () => current, () => ({ x: 0, y: 0 }), async r => sent.push(r), assert.fail);
  owner.schedule(); owner.schedule(); current = { ...state, rev: 18 }; frame(); await tick();
  assert.equal(frames, 1); assert.equal(sent[0].stateRev, 18); assert.equal(frames, 1, 'no perpetual rAF loop'); owner.dispose();
});
test('terminal credit is issued only from the asynchronous xterm parse callback', async () => {
  const parsed = [], scheduled = [], acks = [], writes = [];
  const flow = createTerminalFlow({ reset() {}, write(data, callback) { writes.push(data); parsed.push(callback); } }, async seq => acks.push(seq), assert.fail, callback => scheduled.push(callback));
  flow.snapshot({ seq: 4, data: 'replay' }); flow.event({ type: 'data', seq: 5, data: 'output' });
  assert.deepEqual(acks, []); assert.equal(scheduled.length, 0); parsed.forEach(cb => cb());
  assert.equal(scheduled.length, 1); scheduled.shift()(); await tick(); assert.deepEqual(acks, [5]);
  flow.snapshot({ seq: 4, data: 'duplicate' }); assert.deepEqual(writes, ['replay', 'output']); flow.dispose();
});
test('monitor never polls while hidden even when a cancelled callback fires late', async () => {
  const callbacks = []; let reads = 0;
  const poller = createVisiblePoller(async () => ++reads, () => {}, assert.fail, cb => { callbacks.push(cb); return callbacks.length; }, () => {});
  poller.setVisible(true); await tick(); assert.equal(reads, 1); poller.setVisible(false);
  callbacks.shift()(); await tick(); assert.equal(reads, 1); poller.dispose();
});
test('monitor never overlaps requests across rapid hide and reopen', async () => {
  let resolve, reads = 0;
  const poller = createVisiblePoller(() => { reads++; return new Promise(r => { resolve = r; }); }, () => {}, assert.fail, () => 1, () => {});
  poller.setVisible(true); poller.setVisible(false); poller.setVisible(true);
  assert.equal(reads, 1); resolve(3); await tick(); poller.dispose();
});
test('AI cohort accounting never becomes fabricated today or running data', () => {
  const result = dashboardFromUsage({ account: { provider:'fixture', windows: [{ label:'Session', used_percent:200 }] }, local:{ available:true, totals:{ estimatedCostUsd:99 }, models:[{model:'M',provider:'P',inputTokens:2,outputTokens:3,estimatedCostUsd:null,sessions:1}] } });
  assert.equal(result.value.spendTodayUsd, null); assert.equal(result.value.active, null); assert.deepEqual(result.value.hourly, []); assert.deepEqual(result.value.heatmap, []);
  assert.equal(result.value.limits[0].usedPercent, 100); assert.equal(result.value.byModel[0].costUsd, null); assert.equal(result.value.byModel[0].key, 'M · P');
});
test('unknown registry configuration remains inert and locked home is identified', () => {
  const result = modulesFromRegistry({ version:2, modules:[{id:'future',config:{anything:true},order:1,enabled:true},{id:'terminal',config:{type:'terminal',fontSize:13},order:0,enabled:true}] });
  assert.equal(result.modules[0].known, false); assert.equal(result.modules[1].locked, true);
});
