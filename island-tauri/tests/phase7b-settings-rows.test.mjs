// Phase 5 (Nov 16 review): Settings rows. Group rows, the scheme swatch grid, "Reset section"
// and the weather field group. Runtime tests drive the real components through the hook driver.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
import { driver, nodes, labeled } from './phase7b-runtime-harness.mjs';

const model = load('ui/views/settings-model.ts');
const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const rule = (css, selector) => css.split('\n').find(line => line.startsWith(`${selector} {`)) ?? '';

// One SettingsView fixture: real view + model, stubbed stores/IPC. `settings` is live (mutable).
export async function settingsFixture(run, { settings = {}, defaults = {}, appearance = 'skin' } = {}) {
  const saved = [], appearances = []; let tree;
  const d = driver({
    'ui/stores/index.ts': { useSettings: () => settings, useAppearance: () => appearance, usePalette: () => ({}),
      saveSettings: async patch => { saved.push(patch); }, saveAppearance: async id => { appearances.push(id); } },
    'ui/ipc/client.ts': { displayOutputs: async () => [], settingsDefaults: async () => defaults },
    'ui/ipc/bindings.ts': { commands: { fontFamilies: async () => ({ status: 'ok', data: [] }) } },
    'ui/ipc/notifications.ts': { useNotifications: () => [] },
    'ui/ipc/island-state.ts': { dispatch: async () => {} },
  });
  const values = { window: new EventTarget(), navigator: { userAgent: 'Linux' },
    localStorage: { getItem: () => null, setItem() {} },
    document: { documentElement: { dataset: { displayBackend: 'wayland' } }, querySelector: () => null } };
  const before = Object.fromEntries(Object.keys(values).map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  try {
    for (const [key, value] of Object.entries(values)) Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
    const View = d.load('ui/views/SettingsView.tsx').SettingsView;
    const render = () => (tree = d.render(View));
    const select = id => { nodes(tree).find(node => node.props?.['data-section'] === id).props.onClick(); return render(); };
    const groupNode = id => nodes(tree).find(node => node.props?.['data-group'] === id);
    await run({ ...d, render, select, groupNode, saved, appearances, get tree() { return tree; } });
  } finally {
    d.dispose();
    for (const [key, descriptor] of Object.entries(before)) if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key];
  }
}
const toggle = (tree, label) => nodes(tree).find(node => node.type?.name === 'Switch' && node.props['aria-label'] === label);
const fieldset = group => nodes(group).find(node => node.type === 'fieldset');
// Row elements are unexpanded components (props.id) in SettingsView, plain divs in NotchControls.
const rowIds = tree => nodes(tree).map(node => node.props?.['data-setting-id'] ?? (node.type?.name === 'Row' ? node.props.id : null)).filter(Boolean);

const GROUPS = {
  shelf: { parents: ['shelfEnabled'], children: ['shelfMaxItems', 'shelfRemoveAfterDrag'] },
  notchBorder: { parents: ['notchBorder'], children: ['notchBorderColor', 'notchBorderOpacity', 'notchBorderWidth'] },
  gameMode: { parents: ['gameModeAuto', 'gameMode'], children: ['gameModeDnd', 'gameModeFreezeCompanion', 'gameModePausePollers'] },
  clipboard: { parents: ['clipboardHistory'], children: ['clipboardHistorySize', 'clipboardPersist', 'clipboardDenyPatterns'] },
  battery: { parents: ['batteryAlerts'], children: ['batteryLowPercent', 'batteryCriticalPercent'] },
};

test('group rows: the five groups hold exactly their parent and child rows, consecutive in one card', () => {
  assert.deepEqual(Object.fromEntries(model.SETTINGS_GROUPS.map(g => [g.id, { parents: [...g.parents], children: [...g.children] }])), GROUPS);
  const seen = new Set();
  for (const group of model.SETTINGS_GROUPS) {
    const ids = [...group.parents, ...group.children];
    const card = model.settingLocation(ids[0]).card, order = card.rows.map(row => row.id);
    assert.deepEqual(order.slice(order.indexOf(ids[0]), order.indexOf(ids[0]) + ids.length), ids, `${group.id} rows are consecutive, parents first`);
    for (const id of ids) { assert.ok(!seen.has(id), `${id} belongs to one group`); seen.add(id); }
    assert.equal(model.groupSpec(group.id), group);
  }
  // Segments keep every row exactly once and in card order: grouping never drops or moves a row.
  for (const card of model.SETTINGS_CARDS) {
    const flat = model.cardSegments(card).flatMap(s => 'row' in s ? [s.row.id] : [...s.parents, ...s.children].map(row => row.id));
    assert.deepEqual(flat, card.rows.map(row => row.id), card.id);
  }
});

test('group rows: each group is on only while its parent is, and game mode needs just one of its two switches', () => {
  const state = (id, settings) => model.groupSpec(id).enabled(settings);
  // Schema defaults: shelf and notch border are on, so a missing key reads as on (as their switches do).
  assert.deepEqual([state('shelf', {}), state('shelf', { shelfEnabled: false }), state('shelf', { shelfEnabled: true })], [true, false, true]);
  assert.deepEqual([state('notchBorder', {}), state('notchBorder', { notchBorderEnabled: false })], [true, false]);
  assert.deepEqual([state('clipboard', {}), state('clipboard', { clipboardHistory: true }), state('clipboard', { clipboardHistory: false })], [false, true, false]);
  assert.deepEqual([state('battery', { batteryAlerts: true }), state('battery', { batteryAlerts: false })], [true, false]);
  assert.deepEqual([[true, false], [false, true], [true, true], [false, false]].map(([gameModeAuto, gameMode]) => state('gameMode', { gameModeAuto, gameMode })), [true, true, true, false]);
  assert.equal(model.dimmedBy('batteryLowPercent', { batteryAlerts: false })?.id, 'battery');
  assert.equal(model.dimmedBy('batteryLowPercent', { batteryAlerts: true }), null);
  assert.equal(model.dimmedBy('batteryAlerts', { batteryAlerts: false }), null, 'a parent switch is never dimmed');
  assert.equal(model.dimmedBy('weatherUnit', { weatherEnabled: false }), null, 'rows outside the five groups stay live');
});

test('group rows runtime: children sit in a fieldset that is disabled exactly while the parent is off; the parent stays live', async () => {
  const settings = { clipboardHistory: false, clipboardHistorySize: 77, clipboardPersist: true, clipboardDenyPatterns: ['^secret'],
    batteryAlerts: true, batteryLowPercent: 33, batteryCriticalPercent: 9, gameModeAuto: false, gameMode: false, gameModeDnd: true };
  await settingsFixture(async h => {
    h.render(); h.select('apps');
    for (const [id, enabled] of [['clipboard', false], ['battery', true]]) {
      const group = h.groupNode(id), children = fieldset(group);
      assert.equal(children.props['aria-label'], `${model.groupSpec(id).label} options`);
      assert.equal(group.props['data-group-enabled'], enabled, id);
      assert.equal(children.props.disabled, !enabled, `${id} children are out of Tab and pointer reach only while off`);
      assert.deepEqual(rowIds(children), GROUPS[id].children, `${id} children are inside the fieldset`);
      assert.deepEqual(rowIds(group).filter(row => !GROUPS[id].children.includes(row)), GROUPS[id].parents, `${id} parent stays outside it`);
    }
    // Rows outside a group are not wrapped: the Weather card has none.
    assert.ok(labeled(h.tree, 'Temperature unit') && !nodes(h.tree).some(n => n.props?.['data-group'] === 'weather'));
    h.select('notifications');
    const game = h.groupNode('gameMode');
    assert.equal(fieldset(game).props.disabled, true); assert.deepEqual(rowIds(fieldset(game)), GROUPS.gameMode.children);
    assert.ok(!rowIds(game).includes('dnd'), 'Do Not Disturb is its own row, not a game-mode option');
    settings.gameMode = true; h.render();
    assert.equal(fieldset(h.groupNode('gameMode')).props.disabled, false, 'manual game mode alone enables the options');
  }, { settings });
});

test('group rows runtime: switching a parent off and on writes only the parent key, so stored child values survive', async () => {
  const settings = { clipboardHistory: true, clipboardHistorySize: 77, clipboardPersist: true, clipboardDenyPatterns: ['^secret'] };
  await settingsFixture(async h => {
    h.render(); h.select('apps');
    const size = () => labeled(h.tree, 'Clipboard history size').props.value;
    assert.equal(size(), 77);
    toggle(h.tree, 'Clipboard history').props.onChange({ target: { checked: false } });
    assert.deepEqual(h.saved, [{ clipboardHistory: false }], 'only the parent key is written');
    settings.clipboardHistory = false; h.render();
    assert.equal(fieldset(h.groupNode('clipboard')).props.disabled, true);
    assert.equal(size(), 77, 'the dimmed child still shows its stored value');
    assert.equal(labeled(h.tree, 'Keep pinned clips after restart').props.checked, true);
    toggle(h.tree, 'Clipboard history').props.onChange({ target: { checked: true } });
    assert.deepEqual(h.saved, [{ clipboardHistory: false }, { clipboardHistory: true }]);
    settings.clipboardHistory = true; h.render();
    assert.equal(fieldset(h.groupNode('clipboard')).props.disabled, false);
    assert.equal(size(), 77);
    assert.ok(h.saved.every(patch => Object.keys(patch).join() === 'clipboardHistory'), 'no child key was ever written');
  }, { settings });
});

test('group rows: the notch card groups drop shelf and notch border; values stay; the companion card gets no empty group', () => {
  const { NotchControls } = load('ui/island/NotchControls.tsx');
  const calls = [], base = { shelfMaxItems: 7, shelfRemoveAfterDrag: true, notchBorderColor: '#abcdef', notchBorderOpacity: .6, notchBorderWidth: 2.5 };
  const render = (settings, show) => NotchControls({ settings: { ...base, ...settings }, background: '#000000', accent: '#00ff00', patch: p => calls.push(p), ...(show ? { show } : {}) });
  const group = (tree, id) => nodes(tree).find(node => node.props?.['data-group'] === id);
  for (const [shelfEnabled, notchBorderEnabled] of [[true, true], [false, true], [true, false], [false, false]]) {
    const tree = render({ shelfEnabled, notchBorderEnabled });
    for (const [id, enabled] of [['shelf', shelfEnabled], ['notchBorder', notchBorderEnabled]]) {
      const children = fieldset(group(tree, id));
      assert.equal(children.props.disabled, !enabled, `${id} ${enabled}`);
      assert.deepEqual(rowIds(children), GROUPS[id].children);
      assert.deepEqual(rowIds(group(tree, id)).filter(row => !GROUPS[id].children.includes(row)), GROUPS[id].parents);
    }
    // Stored values are shown unchanged whether or not the parent is on.
    assert.equal(labeled(tree, 'Shelf size').props.value, 7);
    assert.equal(labeled(tree, 'Notch border opacity').props.value, 60);
    assert.equal(labeled(tree, 'Notch border thickness').props.value, 2.5);
    assert.equal(nodes(tree).find(n => n.props?.label === 'Notch border colour').props.value, '#abcdef');
  }
  const tree = render({ shelfEnabled: true, notchBorderEnabled: true });
  toggle(tree, 'Drop shelf').props.onChange({ target: { checked: false } });
  toggle(tree, 'Notch border').props.onChange({ target: { checked: false } });
  assert.deepEqual(calls, [{ shelfEnabled: false }, { notchBorderEnabled: false }], 'parents write only their own key');
  const companion = render({}, id => id.startsWith('companion'));
  assert.equal(nodes(companion).filter(node => node.props?.['data-group']).length, 0);
  assert.doesNotMatch(src('ui/island/NotchControls.tsx'), /can be edited while a feature is off/, 'the note no longer promises editing while off');
});

test('group rows: search still finds every dimmed child and a jump can land on it', () => {
  const off = { shelfEnabled: false, notchBorderEnabled: false, clipboardHistory: false, batteryAlerts: false, gameModeAuto: false, gameMode: false };
  const found = query => model.searchRows(query, off).flatMap(group => group.rows.map(row => row.id));
  for (const group of model.SETTINGS_GROUPS) for (const id of group.children) {
    assert.equal(model.dimmedBy(id, off)?.id, group.id);
    assert.ok(found(model.rowSpec(id).label).includes(id), `${id} is searchable while dimmed`);
    assert.ok(model.shownRows(model.settingLocation(id).card.tab, '', off).has(id), `${id} stays on the page while dimmed`);
  }
  // The jump focuses the first enabled control, else the row itself (tabIndex -1): never a disabled control.
  const view = src('ui/views/SettingsView.tsx');
  assert.match(view, /'input:not\(:disabled\), button:not\(:disabled\), textarea:not\(:disabled\), select:not\(:disabled\)'\) \?\? container/);
});

test('group rows: children are indented on a rail and dimmed once, not twice', () => {
  const css = src('ui/views/settings/settings-controls.css');
  assert.match(rule(css, '.settings-group-children'), /margin: 0 0 var\(--space-1\) var\(--space-3\); padding: 0 0 0 var\(--space-3\);.*border-left: 1px solid var\(--v-stroke\)/);
  assert.match(rule(css, '.settings-group-children:disabled'), /pointer-events: none/);
  assert.match(rule(css, '.settings-group-children:disabled > .settings-row > *'), /opacity: \.4/);
  assert.match(css, /\n\.island \.settings-group-children:disabled :is\(button, input, textarea\):disabled \{ opacity: 1; \}/);
  // Row separators survive the wrapper elements.
  assert.match(css, /\n\.settings-row \+ \.settings-group, \.settings-group \+ \.settings-row, \.settings-group \+ \.settings-group \{ border-top: 1px solid var\(--v-stroke\); \}/);
  assert.match(rule(css, '.settings-group-children > .settings-row'), /border-top: 1px solid var\(--v-stroke\)/);
  const group = src('ui/views/settings/SettingsGroup.tsx');
  assert.match(group, /<fieldset className="settings-group-children" disabled=\{!enabled\}/);
});

// ── Colour scheme swatch grid ───────────────────────────────────────────────────────────
const schemes = load('ui/island/tokens/schemes.ts');
function pickerFixture(props, run) {
  const d = driver({
    'framer-motion': { motion: { div: 'motion-div' }, AnimatePresence: 'presence' },
    'ui/island/components/motion-preference.ts': { useReducedMotionAll: () => false },
    'ui/island/album-scheme.ts': { useAlbumPreview: () => null },
    'ui/island/components/ColorField.tsx': { ColorSwatch: 'color-swatch' },
  });
  try { return run(d.render(d.load('ui/island/SchemePicker.tsx').SchemePicker, { settings: {}, patch() {}, onPick() {}, ...props })); } finally { d.dispose(); }
}
const swatches = tree => nodes(tree).filter(node => node.props?.role === 'radio');

test('scheme swatch grid: every scheme is one tile, in order, with a unique readable name', () => {
  const grids = schemes.schemeGrids();
  assert.deepEqual(grids.map(g => g.id), ['monolith', 'community']);
  assert.deepEqual(grids.flatMap(g => g.schemes.map(s => s.id)).sort(), schemes.SCHEMES.map(s => s.id).sort(), 'every scheme stays selectable, once');
  const names = grids.flatMap(g => g.schemes.map(s => schemes.schemeLabel(s)));
  assert.equal(new Set(names).size, names.length, 'no two tiles share a name');
  const name = id => schemes.schemeLabel(schemes.SCHEMES.find(s => s.id === id));
  assert.deepEqual(['catppuccin-latte', 'rose-pine-dawn', 'dracula', 'green', 'skin'].map(name).slice(0, 3), ['Catppuccin Latte', 'Rosé Pine Dawn', 'Dracula']);
  assert.equal(name('green'), schemes.SCHEMES.find(s => s.id === 'green').name, 'Monolith presets keep their own name');
  assert.ok(names.includes('Tokyo Night') && names.includes('Tokyo Night Storm'), 'a variant named like its family is not doubled');
});

test('scheme swatch grid runtime: one radiogroup, selected tile checked and the only Tab stop, click picks', () => {
  const picked = [];
  pickerFixture({ value: 'dracula', onPick: id => picked.push(id) }, tree => {
    const group = nodes(tree).find(node => node.props?.role === 'radiogroup'), tiles = swatches(tree);
    assert.equal(group.props['aria-label'], 'Colour scheme'); assert.equal(typeof group.props.onKeyDown, 'function');
    assert.equal(tiles.length, schemes.SCHEMES.length);
    assert.ok(tiles.every(tile => tile.type === 'button' && tile.props.className === 'scheme-swatch' && tile.props['data-scheme-nav'] === ''));
    assert.deepEqual(tiles.filter(tile => tile.props['aria-checked']).map(tile => tile.props['data-testid']), ['appearance-dracula']);
    assert.deepEqual(tiles.filter(tile => tile.props.tabIndex === 0).map(tile => tile.props['data-testid']), ['appearance-dracula'], 'roving tabindex');
    assert.ok(tiles.filter(tile => tile.props.tabIndex !== 0).every(tile => tile.props.tabIndex === -1));
    for (const tile of tiles) {
      const visible = nodes(tile).find(node => node.props?.className === 'scheme-name').props.children;
      assert.equal(tile.props['aria-label'], visible, 'the accessible name is the visible name');
      assert.ok(tile.props['data-tooltip'].startsWith(`${visible} · `), 'the shared tooltip carries the full name and note');
    }
    assert.equal(nodes(tree).filter(node => node.props?.className === 'scheme-grid').length, 2);
    assert.equal(nodes(tree).some(node => node.props?.className === 'scheme-custom'), false, 'seed colours stay closed unless Custom is selected');
    tiles.find(tile => tile.props['data-testid'] === 'appearance-nord').props.onClick();
    tiles.find(tile => tile.props['data-testid'] === 'appearance-dracula').props.onClick();
    assert.deepEqual(picked, ['nord'], 'picking the selected scheme again writes nothing');
  });
  pickerFixture({ value: 'custom' }, tree => assert.ok(nodes(tree).some(node => node.props?.className === 'scheme-custom'), 'Custom opens its seed colours under the grid'));
});

test('scheme swatch grid keyboard: arrows move focus through the grid and across the two groups; Home/End jump', () => {
  const target = schemes.gridTarget, sizes = [13, 16];
  assert.deepEqual([target('ArrowRight', 0, sizes, 4), target('ArrowRight', 12, sizes, 4), target('ArrowRight', 28, sizes, 4)], [1, 13, 28]);
  assert.deepEqual([target('ArrowLeft', 5, sizes, 4), target('ArrowLeft', 13, sizes, 4), target('ArrowLeft', 0, sizes, 4)], [4, 12, 0]);
  assert.deepEqual([target('ArrowDown', 1, sizes, 4), target('ArrowDown', 9, sizes, 4), target('ArrowDown', 10, sizes, 4)], [5, 12, 12], 'down keeps the column, else lands on the short last row');
  assert.deepEqual([target('ArrowDown', 12, sizes, 4), target('ArrowDown', 25, sizes, 4), target('ArrowDown', 27, sizes, 4)], [13, 25, 27], 'down crosses into the next grid and stops at the end');
  assert.deepEqual([target('ArrowUp', 5, sizes, 4), target('ArrowUp', 2, sizes, 4), target('ArrowUp', 13, sizes, 4), target('ArrowUp', 15, sizes, 4)], [1, 2, 12, 12], 'up crosses back into the previous grid');
  assert.deepEqual([target('Home', 20, sizes, 4), target('End', 3, sizes, 4), target('ArrowDown', -1, sizes, 4)], [0, 28, 0]);
  assert.deepEqual([target('a', 3, sizes, 4), target('Enter', 3, sizes, 4), target(' ', 3, sizes, 4), target('ArrowDown', 0, [], 4)], [null, null, null, null], 'Enter and Space stay the button\'s own click');
  // The handler moves real focus to the computed swatch and reads the column count from the laid-out grid.
  const tiles = Array.from({ length: 29 }, (_, i) => ({ i, focus() { focused = i; } })); let focused = -1, prevented = 0;
  const grid = (from, to) => ({ querySelectorAll: () => tiles.slice(from, to) });
  const group = { querySelectorAll: selector => selector === '.scheme-grid' ? [grid(0, 13), grid(13, 29)] : tiles };
  const before = Object.fromEntries(['document', 'getComputedStyle'].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  try {
    Object.defineProperty(globalThis, 'getComputedStyle', { value: () => ({ gridTemplateColumns: '120px 120px 120px 120px' }), configurable: true, writable: true });
    Object.defineProperty(globalThis, 'document', { value: { activeElement: tiles[9], documentElement: { dataset: {} } }, configurable: true, writable: true });
    pickerFixture({ value: 'dracula' }, tree => {
      const onKeyDown = nodes(tree).find(node => node.props?.role === 'radiogroup').props.onKeyDown;
      onKeyDown({ key: 'ArrowDown', currentTarget: group, preventDefault() { prevented++; } }); assert.equal(focused, 12);
      globalThis.document.activeElement = tiles[12];
      onKeyDown({ key: 'ArrowRight', currentTarget: group, preventDefault() { prevented++; } }); assert.equal(focused, 13);
      onKeyDown({ key: 'Enter', currentTarget: group, preventDefault() { prevented++; } }); assert.equal(prevented, 2, 'Enter is left to the button');
    });
  } finally { for (const [key, descriptor] of Object.entries(before)) if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; }
});

test('scheme swatch grid: four across, selected tile ringed in accent, focus ring kept, accordion gone', () => {
  const css = src('ui/island/tokens/scheme-picker.css');
  assert.match(rule(css, '.scheme-grid'), /display: grid; grid-template-columns: repeat\(4, minmax\(0, 1fr\)\); gap: var\(--space-2\)/);
  assert.match(rule(css, ".island .scheme-swatch[aria-checked='true']"), /border-color: var\(--ui-accent\); box-shadow: inset 0 0 0 1px var\(--ui-accent\)/);
  assert.match(rule(css, '.scheme-swatch:focus-visible'), /outline: 2px solid var\(--ui-focus, var\(--ui-accent\)\)/);
  assert.match(rule(css, '.scheme-name'), /overflow: hidden; text-overflow: ellipsis; white-space: nowrap/);
  const picker = src('ui/island/SchemePicker.tsx');
  assert.match(picker, /<button key=\{s\.id\} type="button" role="radio"/, 'real buttons: Enter and Space pick natively');
  assert.match(picker, /reduceMotion === true/); assert.match(picker, /duration: 0/);
  for (const file of ['ui/island/SchemePicker.tsx', 'ui/island/tokens/scheme-picker.css', 'ui/island/polish.css', 'ui/island/motion-transient.css', 'ui/island/tokens/light-tone.css'])
    assert.doesNotMatch(src(file), /scheme-(family|variant|chevron|selected|row|item|note)|aria-expanded/, `${file} keeps no accordion leftovers`);
});

// ── Reset section ───────────────────────────────────────────────────────────────────────
// What the per-card Resets restored at ad902e3, per section (cards with `reset: false` excluded).
const SECTION_KEYS = {
  appearance: ['customAccent', 'customBackground', 'customText', 'hermesThemeSync', 'accent', 'background', 'borderColor', 'translucent', 'glassStrength', 'borderEnabled', 'reduceMotion', 'animationSpeed', 'uiFont'],
  companion: ['companionEnabled', 'companionCharacter', 'companionReactive', 'companionMonitor'],
  advanced: ['displayBackend', 'displayId', 'offsetX', 'offsetY'],
};

test('reset section: each section resets exactly the keys its cards reset before, and nothing from another section', () => {
  const tabs = model.SETTINGS_SECTIONS.map(section => section.id);
  for (const tab of tabs) {
    const cards = model.SETTINGS_CARDS.filter(card => card.tab === tab && card.reset !== false);
    assert.deepEqual(model.sectionKeys(tab), [...new Set(cards.flatMap(card => model.cardKeys(card.id)))], `${tab} is the union of its cards' keys`);
    const defaults = Object.fromEntries(model.sectionKeys(tab).map(key => [key, `default:${key}`]));
    assert.deepEqual(model.sectionResetPatch(tab, { ...defaults, unrelated: 1 }), defaults, `${tab} patch holds exactly its keys`);
    assert.deepEqual(model.sectionResetPatch(tab, defaults), Object.assign({}, ...cards.map(card => model.resetPatch(card.id, defaults))), `${tab} equals its cards' resets`);
    assert.equal(model.sectionResetsAppearance(tab), tab === 'appearance', `${tab} scheme reset`);
  }
  for (const [tab, keys] of Object.entries(SECTION_KEYS)) assert.deepEqual(model.sectionKeys(tab), keys, tab);
  // No key is reset by two sections; cards that own their state (launch command, backup, layout editor) stay out.
  const all = tabs.flatMap(tab => model.sectionKeys(tab));
  assert.equal(new Set(all).size, all.length);
  assert.ok(!model.sectionKeys('advanced').includes('moduleRegistry') && !model.sectionKeys('apps').includes('moduleRegistry'), 'the app arrangement keeps its resetKeys: []');
  assert.ok(!all.includes('terminalCommand'), 'no section Reset wipes the Hermes launch command');
  assert.ok(model.sectionKeys('terminal').includes('terminalFontSize'), 'Terminal still resets its other rows');
  for (const key of ['shelfMaxItems', 'notchBorderOpacity']) assert.ok(model.sectionKeys('notch').includes(key));
  assert.ok(model.sectionKeys('notifications').includes('gameModeDnd') && !model.sectionKeys('notifications').includes('clipboardHistory'));
});

test('reset section runtime: one quiet Reset on the title line of every section; cards carry no Reset', async () => {
  const defaults = Object.fromEntries(model.SETTINGS_SECTIONS.flatMap(section => model.sectionKeys(section.id)).map(key => [key, `default:${key}`]));
  await settingsFixture(async h => {
    h.render();
    for (const section of model.SETTINGS_SECTIONS) {
      const tree = h.select(section.id), head = nodes(tree).find(node => node.props?.className === 'settings-section-head');
      const resets = nodes(tree).filter(node => node.type?.name === 'SectionReset');
      assert.equal(resets.length, 1, `${section.id} has one Reset`);
      assert.ok(nodes(head).includes(resets[0]), 'it sits on the page-title line');
      assert.equal(resets[0].props.name, section.name);
      assert.equal(nodes(tree).filter(node => node.type === 'button' && node.props.children === 'Reset').length, 0, `${section.id} cards carry no Reset`);
      h.saved.length = 0; await resets[0].props.onReset();
      assert.deepEqual(Object.keys(h.saved[0]), model.sectionKeys(section.id), `${section.id} writes exactly its keys`);
      assert.ok(Object.entries(h.saved[0]).every(([key, value]) => value === `default:${key}`), 'each key gets its Rust default');
    }
    assert.deepEqual(h.appearances, ['skin'], 'only Appearance returns the scheme to the default');
    // The button itself: the shared quiet variant, a plain-words label, then an inline confirm.
    const view = src('ui/views/SettingsView.tsx');
    assert.match(view, /<button type="button" data-variant="quiet" aria-label=\{`Reset \$\{name\} section`\} disabled=\{busy\} onClick=\{\(\) => confirmation\.arm\(true\)\}>Reset section<\/button>/);
    assert.doesNotMatch(view, /ResetHeader|className="link"[^>]*>Reset</);
  }, { settings: {}, defaults, appearance: 'dracula' });
});

// Cards marked `reset: false` keep their state behind their own controls (layout editor, backup).
// Their rows hold no setting key today, so dropping the filter would change no real patch. The first
// test pins the filter; the second presses the real button with keys planted in the driver's own
// copy of the model: the layout rows as if they were settings, and General's old launch-command card.
test('reset section: a card marked reset: false is never among the cards a section resets', () => {
  const kept = model.SETTINGS_CARDS.filter(card => card.reset === false);
  assert.deepEqual(kept.map(card => `${card.tab}/${card.id}`), ['appearance/layoutEditor', 'advanced/backup']);
  for (const card of kept) assert.ok(!model.sectionResetCards(card.tab).includes(card), `${card.id} stays out of ${card.tab}`);
});

test('reset section runtime: Reset on Appearance and on General never writes a key held by a reset: false card', async () => {
  const PLANTED = { appearance: ['sideButtons', 'islandSize'], general: ['terminalCommand'] };
  const keys = [...model.sectionKeys('appearance'), ...model.sectionKeys('general'), ...Object.values(PLANTED).flat()];
  await settingsFixture(async h => {
    const fixture = h.load('ui/views/settings-model.ts');
    fixture.cardSpec('layoutEditor').rows.forEach((row, index) => { row.keys = [PLANTED.appearance[index]]; });
    fixture.SETTINGS_CARDS.push({ id: 'terminalCommand', title: 'Terminal launch', tab: 'general', reset: false, rows: [{ ...fixture.rowSpec('terminalCommand'), resetKeys: undefined }] });
    assert.deepEqual(fixture.cardKeys('layoutEditor'), PLANTED.appearance); assert.deepEqual(fixture.cardKeys('terminalCommand'), PLANTED.general);
    h.render();
    for (const [tab, planted] of Object.entries(PLANTED)) {
      const reset = nodes(h.select(tab)).find(node => node.type?.name === 'SectionReset');
      h.saved.length = 0; await reset.props.onReset();
      assert.equal(h.saved.length, 1, `${tab} saves one patch`);
      for (const key of planted) assert.ok(!(key in h.saved[0]), `${tab} Reset leaves ${key} alone`);
      assert.deepEqual(Object.keys(h.saved[0]), model.sectionKeys(tab), `${tab} still resets every other card`);
    }
  }, { settings: {}, defaults: Object.fromEntries(keys.map(key => [key, `default:${key}`])) });
});

// ── Weather location field group ─────────────────────────────────────────────────────────
const text = node => node == null || typeof node === 'boolean' ? '' : typeof node !== 'object' ? String(node)
  : Array.isArray(node) ? node.map(text).join('') : text(node.props?.children);

test('weather location: status, Clear, field and Find are one form on a two-column grid', () => {
  const place = { name: 'Palm Coast', latitude: 29.58497, longitude: -81.20784, country: 'United States' };
  const d = driver({ 'ui/ipc/bindings.ts': { commands: { weatherGeocode: async () => ({ status: 'ok', data: [] }) } } });
  try {
    const Location = d.load('ui/views/settings/WidgetControls.tsx').WeatherLocation;
    for (const value of [place, null]) {
      const form = d.render(Location, { value, onSave: async () => {} });
      assert.equal(form.type, 'form'); assert.equal(form.props.className, 'settings-draft-field settings-place');
      // Source order is reading order: row one is the saved place and its Clear, row two the field and Find.
      const [status, clear, field, find, ...rest] = [form.props.children].flat().filter(Boolean);
      assert.deepEqual(rest, [], 'results and errors appear only after a search');
      assert.equal(status.props.className, 'settings-place-status');
      if (value) {
        assert.equal(text(nodes(status).find(node => node.type === 'span' && node !== status)), 'Palm Coast · United States');
        assert.equal(text(nodes(status).find(node => node.type === 'small')), '29.58497, -81.20784');
      } else assert.equal(text(status), 'No location selected');
      assert.equal(text(clear), 'Clear location'); assert.equal(clear.props.disabled, !value);
      assert.equal(clear.props.type, 'button', 'Clear never submits the search'); assert.equal(clear.props['data-variant'], 'quiet');
      assert.equal(field.type, 'input'); assert.equal(field.props['aria-label'], 'Find weather location');
      assert.equal(text(find), 'Find'); assert.equal(find.props.type, 'submit'); assert.equal(find.props['data-variant'], 'secondary');
      assert.equal(nodes(form).filter(node => node.props?.className === 'link').length, 0, 'the Phase 3 variants, not bare links');
    }
  } finally { d.dispose(); }
  const css = src('ui/island/surface.css');
  assert.match(rule(css, '.settings-panel .settings-place'), /display: grid; grid-template-columns: minmax\(0, 1fr\) auto; align-items: center;/);
  for (const selector of ['.settings-place-results', '.settings-place .settings-field-error']) assert.match(rule(css, selector), /grid-column: 1 \/ -1;/, `${selector} spans the group`);
  assert.match(rule(css, ".settings-place > button[type='submit']"), /align-self: stretch/, 'Find is as tall as the field');
  assert.doesNotMatch(css, /\.settings-draft-field form \{/, 'no nested form is left to lay out');
});
