import type { ClipSummary, ClipboardStatus } from '../ipc/client';
import { parseClip, parseClips, parseClipboardStatus } from '../views/app-adapters';
import { newer } from './topic';
type Transport = {
  list(query: string | null, limit: number): Promise<unknown>;
  status(): Promise<unknown>;
  added(cb: (value: ClipSummary) => void): Promise<() => void>;
  changed(cb: (value: ClipboardStatus) => void): Promise<() => void>;
};
export type ClipboardHistory = { items: ClipSummary[]; status: ClipboardStatus | null; loading: boolean; error: string | null; query: string };
/** Events invalidate the authoritative list (including evictions/search), never append
 * speculative rows. One request survives resets; one latest pending invalidation follows. */
export function createClipboardHistory(io: Transport) {
  let state: ClipboardHistory = { items: [], status: null, loading: false, error: null, query: '' };
  let generation = 0, queryGeneration = 0, statusVersion = 0, enabled = false, visible = false;
  let signature = '', dirty = true, flight = false, highWater: number | undefined;
  let statusFlight: Promise<void> | null = null, statusPending = false;
  let off: (() => void)[] = [], disposed = false;
  const listeners = new Set<() => void>();
  const publish = (patch: Partial<ClipboardHistory>) => { state = { ...state, ...patch }; for (const f of listeners) f(); };
  const current = (epoch: number) => !disposed && epoch === generation;
  const fail = (error: unknown) => publish({ error: error instanceof Error ? error.message : 'Clipboard request failed' });
  async function drain() {
    if (flight || !dirty || !enabled || !visible || disposed) return;
    flight = true; dirty = false;
    const epoch = generation, queryEpoch = queryGeneration, query = state.query;
    publish({ loading: true, error: null });
    try {
      const rows = parseClips(await io.list(query || null, 500));
      if (!current(epoch) || queryEpoch !== queryGeneration) return;
      const rev = rows[0]?.rev;
      // Empty lists have no wire revision; the invalidation generation still fences them.
      if (rev !== undefined && highWater !== undefined && rev !== highWater && !newer(rev, highWater)) return;
      if (rev !== undefined) highWater = rev;
      publish({ items: rows });
    } catch (error) { if (current(epoch) && queryEpoch === queryGeneration) fail(error); }
    finally { flight = false; publish({ loading: false }); void drain(); }
  }
  function invalidate() { queryGeneration++; dirty = true; void drain(); }
  function acceptStatus(raw: unknown, epoch: number) {
    if (!current(epoch)) return;
    try {
      const status = parseClipboardStatus(raw); statusVersion++;
      // A callback from before reconfiguration cannot restore enabled/pause state.
      if (status.enabled !== enabled) return;
      publish({ status }); invalidate();
    } catch (error) { fail(error); }
  }
  function refreshStatus(): Promise<void> {
    statusPending = true;
    if (statusFlight) return statusFlight;
    statusFlight = (async () => {
      while (statusPending && !disposed) {
        statusPending = false;
        const epoch = generation, version = statusVersion;
        try { const status = await io.status(); if (current(epoch) && version === statusVersion) acceptStatus(status, epoch); }
        catch (error) { if (current(epoch) && version === statusVersion) fail(error); }
      }
    })().finally(() => { statusFlight = null; });
    return statusFlight;
  }
  async function configure(nextEnabled: boolean, configKey: string) {
    if (signature === configKey || disposed) return;
    signature = configKey; enabled = nextEnabled;
    const epoch = ++generation; queryGeneration++; statusVersion++; highWater = undefined; dirty = true;
    off.forEach(f => f()); off = [];
    publish({ items: [], status: null, error: null });
    const installed: (() => void)[] = [];
    try {
      installed.push(await io.added(raw => {
        if (!current(epoch) || !enabled) return;
        try { const row = parseClip(raw); if (highWater !== undefined && !newer(row.rev, highWater)) return; highWater = row.rev; invalidate(); }
        catch (error) { fail(error); }
      }));
      if (!current(epoch)) { installed.forEach(f => f()); return; }
      // onClipboardStatus already guards the status topic's independent wrapping rev.
      installed.push(await io.changed(status => acceptStatus(status, epoch)));
      if (!current(epoch)) { installed.forEach(f => f()); return; }
      off = installed;
      await refreshStatus();
      if (current(epoch)) void drain();
    } catch (error) { installed.forEach(f => f()); if (current(epoch)) fail(error); }
  }
  return {
    configure, refreshStatus, invalidate,
    epoch: () => generation,
    isCurrent: current,
    setVisible(next: boolean) { if (visible === next) return; visible = next; queryGeneration++; dirty = true; void drain(); },
    setQuery(query: string) { if (new TextEncoder().encode(query).length > 4096 || query.includes('\0')) throw new TypeError('Invalid clipboard search'); if (state.query === query) return; publish({ query, items: [] }); invalidate(); },
    getSnapshot: () => state,
    subscribe(f: () => void) { listeners.add(f); return () => { listeners.delete(f); }; },
    dispose() { disposed = true; generation++; off.forEach(f => f()); off = []; listeners.clear(); },
  };
}
