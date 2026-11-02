import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('withdrawal delay never intercepts the JS-driven shell size spring at zero offset', () => {
  const css = readFileSync(new URL('../ui/island/surface.css', import.meta.url), 'utf8');
  // Without data-glide (the normal zero-offset path), a delay on the default
  // transition-property: all restarts on EVERY spring frame and freezes the box.
  const base = [...css.matchAll(/(?:^|\n)\.island\s*\{([^}]+)\}/g)].map(m => m[1]).join(';');
  assert.match(base, /transition-property\s*:\s*transform\s*[;}]/,
    'the base surface must scope CSS transitions to translation, even without data-glide');
});
import { load } from './phase7b-loader.mjs';
const { axisSpring, springProgress, arrivalTime, sidePose, sideDropPath, WITHDRAW_SECONDS, glassFadePlan, glassOpenness, GLASS_STRETCH, surfaceRadius } = load('ui/island/motion-profile.ts');

test('glass lip: solid out of the notch, opens over the last 48 px, shuts at once on close', () => {
  const open = glassFadePlan(26, 640, true, 0);
  assert.equal(glassOpenness(open, 26), 0); assert.equal(glassOpenness(open, 640 - GLASS_STRETCH), 0);
  assert.ok(glassOpenness(open, 616) > .4 && glassOpenness(open, 616) < .6); assert.equal(glassOpenness(open, 640), 1);
  assert.equal(glassOpenness(open, 652), 1, 'overshoot stays open');
  const close = glassFadePlan(640, 26, false, 1);
  for (const h of [640, 300, 27]) assert.equal(glassOpenness(close, h), 0, 'closing shuts the glass at once');
  const resumed = glassFadePlan(300, 640, true, .5);
  assert.equal(+glassOpenness(resumed, 300).toFixed(6), .5, 'an interrupted motion starts from the openness on screen');
  assert.equal(glassOpenness(resumed, 640), 1);
  assert.equal(glassOpenness(glassFadePlan(640, 500, true, 1), 520), 1, 'a shrinking open page keeps its glass');
});

test('closing radius follows the drawn height into the notch, never a square box', () => {
  assert.equal(surfaceRadius(640), 28); assert.ok(Math.abs(surfaceRadius(26) - 8.84) < 1e-9);
  for (let h = 26; h <= 120; h++) assert.ok(surfaceRadius(h) >= 8.8, `radius at ${h}px`);
  const src = readFileSync(new URL('../ui/island/IslandSurface.tsx', import.meta.url), 'utf8');
  assert.match(src, /surfaceRadius\(h\)/, 'the per-frame radius comes from the drawn height');
  assert.doesNotMatch(src, /setTimeout/, 'notch arrival is a function of the drawn size, never a timer racing a reopen');
});

test('native-pill still frames release ShapeInput and KWin blur at the first withdrawing frame', () => {
  const src = readFileSync(new URL('../ui/island/geometry.ts', import.meta.url), 'utf8');
  assert.match(src, /settled \|\| \(still && nativePill\(\) !== null\)/,
    'an unsettled still frame keeps KWin blurring the fading shell: the grey box over the notch clock');
});

test('shell uses physical axis springs, not visual-duration guesses or suction tweens', () => {
  const grow = axisSpring(144, 700, 'width'), close = axisSpring(700, 144, 'width');
  assert.equal(grow.type, 'spring'); assert.equal(close.type, 'spring');
  assert.ok(Math.abs(grow.stiffness - (2 * Math.PI / .44) ** 2) < 1e-8);
  assert.ok(Math.abs(close.stiffness - (2 * Math.PI / .30) ** 2) < 1e-8);
  assert.equal(close.damping, 2 * Math.sqrt(close.stiffness));
  assert.equal(axisSpring(670, 26, 'height').damping, 2 * (2 * Math.PI / .26));
  for (let ms = 0; ms <= 1500; ms += 5) {
    const p = springProgress(close, ms / 1000);
    assert.ok(p >= 0 && p <= 1, 'closing never undershoots the pill');
  }
});

test('large shell growth has bounded overshoot and buttons wait for arrival', () => {
  const spring = axisSpring(144, 1080, 'width');
  let max = 0;
  for (let ms = 0; ms <= 2000; ms++) max = Math.max(max, springProgress(spring, ms / 1000));
  assert.ok((max - 1) * (1080 - 144) <= 12.01);
  const from = { width: 144, height: 26 }, to = { width: 700, height: 670 };
  const delay = arrivalTime(from, to);
  assert.ok(delay > .2 && delay < .6);
  for (const axis of ['width', 'height']) assert.ok(springProgress(axisSpring(from[axis], to[axis], axis), delay) >= .99);
  assert.equal(arrivalTime(to, to), 0);
  assert.equal(WITHDRAW_SECONDS, .16);
});

test('side circles withdraw only into their own edge, preserving their row', () => {
  for (const side of ['left', 'right', 'bottom']) {
    const hidden = sidePose(side, 44, 12, 0), shown = sidePose(side, 44, 12, 1);
    assert.deepEqual(shown, { x: 0, y: 0, scale: 1, opacity: 1 });
    assert.equal(hidden.scale, .4); assert.equal(hidden.opacity, 0);
    if (side === 'bottom') { assert.equal(hidden.x, 0); assert.equal(hidden.y, -48); }
    else { assert.equal(hidden.y, 0); assert.equal(hidden.x, side === 'left' ? 48 : -48); }
    const middle = sidePose(side, 44, 12, .6);
    assert.ok(middle.scale > hidden.scale && middle.scale < 1);
  }
});

test('liquid neck exists only during edge emergence and never leaves a settled bridge', () => {
  for (const side of ['left', 'right', 'bottom']) {
    assert.equal(sideDropPath(side, 44, 12, 0), '');
    assert.equal(sideDropPath(side, 44, 12, 1), '');
    assert.ok(sideDropPath(side, 44, 12, .8).length > 0);
    assert.doesNotMatch(sideDropPath(side, 44, 12, .8), /NaN|Infinity/);
  }
});
