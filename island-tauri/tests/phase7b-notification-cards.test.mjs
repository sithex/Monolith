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
  'framer-motion': { motion: { article: ({ onUpdate, ...props }) => React.createElement('article', props) } },
  '../../stores': {}, '../../ipc/island-state': {}, '../../ipc/notifications': {}, '../geometry': {}, './notification-cards.css': {},
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
