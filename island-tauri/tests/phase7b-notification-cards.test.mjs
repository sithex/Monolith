import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import vm from 'node:vm';
import ts from 'typescript';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
const nativeRequire = createRequire(import.meta.url);
const file = new URL('../ui/island/notifications/NotificationCards.tsx', import.meta.url);
const code = ts.transpileModule(readFileSync(file, 'utf8'), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX } }).outputText;
const module = { exports: {} };
const mocks = {
  'framer-motion': { motion: { article: ({ onUpdate, ...props }) => React.createElement('article', props),
    span: ({ initial, animate, transition, ...props }) => React.createElement('span', props) } },
  '../../stores': {}, '../../ipc/island-state': {}, '../../ipc/notifications': {}, '../../ipc/client': { terminalCopy: async () => {} }, '../geometry': {}, './notification-cards.css': {}, './notification-hover': {},
};
vm.runInThisContext(`(function(require,module,exports){${code}\n})`, { filename: file.pathname })(name => mocks[name] ?? nativeRequire(name), module, module.exports);
const { NotificationCard } = module.exports;
const note = { rev: 1, id: 'd-1', source: 'system', appName: 'Mail', title: 'A message', body: '<b>not executable</b>', iconPng: null, urgency: 'normal', receivedAt: 1, actions: [{ key: 'default', label: 'Open' }, { key: 'archive', label: 'Archive' }, { key: 'inline-reply', label: 'Reply' }], canInvokeActions: true, progress: null };
const card = (notification = note, extra = {}) => renderToStaticMarkup(React.createElement(NotificationCard, { popup: { notification, remainingMs: notification.urgency === 'critical' ? null : 5000, ttlMs: 6000, paused: false }, perform() {}, ...extra }));
test('normal card escapes text, offers real actions and an honest app-reply fallback', () => {
  const html = card();
  assert.match(html, /notification-timer/); assert.match(html, /Archive/);
  assert.match(html, /Reply in app/); assert.doesNotMatch(html, /<input|<textarea/);
  assert.match(html, /&lt;b&gt;not executable&lt;\/b&gt;/); assert.doesNotMatch(html, /<b>/);
  assert.match(html, /data-island-extension="notification"/); assert.match(html, /Dismiss A message/);
});
test('a picked colour is a colour card: swatch, hex, rgb, copy, no timer and no "open" button', () => {
  const picked = { ...note, id: 'c-1', source: 'alert', appName: 'Colour picker', title: 'Copied #12AB34', body: 'The colour is on your clipboard.', actions: [], canInvokeActions: false };
  const html = card(picked, { popup: { notification: picked, remainingMs: null, ttlMs: 6000, paused: false } });
  assert.match(html, /colour-card/); assert.match(html, /data-colour="#12ab34"/);
  assert.match(html, /#12AB34/); assert.match(html, /rgb\(18, 171, 52\)/);
  assert.match(html, /Copy hex/); assert.match(html, /Copy rgb/);
  assert.doesNotMatch(html, /notification-timer/); assert.doesNotMatch(html, /notification-content/);
  // Only our own alert counts; a system notification with the same name stays a plain card.
  assert.doesNotMatch(card({ ...picked, source: 'system' }), /colour-card/);
  assert.doesNotMatch(card({ ...picked, title: "Couldn't pick a colour" }), /colour-card/);
});
test('cards drawn inside the notch are part of the surface: no separate input region', () => {
  const html = card(note, { inNotch: true });
  assert.doesNotMatch(html, /data-island-extension/); assert.match(html, /data-in-notch="true"/);
});
test('the popup stack is no longer a separate floating window', () => {
  const src = readFileSync(file, 'utf8'), css = readFileSync(new URL('../ui/island/notifications/notification-cards.css', import.meta.url), 'utf8');
  assert.doesNotMatch(src, /notification-stack/); assert.doesNotMatch(css, /\.notification-stack/);
  assert.match(src, /export function NotificationCards[\s\S]*return null;/);
  assert.match(src, /<NotificationCard popup=\{p\} redact=\{settings\.redactBodies\} perform=\{perform\} inNotch mute=\{mute\} \/>/);
});
test('critical cards have no timer; redaction never exposes the body; expired actions disappear', () => {
  const html = card({ ...note, urgency: 'critical', canInvokeActions: false }, { redact: true });
  assert.match(html, /data-critical="true"/); assert.doesNotMatch(html, /notification-timer|not executable|Archive|Reply in app/);
});
test('collapsed geometry includes only notification extensions, not other hidden surfaces or the stack gap', () => {
  const { geometryReport } = load('ui/island/geometry.ts');
  const popup = { dataset: { islandExtension: 'notification' }, getClientRects: () => [{}], getBoundingClientRect: () => ({ x: 400, y: 50, width: 390, height: 150 }) };
  const picker = { dataset: { islandExtension: 'color-picker' }, getClientRects: () => [{}] };
  const outer = { ownerDocument: { querySelectorAll: selector => selector === '[data-island-extension]' ? [popup, picker] : [] }, getBoundingClientRect: () => ({ x: 500, y: 10, width: 144, height: 26 }) };
  globalThis.getComputedStyle = () => ({ visibility: 'visible', borderRadius: '18px' });
  const report = geometryReport(outer, { rev: 1, mode: 'still' }, true, 1);
  assert.equal(report.rects.length, 2); assert.deepEqual(report.rects[1], { x: 400, y: 50, width: 390, height: 150, radius: 18 });
});
test('a settled still island under the notch re-reports when a card appears or disappears', async () => {
  const { createGeometryReporter } = load('ui/island/geometry.ts');
  const queue = [], sent = [], tick = () => new Promise(resolve => setImmediate(resolve));
  const cards = [];
  globalThis.innerWidth = 1180; globalThis.innerHeight = 820; globalThis.devicePixelRatio = 1;
  globalThis.window = { addEventListener() {}, removeEventListener() {}, dispatchEvent() {} };
  globalThis.document = { documentElement: { dataset: {} } };
  globalThis.ResizeObserver = class { observe() {} disconnect() {} };
  globalThis.requestAnimationFrame = cb => { queue.push(cb); return queue.length; }; globalThis.cancelAnimationFrame = () => {};
  const px = { '--native-pill-x': '500px', '--native-pill-y': '4px', '--native-pill-width': '144px', '--native-pill-height': '26px', '--native-pill-radius': '13px' };
  globalThis.getComputedStyle = () => ({ visibility: 'visible', borderRadius: '13px', transform: 'none', getPropertyValue: key => px[key] ?? '' });
  const outer = { style: { setProperty() {} }, ownerDocument: { documentElement: { dataset: {} }, querySelectorAll: () => cards }, getBoundingClientRect: () => ({ x: 500, y: 4, width: 144, height: 26 }) };
  const card = { dataset: { islandExtension: 'notification' }, getClientRects: () => [{}], getBoundingClientRect: () => ({ x: 400, y: 50, width: 390, height: 150 }) };
  const state = { rev: 3, mode: 'still' };
  const owner = createGeometryReporter(outer, () => state, () => ({ x: 0, y: 0 }), async r => sent.push(r), assert.fail);
  const flush = async () => { while (queue.length) queue.shift()(); await tick(); };
  // Let the post-close blur hold elapse so the report is the final settled one.
  await new Promise(resolve => setTimeout(resolve, 120));
  owner.schedule(true); await flush();
  const base = sent.length;
  cards.push(card); owner.schedule(true); await flush();
  assert.equal(sent.length, base + 1, 'a new card must reach Rust even though rev/mode did not change');
  assert.equal(sent.at(-1).rects.length, 2);
  cards.length = 0; owner.schedule(true); await flush();
  assert.equal(sent.length, base + 2, 'a removed card must release its input region');
  assert.equal(sent.at(-1).rects.length, 1);
  owner.dispose();
});
