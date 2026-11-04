/**
 * When the Hermes chat exits (e.g. /quit or Ctrl+D), start a fresh one in the same terminal.
 * A chat that keeps dying right after launch stops respawning instead of looping forever.
 */
export function createAutoRestart(options: {
  restart(): Promise<void>;
  notice(text: string): void;
  fail(error: unknown): void;
  now?: () => number;
  schedule?: (callback: () => void, ms: number) => unknown;
  cancel?: (handle: unknown) => void;
  delayMs?: number;
  quickMs?: number;
  maxQuick?: number;
}) {
  const now = options.now ?? (() => Date.now());
  const schedule = options.schedule ?? ((callback, ms) => setTimeout(callback, ms));
  const cancel = options.cancel ?? (handle => clearTimeout(handle as ReturnType<typeof setTimeout>));
  const delayMs = options.delayMs ?? 700, quickMs = options.quickMs ?? 10_000, maxQuick = options.maxQuick ?? 3;
  let startedAt = now(), quick = 0, pending: unknown = null, disposed = false;
  const trace: string[] = [];
  return {
    /** Test hooks only: what the respawner has done so far. */
    trace: () => trace.slice(),
    /** The current chat began (initial attach or a successful respawn). */
    started() { startedAt = now(); },
    exited() {
      trace.push(`exited disposed=${disposed} pending=${pending !== null}`);
      if (disposed || pending !== null) return;
      quick = now() - startedAt < quickMs ? quick + 1 : 0;
      if (quick >= maxQuick) {
        options.notice('Hermes keeps exiting right after starting, so it was not restarted again. Reopen Monolith to retry.');
        return;
      }
      options.notice('Starting a new Hermes chat…');
      pending = schedule(() => {
        pending = null;
        trace.push(`fire disposed=${disposed}`);
        if (disposed) return;
        options.restart().then(() => { startedAt = now(); trace.push('restarted'); },
          error => { trace.push(`failed ${String(error)}`); options.fail(error); });
      }, delayMs);
    },
    dispose() { trace.push('dispose'); disposed = true; if (pending !== null) cancel(pending); pending = null; },
  };
}
