// Pure notification bookkeeping: no React/Tauri runtime, exercised by node --test.
import type { IslandNotification, NotificationClosed, NotificationSnapshot } from "./client";

export type NotificationDelta = { received: IslandNotification } | { closed: NotificationClosed };
export const NOTIFICATION_CAPACITY = 200;
export const BOOTSTRAP_BUFFER_CAPACITY = 128;
export const TOMBSTONE_CAPACITY = 256;

/** Strict wrapping-u32 ordering (equal and half-range-ambiguous revisions are stale). */
export function notificationRevNewer(rev: number, previous: number): boolean {
  const distance = (rev - previous) >>> 0;
  return distance > 0 && distance < 0x80000000;
}

export function createNotificationStore() {
  const items = new Map<string, IslandNotification>();
  const tombstones = new Map<string, number>();
  const listeners = new Set<() => void>();
  let snapshot: readonly IslandNotification[] = [];
  let buffered: NotificationDelta[] | null = [];
  let droppedBootstrap = 0;
  let authoritativeThrough: number | null = null;
  let pendingRepair: NotificationSnapshot | null = null;
  // Revisions are global to the backend store. Remember a constant-space rejection floor
  // when forgetting a tombstone, buffered event or evicted row. Merely deleting tombstones
  // at capacity would let a late Received resurrect a closed/denied notification.
  // Above the floor, out-of-order updates for distinct ids remain valid. Below it, only
  // updates/removals of still-live rows can apply: an unusually delayed new row can be
  // omitted, never resurrect private content. Privacy changes to live rows must not be lost.
  let retiredThrough: number | null = null;
  const retire = (rev: number) => {
    if (retiredThrough === null || notificationRevNewer(rev, retiredThrough)) retiredThrough = rev;
  };
  const publish = () => {
    snapshot = [...items.values()].sort((a, b) => b.receivedAt - a.receivedAt);
    for (const listener of [...listeners]) listener();
  };
  const merge = (delta: NotificationDelta): boolean => {
    const value = "received" in delta ? delta.received : delta.closed;
    if (authoritativeThrough !== null && !notificationRevNewer(value.rev, authoritativeThrough)) return false;
    const active = items.get(value.id);
    const retired = retiredThrough !== null && !notificationRevNewer(value.rev, retiredThrough);
    if (retired && !active) return false;
    const previous = active?.rev ?? tombstones.get(value.id);
    if (previous !== undefined && !notificationRevNewer(value.rev, previous)) return false;
    tombstones.delete(value.id);
    if ("received" in delta) {
      items.set(value.id, delta.received);
      if (items.size > NOTIFICATION_CAPACITY) {
        // Bound even if backend close deltas were dropped. Use display age, not arrival order.
        let oldest: IslandNotification | undefined;
        for (const item of items.values()) {
          if (!oldest || item.receivedAt < oldest.receivedAt) oldest = item;
        }
        if (oldest) {
          items.delete(oldest.id);
          retire(oldest.rev);
        }
      }
    } else {
      items.delete(value.id);
      // A revision already covered by the floor needs no separate tombstone.
      if (!retired) tombstones.set(value.id, value.rev);
      if (tombstones.size > TOMBSTONE_CAPACITY) {
        const first = tombstones.entries().next().value;
        if (first) {
          tombstones.delete(first[0]);
          retire(first[1]);
        }
      }
    }
    return true;
  };
  const reconcile = (repair: NotificationSnapshot): boolean => {
    if (buffered !== null) {
      if (!pendingRepair || notificationRevNewer(repair.rev, pendingRepair.rev)) {
        pendingRepair = { rev: repair.rev, items: repair.items.slice(0, NOTIFICATION_CAPACITY) };
      }
      return false;
    }
    if (authoritativeThrough !== null && !notificationRevNewer(repair.rev, authoritativeThrough)) return false;
    if (retiredThrough !== null && notificationRevNewer(retiredThrough, repair.rev)) return false;
    // Preserve events newer than the snapshot boundary, including tombstones for
    // rows absent from it. All older bookkeeping is subsumed by one global floor.
    for (const [id, item] of items) if (!notificationRevNewer(item.rev, repair.rev)) items.delete(id);
    for (const [id, rev] of tombstones) if (!notificationRevNewer(rev, repair.rev)) tombstones.delete(id);
    for (const item of repair.items.slice(0, NOTIFICATION_CAPACITY)) {
      if (!items.has(item.id) && !tombstones.has(item.id)) items.set(item.id, item);
    }
    authoritativeThrough = repair.rev;
    retire(repair.rev);
    while (items.size > NOTIFICATION_CAPACITY) {
      const oldest = [...items.values()].sort((a, b) => a.receivedAt - b.receivedAt)[0];
      if (oldest) items.delete(oldest.id);
    }
    publish();
    return true;
  };
  return {
    reconcile,
    apply(delta: NotificationDelta): boolean {
      if (buffered !== null) {
        if (buffered.length === BOOTSTRAP_BUFFER_CAPACITY) {
          const dropped = buffered.shift();
          if (dropped) retire(("received" in dropped ? dropped.received : dropped.closed).rev);
          droppedBootstrap++;
        }
        buffered.push(delta);
        return false;
      }
      const changed = merge(delta);
      if (changed) publish();
      return changed;
    },
    /** Called once after subscribing, before mounting. Repeated/stale seeds cannot resurrect. */
    seed(initial: readonly IslandNotification[]): void {
      if (buffered === null) return;
      // Filter against the overflow floor BEFORE replaying: a dropped close must not be
      // undone by an old bootstrap row. Loss is counted and intentionally privacy-safe.
      for (const received of initial) merge({ received });
      const pending = buffered;
      buffered = null;
      if (pendingRepair) { reconcile(pendingRepair); pendingRepair = null; }
      for (const delta of pending) merge(delta);
      publish();
    },
    getSnapshot: () => snapshot,
    subscribe(listener: () => void): () => void {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    diagnostics: () => ({
      buffered: buffered?.length ?? 0,
      items: items.size,
      tombstones: tombstones.size,
      retiredThrough,
      droppedBootstrap,
      authoritativeThrough,
    }),
  };
}
