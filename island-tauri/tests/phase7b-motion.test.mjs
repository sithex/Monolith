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
const { axisSpring, springProgress, arrivalTime, sidePose, sideDropPath, WITHDRAW_SECONDS, glassFadePlan, glassOpenness, GLASS_STRETCH, surfaceRadius, handOffFade, axisSpring: spring2 } = load('ui/island/motion-profile.ts');

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

test('notch hand-off fade stays inside the notch footprint (never see-through past it)', () => {
  const pill = { width: 144, height: 26 };
  assert.equal(handOffFade(144, 26, pill), 0);
  assert.equal(handOffFade(144, 40, pill), 1);
  assert.equal(handOffFade(200, 27, pill), 1, 'a bar wider than the notch stays opaque even at notch height');
  assert.equal(handOffFade(156, 26, pill), .5);
  // Replay the real close springs (width settles after height): whenever the shell is
  // translucent it is within 24 px of the notch width and 14 px of its height.
  const w = spring2(1040, 144, 'width'), h = spring2(672, 26, 'height');
  for (let f = 1; f <= 240; f++) {
    const s = f / 240, W = 1040 + (144 - 1040) * springProgress(w, s), H = 672 + (26 - 672) * springProgress(h, s);
    const o = handOffFade(W, H, pill);
    if (o < 1) assert.ok(W - 144 <= 24 && H - 26 <= 14, `frame ${f}: translucent at ${W.toFixed(1)}x${H.toFixed(1)}`);
  }
  const src = readFileSync(new URL('../ui/island/IslandSurface.tsx', import.meta.url), 'utf8');
  assert.match(src, /handOffFade\(/, 'the shell fade uses both axes');
});

test('closing radius follows the drawn height into the notch, never a square box', () => {
  assert.equal(surfaceRadius(640), 28); assert.ok(Math.abs(surfaceRadius(26) - 8.84) < 1e-9);
  for (let h = 26; h <= 120; h++) assert.ok(surfaceRadius(h) >= 8.8, `radius at ${h}px`);
  const src = readFileSync(new URL('../ui/island/IslandSurface.tsx', import.meta.url), 'utf8');
  assert.match(src, /surfaceRadius\(h\)/, 'the per-frame radius comes from the drawn height');
  assert.doesNotMatch(src, /setTimeout/, 'notch arrival is a function of the drawn size, never a timer racing a reopen');
});

test('native-pill close: blur held only until the solid frame is on screen, released long before the fade', () => {
  const src = readFileSync(new URL('../ui/island/geometry.ts', import.meta.url), 'utf8');
  assert.match(src, /settled \|\| \(still && !blurHeld && nativePill\(\) !== null\)/,
    'still frames settle (releasing blur) once the hold ends; never left unsettled into the notch fade (grey box)');
  assert.match(src, /blurHeld\) schedule\(\)/, 'the hold keeps reporting so the settled report is always sent');
  const ms = +src.match(/BLUR_RELEASE_MS = (\d+)/)[1];
  assert.ok(ms >= 50, 'covers WebKit presenting the solid frame after the rAF (see-through flash otherwise)');
  // The fade into the notch starts only within 14 px of the notch height: replay the close
  // spring and require the hold to end well before that.
  const h = axisSpring(672, 26, 'height');
  const fadeStart = Array.from({ length: 480 }, (_, f) => f / 1000).find(s => 672 + (26 - 672) * springProgress(h, s) <= 40);
  assert.ok(ms / 1000 < fadeStart / 2, `hold ${ms} ms vs fade at ${fadeStart * 1000} ms`);
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

test('glass gradient covers the border box (no 1px solid/translucent rows on top and bottom edges)', () => {
  const css = readFileSync(new URL('../ui/island/surface.css', import.meta.url), 'utf8');
  const rule = css.match(/html\[data-glass='true'\] \.island \{[\s\S]*?\n\}/)[0];
  const shorthand = rule.indexOf('background: linear-gradient');
  assert.ok(shorthand > 0);
  for (const prop of ['background-origin: border-box', 'background-repeat: no-repeat'])
    assert.ok(rule.indexOf(prop) > shorthand, `${prop} must follow the background shorthand, which resets it`);
});
