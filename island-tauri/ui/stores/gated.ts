import { useCallback, useRef, useSyncExternalStore } from 'react';

/**
 * useSyncExternalStore that stops waking the component while `active` is false: store changes
 * are not forwarded and the last active snapshot is returned, so a hidden pane does not
 * re-render. When `active` turns true the render (already triggered by the prop change) reads
 * the live snapshot, so nothing stale survives a reopen.
 */
export function useGatedStore<T>(subscribe: (listener: () => void) => () => void, getSnapshot: () => T, active: boolean): T {
  const live = useRef(active), last = useRef<{ value: T } | null>(null);
  live.current = active;
  const forward = useCallback((listener: () => void) => subscribe(() => { if (live.current) listener(); }), [subscribe]);
  return useSyncExternalStore(forward, () => {
    if (live.current || last.current === null) last.current = { value: getSnapshot() };
    return last.current.value;
  });
}
