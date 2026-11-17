type EpochTarget<T> = { epoch(): number; apply(value: T, epoch: number): boolean };
/** Capture at user intent time, not when a shared command queue eventually drains. */
export function enqueueEpochCommand<T>(store: EpochTarget<T>, enqueue: (work: () => Promise<unknown>) => void, send: () => Promise<T>) {
  const epoch = store.epoch();
  enqueue(async () => {
    if (epoch !== store.epoch()) return;
    const reply = await send();
    store.apply(reply, epoch);
  });
}
/**
 * Latest-wins lane per key: while one command for `key` is in flight, newer ones replace the
 * waiting one instead of queueing behind it. A dragged slider therefore sends at most one
 * command at a time and always ends on the final value, never replaying the whole gesture.
 */
export function createLatestWins() {
  const lanes = new Map<string, { next: (() => Promise<unknown>) | null }>();
  return (key: string, enqueue: (work: () => Promise<unknown>) => void, work: () => Promise<unknown>) => {
    const lane = lanes.get(key);
    if (lane) { lane.next = work; return; }
    const fresh = { next: work as (() => Promise<unknown>) | null };
    lanes.set(key, fresh);
    const pump = () => {
      const job = fresh.next;
      if (!job) { lanes.delete(key); return; }
      fresh.next = null;
      enqueue(() => job().finally(pump));
    };
    pump();
  };
}
