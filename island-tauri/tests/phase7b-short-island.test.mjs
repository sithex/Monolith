// A short content-fitted page (an empty Agent runs list is one line of text) must open like
// every other page: a plain rounded shell whose edges the side buttons run along, never the
// half-morphed notch silhouette over a square blur box, with buttons hanging below it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load } from './phase7b-loader.mjs';
import { driver, nodes, globals } from './phase7b-runtime-harness.mjs';

const { notchMorphSpan, notchProgress, NOTCH_MORPH_SPAN } = load('ui/island/notch-profile.ts');
const { sideColumnExtent, sideRects } = load('ui/island/geometry.ts');
// The live layout shape: page buttons in slots 1 and 2 on both sides, two below.
const button = (id, side, slot) => ({ id, side, slot, action: { type: 'appGrid' }, label: null });
const sideLayout = { preset: { type: 'custom', width: 900, maxHeight: 960 }, buttonDiameter: 36, gap: 10,
  buttons: [button('l1', 'left', 1), button('l2', 'left', 2), button('r1', 'right', 1), button('r2', 'right', 2), button('b1', 'bottom', 1), button('b2', 'bottom', 2)] };
const PILL = { x: 0, y: 0, width: 300, height: 32, radius: 16 };

test('side column extent honours slot holes and matches where sideRects puts the circles', () => {
  assert.equal(sideColumnExtent({ ...sideLayout, buttons: [] }), 0);
  assert.equal(sideColumnExtent(sideLayout), 3 * 36 + 2 * 10);
  assert.equal(sideColumnExtent({ ...sideLayout, buttons: [button('a', 'left', 0), button('b', 'right', 4), button('c', 'bottom', 9)] }), 5 * 36 + 4 * 10);
  const island = { x: 100, y: 0, width: 560, height: sideColumnExtent(sideLayout) };
  const rects = sideRects(sideLayout, island);
  const lowest = Math.max(...rects.filter(r => !r.id.startsWith('b')).map(r => r.rect.y + r.rect.height));
  assert.equal(lowest, island.y + island.height, 'an island as tall as the extent ends level with its columns');
  for (const r of rects.filter(r => r.id.startsWith('b'))) assert.equal(r.rect.y, island.height + 10, 'bottom row sits one gap under the island');
});

test('notch morph span never outlasts the travel to the open height', () => {
  assert.equal(notchMorphSpan(32, 960), NOTCH_MORPH_SPAN);
  assert.equal(notchMorphSpan(32, 80), 48);
  assert.equal(notchMorphSpan(32, 32), 1);
  for (const open of [40, 80, 103, 104, 500]) {
    const span = notchMorphSpan(32, open);
    assert.equal(notchProgress(open, 32, span), 0, `open at ${open}px rests as the plain shell`);
    assert.equal(notchProgress(32, 32, span), 1, 'starts as the notch');
  }
  assert.equal(notchProgress(80 + 30, 32, 48), 0, 'spring overshoot stays plain');
});

function shortSurface({ buttons, pill }) {
  const state = { rev: 1, mode: 'large', view: 'terminal' }, starts = [];
  const animation = { stop() {}, start: async target => { starts.push(target); } };
  const d = driver({
    'framer-motion': { motion: { section: 'section' }, useReducedMotion: () => true, useAnimationControls: () => animation },
    'ui/ipc/island-state.ts': { useIslandState: () => state },
    'ui/ipc/client.ts': {},
    'ui/stores/index.ts': { useSettings: () => ({ offsetX: 0, offsetY: 0 }), usePalette: () => ({}) },
    'ui/stores/apps.ts': { useLayout: () => ({ rev: 1, unchangedSizing: false, layout: { ...sideLayout, buttons } }),
      useLauncher: () => ({ rev: 1, items: [] }) },
    'ui/island/geometry.ts': { ...load('ui/island/geometry.ts'), nativePill: () => pill },
    'ui/island/components/IslandShell.tsx': { UI_SPRING: {} },
    'ui/island/ApprovalNotice.tsx': { useApprovalHover: () => ({ active: false, width: 360, height: 116 }) },
    'ui/island/notifications/notification-hover.ts': { useNotificationHover: () => ({ active: false, width: 420, height: 200 }) },
  });
  const C = d.load('ui/island/IslandSurface.tsx').IslandSurface;
  return { ...d, state, starts, renderSurface: (props = {}) => d.render(C, { children: null, fail: e => { throw e; }, ...props }) };
}
/** Open the Agent runs page with one line of content (child 36px + 42px top bar + 2). */
function openShort(h, frames, drawn) {
  const tree = h.renderSurface();
  const child = { offsetHeight: 36, querySelector: () => null }, pane = { firstElementChild: child, offsetHeight: 36 };
  const root = { dataset: {}, getBoundingClientRect: () => drawn.current, style: { borderRadius: '', opacity: '', getPropertyValue: () => '', setProperty() {} },
    querySelector: sel => sel.includes('.app-pane') ? pane : sel === '.topline' ? { offsetHeight: 42 } : null };
  tree.props.ref.current = root;
  const path = nodes(tree).find(n => n.type === 'path');
  path.ref.current = { setAttribute() {} };
  h.state.view = 'agents'; h.state.rev++; h.renderSurface();
  while (frames.length) frames.shift()();
  return { root, tree: h.renderSurface() };
}
const env = frames => ({ innerWidth: 2560, innerHeight: 1440, addEventListener() {}, removeEventListener() {},
  ResizeObserver: class { observe() {} unobserve() {} disconnect() {} }, MutationObserver: class { observe() {} disconnect() {} },
  requestAnimationFrame: cb => { frames.push(cb); return frames.length; }, cancelAnimationFrame() {},
  getComputedStyle: () => ({ paddingTop: '0', paddingBottom: '0', borderRadius: '0' }), performance: { now: () => 0 } });

test('a one-line content-fitted page is at least as tall as the side-button columns', () => {
  const frames = [];
  globals(env(frames), () => {
    for (const [buttons, expected] of [[[], 80], [sideLayout.buttons, 128], [[button('l', 'left', 0)], 80], [[button('r', 'right', 4)], 220]]) {
      const h = shortSurface({ buttons, pill: null });
      try {
        const { tree } = openShort(h, frames, { current: { width: 560, height: 80 } });
        assert.equal(tree.props.initial.height, expected, `side buttons ${JSON.stringify(buttons.map(b => b.side + b.slot))}`);
        assert.equal(h.starts.at(-1).height, expected);
      } finally { h.dispose(); frames.length = 0; }
    }
  });
});

test('a short open island under the notch rests as the plain shell, not the notch morph skin', () => {
  const frames = [];
  globals(env(frames), () => {
    // No side buttons: the page rests at MIN_FIT_HEIGHT, 48px below a 32px notch.
    const h = shortSurface({ buttons: [], pill: PILL });
    try {
      const drawn = { current: { width: PILL.width, height: PILL.height } };
      const { root, tree } = openShort(h, frames, drawn);
      assert.equal(h.starts.at(-1).height, 80);
      tree.props.onUpdate({ width: 560, height: 56 });
      assert.equal(root.dataset.notching, 'true', 'mid-open it is still part notch');
      tree.props.onUpdate({ width: 560, height: 80 });
      assert.equal(root.dataset.notching, undefined, 'at rest the skin is gone (square blur box, notch ears)');
      // Closing from that rest starts from the same plain shape (no jump), then morphs into the notch.
      drawn.current = { width: 560, height: 80 };
      h.state.mode = 'still'; h.state.rev++;
      const closing = h.renderSurface();
      closing.props.onUpdate({ width: 560, height: 80 });
      assert.equal(root.dataset.notching, undefined, 'close starts plain');
      closing.props.onUpdate({ width: 400, height: 40 });
      assert.equal(root.dataset.notching, 'true');
      assert.ok(Number(root.dataset.notchProgress) > .8, 'and becomes the notch as it arrives');
    } finally { h.dispose(); frames.length = 0; }
  });
});
