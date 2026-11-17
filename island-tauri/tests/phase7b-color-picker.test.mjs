import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { load } from './phase7b-loader.mjs';
import { driver, nodes, labeled, textButton, globals, deferred } from './phase7b-runtime-harness.mjs';

const path = 'ui/island/components/ColorField.tsx';
const api = load(path);
const src = file => readFileSync(new URL(`../${file}`, import.meta.url), 'utf8');
const css = src('ui/island/components/color-picker.css');
const recentKey = 'monolith.colorPicker.recent', tabKey = 'monolith.colorPicker.tab';
const tabs = tree => nodes(tree).filter(n => n.props?.role === 'tab');
const motion = { motion: { div: 'motion-div' }, AnimatePresence: 'presence', useIsPresent: () => true };

async function fixture(run, { tab = 'Grid', recent = '[]', reduced = false, nav = 'pointer', noStorage = false, value = '#123456' } = {}) {
  const saved = new Map([[tabKey, tab], [recentKey, recent]]), picked = [], focused = [];
  const state = { mode: 'large', view: 'settings' }, win = new EventTarget();
  const root = { dataset: { navVia: nav } };
  const values = { window: win, innerWidth: 1200, innerHeight: 800,
    localStorage: { getItem(key) { if (noStorage) throw Error('unavailable'); return saved.get(key) ?? null; }, setItem(key, value) { if (noStorage) throw Error('unavailable'); saved.set(key, value); } },
    document: { documentElement: root, querySelector: () => null, querySelectorAll: () => [], getElementById: id => ({ focus() { focused.push(id); } }) },
    getComputedStyle: () => ({ getPropertyValue: key => ({ '--skin-accent': '#abcdef', '--skin-text': '#eeeeee', '--skin-background': '#111111', '--skin-surface': '#222222', '--skin-dim': '#777777' })[key] ?? '' }) };
  const before = Object.fromEntries(Object.keys(values).map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]));
  const d = driver({ 'framer-motion': motion, 'ui/ipc/island-state.ts': { useIslandState: () => state }, 'ui/island/components/motion-preference.ts': { useReducedMotionAll: () => reduced } });
  try {
    for (const [key, value] of Object.entries(values)) Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
    const module = d.load(path), C = module.ColorPickerPanel;
    const request = { id: 'test-swatch', label: 'Notch colour', value, anchorX: 900, anchorY: 70, onPick: hex => picked.push(hex) };
    module.colorPicker.open(request);
    const render = () => d.render(C);
    render(); let tree = render();
    await run({ ...module, ...d, render, tree, saved, picked, state, win, root, request, focused });
  } finally {
    d.dispose();
    for (const [key, previous] of Object.entries(before)) if (previous) Object.defineProperty(globalThis, key, previous); else delete globalThis[key];
  }
}

const keyEvent = key => ({ key, preventDefault() { this.prevented = true; } });
function pointerTarget() {
  const handlers = new Map(); let capture = null, reads = 0;
  const target = {
    focus() {}, setPointerCapture(id) { capture = id; }, hasPointerCapture(id) { return capture === id; }, releasePointerCapture() { capture = null; },
    getBoundingClientRect() { reads++; return { left: 0, top: 0, width: 100, height: 100 }; },
    addEventListener(type, fn) { handlers.set(type, fn); }, removeEventListener(type) { handlers.delete(type); },
    fire(type, x, y, id = 1) { handlers.get(type)?.({ type, pointerId: id, clientX: x, clientY: y }); },
    get capture() { return capture; }, get reads() { return reads; }, get listeners() { return handlers.size; },
  };
  return target;
}
const down = (target, x = 50, y = 50, id = 1) => ({ currentTarget: target, clientX: x, clientY: y, pointerId: id, button: 0, isPrimary: true, preventDefault() {} });

test('colour conversion: hex RGB HSV round trips, achromatic and dark colours, hue wrapping and channel clamping', () => {
  const { hexToRgb, rgbToHex, rgbToHsv, hsvToRgb, hexToHsv, hsvToHex } = api;
  for (let r = 0; r <= 255; r += 17) for (let g = 0; g <= 255; g += 17) for (let b = 0; b <= 255; b += 17) {
    const rgb = { r, g, b }, hex = rgbToHex(rgb);
    assert.deepEqual(hexToRgb(hex), rgb); assert.deepEqual(hsvToRgb(rgbToHsv(rgb)), rgb); assert.equal(hsvToHex(hexToHsv(hex)), hex);
  }
  for (const hex of ['#010203', '#090001', '#123456', '#000000', '#ffffff']) assert.equal(hsvToHex(hexToHsv(hex)), hex);
  assert.deepEqual(rgbToHsv({ r: 0, g: 0, b: 0 }), { h: 0, s: 0, v: 0 });
  assert.equal(hsvToHex({ h: 360, s: 1, v: 1 }), '#ff0000'); assert.equal(hsvToHex({ h: -120, s: 1, v: 1 }), '#0000ff');
  assert.equal(rgbToHex({ r: -1, g: 256, b: 127.5 }), '#00ff80'); assert.equal(rgbToHex({ r: NaN, g: Infinity, b: 5 }), '#000005');
});

test('hex validation accepts six channels only and normalizes optional hash/case/whitespace', () => {
  for (const value of ['#123456', '123456', ' ABCDEF ', '#ABCDEF']) assert.equal(api.parseHexInput(value), value.includes('123') ? '#123456' : '#abcdef');
  for (const value of ['', '#abc', '#ffffffff', '#12z456', 'rgb(1,2,3)']) assert.equal(api.parseHexInput(value), null);
});

test('recent list is validated, case-insensitively deduplicated, newest first, immutable, capped at 12', () => {
  const input = ['#ABCDEF', '#abcdef', '#123456', null, '#abc', 7];
  assert.deepEqual(api.recentColors(input), ['#abcdef', '#123456']);
  assert.deepEqual(api.addRecentColor(input, '#123456'), ['#123456', '#abcdef']); assert.equal(input.length, 6);
  const many = Array.from({ length: 15 }, (_, i) => `#${i.toString(16).padStart(6, '0')}`);
  assert.deepEqual(api.addRecentColor(many, '#ff0000'), ['#ff0000', ...many.slice(0, 11)]);
  for (const value of [null, {}, 'oops', 4]) assert.deepEqual(api.recentColors(value), []);
  assert.deepEqual(api.addRecentColor(['#123456'], 'bad'), ['#123456']);
});

test('Apple grid preserves the complete 144-cell source palette including duplicates', () => {
  assert.equal(api.APPLE_GRID_COLORS.length, 144);
  // Digest pins every source swatch and its order without duplicating the production palette.
  assert.equal(createHash('sha256').update(api.APPLE_GRID_COLORS.join(',')).digest('hex'), '4e1370f6462f90ffd97f768c9386c54bf63d9dab13cce82425483b1dada4b2f5');
  assert.equal(api.gridSelection('#FFFFFF'), 0); assert.equal(api.gridSelection('#000000'), 11); assert.equal(api.gridSelection('#123456'), -1);
});

test('grid navigation moves 12 columns per vertical key, clamps edges and supports row Home/End', () => {
  const cases = [[0, 'ArrowLeft', 0], [0, 'ArrowUp', 0], [143, 'ArrowRight', 143], [143, 'ArrowDown', 143], [11, 'ArrowRight', 12], [12, 'ArrowLeft', 11], [15, 'ArrowUp', 3], [15, 'ArrowDown', 27], [15, 'Home', 12], [15, 'End', 23], [143, 'Home', 132], [143, 'End', 143], [1, 'Enter', -1], [1, ' ', -1], [1, 'Tab', -1]];
  for (const [index, key, expected] of cases) assert.equal(api.gridNavigation(index, key), expected, `${index}/${key}`);
  assert.equal(api.gridNavigation(14, 'End', 17), 16); assert.equal(api.gridNavigation(0, 'ArrowRight', 0), -1);
});

test('grid runtime: one checked duplicate, one roving Tab stop, arrows move focus without committing, native buttons pick', () => {
  const d = driver(), picked = [], C = d.load(path).ColorGrid;
  try {
    const props = { hex: '#000000', onPick: c => picked.push(c) }; let tree = d.render(C, props);
    const cells = nodes(tree).filter(n => n.props?.role === 'radio');
    assert.equal(tree.props.role, 'radiogroup'); assert.equal(cells.length, 144); assert.equal(new Set(cells.map(n => n.key)).size, 144);
    assert.deepEqual(cells.filter(n => n.props['aria-checked']).map(n => n.key), ['11']); assert.equal(cells.filter(n => n.props.tabIndex === 0).length, 1);
    let focused = -1; const children = cells.map((_, i) => ({ focus() { focused = i; } }));
    const event = { ...keyEvent('ArrowDown'), currentTarget: { parentElement: { children } } };
    cells[11].props.onKeyDown(event); assert.equal(focused, 23); assert.ok(event.prevented); assert.deepEqual(picked, []);
    tree = d.render(C, props); assert.equal(nodes(tree).find(n => n.key === '23').props.tabIndex, 0);
    assert.ok(cells.every(n => n.type === 'button' && n.props.type === 'button'), 'Enter/Space use native button activation');
    cells[23].props.onClick(); assert.deepEqual(picked, ['#330033']);
  } finally { d.dispose(); }
});

test('ColorSwatch API keeps open/toggle, anchor, callback round trip and dialog semantics', () => {
  const d = driver(); const win = new EventTarget(); let changes = 0;
  globals({ window: win, innerWidth: 1200, innerHeight: 800 }, () => {
    const { ColorSwatch, colorPicker } = d.load(path), picked = [], props = { id: 'swatch', label: 'Test colour', value: '#123456', onPick: c => picked.push(c), resetLabel: 'Default', onReset() {} };
    win.addEventListener('island-surfaces-changed', () => changes++);
    try {
      let tree = d.render(ColorSwatch, props); assert.equal(tree.type, 'button'); assert.equal(tree.props['aria-haspopup'], 'dialog'); assert.equal(tree.props['aria-expanded'], false);
      tree.props.ref.current = { getBoundingClientRect: () => ({ left: 20, top: 30, width: 40, height: 20 }) };
      tree.props.onClick(); assert.equal(colorPicker.get().anchorX, 40); assert.equal(colorPicker.get().anchorY, 40);
      const unsubscribe = colorPicker.subscribe(() => changes++);
      tree = d.render(ColorSwatch, { ...props, value: '#abcdef', onPick: c => picked.push(`new:${c}`) });
      assert.equal(tree.props['aria-controls'], 'color-picker-panel'); assert.equal(tree.props['aria-expanded'], true);
      assert.equal(colorPicker.get().value, '#abcdef'); colorPicker.get().onPick('#abcdef'); assert.deepEqual(picked, ['new:#abcdef']);
      tree.props.onClick(); assert.equal(colorPicker.get(), null); assert.ok(changes >= 3); unsubscribe();
    } finally { d.dispose(); }
  });
});

test('panel runtime: labelled dialog/header, persisted tabs, native RGB inputs, skin-first presets, fixed body and no opacity/pipette', async () => {
  await fixture(h => {
    assert.equal(labeled(h.tree, 'Notch colour picker').props.role, 'dialog');
    assert.equal(labeled(h.tree, 'Notch colour picker').props['data-island-extension'], 'color-picker');
    assert.deepEqual(tabs(h.tree).map(n => n.props.children), ['Grid', 'Spectrum', 'Sliders']);
    assert.deepEqual(tabs(h.tree).filter(n => n.props['aria-selected']).map(n => n.props.children), ['Sliders']);
    assert.equal(tabs(h.tree).filter(n => n.props.tabIndex === 0).length, 1);
    for (const [label, value, gradient] of [['Red', 18, '#003456, #ff3456'], ['Green', 52, '#120056, #12ff56'], ['Blue', 86, '#123400, #1234ff']]) {
      const input = labeled(h.tree, label); assert.equal(input.type, 'input'); assert.equal(input.props.type, 'range');
      assert.equal(input.props.min, 0); assert.equal(input.props.max, 255); assert.equal(input.props.value, value); assert.ok(input.props.style.background.includes(gradient));
      assert.equal(labeled(h.tree, `${label} value`).props.type, 'number');
    }
    const presets = labeled(h.tree, 'Theme preset colours'); assert.deepEqual(nodes(presets).filter(n => n.type === 'button').slice(0, 5).map(n => n.props.style.background), ['#abcdef', '#eeeeee', '#111111', '#222222', '#777777']);
    assert.equal(labeled(h.tree, 'Pick colour from screen'), undefined); assert.equal(labeled(h.tree, 'Opacity'), undefined);
    assert.ok(labeled(h.tree, 'Hex colour')); assert.ok(labeled(h.tree, 'Sliders hex colour')); assert.ok(textButton(h.tree, 'Done'));
    tabs(h.tree)[0].props.onClick({ detail: 1 }); assert.equal(h.saved.get(tabKey), 'Grid'); assert.equal(labeled(h.render(), 'Notch colour picker').props.style.height, 420);
    const event = keyEvent('ArrowRight'); tabs(h.render())[0].props.onKeyDown(event); assert.equal(h.saved.get(tabKey), 'Spectrum'); assert.ok(event.prevented); assert.ok(h.focused.includes('color-tab-Spectrum'));
    assert.equal(labeled(h.render(), 'Colour controls').props['data-instant'], true);
  }, { tab: 'Sliders' });
});

test('recent runtime: automatic commit, explicit pin, reselect, invalid storage and storage failures', async () => {
  await fixture(h => {
    labeled(h.tree, 'Pin current colour to recent colours').props.onClick(); assert.deepEqual(JSON.parse(h.saved.get(recentKey)), ['#123456']); assert.deepEqual(h.picked, []);
    labeled(h.tree, 'Use #abcdef').props.onClick(); assert.deepEqual(h.picked, ['#abcdef']); assert.deepEqual(JSON.parse(h.saved.get(recentKey)), ['#abcdef', '#123456']);
    labeled(h.render(), 'Recent #123456').props.onClick(); assert.equal(h.picked.at(-1), '#123456'); assert.deepEqual(JSON.parse(h.saved.get(recentKey)), ['#123456', '#abcdef']);
  }, { recent: 'bad JSON', tab: 'unexpected' });
  await fixture(h => { assert.equal(tabs(h.tree)[0].props['aria-selected'], true); labeled(h.tree, 'Use #abcdef').props.onClick(); assert.deepEqual(h.picked, ['#abcdef']); }, { noStorage: true });
});

test('RGB runtime: pointer preview commits on release; keyboard changes and numeric fields commit clamped channels', async () => {
  await fixture(h => {
    const red = labeled(h.tree, 'Red'), target = pointerTarget();
    red.props.onPointerDown(down(target)); assert.equal(target.capture, 1);
    red.props.onChange({ target: { value: '200' } }); assert.deepEqual(h.picked, []);
    red.props.onPointerUp(); assert.equal(h.picked.at(-1), '#c83456');
    labeled(h.render(), 'Green').props.onChange({ target: { value: '100' } }); assert.equal(h.picked.at(-1), '#c86456');
    labeled(h.render(), 'Blue value').props.onChange({ target: { value: '300' } }); assert.equal(h.picked.at(-1), '#c864ff');
    labeled(h.render(), 'Red value').props.onChange({ target: { value: '-5' } }); assert.equal(h.picked.at(-1), '#0064ff');
  }, { tab: 'Sliders' });
});

test('Spectrum runtime: captured pointer drags clamp, ignore other pointers, reach black and commit only at release', async () => {
  await fixture(h => {
    const sv = labeled(h.tree, 'Saturation and brightness'), target = pointerTarget();
    assert.ok(sv.props.style.backgroundColor, 'base hue preserves the CSS black/white gradient layers'); assert.equal(sv.props.style.background, undefined);
    assert.equal(sv.props.role, 'slider'); assert.equal(sv.props.tabIndex, 0); assert.ok(sv.props['aria-valuetext'].includes('brightness'));
    sv.props.onPointerDown(down(target)); assert.equal(target.capture, 1); assert.equal(target.reads, 1); assert.deepEqual(h.picked, []);
    target.fire('pointermove', 80, 80, 2); assert.equal(labeled(h.render(), 'Hex colour').props.value, '#406080');
    target.fire('pointermove', 400, 500); target.fire('pointerup', 400, 500); assert.equal(h.picked.at(-1), '#000000');
    assert.equal(target.capture, null); assert.equal(target.listeners, 0); assert.equal(target.reads, 1, 'no layout reads on pointermove');
    const hue = labeled(h.render(), 'Hue'), hueTarget = pointerTarget(); hue.props.onPointerDown(down(hueTarget, 200, 0)); hueTarget.fire('pointerup', 200, 0);
    assert.equal(labeled(h.render(), 'Hue').props['aria-valuenow'], 360); assert.ok(h.picked.every(api.isHex));
    const event = keyEvent('ArrowUp'); labeled(h.render(), 'Saturation and brightness').props.onKeyDown(event); assert.ok(event.prevented); assert.notEqual(h.picked.at(-1), '#000000');
    hue.props.onPointerDown(down(hueTarget)); h.colorPicker.close(); h.render(); assert.equal(hueTarget.listeners, 0, 'closing during drag removes listeners');
  }, { tab: 'Spectrum' });
});

test('footer runtime: invalid hex stays uncommitted, Enter/blur/Done commit valid hex and reset preserves request callback', async () => {
  await fixture(h => {
    labeled(h.tree, 'Hex colour').props.onChange({ target: { value: '#12z456' } });
    let tree = h.render(), input = labeled(tree, 'Hex colour'); assert.equal(input.props['aria-invalid'], true); input.props.onBlur(); assert.deepEqual(h.picked, []);
    input.props.onChange({ target: { value: 'ABCDEF' } }); tree = h.render(); input = labeled(tree, 'Hex colour'); assert.equal(input.props['aria-invalid'], false);
    input.props.onKeyDown(keyEvent('Enter')); assert.equal(h.picked.at(-1), '#abcdef'); input.props.onBlur(); assert.equal(h.picked.at(-1), '#abcdef');
    textButton(h.render(), 'Done').props.onClick(); assert.equal(h.colorPicker.get(), null); assert.ok(h.focused.includes('test-swatch'));
  });
  await fixture(h => { let resets = 0; h.request.onReset = () => resets++; h.request.resetLabel = 'Use theme'; textButton(h.render(), 'Use theme').props.onClick(); assert.equal(resets, 1); assert.equal(h.colorPicker.get(), null); });
});

test('lifecycle runtime: Escape is consumed, page/collapse closes, exit and resize report surface changes', async () => {
  await fixture(h => {
    let reports = 0; h.win.addEventListener('island-surfaces-changed', () => reports++);
    const event = new Event('keydown', { cancelable: true }); Object.defineProperty(event, 'key', { value: 'Escape' }); h.win.dispatchEvent(event);
    assert.ok(event.defaultPrevented); assert.equal(h.colorPicker.get(), null);
    h.tree.props.onExitComplete(); assert.ok(reports >= 2);
    h.colorPicker.open(h.request); h.render(); h.render(); h.state.view = 'other'; h.render(); assert.equal(h.colorPicker.get(), null);
  });
  await fixture(h => { h.state.mode = 'still'; h.render(); assert.equal(h.colorPicker.get(), null); });
  await fixture(h => { globalThis.innerHeight = 300; h.win.dispatchEvent(new Event('resize')); const panel = labeled(h.render(), 'Notch colour picker'); assert.equal(panel.props.style.height, 284); assert.equal(panel.props.style.top, 8); });
});

test('motion runtime: reduced/keyboard panel and tab transitions are instant; exiting tab is inert; pointer crossfade <=180ms', async () => {
  for (const mode of [{ reduced: true }, { nav: 'key' }]) await fixture(h => {
    const panel = labeled(h.tree, 'Notch colour picker'); assert.equal(panel.props.transition.duration, 0);
    assert.equal(panel.props.variants.closed({ keyboard: mode.nav === 'key', reduced: !!mode.reduced }).transition.duration, 0);
    assert.equal(labeled(h.tree, 'Colour controls').props['data-instant'], true);
  }, mode);
  for (const instant of [true, false]) {
    const d = driver({ 'framer-motion': { ...motion, useIsPresent: () => false } });
    try {
      const tree = d.render(d.load(path).ColorTabPanel, { tab: 'Grid', instant, children: 'content' });
      assert.equal(tree.props.role, 'tabpanel'); assert.equal(tree.props.inert, true); assert.equal(tree.props['aria-hidden'], true);
      assert.ok(tree.props.transition.duration <= .18); assert.equal(tree.props.transition.duration === 0, instant);
      assert.equal(tree.props.initial.transform, instant ? 'translateX(0px)' : 'translateX(4px)');
      assert.equal(tree.props.variants.hidden({ instant: true }).transition.duration, 0);
    } finally { d.dispose(); }
  }
});

test('EyeDropper runtime: only supported windows show pipette; cancel or stale result does not change another request', async () => {
  await fixture(async h => {
    h.win.EyeDropper = class { async open() { return { sRGBHex: '#ABCDEF' }; } };
    await labeled(h.render(), 'Pick colour from screen').props.onClick(); assert.equal(h.picked.at(-1), '#abcdef');
    h.win.EyeDropper = class { async open() { throw Error('cancel'); } };
    await labeled(h.render(), 'Pick colour from screen').props.onClick(); assert.equal(h.picked.length, 1);
    const sample = deferred(); h.win.EyeDropper = class { open() { return sample.promise; } };
    const pending = labeled(h.render(), 'Pick colour from screen').props.onClick(); h.colorPicker.close(); sample.resolve({ sRGBHex: '#ff0000' }); await pending;
    assert.equal(h.picked.length, 1);
  });
});

test('docking clamps right/left/inside/missing-island and short/narrow viewports, accounting for side controls', () => {
  let rect = { left: 100, right: 700, top: 20, bottom: 500 }, buttons = [];
  globals({ innerWidth: 1200, innerHeight: 800, document: { querySelector: () => rect && ({ getBoundingClientRect: () => rect }), querySelectorAll: () => buttons }, getComputedStyle: b => ({ visibility: b.visibility ?? 'visible' }) }, () => {
    assert.equal(api.dock(80).side, 'right'); assert.equal(api.dock(80).x, 714);
    buttons = [{ getBoundingClientRect: () => ({ left: 700, right: 740, top: 50 }) }]; assert.equal(api.dock(80).x, 754);
    buttons[0].visibility = 'hidden'; assert.equal(api.dock(80).x, 714); buttons = [];
    rect = { left: 500, right: 1100, top: 700, bottom: 850 }; let pos = api.dock(800); assert.equal(pos.side, 'left'); assert.equal(pos.y, 372);
    globalThis.innerWidth = 800; rect = { left: 100, right: 700, top: -20, bottom: 250 }; pos = api.dock(-500); assert.equal(pos.side, 'inside'); assert.equal(pos.y, 8);
    globalThis.innerWidth = 240; globalThis.innerHeight = 300; pos = api.dock(500); assert.equal(pos.width, 224); assert.equal(pos.height, 284); assert.equal(pos.x, 8); assert.equal(pos.y, 8);
    rect = null; assert.equal(api.dock(0).x, 8);
  });
});

test('new CSS permits literal colours only in SV/hue gradients and the white-ringed movable colour swatches', () => {
  const clean = css.replace(/\/\*[\s\S]*?\*\//g, '');
  const literals = /#[0-9a-f]{3,8}\b|\brgba?\(\s*\d|\bhsla?\(\s*\d|\b(?:white|black|red|blue|gray|grey)\b/i;
  for (const match of clean.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selector = match[1].trim();
    for (const declaration of match[2].split(';')) if (literals.test(declaration.slice(declaration.indexOf(':') + 1))) {
      const gradient = ['.color-pop-sv', '.color-pop-hue'].includes(selector) && /^\s*background(?:-image)?:.*linear-gradient/.test(declaration);
      const swatchRing = selector === '.color-pop-sv i, .color-pop-hue i' && /^\s*border:\s*2px solid #ffffff\s*$/.test(declaration);
      assert.ok(gradient || swatchRing, `${selector}: ${declaration}`);
    }
  }
  for (const token of ['--ui-raised', '--ui-surface', '--ui-text', '--ui-text-secondary', '--ui-border', '--ui-focus', '--ui-danger', '--v-fill', '--v-stroke', '--ui-shadow']) assert.ok(css.includes(`var(${token})`), token);
  assert.match(css, /\.color-pop-body \{ height: 200px; flex: none;/); assert.match(css, /inset 0 0 0 2px var\(--ui-text\), inset 0 0 0 3px var\(--ui-background\)/);
  assert.match(css, /html\[data-nav-via='key'\]/); assert.match(css, /html\[data-reduce-motion='true'\]/); assert.match(css, /prefers-reduced-motion: reduce/);
  assert.match(css, /@media \(prefers-reduced-motion: no-preference\) \{\s*html:not\(\[data-nav-via='key'\]\):not\(\[data-reduce-motion='true'\]\).*:active/);
  assert.doesNotMatch(css, /(?:backdrop-filter|filter:|transition: all)/);
  for (const old of ['ui/island/polish.css', 'ui/island/tokens/light-tone.css', 'ui/island/components/controls.css']) assert.doesNotMatch(src(old), /\.color-(?:pop|swatch)/, `${old} no longer owns picker rules`);
  assert.match(src('ui/island/Island.tsx'), /import '\.\/components\/color-picker.css';/, 'CSS loads outside the component test loader');
});
