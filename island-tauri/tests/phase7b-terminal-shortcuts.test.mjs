import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load } from './phase7b-loader.mjs';

test('paste never reaches the PTY key encoder and preserves native clipboard events', () => {
  const { terminalShortcut } = load('ui/views/terminal-shortcuts.ts');
  const sent = [];
  for (const key of ['v']) for (const type of ['keydown', 'keypress', 'keyup']) {
    const event = { key, type, ctrlKey: true, shiftKey: false, altKey: false, metaKey: false,
      preventDefault() { assert.fail('Native clipboard operation must remain enabled'); } };
    assert.equal(terminalShortcut(event, data => sent.push(data)), false);
  }
  assert.deepEqual(sent, []);
});

test('copy explicitly writes the xterm selection even when WebKit emits no copy event', async () => {
  const { terminalShortcut } = load('ui/views/terminal-shortcuts.ts');
  const copied = [], sent = [], errors = [];
  let prevented = 0;
  const clipboard = { selection: () => 'selected terminal text', copy: async text => copied.push(text), fail: error => errors.push(error) };
  for (const [type, repeat] of [['keydown', false], ['keydown', true], ['keypress', false], ['keyup', false]]) {
    assert.equal(terminalShortcut({ key: 'c', type, repeat, ctrlKey: true, shiftKey: false,
      altKey: false, metaKey: false, preventDefault() { prevented++; } }, data => sent.push(data), clipboard), false);
  }
  await Promise.resolve();
  assert.deepEqual(copied, ['selected terminal text']);
  assert.deepEqual(sent, []);
  assert.deepEqual(errors, []);
  assert.equal(prevented, 4);
});

test('empty copy preserves the clipboard and a failed write is surfaced', async () => {
  const { terminalShortcut } = load('ui/views/terminal-shortcuts.ts');
  const event = { key: 'c', type: 'keydown', ctrlKey: true, preventDefault() {} };
  const errors = [], failure = new Error('clipboard unavailable');
  terminalShortcut(event, () => assert.fail(), { selection: () => '', copy: () => assert.fail('empty copy must not clear clipboard'), fail: error => errors.push(error) });
  terminalShortcut(event, () => assert.fail(), { selection: () => 'text', copy: async () => { throw failure; }, fail: error => errors.push(error) });
  await Promise.resolve();
  assert.deepEqual(errors, [failure]);
});

test('Ctrl+Shift+C interrupts once on keydown, not keyup or auto-repeat', () => {
  const { terminalShortcut } = load('ui/views/terminal-shortcuts.ts');
  const sent = [];
  let prevented = 0;
  for (const [type, repeat] of [['keydown', false], ['keydown', true], ['keypress', false], ['keyup', false]]) {
    assert.equal(terminalShortcut({ key: 'C', type, repeat, ctrlKey: true, shiftKey: true,
      altKey: false, metaKey: false, preventDefault() { prevented++; } }, data => sent.push(data)), false);
  }
  assert.deepEqual(sent, ['\x03']);
  assert.equal(prevented, 4);
  assert.equal(terminalShortcut({ key: 'c', type: 'keydown', ctrlKey: false }, () => assert.fail()), true);
  assert.equal(terminalShortcut({ key: 'c', ctrlKey: true, altKey: true }, () => assert.fail()), true);
});

test('OSC 52 writes from the TUI reach the clipboard; queries and junk are ignored', async () => {
  const { keepTerminalClipboard, decodeOsc52 } = load('ui/views/terminal-shortcuts.ts');
  let handler;
  const copied = [], errors = [];
  const disposable = keepTerminalClipboard({ parser: { registerOscHandler(code, fn) { assert.equal(code, 52); handler = fn; return { dispose() {} }; } } },
    async text => copied.push(text), error => errors.push(error));
  const b64 = Buffer.from('héllo TUI\nline 2', 'utf8').toString('base64');
  assert.equal(handler(`c;${b64}`), true);
  assert.equal(handler('c;?'), true);
  assert.equal(handler('c;'), true);
  assert.equal(handler('c;not*base64'), true);
  assert.equal(handler(`;${Buffer.from('empty target').toString('base64')}`), true);
  await Promise.resolve();
  assert.deepEqual(copied, ['héllo TUI\nline 2', 'empty target']);
  assert.deepEqual(errors, []);
  assert.equal(decodeOsc52('x;' + b64), null);
  assert.equal(typeof disposable.dispose, 'function');
});

test('Ctrl+C hands a TUI mouse selection to the TUI once, otherwise sends nothing', () => {
  const { terminalShortcut } = load('ui/views/terminal-shortcuts.ts');
  const sent = [];
  let armed = true;
  const clipboard = { selection: () => '', copy: () => assert.fail('xterm selection is empty'), fail: assert.fail,
    appSelection: () => armed, appCopied: () => { armed = false; } };
  const ctrlC = { key: 'c', type: 'keydown', ctrlKey: true, shiftKey: false, preventDefault() {} };
  assert.equal(terminalShortcut(ctrlC, data => sent.push(data), clipboard), false);
  assert.equal(terminalShortcut(ctrlC, data => sent.push(data), clipboard), false);
  assert.deepEqual(sent, ['\x03']);
});

test('drag selection tracking only arms in mouse-tracking mode and clears on keys and clicks', () => {
  const { trackAppSelection } = load('ui/views/terminal-shortcuts.ts');
  const listeners = {}, globalListeners = {};
  const element = { addEventListener(type, fn) { listeners[type] = fn; }, removeEventListener(type) { delete listeners[type]; } };
  const saved = globalThis.window;
  globalThis.window = { addEventListener(type, fn) { globalListeners[type] = fn; }, removeEventListener(type) { delete globalListeners[type]; } };
  try {
    const terminal = { modes: { mouseTrackingMode: 'none' } };
    const tracker = trackAppSelection(terminal, element);
    const drag = () => { listeners.mousedown({ button: 0, shiftKey: false, clientX: 0, clientY: 0 }); globalListeners.mouseup({ clientX: 40, clientY: 0 }); };
    drag(); assert.equal(tracker.active(), false);
    terminal.modes.mouseTrackingMode = 'any';
    drag(); assert.equal(tracker.active(), true);
    listeners.keydown({ key: 'Control', ctrlKey: true }); assert.equal(tracker.active(), true);
    listeners.keydown({ key: 'a', ctrlKey: false }); assert.equal(tracker.active(), false);
    drag(); listeners.mousedown({ button: 0, clientX: 5, clientY: 5 }); globalListeners.mouseup({ clientX: 5, clientY: 5 });
    assert.equal(tracker.active(), false);
    listeners.dblclick(); assert.equal(tracker.active(), true);
    tracker.clear(); assert.equal(tracker.active(), false);
    tracker.dispose(); assert.deepEqual(Object.keys(listeners), []);
  } finally { globalThis.window = saved; }
});
