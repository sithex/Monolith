import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
const { clipToWindow, geometryReport } = load('ui/island/geometry.ts');

function viewport(run) {
  const saved = ['innerWidth', 'innerHeight', 'getComputedStyle'].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]);
  globalThis.innerWidth = 300; globalThis.innerHeight = 200;
  try { run(); } finally {
    for (const [key, descriptor] of saved) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  }
}
const rect = (x, y, width, height, radius = 8) => ({ x, y, width, height, radius });

test('spring overshoot is clipped at all four viewport edges without mutating valid surfaces', () => viewport(() => {
  const valid = { ...rect(10, 20, 30, 40), id: 'unchanged' };
  const input = [rect(-5, -10, 30, 40), rect(280, 190, 50, 40), valid];
  const result = clipToWindow(input);
  assert.deepEqual(result, [rect(0, 0, 25, 30), rect(280, 190, 20, 10), valid]);
  assert.equal(result[2], valid);
  assert.deepEqual(input[0], rect(-5, -10, 30, 40));
}));

test('empty, invalid and subpixel slivers are dropped, radii remain within Rust limits', () => viewport(() => {
  const invalid = [rect(-20, 0, 10, 20), rect(300, 0, 20, 20), rect(0, 200, 20, 20),
    rect(0, 0, -1, 20), rect(0, 0, .5, 20), rect(NaN, 0, 20, 20), rect(0, 0, Infinity, 20)];
  assert.deepEqual(clipToWindow(invalid), []);
  assert.deepEqual(clipToWindow([rect(0, 0, 20, 20, 101), rect(0, 0, 20, 20, -1), rect(0, 0, 20, 20, NaN)]),
    [rect(0, 0, 20, 20, 100), rect(0, 0, 20, 20, 0), rect(0, 0, 20, 20, 0)]);
}));

test('geometry reports clip shell, side controls and notification extensions together', () => viewport(() => {
  const button = { dataset: {}, getClientRects: () => [{}], offsetLeft: 290, offsetTop: 20, offsetWidth: 30, offsetHeight: 30 };
  const notification = { dataset: { islandExtension: 'notification' }, getClientRects: () => [{}], getBoundingClientRect: () => rect(20, 190, 80, 30) };
  const outer = { getBoundingClientRect: () => rect(-4, -5, 100, 90), ownerDocument: {
    querySelectorAll: selector => selector === '[data-side-button]' ? [button] : [notification],
  } };
  globalThis.getComputedStyle = () => ({ visibility: 'visible', borderRadius: '8px' });
  const report = geometryReport(outer, { rev: 7, mode: 'large' }, false, 1);
  assert.equal(report.stateRev, 7);
  assert.deepEqual(report.rects, [rect(0, 0, 96, 85), rect(290, 20, 10, 30), rect(20, 190, 80, 10)]);
}));

test('monitor geometry changes reuse the existing online-only settled relocation timer', () => {
  const main = readFileSync(new URL('../../island/plasma-anchor/contents/ui/main.qml', import.meta.url), 'utf8');
  assert.match(main, /onScreenGeometryChanged: if \(root.online\) panelSettled.restart\(\)/);
  assert.match(main, /Timer \{ id: panelSettled; interval: 450; onTriggered: root.requests.enqueue\("moved"\) \}/);
});
