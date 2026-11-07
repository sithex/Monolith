import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
import { driver, nodes, labeled, textButton, deferred, tick } from './phase7b-runtime-harness.mjs';

const model = load('ui/views/settings-model.ts');
const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const allRows = model.SETTINGS_CARDS.flatMap(card => card.rows);
const rowIds = groups => groups.flatMap(group => group.rows.map(row => row.id));
const sections = ['general', 'notifications', 'look', 'notch', 'terminal', 'apps', 'island'];
const newKeys = ['autostart', 'showInTaskbar', 'hourFormat', 'hoverDelay', 'toastTtlMs', 'clipboardHistory',
  'clipboardHistorySize', 'clipboardPersist', 'clipboardDenyPatterns', 'weatherEnabled', 'weatherLocation',
  'weatherUnit', 'batteryAlerts', 'batteryLowPercent', 'batteryCriticalPercent', 'lyricsEnabled',
  'equalizerEnabled', 'equalizerBands', 'displayId', 'moduleRegistry'];

test('sidebar: every Rust schema key is covered by row.keys, with no unknown keys', () => {
  const schema = src('src-tauri/src/contract/settings.rs').match(/settings_schema! \{([\s\S]*?)\n\}/)?.[1];
  assert.ok(schema, 'parse the actual Rust schema, not a hand-maintained key list');
  const rustKeys = [...schema.matchAll(/^\s*\w+: [^\n]*, "(\w+)",/gm)].map(match => match[1]);
  const generated = src('ui/ipc/bindings.ts').match(/export type Settings = \{([\s\S]*?)\n\};/)[1]
    .replace(/\/\*\*[\s\S]*?\*\//g, '');
  const generatedKeys = [...generated.matchAll(/^\s*(\w+)\??:/gm)].map(match => match[1]);
  assert.ok(rustKeys.includes('moduleRegistry') && rustKeys.includes('equalizerBands'));
  assert.deepEqual([...rustKeys].sort(), generatedKeys.sort(), 'schema parser and generated contract agree');
  assert.deepEqual([...new Set(allRows.flatMap(row => row.keys))].sort(), [...rustKeys].sort());
  assert.equal(newKeys.length, 20);
  for (const key of newKeys) assert.ok(allRows.some(row => row.keys.includes(key)), key);
});

test('sidebar: section/card/row IDs are unique and nonempty; every card belongs to a valid section', () => {
  assert.deepEqual(model.SETTINGS_SECTIONS.map(section => section.id), sections);
  for (const list of [model.SETTINGS_SECTIONS, model.SETTINGS_CARDS, allRows]) {
    assert.equal(new Set(list.map(item => item.id)).size, list.length);
    for (const item of list) assert.match(item.id, /^[A-Za-z][A-Za-z0-9]*$/);
  }
  for (const section of model.SETTINGS_SECTIONS) {
    assert.ok(section.name.trim());
    assert.ok(model.SETTINGS_CARDS.some(card => card.tab === section.id));
  }
  for (const card of model.SETTINGS_CARDS) {
    assert.ok(sections.includes(card.tab), card.id);
    assert.ok(card.title.trim() && card.rows.length, card.id);
    for (const row of card.rows) {
      assert.ok(row.label.trim(), row.id);
      assert.deepEqual(model.settingLocation(row.id), { card, row });
    }
  }
  assert.equal(model.settingLocation('not-a-setting'), null);
});

test('sidebar: all pre-sidebar row IDs remain valid jump targets', () => {
  const legacyIds = ['swipeNavigation', 'stealth', 'clickSounds', 'openShortcut', 'paletteShortcut',
    'gameModeAuto', 'gameMode', 'gameModeDnd', 'gameModeFreezeCompanion', 'gameModePausePollers',
    'terminalCommand', 'notificationsEnabled', 'dnd', 'systemNotificationMirror', 'silenceKdePopups',
    'notificationSound', 'notificationSoundId', 'approvalsInNotch', 'redactBodies', 'deniedApps', 'appRules',
    'showDate', 'displayBackend', 'hardwareMonitor', 'arrangeApps', 'exportSettings', 'importSettings',
    'scheme', 'hermesThemeSync', 'overrides', 'translucent', 'glassStrength', 'borderEnabled', 'reduceMotion',
    'animationSpeed', 'everyMonitor', 'notchMonitors', 'waitingPulse', 'hoverPeek', 'companion',
    'companionCharacter', 'companionReactive', 'companionMonitor', 'notchColor', 'notchWidth', 'notchBorder',
    'notchBorderColor', 'notchBorderOpacity', 'notchBorderWidth', 'uiFont', 'terminalFontSize',
    'terminalFontFamily', 'terminalScrollback', 'terminalCursorStyle', 'terminalCursorBlink',
    'sideButtons', 'islandSize', 'offsetX', 'offsetY'];
  for (const id of legacyIds) assert.equal(model.settingLocation(id)?.row.id, id);
});

test('sidebar: search indexes every label, card title, section, schema key, and synonym even with parents off', () => {
  const off = { translucent: false, companionEnabled: false, showNotchOnEveryMonitor: false,
    reduceMotion: true, clipboardHistory: false, weatherEnabled: false, batteryAlerts: false,
    lyricsEnabled: false, equalizerEnabled: false, notificationSound: false, notificationsEnabled: false };
  for (const card of model.SETTINGS_CARDS) for (const row of card.rows) {
    for (const query of [row.label, card.title, model.sectionName(card.tab), ...row.keys,
      ...(row.synonyms?.split(/\s+/) ?? [])]) {
      assert.ok(rowIds(model.searchRows(query.toUpperCase(), off)).includes(row.id), `${row.id}: ${query}`);
    }
    assert.ok(model.shownRows(card.tab, '', off).has(row.id), `${row.id} remains editable with parent off`);
  }
  assert.ok(rowIds(model.searchRows('MÁSCOT', off)).includes('companionCharacter'));
  assert.ok(rowIds(model.searchRows('look blur', off)).includes('glassStrength'));
  assert.deepEqual(model.searchRows('look blur unmatchable-token', off), []);
});

test('sidebar: CSS reserves a 40px rail, gives content flex:1/min-width:0, and adapts at 420px', () => {
  const css = src('ui/island/surface.css');
  const rule = selector => css.match(new RegExp(`${selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\{([^}]+)\\}`))?.[1] ?? '';
  assert.match(rule('.settings-layout'), /display:\s*flex\s*;/);
  assert.match(rule('.settings-rail'), /width:\s*40px\s*;/);
  assert.match(rule('.settings-rail'), /flex:\s*0\s+0\s+40px\s*;/);
  assert.match(rule('.settings-content'), /flex:\s*1\s*;/);
  assert.match(rule('.settings-content'), /min-width:\s*0\s*;/);
  assert.match(rule('.settings-content'), /overflow-y:\s*auto\s*;/);
  assert.match(rule('.settings-panel'), /container-type:\s*inline-size\s*;/);
  const narrow = css.match(/@container\s*\(max-width:\s*420px\)\s*\{([\s\S]*?)\n\}/)?.[1] ?? '';
  assert.match(narrow, /\.settings-layout\s*\{[^}]*flex-direction:\s*column/);
  assert.match(narrow, /\.settings-rail\s*\{[^}]*flex-direction:\s*row[^}]*overflow-x:\s*auto/);
  assert.match(narrow, /\.settings-rail-name\s*\{[^}]*visibility:\s*visible/);
});

test('sidebar: saved sections validate supported IDs and reject old tabs/corrupt storage', () => {
  for (const id of sections) assert.equal(model.savedSection(id), id);
  for (const value of [null, '', 'appearance', 'layout', 'unknown', 'Terminal', '__proto__']) assert.equal(model.savedSection(value), 'general');
});

class FixtureCustomEvent extends Event { constructor(name, init) { super(name); this.detail = init?.detail; } }
async function fixture(run, { stored = null, settings = {}, dispatch } = {}) {
  const saved = [], intents = [], lookups = [], focused = [], storage = new Map(stored ? [[model.SECTION_STORAGE_KEY, stored]] : []);
  const events = new EventTarget(); let tree;
  const d = driver({
    'ui/stores/index.ts': { useSettings: () => settings, useAppearance: () => 'skin', usePalette: () => ({}), saveSettings: async patch => saved.push(patch) },
    'ui/ipc/client.ts': { displayOutputs: async () => [], settingsDefaults: async () => ({}) },
    'ui/ipc/bindings.ts': { commands: { fontFamilies: async () => ({ status: 'ok', data: [] }) } },
    'ui/ipc/notifications.ts': { useNotifications: () => [] },
    'ui/ipc/island-state.ts': { dispatch: async intent => { intents.push(intent); if (dispatch) await dispatch(intent); } },
  });
  const values = { window: events, CustomEvent: FixtureCustomEvent, navigator: { userAgent: 'Linux' },
    localStorage: { getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) },
    document: { documentElement: { dataset: { displayBackend: 'wayland' } }, querySelector: selector => {
      const id = selector.match(/^\.settings-panel \[data-setting-id="([^"]+)"\]$/)?.[1];
      assert.ok(id, `jump must use stable data-setting-id, not label: ${selector}`); lookups.push(id);
      assert.ok(nodes(tree).some(node => node.props?.id === id || node.props?.['data-setting-id'] === id), `${id} is in committed section tree`);
      return { scrollIntoView: () => focused.push(['scroll', id]), querySelector: () => ({ focus: () => focused.push(['focus', id]) }), setAttribute() {}, removeAttribute() {} };
    } } };
  const before = Object.fromEntries(Object.keys(values).map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  try {
    for (const [key, value] of Object.entries(values)) Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
    const View = d.load('ui/views/SettingsView.tsx').SettingsView;
    // driver flushes effects before returning; publish the rendered tree before each effect.
    const Wrapped = () => { tree = View(); return tree; };
    const render = () => d.render(Wrapped);
    const select = id => { nodes(tree).find(node => node.props?.['data-section'] === id).props.onClick(); return render(); };
    const query = value => { labeled(tree, 'Search settings').props.onChange({ target: { value } }); return render(); };
    const selected = () => nodes(tree).find(node => node.props?.['aria-current'] === 'page')?.props['data-section'];
    await run({ ...d, render, select, query, selected, saved, intents, events, storage, focused, lookups, get tree() { return tree; } });
  } finally {
    d.dispose();
    for (const [key, descriptor] of Object.entries(before)) if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key];
  }
}

test('sidebar runtime: stored section restores on mount and rail clicks persist the new section', async () => {
  await fixture(async h => {
    h.render(); assert.equal(h.selected(), 'apps');
    assert.ok(nodes(h.tree).some(node => node.props?.id === 'equalizerBands'));
    h.select('notifications'); assert.equal(h.selected(), 'notifications');
    assert.equal(h.storage.get(model.SECTION_STORAGE_KEY), 'notifications');
    assert.deepEqual(nodes(h.tree).filter(node => node.props?.['data-section']).map(node => node.props['data-section']), sections);
  }, { stored: 'apps' });
});

test('sidebar runtime: unknown jump is ignored; ID jump switches section, clears search, and focuses exact target', async () => {
  await fixture(async h => {
    h.render(); h.query('anything');
    h.events.dispatchEvent(new FixtureCustomEvent('island-setting-jump', { detail: 'not-a-setting' }));
    h.render(); assert.equal(h.selected(), 'general'); assert.equal(labeled(h.tree, 'Search settings').props.value, 'anything');
    h.events.dispatchEvent(new FixtureCustomEvent('island-setting-jump', { detail: 'terminalFontFamily' }));
    h.render(); assert.equal(h.selected(), 'terminal'); assert.equal(labeled(h.tree, 'Search settings').props.value, '');
    assert.deepEqual(h.focused, [['scroll', 'terminalFontFamily'], ['focus', 'terminalFontFamily']]);
    h.render(); assert.equal(h.tree.props['data-highlight'], 'terminalFontFamily');
  });
});

test('sidebar runtime: search result click selects its section and renders the target without enabling the parent', async () => {
  await fixture(async h => {
    h.render(); h.query('equalizerBands');
    const results = nodes(h.tree).filter(node => node.props?.className === 'settings-result');
    assert.equal(results.length, 1); assert.equal(results[0].props.children[0].props.children, 'Equalizer bands');
    results[0].props.onClick(); h.render();
    assert.equal(h.selected(), 'apps'); assert.equal(labeled(h.tree, 'Search settings').props.value, '');
    assert.ok(labeled(h.tree, 'Equalizer bands')); assert.deepEqual(h.saved, []);
    assert.deepEqual(h.focused, [['scroll', 'equalizerBands'], ['focus', 'equalizerBands']]);
  }, { settings: { equalizerEnabled: false, equalizerBands: 16 } });
});

test('sidebar runtime: actual openSetting retains a jump while Settings is initially unmounted, then consumes it once', async () => {
  await fixture(async h => {
    const link = h.load('ui/views/settings/SettingsLink.tsx');
    await link.openSetting('terminalCommand');
    assert.deepEqual(h.intents, [{ type: 'selectView', view: 'settings' }]);
    h.render(); h.render();
    assert.equal(h.selected(), 'terminal'); assert.equal(link.takePendingSettingJump(), null);
    assert.deepEqual(h.focused, [['scroll', 'terminalCommand'], ['focus', 'terminalCommand']]);
    h.render(); assert.equal(h.focused.length, 2, 'no duplicate jump on rerender');
  });
});

test('sidebar runtime: actual openSetting delivers to mounted Settings and failed navigation clears its pending jump', async () => {
  await fixture(async h => {
    h.render(); h.query('not found');
    const link = h.load('ui/views/settings/SettingsLink.tsx');
    await link.openSetting('clipboardHistorySize'); h.render();
    assert.equal(h.selected(), 'apps'); assert.equal(link.takePendingSettingJump(), null);
    assert.deepEqual(h.focused, [['scroll', 'clipboardHistorySize'], ['focus', 'clipboardHistorySize']]);
  });
  await fixture(async h => {
    const link = h.load('ui/views/settings/SettingsLink.tsx');
    await assert.rejects(link.openSetting('terminalCommand'), /fixture navigation rejected/);
    assert.equal(link.takePendingSettingJump(), null);
    h.render(); assert.equal(h.selected(), 'general');
  }, { dispatch: async () => { throw new Error('fixture navigation rejected'); } });
});

test('sidebar runtime: openSetting queues before navigation finishes and superseded dispatches do not resurrect old jumps', async () => {
  const navigation = [deferred(), deferred()]; let index = 0;
  await fixture(async h => {
    const link = h.load('ui/views/settings/SettingsLink.tsx');
    const old = link.openSetting('terminalCommand'), latest = link.openSetting('equalizerBands');
    await tick(); assert.equal(h.intents.length, 2);
    h.render(); h.render(); assert.equal(h.selected(), 'apps');
    navigation[1].resolve(); await latest; h.render();
    navigation[0].resolve(); await old; h.render();
    assert.equal(h.selected(), 'apps');
    assert.ok(!h.lookups.includes('terminalCommand'));
    assert.equal(link.takePendingSettingJump(), null);
  }, { dispatch: () => navigation[index++].promise });
});

test('sidebar runtime: island section exposes offset controls with exact ranges and commits', async () => {
  await fixture(async h => {
    h.render(); h.select('island');
    for (const [id, label, min, max] of [['offsetX', 'Horizontal offset', -400, 400], ['offsetY', 'Vertical offset', -200, 200]]) {
      const control = labeled(h.tree, label);
      assert.deepEqual([control.props.min, control.props.max], [min, max]);
      control.props.onCommit(12, false); assert.deepEqual(h.saved, []);
      control.props.onCommit(12, true); assert.deepEqual(h.saved.pop(), { [id]: 12 });
    }
  });
});

test('sidebar runtime: new range controls expose exact schema bounds and persist only final commits', async () => {
  await fixture(async h => {
    h.render();
    for (const [section, key, label, min, max, step] of [
      ['general', 'hoverDelay', 'Hover delay', 0, 1200, 10],
      ['notifications', 'toastTtlMs', 'Popup duration', 1500, 20000, 500],
      ['apps', 'clipboardHistorySize', 'Clipboard history size', 10, 500, 1],
      ['apps', 'batteryLowPercent', 'Low battery threshold', 5, 50, 1],
      ['apps', 'batteryCriticalPercent', 'Critical battery threshold', 1, 30, 1],
      ['apps', 'equalizerBands', 'Equalizer bands', 8, 32, 1],
    ]) {
      h.select(section); const control = labeled(h.tree, label);
      assert.ok(control, key); assert.deepEqual([control.props.min, control.props.max, control.props.step], [min, max, step], key);
      const count = h.saved.length; control.props.onCommit(min, false); assert.equal(h.saved.length, count);
      control.props.onCommit(max, true); assert.deepEqual(h.saved.at(-1), { [key]: max });
    }
    assert.ok(h.saved.every(patch => !Object.hasOwn(patch, 'equalizerEnabled') && !Object.hasOwn(patch, 'clipboardHistory') && !Object.hasOwn(patch, 'batteryAlerts')));
  }, { settings: { equalizerEnabled: false, clipboardHistory: false, batteryAlerts: false } });
});

test('sidebar runtime: hour, weather unit, UI font and cursor enums emit exact contract strings', async () => {
  await fixture(async h => {
    h.render();
    for (const [section, label, key, choices] of [
      ['general', 'Time format', 'hourFormat', [['12 hour', 'h12'], ['24 hour', 'h24']]],
      ['apps', 'Temperature unit', 'weatherUnit', [['Fahrenheit', 'fahrenheit'], ['Celsius', 'celsius']]],
      ['look', 'UI font', 'uiFont', [['System', 'system'], ['OpenRunde', 'openrunde']]],
      ['terminal', 'Terminal cursor', 'terminalCursorStyle', [['Block', 'block'], ['Bar', 'bar'], ['Underline', 'underline']]],
    ]) {
      h.select(section); const group = nodes(h.tree).find(node => node.props?.role === 'group' && node.props['aria-label'] === label);
      assert.ok(group, key);
      for (const [name, value] of choices) { textButton(group, name).props.onClick(); assert.deepEqual(h.saved.at(-1), { [key]: value }); }
    }
  });
});

test('sidebar runtime: new feature switches save one boolean key and do not start dependent services', async () => {
  await fixture(async h => {
    h.render();
    for (const [section, key, label] of [
      ['general', 'autostart', 'Start at login'], ['general', 'showInTaskbar', 'Show in taskbar'],
      ['apps', 'clipboardHistory', 'Clipboard history'], ['apps', 'clipboardPersist', 'Keep pinned clips after restart'],
      ['apps', 'weatherEnabled', 'Weather'], ['apps', 'batteryAlerts', 'Battery alerts'],
      ['apps', 'lyricsEnabled', 'Synced lyrics'], ['apps', 'equalizerEnabled', 'Live equalizer'],
    ]) {
      h.select(section); const control = nodes(h.tree).find(node => node.type === 'input' && node.props['aria-label'] === label); assert.equal(control.props.checked, false, key);
      control.props.onChange({ target: { checked: true } }); assert.deepEqual(h.saved.at(-1), { [key]: true });
      control.props.onChange({ target: { checked: false } }); assert.deepEqual(h.saved.at(-1), { [key]: false });
    }
    assert.deepEqual(h.intents, []);
  });
});

test('sidebar runtime: nullable display choice preserves saved output and emits exact displayId patches', async () => {
  const d = driver(), patches = [];
  try {
    const Choice = d.load('ui/views/settings/WidgetControls.tsx').DisplayChoice;
    const tree = d.render(Choice, { value: 'HDMI-A-1', outputs: ['DP-1', 'DP-1'], onChange: displayId => patches.push({ displayId }) });
    assert.equal(tree.props.label, 'Island display'); assert.equal(tree.props.value, 'HDMI-A-1');
    assert.deepEqual(tree.props.options, [{ value: '', label: 'Default display' }, { value: 'DP-1', label: 'DP-1' }, { value: 'HDMI-A-1', label: 'HDMI-A-1' }]);
    tree.props.onChange('DP-1'); tree.props.onChange('');
    assert.deepEqual(patches, [{ displayId: 'DP-1' }, { displayId: null }]);
  } finally { d.dispose(); }
});

test('sidebar runtime: clipboard patterns save explicitly, preserve regex text, and show rejected-save errors', async () => {
  const d = driver(), patches = []; let reject = false;
  try {
    const Patterns = d.load('ui/views/settings/WidgetControls.tsx').ClipboardPatterns;
    const props = { value: [], onSave: async patch => { patches.push(patch); if (reject) throw new Error('Invalid regex'); } };
    let tree = d.render(Patterns, props);
    labeled(tree, 'Clipboard exclusion patterns').props.onChange({ target: { value: '^secret\\d+\n(?i)token' } });
    tree = d.render(Patterns, props); assert.deepEqual(patches, [], 'drafts do not persist while typing');
    tree.props.onSubmit({ preventDefault() {} }); await tick(); tree = d.render(Patterns, props);
    assert.deepEqual(patches.pop(), { clipboardDenyPatterns: ['^secret\\d+', '(?i)token'] });
    reject = true; tree.props.onSubmit({ preventDefault() {} }); await tick(); tree = d.render(Patterns, props);
    assert.equal(nodes(tree).find(node => node.props?.role === 'alert').props.children, 'Invalid regex');
    reject = false; labeled(tree, 'Clipboard exclusion patterns').props.onChange({ target: { value: '' } }); tree = d.render(Patterns, props);
    tree.props.onSubmit({ preventDefault() {} }); await tick();
    assert.deepEqual(patches.at(-1), { clipboardDenyPatterns: [] });
  } finally { d.dispose(); }
});

test('sidebar runtime: weather geocodes only on explicit submit, selects full GeoPlace and clears with null', async () => {
  const queries = [], patches = [], place = { name: 'Toronto', latitude: 43.65, longitude: -79.38, country: 'CA' };
  const d = driver({ 'ui/ipc/bindings.ts': { commands: { weatherGeocode: async query => { queries.push(query); return { status: 'ok', data: [place] }; } } } });
  try {
    const Location = d.load('ui/views/settings/WidgetControls.tsx').WeatherLocation;
    const props = { value: null, onSave: async patch => patches.push(patch) };
    let tree = d.render(Location, props); assert.deepEqual(queries, []);
    labeled(tree, 'Find weather location').props.onChange({ target: { value: '  Toronto  ' } }); tree = d.render(Location, props);
    assert.deepEqual(queries, []);
    nodes(tree).find(node => node.type === 'form').props.onSubmit({ preventDefault() {} }); await tick(); tree = d.render(Location, props);
    assert.deepEqual(queries, ['Toronto']); assert.deepEqual(patches, []);
    const result = nodes(labeled(tree, 'Weather locations')).find(node => node.type === 'button');
    result.props.onClick(); await tick(); tree = d.render(Location, { ...props, value: place });
    assert.deepEqual(patches, [{ weatherLocation: place }]); assert.equal(labeled(tree, 'Weather locations'), undefined);
    textButton(tree, 'Clear location').props.onClick(); await tick(); assert.deepEqual(patches.at(-1), { weatherLocation: null });
  } finally { d.dispose(); }
});

test('sidebar runtime: weather ignores stale search results and renders command errors inline', async () => {
  const pending = [deferred(), deferred()]; let index = 0;
  const d = driver({ 'ui/ipc/bindings.ts': { commands: { weatherGeocode: () => pending[index++].promise } } });
  try {
    const Location = d.load('ui/views/settings/WidgetControls.tsx').WeatherLocation;
    const props = { value: null, onSave: async () => assert.fail('search never saves implicitly') };
    let tree = d.render(Location, props);
    const search = query => {
      labeled(tree, 'Find weather location').props.onChange({ target: { value: query } }); tree = d.render(Location, props);
      nodes(tree).find(node => node.type === 'form').props.onSubmit({ preventDefault() {} }); tree = d.render(Location, props);
    };
    search('old'); search('new');
    pending[1].resolve({ status: 'error', error: { code: 'unavailable', message: 'Weather service unavailable' } }); await tick(); tree = d.render(Location, props);
    assert.equal(nodes(tree).find(node => node.props?.role === 'alert').props.children, 'Weather service unavailable');
    pending[0].resolve({ status: 'ok', data: [{ name: 'Stale city' }] }); await tick(); tree = d.render(Location, props);
    assert.equal(labeled(tree, 'Weather locations'), undefined);
    assert.equal(nodes(tree).find(node => node.props?.role === 'alert').props.children, 'Weather service unavailable');
  } finally { d.dispose(); }
});
