import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
import { driver, nodes, labeled, deferred, tick } from './phase7b-runtime-harness.mjs';
const { swipeAxis, swipeDirection, swipeOffset, rubberband, projectDistance, FLICK_VELOCITY, wiggleTiming } = load('ui/island/launcher-motion.ts');
const { DUR, EASE_OUT, SPRING, setMotionSpeed } = load('ui/island/motion-profile.ts');

test('swipe needs ~10 px of slop before picking an axis; vertical intent never pages', () => {
  assert.equal(swipeAxis(6, 0), 'pending');
  assert.equal(swipeAxis(9, 4), 'pending');
  assert.equal(swipeAxis(10, 0), 'x');
  assert.equal(swipeAxis(20, 21), 'y');
  assert.equal(swipeDirection(5, 0, 400, 2, 1, 3), 0);
  assert.equal(swipeDirection(200, 201, 400, 2, 1, 3), 0);
});
test('a slow drag commits once the projected landing passes half a page', () => {
  assert.equal(swipeDirection(-200, 0, 400, 0, 1, 3), 0);
  assert.equal(swipeDirection(-201, 0, 400, 0, 1, 3), 1);
  assert.equal(swipeDirection(201, 0, 400, 0, 1, 3), -1);
  // Momentum below flick speed still counts: 0.29 px/ms coasts ~28.7 px, carrying 180 px past 200.
  assert.ok(Math.abs(projectDistance(-0.29) + 28.71) < 1e-9);
  assert.equal(swipeDirection(-180, 0, 400, 0, 1, 3), 0);
  assert.equal(swipeDirection(-180, 0, 400, -0.29, 1, 3), 1);
  assert.equal(swipeDirection(-180, 0, 400, 0.29, 1, 3), 0);
  assert.equal(swipeDirection(-200, 0, 0, 0, 1, 3), 0);
});
test('a flick decides by its direction: short flicks page, a flick back against the drag cancels, edges never wrap', () => {
  assert.equal(swipeDirection(-12, 1, 400, -FLICK_VELOCITY + 0.01, 1, 3), 0);
  assert.equal(swipeDirection(-12, 1, 400, -FLICK_VELOCITY, 1, 3), 1);
  assert.equal(swipeDirection(12, 1, 400, 0.5, 1, 3), -1);
  assert.equal(swipeDirection(-300, 0, 400, 0.5, 1, 3), 0, 'flicked back after a long drag');
  assert.equal(swipeDirection(140, 0, 400, 1, 0, 3), 0);
  assert.equal(swipeDirection(-140, 0, 400, -1, 2, 3), 0);
  assert.equal(swipeDirection(140, 0, 400, 1, 0, 1), 0);
});
test('swipe tracks 1:1 between pages; past an end the resistance grows with the pull', () => {
  assert.equal(swipeOffset(100, 1, 3, 400), 100);
  assert.equal(swipeOffset(-100, 1, 3, 400), -100);
  assert.equal(swipeOffset(-100, 0, 3, 400), -100);
  const small = swipeOffset(40, 0, 3, 400), big = swipeOffset(400, 0, 3, 400);
  assert.ok(small > 0 && small < 40);
  assert.ok(small / 40 > big / 400, 'fraction followed shrinks as the pull grows');
  assert.ok(big < 400 * 0.55, 'bounded by the rubber-band constant');
  assert.equal(swipeOffset(-40, 2, 3, 400), -small);
  assert.equal(rubberband(50, 0), 0);
});
test('tile wiggle samples stay within the requested period and always start at a negative phase', () => {
  for (const a of [0, 0.25, 0.5, 0.999]) for (const b of [0, 0.5, 0.999]) {
    const style = wiggleTiming(a, b);
    const period = parseFloat(style['--wiggle-period']), delay = parseFloat(style['--wiggle-delay']);
    assert.ok(period >= 0.22 && period <= 0.28);
    assert.ok(delay < 0 && delay >= -0.281);
  }
  assert.notDeepEqual(wiggleTiming(0, 0), wiggleTiming(0.9, 0.8));
});

async function fixture(run) {
  const doc = Object.assign(new EventTarget(), { hidden: false, documentElement: { dataset: { navVia: 'pointer' } }, body: {} });
  const win = new EventTarget();
  const before = [Object.getOwnPropertyDescriptor(globalThis, 'document'), Object.getOwnPropertyDescriptor(globalThis, 'window')];
  Object.defineProperty(globalThis, 'document', { value: doc, configurable: true });
  Object.defineProperty(globalThis, 'window', { value: win, configurable: true });
  let state = { rev: 1, editing: true, items: ['terminal', 'monitor', 'settings', 'music'].map(id => ({ id, view: id, enabled: true, known: true, locked: id === 'terminal' || id === 'settings' })) };
  let island = { mode: 'large', view: 'launcher' }, reduced = false;
  const calls = [], starts = [], positions = [], captures = [], navigated = [];
  const controls = { set: p => positions.push(p), start: p => { const flight = deferred(); starts.push({ ...flight, target: p }); return flight.promise; }, stop() {} };
  const h = driver({
    'framer-motion': { motion: { div: 'motion.div', button: 'motion.button' }, LayoutGroup: 'layout-group', AnimatePresence: 'presence', useAnimationControls: () => controls },
    'react-dom': { createPortal: child => child },
    'ui/ipc/island-state.ts': { useIslandState: () => island },
    'ui/island/components/motion-preference.ts': { useReducedMotionPreference: () => reduced },
    'ui/island/components/Icon.tsx': { Icon: 'icon' },
    'ui/ipc/client.ts': { launcherCommand: async op => { calls.push(op); return state; } },
    'ui/stores/apps.ts': { useLauncher: () => state, launcherStore: { apply: () => {}, getSnapshot: () => state } },
  });
  const { Launcher } = h.load('ui/island/Launcher.tsx');
  let tree;
  const containers = new Map();
  const render = () => {
    tree = h.render(Launcher, { navigate: view => navigated.push(view), fail: error => { throw error; } });
    for (const name of ['Apps', 'Hidden apps']) {
      const node = labeled(tree, name); if (!node) continue;
      const top = name === 'Apps' ? 200 : 500;
      const tiles = nodes(node).filter(n => n.props?.['data-tile']);
      const slots = tiles.map((n, i) => ({ dataset: { tile: n.props['data-tile'] }, offsetLeft: i * 122, offsetTop: 0, offsetWidth: 110,
        getBoundingClientRect: () => ({ left: 100 + i * 122, right: 210 + i * 122, top, bottom: top + 103, width: 110 }) }));
      const container = { scrollLeft: 0, scrollTop: 0, querySelectorAll: () => slots, getBoundingClientRect: () => ({ left: 100, right: 700, top, bottom: top + 120 }) };
      node.props.ref.current = container; containers.set(name, container);
    }
    return tree;
  };
  const pointer = (id, x = 240, y = 220) => ({ button: 0, pointerId: 7, clientX: x, clientY: y,
    currentTarget: { getBoundingClientRect: () => containers.get('Apps').querySelectorAll().find(n => n.dataset.tile === id).getBoundingClientRect(), setPointerCapture: id => captures.push(id) } });
  const event = (type, x, y, pointerId = 7) => { const e = new Event(type); Object.assign(e, { clientX: x, clientY: y, pointerId }); win.dispatchEvent(e); };
  const pickUp = () => { labeled(tree, 'Open System monitor').props.onPointerDown(pointer('monitor')); labeled(tree, 'Open System monitor').props.onPointerMove(pointer('monitor', 248)); render(); };
  const ghost = () => nodes(tree).find(n => n.props?.className === 'drag-ghost launcher-drag-ghost');
  try { render(); await run({ render, tree: () => tree, ghost, doc, event, pickUp, pointer, calls, starts, positions, captures, navigated,
    setIsland: value => { island = value; }, setReduced: value => { reduced = value; }, setState: value => { state = value; }, state: () => state }); }
  finally { h.dispose(); for (const [i, key] of ['document', 'window'].entries()) { if (before[i]) Object.defineProperty(globalThis, key, before[i]); else delete globalThis[key]; } }
}

test('wiggle is visible-only, honours reduced motion and page visibility, and keeps per-id timings', async () => fixture(async f => {
  assert.equal(f.tree().props['data-motion-active'], true);
  const faces = () => nodes(f.tree()).filter(n => n.props?.className === 'tile-face');
  const timing = faces()[1].props.style;
  assert.equal(faces()[0].props['data-wiggle'], false, 'Hermes stays fixed');
  f.setIsland({ mode: 'idle', view: 'launcher' }); f.render();
  assert.equal(f.tree().props['data-motion-active'], false);
  f.setIsland({ mode: 'large', view: 'launcher' }); f.setReduced(true); f.render();
  assert.equal(f.tree().props['data-motion-active'], false);
  assert.equal(f.tree().props['data-reduced-motion'], true);
  assert.equal(nodes(f.tree()).find(n => n.props?.['data-tile'] === 'monitor').props.layout, false);
  f.setReduced(false); f.doc.hidden = true; f.doc.dispatchEvent(new Event('visibilitychange')); f.render();
  assert.equal(f.tree().props['data-motion-active'], false);
  assert.equal(faces()[1].props.style, timing, 'rerenders never resample the phase');
}));
test('badge enters and exits at .9 plus opacity, with shared fast ease-out; keyboard is instant', async () => fixture(async f => {
  const badge = () => labeled(f.tree(), 'Hide System monitor');
  assert.deepEqual(badge().props.initial, { scale: 0.9, opacity: 0 });
  assert.deepEqual(badge().props.exit, { scale: 0.9, opacity: 0 });
  assert.deepEqual(badge().props.transition, { duration: DUR.fast, ease: EASE_OUT });
  f.setReduced(true); f.render();
  assert.equal(badge().props.initial.scale, 1);
  assert.equal(badge().props.transition.duration, DUR.fast, 'reduction retains the short fade');
  f.doc.documentElement.dataset.navVia = 'key'; f.render();
  assert.equal(badge().props.transition.duration, 0);
  assert.equal(nodes(f.tree()).find(n => n.props?.['data-tile'] === 'monitor').props.layout, false);
}));
test('a sub-6px movement remains a tile click and never captures a pointer', async () => fixture(async f => {
  f.setState({ ...f.state(), editing: false }); f.render();
  const tile = labeled(f.tree(), 'Open System monitor');
  tile.props.onPointerDown(f.pointer('monitor')); tile.props.onPointerMove(f.pointer('monitor', 243, 224));
  tile.props.onPointerUp(); tile.props.onClick();
  assert.deepEqual(f.navigated, ['monitor']); assert.deepEqual(f.captures, []); assert.equal(f.ghost(), undefined);
}));
test('a dropped tile keeps its ghost until SPRING.ui settles, even for a zero-distance drop', async () => fixture(async f => {
  f.pickUp(); assert.deepEqual(f.captures, [7]); assert.ok(f.ghost());
  labeled(f.tree(), 'Open System monitor').props.onClick(); assert.deepEqual(f.navigated, []);
  f.event('pointerup', 248, 220); f.render();
  assert.ok(f.ghost(), 'drop must not remove the moving tile'); assert.equal(f.starts.length, 1);
  const spring = SPRING.ui();
  assert.deepEqual(f.starts[0].target, { x: 222, y: 200, scale: 1, transition: { x: { ...spring, velocity: 0 }, y: { ...spring, velocity: 0 }, scale: spring } });
  assert.equal(nodes(f.tree()).find(n => n.props?.['data-tile'] === 'monitor').props.style.opacity, 0);
  f.starts[0].resolve(); await tick(); f.render();
  assert.equal(f.ghost(), undefined);
  assert.equal(nodes(f.tree()).find(n => n.props?.['data-tile'] === 'monitor').props.style.opacity, 1);
}));
test('drag live reflow and server move survive the glide, with no per-frame left/top writes', async () => fixture(async f => {
  f.pickUp(); f.event('pointermove', 450, 220); f.render();
  assert.equal(f.ghost().props.style.left, 0); assert.equal(f.ghost().props.style.top, 0);
  assert.ok(f.positions.length > 1, 'pointer motion updates transform controls');
  f.event('pointerup', 450, 220); f.render(); await tick();
  assert.deepEqual(f.calls, [{ type: 'move', id: 'monitor', to: 2 }]);
  assert.equal(f.starts[0].target.x, 344);
  f.starts[0].resolve(); await tick(); f.render(); assert.equal(f.ghost(), undefined);
}));
test('dropping into Hidden glides to that grid and preserves the disable operation', async () => fixture(async f => {
  f.pickUp(); f.event('pointermove', 150, 520); f.render();
  f.event('pointerup', 150, 520); f.render(); await tick();
  assert.deepEqual(f.calls, [{ type: 'disable', id: 'monitor' }]);
  assert.equal(f.starts[0].target.x, 100); assert.equal(f.starts[0].target.y, 500);
  f.starts[0].resolve(); await tick(); f.render(); assert.equal(f.ghost(), undefined);
}));
test('an unrelated pointer cannot move or finish a tile drag', async () => fixture(async f => {
  f.pickUp(); const positionCount = f.positions.length;
  f.event('pointermove', 450, 220, 99); f.render();
  f.event('pointerup', 450, 220, 99); f.render();
  assert.ok(f.ghost()); assert.equal(f.positions.length, positionCount); assert.equal(f.starts.length, 0);
}));
test('a new pickup interrupts the previous landing without an old completion clearing the new drag', async () => fixture(async f => {
  f.pickUp(); f.event('pointerup', 248, 220); f.render(); assert.equal(f.starts.length, 1);
  f.pickUp(); f.starts[0].resolve(); await tick(); f.render();
  assert.ok(f.ghost());
  assert.equal(nodes(f.tree()).find(n => n.props?.['data-tile'] === 'monitor').props.style.opacity, 0);
}));
test('reduced motion has no pickup scale or drop glide', async () => fixture(async f => {
  f.setReduced(true); f.render(); f.pickUp();
  assert.equal(f.ghost().props.children.props.animate.scale, 1);
  f.event('pointerup', 248, 220); f.render();
  assert.equal(f.ghost(), undefined); assert.equal(f.starts.length, 0);
}));
test('closing interrupts a landing and cannot leave a portal ghost behind', async () => fixture(async f => {
  f.pickUp(); f.event('pointerup', 248, 220); f.render(); assert.equal(f.starts.length, 1);
  f.setIsland({ mode: 'idle', view: 'launcher' }); f.render(); f.render();
  assert.equal(f.ghost(), undefined);
  f.starts[0].resolve(); await tick(); f.render(); assert.equal(f.ghost(), undefined);
}));
test('wiggle CSS owns rotation on a child, is paused when hidden and gates hover to fine pointers', () => {
  const css = readFileSync(new URL('../ui/island/motion-launcher.css', import.meta.url), 'utf8');
  const tsx = readFileSync(new URL('../ui/island/Launcher.tsx', import.meta.url), 'utf8');
  assert.match(css, /animation-play-state: paused/);
  assert.match(css, /data-motion-active='true'[\s\S]*?animation-play-state: running/);
  assert.match(css, /@media \(hover: hover\) and \(pointer: fine\)/);
  assert.match(css, /@media \(prefers-reduced-motion: reduce\)[\s\S]*?animation: none/);
  assert.match(css, /var\(--motion-speed, 1\)/);
  assert.doesNotMatch(tsx, /repeat: Infinity|animate=\{\{ rotate|index % 4/);
});
test('launcher badge and tile springs use the animation-speed tokens', () => {
  setMotionSpeed(200);
  try { assert.equal(SPRING.ui().visualDuration, 0.14); }
  finally { setMotionSpeed(100); }
});
test('launcher shows 5 apps per row, fewer only when narrow', () => {
  const tsx = readFileSync(new URL('../ui/island/Launcher.tsx', import.meta.url), 'utf8');
  const body = /export function balancedColumns\(_?count: number, fit: number\): number \{([\s\S]*?)\n\}/.exec(tsx)[1];
  const balancedColumns = new Function('count', 'fit', body);
  assert.equal(balancedColumns(16, 8), 5);
  assert.equal(balancedColumns(40, 12), 5);
  assert.equal(balancedColumns(16, 3), 3);
  assert.equal(balancedColumns(3, 8), 5); // short lists keep full-size columns (tiles stretch)
  assert.equal(balancedColumns(0, 8), 5);
  const css = readFileSync(new URL('../ui/island/surface.css', import.meta.url), 'utf8');
  assert.match(css, /\.app-grid\[data-cols\] \{ grid-template-columns: repeat\(var\(--cols\), 100px\)/);
});

test('compact pages: one top bar under the notch, small pages hug their content (Nick, Nov 9)', () => {
  const css = readFileSync(new URL('../ui/island/island.css', import.meta.url), 'utf8');
  assert.match(css, /\.island:has\(> \.topline\[data-notch-clock='true'\]\) \.island-shell > \.title-row \{\s*margin-top: -42px; height: 42px;/);
  assert.match(css, /title-row \{[^}]*pointer-events: none/);
  const tsx = readFileSync(new URL('../ui/island/IslandSurface.tsx', import.meta.url), 'utf8');
  for (const view of ['launcher', 'agents', 'mixer']) assert.match(tsx, new RegExp(`COMPACT_VIEWS[^\\n]*'${view}'`));
  assert.match(tsx, /export const LAUNCHER_WIDTH = 560;/);
  // The fit-width measurement must not read the island's live (springing) width.
  assert.doesNotMatch(tsx, /wantW = Math\.ceil\(root\.offsetWidth/);
  const mixer = readFileSync(new URL('../ui/island/components/VolumeMixer.tsx', import.meta.url), 'utf8');
  assert.match(mixer, /className="mixer-strip" data-fit-width=""/);
});
