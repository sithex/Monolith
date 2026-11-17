import { useEffect } from 'react';

/** After first paint the xterm chunk loads in a quiet moment; this is how long the island waits before asking. */
export const TERMINAL_IDLE_DELAY_MS = 1500;
const IDLE_CAP_MS = 2000;

/**
 * Warms the (cached) terminal chunk without blocking startup: at once on the first sign of intent (the island
 * leaves its closed state, or a terminal session exists), otherwise when the page is idle after first paint.
 * `load` is idempotent (a dynamic import), so repeats are free; it never creates the terminal or its PTY. A failed
 * warm-up is ignored here: the page that needs the chunk imports it again and reports its own error.
 */
export function useTerminalWarmup(load: () => unknown, mode: string, terminalStarted: boolean) {
  const intent = mode !== 'still' || terminalStarted;
  useEffect(() => {
    const warm = () => { void Promise.resolve().then(load).catch(() => {}); };
    if (intent) warm();
    if (intent) return;
    let idle = 0;
    const timer = window.setTimeout(() => {
      if (typeof window.requestIdleCallback === 'function') idle = window.requestIdleCallback(warm, { timeout: IDLE_CAP_MS });
      else warm();
    }, TERMINAL_IDLE_DELAY_MS);
    return () => { window.clearTimeout(timer); if (idle) window.cancelIdleCallback?.(idle); };
  }, [intent, load]);
}
