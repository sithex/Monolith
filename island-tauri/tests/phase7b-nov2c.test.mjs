// Nov 2 (third batch): in-island colour picker (Wayland stacks the GTK chooser behind the
// island), click-sound context suspend/close (ghost PipeWire stream), launcher lock rule.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
const { hexToHsv, hsvToHex, isHex } = load('ui/island/components/ColorField.tsx');
const { createClickSound, SUSPEND_AFTER_MS, CLOSE_AFTER_MS } = load('ui/island/click-sound.ts');
const { geometryReport } = load('ui/island/geometry.ts');
const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

test('colour conversions round-trip every channel extreme and mid value', () => {
  for (const hex of ['#000000', '#ffffff', '#ff0000', '#00ff00', '#0000ff', '#80ff36', '#123456', '#fedcba', '#7f7f7f'])
    assert.equal(hsvToHex(hexToHsv(hex)), hex, hex);
  assert.ok(isHex('#a1B2c3') && !isHex('a1b2c3') && !isHex('#abc') && !isHex('#ggg000'));
});

test('no settings page uses the native colour input any more', () => {
  for (const file of ['ui/island/NotchControls.tsx', 'ui/island/SchemePicker.tsx', 'ui/views/SettingsView.tsx'])
    assert.doesNotMatch(src(file), /type=["']color["']/, file);
  assert.match(src('ui/island/Island.tsx'), /<ColorPickerPanel \/>/, 'the docked panel is mounted once');
});

test('docked island extensions join the input/blur region only while large', () => {
  const panel = { dataset: { islandExtension: 'color-picker' }, offsetLeft: 900, offsetTop: 60, offsetWidth: 248, offsetHeight: 312, getClientRects: () => [{}] };
  const outer = { ownerDocument: { querySelectorAll: sel => sel === '[data-island-extension]' ? [panel] : [] }, getBoundingClientRect: () => ({ x: 100, y: 11, width: 600, height: 500 }) };
  globalThis.getComputedStyle = e => ({ visibility: 'visible', borderRadius: e === panel ? '20px' : '28px' });
  assert.deepEqual(geometryReport(outer, { rev: 1, mode: 'large' }, true, 1).rects[1], { x: 900, y: 60, width: 248, height: 312, radius: 20 });
  assert.equal(geometryReport(outer, { rev: 1, mode: 'still' }, true, 1).rects.length, 1);
});

test('click sound suspends its AudioContext after the blip and closes it after a quiet spell', async () => {
  const timers = []; const fake = { set(fn, ms) { timers.push({ fn, ms }); return timers.length - 1; }, clear(id) { if (timers[id]) timers[id].fn = () => {}; } };
  const contexts = [];
  const make = () => { const node = () => ({ connect() {}, disconnect() {}, start() {}, stop() {}, type: '', frequency: { setValueAtTime() {} }, gain: { setValueAtTime() {}, exponentialRampToValueAtTime() {} } });
    const c = { state: 'running', currentTime: 0, destination: {}, createOscillator: node, createGain: node,
      suspend: async () => { c.state = 'suspended'; }, resume: async () => { c.state = 'running'; }, close: async () => { c.state = 'closed'; } };
    contexts.push(c); return c; };
  const play = createClickSound(make, fake);
  await play(true);
  assert.deepEqual(timers.map(t => t.ms), [SUSPEND_AFTER_MS, CLOSE_AFTER_MS]);
  timers[0].fn(); await null; assert.equal(contexts[0].state, 'suspended', 'no live PipeWire stream between clicks');
  await play(true); assert.equal(contexts.length, 1); assert.equal(contexts[0].state, 'running', 'the next click resumes the same context');
  for (const t of timers.slice(2)) t.fn(); await null;
  assert.equal(contexts[0].state, 'closed');
  await play(true); assert.equal(contexts.length, 2, 'a closed context is recreated on the next click');
  await play(false); assert.equal(contexts.length, 2);
});

test('launcher lets tiles move and hide around Settings; only Hermes home keeps its slot', () => {
  const policy = src('ui/island/launcher-policy.ts'), rust = src('src-tauri/src/services/workspace.rs');
  assert.doesNotMatch(policy, /Error\('Hermes home and Settings positions are fixed'\)/);
  assert.doesNotMatch(rust, /invalid\(\s*"Hermes home and Settings positions are fixed"/);
  assert.match(rust, /Hermes home and Settings cannot be hidden/);
});
