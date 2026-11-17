import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
import { driver, nodes } from './phase7b-runtime-harness.mjs';
const src = p => readFileSync(new URL(`../${p}`, import.meta.url), 'utf8');
const { ShelfApp } = load('ui/island/components/ShelfApp.tsx');
const item = present => ({ id: 'file:///fixture/report.pdf', uri: 'file:///fixture/report.pdf', path: '/fixture/report.pdf', name: 'report.pdf', present, folder: false, addedMs: 1 });
function shelf(present, ops = []) {
  const file = item(present);
  const tree = ShelfApp({ model: { enabled: true, maxItems: 12, items: [file] }, removeAfterDrag: false,
    intents: { shelf: op => ops.push(op), copy: path => ops.push({ type: 'copy', path }) } });
  const row = nodes(tree).find(n => n.type === 'li');
  return nodes(row).filter(n => n.type === 'button');
}

test('page polish b: launcher run list and AI sparkles use different Framework7 glyphs, with the same labels', () => {
  const d = driver({ 'ui/ipc/client.ts': {}, 'ui/ipc/island-state.ts': {}, 'ui/stores/apps.ts': {} });
  const { launcherIcons, viewIcons, viewLabels } = d.load('ui/island/Launcher.tsx');
  const { GLYPHS, GLYPH_SOURCE } = load('ui/island/components/icon-glyphs.ts');
  assert.equal(viewLabels.agents, 'Agent runs'); assert.equal(viewLabels.ai, 'AI Agents');
  assert.equal(GLYPH_SOURCE[launcherIcons.agents], 'list_bullet');
  assert.equal(GLYPH_SOURCE[launcherIcons.ai], 'sparkles');
  assert.equal(viewIcons.agents, 'spark', 'configured side actions are outside this change');
  assert.notDeepEqual(GLYPHS[launcherIcons.agents], GLYPHS[launcherIcons.ai]);
  assert.ok(GLYPHS[launcherIcons.agents][0].length > 100);
  const launcher = src('ui/island/Launcher.tsx');
  assert.match(launcher, /name=\{view \? launcherIcons\[view\]/);
  assert.match(launcher, /name=\{dragged\.view \? launcherIcons\[dragged\.view\]/);
  d.dispose();
});

test('page polish b: present Shelf row has one primary, quiet secondary actions and danger Remove in the original order', () => {
  const buttons = shelf(true);
  assert.deepEqual(buttons.map(b => [b.props.children, b.props['data-variant']]), [['Open', 'primary'], ['Reveal', 'quiet'], ['Copy path', 'quiet'], ['Remove', 'danger']]);
  assert.equal(buttons.filter(b => b.props['data-variant'] === 'primary').length, 1);
});

test('page polish b: missing Shelf row promotes Copy path, keeps danger Remove, and never offers Open/Reveal', () => {
  assert.deepEqual(shelf(false).map(b => [b.props.children, b.props['data-variant']]), [['Copy path', 'primary'], ['Remove', 'danger']]);
});

test('page polish b: every Shelf action retains its label, native keyboard reachability and original intent', () => {
  const ops = [], buttons = shelf(true, ops);
  assert.deepEqual(buttons.map(b => b.props['aria-label']), ['Open report.pdf', 'Reveal report.pdf', 'Copy path of report.pdf', 'Remove report.pdf from the shelf']);
  for (const b of buttons) { assert.equal(b.props.disabled, undefined); assert.equal(b.props.tabIndex, undefined); b.props.onClick(); }
  assert.deepEqual(ops, [{ type: 'open', id: item(true).id }, { type: 'reveal', id: item(true).id }, { type: 'copy', path: item(true).path }, { type: 'remove', id: item(true).id }]);
  const missingOps = []; shelf(false, missingOps).forEach(b => b.props.onClick());
  assert.deepEqual(missingOps, [{ type: 'copy', path: item(false).path }, { type: 'remove', id: item(false).id }]);
});

test('page polish b: mixer columns share available width; names wrap without ellipsis or a smaller font', () => {
  const css = src('ui/island/surface.css');
  const rule = name => css.match(new RegExp(`\\.${name} \\{([^}]+)\\}`))[1];
  assert.match(rule('fader-col'), /flex: 1 0 96px/);
  assert.match(rule('fader-name'), /min-height: 32px; flex: 1/);
  const label = rule('fader-name span');
  assert.match(label, /white-space: normal/); assert.match(label, /overflow-wrap: anywhere/);
  assert.doesNotMatch(label, /ellipsis|overflow: hidden/);
  assert.match(rule('fader-name'), /font-size: var\(--fs-sm\)/);
});

test('page polish b: mixer adapter preserves long app names and supplied symbolic icons or paths without inventing icons', () => {
  const { adaptMixer } = load('ui/views/app-adapters.ts');
  for (const iconName of ['firefox', '/fixture/icons/player.png', null]) {
    const name = 'AnUnbrokenApplicationNameForWrapping';
    const model = adaptMixer({ rev: 1, available: true, detail: '', defaultSink: null, sinks: [], apps: [], streams: [
      { index: 1, sinkIndex: 2, appName: name, binary: null, pid: null, mediaName: null, iconName, muted: false, corked: false, volumePercent: 40 },
    ] });
    assert.equal(model.value.streams[0].appName, name); assert.equal(model.value.streams[0].iconName, iconName);
  }
});
