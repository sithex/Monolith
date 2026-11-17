import { useSyncExternalStore } from 'react';

// One shared 1 s clock for the small labels that count (relative resets, session elapsed time).
// It runs only while something is subscribed, and nothing subscribes while the page is hidden.
const listeners = new Set<() => void>();
let timer: number | undefined;
function subscribeClock(listener: () => void) {
  listeners.add(listener);
  if (timer === undefined) timer = window.setInterval(() => { for (const fn of [...listeners]) fn(); }, 1000);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && timer !== undefined) { window.clearInterval(timer); timer = undefined; }
  };
}
const subscribeNothing = () => () => {};

/**
 * The text `format(now)` produces, kept live while `active`. The snapshot is the formatted string, so
 * React re-renders only the calling label, and only when its text changes (once a minute for the AI
 * page's labels), never the dashboard around it. While inactive the label is frozen at `now`, the
 * last time the data was read; reopening the page reads the live clock again.
 */
export function useRelativeText(active: boolean, now: number, format: (now: number) => string): string {
  return useSyncExternalStore(active ? subscribeClock : subscribeNothing, () => format(active ? Date.now() : now), () => format(now));
}
