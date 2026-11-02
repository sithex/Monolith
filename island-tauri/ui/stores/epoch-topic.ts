import { newer } from './topic';
// A service revision is meaningful only inside its lifetime/query generation.
export function createEpochTopic<T extends { rev: number }>() {
  let value: T | null = null, epoch = 0, revision: number | undefined;
  const listeners = new Set<() => void>();
  const publish = () => { for (const listener of listeners) listener(); };
  return {
    epoch: () => epoch,
    reset() { epoch++; revision = undefined; value = null; publish(); return epoch; },
    apply(next: T, generation = epoch) {
      if (generation !== epoch || (revision !== undefined && !newer(next.rev, revision))) return false;
      revision = next.rev; value = next; publish(); return true;
    },
    getSnapshot: () => value,
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; },
  };
}
