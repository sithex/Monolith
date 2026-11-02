// Pure revision guard. IPC payloads remain generated types supplied by the caller.
export const newer = (next: number, previous: number) => {
  const distance = (next - previous) >>> 0;
  return distance > 0 && distance < 0x80000000;
};
export function createTopic<T>() {
  let value: T;
  let revision: number | undefined;
  let bootstrapped = false;
  const listeners = new Set<() => void>();
  function apply(next: T, rev: number) {
    if (revision !== undefined && !newer(rev, revision)) return false;
    value = next; revision = rev;
    for (const listener of listeners) listener();
    return true;
  }
  return {
    apply,
    seed(next: T, rev: number) { apply(next, rev); bootstrapped = true; },
    getSnapshot(): T {
      if (!bootstrapped) throw new Error('Topic not bootstrapped');
      return value;
    },
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; },
  };
}

/** Await every snapshot before mount. Rejections never mount a partial/default renderer. */
export async function beforeFirstRender<T>(load: () => Promise<T>, seed: (snapshot: T) => void, render: () => void) {
  const snapshot = await load();
  seed(snapshot);
  render();
}
