// Failsafes: the page never ends as an invisible window, and delivery outages are explained.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
const f = load('ui/island/error-fallback.tsx');
const adapt = load('ui/views/app-adapters.ts');
const { NotificationsView } = load('ui/island/components/NotificationsView.tsx');
const { describeServiceReason } = load('ui/island/service-reasons.ts');

function capture(Component, props) { let tree; function Capture() { tree = Component(props); return tree; } renderToStaticMarkup(createElement(Capture)); return tree; }
function nodes(tree) {
  if (!tree || typeof tree !== 'object') return [];
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  return [tree, ...nodes(tree.props?.children)];
}
const fakeIpc = () => {
  const calls = { reports: [], reloads: 0, unlistened: 0, listener: null };
  return { calls, ipc: {
    collapse: async () => ({ rev: 7, mode: 'still' }),
    report: async report => { calls.reports.push(report); return { applied: true, rows: 1 }; },
    onState: async listener => { calls.listener = listener; return () => { calls.unlistened++; }; },
    reload: () => { calls.reloads++; },
  } };
};
const settle = () => new Promise(resolve => setTimeout(resolve, 0));

test('a bootstrap that rejects, a seed that throws or a first render that throws shows the fallback', async () => {
  for (const [name, load, seed, mount] of [
    ['load', () => Promise.reject(new Error('unavailable: island_bootstrap failed')), () => {}, () => {}],
    ['seed', async () => ({}), () => { throw new Error('malformed snapshot'); }, () => {}],
    ['mount', async () => ({}), () => {}, () => { throw new Error('Island root missing'); }],
  ]) {
    const failures = []; let mounted = 0;
    const quiet = console.error; console.error = () => {};
    try { await f.startIsland(load, seed, () => { mounted++; mount(); }, error => failures.push(error)); } finally { console.error = quiet; }
    assert.equal(failures.length, 1, `${name}: the fallback is shown once`);
    assert.ok(failures[0] instanceof Error);
    if (name !== 'mount') assert.equal(mounted, 0, `${name}: no partial page is mounted`);
  }
  const failures = []; let mounted = 0;
  await f.startIsland(async () => ({ ok: true }), () => {}, () => { mounted++; }, error => failures.push(error));
  assert.deepEqual([mounted, failures.length], [1, 0], 'a healthy boot never shows the fallback');
});

test('the fallback card says what failed in one line and its Reload button reloads', () => {
  const { ipc, calls } = fakeIpc();
  const html = renderToStaticMarkup(createElement(f.FallbackCard, { error: new Error('unavailable: Notifications are unavailable\n    at stack line'), ipc }));
  assert.match(html, /role="alert"/);
  assert.match(html, /data-island-fallback=""/);
  assert.match(html, /Monolith hit an error\.<\/strong> unavailable: Notifications are unavailable<\/p>/);
  assert.doesNotMatch(html, /stack line/);
  assert.match(html, /<button type="button" class="island-fallback-reload">Reload<\/button>/);
  const reload = nodes(capture(f.FallbackCard, { error: 'x', ipc })).find(n => n.type === 'button');
  reload.props.onClick();
  assert.equal(calls.reloads, 1);
  assert.equal(f.errorLine('x'.repeat(400)).length, 140);
  assert.equal(f.errorLine(undefined), 'Unknown error.');
  assert.equal(f.errorLine({ weird: true }), 'Unknown error.');
});

test('a render that throws below the boundary swaps the island for the card', () => {
  const { ipc } = fakeIpc();
  const boundary = new f.IslandErrorBoundary({ ipc, children: 'island' });
  assert.equal(boundary.render(), 'island');
  const error = new Error('Test-requested render failure');
  boundary.state = f.IslandErrorBoundary.getDerivedStateFromError(error);
  const card = boundary.render();
  assert.equal(card.type, f.FallbackCard);
  assert.equal(card.props.error, error);
  assert.match(renderToStaticMarkup(card), /Test-requested render failure/);
  // Real SSR through the boundary with a child that throws: React does not run boundaries on
  // the server, so the throw must surface rather than render a silent empty page.
  function Throws() { throw new Error('boom'); }
  assert.throws(() => renderToStaticMarkup(createElement(f.IslandErrorBoundary, { ipc }, createElement(Throws))), /boom/);
});

test('the card holds the input/blur region for whatever state the island is in, then lets go', async () => {
  const listeners = new Map();
  const saved = { window: globalThis.window, getComputedStyle: globalThis.getComputedStyle };
  globalThis.window = { devicePixelRatio: 1, addEventListener: (k, fn) => listeners.set(k, fn), removeEventListener: k => listeners.delete(k) };
  globalThis.getComputedStyle = () => ({ borderRadius: '14px' });
  try {
    const { ipc, calls } = fakeIpc();
    const card = { getBoundingClientRect: () => ({ x: 900, y: 44, width: 320, height: 40 }) };
    const dispose = f.holdFallbackGeometry(card, ipc);
    await settle();
    const rect = { x: 900, y: 44, width: 320, height: 40, radius: 14 };
    assert.deepEqual(calls.reports.at(-1), { stateRev: 7, mode: 'still', scale: 1, settled: true, rects: [rect, rect] }, 'collapsed: the card is an extension rect');
    calls.listener({ rev: 8, mode: 'large' });
    assert.deepEqual([calls.reports.at(-1).stateRev, calls.reports.at(-1).mode], [8, 'large'], 'follows the actor');
    listeners.get('resize')();
    assert.equal(calls.reports.length, 3, 'a resize re-reports');
    dispose();
    calls.listener({ rev: 9, mode: 'still' });
    assert.equal(calls.reports.length, 3, 'nothing after dispose');
    assert.equal(calls.unlistened, 1);
    assert.equal(listeners.size, 0);
    assert.deepEqual(f.fallbackReport({ x: -3, y: 5, width: 10, height: 10, radius: 400 }, { rev: 1, mode: 'hover' }, 2).rects[0], { x: 0, y: 5, width: 10, height: 10, radius: 100 });
  } finally { globalThis.window = saved.window; globalThis.getComputedStyle = saved.getComputedStyle; }
});

test('the bootstrap fault hook fires once and only in test instances', () => {
  const store = new Map([['island-test-fault', 'bootstrap']]);
  const storage = { getItem: k => store.get(k) ?? null, removeItem: k => store.delete(k) };
  assert.equal(f.takeTestFault(storage, false), false, 'never in the product');
  assert.equal(store.size, 1);
  assert.equal(f.takeTestFault(storage, true), true);
  assert.equal(f.takeTestFault(storage, true), false, 'consumed: Reload brings the page back');
  assert.equal(f.takeTestFault(undefined, true), false);
});

test('notification delivery outages are explained on the Notifications page', () => {
  assert.equal(adapt.notificationsDown(null), undefined);
  assert.equal(adapt.notificationsDown({ available: true, detail: 'ready' }), undefined);
  assert.equal(adapt.notificationsDown({ available: false, detail: 'starting' }), undefined);
  assert.equal(adapt.notificationsDown({ available: false, detail: 'delivery-stopped' }), 'delivery-stopped');
  const model = { ...adapt.adaptNotifications([], false, false), unavailable: 'ingest-unavailable' };
  const html = renderToStaticMarkup(createElement(NotificationsView, { model, intents: { notification: () => {} } }));
  assert.match(html, /class="card service-notice" data-kind="error" role="status"/);
  assert.match(html, /Hermes can’t send alerts to the island right now\. Desktop notifications still work\./);
  assert.doesNotMatch(renderToStaticMarkup(createElement(NotificationsView, { model: adapt.adaptNotifications([], false, false), intents: { notification: () => {} } })), /service-notice/);
  for (const [code, source, kind] of [['start-failed', 'notifications', 'error'], ['delivery-restarting', 'notifications', 'loading'], ['delivery-stopped', 'clipboard', 'error'], ['start-failed', 'clipboard', 'error'], ['start-failed', undefined, 'error']]) {
    const reason = describeServiceReason(code, source);
    assert.equal(reason.kind, kind, `${source}:${code}`);
    assert.equal(reason.code, undefined, `${code} has its own sentence`);
  }
});
