import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import ts from 'typescript';
import { driver, globals, labeled, nodes } from './phase7b-runtime-harness.mjs';
const src = p => readFileSync(new URL(`../${p}`, import.meta.url), 'utf8');
const css = () => src('ui/island/components/controls.css');
const key = (input, name) => { let prevented = false, stopped = false; input.props.onKeyDown({ key: name, preventDefault() { prevented = true; }, stopPropagation() { stopped = true; } }); assert.ok(prevented && stopped, name); };

test('stepper: keyboard and buttons commit bounded whole counts without island shortcuts', () => {
  const d = driver(), { NumberStepper } = d.load('ui/island/components/NumberStepper.tsx'), commits = [];
  const props = { 'aria-label': 'Shelf size', value: 12, min: 1, max: 50, onCommit: (...args) => commits.push(args) };
  const render = () => d.render(NumberStepper, props);
  let tree = render();
  assert.equal(labeled(tree, 'Shelf size').props.type, 'number');
  key(labeled(tree, 'Shelf size'), 'End');
  tree = render(); assert.equal(labeled(tree, 'Increase shelf size').props.disabled, true);
  key(labeled(tree, 'Shelf size'), 'ArrowUp');
  key(labeled(tree, 'Shelf size'), 'Home');
  tree = render(); assert.equal(labeled(tree, 'Decrease shelf size').props.disabled, true);
  key(labeled(tree, 'Shelf size'), 'ArrowDown');
  key(labeled(tree, 'Shelf size'), 'PageUp');
  key(labeled(tree, 'Shelf size'), 'ArrowDown');
  tree = render(); labeled(tree, 'Increase shelf size').props.onClick();
  assert.deepEqual(commits, [[50,true], [50,true], [1,true], [1,true], [11,true], [10,true], [11,true]]);
  assert.equal(labeled(render(), 'Shelf size').props.value, '11'); d.dispose();
});

test('stepper: draft typing waits, clamps on blur/Enter, Escape cancels, scrollback uses 500-line steps', () => {
  const d = driver(), { NumberStepper } = d.load('ui/island/components/NumberStepper.tsx'), commits = [];
  const props = { 'aria-label': 'Terminal scrollback', value: 5000, min: 500, max: 100000, step: 500, unit: ' lines', onCommit: (...a) => commits.push(a) };
  const input = () => labeled(d.render(NumberStepper, props), props['aria-label']);
  let field = input(); field.props.onFocus(); field.props.onChange({ target: { value: '100001' } });
  assert.deepEqual(commits, []); field.props.onBlur(); assert.deepEqual(commits.pop(), [100000, true]);
  field = input(); field.props.onChange({ target: { value: '-100' } }); key(field, 'Enter'); assert.deepEqual(commits.pop(), [500, true]);
  field = input(); field.props.onChange({ target: { value: '1700' } }); key(field, 'Enter'); assert.deepEqual(commits.pop(), [1500, true]);
  field = input(); key(field, 'ArrowUp'); assert.deepEqual(commits.pop(), [2000, true]);
  field = input(); field.props.onChange({ target: { value: '' } }); key(field, 'Escape'); field.props.onBlur();
  assert.equal(input().props.value, '5000'); assert.deepEqual(commits, []);
  field.props.onChange({ target: { value: '' } }); field.props.onBlur(); assert.deepEqual(commits, []);
  assert.equal(labeled(d.render(NumberStepper, { ...props, disabled: true }), 'Increase terminal scrollback').props.disabled, true); d.dispose();
});

test('count settings use the shared stepper, retaining schema ranges and commit-only saves', () => {
  const settings = src('ui/views/SettingsView.tsx'), notch = src('ui/island/NotchControls.tsx');
  assert.match(settings, /\['terminalScrollback', 'clipboardHistorySize', 'equalizerBands'\]\.includes\(key\)/);
  assert.match(settings, /<NumberStepper[^>]*min=\{min\}[^>]*max=\{max\}[^>]*step=\{step\}/);
  assert.match(notch, /<NumberStepper aria-label="Shelf size" min=\{1\} max=\{50\}/);
  assert.doesNotMatch(notch, /<DraftRange aria-label="Shelf size"/);
});

test('sliders: one token skin, full-thickness pill fill, slim grips and nonwrapping fixed values', () => {
  const shared = src('ui/island/components/range.css');
  assert.match(shared, /\.pill-range-track\s*\{[^}]*inset: 3px 0[^}]*background: var\(--v-track\)/);
  assert.match(shared, /\.pill-range-fill\s*\{[^}]*inset: 0; background: var\(--ui-accent\)[^}]*transform: scaleX\(var\(--fill-ratio\)\)/);
  assert.match(shared, /::-webkit-slider-thumb\s*\{[^}]*width: 0; height: 20px/);
  assert.match(shared, /\.fader-track\s*\{[^}]*width: 36px/);
  assert.match(shared, /\.fader-thumb\s*\{[^}]*width: 20px; height: 3px/);
  assert.match(shared, /\.settings-row output[^}]*flex: 0 0 9ch; width: 9ch; white-space: nowrap/);
  assert.match(src('ui/island/components/AppPrimitives.tsx'), /className="fader-thumb"/);
  for (const path of ['surface.css', 'tokens/light-tone.css', 'media.css']) assert.doesNotMatch(src(`ui/island/${path}`), /::-webkit-slider-|\.fader-fill\s*\{/);
});

test('shared fader: bounds, keyboard values, labels and shortcut isolation survive the move', () => {
  let now = 1000;
  globals({ performance: { now: () => now } }, () => {
    const d = driver(), { Fader } = d.load('ui/island/components/AppPrimitives.tsx'), commits = [];
    const render = () => d.render(Fader, { value: 50, label: 'Output', muted: false, onCommit: n => commits.push(n) });
    for (const [name, expected] of [['End',100], ['ArrowUp',100], ['Home',0], ['ArrowDown',0], ['PageUp',10], ['ArrowRight',12], ['PageDown',2]]) {
      now += 70; key(render(), name);
      const slider = render();
      assert.equal(slider.props.role, 'slider'); assert.equal(slider.props['aria-label'], 'Output volume');
      assert.equal(slider.props['aria-valuenow'], expected); assert.equal(slider.props['aria-valuetext'], `${expected}%`);
      assert.equal(slider.props['aria-orientation'], 'vertical');
      assert.equal(nodes(slider).find(n => n.props.className === 'fader-thumb').props.style.bottom, `${expected}%`);
    }
    assert.deepEqual(commits, [100,100,0,0,10,12,2]); d.dispose();
  });
});

function emitter(extra = {}) {
  const listeners = new Map();
  return { ...extra, listeners, addEventListener(type, f) { listeners.set(type, f); }, removeEventListener(type) { listeners.delete(type); }, fire(type, event = {}) { listeners.get(type)?.({ type, ...event }); } };
}
function fixture() {
  const island = { getBoundingClientRect: () => ({ left: 20, right: 930, top: 8, bottom: 708, width: 910, height: 700 }) };
  const attrs = new Map([['aria-label', 'Next track'], ['aria-describedby', 'existing-help']]);
  const button = { dataset: {}, disabled: false, textContent: '', isConnected: true,
    closest(selector) { return selector === 'button' ? this : selector === '.island, [data-island-extension]' ? island : null; }, contains(node) { return node === this; },
    getAttribute(name) { return attrs.get(name) ?? null; }, setAttribute(name, value) { attrs.set(name, value); }, removeAttribute(name) { attrs.delete(name); },
    getBoundingClientRect: () => ({ left: 890, right: 920, top: 675, bottom: 700, width: 30, height: 25 }),
  };
  const doc = emitter({ body: {}, activeElement: null, querySelector: () => ({ getBoundingClientRect: () => ({ left: 20, right: 930, top: 8, bottom: 708, width: 910 }) }) });
  const win = emitter(); let observer;
  class MutationObserver { constructor(f) { observer = f; } observe() {} disconnect() {} }
  return { doc, win, button, attrs, MutationObserver, mutate: () => observer() };
}

test('tooltip: immediate hover/focus, no focus stealing; out/blur/Escape/scroll/removed target hide and restore ARIA', () => {
  const f = fixture();
  globals({ MutationObserver: f.MutationObserver }, () => {
    const d = driver(), { bindInstantTooltips } = d.load('ui/island/components/InstantTooltip.tsx'), shown = [];
    const dispose = bindInstantTooltips(f.doc, f.win, b => shown.push(b));
    f.doc.fire('pointerover', { target: f.button }); assert.equal(shown.at(-1), f.button);
    assert.equal(f.attrs.get('aria-describedby'), 'existing-help island-control-tooltip');
    f.doc.fire('pointerout', { relatedTarget: null }); assert.equal(shown.at(-1), null); assert.equal(f.attrs.get('aria-describedby'), 'existing-help');
    f.doc.activeElement = f.button; f.doc.fire('focusin', { target: f.button }); assert.equal(shown.at(-1), f.button);
    f.doc.fire('pointerout'); assert.equal(shown.at(-1), f.button, 'hover leave retains keyboard tooltip');
    let stopped = false; f.doc.fire('keydown', { key: 'Escape', stopPropagation() { stopped = true; } }); assert.equal(stopped, false, 'Escape still reaches the island close handler'); assert.equal(shown.at(-1), null);
    for (const type of ['focusout', 'scroll', 'pointerdown', 'visibilitychange']) { f.doc.fire('focusin', { target: f.button }); f.doc.fire(type); assert.equal(shown.at(-1), null, type); }
    f.doc.fire('focusin', { target: f.button }); f.button.isConnected = false; f.mutate(); assert.equal(shown.at(-1), null);
    f.button.disabled = true; f.doc.fire('pointerover', { target: f.button }); assert.equal(shown.at(-1), null);
    f.button.disabled = false; f.button.isConnected = true; f.button.textContent = '−';
    f.doc.fire('focusin', { target: f.button }); assert.equal(shown.at(-1), f.button, 'symbol-only buttons have keyboard hints');
    f.win.fire('blur'); assert.equal(shown.at(-1), null);
    dispose(); assert.equal(f.doc.listeners.size, 0); assert.equal(f.win.listeners.size, 0); d.dispose();
  });
});

test('tooltip: rendering stays inside island bounds and cannot expand/intercept the native input region', () => {
  const f = fixture();
  globals({ document: f.doc, window: f.win, MutationObserver: f.MutationObserver }, () => {
    const d = driver(), { InstantTooltip } = d.load('ui/island/components/InstantTooltip.tsx');
    d.render(InstantTooltip, {}); f.doc.fire('focusin', { target: f.button });
    let tree = d.renderOnly(InstantTooltip, {});
    tree.props.ref.current = { style: {}, getBoundingClientRect: () => ({ width: 100, height: 30 }) }; d.flushEffects();
    tree = d.renderOnly(InstantTooltip, {});
    assert.equal(tree.props.role, 'tooltip'); assert.equal(tree.props.tabIndex, undefined);
    assert.equal(tree.props.style.left, 822); assert.equal(tree.props.style.top, 639, 'flips above when there is no room below');
    assert.equal(tree.props['data-island-extension'], undefined);
    assert.match(css(), /\.instant-tooltip\s*\{[^}]*position: fixed;[^}]*pointer-events: none/);
    assert.doesNotMatch(src('ui/island/components/InstantTooltip.tsx'), /\.focus\(|setTimeout\(/); d.dispose();
  });
});

test('button family retains four roles; icon title replacements preserve accessible names and selectors', () => {
  const shared = css();
  for (const role of ['primary', 'secondary', 'quiet', 'danger']) assert.ok(shared.includes(`button[data-variant='${role}']`), role);
  for (const [file, label] of [['PreviewNotice.tsx','Dismiss preview'], ['ApprovalNotice.tsx','Dismiss'], ['components/UpdatesApp.tsx','Check updates']]) {
    const source = src(`ui/island/${file}`); assert.ok(source.includes(`aria-label="${label}"`)); assert.doesNotMatch(source, new RegExp(`title="${label}`));
  }
  assert.match(src('ui/island/Island.tsx'), /<InstantTooltip\s*\/>/);
  assert.match(src('ui/island/components/ShelfApp.tsx'), /data-variant="danger" aria-label=\{`Remove/);
});

test('icon action hints: native titles never return on SVG/icon-only buttons', () => {
  for (const file of readdirSync(new URL('../ui/', import.meta.url), { recursive: true }).filter(p => /^(island|views)\//.test(p) && p.endsWith('.tsx'))) {
    const source = ts.createSourceFile(file, src(`ui/${file}`), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
    function visit(node) {
      if (ts.isJsxElement(node) && /(?:^|\.)button$/.test(node.openingElement.tagName.getText(source))) {
        const title = node.openingElement.attributes.properties.some(p => ts.isJsxAttribute(p) && p.name.getText(source) === 'title');
        const isText = c => ts.isJsxText(c) ? !!c.text.trim() : ts.isJsxExpression(c) ? !!c.expression && !ts.isJsxElement(c.expression) && !ts.isJsxSelfClosingElement(c.expression) : ts.isJsxElement(c) && c.openingElement.tagName.getText(source) !== 'svg' && c.children.some(isText);
        const text = node.children.some(isText);
        const icon = node.children.some(c => (ts.isJsxElement(c) || ts.isJsxSelfClosingElement(c)) && /^(Icon|svg|TransportGlyph)$/.test((ts.isJsxElement(c) ? c.openingElement.tagName : c.tagName).getText(source)));
        assert.ok(!(title && icon && !text), `${file}: icon button has a native title`);
      }
      ts.forEachChild(node, visit);
    }
    visit(source);
  }
});

test('tooltip placement: hugs its button, side buttons point outward, picker swatches stay on the picker', () => {
  const d = driver(), { placeTooltip } = d.load('ui/island/components/InstantTooltip.tsx');
  const box = (left, top, width, height) => ({ left, top, width, height, right: left + width, bottom: top + height });
  const win = box(0, 0, 1400, 900), tip = { width: 80, height: 24 };
  // Left side button at x 300..336: tip sits just left of it, vertically centred.
  assert.deepEqual(placeTooltip(box(300, 100, 36, 36), tip, win, 'left'), { left: 214, top: 106, maxWidth: 1384 });
  assert.deepEqual(placeTooltip(box(1000, 100, 36, 36), tip, win, 'right'), { left: 1042, top: 106, maxWidth: 1384 });
  // No room outward: falls back to below the button, not into some far surface.
  assert.equal(placeTooltip(box(10, 100, 36, 36), tip, win, 'left').top, 142);
  // Swatch inside a docked picker: bounded by the picker, directly under the swatch.
  const picker = box(940, 10, 300, 400), swatch = box(1000, 200, 20, 20);
  assert.deepEqual(placeTooltip(swatch, tip, picker, 'below'), { left: 970, top: 226, maxWidth: 284 });
  d.dispose();
});
