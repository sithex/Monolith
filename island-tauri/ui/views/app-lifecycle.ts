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
