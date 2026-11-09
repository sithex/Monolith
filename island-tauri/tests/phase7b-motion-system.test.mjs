import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
import { driver, globals, nodes, textButton } from './phase7b-runtime-harness.mjs';
const { confirmationPhase, CONFIRM_ARM_MS, CONFIRM_EXPIRE_MS } = load('ui/views/settings/confirmation-guard.ts');

function clock(t) {
  let now = 100;
  t.mock.method(performance, 'now', () => now);
  t.mock.timers.enable({ apis: ['setTimeout'] });
  return ms => { now += ms; t.mock.timers.tick(ms); };
}
test('confirmation phase is fail-closed and uses exact arm/expiry boundaries', () => {
  assert.equal(CONFIRM_ARM_MS, 600); assert.equal(CONFIRM_EXPIRE_MS, 4000);
  for (const [start, now] of [[null, 100], [100, 99], [NaN, 100], [100, Infinity]]) assert.equal(confirmationPhase(start, now), 'idle');
  assert.equal(confirmationPhase(100, 100), 'arming');
  assert.equal(confirmationPhase(100, 699.99), 'arming');
  assert.equal(confirmationPhase(100, 700), 'ready');
  assert.equal(confirmationPhase(100, 4099.99), 'ready');
  assert.equal(confirmationPhase(100, 4100), 'idle');
});
test('shared confirmation guard rejects early/repeated events and expires visually', t => {
  const advance = clock(t), d = driver(), hook = d.load('ui/views/settings/use-confirmation-guard.ts').useConfirmationGuard;
  try {
    let guard = d.render(hook); guard.arm('first'); guard = d.render(hook);
    assert.equal(guard.ready, false); assert.equal(guard.accept(), false);
    advance(599); assert.equal(guard.accept(), false);
    advance(1); guard = d.render(hook); assert.equal(guard.ready, true);
    assert.equal(guard.accept(), true); assert.equal(guard.accept(), false);
    guard = d.render(hook); assert.equal(guard.pending, null);
    guard.arm('second'); guard = d.render(hook); advance(4000);
    guard = d.render(hook); assert.equal(guard.pending, null); assert.equal(guard.accept(), false);
  } finally { d.dispose(); }
});
test('rearming restarts the window and stale expiry is cleaned up', t => {
  const advance = clock(t), d = driver(), hook = d.load('ui/views/settings/use-confirmation-guard.ts').useConfirmationGuard;
  try {
    let guard = d.render(hook); guard.arm('first'); guard = d.render(hook);
    advance(3800); guard.arm('replacement'); guard = d.render(hook);
    advance(200); guard = d.render(hook); assert.equal(guard.pending, 'replacement'); assert.equal(guard.accept(), false);
    advance(400); guard = d.render(hook); assert.equal(guard.ready, true);
    guard.disarm(); guard = d.render(hook); assert.equal(guard.pending, null);
    advance(4000); assert.equal(d.render(hook).pending, null);
  } finally { d.dispose(); }
});
function processHarness() {
  const calls = [], row = { pid: 42, ppid: 1, startTicks: 420, name: 'fixture', command: 'fixture', cpu: 5, memoryBytes: 1024, gpu: null, vramBytes: null, threads: 1, own: true };
  const d = driver({
    'ui/ipc/client.ts': { processList: () => {}, processKill: async (...args) => calls.push(args) },
    'ui/views/visible-poller.ts': { createVisiblePoller: (_read, apply) => ({ setVisible: on => { if (on) apply({ rows: [row], cpuCount: 4, gpuAvailable: false }); }, dispose() {} }) },
  });
  const C = d.load('ui/views/ProcessesPanel.tsx').ProcessesPanel;
  const render = () => d.render(C, { active: true }); render();
  return { ...d, calls, render };
}
test('process force kill cannot double-click through arming and submits only once', t => {
  const advance = clock(t), h = processHarness();
  try {
    textButton(h.render(), 'Kill').props.onClick();
    let confirm = textButton(h.render(), 'Force kill'); assert.equal(confirm.props.disabled, true);
    confirm.props.onClick(); assert.deepEqual(h.calls, []);
    advance(600); confirm = textButton(h.render(), 'Force kill'); assert.equal(confirm.props.disabled, false);
    confirm.props.onClick(); confirm.props.onClick(); assert.deepEqual(h.calls, [[42, 420, 'kill']]);
  } finally { h.dispose(); }
});
test('process confirmation auto-disarms and stale handlers cannot signal after expiry', t => {
  const advance = clock(t), h = processHarness();
  try {
    textButton(h.render(), 'Kill').props.onClick(); const stale = textButton(h.render(), 'Force kill');
    advance(4000); stale.props.onClick();
    assert.deepEqual(h.calls, []); assert.ok(textButton(h.render(), 'Kill'));
  } finally { h.dispose(); }
});
test('rail waits once, skips the delay for neighbours, and resets on leaving', t => {
  const advance = clock(t);
  globals({ matchMedia: () => ({ matches: true }) }, () => {
    const d = driver(), hook = d.load('ui/views/settings/use-rail-tooltip.ts').useRailTooltip;
    try {
      let rail = d.render(hook); rail.enter('general'); advance(299); assert.equal(d.render(hook).shown, null);
      advance(1); rail = d.render(hook); assert.equal(rail.shown, 'general');
      rail.leave(); rail.enter('look'); rail = d.render(hook); assert.equal(rail.shown, 'look');
      rail.reset(); rail.enter('terminal'); advance(299); assert.equal(d.render(hook).shown, null);
      advance(1); assert.equal(d.render(hook).shown, 'terminal');
    } finally { d.dispose(); }
  });
});
test('sweeping an unopened rail cancels pending tips and touch never schedules them', t => {
  const advance = clock(t); let hover = true;
  globals({ matchMedia: () => ({ matches: hover }) }, () => {
    const d = driver(), hook = d.load('ui/views/settings/use-rail-tooltip.ts').useRailTooltip;
    try {
      let rail = d.render(hook); rail.enter('general'); advance(150); rail.leave(); rail.enter('look');
      advance(150); assert.equal(d.render(hook).shown, null);
      rail.reset(); hover = false; rail.enter('general'); advance(1000); assert.equal(d.render(hook).shown, null);
    } finally { d.dispose(); }
  });
});
test('timer reduced-motion digits fade without transforms and keyboard changes do not animate', () => {
  const d = driver({}, (path, source) => path === 'ui/island/components/TimerApp.tsx' ? `${source}\nexport { RollingText, MinuteWheel };` : source);
  try {
    const { RollingText } = d.load('ui/island/components/TimerApp.tsx');
    const digits = nodes(RollingText({ text: '1:23', reduced: true })).filter(n => n.props?.className === 'digit');
    assert.equal(digits.length, 4);
    for (const digit of digits) {
      assert.deepEqual(digit.props.initial, { opacity: 0 }); assert.deepEqual(digit.props.animate, { opacity: 1 });
      assert.deepEqual(digit.props.exit, { opacity: 0 }); assert.ok(digit.props.transition.duration <= .16);
    }
    globals({ document: { documentElement: { dataset: { navVia: 'key' } } } }, () => {
      const digit = nodes(RollingText({ text: '2', reduced: false })).find(n => n.props?.className === 'digit');
      assert.equal(digit.props.transition.duration, 0); assert.deepEqual(digit.props.animate, { opacity: 1 });
    });
  } finally { d.dispose(); }
});
test('timer wheel keyboard/wheel changes and reduced-motion settling bypass springs', () => {
  const animated = [], values = [];
  const d = driver({ 'framer-motion': {
    motion: { div: 'div', span: 'span' },
    useMotionValue: initial => { let value = initial; return { get: () => value, set: next => { value = next; } }; },
    useMotionValueEvent() {}, useTransform: (value, f) => f(value.get()),
    animate: (...args) => { animated.push(args); return { stop() {} }; },
  } }, (path, source) => path === 'ui/island/components/TimerApp.tsx' ? `${source}\nexport { MinuteWheel };` : source);
  try {
    const { MinuteWheel } = d.load('ui/island/components/TimerApp.tsx');
    let wheel = d.render(MinuteWheel, { value: 25, onChange: value => values.push(value), reduced: false, disabled: false });
    wheel.props.onKeyDown({ key: 'ArrowRight', preventDefault() {}, stopPropagation() {} });
    wheel.props.onWheel({ deltaY: 24, deltaX: 0, deltaMode: 0 });
    assert.deepEqual(values, [26, 27]); assert.deepEqual(animated, []);
    wheel = d.render(MinuteWheel, { value: 25, onChange: value => values.push(value), reduced: true, disabled: false });
    const target = { getBoundingClientRect: () => ({ left: 0, width: 200 }), setPointerCapture() {} };
    wheel.props.onPointerDown({ button: 0, clientX: 50, pointerId: 1, currentTarget: target, timeStamp: 0 });
    wheel.props.onPointerMove({ clientX: 60, timeStamp: 20 });
    wheel.props.onPointerUp({ clientX: 60, currentTarget: target, timeStamp: 30 });
    assert.deepEqual(animated, [], 'reduced-motion release does not coast or spring');
  } finally { d.dispose(); }
});
test('mixer drag and key input update a transform directly without spring interpolation', () => {
  const d = driver({}, (path, source) => path === 'ui/island/components/VolumeMixer.tsx' ? `${source}\nexport { Fader };` : source);
  try {
    const { Fader } = d.load('ui/island/components/VolumeMixer.tsx');
    const props = { value: 20, label: 'Fixture', muted: false, onCommit() {} };
    let tree = d.render(Fader, props);
    nodes(tree).find(n => n.props?.className === 'fader-track').props.ref.current = { getBoundingClientRect: () => ({ bottom: 200, height: 200 }) };
    tree.props.onPointerDown({ button: 0, pointerId: 1, clientY: 50, currentTarget: { setPointerCapture() {} } });
    tree = d.render(Fader, props);
    let fill = nodes(tree).find(n => n.props?.className === 'fader-fill');
    assert.equal(fill.props.style.transform, 'scaleY(0.75)'); assert.equal(fill.props['data-direct'], true);
    tree.props.onPointerMove({ clientY: 80 }); tree = d.render(Fader, props);
    fill = nodes(tree).find(n => n.props?.className === 'fader-fill'); assert.equal(fill.props.style.transform, 'scaleY(0.6)');
    tree.props.onKeyDown({ key: 'ArrowDown', preventDefault() {}, stopPropagation() {} }); tree = d.render(Fader, props);
    fill = nodes(tree).find(n => n.props?.className === 'fader-fill'); assert.equal(fill.props.style.transform, 'scaleY(0.58)');
    assert.equal(fill.props['data-direct'], true);
  } finally { d.dispose(); }
});
test('system meter CSS uses transforms and disables interpolation for both reduced-motion sources', () => {
  const css = readFileSync(new URL('../ui/island/motion-system.css', import.meta.url), 'utf8');
  assert.match(css, /transform-origin: left/); assert.match(css, /transform-origin: bottom/);
  assert.match(css, /transition: transform var\(--dur-slow\) var\(--ease-out\)/);
  assert.match(css, /html\[data-reduce-motion='true'\]/); assert.match(css, /prefers-reduced-motion: reduce/);
  assert.match(css, /\.fader-fill\[data-direct='true'\]/);
  for (const file of ['AiDashboard', 'AiInsights', 'VolumeMixer']) {
    const src = readFileSync(new URL(`../ui/island/components/${file}.tsx`, import.meta.url), 'utf8');
    assert.match(src, /transform: `scale[XY]\(/);
  }
});
