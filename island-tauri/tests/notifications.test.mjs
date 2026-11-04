import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  createNotificationStore, notificationRevNewer,
  NOTIFICATION_CAPACITY, BOOTSTRAP_BUFFER_CAPACITY, TOMBSTONE_CAPACITY,
} from '../ui/ipc/notifications-core.ts';

const notification = (id, rev, extra = {}) => ({
  id, rev, source: 'hermes', appName: 'Fixture', title: id, body: 'private fixture',
  iconPng: null, urgency: 'normal', receivedAt: rev, actions: [], canInvokeActions: false,
  progress: null, ...extra,
});
const receive = (store, id, rev, extra) => store.apply({ received: notification(id, rev, extra) });
const close = (store, id, rev) => store.apply({ closed: { id, rev, reason: 'dismissed' } });
const ids = (store) => store.getSnapshot().map((n) => n.id);
const seeded = () => { const s = createNotificationStore(); s.seed([]); return s; };

test('subscribed events wait for bootstrap and replay once with no stale intermediate publication', () => {
  const store = createNotificationStore();
  let publications = 0;
  store.subscribe(() => { publications++; });
  receive(store, 'updated', 5, { body: 'Hidden', receivedAt: 50 });
  close(store, 'removed', 6);
  assert.deepEqual(store.getSnapshot(), []);
  assert.equal(publications, 0);
  store.seed([notification('updated', 3), notification('removed', 4)]);
  assert.deepEqual(ids(store), ['updated']);
  assert.equal(store.getSnapshot()[0].body, 'Hidden');
  assert.equal(publications, 1);
  assert.equal(store.diagnostics().buffered, 0);
  assert.equal(store.diagnostics().tombstones, 1);
  store.seed([notification('removed', 4)]);
  assert.deepEqual(ids(store), ['updated'], 'repeated bootstrap never resurrects a removed row');
  assert.equal(publications, 1);
});

test('a newer bootstrap row wins over a stale buffered event for the same id', () => {
  const store = createNotificationStore();
  receive(store, 'a', 2, { body: 'old' });
  close(store, 'a', 3);
  store.seed([notification('a', 4, { body: 'current' })]);
  assert.equal(store.getSnapshot()[0].body, 'current');
});

test('equal and stale receives or closes do not replace rows or notify subscribers', () => {
  const store = seeded();
  let calls = 0;
  const off = store.subscribe(() => { calls++; });
  assert.equal(receive(store, 'a', 10), true);
  const stable = store.getSnapshot();
  assert.equal(receive(store, 'a', 10, { body: 'stale' }), false);
  assert.equal(receive(store, 'a', 9), false);
  assert.equal(close(store, 'a', 10), false);
  assert.equal(close(store, 'a', 8), false);
  assert.equal(store.getSnapshot(), stable, 'useSyncExternalStore snapshot reference is stable');
  assert.equal(calls, 1);
  off(); receive(store, 'a', 11);
  assert.equal(calls, 1);
});

test('out-of-order distinct ids are accepted; a global latest-rev guard would lose valid rows', () => {
  const store = seeded();
  receive(store, 'newer', 20);
  receive(store, 'older', 10);
  assert.deepEqual(ids(store), ['newer', 'older']);
  close(store, 'older', 11);
  assert.deepEqual(ids(store), ['newer']);
});

test('wrapping u32 guard accepts wrap and rejects equal, stale and half-range ambiguous revisions', () => {
  assert.equal(notificationRevNewer(0, 0xffffffff), true);
  assert.equal(notificationRevNewer(0xffffffff, 0), false);
  assert.equal(notificationRevNewer(42, 42), false);
  assert.equal(notificationRevNewer(0x80000000, 0), false);
  const store = seeded();
  receive(store, 'a', 0xfffffffe);
  close(store, 'a', 0xffffffff);
  assert.equal(receive(store, 'a', 0xfffffffe), false);
  assert.equal(receive(store, 'a', 0), true);
  assert.equal(close(store, 'a', 0xffffffff), false);
  assert.deepEqual(ids(store), ['a']);
});

test('privacy close tombstones block older/equal receives, but permit a genuinely newer row', () => {
  const store = seeded();
  receive(store, 'private', 1);
  close(store, 'private', 3);
  assert.equal(receive(store, 'private', 2), false);
  assert.equal(receive(store, 'private', 3), false);
  assert.deepEqual(ids(store), []);
  assert.equal(receive(store, 'private', 4, { body: 'Hidden' }), true);
  assert.equal(store.diagnostics().tombstones, 0, 'reopened ids do not retain redundant tombstones');
  assert.equal(store.getSnapshot()[0].body, 'Hidden');
});

test('redaction replacements cannot be rolled back by a late event or repeated seed', () => {
  const store = seeded();
  receive(store, 'private', 1);
  receive(store, 'private', 2, { body: 'Hidden' });
  receive(store, 'private', 1);
  store.seed([notification('private', 1)]);
  assert.equal(store.getSnapshot()[0].body, 'Hidden');
});

test('bootstrap buffer has an exact hard cap and accounts for every dropped delta', () => {
  const store = createNotificationStore();
  for (let i = 0; i < BOOTSTRAP_BUFFER_CAPACITY; i++) receive(store, `b-${i}`, i + 1);
  assert.equal(store.diagnostics().buffered, BOOTSTRAP_BUFFER_CAPACITY);
  assert.equal(store.diagnostics().droppedBootstrap, 0);
  receive(store, 'overflow', BOOTSTRAP_BUFFER_CAPACITY + 1);
  assert.equal(store.diagnostics().buffered, BOOTSTRAP_BUFFER_CAPACITY);
  assert.equal(store.diagnostics().droppedBootstrap, 1);
  assert.equal(store.diagnostics().retiredThrough, 1);
  store.seed([]);
  assert.equal(store.diagnostics().buffered, 0);
  assert.equal(store.getSnapshot().length, BOOTSTRAP_BUFFER_CAPACITY);
  assert.ok(!ids(store).includes('b-0'));
  assert.ok(ids(store).includes('overflow'));
});

test('a close lost to bootstrap overflow cannot be resurrected by the stale snapshot', () => {
  const store = createNotificationStore();
  close(store, 'secret', 2);
  for (let i = 0; i < 10000; i++) receive(store, `flood-${i}`, i + 3);
  const stats = store.diagnostics();
  assert.equal(stats.buffered, BOOTSTRAP_BUFFER_CAPACITY);
  assert.equal(stats.droppedBootstrap, 10001 - BOOTSTRAP_BUFFER_CAPACITY);
  store.seed([notification('secret', 1)]);
  assert.ok(!ids(store).includes('secret'));
  assert.equal(store.getSnapshot().length, BOOTSTRAP_BUFFER_CAPACITY);
  assert.equal(receive(store, 'secret', 1), false);
  assert.equal(store.diagnostics().buffered, 0);
});

test('a dropped redaction event also prevents old bootstrap content from appearing', () => {
  const store = createNotificationStore();
  receive(store, 'secret', 2, { body: 'Hidden' });
  for (let i = 0; i < BOOTSTRAP_BUFFER_CAPACITY; i++) close(store, `other-${i}`, i + 3);
  store.seed([notification('secret', 1)]);
  assert.ok(!ids(store).includes('secret'), 'fail closed rather than display an unredacted stale row');
});

test('tombstones stay bounded through churn and evicted tombstones still reject stale resurrection', () => {
  const store = seeded();
  receive(store, 'secret', 1);
  close(store, 'secret', 2);
  for (let i = 0; i < TOMBSTONE_CAPACITY - 1; i++) close(store, `c-${i}`, i + 3);
  assert.equal(store.diagnostics().tombstones, TOMBSTONE_CAPACITY);
  assert.equal(store.diagnostics().retiredThrough, null);
  close(store, 'evicts-secret', TOMBSTONE_CAPACITY + 2);
  assert.equal(store.diagnostics().tombstones, TOMBSTONE_CAPACITY);
  assert.equal(store.diagnostics().retiredThrough, 2);
  assert.equal(receive(store, 'secret', 1), false);
  assert.equal(receive(store, 'secret', 2), false);
  for (let i = 0; i < 10000; i++) close(store, `churn-${i}`, i + TOMBSTONE_CAPACITY + 3);
  assert.equal(store.diagnostics().tombstones, TOMBSTONE_CAPACITY);
  assert.equal(store.diagnostics().items, 0);
  assert.equal(receive(store, 'secret', 1), false);
  assert.equal(receive(store, 'fresh', 20000), true);
});

test('retirement floor and tombstone eviction remain correct across u32 wrap', () => {
  const store = seeded();
  const start = 0xfffffff0;
  receive(store, 'secret', start);
  close(store, 'secret', (start + 1) >>> 0);
  for (let i = 0; i < TOMBSTONE_CAPACITY + 40; i++) close(store, `wrap-${i}`, (start + 2 + i) >>> 0);
  const last = (start + 1 + TOMBSTONE_CAPACITY + 40) >>> 0;
  assert.equal(store.diagnostics().tombstones, TOMBSTONE_CAPACITY);
  assert.equal(receive(store, 'secret', start), false);
  assert.equal(receive(store, 'fresh', (last + 1) >>> 0), true);
});

test('active renderer rows are bounded even when backend capacity closes are lost', () => {
  const store = seeded();
  for (let i = 0; i < 1000; i++) {
    receive(store, `entry-${i}`, i + 1);
    assert.ok(store.diagnostics().items <= NOTIFICATION_CAPACITY);
  }
  assert.equal(store.getSnapshot().length, NOTIFICATION_CAPACITY);
  assert.equal(store.diagnostics().tombstones, 0);
  assert.equal(receive(store, 'entry-0', 1), false, 'an evicted active row cannot reappear stale');
  assert.equal(store.getSnapshot()[0].id, 'entry-999');
});

test('oversized bootstrap cannot bypass the active row bound', () => {
  const store = createNotificationStore();
  store.seed(Array.from({ length: 1000 }, (_, i) => notification(`initial-${i}`, i + 1)));
  assert.equal(store.diagnostics().items, NOTIFICATION_CAPACITY);
  assert.equal(store.getSnapshot().length, NOTIFICATION_CAPACITY);
  assert.ok(!ids(store).includes('initial-0'));
});

test('updating an existing tombstone refreshes it rather than growing the bookkeeping', () => {
  const store = seeded();
  close(store, 'a', 1);
  close(store, 'b', 2);
  close(store, 'a', 3);
  assert.equal(store.diagnostics().tombstones, 2);
  assert.equal(receive(store, 'a', 2), false);
});

test('retirement never loses delayed privacy updates or removals for still-live rows', () => {
  const store = seeded();
  receive(store, 'secret', 1);
  for (let i = 0; i < TOMBSTONE_CAPACITY + 20; i++) close(store, `other-${i}`, i + 100);
  assert.ok(store.diagnostics().retiredThrough > 3);
  assert.equal(receive(store, 'secret', 2, { body: 'Hidden' }), true);
  assert.equal(store.getSnapshot()[0].body, 'Hidden', 'redaction below the floor must still scrub a live row');
  assert.equal(close(store, 'secret', 3), true, 'a delayed privacy close must still remove a live row');
  assert.deepEqual(ids(store), []);
  assert.equal(receive(store, 'secret', 2), false, 'floor protects the removal without an extra tombstone');
});

test('notification listener is registered before bootstrap, with closes before receives', () => {
  const adapter = readFileSync(new URL('../ui/ipc/notifications.ts', import.meta.url), 'utf8');
  const shim = readFileSync(new URL('../ui/ipc/legacy-shim.ts', import.meta.url), 'utf8');
  assert.ok(adapter.indexOf('await onNotificationClosed(') < adapter.indexOf('await onNotificationReceived('));
  assert.ok(shim.indexOf('await listenNotifications()') < shim.indexOf('await bootstrap()'));
  assert.match(adapter, /createNotificationStore\(\)/, 'product adapter must use the tested bounded core');
});

test('authoritative overflow repair scrubs all 200 rows despite 72 lost privacy deltas', () => {
  const store = seeded();
  for (let i = 0; i < 200; i++) receive(store, `n-${i}`, i + 1);
  const scrubbed = Array.from({length: 200}, (_, i) => notification(`n-${i}`, i + 201, {body: 'Hidden'}));
  for (const row of scrubbed.slice(72)) store.apply({received: row});
  assert.equal(store.getSnapshot().filter(n => n.body !== 'Hidden').length, 72);
  store.reconcile({rev: 400, items: scrubbed});
  assert.equal(store.getSnapshot().length, 200);
  assert.ok(store.getSnapshot().every(n => n.body === 'Hidden'));
  assert.equal(receive(store, 'n-0', 201), false, 'late private deltas cannot roll back a repair');
  store.reconcile({rev: 600, items: []});
  assert.deepEqual(ids(store), [], 'a deny/clear overflow repairs absent entries too');
  assert.equal(receive(store, 'n-0', 401), false);
});

test('repair is bounded before bootstrap and preserves newer receives and closes', () => {
  const store = createNotificationStore();
  store.reconcile({rev: 100, items: Array.from({length: 1000}, (_, i) => notification(`r-${i}`, 50))});
  store.reconcile({rev: 99, items: [notification('stale', 40)]});
  receive(store, 'newer', 102);
  close(store, 'r-1', 103);
  store.seed([notification('old-private', 1)]);
  assert.ok(!ids(store).includes('old-private') && !ids(store).includes('stale') && !ids(store).includes('r-1'));
  assert.ok(ids(store).includes('newer'));
  assert.ok(store.getSnapshot().length <= 200);
  store.reconcile({rev: 101, items: [notification('r-1', 50)]});
  assert.ok(!ids(store).includes('r-1'), 'a future close dominates an older repair');
  assert.ok(ids(store).includes('newer'));
});

test('a repair older than forgotten tombstones cannot resurrect private rows', () => {
  const store = seeded();
  for (let i = 0; i < 1000; i++) close(store, `n-${i}`, i + 100);
  assert.equal(store.reconcile({rev: 10, items: [notification('n-0', 1)]}), false);
  assert.deepEqual(ids(store), []);
  assert.equal(store.reconcile({rev: 0xfffffff0, items: []}), false);
});
