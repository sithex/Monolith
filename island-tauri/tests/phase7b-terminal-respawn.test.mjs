import test from 'node:test';
import assert from 'node:assert/strict';
import { load } from './phase7b-loader.mjs';

const { createAutoRestart } = load('ui/views/terminal-respawn.ts');

function harness(start = 0) {
  let clock = start; const timers = [], notices = [], restarts = [];
  const auto = createAutoRestart({
    now: () => clock, schedule: (fn, ms) => { const t = { fn, ms, live: true }; timers.push(t); return t; },
    cancel: t => { t.live = false; }, notice: text => notices.push(text), fail: assert.fail,
    restart: async () => { restarts.push(clock); },
  });
  const fire = async () => { for (const t of timers.splice(0)) if (t.live) t.fn(); await Promise.resolve(); await Promise.resolve(); };
  return { auto, notices, restarts, timers, fire, tick: ms => { clock += ms; } };
}

test('an exited chat is replaced by a fresh one after a short delay', async () => {
  const h = harness(); h.tick(60_000);
  h.auto.exited(); h.auto.exited();
  assert.equal(h.timers.length, 1, 'duplicate exits schedule one respawn');
  assert.equal(h.restarts.length, 0);
  await h.fire();
  assert.equal(h.restarts.length, 1);
  assert.match(h.notices[0], /new Hermes chat/);
});

test('a chat that keeps dying at launch stops respawning instead of looping', async () => {
  const h = harness();
  for (let i = 0; i < 2; i++) { h.tick(1000); h.auto.exited(); await h.fire(); }
  h.tick(1000); h.auto.exited();
  assert.equal(h.restarts.length, 2);
  assert.equal(h.timers.length, 0);
  assert.match(h.notices.at(-1), /not restarted/);
});

test('a long-lived chat resets the crash counter and dispose cancels a pending respawn', async () => {
  const h = harness();
  h.tick(1000); h.auto.exited(); await h.fire();
  h.tick(1000); h.auto.exited(); await h.fire();
  h.tick(30_000); h.auto.exited(); await h.fire();
  assert.equal(h.restarts.length, 3);
  h.tick(30_000); h.auto.exited(); h.auto.dispose(); await h.fire();
  assert.equal(h.restarts.length, 3);
});

test('flow reset lets a respawned run snapshot replace the old one', () => {
  const { createTerminalFlow } = load('ui/views/terminal-flow.ts');
  const written = []; let resets = 0;
  const flow = createTerminalFlow({ write: (d, cb) => { written.push(d); cb(); }, reset: () => { resets++; } }, async () => {}, assert.fail, () => {});
  flow.snapshot({ seq: 500, data: 'old', running: true });
  flow.event({ type: 'snapshot', seq: 1, data: 'new', running: true });
  assert.deepEqual(written, ['old'], 'stale sequence is ignored without reset');
  flow.reset();
  flow.event({ type: 'snapshot', seq: 1, data: 'new', running: true });
  flow.event({ type: 'data', seq: 2, data: '!' });
  assert.deepEqual(written, ['old', 'new', '!']);
  assert.equal(resets, 2);
});
