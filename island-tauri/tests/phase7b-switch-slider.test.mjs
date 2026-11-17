import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
import { driver, globals, labeled, nodes } from './phase7b-runtime-harness.mjs';
const src = p => readFileSync(new URL(`../${p}`, import.meta.url), 'utf8');
const knob = tree => nodes(tree).find(n => n.props?.className === 'switch-knob');

test('Switch renders a native labeled checkbox with switch role, disabled state and no mount animation', () => {
  const { Switch } = load('ui/island/components/Switch.tsx');
  const html = renderToStaticMarkup(createElement(Switch, { 'aria-label': 'Enable feature', checked: true, disabled: true }));
  assert.match(html, /<label class="switch-track"/);
  assert.match(html, /<input[^>]*aria-label="Enable feature"[^>]*class="switch"[^>]*type="checkbox"[^>]*role="switch"[^>]*disabled=""[^>]*checked=""/);
  for (const checked of [false, true]) {
    const d = driver(), C = d.load('ui/island/components/Switch.tsx').Switch;
    const tree = d.render(C, { checked });
    assert.equal(knob(tree).props.initial, false);
    assert.equal(knob(tree).props.animate.transform, `translateX(${checked ? 20 : 0}px) scale(1, 1)`);
    d.dispose();
  }
});

test('Switch preserves native change, focus and Space handlers and has a measured small variant', () => {
  const d = driver(), { Switch } = d.load('ui/island/components/Switch.tsx'), changes = [], keys = [];
  let focused = false;
  const tree = d.render(Switch, { size: 'sm', 'aria-label': 'Monospace', checked: false,
    onChange: e => changes.push(e.target.checked), onKeyDown: e => keys.push(e.key), onFocus: () => { focused = true; } });
  const input = labeled(tree, 'Monospace');
  assert.equal(input.type, 'input'); assert.equal(input.props.role, 'switch'); assert.equal(tree.props['data-size'], 'sm');
  input.props.onChange({ target: { checked: true } }); input.props.onChange({ target: { checked: false } });
  input.props.onKeyDown({ key: ' ' }); input.props.onFocus();
  assert.deepEqual(changes, [true, false]); assert.deepEqual(keys, [' ']); assert.ok(focused);
  const css = src('ui/island/components/switch.css');
  assert.match(css, /data-size='sm'\] \{ width: 34px; height: 20px/);
  assert.match(css, /data-size='sm'\] \.switch-knob \{ width: 16px; height: 16px/);
  d.dispose();
});

test('Switch squishes on changes without delay and settles instantly for keyboard and either reduced-motion source', () => {
  for (const [dataset, os, instant] of [[{}, false, false], [{ navVia: 'key' }, false, true], [{ reduceMotion: 'true' }, false, true], [{}, true, true]]) {
    globals({ document: { documentElement: { dataset } }, matchMedia: () => ({ matches: os }) }, () => {
      const d = driver(), { Switch } = d.load('ui/island/components/Switch.tsx');
      d.render(Switch, { checked: false });
      const k = knob(d.render(Switch, { checked: true }));
      if (instant) { assert.equal(k.props.transition.duration, 0); assert.equal(typeof k.props.animate.transform, 'string'); }
      else {
        assert.deepEqual(k.props.animate.transform, ['translateX(0px) scale(1, 1)', 'translateX(10px) scale(1.4, .7)', 'translateX(20px) scale(1, 1)']);
        assert.equal(k.props.transition.duration, .26); assert.equal(k.props.transition.delay, undefined);
        assert.deepEqual(k.props.transition.ease, [.23, 1, .32, 1]);
      }
      assert.equal(k.props.initial, false); d.dispose();
    });
  }
});

test('every toggle checkbox is centralized in Switch, and the old switch pseudo-element and scale hack are gone', () => {
  const root = new URL('../ui/', import.meta.url).pathname;
  const files = dir => readdirSync(dir).flatMap(name => {
    const path = join(dir, name); return name === 'legacy' ? [] : statSync(path).isDirectory() ? files(path) : path.endsWith('.tsx') ? [path] : [];
  });
  const checkboxFiles = files(root).filter(p => /type="checkbox"/.test(readFileSync(p, 'utf8')));
  assert.deepEqual(checkboxFiles.map(p => p.slice(root.length)), ['island/components/Switch.tsx']);
  assert.doesNotMatch(src('ui/island/surface.css'), /input\.switch/);
  assert.doesNotMatch(src('ui/views/settings/settings-controls.css'), /input\.switch/);
  const { Switch } = load('ui/island/components/Switch.tsx');
  const { NotchControls } = load('ui/island/NotchControls.tsx');
  const tree = NotchControls({ settings: {}, background: '', accent: '', outputs: ['DP-1'], patch() {} });
  assert.equal(labeled(tree, 'Notch companion').type, Switch);
  assert.equal(labeled(tree, 'Notch on DP-1').type, Switch);
  assert.equal(labeled(tree, 'Notch on DP-1').props.size, 'sm');
});

test('Switch paint is token-derived in both tones, with native focus, disabled and instant-motion CSS', () => {
  const css = src('ui/island/components/switch.css');
  assert.doesNotMatch(css, /#[\da-f]{3,8}\b|\b(?:rgb|rgba|hsl|hsla)\(|\b(?:white|black)\s*[;}]/i);
  assert.match(css, /--switch-knob: oklch\(from var\(--ui-accent\) 1 0 0\)/);
  for (const token of ['--ui-accent', '--v-track', '--v-stroke-hi', '--ui-focus']) assert.ok(css.includes(`var(${token})`));
  assert.match(css, /box-shadow: inset 0 0 12px var\(--switch-shade\)/);
  assert.match(css, /:has\(\.switch:focus-visible\)/);
  assert.match(css, /:has\(\.switch:disabled\).*opacity: \.4/);
  assert.match(css, /prefers-reduced-motion: reduce/); assert.match(css, /data-nav-via='key'/);
  assert.match(css, /transition: opacity var\(--dur-base\) var\(--ease-out\)/);
  assert.match(src('ui/island/tokens/light-tone.css'), /--v-track: color-mix\(in srgb, var\(--ui-text\) 16%, transparent\)/);
});

function clock(t) {
  let now = 1000;
  t.mock.method(performance, 'now', () => now);
  t.mock.timers.enable({ apis: ['setTimeout'] });
  return ms => { now += ms; t.mock.timers.tick(ms); };
}

test('DraftRange keeps the native input, exposes bounded fill ratios and retains bounds, labels and disabled props', () => {
  for (const [value, expected] of [[0, 0], [110, 0], [215, .5], [320, 1], [400, 1]]) {
    const d = driver(), { DraftRange } = d.load('ui/island/components/AppPrimitives.tsx');
    const tree = d.render(DraftRange, { 'aria-label': 'Notch width', min: 110, max: 320, step: 2, value, disabled: true, onCommit() {} });
    const input = labeled(tree, 'Notch width');
    assert.equal(tree.props.className, 'pill-range'); assert.equal(tree.props.style['--fill-ratio'], expected);
    assert.equal(input.type, 'input'); assert.equal(input.props.type, 'range'); assert.equal(input.props.disabled, true);
    assert.deepEqual([input.props.min, input.props.max, input.props.step, input.props.value], [110, 320, 2, value]);
    assert.equal(nodes(tree).find(n => n.props?.className === 'pill-range-track').props['aria-hidden'], 'true');
    d.dispose();
  }
});

test('DraftRange throttles draft commits, resists stale Rust values mid-drag and always flushes the final value', t => {
  const advance = clock(t), d = driver(), { DraftRange } = d.load('ui/island/components/AppPrimitives.tsx');
  const commits = [], drafts = [], props = { value: 20, min: 0, max: 100, 'aria-label': 'Volume', onCommit: (...a) => commits.push(a), onDraft: n => drafts.push(n) };
  const render = () => d.render(DraftRange, props), input = () => labeled(render(), 'Volume');
  try {
    input().props.onPointerDown({}); input().props.onChange({ target: { value: '40' } });
    assert.deepEqual(commits, [[40, false]]); assert.equal(render().props['data-dragging'], true);
    props.value = 5; render(); assert.equal(input().props.value, 40, 'stale Rust echo cannot replace the pointer draft');
    advance(10); input().props.onChange({ target: { value: '60' } });
    assert.equal(render().props.style['--fill-ratio'], .6);
    advance(79); assert.equal(commits.length, 1);
    advance(1); assert.deepEqual(commits, [[40, false], [60, false]]);
    input().props.onPointerUp({});
    assert.deepEqual(commits, [[40, false], [60, false], [60, true]], 'already-sent value is still flushed as final');
    assert.deepEqual(drafts, [40, 60, null]); assert.equal(render().props['data-dragging'], false);
    props.value = 75; render(); assert.equal(input().props.value, 75); assert.equal(render().props.style['--fill-ratio'], .75);
  } finally { d.dispose(); }
});

test('DraftRange commit-only mode follows drafts immediately and flushes release, cancellation, blur and keyboard exactly once', t => {
  clock(t);
  for (const ending of ['onPointerUp', 'onPointerCancel', 'onBlur', 'onKeyUp']) {
    const d = driver(), { DraftRange } = d.load('ui/island/components/AppPrimitives.tsx'), commits = [], forwarded = [];
    const props = { value: 10, live: false, 'aria-label': 'Size', className: 'ac-range', style: { width: 160 }, onCommit: (...a) => commits.push(a), [ending]: () => forwarded.push(ending) };
    const render = () => d.render(DraftRange, props), input = () => labeled(render(), 'Size');
    try {
      input().props[ending === 'onKeyUp' ? 'onKeyDown' : 'onPointerDown']({ key: 'ArrowUp' });
      input().props.onChange({ target: { value: '42' } });
      const tree = render(); assert.equal(tree.props.style['--fill-ratio'], .42); assert.equal(tree.props.style.width, 160);
      assert.equal(tree.props.className, 'pill-range ac-range'); assert.deepEqual(commits, []);
      input().props[ending]({}); assert.deepEqual(commits, [[42, true]]); assert.deepEqual(forwarded, [ending]);
      input().props.onBlur({}); assert.deepEqual(commits, [[42, true]], 'blur after release does not duplicate the save');
    } finally { d.dispose(); }
  }
});

test('DraftRange pending timers flush on release and are removed when unmounted', t => {
  const advance = clock(t), d = driver(), { DraftRange } = d.load('ui/island/components/AppPrimitives.tsx'), commits = [];
  const props = { value: 0, 'aria-label': 'Volume', onCommit: (...a) => commits.push(a) };
  const input = () => labeled(d.render(DraftRange, props), 'Volume');
  input().props.onPointerDown({}); input().props.onChange({ target: { value: '10' } });
  advance(1); input().props.onChange({ target: { value: '20' } }); input().props.onPointerUp({});
  advance(100); assert.deepEqual(commits, [[10, false], [20, true]], 'no stale nonfinal save after release');
  input().props.onPointerDown({}); input().props.onChange({ target: { value: '30' } });
  advance(1); input().props.onChange({ target: { value: '40' } });
  const count = commits.length; d.dispose(); advance(100); assert.equal(commits.length, count);
});

test('Fader retains wheel/keyboard math, bounds and aria, with relative pointer dragging, tap-to-set and final flushing', t => {
  const advance = clock(t), d = driver(), { Fader } = d.load('ui/island/components/AppPrimitives.tsx'), commits = [];
  const props = { value: 50, label: 'Output', muted: true, onCommit: v => commits.push(v) };
  const render = () => d.render(Fader, props);
  try {
    let tree = render();
    assert.equal(tree.props.role, 'slider'); assert.equal(tree.props.tabIndex, 0); assert.equal(tree.props['aria-label'], 'Output volume');
    assert.equal(tree.props['aria-orientation'], 'vertical'); assert.equal(tree.props['data-muted'], true);
    assert.deepEqual([tree.props['aria-valuemin'], tree.props['aria-valuemax']], [0, 100]);
    const check = value => {
      const tree = render(), all = nodes(tree);
      assert.equal(tree.props['aria-valuenow'], value); assert.equal(tree.props['aria-valuetext'], `${value}%`);
      assert.equal(all.find(n => n.props?.className === 'fader-fill').props.style.transform, `scaleY(${value / 100})`);
    };
    let stopped = 0;
    tree.props.onWheel({ deltaY: 1, deltaX: 0, stopPropagation() { stopped++; } }); check(48);
    advance(60); render().props.onWheel({ deltaY: 0, deltaX: 1, stopPropagation() { stopped++; } }); check(50);
    advance(60); render().props.onWheel({ deltaY: -1, deltaX: 0, stopPropagation() { stopped++; } }); check(52);
    assert.equal(stopped, 3);
    for (const [key, value] of [['ArrowUp', 54], ['ArrowLeft', 52], ['PageUp', 62], ['PageDown', 52], ['Home', 0], ['ArrowDown', 0], ['End', 100], ['ArrowRight', 100]]) {
      advance(60); let prevented = false, stopped = false;
      render().props.onKeyDown({ key, preventDefault() { prevented = true; }, stopPropagation() { stopped = true; } });
      assert.ok(prevented && stopped); check(value);
    }
    tree = render(); nodes(tree).find(n => n.props?.className === 'fader-track').props.ref.current = { getBoundingClientRect: () => ({ bottom: 200, height: 200 }) };
    let captured;
    tree.props.onPointerDown({ button: 0, pointerId: 7, clientY: 50, currentTarget: { setPointerCapture: n => { captured = n; } } });
    check(100); assert.equal(captured, 7); assert.equal(render().props['data-dragging'], true, 'grabbing does not jump');
    tree = render(); tree.props.onPointerMove({ clientY: 51 }); check(100);
    tree = render(); tree.props.onPointerMove({ clientY: 130 }); check(60);
    tree = render(); tree.props.onPointerMove({ clientY: 10 }); check(100);
    assert.notEqual(nodes(render()).find(n => n.props?.className === 'fader-track').props['data-stretch'], 'none', 'overshoot rubber-bands');
    tree = render(); tree.props.onPointerMove({ clientY: 130 }); check(60);
    assert.equal(nodes(render()).find(n => n.props?.className === 'fader-fill').props['data-direct'], true);
    render().props.onPointerUp(); assert.equal(commits.at(-1), 60);
    advance(100); assert.equal(commits.at(-1), 60); assert.equal(render().props['data-dragging'], false);
    assert.equal(nodes(render()).find(n => n.props?.className === 'fader-track').props['data-stretch'], 'none');
    check(60); // held: the stale backend value (50) does not snap it back while the write settles
    tree = render(); tree.props.onPointerDown({ button: 0, pointerId: 8, clientY: 150, currentTarget: { setPointerCapture() {} } });
    render().props.onPointerUp({ clientY: 150 }); check(25); assert.equal(commits.at(-1), 25, 'a tap sets the tapped level');
  } finally { d.dispose(); }
});

test('range paint uses only tokens; fill and interaction motion use transforms, respecting direct input, keyboard and reduced motion', () => {
  const css = src('ui/island/components/range.css');
  assert.doesNotMatch(css, /#[\da-f]{3,8}\b|\b(?:rgb|rgba|hsl|hsla)\(|\b(?:white|black)\s*[;}]/i);
  assert.match(css, /transform: scaleX\(var\(--fill-ratio\)\); transform-origin: left/);
  assert.match(css, /transform-origin: bottom; transition: transform var\(--dur-base\) var\(--ease-out\)/);
  assert.match(css, /data-dragging='true'\] :is\(\.pill-range-fill, \.pill-range-grip\) \{ transition: none/);
  assert.match(css, /\.fader-fill\[data-direct='true'\] \{ transition: none/);
  assert.match(css, /@media \(hover: hover\) and \(pointer: fine\)/);
  assert.match(css, /scaleY\(1\.428571\)/); assert.match(css, /height: 20px/);
  assert.match(css, /\.fader\[data-muted='true'\] \.fader-fill \{ background: var\(--ui-text-hint\)/);
  assert.match(css, /:has\(> input:focus-visible\).*var\(--ui-focus\)/);
  assert.match(css, /:has\(> input:disabled\) \{ opacity: \.4/);
  assert.match(css, /data-nav-via='key'/); assert.match(css, /data-reduce-motion='true'/); assert.match(css, /prefers-reduced-motion: reduce/);
  assert.doesNotMatch(css, /transition:\s*(?:all|width|height|bottom)/);
  assert.doesNotMatch(src('ui/island/components/controls.css'), /slider-thumb|\.fader-track\s*\{/);
});
