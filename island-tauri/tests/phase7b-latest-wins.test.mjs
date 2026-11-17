import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load } from './phase7b-loader.mjs';
const { createLatestWins } = load('ui/views/app-lifecycle.ts');

const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };

test('latest-wins lane: one command in flight per key, newer values replace the waiting one', async () => {
  const latest = createLatestWins(), queue = [], sent = [], gates = [];
  const enqueue = work => queue.push(work);
  const send = (key, v) => latest(key, enqueue, () => { sent.push(`${key}=${v}`); const g = deferred(); gates.push(g); return g.promise; });
  send('a', 1); send('a', 2); send('a', 3); send('a', 4); send('b', 9);
  assert.equal(queue.length, 2, 'only the first of each key is queued');
  const first = queue.shift()(); assert.deepEqual(sent, ['a=1']);
  gates.shift().resolve(); await first;
  assert.equal(queue.length, 2, 'finishing a=1 queues only the newest a');
  const b = queue.shift()(); gates.shift().resolve(); await b;
  const a = queue.shift()(); gates.shift().resolve(); await a;
  assert.deepEqual(sent, ['a=1', 'b=9', 'a=4'], 'drag intermediates 2 and 3 are dropped, the final value is kept');
  assert.equal(queue.length, 0);
  send('a', 5); assert.equal(queue.length, 1, 'an idle lane sends immediately again');
});

test('latest-wins lane keeps draining after a failed command', async () => {
  const latest = createLatestWins(), queue = [], sent = [];
  latest('k', w => queue.push(w), () => { sent.push(1); return Promise.reject(new Error('x')); });
  latest('k', w => queue.push(w), () => { sent.push(2); return Promise.resolve(); });
  await queue.shift()().catch(() => {});
  await queue.shift()();
  assert.deepEqual(sent, [1, 2]);
});
