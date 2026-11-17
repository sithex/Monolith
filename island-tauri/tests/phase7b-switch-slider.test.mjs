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
