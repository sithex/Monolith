import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import * as React from 'react';
import { load } from './phase7b-loader.mjs';
const { searchCommands, fuzzyScore, nextSelection } = load('ui/island/command-search.ts');
const item = (id, label, group = 'Pages') => ({ id, label, group, run() {} });

test('palette fuzzy ranking is accent-insensitive, multiword, bounded and stable', () => {
  const rows = [item('s', 'Terminal font', 'Settings'), item('t', 'Terminal'), item('d', 'Do Not Disturb', 'Quick actions')];
  assert.equal(searchCommands(rows, 'terminal')[0].id, 't');
  assert.equal(searchCommands(rows, 'trm fnt')[0].id, 's');
  assert.equal(searchCommands(rows, 'disturb quick')[0].id, 'd');
  assert.equal(fuzzyScore('Café calendar', 'cafe'), 0);
  assert.equal(fuzzyScore('Terminal', 'zzzz'), null);
  assert.equal(searchCommands(Array.from({ length: 500 }, (_, i) => item(String(i), 'Result')), '').length, 12);
  assert.equal(searchCommands(rows, 'nothing matches').length, 0);
  assert.equal(searchCommands(rows, '', -1).length, 0);
  assert.deepEqual(searchCommands(rows, '').map(r => r.id), ['s', 't', 'd']);
});
test('palette keyboard selection wraps, Home/End and empty lists are safe', () => {
  assert.equal(nextSelection(0, 'ArrowUp', 3), 2);
  assert.equal(nextSelection(2, 'ArrowDown', 3), 0);
  assert.equal(nextSelection(1, 'Home', 3), 0);
  assert.equal(nextSelection(1, 'End', 3), 2);
  assert.equal(nextSelection(0, 'ArrowDown', 0), 0);
});
function harness() {
  const slots = [], effects = [], frames = [], actions = [], io = [], events = new EventTarget();
  let index = 0, tree, current = { mode: 'large', view: 'terminal' };
  const hooks = { ...React,
    useState(initial) { const i = index++; slots[i] ??= { value: typeof initial === 'function' ? initial() : initial }; return [slots[i].value, next => { slots[i].value = typeof next === 'function' ? next(slots[i].value) : next; }]; },
    useRef(initial) { const i = index++; return slots[i] ??= { current: initial }; },
    useMemo(fn, deps) { const i = index++, old = slots[i]; if (!old || deps.some((v, n) => v !== old.deps[n])) slots[i] = { value: fn(), deps }; return slots[i].value; },
    useEffect(fn, deps) { const i = index++, old = slots[i]; if (!old || deps.some((v, n) => v !== old.deps[n])) { slots[i] = { deps }; effects.push(() => { old?.cleanup?.(); slots[i].cleanup = fn(); }); } },
  };
  let activeElement = { isConnected: true, closest: () => null, focus() { io.push('restore'); } };
  const fakeInput = { focus() { activeElement = fakeInput; io.push('search-focus'); } };
  const fakeDocument = { get activeElement() { return activeElement; }, querySelector: () => null };
  const source = readFileSync(new URL('../ui/island/CommandPalette.tsx', import.meta.url), 'utf8');
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX } }).outputText;
  const module = { exports: {} };
  const require = name => {
    if (name === 'react') return hooks;
    if (name === 'react/jsx-runtime') return { jsx: React.createElement, jsxs: React.createElement };
    if (name === '@tauri-apps/api/event') return { listen: async () => () => {} };
    if (name === '../ipc/island-state') return { useIslandState: () => current, dispatch: async intent => { actions.push(intent); if (intent.type === 'selectView') current = { ...current, view: intent.view }; } };
    if (name === '../stores') return { useSettings: () => ({ clipboardHistory: true }) };
    if (name === '../stores/apps') return { useLauncher: () => ({ items: [] }), useApp: () => null, appStores: { timer: { apply() {} } } };
    if (name === '../ipc/client') return { clipboardList: async () => { io.push('clips'); return [{ id: 'clip', preview: 'Copied phrase', kind: 'text', sourceApp: null }]; }, clipboardCopy: async id => actions.push(['copy', id]), appApi: { timerCommand: async op => actions.push(op) } };
    if (name === '../views/settings-model') return { sectionName: () => 'Terminal', SETTINGS_CARDS: [{ tab: 'terminal', title: 'Terminal', rows: [{ id: 'font', label: 'Terminal font', keys: ['terminalFontFamily'] }] }] };
    if (name === '../views/settings/SettingsLink') return { openSetting: async rowId => { actions.push({ type: 'selectView', view: 'settings' }); const event = new Event('island-setting-jump'); event.detail = rowId; events.dispatchEvent(event); } };
    if (name === './side-actions') return { ACTION_OPTIONS: [{ key: 'dnd', label: 'Do Not Disturb', group: 'Quick actions', action: { type: 'toggleDnd' } }], runSideAction: async action => actions.push(action) };
    if (name === './command-search') return { searchCommands, nextSelection };
    if (name.endsWith('.css')) return {};
    throw Error(name);
  };
  class CustomEvent extends Event { constructor(name, init) { super(name); this.detail = init?.detail; } }
  vm.runInNewContext(`(function(require,module,exports){${js}\n})`, { window: events, document: fakeDocument, Event, CustomEvent, Date,
    requestAnimationFrame: fn => { frames.push(fn); return frames.length; }, cancelAnimationFrame() {} })(require, module, module.exports);
  function walk(node, out = []) { if (!node || typeof node !== 'object') return out; if (Array.isArray(node)) { node.forEach(n => walk(n, out)); return out; } out.push(node); walk(node.props?.children, out); return out; }
  const render = () => { index = 0; tree = module.exports.CommandPalette({ fail: assert.fail }); for (const node of walk(tree)) if (node.props?.ref) node.props.ref.current = node.type === 'input' ? fakeInput : { querySelector: () => null }; while (effects.length) effects.shift()(); return tree; };
  const key = (key, extra = {}) => { const event = new Event('keydown', { cancelable: true }); Object.assign(event, { key, ...extra }); events.dispatchEvent(event); return event; };
  const query = value => { walk(tree).find(n => n.type === 'input').props.onChange({ target: { value } }); render(); };
  render();
  return { render, key, query, actions, io, events, walk: () => walk(tree), frames, collapse() { current = { ...current, mode: 'still' }; render(); }, dispose() { slots.forEach(s => s?.cleanup?.()); } };
}
const drain = () => new Promise(resolve => setImmediate(resolve));
test('palette captures Ctrl+K, focuses search, runs filtered clipboard/settings and Esc does not collapse', async () => {
  const h = harness();
  try {
    const chord = h.key('k', { ctrlKey: true }); assert.ok(chord.defaultPrevented); h.render(); await drain(); h.render();
    assert.ok(h.io.includes('search-focus')); assert.equal(h.io.filter(v => v === 'clips').length, 1);
    h.query('Copied phrase');
    assert.equal(h.walk().filter(n => n.props?.role === 'option').length, 1);
    h.key('Enter'); await drain(); h.render();
    assert.ok(h.actions.some(a => Array.isArray(a) && a[0] === 'copy' && a[1] === 'clip'));
    assert.equal(h.walk().filter(n => n.props?.role === 'dialog').length, 0);
    h.key('k', { ctrlKey: true }); h.render();
    h.query('font'); let jump;
    h.events.addEventListener('island-setting-jump', event => { jump = event.detail; });
    h.key('Enter'); await drain(); h.render();
    assert.equal(jump, 'font'); assert.ok(h.actions.some(a => a.type === 'selectView' && a.view === 'settings'));
    h.key('k', { ctrlKey: true }); h.render();
    const esc = h.key('Escape'); assert.ok(esc.defaultPrevented); h.render();
    assert.ok(!h.actions.some(a => a.type === 'collapse'));
    h.key('k', { ctrlKey: true }); h.render(); h.collapse();
    assert.equal(h.walk().filter(n => n.props?.role === 'dialog').length, 0);
  } finally { h.dispose(); }
});
