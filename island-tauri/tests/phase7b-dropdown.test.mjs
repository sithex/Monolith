import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { driver, nodes, globals } from './phase7b-runtime-harness.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const src = path => readFileSync(join(root, path), 'utf8');
const css = src('ui/island/components/dropdown.css');
const OPTIONS = [
  { value: 'a', label: 'Alpha', hint: 'first' }, { value: 'b', label: 'Beta' },
  { value: 'c', label: 'Gamma', hint: 'third' }, { value: 'd', label: 'Delta' }, { value: 'e', label: 'Epsilon' },
];
const text = node => typeof node === 'string' ? node : Array.isArray(node) ? node.map(text).join('') : node?.props ? text(node.props.children) : '';
const byClass = (tree, cls) => nodes(tree).filter(n => typeof n.props?.className === 'string' && n.props.className.split(' ').includes(cls));
const key = (k, log = []) => ({ key: k, preventDefault() { log.push(`prevent:${k}`); }, stopPropagation() { log.push(`stop:${k}`); } });

/** Private DOM fixtures for the whole test body (handlers read `document` when they run). */
function dom(dataset, run) {
  const listeners = [];
  const document = { documentElement: { dataset }, addEventListener: (...a) => listeners.push(a), removeEventListener() {} };
  return globals({ document, matchMedia: () => ({ matches: false }) }, () => run(listeners));
}
/** Mount one Dropdown through the hook driver; call inside dom(). */
function mount(props = {}) {
  const changes = [], d = driver();
  const { Dropdown } = d.load('ui/island/components/Dropdown.tsx');
  const all = { label: 'Pick', value: 'b', options: OPTIONS, onChange: v => changes.push(v), ...props };
  return { d, render: () => d.render(Dropdown, all), changes, all };
}
const trigger = tree => nodes(tree).find(n => n.type === 'button' && n.props.className === 'dropdown-trigger');
const menu = tree => nodes(tree).find(n => n.props?.role === 'listbox');

test('Dropdown is a listbox of options with hints, the selected row checked in the accent', () => dom({}, () => {
  const m = mount();
  let tree = m.render();
  assert.equal(menu(tree), undefined, 'closed: no menu mounted');
  const t = trigger(tree);
  assert.equal(t.props['aria-haspopup'], 'listbox'); assert.equal(t.props['aria-expanded'], false); assert.equal(t.props['aria-label'], 'Pick');
  assert.equal(text(byClass(tree, 'dropdown-value')[0]), 'Beta');
  t.props.onClick(); tree = m.render();
  const list = menu(tree);
  assert.equal(list.type, 'ul'); assert.equal(list.props['aria-label'], 'Pick'); assert.equal(list.props.id, t.props['aria-controls']);
  assert.equal(list.props['data-state'], 'open'); assert.equal(list.props.inert, undefined);
  assert.equal(trigger(tree).props['aria-expanded'], true); assert.equal(tree.props['data-open'], true);
  const rows = nodes(list).filter(n => n.props?.role === 'option');
  assert.deepEqual(rows.map(r => text(byClass(r, 'dropdown-label'))), ['Alpha', 'Beta', 'Gamma', 'Delta', 'Epsilon']);
  assert.deepEqual(rows.map(r => text(byClass(r, 'dropdown-hint'))), ['first', '', 'third', '', '']);
  assert.deepEqual(rows.map(r => r.props['aria-selected']), [false, true, false, false, false]);
  assert.deepEqual(rows.map(r => r.props['data-focus']), [false, true, false, false, false], 'focus starts on the selected row');
  // Exactly one checkmark, on the selected row; the others keep an aligned empty slot.
  const checks = rows.map(r => nodes(r).filter(n => n.type?.name === 'Check' || n.props?.className === 'dropdown-check'));
  assert.deepEqual(checks.map(c => c.length), [0, 1, 0, 0, 0]);
  assert.deepEqual(rows.map(r => byClass(r, 'dropdown-check-slot').length), [1, 0, 1, 1, 1]);
  const svg = checks[1][0].type(checks[1][0].props ?? {});
  assert.equal(svg.props.className, 'dropdown-check');
  assert.match(css, /\.dropdown-check \{ color: var\(--ui-accent\); \}/);
  m.d.dispose();
}));

test('Dropdown keyboard: arrows wrap, Home/End, Enter picks instantly, Escape is kept from closing the island', () => dom({ navVia: 'key' }, () => {
  const m = mount();
  let tree = m.render(), log = [];
  tree.props.onKeyDown(key('ArrowDown', log)); tree = m.render();
  assert.equal(menu(tree).props['data-state'], 'open'); assert.deepEqual(log, ['prevent:ArrowDown']);
  const focused = () => nodes(menu(tree)).filter(n => n.props?.role === 'option').findIndex(r => r.props['data-focus']);
  assert.equal(focused(), 1);
  tree.props.onKeyDown(key('ArrowDown')); tree = m.render(); assert.equal(focused(), 2);
  tree.props.onKeyDown(key('End')); tree = m.render(); assert.equal(focused(), 4);
  tree.props.onKeyDown(key('ArrowDown')); tree = m.render(); assert.equal(focused(), 0, 'wraps to the first row');
  tree.props.onKeyDown(key('ArrowUp')); tree = m.render(); assert.equal(focused(), 4, 'wraps to the last row');
  tree.props.onKeyDown(key('Home')); tree = m.render(); assert.equal(focused(), 0);
  tree.props.onKeyDown(key('ArrowDown')); tree.props.onKeyDown(key('ArrowDown')); tree = m.render();
  tree.props.onKeyDown(key('Enter')); tree = m.render();
  assert.deepEqual(m.changes, ['c']);
  assert.equal(menu(tree), undefined, 'keyboard pick unmounts at once, no exit animation');
  // Escape: preventDefault + stopPropagation so the island itself stays open.
  tree.props.onKeyDown(key('Enter')); tree = m.render(); assert.ok(menu(tree));
  log = []; tree.props.onKeyDown(key('Escape', log)); tree = m.render();
  assert.deepEqual(log, ['prevent:Escape', 'stop:Escape']);
  assert.equal(menu(tree), undefined); assert.deepEqual(m.changes, ['c'], 'Escape never changes the value');
  // Tab closes without swallowing focus movement.
  tree.props.onKeyDown(key(' ')); tree = m.render(); assert.ok(menu(tree));
  log = []; tree.props.onKeyDown(key('Tab', log)); tree = m.render();
  assert.deepEqual(log, []); assert.equal(menu(tree), undefined);
  m.d.dispose();
}));

test('Dropdown pointer close plays the exit: closing state stays mounted, inert, until the card animation ends', () => dom({}, listeners => {
  const m = mount();
  let tree = m.render();
  trigger(tree).props.onClick(); tree = m.render();
  assert.equal(listeners.filter(([type]) => type === 'pointerdown').length, 1, 'click-outside listener while open');
  const rows = () => nodes(menu(tree)).filter(n => n.props?.role === 'option');
  rows()[3].props.onClick(); tree = m.render();
  assert.deepEqual(m.changes, ['d']);
  const closing = menu(tree);
  assert.equal(closing.props['data-state'], 'closing'); assert.equal(closing.props.inert, true);
  assert.equal(tree.props['data-open'], false);
  // A row's own animationend (bubbling) must not cut the card's exit short.
  closing.props.onAnimationEnd({ target: {}, currentTarget: closing }); tree = m.render();
  assert.equal(menu(tree).props['data-state'], 'closing');
  const card = {}; menu(tree).props.onAnimationEnd({ target: card, currentTarget: card }); tree = m.render();
  assert.equal(menu(tree), undefined, 'unmounted after the exit animation');
  // Picking the current value closes but does not report a change.
  trigger(tree).props.onClick(); tree = m.render(); rows()[1].props.onClick();
  assert.deepEqual(m.changes, ['d']);
  m.d.dispose();
}));

test('Dropdown under reduced motion closes instantly; disabled or empty never opens', () => dom({ reduceMotion: 'true' }, () => {
  const m = mount();
  let tree = m.render();
  trigger(tree).props.onClick(); tree = m.render(); assert.ok(menu(tree));
  trigger(tree).props.onClick(); tree = m.render(); assert.equal(menu(tree), undefined);
  m.d.dispose();
  for (const props of [{ disabled: true }, { options: [] }]) {
    const n = mount(props);
    let t = n.render();
    assert.equal(trigger(t).props.disabled, true);
    t.props.onKeyDown(key('ArrowDown')); t = n.render(); assert.equal(menu(t), undefined);
    n.d.dispose();
  }
}));

test('Dropdown trigger renders icon, prefix, value and placeholder; align and up are exposed for CSS', () => dom({}, () => {
  const m = mount({ icon: 'mic', prefix: 'Voice', align: 'end' });
  const tree = m.render(), t = trigger(tree);
  assert.equal(tree.props['data-align'], 'end'); assert.equal(tree.props['data-up'], false);
  const kids = nodes(t).filter(n => typeof n.props?.className === 'string').map(n => n.props.className);
  assert.deepEqual(kids.slice(0, 4), ['dropdown-trigger', 'dropdown-icon', 'dropdown-prefix', 'dropdown-value'], 'icon, then prefix, then value');
  const icon = byClass(t, 'dropdown-icon')[0];
  assert.equal(icon.props['aria-hidden'], 'true');
  assert.equal(nodes(icon).find(n => n.type?.name === 'Icon').props.name, 'mic');
  assert.equal(text(byClass(t, 'dropdown-prefix')), 'Voice');
  assert.equal(text(byClass(t, 'dropdown-value')), 'Beta');
  assert.equal(byClass(t, 'dropdown-value')[0].props['data-placeholder'], undefined);
  m.d.dispose();
  const bare = mount({ value: 'zzz', placeholder: 'Unknown voice' });
  const b = trigger(bare.render());
  assert.equal(byClass(b, 'dropdown-icon').length, 0); assert.equal(byClass(b, 'dropdown-prefix').length, 0);
  assert.equal(text(byClass(b, 'dropdown-value')), 'Unknown voice');
  assert.equal(byClass(b, 'dropdown-value')[0].props['data-placeholder'], true);
  bare.d.dispose();
  assert.match(css, /\.dropdown\[data-open='true'\] \.dropdown-chevron \{ transform: rotate\(180deg\); \}/);
  assert.match(css, /\.dropdown-chevron \{[^}]*transition: transform calc\(200ms \/ var\(--motion-speed, 1\)\) var\(--ease-out\);/);
  assert.match(css, /\.dropdown-value \{[^}]*font-weight: var\(--fw-strong\);/);
  assert.match(css, /\.dropdown-label \{[^}]*font-weight: var\(--fw-strong\);/);
}));

test('Dropdown is the shared primitive: re-exported from AppPrimitives, used by the voice pill and AI period pickers', () => {
  assert.match(src('ui/island/components/AppPrimitives.tsx'), /^export \{ Dropdown \} from '\.\/Dropdown';$/m);
  assert.doesNotMatch(src('ui/island/components/AppPrimitives.tsx'), /function Dropdown/);
  assert.match(src('ui/island/Island.tsx'), /import '\.\/components\/dropdown\.css';/);
  const voice = src('ui/island/VoiceControls.tsx');
  assert.match(voice, /<Dropdown label="Hermes voice"[^>]*icon="mic" prefix="Voice" align="end"/);
  assert.match(voice, /disabled=\{busy\}/);
  assert.match(src('ui/island/components/ControlsApp.tsx'), /<Dropdown label="Hermes voice"[\s\S]*?onChange=\{name => intents\.tile\(\{ type: 'voice', name \}\)\}/);
  for (const [file, label] of [['AiInsights.tsx', 'Insights period'], ['AiDashboard.tsx', 'Spending period']])
    assert.match(src(`ui/island/components/${file}`), new RegExp(`<Dropdown label="${label}" value=\\{period\\} options=\\{PERIOD_OPTIONS\\}`));
  const font = src('ui/views/settings/FontPicker.tsx');
  assert.match(font, /useMenuPresence/); assert.match(font, /className="dropdown-menu font-menu" data-state=\{menu\.state\}/);
});

test('no native <select> anywhere under ui/ outside the legacy views', () => {
  const offenders = [];
  const visit = dir => { for (const name of readdirSync(dir)) {
    const path = join(dir, name), rel = relative(root, path);
    if (rel.startsWith(join('ui', 'legacy'))) continue;
    if (statSync(path).isDirectory()) visit(path);
    else if (/\.(tsx|ts|html)$/.test(name) && /<select[\s>]/.test(readFileSync(path, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, ''))) offenders.push(rel);
  } };
  visit(join(root, 'ui'));
  assert.deepEqual(offenders, []);
});

test('dropdown.css: theme tokens only, rows 36 px, 3xl card, stagger 30 ms for four rows capped at 90 ms', () => {
  const body = css.replace(/\/\*[\s\S]*?\*\//g, '');
  assert.doesNotMatch(body, /#[0-9a-f]{3,8}\b/i, 'no hex colours');
  assert.doesNotMatch(body, /\b(?:rgba?|hsla?|oklch|lab|lch)\(/i, 'no literal colour functions');
  assert.doesNotMatch(body, /(?:^|[\s:,(])(?:white|black|red|green|blue|gray|grey|silver|orange|yellow|purple|pink)(?=[\s;,)])/i, 'no named colours');
  assert.doesNotMatch(body, /backdrop-filter/);
  assert.match(body, /\.island \.dropdown-trigger \{[^}]*border-radius: var\(--radius-full\); background: var\(--v-fill\);/);
  assert.match(body, /@media \(hover: hover\) and \(pointer: fine\) \{\s*\.island \.dropdown-trigger:hover:not\(:disabled\) \{ background: var\(--v-fill-hi\); \}/);
  assert.match(body, /\.dropdown-trigger:active:not\(:disabled\) \{ transform: scale\(\.98\); \}/);
  assert.match(body, /\.dropdown-trigger:focus-visible \{ outline: 2px solid var\(--ui-focus\);/);
  assert.match(body, /\.island \.dropdown-menu \{[^}]*border: \.5px solid var\(--ui-border\); border-radius: var\(--radius-3xl\);/);
  assert.match(body, /\.island \.dropdown-menu \{[^}]*background-image: linear-gradient\(var\(--ui-raised\), var\(--ui-raised\)\)/);
  assert.match(body, /\.island \.dropdown-menu li \{[^}]*min-height: 36px;[^}]*border-radius: var\(--radius-lg\);/);
  assert.match(body, /li\[data-focus='true'\] \{ background: var\(--v-hover\); \}/);
  // Stagger: 0 / 30 / 60 ms, then every later row shares the 90 ms of the fourth.
  const delays = [...body.matchAll(/> li(:nth-child\([^)]*\))? \{ animation-delay: calc\((\d+)ms/g)].map(m => [m[1], +m[2]]);
  assert.deepEqual(delays, [[':nth-child(2)', 30], [':nth-child(3)', 60], [':nth-child(n+4)', 90]]);
  for (const [, ms] of body.matchAll(/animation-delay:[^;]*?(\d+)ms/g)) assert.ok(+ms <= 90, `delay ${ms}ms > 90ms`);
  // Keyboard and reduced motion: the card and its rows do not animate; the exit is hidden at once.
  for (const scope of [`html\\[data-nav-via='key'\\]`, `html\\[data-reduce-motion='true'\\]`]) {
    assert.match(body, new RegExp(`${scope} \\.island \\.dropdown-menu\\[data-state\\][^{]*\\{ animation: none; \\}`));
    assert.match(body, new RegExp(`${scope} \\.island \\.dropdown-menu > li[^{]*\\{ animation: none; \\}`));
    assert.match(body, new RegExp(`${scope} \\.island \\.dropdown-menu\\[data-state='closing'\\][^{]*\\{ visibility: hidden; \\}`));
  }
  assert.match(body, /@media \(prefers-reduced-motion: reduce\) \{\s*:root \.island \.dropdown-menu\[data-state\], :root \.island \.dropdown-menu > li \{ animation: none; \}/);
  // WebKitGTK ignores var() inside a keyframe step's timing function: keyframes carry none.
  for (const [, kf] of body.matchAll(/@keyframes [\w-]+ \{([^\n]*)\}/g)) assert.doesNotMatch(kf, /animation-timing-function|var\(/);
  // Stale homes of the dropdown rules are gone.
  for (const file of ['ui/island/surface.css', 'ui/island/polish.css', 'ui/island/motion-transient.css', 'ui/views/settings/settings-controls.css'])
    assert.doesNotMatch(src(file).replace(/\/\*[\s\S]*?\*\//g, ''), /\.dropdown-(?:menu|trigger)|\.font-menu|\.font-option/, file);
});
