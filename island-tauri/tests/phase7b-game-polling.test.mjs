import test from 'node:test';
import assert from 'node:assert/strict';
import { load } from './phase7b-loader.mjs';
const { gamePollingActive } = load('ui/views/game-polling-policy.ts');
const { createVisiblePoller } = load('ui/views/visible-poller.ts');
const drain = async () => { await Promise.resolve(); await Promise.resolve(); };

test('game polling respects active view, effective game state and checkbox independently', () => {
  for (const active of [false, true]) for (const game of [false, true]) for (const setting of [false, true]) {
    assert.equal(gamePollingActive(active, game, setting), active && (!game || !setting));
  }
});
test('game pause cancels timer and resume refreshes now without waiting a cadence', async () => {
  let reads = 0, timer = null;
  const accepted = [];
  const poller = createVisiblePoller(async () => ++reads, value => accepted.push(value), assert.fail,
    callback => { timer = callback; return callback; }, () => { timer = null; });
  poller.setVisible(gamePollingActive(true, false, true));
  await drain();
  assert.equal(reads, 1);
  assert.ok(timer);
  poller.setVisible(gamePollingActive(true, true, true));
  assert.equal(timer, null);
  await drain();
  assert.equal(reads, 1);
  poller.setVisible(gamePollingActive(true, false, true));
  await drain();
  assert.equal(reads, 2);
  assert.deepEqual(accepted, [1, 2]);
  poller.dispose();
});
test('resume during an in-flight read discards stale result and refreshes immediately once drained', async () => {
  let resolve, reads = 0, scheduled = 0;
  const accepted = [];
  const poller = createVisiblePoller(() => { reads++; return new Promise(done => { resolve = done; }); },
    value => accepted.push(value), assert.fail, () => { scheduled++; });
  poller.setVisible(true);
  poller.setVisible(false);
  poller.setVisible(true);
  assert.equal(reads, 1, 'never overlap requests');
  resolve('stale');
  await drain();
  assert.deepEqual(accepted, []);
  assert.equal(reads, 2, 'resume refresh is not delayed by the old cadence');
  assert.equal(scheduled, 0);
  resolve('fresh');
  await drain();
  assert.deepEqual(accepted, ['fresh']);
  poller.dispose();
});
