import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createIslandStore, revBefore } from '../ui/ipc/island-state-core.ts';

const state = (rev, extra = {}) => ({
  rev, mode: 'still', view: 'terminal', hoverCause: null, dnd: false, unread: 0, terminalStarted: false, ...extra,
});
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };

test('unseeded store fails closed instead of rendering a guessed state', () => {
  const store = createIslandStore(async () => {});
  assert.throws(() => store.requireSnapshot(), /not seeded/);
});

test('seed from bootstrap, then only IslandStateChanged advances it', async () => {
  const sent = [];
  const store = createIslandStore(async (intent) => { sent.push(intent); return state(99, { mode: 'large' }); });
  store.seed(state(4));
  assert.equal(store.requireSnapshot().rev, 4);
  let notified = 0;
  store.subscribe(() => { notified++; });
  // A dispatch reply (even a newer state) must NOT move the store: events are the only writer.
  await store.dispatch({ type: 'pin' });
  assert.deepEqual(sent, [{ type: 'pin' }]);
  assert.equal(store.requireSnapshot().rev, 4);
  assert.equal(notified, 0);
  assert.equal(store.apply(state(5, { mode: 'large' })), true);
  assert.equal(store.requireSnapshot().mode, 'large');
  assert.equal(notified, 1);
});

test('rev guard drops older and equal states, including across u32 wrap', () => {
  const store = createIslandStore(async () => {});
  store.seed(state(10));
  assert.equal(store.apply(state(9, { view: 'ai' })), false);
  assert.equal(store.apply(state(10, { view: 'ai' })), false);
  assert.equal(store.requireSnapshot().view, 'terminal');
  assert.equal(store.apply(state(11, { view: 'ai' })), true);
  const wrapping = createIslandStore(async () => {});
  wrapping.seed(state(0xfffffffe));
  assert.equal(wrapping.apply(state(0xffffffff)), true);
  assert.equal(wrapping.apply(state(0, { view: 'monitor' })), true, 'rev 0 is newer than 0xffffffff after wrapping');
  assert.equal(wrapping.apply(state(0xfffffffe)), false);
  assert.equal(wrapping.requireSnapshot().view, 'monitor');
  assert.equal(revBefore(0xffffffff, 0), true);
  assert.equal(revBefore(0, 0xffffffff), false);
});

test('an event that raced the bootstrap wins; a stale seed never regresses it', () => {
  const store = createIslandStore(async () => {});
  store.apply(state(7, { mode: 'hover' }));
  store.seed(state(3));
  assert.equal(store.requireSnapshot().rev, 7);
  assert.equal(store.requireSnapshot().mode, 'hover');
  store.seed(state(8));
  assert.equal(store.requireSnapshot().rev, 8);
});

test('subscribers run once per accepted state, never for a dropped one', () => {
  const store = createIslandStore(async () => {});
  store.seed(state(1));
  let runs = 0;
  const off = store.subscribe(() => { runs++; });
  store.apply(state(1)); store.apply(state(0));
  assert.equal(runs, 0);
  store.apply(state(2));
  assert.equal(runs, 1);
  off(); store.apply(state(3));
  assert.equal(runs, 1);
});

test('intents leave one at a time in call order, even when an earlier one is slow', async () => {
  const gates = []; const started = [];
  const store = createIslandStore((intent) => { started.push(intent.type); const g = deferred(); gates.push(g); return g.promise; });
  const a = store.dispatch({ type: 'pointerEnter' });
  const b = store.dispatch({ type: 'pointerLeave' });
  const c = store.dispatch({ type: 'pin' });
  await new Promise((r) => setImmediate(r));
  assert.deepEqual(started, ['pointerEnter'], 'later intents wait for the earlier reply');
  gates[0].resolve(); await a;
  await new Promise((r) => setImmediate(r));
  assert.deepEqual(started, ['pointerEnter', 'pointerLeave']);
  gates[1].resolve(); await b;
  await new Promise((r) => setImmediate(r));
  gates[2].resolve(); await c;
  assert.deepEqual(started, ['pointerEnter', 'pointerLeave', 'pin']);
});

test('a backend error rejects that dispatch (the renderer routes it to fail) and does not wedge the queue', async () => {
  let n = 0;
  const store = createIslandStore(async () => { if (n++ === 0) throw new Error('unavailable: State actor stopped'); });
  const first = store.dispatch({ type: 'collapse' });
  const second = store.dispatch({ type: 'pin' });
  await assert.rejects(first, /State actor stopped/);
  await second;
  assert.equal(n, 2);
  // the renderer's contract: dispatchIntent(...).catch(fail)
  let surfaced = '';
  n = 0;
  await store.dispatch({ type: 'pin' }).catch((e) => { surfaced = e.message; });
  assert.match(surfaced, /State actor stopped/);
});

test('cues are relayed; a throwing listener does not starve the others', () => {
  const store = createIslandStore(async () => {});
  const seen = [];
  const quiet = console.error; console.error = () => {};
  try {
    store.subscribeCue(() => { throw new Error('boom'); });
    store.subscribeCue((c) => seen.push(c.kind));
    store.cue({ kind: 'click' });
  } finally { console.error = quiet; }
  assert.deepEqual(seen, ['click']);
});

// ---- renderer ownership seam: static guards (the nested-KWin GUI test is the behavioral proof) ----
const renderer = readFileSync(new URL('../ui/legacy/renderer.jsx', import.meta.url), 'utf8');
test('renderer no longer owns mode, view, started, hover timers or the anchor subscription', () => {
  for (const banned of ['useReducer', 'stateMachine', 'hoverTimer', 'leaveTimer', 'cancelHoverTimers', 'setView(', 'setStarted', 'onAnchor', 'api.pin', 'modeRef']) {
    assert.ok(!renderer.includes(banned), `renderer.jsx still contains ${banned}`);
  }
  assert.ok(!/setTimeout\([^)]*dispatch/.test(renderer), 'no renderer-side delayed state change');
});
test('renderer sends the SPEC intents and reports geometry with the store rev', () => {
  for (const wanted of ['{ type: "pointerEnter" }', '{ type: "pointerLeave" }', '{ type: "pin" }', '{ type: "collapse" }', 'type: "selectView"', 'useIslandState()', 'subscribeCue', 'api.layout(rect, mode, devicePixelRatio, stateRev.current)']) {
    assert.ok(renderer.includes(wanted), `renderer.jsx lacks ${wanted}`);
  }
  // every dispatch routes its rejection to the renderer's existing `fail`
  const dispatches = renderer.match(/dispatchIntent\(/g).length;
  const failed = renderer.match(/dispatchIntent\([^;]*\)\.catch\(fail\)/g).length;
  assert.equal(dispatches, failed);
});
