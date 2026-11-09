import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import React from 'react';
import ts from 'typescript';
import { load } from './phase7b-loader.mjs';

const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const jsx = (type, props, key) => React.createElement(type, { ...props, key });
function compile(path, mocks, globals = {}) {
  const js = ts.transpileModule(src(path), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX } }).outputText;
  const module = { exports: {} };
  const require = name => {
    if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
    if (name in mocks) return mocks[name];
    throw Error(`Unexpected standalone import: ${name}`);
  };
  vm.runInNewContext(`(function(require,module,exports){${js}\n})`, globals)(require, module, module.exports);
  return module.exports;
}
function walk(node) {
  if (!node || typeof node !== 'object') return [];
  if (Array.isArray(node)) return node.flatMap(walk);
  return [node, ...walk(node.props?.children)];
}

test('notification exit retracts upward in 150ms ease-out; each reduced-motion source keeps only opacity', () => {
  for (const source of ['none', 'setting', 'html', 'media', 'hook']) {
    const popup = { notification: { id: 'test' } };
    const mocks = {
      react: { useRef: () => ({ current: null }), useEffect() {}, useLayoutEffect() {} },
      'framer-motion': { useReducedMotion: () => source === 'hook', AnimatePresence: 'presence', motion: { div: 'motion-div' } },
      '../../stores': { useSettings: () => ({ reduceMotion: source === 'setting', animationSpeed: 100 }) },
      '../../ipc/island-state': { useIslandState: () => ({ mode: 'hover' }) },
      '../../ipc/notifications': { usePopups: () => ({ items: [popup] }) },
      '../geometry': {}, './notification-cards.css': {},
      './notification-hover': { useShownPopups: () => [popup] },
    };
    const { NotchNotifications } = compile('ui/island/notifications/NotificationCards.tsx', mocks, {
      document: { documentElement: { dataset: { reduceMotion: String(source === 'html') } } },
      matchMedia: () => ({ matches: source === 'media' }),
    });
    const card = walk(NotchNotifications({ fail: assert.fail, placement: 'hover' })).find(n => n.type === 'motion-div');
    const reduced = source !== 'none';
    assert.equal(card.props.exit.y, reduced ? 0 : -8, source);
    assert.equal(card.props.exit.scale, reduced ? 1 : .97, source);
    assert.equal(card.props.exit.opacity, 0);
    assert.equal(card.props.transition.duration, .15);
    assert.deepEqual([...card.props.transition.ease], [.23, 1, .32, 1]);
    assert.equal(card.props.layout, !reduced);
  }
});

test('countdown remains real-time information, not a reduced-motion decoration', () => {
  assert.match(src('ui/island/notifications/notification-cards.css'), /animation: notification-countdown linear forwards/);
  assert.match(src('ui/island/polish.css'), /\*:not\(\.notification-timer > span\)/);
  assert.doesNotMatch(src('ui/island/motion-transient.css'), /notification-countdown/);
});

test('approval brightness follows the existing deadline, resets for replacement prompts and cleans up timers', () => {
  const { ARM_DELAY_MS, canPress, noticeFor, KIND_LABEL } = load('ui/island/approval-model.ts');
  const slots = [], effects = [], timers = new Map(), answers = [];
  let index = 0, now = 1000, nextTimer = 0;
  let prompt = { id: 'first', kind: 'approval', text: 'A command', more: 0, answerable: true, answered: false,
    choices: [{ id: 'once', label: 'Once', style: 'once' }] };
  const hooks = {
    useState(initial) { const i = index++; slots[i] ??= { value: initial }; return [slots[i].value, v => { slots[i].value = typeof v === 'function' ? v(slots[i].value) : v; }]; },
    useRef(initial) { const i = index++; return slots[i] ??= { current: initial }; },
    useEffect(fn, deps) { const i = index++, old = slots[i]; if (!old || deps.some((v, n) => v !== old.deps[n])) { slots[i] = { deps }; effects.push(() => { old?.cleanup?.(); slots[i].cleanup = fn(); }); } },
  };
  const { ApprovalNotice } = compile('ui/island/ApprovalNotice.tsx', {
    react: hooks, './approval-notice.css': {},
    '../ipc/island-state': { useIslandState: () => ({ rev: 1 }) },
    '../ipc/approvals': { useApproval: () => ({ prompt }), answerApproval: async (...args) => answers.push(args) },
    './approval-model': { ARM_DELAY_MS, canPress, noticeFor, KIND_LABEL, CHOICE_HINT: {}, needsExpand: () => false },
  }, {
    performance: { now: () => now }, window: { addEventListener() {}, removeEventListener() {} },
    setTimeout(fn, ms) { const id = ++nextTimer; timers.set(id, { at: now + ms, fn }); return id; },
    clearTimeout(id) { timers.delete(id); },
  });
  const render = () => { index = 0; const node = ApprovalNotice({ fail: assert.fail }); while (effects.length) effects.shift()(); return node; };
  const button = node => walk(node.type(node.props)).find(n => n.props?.['data-testid'] === 'approval-once');
  const advance = ms => { now += ms; for (const [id, timer] of timers) if (timer.at <= now) { timers.delete(id); timer.fn(); } };
  try {
    let node = render(); assert.equal(node.props.armed, false);
    advance(ARM_DELAY_MS - 1); node = render(); assert.equal(node.props.armed, false);
    button(node).props.onClick({ detail: 1, stopPropagation() {} }); assert.equal(answers.length, 0);
    advance(1); node = render(); assert.equal(node.props.armed, true);
    assert.equal(button(node).props['aria-disabled'], false);
    button(node).props.onClick({ detail: 1, stopPropagation() {} }); assert.equal(answers.length, 1);
    prompt = { ...prompt, id: 'second' }; node = render(); assert.equal(node.props.armed, false);
    assert.equal(button(node).props['aria-disabled'], true);
    advance(100); prompt = { ...prompt, id: 'third' }; render();
    assert.equal(timers.size, 1, 'replacement cancels the old prompt timer');
    advance(ARM_DELAY_MS - 100); node = render(); assert.equal(node.props.armed, false);
    advance(100); node = render(); assert.equal(node.props.armed, true);
    prompt = { ...prompt, id: 'fourth' }; render(); assert.equal(timers.size, 1);
  } finally { slots.forEach(slot => slot?.cleanup?.()); }
  assert.equal(timers.size, 0, 'unmount cancels the arm indicator timer');
});

test('dropdown direction/origins and picker reveals use transform/opacity, never zero scale or animated height', () => {
  const css = src('ui/island/motion-transient.css');
  assert.match(css, /transient-dropdown-in[^}]*translateY\(-4px\) scale\(\.97\)/s);
  assert.match(css, /transient-dropdown-up[^}]*translateY\(4px\) scale\(\.97\)/s);
  assert.match(css, /data-up='true'\] \.dropdown-menu \{\s*transform-origin: center bottom/);
  assert.match(css, /font-picker\[data-up='true'\] \.font-menu[\s\S]*?transform-origin: right bottom/);
  assert.match(css, /scheme-variants, \.island \.scheme-custom \{ transform-origin: left top/);
  const scheme = src('ui/island/SchemePicker.tsx');
  // Accordion rows push siblings, so height animates there (controller decision, Nov 9); never under reduced motion.
  assert.match(scheme, /const expand = keyboard \|\| reduced \? \{ duration: 0 \}/);
  assert.match(scheme, /key="custom"[^>]*exit="closed"[^>]*variants=\{variants\}/);
  for (const file of ['SchemePicker.tsx', 'CommandPalette.tsx', 'ApprovalNotice.tsx', 'components/ColorField.tsx', 'components/AppPrimitives.tsx'])
    assert.doesNotMatch(src(`ui/island/${file}`), /scale:\s*0(?:\s*[,}])/);
});
