// Phase 7 (Nov 17 review): frontend performance. lit-ring scope, Settings active gate, AI clock isolation,
// xterm warm-up, OpenRunde woff2, lyrics backdrop, fit observers. Runtime tests drive the production modules.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, openSync, readSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import { driver, nodes, globals } from './phase7b-runtime-harness.mjs';

const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const rulesOf = css => [...css.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(m => ({ selector: m[1].trim(), body: m[2] }));

// ---- lit-ring -------------------------------------------------------------------------------------------
test('lit-ring: the infinite animation and will-change exist only on a lit button of the OPEN island', () => {
  const css = src('ui/island/polish.css'), rules = rulesOf(css);
  for (const rule of rules.filter(r => /lit-ring/.test(r.body) || /will-change/.test(r.body) && /live-side-button/.test(r.selector))) {
    if (/animation:\s*none/.test(rule.body)) continue;
    assert.match(rule.selector, /\[data-lit='true'\]\[data-open='true'\]::after/, `${rule.selector} must require data-open`);
  }
  const base = rules.find(r => r.selector === '.live-side-button::after'), lit = rules.find(r => r.selector === ".live-side-button[data-lit='true']::after");
  assert.doesNotMatch(base.body, /will-change|animation/, 'the closed/unlit base rule holds no layer hint or loop');
  assert.doesNotMatch(lit.body, /will-change|animation/, 'a lit button on a closed island does not animate');
  assert.match(css, /\.live-side-button\[data-lit='true'\]\[data-open='true'\]::after \{ will-change: transform, opacity; animation: lit-ring/);
});
test('lit-ring: SideControls marks every bubble open only while the island is open', () => {
  const source = src('ui/island/SideControls.tsx');
  assert.match(source, /data-lit=\{lit\} data-open=\{open\}/);
  assert.match(source, /interactive=\{active && ready\} open=\{active\}/);
});

// ---- Settings active gate -------------------------------------------------------------------------------
function settingsPane(run) {
  const calls = { settings: [], appearance: [], palette: [], notifications: [] };
  let settings = { uiFont: 'system' };
  const d = driver({
    'ui/stores/index.ts': { useSettings: a => (calls.settings.push(a), settings), useAppearance: a => (calls.appearance.push(a), 'skin'), usePalette: a => (calls.palette.push(a), {}),
      saveSettings: async () => {}, saveAppearance: async () => {} },
    'ui/ipc/client.ts': { displayOutputs: async () => [], settingsDefaults: async () => ({}) },
    'ui/ipc/bindings.ts': { commands: { fontFamilies: async () => ({ status: 'ok', data: [] }) } },
    'ui/ipc/notifications.ts': { useNotifications: a => (calls.notifications.push(a), []) },
    'ui/ipc/island-state.ts': { dispatch: async () => {} },
  });
  const values = { window: new EventTarget(), navigator: { userAgent: 'Linux' }, localStorage: { getItem: () => null, setItem() {} },
    document: { documentElement: { dataset: { displayBackend: 'wayland' } }, querySelector: () => null } };
  return globals(values, () => {
    try { return run({ d, calls, View: d.load('ui/views/SettingsView.tsx').SettingsView, setSettings: next => { settings = next; } }); } finally { d.dispose(); }
  });
}
test('SettingsView: hidden, it hands React the tree it last drew and every store hook is told it is inactive', () => settingsPane(({ d, calls, View, setSettings }) => {
  const open = d.render(View, { active: true });
  assert.deepEqual([calls.settings.at(-1), calls.appearance.at(-1), calls.palette.at(-1), calls.notifications.at(-1)], [true, true, true, true]);
  setSettings({ uiFont: 'openrunde' });
  const hidden = d.render(View, { active: false });
  assert.equal(hidden, open, 'identical element: React bails out of the whole subtree');
  assert.deepEqual([calls.settings.at(-1), calls.appearance.at(-1), calls.palette.at(-1), calls.notifications.at(-1)], [false, false, false, false]);
  const reopened = d.render(View, { active: true });
  assert.notEqual(reopened, open, 'reopening renders from the live store again');
}));
test('SettingsView: section and search text survive closing and reopening (state lives in the mounted pane)', () => settingsPane(({ d, View }) => {
  let tree = d.render(View, { active: true });
  nodes(tree).find(n => n.props?.['data-section'] === 'notch').props.onClick();
  nodes(d.render(View, { active: true })).find(n => n.type === 'input' && n.props['aria-label'] === 'Search settings').props.onChange({ target: { value: 'font' } });
  tree = d.render(View, { active: true });
  const hidden = d.render(View, { active: false });
  assert.equal(hidden, tree);
  const back = d.render(View, { active: true });
  assert.equal(nodes(back).find(n => n.type === 'input' && n.props['aria-label'] === 'Search settings').props.value, 'font', 'search text kept');
}));
test('SettingsView: Island passes the gate (open AND on the Settings page)', () => {
  assert.match(src('ui/island/Island.tsx'), /<SettingsView active=\{active && view === 'settings'\} \/>/);
});
test('useGatedStore: hidden components are not woken and keep the last active snapshot; reopening reads live', () => {
  const js = ts.transpileModule(src('ui/stores/gated.ts'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  const refs = [], captured = {};
  const react = { useRef: v => { const i = captured.i++; return refs[i] ??= { current: v }; }, useCallback: f => f, useSyncExternalStore: (sub, get) => { captured.sub = sub; captured.get = get; return get(); } };
  const mod = { exports: {} }; vm.runInNewContext(`(function(require,module,exports){${js}\n})`, {})(n => react, mod, mod.exports);
  let value = 1, wakes = 0; const listeners = new Set();
  const subscribe = l => { listeners.add(l); return () => listeners.delete(l); }, emit = () => listeners.forEach(l => l());
  const hook = active => { captured.i = 0; return mod.exports.useGatedStore(subscribe, () => value, active); };
  assert.equal(hook(true), 1); captured.sub(() => { wakes++; }); emit(); assert.equal(wakes, 1, 'active: forwarded');
  assert.equal(hook(false), 1); value = 2; emit(); assert.equal(wakes, 1, 'inactive: not forwarded');
  assert.equal(captured.get(), 1, 'inactive: the old snapshot, so no re-render and no tearing');
  assert.equal(hook(true), 2, 'active again: live value');
});

// ---- AI clock -------------------------------------------------------------------------------------------
test('AI page: the 1 s clock lives in two labels, never in the dashboard; one shared interval, none while hidden', () => {
  const view = src('ui/views/AiStatsView.tsx');
  assert.doesNotMatch(view, /setInterval|clearInterval/);
  const dash = src('ui/island/components/AiDashboard.tsx');
  assert.match(dash, /<Countdown reset=\{w\.reset\} now=\{now\} active=\{active\}\/>/);
  assert.match(dash, /<Elapsed seconds=\{r\.elapsed\} now=\{now\} active=\{active\}\/>/);
  const js = ts.transpileModule(src('ui/island/components/relative-clock.ts'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  const intervals = new Map(); let id = 0, subs = [];
  const win = { setInterval: (f, ms) => { intervals.set(++id, { f, ms }); return id; }, clearInterval: n => intervals.delete(n) };
  const react = { useSyncExternalStore: (sub, get) => { subs.push(sub); return get(); } };
  const mod = { exports: {} }; vm.runInNewContext(`(function(require,module,exports){${js}\n})`, { window: win, Date: { now: () => 5000 } })(() => react, mod, mod.exports);
  const text = (active, now = 1000) => (subs = [], mod.exports.useRelativeText(active, now, at => `t${at}`));
  assert.equal(text(false), 't1000', 'inactive: frozen at the data time'); const idleSub = subs[0]; const off = idleSub(() => {});
  assert.equal(intervals.size, 0, 'inactive: subscribing starts no interval, nothing ticks'); off();
  assert.equal(text(true), 't5000', 'active: live clock');
  const a = subs[0](() => {}), b = subs[0](() => {});
  assert.equal(intervals.size, 1, 'one interval shared by every label'); assert.equal([...intervals.values()][0].ms, 1000);
  a(); assert.equal(intervals.size, 1); b(); assert.equal(intervals.size, 0, 'last label gone: interval cleared');
});

// ---- xterm warm-up --------------------------------------------------------------------------------------
async function warmup(mode, started, run) {
  const timers = [], idles = []; let loads = 0, tick = 0;
  const win = { setTimeout: (f, ms) => (timers.push({ f, ms, id: ++tick }), tick), clearTimeout: id => { const i = timers.findIndex(t => t.id === id); if (i >= 0) timers.splice(i, 1); },
    requestIdleCallback: (f, o) => (idles.push({ f, o }), idles.length), cancelIdleCallback() {} };
  const d = driver(), had = Object.getOwnPropertyDescriptor(globalThis, 'window');
  globalThis.window = win;
  try {
    const hook = d.load('ui/island/terminal-warmup.ts').useTerminalWarmup;
    const load = () => { loads++; };
    const render = (m, s) => { d.renderOnly(() => { hook(load, m, s); return null; }, {}); d.flushEffects(); };
    return await run({ render, timers, idles, loads: () => loads, win });
  } finally { d.dispose(); if (had) Object.defineProperty(globalThis, 'window', had); else delete globalThis.window; }
}
const settle = () => new Promise(r => setImmediate(r));
test('xterm chunk: nothing loads at module load or while closed; the idle fallback waits for the first paint', async () => {
  const source = src('ui/island/Island.tsx');
  assert.doesNotMatch(source, /^if \(typeof window.*loadTerminal/m, 'no module-scope preload');
  assert.equal([...source.matchAll(/loadTerminal\(\)/g)].length, 1, 'the only call is the lazy component\'s own import');
  assert.doesNotMatch(source, /setTimeout\([^)]*loadTerminal/);
  await warmup('still', false, async ({ render, timers, idles, loads }) => {
    render('still', false); await settle();
    assert.equal(loads(), 0, 'closed + no terminal: no load yet');
    assert.equal(timers.length, 1); assert.ok(timers[0].ms >= 1000, `idle fallback is delayed past first paint (${timers[0].ms} ms)`);
    timers[0].f(); assert.equal(idles.length, 1, 'then it waits for an idle moment'); assert.equal(loads(), 0);
    idles[0].f(); await settle(); assert.equal(loads(), 1);
  });
});
test('xterm chunk: the first sign of intent (island leaves closed, or a terminal exists) loads it immediately', async () => {
  await warmup('still', false, async ({ render, timers, loads }) => {
    render('still', false); render('hover', false); await settle();
    assert.equal(loads(), 1, 'opening starts the load at once'); assert.equal(timers.length, 0, 'idle fallback cancelled');
  });
  await warmup('still', true, async ({ render, loads }) => { render('still', true); await settle(); assert.equal(loads(), 1, 'a running terminal session loads it at once'); });
  await warmup('still', false, async ({ render, loads }) => {
    render('large', false); await settle(); assert.equal(loads(), 1);
  });
});
test('xterm chunk: the terminal is still one lazy component created once, behind terminalStarted', () => {
  const source = src('ui/island/Island.tsx');
  assert.equal([...source.matchAll(/lazy\(/g)].length, 1); assert.equal([...source.matchAll(/<TerminalView /g)].length, 1);
  assert.match(source, /state\.terminalStarted && <Suspense fallback=\{null\}><TerminalView/);
});

// ---- fonts ----------------------------------------------------------------------------------------------
test('OpenRunde ships as woff2 and font-display never hides text', () => {
  const css = src('ui/island/tokens/themes.css'), faces = css.split('\n').filter(l => l.startsWith('@font-face'));
  assert.equal(faces.length, 4);
  for (const face of faces) {
    assert.match(face, /OpenRunde-[A-Za-z]+\.woff2'\) format\('woff2'\)/); assert.doesNotMatch(face, /\.woff'/);
    assert.match(face, /font-display: (swap|fallback)/, 'not block');
  }
  for (const name of ['Regular', 'Medium', 'Semibold', 'Bold']) {
    const fd = openSync(new URL(`../ui/src/assets/fonts/OpenRunde-${name}.woff2`, import.meta.url), 'r'), magic = Buffer.alloc(4); readSync(fd, magic, 0, 4, 0);
    assert.equal(magic.toString('latin1'), 'wOF2', name);
  }
});

// ---- lyrics backdrop ------------------------------------------------------------------------------------
test('lyrics backdrop: blurred at a quarter size and scaled back, landing on exactly the old box', () => {
  const css = src('ui/island/media.css'), rule = rulesOf(css).find(r => r.selector === '.np-backdrop .bounded-image').body;
  assert.match(rule, /filter: blur\(14px\) saturate\(1\.6\)/); assert.doesNotMatch(css, /blur\(56px\)/);
  assert.match(rule, /width: 25%; height: 25%; max-height: 45px/); assert.match(rule, /transform-origin: 0 0; transform: translate\(-70%, -70%\) scale\(5\.4\)/);
  // Geometry: old = box (W x h) at (0,10) scaled 1.35 about its centre; new = box (W/4 x h/4) at (0,10) -> translate -70% of itself, scale 5.4 about (0,0).
  for (const [W, H] of [[560, 400], [700, 150], [300, 900]]) {
    const h = Math.min(H, 180), oldBox = { x: W / 2 - 1.35 * W / 2, y: 10 + h / 2 - 1.35 * h / 2, w: 1.35 * W, h: 1.35 * h };
    const w4 = W / 4, h4 = Math.min(H / 4, 45), newBox = { x: -0.7 * w4, y: 10 - 0.7 * h4, w: 5.4 * w4, h: 5.4 * h4 };
    for (const k of ['x', 'y', 'w', 'h']) assert.ok(Math.abs(oldBox[k] - newBox[k]) < 1e-9, `${W}x${H} ${k}: ${oldBox[k]} vs ${newBox[k]}`);
  }
  assert.equal(56 * 1.35, 14 * 5.4, 'effective blur radius is unchanged');
});

// ---- fit observers --------------------------------------------------------------------------------------
function fitFixture() {
  const log = { ro: [], mo: [], roOff: [], disconnects: [] };
  class RO { constructor(f) { this.f = f; log.ro.push(this); } observe(e) { (this.seen ??= []).push(e); } unobserve(e) { log.roOff.push(e); } disconnect() { log.disconnects.push('ro'); } }
  class MO { constructor(f) { this.f = f; this.calls = []; log.mo.push(this); } observe(t, o) { this.calls.push([t, o]); } disconnect() { log.disconnects.push('mo'); } }
  const el = (name, children = []) => ({ name, children });
  return { log, RO, MO, el };
}
test('fit observers: no subtree watch of the page; panes watched for hidden/root swaps; scroller and row children by size', () => {
  const f = fitFixture(), panes = [f.el('a'), f.el('b'), f.el('c')];
  const root = { querySelectorAll: s => s === '.view-pane > .app-pane' ? panes : [], querySelector: () => null };
  globals({ ResizeObserver: f.RO, MutationObserver: f.MO }, () => {
    const mod = driver().load('ui/island/fit-observers.ts'); let changes = 0;
    const fit = mod.createFitObservers(root, () => changes++);
    const mo = f.log.mo[0];
    assert.deepEqual(mo.calls.map(c => c[0]), panes);
    for (const [, opts] of mo.calls) { assert.ok(!opts.subtree, 'never the whole subtree'); assert.deepEqual(opts.attributeFilter, ['hidden']); assert.equal(opts.childList, true); }
    const card1 = f.el('card1'), card2 = f.el('card2'), scroller = f.el('scroller', [card1, card2]), strip = f.el('strip'), row = f.el('row', [strip]), child = f.el('child');
    fit.retarget(child, scroller, row);
    assert.deepEqual(f.log.ro[0].seen, [child, card1, card2, strip], 'size observer: page root + scroller children + row children');
    assert.deepEqual(mo.calls.slice(3).map(c => [c[0], c[1]]), [[scroller, { childList: true }], [row, { childList: true }]]);
    fit.retarget(child, null, null);
    assert.deepEqual(f.log.roOff, [card1, card2, strip], 'stale targets released when the page changes');
    f.log.ro[0].f(); mo.f(); assert.equal(changes, 2, 'both observers reach the scheduler');
    fit.dispose(); assert.deepEqual(f.log.disconnects, ['ro', 'mo']);
  });
});
test('fit observers: IslandSurface uses them (no inline subtree MutationObserver left)', () => {
  const source = src('ui/island/IslandSurface.tsx');
  assert.match(source, /createFitObservers\(root, \(\) => schedule\(\)\)/); assert.doesNotMatch(source, /new MutationObserver|subtree: true/);
});
